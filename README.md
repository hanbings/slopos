# SlopOS

SlopOS 是一个从零实现、以 Rust 为主要语言、面向 x86-64 UEFI/QEMU 的独立操作系统项目。

当前仓库已经有一个可重复启动的早期系统，而不是完成版操作系统：0BSD Rust UEFI 加载器会从 FAT ESP 读取并解析独立的 ELF64 内核，取得 ACPI RSDP 与 GOP，加载 bootstrap image 和一份独立 Rust PID 1 ELF 的引导校验副本，取得最终 memory map，调用 `ExitBootServices`，再把控制权交给 SlopOS 内核。内核接管串口、GOP framebuffer 与 PS/2 键鼠，挂载 ext4 root、从 `/sbin/slop-init` 和 `/sbin/slop-shell` 读取两个实际 ELF，并以独立 CR3 交错运行 CPL3 PID 1/2。

root image 现在包含两个独立 Rust `no_std` ELF：inode 23 的 `/sbin/slop-init`（26344 bytes）和 inode 24 的 `/sbin/slop-shell`（26560 bytes）。kernel 经 ext4 path walker 各跨七个文件块读入；PID 1 image 仍须与 BootInfo v2 保留的 ESP 引导副本逐字节相同，desktop service 则只来自 root VFS。两者各有独立 CR3、user code page、两页 user stack、Linux `argc/argv/envp/auxv`、保存的 syscall/interrupt frame、pending request 和容量 8 的 fd 表。容量 4 的 process table 实现 `Ready/Running/Blocked/Runnable/Exited`；除 `sched_yield` cooperative round-robin 外，100 Hz PIT 现在会在 CPL3 保存全部 15 个 GPR、RIP/RFLAGS/RSP 后抢占当前进程，并由 block-task continuation 选择另一个 Ready/Runnable CR3。QEMU 已实测 PID 1→2 与 PID 2→1 的非合作式切换，两个用户程序各用约 100,000,000 TSC tick 的无 syscall 窗口证明至少被抢占一次，并在其余交错执行中各自拥有数字相同但 ownership 独立的 fd 3。

PID 1 完成 17 次 syscall 与跨两页可逆 write/read，显式关闭最后一个 fd 后以 `wait4(-1)` 常驻为 supervisor；PID 2 分块读取完整 Waybar JSONC 与 swww environment并确认 EOF，再经有 magic/version/size/capability/config-hash 校验的 `slopos-desktop-v1` 私有提交 ABI 发布 CPU/memory provider 值与 Aurora 初始壁纸策略。desktop task 实际应用 policy 后回送 32-byte `policy-applied`，配置 bank 实际应用后另回送 `config-applied`。PID 2 收到后一事件便再次从 root VFS 读取 Waybar/swww、提交下一代 policy，然后继续阻塞等待下一代配置。初始 config generation 1 已推动 policy generation 2；交互 QEMU 又验证 `RELOAD` 的 config generation 2 推动 policy generation 3，而非法 reload 既不发布 config generation 3，也不唤醒出 policy generation 4。PID 1/2 的地址空间、frame、fd table 与 VFS backing array 因而由 block task 常驻持有；framebuffer、输入、niri 状态机与实际合成目前仍属于 kernel mechanism。

内核还会在启动时通过一个独立的 eBPF verifier 执行内建测试程序；当前只是无动态分配、前向控制流的安全子集，并不声称兼容 Linux eBPF。

QEMU 另挂载一个可重复生成的 256 MiB、双 block-group ext4 root disk。异步 mount/file API 核对复杂读取路径；读写 fd 3 除原位覆写外，还能从 EOF 4096 追加一整块：五 tag transaction 分配 block 117、把 inode 31 extent 增长到 8192，descriptor size/offset 同步推进，新增数据再经 fd 读回，最后 truncate/释放恢复。另一组 transaction 分配 inode 32、插入空文件 `/usr/share/slopos/create-probe`，以读写 fd 3 打开并验证 EOF，再 close/unlink。由于 timer preemption 与 desktop event wake 会合法改变进程/cache probe 的交错次序，当前 clean-boot/interaction 证据覆盖 157–163 hit、122–128 miss、固定 18 次 invalidation；测试核对设备请求恒等于队列中断加一，而不把某一种合法调度次序写死。两阶段 crash-injection 还会停在 allocation commit 后/home 前，再由普通 kernel 于下次 mount 重放、清理并继续进入桌面。当前 replay 支持最多八个 tag 的零-feature、连续且非 wrap transaction；这些 create/growth 操作仍是启动回归路径，没有通用可写 namespace mutation syscall。

![SlopOS scrolling-tile desktop](evidence/desktop.png)

早期桌面已开始沿 niri/Waybar/swww 方向重构，并已实际验证：

- niri 式横向 column strip，打开新列不改变既有列宽；
- 纵向 workspace 切换、named workspace、KDL `binds` 与顺序叠加的 `window-rule`；
- 50% 默认列宽、16 px gap、focus ring 与 edge scroll；
- Tab/`Mod+方向键` 切换焦点与 workspace、`Mod+Shift+左右` 重排列、`Mod+Shift+上下` 跨 workspace 移列、`Mod+Minus/Equal` 缩放列、`Mod+Q` 关闭窗口；
- 鼠标标题栏横拖滚动 viewport、关闭 tiled window；
- Waybar JSONC 驱动的 left/center/right 顶部 module 栏、module format/interval/length option；
- Waybar GTK CSS selector 的颜色、背景、padding/margin 与底边框子集；
- swww 式 daemon 状态、`img/query/kill` 命令、环境默认值与 CPU transition；
- root VFS `/sbin/slop-shell` 常驻用户态服务读取 Waybar/swww 配置，经版本化协议发布 bar provider 与 wallpaper policy，并在配置 generation 更新后重读；
- 两张可在运行时切换的嵌入式 P3/PNM 壁纸；
- niri KDL、Waybar JSONC/CSS 与 swww 配置/状态机子集，加桌面提交/事件协议，共 32 项宿主测试；
- root ext4 上按 XDG/系统/fallback 顺序发现四份桌面配置，parse-before-swap 后以双 bank generation 原子发布；
- `RELOAD` 与 Config surface 可触发运行时重读，非法配置保留上一代完整桌面状态；
- 键盘输入；
- 可执行 `HELP`、`STATUS`、`ABOUT`、`CLEAR` 和 `SWWW ...` 的图形 kernel monitor；
- 系统状态窗口；
- 配置 surface。

桌面现在已有跨运行时 reload 的双向用户态服务边界，但尚未整体迁出内核。`/sbin/slop-shell` 是 lifecycle-aware 的常驻 policy/provider 进程：它验证 root VFS 的 Waybar/swww 配置、等待每一代 `policy-applied` 与 `config-applied`，并在有效配置更新后重新读取和提交；PID 1 则常驻 `wait4` 作为 supervisor。kernel 仍持有配置文件发现/parse bank、niri 状态机、输入、swww daemon state、GOP renderer 与窗口 surface。系统内还没有普通配置编辑器或文件变更 watcher，Waybar 没有完整 GTK CSS、Pango、action 或硬件 backend；swww policy provider 也还不是 Unix-socket/Wayland layer-shell daemon，图片来源暂限两个编译时 PNM asset。完整兼容边界见 [docs/desktop-shell.md](docs/desktop-shell.md)，保守完成度见 [docs/status.md](docs/status.md)。

## 构建与运行

已验证环境：

- Debian trixie x86-64；
- Rust 1.88.0（由 `rust-toolchain.toml` 固定）；
- QEMU 10.0.11；
- OVMF 2025.02；
- `mtools`、`dosfstools`、`e2fsprogs`；
- 可选 `netpbm`，用于把 QEMU PPM 截图转换为 PNG。

安装 Rust target 后：

```bash
make image
make test-acpi
make test-ebpf
make test-elf
make test-process
make test-shell
make test-pci
make test-virtio
make test-ext4
make test-vfs
make test-boot
make test-interaction
make test-desktop-custom-config
make test-page-fault
make test-journal-replay
make run
```

`make test-acpi` 在宿主运行 RSDP/XSDT/MADT parser 的构造表测试，`make test-ebpf` 运行 verifier/interpreter 边界测试，`make test-elf` 的 10 项测试覆盖 ELF64 header、`PT_LOAD`、BSS、范围/对齐/重叠/W^X 与 entry validation，`make test-process` 的 6 项测试覆盖 Linux 初始栈、PID/capacity、`Blocked/Runnable` 生命周期、round-robin selection、child lookup/reap（含 immediate zombie）、每进程 fd isolation/seek 与退出清理，`make test-shell` 的 32 项测试覆盖 niri KDL workspace/bind/rule/column width/reorder、Waybar JSONC option/format 与 CSS、swww CLI/daemon/PNM/transition、滚动平铺状态机及桌面提交/事件协议，`make test-pci` 运行 PCI multifunction/capability 枚举测试，`make test-virtio` 检查 split-ring layout 及 read/write/flush descriptor chain，`make test-ext4` 的 28 项测试覆盖 superblock/group/inode/extent/directory/symlink、block/inode allocation、目录项 mutation、多 tag JBD2 records 和 recovery/state 更新，`make test-vfs` 的 5 项测试检查绝对路径、mount-prefix、fd offset/access mode、`close_all` 与 EOF growth。`make test-boot` 在 OVMF 中验证两个 root VFS ELF→独立 Linux initial stack/CR3→CPL3 cooperative yield 与双向 timer preemption、异步 `openat/read/write/close/wait4` suspend/completion、PID 1 supervisor 与 PID 2 service 常驻、同号 fd isolation、跨两页 user copy、policy/config 两类应用事件及连续 generation，以及 virtio 请求/INTx completion 的 `requests = interrupts + 1` 关系、桌面配置和全部存储探针。`make test-interaction` 注入真实 PS/2 命令，验证 config generation 2 唤醒 PID 2 并产生 policy generation 3、非法 CSS 回滚且不唤醒服务、swww Sunset 换图、center transition、query、kill/restart、niri viewport 横拖、`Mod+Equal/Minus` 相对缩放、`Mod+Shift+Right/Left` 列重排、`Mod+Q` 与 `Mod+Down`，并确认 window rule 把 Config 放入 named workspace；`make test-desktop-custom-config` 把不同长度的合法 Waybar 文件写入临时 rootfs，验证 PID 2 分块哈希、连续两代 policy 与桌面应用路径不依赖编译期默认 bytes；`make test-page-fault` 在两个用户进程保持 Blocked、kernel CR3 已恢复时核验 vector 14、RIP、error code 和 CR2；`make test-journal-replay` 对五 tag allocation transaction 生成 committed/未 checkpoint 的 dirty disk，再以普通 kernel 重启验证 mount-time replay、当前证据中的 545 次请求/544 次 completion、常驻桌面服务、桌面继续运行和宿主 fsck。

`make run` 打开 QEMU 图形窗口。桌面中可以直接输入命令；`RELOAD` 重读四份 VFS 配置，Tab 或 `Mod+左右` 沿 column strip 切换焦点，`Mod+上下` 切换 workspace，`Mod+Shift+左右` 在当前 strip 重排 focused column，`Mod+Shift+上下` 把它移入相邻 workspace，`Mod+Minus/Equal` 以 output 宽度的 10% 缩小/放大 focused column，`Mod+Q` 或红色 `X` 关闭 tiled window，横拖标题栏滚动 viewport。

## 设计边界

- SlopOS 原创源码均为 Rust，许可证为 0BSD。
- 当前只使用三个 MIT/Apache-2.0 Rust 依赖；见 [docs/dependencies.md](docs/dependencies.md)。
- UEFI 高层包装由 SlopOS 自己实现，仅使用宽松许可证的 `uefi-raw` 数据布局和函数表绑定。
- 内联/全局汇编只用于 x86 I/O port、interrupt entry、CR3/segment 操作、CPL3 transition、`cli`、`pause` 和 `hlt`；安全边界见 [docs/architecture.md](docs/architecture.md)。
- 不依赖 Linux、GRUB 或宿主桌面来运行 SlopOS 代码。

## 文档

- [架构和启动协议](docs/architecture.md)
- [niri/Waybar/swww 桌面兼容边界](docs/desktop-shell.md)
- [首个用户进程、process table 与 fast syscall](docs/processes.md)
- [逐子系统完成度](docs/status.md)
- [异步内核设计状态](docs/async-kernel.md)
- [ACPI 与 APIC 中断路径](docs/acpi-apic.md)
- [PCI 枚举边界](docs/pci.md)
- [virtio modern block 路径](docs/virtio.md)
- [ext4 root disk 与 parser 边界](docs/ext4.md)
- [VFS namespace 与文件描述符](docs/vfs.md)
- [eBPF 子集与验证边界](docs/ebpf.md)
- [依赖与许可证](docs/dependencies.md)
- [已知问题](docs/known-issues.md)
- [验证证据](evidence/README.md)
