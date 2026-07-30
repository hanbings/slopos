# Verified subsystem status

状态日期：2026-07-30。状态词严格使用任务规格定义的级别。

| 规格范围 | 状态 | 当前证据与边界 |
|---|---|---|
| 原生图形桌面 | 部分实现 | QEMU 中三个 surface 已进入独立 workspace 的 niri 式 horizontal column strip；列宽稳定、edge scroll、bind focus/close、workspace switch、window rule、titlebar drag viewport 与顶部 bar 已自动验证。仍是内核态 early desktop，不是用户态 compositor。 |
| Rust 实现语言 | 已实现并验证（当前代码范围） | loader、kernel、UI、输入和脚本所对应的 SlopOS 源码均为 Rust；x86 汇编限于 port/interrupt/CR3/CPL transition 边界。 |
| UEFI 引导 | 已实现并验证 | 独立 loader 加载 ELF kernel、Rust userspace ELF 校验副本、ACPI、GOP、bootstrap image、memory map，调用 `ExitBootServices` 并跳入内核；实际 PID 1 bytes 后续来自 ext4 root。 |
| 异步内核 | 部分实现 | 三任务 `Future` executor、task-ready bit queue、RawWaker、PIT timer、PS/2 input 与 virtio block INTx→waker→completion 已在 QEMU 运行；PID 1 的同步 `openat/read/close` 会保存 user frame、返回 block task 异步等待 I/O，再恢复 CR3/frame，而不是 busy-wait。动态 task arena、timer wheel、locks、cancellation、通用 backpressure 和 SMP 尚未实现。 |
| 进程/线程/调度 | 部分实现 | 独立 `userspace/init` ELF64 从 root `/sbin/slop-init` 跨七块读入，并与 UEFI/BootInfo v2 校验副本完全比较后经严格 `PT_LOAD` parser 装入；kernel 构造 16-byte aligned `argc/argv/envp/auxv`，PID 1 进入 Rust 前实际核对。容量 4 的 process table 保存 PID/parent/image/initial RSP、Ready→Running→Exited、status/syscall count 与每 slot 容量 8 的 VFS fd table。PID 1 以独立 CR3、user code/stack page 和 CPL3 运行，可为异步文件 syscall 暂停/恢复，最终回到 kernel continuation。仍无任意路径/multi-segment exec、第二个实际 process、thread、scheduler、preemption、通用 context switch、wait/signal 或资源回收。 |
| 内存管理 | 部分实现 | kernel 解析 firmware descriptor stride，建立 frame allocator、自有四级页表并切换 CR3，建立 1 MiB kernel bump heap；PID 1 使用 private PML4、supervisor kernel mapping、user read-only code/user writable stack；frame/heap 读回与真实 vector-14 diagnostic 已验证。没有 NX、kernel section W^X、COW、demand paging 或回收。 |
| ext4/btrfs 文件系统 | 部分实现 | QEMU 完成 VFS ELF 多块读取、fd 原位 write、五 tag block allocation/extent growth 和 inode 32/directory create→fd open→unlink transaction；两阶段重启验证 mount-time replay。mutation 仍是固定启动回归流程，btrfs 未实现。 |
| VFS 与文件描述符 | 部分实现 | `no_std` path/mount/fd crate 有 5 项宿主测试；QEMU 把 ext4 挂到 `/`，从 `/sbin/slop-init` 取得实际 PID 1 image。PID 1 的独立 fd table 先以 `O_RDONLY openat/read/close` 异步读取配置，再以 `O_RDWR` + `lseek/write/read` 对 inode 31 完成可逆 16-byte patch；kernel probe fd 另可 EOF 单块 append/truncate，并以读写模式打开刚创建的空文件。mount/backing object 仍是单一 block task 专用，用户 copy 限于单页/256 bytes，尚非通用 POSIX VFS。 |
| 设备与驱动 | 部分实现 | GOP、COM1、QEMU debugcon、PS/2 键鼠可用；校验 XSDT/MADT，自有 GDT/IDT、xAPIC/IOAPIC、100 Hz PIT；PCI/modern virtio-blk 支持 read/write/flush，clean boot 的 489 个请求由 488 次 INTx 唤醒完成。没有通用 descriptor allocator、MSI-X、其他设备类或 application processor。 |
| 图形系统与 Wayland | 部分实现 | framebuffer renderer、niri 式滚动平铺/workspace/bind/rule、Waybar JSONC option/format + CSS 顶栏、swww 风格 daemon state/CLI/PNM/CPU transition 可用；25 项 shell 测试与真实 workspace/规则/样式/换图/query/kill/restart 通过。仍无 Wayland wire/object/global/xdg、真实 Waybar hardware provider/完整 GTK CSS，壁纸服务也不是独立 layer-shell client。 |
| 声明式配置 | 部分实现 | niri KDL、Waybar JSONC/CSS 与 swww environment 按 user/system/fallback 顺序从 root VFS 发现；双 static bank 会先验证四份文本，再按 generation 原子发布。交互测试已验证 generation 1→2 重载及非法 CSS 保留 generation 2。仍无文件 watcher/inotify、普通配置编辑器、结构化 diff 或跨进程配置服务。 |
| 文本编辑器 | 尚未实现 | kernel monitor 只编辑当前命令行，不是普通文本或配置文件编辑器。 |
| `slopd` | 尚未实现 | 没有用户态 init、unit、dependency graph 或 supervision。 |
| eBPF | 部分实现 | 独立 `no_std` crate 提供 8-byte instruction decode、无分配 verifier、ALU64/前向 branch/512-byte stack/helper 子集解释器；10 项宿主测试与内核返回 42 均已验证。没有 ELF loader、map、program type、attach point、权限模型或 JIT。 |
| AMD SVM VMM | 尚未实现 | 尚未进行 SVM capability detection 或 `VMRUN`。 |
| Linux x86-64 ABI | 部分实现 | root VFS 中的独立 Rust ELF64 `ET_EXEC` CPL3 PID 1 使用 Linux x86-64 initial stack/register/syscall number，真实解析 `argc/argv/envp/auxv` 并执行 `openat/read/write/lseek/close/exit`；文件 I/O 保存完整 user frame、经异步 ext4/virtio 完成后 `SYSRETQ` 恢复。10 项 ELF 与 6 项 process 测试覆盖 parser/initial stack/lifecycle/fd isolation。尚无任意路径/多 segment exec、跨页通用 copy-user、广泛 syscall surface、proxy 或 guest agent。 |
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
- 最后成功 process 单元测试：2026-07-30，`make test-process`，6 项。
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

下一阶段把当前单页 bounded 的用户 copy 扩展为跨页分块 `copy_from_user`/`copy_to_user`，补通用错误映射，并让 process exit 统一关闭 backing objects。随后把固定 `/sbin/slop-init`、强制等于 BootInfo 副本的单 `PT_LOAD` 装载器扩展为任意 VFS executable/参数输入，再进入第二个实际进程、可抢占 context switch、wait/资源回收。桌面兼容主线还需动态 workspace、完整 action/rule/output/IPC，Waybar 真实 provider/Pango/action/完整 GTK CSS，以及 swww 任意 VFS image、PNG/JPEG/GIF、多 output 和真正的用户态 IPC/layer-shell。
