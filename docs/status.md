# Verified subsystem status

状态日期：2026-07-30。状态词严格使用任务规格定义的级别。

| 规格范围 | 状态 | 当前证据与边界 |
|---|---|---|
| 原生图形桌面 | 部分实现 | QEMU 中三个 surface 已进入独立 workspace 的 niri 式 tiled strip + floating layer；浮窗始终盖在 tile 上且不随 viewport 滚动，KDL `Mod+V`/`Mod+Shift+V`、`open-floating true|false`、方向 move、跨层 focus、标题栏移动与 pointer resize 已实现。tiled 部分另有 normal/tabbed column、preset/explicit width+height、column+edge maximize、single/focused/visible center、available-width expand、固定/方向 consume+expel、focus/reorder、动态 workspace 与 rule。裸机 floating 证据为 Terminal x=16/y=56/488×696 tile → x=16/y=161/488×485 float → 跨层聚焦 System → float y=211 → x=520 tile；tabbed、edge maximize 与左右合并闭环也继续通过。PID 2 `/sbin/slop-shell` 仍作为常驻 provider/policy 服务跨 reload 休眠与恢复；compositor、surface 与 renderer 是 kernel early mechanism。 |
| Rust 实现语言 | 已实现并验证（当前代码范围） | loader、kernel、UI、输入和脚本所对应的 SlopOS 源码均为 Rust；x86 汇编限于 port/interrupt/CR3/CPL transition 边界。 |
| UEFI 引导 | 已实现并验证 | 独立 loader 加载 ELF kernel、Rust userspace ELF 校验副本、ACPI、GOP、bootstrap image、memory map，调用 `ExitBootServices` 并跳入内核；实际 PID 1 bytes 后续来自 ext4 root。 |
| 异步内核 | 部分实现 | 三任务 `Future` executor、task-ready bit queue、RawWaker、PIT timer、PS/2 input 与 virtio block INTx→waker→completion 已在 QEMU 运行；PID 1/2 的同步 `openat/read/write/close` 会保存各自 user frame、转为 Blocked 并返回 block task 异步等待 I/O，completion 再转为 Runnable、恢复对应 CR3/frame，而不是 busy-wait。100 Hz tick 还会在 CPL3 保存完整 interrupt frame并返回 block-task scheduler，kernel future 自身仍为 cooperative。动态 task arena、timer wheel、locks、cancellation、通用 backpressure 和 SMP 尚未实现。 |
| 进程/线程/调度 | 部分实现 | 两个独立 Rust ELF64 从 root `/sbin/slop-init` 与 `/sbin/slop-shell` 各跨七块读入，经严格 `PT_LOAD` parser 装入独立 CR3；init 另与 UEFI/BootInfo v2 副本比较。kernel 为 PID 1/2 各构造 `argc/argv/envp/auxv`、user code/two-page stack、pending syscall/frame 与容量 8 的 fd table。容量 4 的 process table 支持 Ready/Running/Blocked/Runnable/Exited；除 `sched_yield` round-robin 外，100 Hz PIT 已在两个无 syscall TSC 窗口验证双向 user preemption、全 GPR 保存和独立 CR3 恢复。PID 1 现常驻 `wait4`，PID 2 常驻 config event；block task 保留 runtime 并可在每次事件后继续驱动 VFS/syscall。process 宿主测试仍覆盖 exit、zombie/immediate reap 与 frame cleanup。仍无任意路径/multi-segment exec、thread/kernel preemption、通用 wait/kill/signal、orphan adoption 或 PID reuse。 |
| 内存管理 | 部分实现 | kernel 解析 firmware descriptor stride，建立带 bounded recycled-frame stack 的 frame allocator、自有四级页表并切换 CR3，建立 1 MiB kernel bump heap；PID 1/2 各使用 private PML4、supervisor kernel mapping、user read-only code/two-page writable stack，per-PID bounded copy 与跨两个独立 physical frame 的 user copy 已在 QEMU 验证。常驻服务保留两套地址空间，page-fault 回归证明 block task 恢复 kernel CR3 后仍能诊断未映射访问；process 状态机的 reap 路径与宿主测试仍覆盖每进程 7-frame 回收。没有 NX、kernel section W^X、COW、demand paging 或通用映射回收。 |
| ext4/btrfs 文件系统 | 部分实现 | QEMU 完成 VFS ELF 多块读取、fd 原位 write、五 tag block allocation/extent growth 和 inode 32/directory create→fd open→unlink transaction；两阶段重启验证 mount-time replay。mutation 仍是固定启动回归流程，btrfs 未实现。 |
| VFS 与文件描述符 | 部分实现 | `no_std` path/mount/fd crate 有 5 项宿主测试；QEMU 把 ext4 挂到 `/` 并取得两个实际 user image。PID 1/2 的独立 fd table 在交错执行中同时各自取得 fd 3 读取配置；PID 1 另以 `O_RDWR` + `lseek/write/read` 对 inode 31 完成跨两页的可逆 64-byte patch并显式 close。PID 2 每次有效 config generation 都重新 open/read/close Waybar 与 swww。kernel probe fd 另可 EOF 单块 append/truncate，并以读写模式打开刚创建的空文件。mount/backing object 仍是单一 block task 专用，用户 copy 限于已知 code/two-page stack mappings 与 256 bytes，尚非通用 POSIX VFS。 |
| 设备与驱动 | 部分实现 | GOP、COM1、QEMU debugcon、PS/2 键鼠可用，IntelliMouse ID 3/4 协商后可解码四字节滚轮包并回退标准三字节包；校验 XSDT/MADT，自有 GDT/IDT、xAPIC/IOAPIC、100 Hz PIT；PCI/modern virtio-blk 支持 read/write/flush。抢占产生的合法 cache 交错会改变 clean-boot 绝对请求数，测试核对 `requests = interrupts + 1` 且 top-half/queue interrupt 相等。没有通用 descriptor allocator、MSI-X、其他设备类或 application processor。 |
| 图形系统与 Wayland | 部分实现 | framebuffer renderer、niri 式 tiled/floating workspace/bind/rule/normal+tabbed stack/fixed+directional consume/expel/focus+reorder+explicit/preset resize+column/edge maximize+center/expand/direct move、Waybar JSONC option/format/受限三键/滚轮 action + CSS 顶栏及 workspace 点击、swww daemon state/CLI/PNM/clear/CPU transition 可用；35 项 shell + 5 项 protocol 测试及真实 floating/tabbed/跨 reload/action/样式/换图回归通过。仍无 Wayland wire/object/global/xdg、真实 Waybar hardware provider/POSIX shell action/完整 GTK CSS；PID 2 是 policy/provider，不是用户态 compositor、独立 bar surface 或 layer-shell client。 |
| 声明式配置 | 部分实现 | niri KDL、Waybar JSONC/CSS 与 swww environment 按 user/system/fallback 顺序从 root VFS 发现；双 static bank 会先验证四份文本，再按 generation 原子发布。交互测试已验证 config generation 1→2 唤醒 PID 2、policy generation 2→3，以及非法 CSS 保留 generation 2 且不唤醒服务；custom-config 回归验证 PID 2 对不同长度合法 Waybar override 的有界分块 hash、连续发布，以及 left/center module placement 和 clock 三键/双向滚轮 action 驱动渲染与壁纸切换。仍无文件 watcher/inotify、普通配置编辑器、结构化 diff，配置发现/parser 也仍属于 kernel service。 |
| 文本编辑器 | 尚未实现 | kernel monitor 只编辑当前命令行，不是普通文本或配置文件编辑器。 |
| `slopd` | 尚未实现 | 当前 `/sbin/slop-init` 只是固定 ABI probe，没有 unit、dependency graph、service manager 或 supervision。 |
| eBPF | 部分实现 | 独立 `no_std` crate 提供 8-byte instruction decode、无分配 verifier、ALU64/前向 branch/512-byte stack/helper 子集解释器；10 项宿主测试与内核返回 42 均已验证。没有 ELF loader、map、program type、attach point、权限模型或 JIT。 |
| AMD SVM VMM | 尚未实现 | 尚未进行 SVM capability detection 或 `VMRUN`。 |
| Linux x86-64 ABI | 部分实现 | root VFS 中两个独立 Rust ELF64 `ET_EXEC` CPL3 进程使用 Linux x86-64 initial stack/register/syscall number，真实解析各自 `argc/argv/envp/auxv` 并执行 `openat/read/write/lseek/close/sched_yield/wait4`；同步 fast return 使用 `SYSRETQ`，异步 I/O 与 timer saved frame 经 `IRETQ` 恢复完整 GPR。PID 1 当前停在 blocked wait4，PID 2 由 SlopOS 私有 commit/event ABI 跨 reload 恢复；exit/reap 与 immediate child lookup 由 6 项 process 宿主测试继续覆盖。10 项 ELF 测试和 QEMU 实测还覆盖 syscall/interrupt 两种 frame 的 CR3 switch、同号 fd isolation与跨两页 copy-user。尚无任意路径/多 segment exec、通用页表/demand-page copy、广泛 syscall surface、proxy 或 guest agent。 |
| 网络与 IPC | 尚未实现 | Ethernet/ARP/IP/TCP/UDP/DHCP/DNS 尚未实现；desktop commit→blocking wait→apply event 已证明一个固定双向 lifecycle channel，但不是通用 pipe/local socket/shared memory/event object/message queue。 |
| 许可证 | 已实现并验证（当前代码范围） | 原创源码为 0BSD；锁定的三个第三方 crate 均为 MIT OR Apache-2.0。 |
| 可重复构建与证据 | 已实现并验证（当前里程碑） | 工具链、镜像脚本、QEMU 命令、串口日志、debugcon、截图及交互测试均已保留。 |

## 当前可运行状态

- 最后成功构建：2026-07-30，`make image`。
- 最后成功启动：2026-07-30，QEMU 10.0.11 + OVMF 2025.02，`make test-boot`。
- 最后成功交互测试：2026-07-30，`make test-interaction`。
- 最后成功自定义桌面配置测试：2026-07-30，`make test-desktop-custom-config`。
- 最后成功异常测试：2026-07-30，`make test-page-fault`。
- 最后成功 journal recovery 测试：2026-07-30，`make test-journal-replay`。
- 最后成功 eBPF 单元测试：2026-07-30，`make test-ebpf`，10 项。
- 最后成功 ELF 单元测试：2026-07-30，`make test-elf`，10 项。
- 最后成功 process 单元测试：2026-07-30，`make test-process`，6 项。
- 最后成功 shell/protocol 单元测试：2026-07-30，`make test-shell`，40 项（shell 35 + desktop protocol 5）。
- 最后成功 ACPI 单元测试：2026-07-30，`make test-acpi`，3 项。
- 最后成功 PCI 单元测试：2026-07-30，`make test-pci`，3 项。
- 最后成功 virtio 单元测试：2026-07-30，`make test-virtio`，4 项。
- 最后成功 ext4 单元测试：2026-07-30，`make test-ext4`，28 项。
- 最后成功 VFS 单元测试：2026-07-30，`make test-vfs`，5 项。
- 已验证的 kernel entry：`0x04000000`。
- 已验证 GOP mode：1024×768，stride 1024。
- 当前 bootstrap image：186 bytes，临时 SlopOS 文本格式。

## 下一项最高价值工作

下一阶段以已验证的跨 runtime reload 常驻 PID 2 和双事件 lifecycle 为基础，建立通用 message queue/local socket 与共享 surface buffer，再逐步把 compositor/bar/wallpaper mechanism 移出 kernel；随后把有界动态 workspace 扩展到任意数量并补齐完整 action/rule/output/IPC，Waybar 真实 provider/Pango/action/完整 GTK CSS，以及 swww 任意 VFS image、PNG/JPEG/GIF、多 output 和真正的 layer-shell。通用进程主线仍需任意路径/多 `PT_LOAD` exec、通用 wait/signal、独立线程/kernel stack 与 SMP run queue。
