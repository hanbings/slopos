# 首个用户进程与 syscall trap

当前内核在中断子系统初始化后、桌面启动前同步运行 PID 1 probe。它的目标是验证一条最小但真实的 x86-64 privilege boundary，而不是模拟用户态日志：

- 从 frame allocator 建立独立 PML4，并保留 supervisor-only kernel identity map；
- 在 `0x40000000` 映射一个 CPL3 可读、不可写的 code page；
- 在相邻页映射 CPL3 可读写 stack，初始 `RSP=0x40002000`；
- 通过 user code selector `0x23`、user data selector `0x1b` 和 `IRETQ` 从 CPL0 进入 CPL3；
- 通过 64-bit TSS 的 `RSP0` 在 trap 时切换到独立 16 KiB kernel privilege stack；
- 经 DPL3 interrupt gate `0x80` 保存 15 个通用寄存器并进入 Rust handler；
- 在 `exit(0)` 后恢复原 kernel CR3、kernel stack 与 callee-saved registers。

内嵌程序按 Linux x86-64 调用约定把 syscall number 放在 `RAX`，参数放在 `RDI/RSI/RDX`。它先调用编号 1 的 `write`，内核只接受 fd 1、精确的用户地址和 18-byte payload，返回 18；程序检查返回值后调用编号 60 的 `exit(0)`。handler 同时检查 CPU 保存的 `CS`/`SS` RPL 为 3，并以原子状态机约束调用顺序。未知编号返回 `-ENOSYS`。

当前 trap 入口是临时的 `int 0x80`，不是 Linux x86-64 的 `SYSCALL` instruction ABI。程序也是编译进 kernel 的一页机器码，不是从 ELF、VFS 或 initrd 装载。现阶段没有：

- ELF `PT_LOAD` parser、动态链接、`argv`/`envp` 或 auxiliary vector；
- `SYSCALL/SYSRET`、MSR 配置或统一的 `copy_from_user`；
- process table、scheduler、preemption、wait/kill/signal/TLS；
- 每进程 fd table、VFS syscall、权限/credential 或资源回收；
- NX stack、kernel section W^X、COW 或 demand paging。

页表和 privilege transition 遵循 Intel 64 架构定义；参考 [Intel 64 and IA-32 Architectures Software Developer Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)。`make test-boot`、`make test-interaction`、`make test-page-fault` 和两阶段 journal replay 都要求 PID 1 成功返回，借此覆盖新 GDT/TSS/page-table 路径与已有中断、异常和存储流程的组合。
