# 首个用户进程、process table 与 fast syscall

当前内核在中断子系统初始化、ext4 root mount 与 journal recovery 后，由 block task 同步运行 PID 1 probe。它的目标是验证一条最小但真实的 VFS ELF→process table→x86-64 privilege/syscall boundary，而不是模拟用户态日志：

- 以独立 `slopos-elf` crate 校验 little-endian x86-64 `ET_EXEC`、program-header geometry、`PT_LOAD` range/alignment/overlap/W^X 与 executable entry；
- 由 `userspace/init` 生成独立 4848-byte Rust ELF；rootfs builder 把它安装为 `/sbin/slop-init`，kernel 的 ext4 path walker 从 inode 23 跨两个逻辑块读取全部 bytes；
- UEFI 仍从 ESP `/slopos/init.elf` 读入 BootInfo v2 校验副本；kernel 要求 root VFS image 与该副本逐字节一致，差异会停止启动；
- 从 ELF file offset `0x1000` 复制 66-byte `PT_LOAD`，剩余 code page 保持为零；
- 把 image/CR3/entry/stack/user range 插入容量 4 的 `slopos-process` 表，按 `Ready → Running → Exited` 转换 PID 1，保留 exit status 与 syscall count；
- 每个 slot 自带独立、容量 8 的 `slopos-vfs::FileDescriptorTable`；4 项宿主测试覆盖 PID/parent/capacity、非法转换、exit/reap，以及两个进程各自取得 fd 3 且 offset 互不影响；
- 从 frame allocator 建立独立 PML4，并保留 supervisor-only kernel identity map；
- 在 `0x40000000` 映射一个 CPL3 可读、不可写的 code page；
- 在相邻页映射 CPL3 可读写 stack，初始 `RSP=0x40002000`；
- 通过 user code selector `0x23`、user data selector `0x1b` 和 `IRETQ` 从 CPL0 进入 CPL3；
- 先用 CPUID extended leaf 检查 SYSCALL/SYSRET，再配置并读回 `IA32_EFER.SCE`、`STAR`、`LSTAR` 与 `FMASK=0x47700`；
- 用户 ELF 发出真实 `SYSCALL`（opcode `0f 05`）；entry 在 IF/TF/DF/IOPL/NT/AC 被 mask 后保存 user `RSP`、`RCX` return RIP、`R11` flags 与 15 个通用寄存器，切到暂停中的 kernel continuation stack，再进入 Rust handler；
- `write` 更新 frame 的 `RAX` 后由 `SYSRETQ` 回到 user RIP/stack；`exit(0)` 把 process slot 标为 `Exited`，恢复原 kernel CR3、kernel stack 与 callee-saved registers。

独立 ELF 程序按 Linux x86-64 调用约定把 syscall number 放在 `RAX`，参数放在 `RDI/RSI/RDX`，并声明 `RCX`/`R11` 为 architectural clobber。它先调用编号 1 的 `write`；kernel 要求 fd 1 和 18-byte length，对 pointer+length 做 overflow check，并要求 payload 与 return RIP 位于 process table 中已验证的 `PT_LOAD` range、user RSP 位于 stack page，之后才读取并核对 payload，返回 18。程序检查返回值后调用编号 60 的 `exit(0)`。process table 记录两次 syscall 和 status 0；未知编号返回 `-ENOSYS`。

`STAR=0x10000800000000` 与当前 GDT 对应：SYSCALL 使用 kernel CS/SS `0x08/0x10`，64-bit SYSRET 生成 user CS/SS `0x23/0x1b`。`LSTAR` 指向 kernel ELF 内的 assembly entry；每次 QEMU 启动都核验 MSR readback。`FMASK` 确保 fast entry 在启用 Rust stack 前没有 IRQ/trace/direction-flag 窗口；返回前再清 user IOPL/NT/RF/VM 并恢复 reserved bit 与 IF。IDT 不再暴露 DPL3 vector `0x80`。

ELF 已与 kernel 分离，实际执行 bytes 来自 root VFS 的固定路径 `/sbin/slop-init`；ESP/BootInfo 副本目前仍是强制相等的启动信任锚，因此这不是任意路径的通用 `exec`。process table 与每进程 fd ownership 已存在，但当前同步 probe 仍只有一个实际 running process，fd 1 仍由 syscall handler 特判，尚未连接 root ext4 descriptor。现阶段没有：

- 任意路径/多 `PT_LOAD` page mapping、动态链接、`argv`/`envp` 或 auxiliary vector；
- 通用 `copy_from_user`/`copy_to_user`、VFS read/write/open/close syscall；
- scheduler、preemption、context switch、wait/kill/signal/TLS；
- PID reuse policy、credential、引用计数或物理 frame/page-table 回收；
- NX stack、kernel section W^X、COW 或 demand paging。

页表和 privilege transition 遵循 [Intel 64 and IA-32 Architectures Software Developer Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)，segment 规则依据 [System V ABI Program Header](https://refspecs.linuxfoundation.org/elf/gabi4%2B/ch5.pheader.html) 与 [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI)。`make test-process` 的 4 项测试覆盖 process/fd state；`make test-elf` 的 10 项测试覆盖 parser 边界；`make test-boot`、`make test-interaction`、`make test-page-fault` 和两阶段 journal replay 都要求 PID 1 成功返回，借此覆盖 ELF/GDT/MSR/page-table fast path 与已有中断、异常和存储流程的组合。
