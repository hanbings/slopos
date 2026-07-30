# 首个用户进程与 syscall trap

当前内核在中断子系统初始化后、桌面启动前同步运行 PID 1 probe。它的目标是验证一条最小但真实的 ELF→x86-64 privilege boundary，而不是模拟用户态日志：

- 以独立 `slopos-elf` crate 校验 little-endian x86-64 `ET_EXEC`、program-header geometry、`PT_LOAD` range/alignment/overlap/W^X 与 executable entry；
- 由 `userspace/init` 生成独立 4848-byte Rust ELF，由 UEFI 从 `/slopos/init.elf` 读入并经 BootInfo v2 交给 kernel；
- 从 ELF file offset `0x1000` 复制 66-byte `PT_LOAD`，剩余 code page 保持为零；
- 从 frame allocator 建立独立 PML4，并保留 supervisor-only kernel identity map；
- 在 `0x40000000` 映射一个 CPL3 可读、不可写的 code page；
- 在相邻页映射 CPL3 可读写 stack，初始 `RSP=0x40002000`；
- 通过 user code selector `0x23`、user data selector `0x1b` 和 `IRETQ` 从 CPL0 进入 CPL3；
- 通过 64-bit TSS 的 `RSP0` 在 trap 时切换到独立 16 KiB kernel privilege stack；
- 经 DPL3 interrupt gate `0x80` 保存 15 个通用寄存器并进入 Rust handler；
- 在 `exit(0)` 后恢复原 kernel CR3、kernel stack 与 callee-saved registers。

独立 ELF 程序按 Linux x86-64 调用约定把 syscall number 放在 `RAX`，参数放在 `RDI/RSI/RDX`。它先调用编号 1 的 `write`；kernel 要求 fd 1 和 18-byte length，对 pointer+length 做 overflow check 并要求整个 range 属于已验证 `PT_LOAD` memory，之后才读取并核对 payload，返回 18。程序检查返回值后调用编号 60 的 `exit(0)`。handler 同时检查 CPU 保存的 `CS`/`SS` RPL 为 3，并以原子状态机约束调用顺序。未知编号返回 `-ENOSYS`。

当前 trap 入口是临时的 `int 0x80`，不是 Linux x86-64 的 `SYSCALL` instruction ABI。ELF 已与 kernel 分离，但仍来自 FAT ESP/BootInfo，loader 当前只接受一个固定单页 R+X layout，尚未从 root VFS 按路径启动任意 executable。现阶段没有：

- 多 `PT_LOAD` page mapping、VFS `exec`、动态链接、`argv`/`envp` 或 auxiliary vector；
- `SYSCALL/SYSRET`、MSR 配置或统一的 `copy_from_user`；
- process table、scheduler、preemption、wait/kill/signal/TLS；
- 每进程 fd table、VFS syscall、权限/credential 或资源回收；
- NX stack、kernel section W^X、COW 或 demand paging。

页表和 privilege transition 遵循 [Intel 64 and IA-32 Architectures Software Developer Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)，segment 规则依据 [System V ABI Program Header](https://refspecs.linuxfoundation.org/elf/gabi4%2B/ch5.pheader.html) 与 [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI)。`make test-elf` 的 10 项宿主测试覆盖 parser 边界；`make test-boot`、`make test-interaction`、`make test-page-fault` 和两阶段 journal replay 都要求 PID 1 成功返回，借此覆盖 ELF/GDT/TSS/page-table 路径与已有中断、异常和存储流程的组合。
