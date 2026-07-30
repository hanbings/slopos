# 首个用户进程、process table 与异步 VFS syscall

当前内核在中断子系统初始化、ext4 root mount 与 journal recovery 后，由 block task 驱动 PID 1。它的目标是验证一条最小但真实的 VFS ELF→process table→x86-64 privilege/syscall→async block completion boundary，而不是模拟用户态日志：

- 以独立 `slopos-elf` crate 校验 little-endian x86-64 `ET_EXEC`、program-header geometry、`PT_LOAD` range/alignment/overlap/W^X 与 executable entry；
- 由 `userspace/init` 生成独立 25896-byte Rust ELF；rootfs builder 把它安装为 `/sbin/slop-init`，kernel 的 ext4 path walker 从 inode 23 跨七个逻辑块读取全部 bytes；
- UEFI 仍从 ESP `/slopos/init.elf` 读入 BootInfo v2 校验副本；kernel 要求 root VFS image 与该副本逐字节一致，差异会停止启动；
- 从 ELF file offset `0x1000` 复制 2160-byte `PT_LOAD`，剩余 code page 保持为零；
- 把 image/CR3/entry/stack/user range 插入容量 4 的 `slopos-process` 表，按 `Ready → Running → Exited` 转换 PID 1，保留 exit status 与 syscall count；
- 每个 slot 自带独立、容量 8 的 `slopos-vfs::FileDescriptorTable`；6 项宿主测试覆盖 Linux 初始栈、PID/parent/capacity、非法转换、exit/reap，以及两个进程各自取得 fd 3、独立 seek 且 offset 互不影响；
- 从 frame allocator 建立独立 PML4，并保留 supervisor-only kernel identity map；
- 在 `0x40000000` 映射一个 CPL3 可读、不可写的 code page；
- 在相邻页映射 CPL3 可读写 stack；kernel 从页顶向下复制 `argv`/`envp` strings，再编码 `argc=2`、pointer vectors 和 9 对 Linux auxv，最终 `RSP=0x40001ec0` 且 16-byte aligned；
- 用户入口 assembly 保留原始 `RSP` 并传给 Rust；PID 1 在任何 syscall 前核对 `argv[0]`/`argv[1]`、3 项 environment、`AT_PAGESZ`/`AT_ENTRY`/uid/gid/secure/`AT_EXECFN`/`AT_NULL` 与所有 NUL boundary；
- 通过 user code selector `0x23`、user data selector `0x1b` 和 `IRETQ` 从 CPL0 进入 CPL3；
- 先用 CPUID extended leaf 检查 SYSCALL/SYSRET，再配置并读回 `IA32_EFER.SCE`、`STAR`、`LSTAR` 与 `FMASK=0x47700`；
- 用户 ELF 发出真实 `SYSCALL`（opcode `0f 05`）；entry 在 IF/TF/DF/IOPL/NT/AC 被 mask 后保存 user `RSP`、`RCX` return RIP、`R11` flags 与 15 个通用寄存器，切到暂停中的 kernel continuation stack，再进入 Rust handler；
- stdout `write` 更新 frame 的 `RAX` 后直接由 `SYSRETQ` 回到 user RIP/stack；
- `openat`/regular-file `read`/`write`/`close` 把 frame 复制到单进程 pending slot，恢复 kernel CR3/continuation，让 block task 解析 VFS path 或等待 ext4/virtio Future；完成后把 return value 与 read bytes 写回保存的 frame/user stack，重新切换 process CR3，以 `SYSRETQ` 恢复同一 RIP、RSP 和通用寄存器；
- `lseek(fd, offset, SEEK_SET)` 只修改该进程 descriptor offset，不需要设备 I/O，因此在 fast handler 内直接返回；
- `exit(0)` 把 process slot 标为 `Exited`，恢复原 kernel CR3、kernel stack 与 callee-saved registers。

独立 ELF 程序按 Linux x86-64 调用约定把 syscall number 放在 `RAX`，前三个参数放在 `RDI/RSI/RDX`，`openat` 的第四个参数放在 `R10`，并声明 `RCX`/`R11` 为 architectural clobber。它依次执行：

1. `openat(AT_FDCWD, "/etc/slopos/system.conf", O_RDONLY, 0)`，root namespace/path walker 打开 inode 18，进程自己的 descriptor table 返回 fd 3；
2. `read(3, stack_buffer, 76)`；用户 frame 暂停，block task 经 ext4 cache/virtio 异步读取，完成后用 kernel identity map 对应的 physical stack frame 执行有界 `copy_to_user`，推进该进程 fd offset，再恢复用户上下文；
3. 用户态把 76 bytes 与编译时预期内容逐字节比较，随后 `close(3)`；
4. `openat(AT_FDCWD, "/usr/share/slopos/write-probe.bin", O_RDWR, 0)` 复用 fd 3；四次 `lseek(3, 123, SEEK_SET)` 分别定位 patch/verify/restore/verify；
5. 两次 `write(3, ..., 16)` 经 block task 对 inode 31 执行 read-modify-write、virtio write/flush 与 cache invalidation；两次 `read(3, ..., 16)` 分别验证全 `0xa5` patch 和恢复后的全 `P` 内容；
6. `close(3)` 后，`write(1, message, 18)` 经 stdout 特判直接返回 18，`exit(0)` 返回 kernel continuation。

process table 最终记录 15 次 syscall 和 status 0。路径 copy 最多 128 bytes，单次 read/write 最多 256 bytes；read destination 必须完全落在单页 user stack，path/write input 必须落在单页 code 或 stack mapping。kernel 通过保存的 physical code/stack frame 在 kernel CR3 下复制，不会把任意 user virtual pointer 直接解引用。未知编号返回 `-ENOSYS`，已连接调用对坏 fd、pointer、flag/path/offset 等返回当前子集的负 errno。

`STAR=0x10000800000000` 与当前 GDT 对应：SYSCALL 使用 kernel CS/SS `0x08/0x10`，64-bit SYSRET 生成 user CS/SS `0x23/0x1b`。`LSTAR` 指向 kernel ELF 内的 assembly entry；每次 QEMU 启动都核验 MSR readback。`FMASK` 确保 fast entry 在启用 Rust stack 前没有 IRQ/trace/direction-flag 窗口；返回前再清 user IOPL/NT/RF/VM 并恢复 reserved bit 与 IF。IDT 不再暴露 DPL3 vector `0x80`。

ELF 已与 kernel 分离，实际执行 bytes 来自 root VFS 的固定路径 `/sbin/slop-init`；ESP/BootInfo 副本目前仍是强制相等的启动信任锚，因此这不是任意路径的通用 `exec`。每进程 descriptor ownership 已实际连接 root ext4 的 `O_RDONLY`/`O_RDWR openat`、read/write/lseek/close，但 PID 1 仍嵌在单一 block task 的专用 suspend/resume loop 中，系统也仍只有一个实际 running process；fd 1 继续由 syscall handler 特判。现阶段没有：

- 任意路径/多 `PT_LOAD` page mapping、动态链接，或从通用 exec 参数构建初始栈；
- 跨页/多 mapping 的通用 `copy_from_user`/`copy_to_user`，或 grow/truncate/stat/directory/dup/poll/mmap 等通用 VFS syscall；
- scheduler、preemption、context switch、wait/kill/signal/TLS；
- PID reuse policy、credential、引用计数或物理 frame/page-table 回收；
- NX stack、kernel section W^X、COW 或 demand paging。

页表和 privilege transition 遵循 [Intel 64 and IA-32 Architectures Software Developer Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)，segment 与初始栈规则依据 [System V ABI Program Header](https://refspecs.linuxfoundation.org/elf/gabi4%2B/ch5.pheader.html) 与 [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI)。`make test-process` 的 6 项测试覆盖 initial-stack/process/fd state；`make test-elf` 的 10 项测试覆盖 parser 边界；`make test-boot`、`make test-interaction`、`make test-page-fault` 和两阶段 journal replay 都要求 PID 1 成功返回，借此覆盖 ELF/GDT/MSR/page-table fast path 与已有中断、异常和存储流程的组合。
