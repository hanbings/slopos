# 两个用户进程、timer preemption、wait/reap 与异步 VFS syscall

当前内核在中断子系统初始化、ext4 root mount 与 journal recovery 后，由 block task 驱动 PID 1 与 PID 2。它的目标是验证一条最小但真实的 VFS ELF→process table→cooperative/preemptive scheduler→x86-64 privilege/syscall→async block completion boundary，而不是模拟用户态日志：

- 以独立 `slopos-elf` crate 校验 little-endian x86-64 `ET_EXEC`、program-header geometry、`PT_LOAD` range/alignment/overlap/W^X 与 executable entry；
- 由 `userspace/init` 生成 26312-byte Rust ELF、由 `userspace/worker` 生成另一个 25552-byte Rust ELF；rootfs builder 分别安装为 inode 23 `/sbin/slop-init` 和 inode 24 `/sbin/slop-worker`，kernel 的 ext4 path walker 各跨七个逻辑块读取全部 bytes；
- UEFI 仍从 ESP `/slopos/init.elf` 读入 BootInfo v2 校验副本；kernel 要求 root VFS image 与该副本逐字节一致，差异会停止启动；
- 从 ELF file offset `0x1000` 分别复制 init 的 2576-byte 与 worker 的 1808-byte R+X `PT_LOAD`，各 code page 的剩余部分保持为零；
- 把 image/CR3/entry/stack/user range 插入容量 4 的 `slopos-process` 表；状态机支持 `Ready → Running → Blocked/Runnable → Running → Exited`，每个 PID 独立保留 exit status、syscall count、pending syscall 与保存的 syscall frame；
- 每个 slot 自带独立、容量 8 的 `slopos-vfs::FileDescriptorTable`；6 项宿主测试覆盖 Linux 初始栈、PID/parent/capacity、blocked/runnable/round-robin transition、exit/reap/`close_all`，以及两个进程各自取得 fd 3、独立 seek 且 offset 互不影响；
- frame allocator 为两个进程各建一个 PML4，并保留 supervisor-only kernel identity map；allocator 另有容量 256 的 recycled-frame stack，拒绝未分配、未对齐、重复或超容量释放；
- 每个地址空间都在 `0x40000000` 映射一个 CPL3 可读、不可写的 code page；
- 每个地址空间都在 `0x40001000..0x40003000` 映射两个 CPL3 可读写 stack page；各 physical frame 独立，不依赖物理连续。kernel 从上层页顶向下复制各自的 `argv`/`envp` strings，再编码 `argc=2`、pointer vectors 和 9 对 Linux auxv，最终 `RSP=0x40002ec0` 且 16-byte aligned；
- 用户入口 assembly 保留原始 `RSP` 并传给 Rust；两个程序在任何 syscall 前分别核对自身 `argv[0]`/`argv[1]`、3 项 environment、`AT_PAGESZ`/`AT_ENTRY`/uid/gid/secure/`AT_EXECFN`/`AT_NULL` 与所有 NUL boundary；
- 通过 user code selector `0x23`、user data selector `0x1b` 和 `IRETQ` 从 CPL0 进入 CPL3；
- 先用 CPUID extended leaf 检查 SYSCALL/SYSRET，再配置并读回 `IA32_EFER.SCE`、`STAR`、`LSTAR` 与 `FMASK=0x47700`；
- 用户 ELF 发出真实 `SYSCALL`（opcode `0f 05`）；entry 在 IF/TF/DF/IOPL/NT/AC 被 mask 后保存 user `RSP`、`RCX` return RIP、`R11` flags 与 15 个通用寄存器，切到暂停中的 kernel continuation stack，再进入 Rust handler；
- stdout `write` 更新 frame 的 `RAX` 后直接由 `SYSRETQ` 回到 user RIP/stack；
- `openat`/regular-file `read`/`write`/`close` 把 frame 复制到该 PID 的 pending slot、把状态标为 `Blocked`，恢复 kernel CR3/continuation，让 block task 解析 VFS path 或等待 ext4/virtio Future；完成后把 return value 与 read bytes 写回保存的 frame/user stack，转为 `Runnable`，重新切换对应 process CR3，以 `SYSRETQ` 恢复同一 RIP、RSP 和通用寄存器。copy helper 会先验证整个 range，再按 virtual page 拆分并通过该 PID 的 mapping 翻译到各自的 identity-mapped physical frame；
- `sched_yield` 保存完整 frame、把 `Running` 转成 `Runnable` 并回到 block task；scheduler 从当前 PID 之后选择下一项 `Ready`/`Runnable`，到表尾时回绕。串口证据验证 `1→2→1→2`，每次切换使用不同 CR3；
- 100 Hz PIT timer 有独立的 interrupt stub，先保存 15 个 GPR，再读取 CPL3 hardware frame 的 RIP/CS/RFLAGS/RSP/SS。若另有可运行进程，top half 把完整上下文写入当前 PID 的 frame、记录 preemption tick/count、标为 `Runnable`，发送 EOI 后跳到共享 kernel continuation；block task 随即选择另一个 Ready/Runnable PID。若只有当前进程可运行则原样恢复并 `IRETQ`，kernel-mode tick 也不参与用户调度；
- 两个 ELF 在首次 cooperative 往返后都进入约 100,000,000 TSC tick 的无 syscall `spin_loop`。QEMU 串口分别出现 `timer preempt from=1 to=2` 与 `from=2 to=1`，且两个 exit marker 的 preemption count 均非零，证明切换并非由隐藏的 yield 或阻塞调用触发；
- `lseek(fd, offset, SEEK_SET)` 只修改该进程 descriptor offset，不需要设备 I/O，因此在 fast handler 内直接返回；
- `exit(0)` 把对应 process slot 标为 `Exited`，恢复原 kernel CR3、kernel stack 与 callee-saved registers；block task 随后只对该 PID 执行 `close_all`，并逐一释放它自己的 `Ext4File` backing object；
- `wait4(-1, status, 0, NULL)` 先验证 child 与 4-byte writable status range。若 child 尚未退出，它保存 PID 1 frame、转为 `Blocked`；child exit 后 block task 写回 Linux wait status、完成 reap，令父进程 `Blocked → Runnable`。timer interleaving 也可能令 PID 2 先退出，此时 slot 以 zombie 状态保留，稍后到达的 wait 立即写 status、reap 并以 child PID 返回。两条路径都不会丢失退出通知；
- reap 从 process table 移除 slot，并释放该进程的 PML4、克隆 low PDPT、user directory/table、code page 与两页 stack，共 7 个 physical frame。QEMU marker 还立即 allocate/deallocate 最后释放的 frame，证明 recycled allocator 真正复用而非只记录计数。

两个 ELF 程序都按 Linux x86-64 调用约定把 syscall number 放在 `RAX`，前三个参数放在 `RDI/RSI/RDX`，`openat` 的第四个参数放在 `R10`，并声明 `RCX`/`R11` 为 architectural clobber。PID 1 依次执行：

1. 先 `sched_yield()`，让尚为 `Ready` 的 PID 2 首次进入并立即 yield 回来；随后执行无 syscall 的 TSC 窗口，让 timer 能从 PID 1 抢占到 PID 2，而 PID 2 的对应窗口再证明反向抢占；
2. `openat(AT_FDCWD, "/etc/slopos/system.conf", O_RDONLY, 0)`，root namespace/path walker 打开 inode 18，进程自己的 descriptor table 返回 fd 3；
3. `read(3, stack_buffer, 76)`；用户 frame 暂停，block task 经 ext4 cache/virtio 异步读取，完成后用 kernel identity map 对应的 physical stack frame 执行有界 `copy_to_user`，推进该进程 fd offset，再恢复用户上下文；
4. 用户态把 76 bytes 与编译时预期内容逐字节比较，随后 `close(3)`；
5. `openat(AT_FDCWD, "/usr/share/slopos/write-probe.bin", O_RDWR, 0)` 复用 fd 3，然后在 fd 保持打开时再次 yield。PID 2 随即以自己的 descriptor table 打开配置并同样取得 fd 3，由此实测同号 descriptor 的 per-process ownership；
6. 四次 `lseek(3, 123, SEEK_SET)` 分别定位 patch/verify/restore/verify；64-byte scratch buffer 位于 `0x40001fe0..0x40002020`，故意横跨两个 stack page。两次 write 和两次 read 完成可逆 patch，四个 completion marker 都记录 `cross_page=true`；
7. PID 1 故意不显式关闭 fd 3；`write(1, message, 18)` 经 stdout 特判直接返回 18，再以 `wait4(-1, &status, 0, NULL)` 等待。根据 timer interleaving，这一步会走 blocked/wake 或 zombie/immediate 路径；两者都返回 child PID 2 与 status 0。PID 1 随后 `exit(0)`，cleanup marker 核对 `descriptors=1 backing_objects=1`。

PID 2 在第一次 yield 后打开 `/etc/slopos/system.conf`、保持自己的 fd 3 再 yield，等 PID 1 进入 wait 后读取并核对 76 bytes、close、写出 19-byte stdout message 并 `exit(0)`。process table 最终分别记录 PID 1 的 17 次 syscall、PID 2 的 7 次 syscall 与 status 0；cleanup 为 PID 2 核对 `descriptors=0 backing_objects=0`。PID 2 被 PID 1 reap，PID 1 退出时由 kernel owner reap；两者各释放 7 个 frame。

路径 copy 最多 128 bytes，单次 read/write 最多 256 bytes；read destination 必须完全落在该 PID 已知的 writable stack mappings，path/write input 必须落在它的 code 或 stack mappings。整个 range 会先完成 overflow/权限/映射验证，再逐页复制，因此不会把任意 user virtual pointer 直接解引用，也不会假定相邻 virtual page 的 physical frame 连续。未知编号返回 `-ENOSYS`，已连接调用对坏 fd、pointer、flag/path/offset 等返回当前子集的负 errno。

`STAR=0x10000800000000` 与当前 GDT 对应：SYSCALL 使用 kernel CS/SS `0x08/0x10`，64-bit SYSRET 生成 user CS/SS `0x23/0x1b`。`LSTAR` 指向 kernel ELF 内的 assembly entry；每次 QEMU 启动都核验 MSR readback。`FMASK` 确保 fast entry 在启用 Rust stack 前没有 IRQ/trace/direction-flag 窗口；返回前再清 user IOPL/NT/RF/VM 并恢复 reserved bit 与 IF。IDT 不再暴露 DPL3 vector `0x80`。

ELF 已与 kernel 分离，实际执行 bytes 来自 root VFS 的固定 `/sbin/slop-init` 与 `/sbin/slop-worker`；ESP/BootInfo 副本目前仍是 PID 1 的强制相等启动信任锚，因此这不是任意路径的通用 `exec`。每进程 descriptor ownership 已实际连接 root ext4 的 `O_RDONLY`/`O_RDWR openat`、read/write/lseek/close；单核 cooperative/preemptive scheduler 仍嵌在一个 block task 的专用 suspend/resume loop，fd 1 继续由 syscall handler 特判。现阶段没有：

- 任意路径/多 `PT_LOAD` page mapping、动态链接，或从通用 exec 参数构建初始栈；
- 超出当前一页 code/两页 stack mapping 的通用 `copy_from_user`/`copy_to_user`，或 grow/truncate/stat/directory/dup/poll/mmap 等通用 VFS syscall；
- 独立 per-thread kernel stack、kernel preemption、SMP run queue、quantum policy、kill/signal/TLS；当前 timer 只在 CPL3 且存在另一个 Ready/Runnable PID 时抢占；
- 通用 wait selector/options/rusage、多个并发 waiter、orphan adoption、PID reuse policy、credential 或通用资源引用计数；当前 ABI 只支持一个父进程以 `wait4(-1, status, 0, NULL)` 等待唯一 child；
- NX stack、kernel section W^X、COW 或 demand paging。

页表和 privilege transition 遵循 [Intel 64 and IA-32 Architectures Software Developer Manuals](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)，segment 与初始栈规则依据 [System V ABI Program Header](https://refspecs.linuxfoundation.org/elf/gabi4%2B/ch5.pheader.html) 与 [x86-64 psABI](https://gitlab.com/x86-psABIs/x86-64-ABI)。`make test-process` 的 6 项测试覆盖 initial-stack/process/scheduler/fd state；`make test-elf` 的 10 项测试覆盖 parser 边界；`make test-boot`、`make test-interaction`、`make test-page-fault` 和两阶段 journal replay 都要求两个 PID 成功返回、两个方向的 timer marker 均出现且每进程 preemption count 非零，借此覆盖 ELF/GDT/MSR/per-process page table、cooperative/preemptive switch 与已有中断、异常和存储流程的组合。
