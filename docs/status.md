# Verified subsystem status

状态日期：2026-07-30。状态词严格使用任务规格定义的级别。

| 规格范围 | 状态 | 当前证据与边界 |
|---|---|---|
| 原生图形桌面 | 部分实现 | QEMU 中直接进入三个窗口的交互桌面；键盘命令和鼠标拖动已自动验证。仍是内核态 early desktop，不是最终用户态桌面。 |
| Rust 实现语言 | 已实现并验证（当前代码范围） | loader、kernel、UI、输入和脚本所对应的 SlopOS 源码均为 Rust；仅有最小内联 x86 汇编。 |
| UEFI 引导 | 已实现并验证 | 独立 loader 加载 ELF kernel、ACPI、GOP、bootstrap image、memory map，调用 `ExitBootServices` 并跳入内核。 |
| 异步内核 | 部分实现 | 两任务 `Future` executor、task-ready bit queue、RawWaker、PIT timer future 和 IRQ-to-input-queue wakeup 已在 QEMU 运行；timer wheel、locks、cancellation、backpressure、I/O completion 和 SMP 尚未实现。 |
| 进程/线程/调度 | 尚未实现 | 当前只有一个内核执行流；没有用户态、地址空间或 preemption。 |
| 内存管理 | 部分实现 | kernel 解析 firmware descriptor stride，建立 frame allocator、自有四级页表并切换 CR3，建立 1 MiB kernel bump heap；frame/heap 读回与真实 vector-14 diagnostic 已验证。没有 user address space、细粒度页权限、COW 或 demand paging。 |
| ext4/btrfs 文件系统 | 尚未实现 | 启动文件由 firmware FAT 协议读取；`initrd.slp` 只是 bootstrap payload，绝不声明为 ext4/btrfs。 |
| 设备与驱动 | 部分实现 | GOP、COM1、QEMU debugcon、PS/2 键鼠可用；自有 GDT/IDT、8259 PIC、100 Hz PIT 与 PS/2 IRQ 上半部已验证。PCI/APIC/virtio/NVMe 未实现。 |
| 图形系统与 Wayland | 部分实现 | framebuffer renderer 和早期 window manager 可用；所有 Wayland wire/object/global/xdg 功能尚未实现。 |
| 声明式配置 | 部分实现 | UI 中可原子切换一个内存主题预览；没有 parser、types、schema、module、diff、持久化或 rollback。 |
| 文本编辑器 | 尚未实现 | kernel monitor 只编辑当前命令行，不是普通文本或配置文件编辑器。 |
| `slopd` | 尚未实现 | 没有用户态 init、unit、dependency graph 或 supervision。 |
| eBPF | 尚未实现 | 没有 instruction parser、VM、verifier、map 或 attach point。 |
| AMD SVM VMM | 尚未实现 | 尚未进行 SVM capability detection 或 `VMRUN`。 |
| Linux x86-64 ABI | 尚未实现 | 没有 Linux ELF 用户进程、syscall ABI、proxy 或 guest agent。 |
| 网络与 IPC | 尚未实现 | Ethernet/ARP/IP/TCP/UDP/DHCP/DNS 与 IPC 均未实现。 |
| 许可证 | 已实现并验证（当前代码范围） | 原创源码为 0BSD；锁定的三个第三方 crate 均为 MIT OR Apache-2.0。 |
| 可重复构建与证据 | 已实现并验证（当前里程碑） | 工具链、镜像脚本、QEMU 命令、串口日志、debugcon、截图及交互测试均已保留。 |

## 当前可运行状态

- 最后成功构建：2026-07-30，`make image`。
- 最后成功启动：2026-07-30，QEMU 10.0.11 + OVMF 2025.02，`make test-boot`。
- 最后成功交互测试：2026-07-30，`make test-interaction`。
- 最后成功异常测试：2026-07-30，`make test-page-fault`。
- 已验证的 kernel entry：`0x04000000`。
- 已验证 GOP mode：1024×768，stride 1024。
- 当前 bootstrap image：186 bytes，临时 SlopOS 文本格式。

## 下一项最高价值工作

下一阶段应把固定两任务 executor 扩展为可 spawn/cancel 的 task arena 与 timer wheel，并为 heap 增加回收。随后解析 ACPI MADT、启用 LAPIC/IOAPIC 替换兼容 PIC，并为 virtio completion 提供同一套 IRQ-to-waker 路径。
