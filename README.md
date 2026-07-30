# SlopOS

SlopOS 是一个从零实现、以 Rust 为主要语言、面向 x86-64 UEFI/QEMU 的独立操作系统项目。

当前仓库已经有一个可重复启动的早期系统，而不是完成版操作系统：0BSD Rust UEFI 加载器会从 FAT ESP 读取并解析独立的 ELF64 内核，取得 ACPI RSDP 与 GOP，加载 bootstrap image 和独立 Rust PID 1 ELF，取得最终 memory map，调用 `ExitBootServices`，再把控制权交给 SlopOS 内核。内核接管串口、GOP framebuffer 与 PS/2 键鼠，先运行一个有独立 CR3 的 CPL3 PID 1 probe，再进入早期交互桌面。

PID 1 由 `userspace/init` 独立构建为 `/slopos/init.elf`，UEFI loader 通过 BootInfo v2 传给 kernel。它使用独立 user code/stack page、GDT user segments、TSS `RSP0` privilege stack 与 DPL3 trap gate。`no_std` ELF crate 严格校验 ELF64/x86-64/`ET_EXEC` 和 `PT_LOAD`，按 `p_filesz` 复制 executable 并从 ELF entry 进入。程序按 Linux x86-64 的寄存器和编号约定发出 `write(1, ..., 18)` 与 `exit(0)`；kernel 对用户 pointer 先做已验证 `PT_LOAD` 范围检查，再核对 payload、返回值和调用顺序，最后恢复 kernel CR3/stack。trap 暂用 `int 0x80`；尚无 `SYSCALL/SYSRET`、调度或通用 syscall 层。

内核还会在启动时通过一个独立的 eBPF verifier 执行内建测试程序；当前只是无动态分配、前向控制流的安全子集，并不声称兼容 Linux eBPF。

QEMU 另挂载一个可重复生成的 256 MiB、双 block-group ext4 root disk。异步 mount/file API 核对复杂读取路径；读写 fd 3 除原位覆写外，还能从 EOF 4096 追加一整块：五 tag transaction 分配 block 99、把 inode 25 extent 增长到 8192，descriptor size/offset 同步推进，新增数据再经 fd 读回，最后 truncate/释放恢复。另一组 transaction 分配 inode 26、插入空文件 `/usr/share/slopos/create-probe`，以读写 fd 3 打开并验证 EOF，再 close/unlink。标准 clean boot 的 8-entry cache 记录 74 hit/69 miss/16 invalidation，共 447 个设备请求、446 次队列中断。两阶段 crash-injection 还会停在 allocation commit 后/home 前，再由普通 kernel 于下次 mount 重放、清理并继续进入桌面。当前 replay 支持最多八个 tag 的零-feature、连续且非 wrap transaction；这些 create/growth 操作仍是启动回归路径，没有通用可写 namespace 或 syscall。

![SlopOS scrolling-tile desktop](evidence/desktop.png)

早期桌面已开始沿 niri/Waybar/swww 方向重构，并已实际验证：

- niri 式横向 column strip，打开新列不改变既有列宽；
- 纵向 workspace 切换、named workspace、KDL `binds` 与顺序叠加的 `window-rule`；
- 50% 默认列宽、16 px gap、focus ring 与 edge scroll；
- Tab/`Mod+方向键` 切换焦点与 workspace、`Mod+Shift+上下` 移列、`Mod+Q` 关闭窗口；
- 鼠标标题栏横拖滚动 viewport、关闭 tiled window；
- Waybar JSONC 驱动的 left/center/right 顶部 module 栏、module format/interval/length option；
- Waybar GTK CSS selector 的颜色、背景、padding/margin 与底边框子集；
- swww 式 daemon 状态、`img/query/kill` 命令、环境默认值与 CPU transition；
- 两张可在运行时切换的嵌入式 P3/PNM 壁纸；
- niri KDL、Waybar JSONC/CSS 与 swww 配置/状态机子集，共 25 项宿主测试；
- 键盘输入；
- 可执行 `HELP`、`STATUS`、`ABOUT`、`CLEAR` 和 `SWWW ...` 的图形 kernel monitor；
- 系统状态窗口；
- 配置 surface。

这些桌面功能目前仍在内核态。Waybar module 顺序/栏高/间距/format 与 CSS 样式已经配置化，但 provider 仍是固定 kernel 数据，没有完整 GTK CSS、Pango、action 或硬件 backend；swww 状态机也还不是独立进程或 Wayland layer-shell client，图片来源暂限两个编译时 PNM asset。完整兼容边界见 [docs/desktop-shell.md](docs/desktop-shell.md)，保守完成度见 [docs/status.md](docs/status.md)。

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
make test-shell
make test-pci
make test-virtio
make test-ext4
make test-vfs
make test-boot
make test-interaction
make test-page-fault
make test-journal-replay
make run
```

`make test-acpi` 在宿主运行 RSDP/XSDT/MADT parser 的构造表测试，`make test-ebpf` 运行 verifier/interpreter 边界测试，`make test-elf` 的 10 项测试覆盖 ELF64 header、`PT_LOAD`、BSS、范围/对齐/重叠/W^X 与 entry validation，`make test-shell` 的 25 项测试覆盖 niri KDL workspace/bind/rule、Waybar JSONC option/format 与 CSS、swww CLI/daemon/PNM/transition 和滚动平铺状态机，`make test-pci` 运行 PCI multifunction/capability 枚举测试，`make test-virtio` 检查 split-ring layout 及 read/write/flush descriptor chain，`make test-ext4` 的 28 项测试覆盖 superblock/group/inode/extent/directory/symlink、block/inode allocation、目录项 mutation、多 tag JBD2 records 和 recovery/state 更新，`make test-vfs` 的 5 项测试检查绝对路径、mount-prefix、fd offset/access mode 与 EOF growth。`make test-boot` 在 OVMF 中验证 ELF→CPL3 enter/trap/exit、niri/Waybar/swww 配置、上述硬件路径、447 次 virtio 请求及 446 次 INTx completion、fd overwrite/append/truncate、active transaction、IRQ、async timer 和桌面循环。`make test-interaction` 注入真实 PS/2 命令，验证 swww Sunset 换图、center transition、query、kill/restart、niri viewport 横拖、`Mod+Q` 与 `Mod+Down`，并确认 window rule 把 Config 放入 named workspace；`make test-page-fault` 在用户进程退出并恢复 kernel CR3 后核验 vector 14、RIP、error code 和 CR2；`make test-journal-replay` 对五 tag allocation transaction 生成 committed/未 checkpoint 的 dirty disk，再以普通 kernel 重启验证 mount-time replay、477 次请求/476 次 completion、桌面继续运行和宿主 fsck。

`make run` 打开 QEMU 图形窗口。桌面中可以直接输入命令；Tab 或 `Mod+左右` 沿 column strip 切换焦点，`Mod+上下` 切换 workspace，`Mod+Shift+上下` 移动 focused column，`Mod+Q` 或红色 `X` 关闭 tiled window，横拖标题栏滚动 viewport。

## 设计边界

- SlopOS 原创源码均为 Rust，许可证为 0BSD。
- 当前只使用三个 MIT/Apache-2.0 Rust 依赖；见 [docs/dependencies.md](docs/dependencies.md)。
- UEFI 高层包装由 SlopOS 自己实现，仅使用宽松许可证的 `uefi-raw` 数据布局和函数表绑定。
- 内联/全局汇编只用于 x86 I/O port、interrupt entry、CR3/segment 操作、CPL3 transition、`cli`、`pause` 和 `hlt`；安全边界见 [docs/architecture.md](docs/architecture.md)。
- 不依赖 Linux、GRUB 或宿主桌面来运行 SlopOS 代码。

## 文档

- [架构和启动协议](docs/architecture.md)
- [niri/Waybar/swww 桌面兼容边界](docs/desktop-shell.md)
- [首个用户进程与 syscall trap](docs/processes.md)
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
