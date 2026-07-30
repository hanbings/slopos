# Verified subsystem status

状态日期：2026-07-30。状态词严格使用任务规格定义的级别。

| 规格范围 | 状态 | 当前证据与边界 |
|---|---|---|
| 原生图形桌面 | 部分实现 | QEMU 中三个 surface 已进入独立 workspace 的 niri 式 horizontal column strip；列宽稳定、edge scroll、bind focus/close、workspace switch、window rule、titlebar drag viewport 与顶部 bar 已自动验证。仍是内核态 early desktop，不是用户态 compositor。 |
| Rust 实现语言 | 已实现并验证（当前代码范围） | loader、kernel、UI、输入和脚本所对应的 SlopOS 源码均为 Rust；x86 汇编限于 port/interrupt/CR3/CPL transition 边界。 |
| UEFI 引导 | 已实现并验证 | 独立 loader 加载 ELF kernel、独立 Rust userspace ELF、ACPI、GOP、bootstrap image、memory map，调用 `ExitBootServices` 并跳入内核。 |
| 异步内核 | 部分实现 | 三任务 `Future` executor、task-ready bit queue、RawWaker、PIT timer、PS/2 input 与 virtio block INTx→waker→completion 已在 QEMU 运行；动态 task arena、timer wheel、locks、cancellation、通用 backpressure 和 SMP 尚未实现。 |
| 进程/线程/调度 | 部分实现 | 独立 `userspace/init` ELF64 经 UEFI/BootInfo v2 和严格 `PT_LOAD` parser 装入，以独立 CR3、user code/stack page、CPL3、TSS `RSP0` 运行，并经 trap 执行 write/exit 后恢复 kernel continuation。仍无 process table、thread、scheduler、preemption、wait、signal 或资源回收。 |
| 内存管理 | 部分实现 | kernel 解析 firmware descriptor stride，建立 frame allocator、自有四级页表并切换 CR3，建立 1 MiB kernel bump heap；PID 1 使用 private PML4、supervisor kernel mapping、user read-only code/user writable stack；frame/heap 读回与真实 vector-14 diagnostic 已验证。没有 NX、kernel section W^X、COW、demand paging 或回收。 |
| ext4/btrfs 文件系统 | 部分实现 | QEMU 完成 fd 原位 write、五 tag block allocation/extent growth 和 inode 26/directory create→fd open→unlink transaction；两阶段重启验证 mount-time replay。mutation 仍是固定启动回归流程，btrfs 未实现。 |
| VFS 与文件描述符 | 部分实现 | `no_std` path/mount/fd crate 有 5 项宿主测试；QEMU 把 ext4 挂到 `/`，fd 3 可读/seek/原位覆写、EOF 单块 append/truncate，并以读写模式打开刚创建的空文件。仍是 block task 局部、root-only 状态，不是可复用或每进程 POSIX VFS。 |
| 设备与驱动 | 部分实现 | GOP、COM1、QEMU debugcon、PS/2 键鼠可用；校验 XSDT/MADT，自有 GDT/IDT、xAPIC/IOAPIC、100 Hz PIT；PCI/modern virtio-blk 支持 read/write/flush，clean boot 的 447 个请求由 446 次 INTx 唤醒完成。没有通用 descriptor allocator、MSI-X、其他设备类或 application processor。 |
| 图形系统与 Wayland | 部分实现 | framebuffer renderer、niri 式滚动平铺/workspace/bind/rule、Waybar JSONC option/format + CSS 顶栏、swww 风格 daemon state/CLI/PNM/CPU transition 可用；25 项 shell 测试与真实 workspace/规则/样式/换图/query/kill/restart 通过。仍无 Wayland wire/object/global/xdg、真实 Waybar hardware provider/完整 GTK CSS，壁纸服务也不是独立 layer-shell client。 |
| 声明式配置 | 部分实现 | niri KDL 驱动 layout/workspace/bind/window-rule，Waybar JSONC 驱动 bar/module option/format，CSS 驱动 selector colors/box/border，`SWWW_TRANSITION*` 环境格式驱动壁纸默认值。仍是编译时 asset；没有 VFS/XDG live reload、diff/rollback。 |
| 文本编辑器 | 尚未实现 | kernel monitor 只编辑当前命令行，不是普通文本或配置文件编辑器。 |
| `slopd` | 尚未实现 | 没有用户态 init、unit、dependency graph 或 supervision。 |
| eBPF | 部分实现 | 独立 `no_std` crate 提供 8-byte instruction decode、无分配 verifier、ALU64/前向 branch/512-byte stack/helper 子集解释器；10 项宿主测试与内核返回 42 均已验证。没有 ELF loader、map、program type、attach point、权限模型或 JIT。 |
| AMD SVM VMM | 尚未实现 | 尚未进行 SVM capability detection 或 `VMRUN`。 |
| Linux x86-64 ABI | 部分实现 | 独立 Rust ELF64 `ET_EXEC` CPL3 PID 1 使用 Linux x86-64 的寄存器与 syscall 编号调用 write(1) 和 exit(60)；10 项 parser 测试覆盖 `PT_LOAD`、BSS 与拒绝边界。trap 暂为 `int 0x80`；尚无 VFS exec、多 segment、`SYSCALL/SYSRET`、通用 syscall surface、proxy 或 guest agent。 |
| 网络与 IPC | 尚未实现 | Ethernet/ARP/IP/TCP/UDP/DHCP/DNS 与 IPC 均未实现。 |
| 许可证 | 已实现并验证（当前代码范围） | 原创源码为 0BSD；锁定的三个第三方 crate 均为 MIT OR Apache-2.0。 |
| 可重复构建与证据 | 已实现并验证（当前里程碑） | 工具链、镜像脚本、QEMU 命令、串口日志、debugcon、截图及交互测试均已保留。 |

## 当前可运行状态

- 最后成功构建：2026-07-30，`make image`。
- 最后成功启动：2026-07-30，QEMU 10.0.11 + OVMF 2025.02，`make test-boot`。
- 最后成功交互测试：2026-07-30，`make test-interaction`。
- 最后成功异常测试：2026-07-30，`make test-page-fault`。
- 最后成功 journal recovery 测试：2026-07-30，`make test-journal-replay`。
- 最后成功 eBPF 单元测试：2026-07-30，`make test-ebpf`，10 项。
- 最后成功 ELF 单元测试：2026-07-30，`make test-elf`，10 项。
- 最后成功 shell 单元测试：2026-07-30，`make test-shell`，25 项。
- 最后成功 ACPI 单元测试：2026-07-30，`make test-acpi`，3 项。
- 最后成功 PCI 单元测试：2026-07-30，`make test-pci`，3 项。
- 最后成功 virtio 单元测试：2026-07-30，`make test-virtio`，4 项。
- 最后成功 ext4 单元测试：2026-07-30，`make test-ext4`，28 项。
- 最后成功 VFS 单元测试：2026-07-30，`make test-vfs`，5 项。
- 已验证的 kernel entry：`0x04000000`。
- 已验证 GOP mode：1024×768，stride 1024。
- 当前 bootstrap image：186 bytes，临时 SlopOS 文本格式。

## 下一项最高价值工作

下一阶段继续桌面兼容主线：把 niri、Waybar JSONC/CSS 与 swww 三套编译时配置改为 VFS/XDG load/reload，并实现 parse-before-swap rollback。niri 侧随后需动态 workspace、完整 action/rule/output/IPC；Waybar 侧需真实 provider/Pango/action/完整 GTK CSS；swww 侧需任意 VFS image、PNG/JPEG/GIF、多 output 和真正的用户态 IPC/layer-shell。进程主线仍需多 `PT_LOAD`/VFS exec、`SYSCALL/SYSRET`、process table 与可恢复的 per-process context，并把 VFS fd 接入用户 syscall。
