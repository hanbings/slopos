# SlopOS

SlopOS 是一个从零实现、以 Rust 为主要语言、面向 x86-64 UEFI/QEMU 的独立操作系统项目。

当前仓库已经有一个可重复启动的早期系统，而不是完成版操作系统：0BSD Rust UEFI 加载器会从 FAT ESP 读取并解析独立的 ELF64 内核，取得 ACPI RSDP 与 GOP，加载 bootstrap image 和一份独立 Rust PID 1 ELF 的引导校验副本，取得最终 memory map，调用 `ExitBootServices`，再把控制权交给 SlopOS 内核。内核接管串口、GOP framebuffer 与 PS/2 键鼠，挂载 ext4 root、从 `/sbin/slop-init` 和 `/sbin/slop-shell` 读取两个实际 ELF，并以独立 CR3 交错运行 CPL3 PID 1/2。

root image 现在包含两个独立 Rust `no_std` ELF：inode 23 的 `/sbin/slop-init`（26344 bytes）和 inode 24 的 `/sbin/slop-shell`（26560 bytes）。kernel 经 ext4 path walker 各跨七个文件块读入；PID 1 image 仍须与 BootInfo v2 保留的 ESP 引导副本逐字节相同，desktop service 则只来自 root VFS。两者各有独立 CR3、user code page、两页 user stack、Linux `argc/argv/envp/auxv`、保存的 syscall/interrupt frame、pending request 和容量 8 的 fd 表。容量 4 的 process table 实现 `Ready/Running/Blocked/Runnable/Exited`；除 `sched_yield` cooperative round-robin 外，100 Hz PIT 现在会在 CPL3 保存全部 15 个 GPR、RIP/RFLAGS/RSP 后抢占当前进程，并由 block-task continuation 选择另一个 Ready/Runnable CR3，再以 `IRETQ` 恢复异步中断上下文，避免把 RCX/R11 当成 syscall scratch。QEMU 已实测 PID 1→2 与 PID 2→1 的非合作式切换，两个用户程序各用约 100,000,000 TSC tick 的无 syscall 窗口证明至少被抢占一次，并在其余交错执行中各自拥有数字相同但 ownership 独立的 fd 3。

PID 1 完成 17 次 syscall 与跨两页可逆 write/read，显式关闭最后一个 fd 后以 `wait4(-1)` 常驻为 supervisor；PID 2 用 256-byte buffer 流式读取非空、最多 4096-byte 的 Waybar JSONC 与最多 512-byte 的 swww environment，确认 EOF并增量计算 FNV-1a，再经有 magic/version/size/capability/config-hash 校验的 `slopos-desktop-v1` 私有提交 ABI 发布 CPU/memory provider 值与 Aurora 初始壁纸策略。kernel 不再把摘要与编译期默认文件绑定；配置 bank 仍独立执行 UTF-8、JSONC/CSS/KDL/environment parse-before-swap。desktop task 实际应用 policy 后回送 32-byte `policy-applied`，配置 bank 实际应用后另回送 `config-applied`。PID 2 收到后一事件便再次从 root VFS 读取 Waybar/swww、提交下一代 policy，然后继续阻塞等待下一代配置。初始 config generation 1 已推动 policy generation 2；交互 QEMU 又验证 `RELOAD` 的 config generation 2 推动 policy generation 3，而非法 reload 既不发布 config generation 3，也不唤醒出 policy generation 4。PID 1/2 的地址空间、frame、fd table 与 VFS backing array 因而由 block task 常驻持有；framebuffer、输入、niri 状态机与实际合成目前仍属于 kernel mechanism。

内核还会在启动时通过一个独立的 eBPF verifier 执行内建测试程序；当前只是无动态分配、前向控制流的安全子集，并不声称兼容 Linux eBPF。

QEMU 另挂载一个可重复生成的 256 MiB、双 block-group ext4 root disk。异步 mount/file API 核对复杂读取路径；读写 fd 3 除原位覆写外，还能从 EOF 4096 追加一整块：五 tag transaction 分配 block 117、把 inode 31 extent 增长到 8192，descriptor size/offset 同步推进，新增数据再经 fd 读回，最后 truncate/释放恢复。另一组 transaction 分配 inode 32、插入空文件 `/usr/share/slopos/create-probe`，以读写 fd 3 打开并验证 EOF，再 close/unlink。由于 timer preemption 与 desktop event wake 会合法改变进程/cache probe 的交错次序，当前 clean-boot/interaction 证据覆盖 157–163 hit、119–128 miss、16–18 次 invalidation；测试核对设备请求恒等于队列中断加一，而不把某一种合法调度次序写死。两阶段 crash-injection 还会停在 allocation commit 后/home 前，再由普通 kernel 于下次 mount 重放、清理并继续进入桌面。当前 replay 支持最多八个 tag 的零-feature、连续且非 wrap transaction；这些 create/growth 操作仍是启动回归路径，没有通用可写 namespace mutation syscall。

![SlopOS scrolling-tile desktop](evidence/desktop.png)

早期桌面已开始沿 niri/Waybar/swww 方向重构，并已实际验证：

- niri 式横向 column strip，打开新列不改变既有列宽；也可把右侧窗口 consume 到 focused column 底部形成纵向 stack、调整单窗高度，再把底窗 expel 回右侧，或用方向感知的左右动作在相邻列合并 focused window/从 stack 拆出准确的 focused row；`Mod+W` 会在普通纵向 stack 与只显示当前窗、带侧边 tab 指示器的 tabbed column 间切换；
- 每个 workspace 另有始终位于 tiled strip 上方、不会随 viewport 滚动的 floating layer；`Mod+V` 在两层间切换当前窗，`Mod+Alt+V`/`Mod+Ctrl+V` 可显式移入 floating/tiling，`Mod+Shift+V` 切换层焦点，`Mod+Alt+G`/`Mod+Alt+T` 可显式聚焦 floating/tiling，标题栏拖动与 Super+右键缩放会直接移动/调整浮窗；
- 纵向 workspace 切换、named workspace、KDL `binds` 与顺序叠加的 `window-rule`；规则可分别覆盖 `open-on-workspace`、`open-floating true|false`、`open-focused true|false`、八向锚定的 `default-floating-position`、tiled/floating 共用的 `default-column-width`/`default-window-height { fixed|proportion; }`、动态 `default-column-display "normal"|"tabbed"`，以及一次性的 `open-maximized`、`open-maximized-to-edges` 与 `open-fullscreen` true/false 初态；
- named workspace 之后维持一个有界动态空 workspace，移入末尾时追加、移出后收缩；
- niri 官方 `Page_Up/Page_Down` 键名可分别聚焦 workspace，配合 Ctrl 移动整列、配合 Shift 把 workspace 的名称、平铺/浮动/fullscreen 状态和焦点整体向上或向下重排；
- 配置驱动、按 niri gap-aware 公式解析的 33.3%/50%/66.7% preset 列宽与窗高循环、50% 默认列宽、16 px gap、focus ring 与 edge scroll；
- Tab/`Mod+方向键`、`Mod+数字` 与 `Mod+Alt+C/M` 切换焦点/workspace，`Mod+Home/End` 直达首/末列，`Mod+Ctrl+Home/End` 把完整列重排到 strip 两端，`Mod+K/J` 在同列上下聚焦，`Mod+Ctrl+K/J` 在同列上下重排 focused window，`Mod+Comma/Period` 合并/拆出底窗，`Mod+[`/`]` 按左/右方向合并或拆出 focused window，`Mod+Shift+Minus/Equal` 调整 focused window 高度、`Mod+Ctrl+Shift+R` 循环 preset 窗高、`Mod+Ctrl+R` 重置等高，`Mod+R`/`Mod+Shift+R` 正反循环 preset 列宽，`Mod+F` 最大化/恢复列，`Mod+Shift+F` 令 tiled/floating focused window 独占完整 output 并可恢复，`Mod+M` 将 focused window 无 gap 最大化到工作区边缘，`Mod+C` 居中当前列，`Mod+Ctrl+C` 居中完整可见列集合，`Mod+Ctrl+F` 吸收完整可见列未占用的余宽，`Mod+Tab` 往返上一 workspace，`Mod+Shift+左右` 重排列；`Mod+Shift+上下`、`Mod+Ctrl+数字` 或 `Mod+Ctrl+Alt+C/M` 跨 workspace 移动完整列，`Mod+Alt+上下`、`Mod+Ctrl+Shift+2` 或 `Mod+Shift+Alt+M` 则只移动 focused window，`Mod+Minus/Equal` 缩放列、`Mod+Q` 关闭窗口；
- 鼠标标题栏横拖滚动 viewport、Super+右键横拖缩放 focused column、关闭 tiled window；
- Waybar JSONC 驱动的 left/center/right 顶部 module 栏、module format/alternate-format/interval/length、可配置 alternate-format click、左/右/中键与上下滚轮 action option，以及可点击的 `niri/workspaces` 数字；
- Waybar GTK CSS selector 的颜色、背景、padding/margin 与底边框子集；
- swww 式 daemon 状态、`img/clear/query/kill` 命令、环境默认值与 CPU transition；
- root VFS `/sbin/slop-shell` 常驻用户态服务读取 Waybar/swww 配置，经版本化协议发布 bar provider 与 wallpaper policy，并在配置 generation 更新后重读；
- 非默认但可解析的 niri KDL 与 Waybar JSONC 能从临时 ext4 副本注入；实机回归确认 `open-focused true` 会让规则放到后台 workspace 的 Config 主动获得焦点并以覆盖 Waybar 的 1024×768 fullscreen 打开，退出后依次恢复保留 Waybar 的 1024×728 edge maximize、992×340 column maximize 与 app rule 的 66.7%/656×340 初始尺寸；`default-floating-position x=24 y=24 relative-to="bottom-right"` 在移入浮动层时产生精确右下间距，移动后的浮动位置经过 tiling 往返仍会恢复。切回 main 后旧 Terminal 焦点也保持不变；
- 两张可在运行时切换的嵌入式 P3/PNM 壁纸；
- niri KDL、Waybar JSONC/CSS 与 swww 配置/状态机子集（含全局 Mod+IntelliMouse 滚轮绑定、app-specific 初始宽高/焦点/浮动锚点、两类最大化与全屏规则），加桌面提交/事件协议，共 49 项宿主测试；
- root ext4 上按 XDG/系统/fallback 顺序发现四份桌面配置，parse-before-swap 后以双 bank generation 原子发布；
- `RELOAD` 与 Config surface 可触发运行时重读，非法配置保留上一代完整桌面状态；
- 键盘输入；
- 可执行 `HELP`、`STATUS`、`ABOUT`、`CLEAR` 和 `SWWW ...` 的图形 kernel monitor；
- 系统状态窗口；
- 配置 surface。

桌面现在已有跨运行时 reload 的双向用户态服务边界，但尚未整体迁出内核。`/sbin/slop-shell` 是 lifecycle-aware 的常驻 policy/provider 进程：它验证 root VFS 的 Waybar/swww 配置、等待每一代 `policy-applied` 与 `config-applied`，并在有效配置更新后重新读取和提交；PID 1 则常驻 `wait4` 作为 supervisor。kernel 仍持有配置文件发现/parse bank、niri 状态机、输入、swww daemon state、GOP renderer 与窗口 surface。系统内还没有普通配置编辑器或文件变更 watcher，Waybar 没有完整 GTK CSS、Pango、action 或硬件 backend；swww `img <path>` 已能经异步 block task 从 root VFS 加载最多 8 KiB 的 P3/PNM，但 policy provider 还不是 Unix-socket/Wayland layer-shell daemon，也不支持压缩图片格式。完整兼容边界见 [docs/desktop-shell.md](docs/desktop-shell.md)，保守完成度见 [docs/status.md](docs/status.md)。

## 构建与运行

已验证环境：

- Debian trixie x86-64；
- Rust 1.88.0（由 `rust-toolchain.toml` 固定）；
- QEMU 10.0.11；
- OVMF 2025.02；
- `mtools`、`dosfstools`、`e2fsprogs`、`socat`（交互回归的 QMP 输入 socket）；
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

`make test-acpi` 在宿主运行 RSDP/XSDT/MADT parser 的构造表测试，`make test-ebpf` 运行 verifier/interpreter 边界测试，`make test-elf` 的 10 项测试覆盖 ELF64 header、`PT_LOAD`、BSS、范围/对齐/重叠/W^X 与 entry validation，`make test-process` 的 6 项测试覆盖 Linux 初始栈、PID/capacity、`Blocked/Runnable` 生命周期、round-robin selection、child lookup/reap（含 immediate zombie）、每进程 fd isolation/seek 与退出清理，`make test-shell` 的 49 项测试覆盖 niri KDL workspace/keyboard+wheel bind/ordered `default-column-width`/`default-window-height`/`default-column-display`/`default-floating-position`/`open-maximized`/`open-maximized-to-edges`/`open-fullscreen`/`open-focused` rule/floating layer/fullscreen/workspace reorder/column width/window height/preset/tabbed column/directional consume-or-expel/column+window workspace transfer/strip-boundary focus+reorder/column+edge maximize/focused+visible center/expand、Waybar JSONC option/format 与 CSS、swww CLI/daemon/PNM/transition、滚动平铺状态机及桌面提交/事件协议，`make test-pci` 运行 PCI multifunction/capability 枚举测试，`make test-virtio` 检查 split-ring layout 及 read/write/flush descriptor chain，`make test-ext4` 的 28 项测试覆盖 superblock/group/inode/extent/directory/symlink、block/inode allocation、目录项 mutation、多 tag JBD2 records 和 recovery/state 更新，`make test-vfs` 的 5 项测试检查绝对路径、mount-prefix、fd offset/access mode、`close_all` 与 EOF growth。`make test-boot` 在 OVMF 中验证两个 root VFS ELF→独立 Linux initial stack/CR3→CPL3 cooperative yield 与双向 timer preemption、异步 `openat/read/write/close/wait4` suspend/completion、PID 1 supervisor 与 PID 2 service 常驻、同号 fd isolation、跨两页 user copy、policy/config 两类应用事件及连续 generation，以及 virtio 请求/INTx completion 的 `requests = interrupts + 1` 关系、桌面配置和全部存储探针。`make test-interaction` 注入真实 PS/2 命令，验证 config generation 2 唤醒 PID 2并产生 policy generation 3、非法 CSS 回滚、swww Sunset 换图、root VFS 双块 P3 异步加载与 missing-path 保留旧图、纯色 `clear`/`query`、kill/restart、单窗/整列跨 workspace 的差异、Home/End 首末列导航/重排、PageUp/PageDown workspace 整体重排与 named 引用、四组 Mod+IntelliMouse workspace/column focus+move+reorder、tiled/floating 全屏与原层恢复、覆盖状态下拒绝 Waybar 点击、显式 floating/tiling move 与 focus target，以及 `Mod+V` 的 tiling→floating→跨层焦点→浮窗下移→tiling 闭环和完整 niri resize/center/expand/maximize/tabbed/consume/workspace/pointer 回归；`make test-desktop-custom-config` 同时注入非默认 niri/Waybar，验证 `open-focused true` 把 Config 激活到目标 workspace、`open-fullscreen true` 的 1024×768 初态、退出后恢复 `open-maximized-to-edges true` 的 1024×728、再恢复 `open-maximized true` 的 992×340 与规则宽高 656×340，同时保留 main 的旧焦点，并显示 `default-column-display "tabbed"` 单 tab 指示条、右下浮动锚点与移动后位置记忆、非默认 Waybar placement、`format-alt` 默认左击往返、五类 module action、IntelliMouse packet、壁纸双向切换与输入保持；`make test-page-fault` 核验 vector 14/CR2；`make test-journal-replay` 验证五 tag crash/replay、桌面继续运行和宿主 fsck。

`make run` 打开 QEMU 图形窗口。桌面中可以直接输入命令；`RELOAD` 重读四份 VFS 配置，点击顶部 workspace 数字或按 `Mod+上下`/`Mod+PageUp/PageDown`/`Mod+滚轮` 切换 workspace，`Mod+Ctrl+PageUp/PageDown` 或 `Mod+Ctrl+滚轮` 移动整列，`Mod+Shift+PageUp/PageDown` 整体重排当前 workspace；`Mod+Shift+滚轮` 沿 column strip 聚焦，`Mod+Ctrl+Shift+滚轮` 重排列。Tab 或 `Mod+左右` 也可切换列焦点。`Mod+V` 在 tiling/floating layer 间切换当前窗，`Mod+Alt+V`/`Mod+Ctrl+V` 分别显式移入 floating/tiling，`Mod+Shift+V` 在两层间切焦点，`Mod+Alt+G`/`Mod+Alt+T` 分别显式聚焦 floating/tiling；浮窗始终盖在 tile 上，方向 move bind、标题栏拖动与 Super+右键缩放作用于浮窗而不滚动 strip。`Mod+Shift+F` 将任一层的 focused window 无装饰铺满 1024×768，覆盖 Waybar 和兄弟窗，再按一次回到原 layer 与精确几何。`Mod+Comma` 把右侧顶窗并入当前列底部、`Mod+W` 在纵向 stack 与单窗 tabbed display 间切换、`Mod+K/J` 在同列上下聚焦（tabbed 时切换可见 tab）、`Mod+Ctrl+K/J` 上下重排当前窗、`Mod+Shift+Minus/Equal` 以 gap-aware output 高度的 10% 调整当前窗并补偿同列其他窗、`Mod+Ctrl+Shift+R` 循环 KDL preset 窗高、`Mod+Ctrl+R` 恢复等高，`Mod+Period` 把当前列底窗拆回右侧；`Mod+[`/`]` 在 singleton column 时把 focused window 合并进左/右邻列，在 stack 时把准确的 focused row 拆成左/右 singleton column，焦点始终跟随该窗。`Mod+R`/`Mod+Shift+R` 正反循环 KDL preset 列宽，`Mod+F` 在保留普通列宽的同时最大化/恢复 focused column，`Mod+M` 把 focused window 无 gap 最大化到 Waybar 下方工作区边缘，`Mod+C` 把 focused column 精确居中，`Mod+Ctrl+C` 把包含它的完整可见列集合整体居中，`Mod+Ctrl+F` 把它扩到其他完整可见列未占用的剩余宽度，`Mod+Shift+左右` 在当前 strip 重排 focused column，`Mod+Shift+上下` 把它移入相邻 workspace，`Mod+Minus/Equal` 以 gap-aware output 宽度的 10% 缩小/放大 focused column，`Mod+Q` 或红色 `X` 关闭当前窗；横拖 tiled 标题栏滚动 viewport，横拖 floating 标题栏则移动浮窗。

跨 workspace 时，`move-column-to-workspace*` 会保留整列的 stack、focused row、列宽和 display state；`move-window-to-workspace*` 只抽出 focused window，并以原列宽在目标建立 singleton column。workspace 自身重排时，名称身份与所有 layer/fullscreen/focus 状态一起移动，因此后续按名称引用仍指向同一逻辑 workspace。默认配置用 `Mod+Alt+上下`、`Mod+Ctrl+Shift+2` 与 `Mod+Shift+Alt+M` 演示单窗形式；Shift+数字产生的符号会按物理数字键规范化后匹配。

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
