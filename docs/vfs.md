# VFS namespace 与文件描述符

`slopos-vfs` 是独立的 `no_std`、无分配状态机，不依赖 ext4 或 virtio。它提供：

- 最多 16 个 component 的绝对路径解析，折叠重复 `/`、`.` 和 `..`，拒绝 NUL、超长 component 和相对路径；
- const-generic mount table，以 component 边界上的最长前缀选择 filesystem；
- 最多 256-byte 的规范化 mount path；
- const-generic fd table，从 fd 3 开始分配；
- 每个 descriptor 保存 filesystem/node identity、file size、offset 和 read/write access mode；
- bounded read/write window、成功后 offset advance、absolute seek 与 close。

内核当前建立容量 4 的 namespace，把 ext4 注册为 filesystem 1 并挂到 `/`。mount/recovery 后，block task 经同一个 component walker 打开 inode 23 的 `/sbin/slop-init` 与 inode 24 的 `/sbin/slop-shell`，各跨七个逻辑块读出 26344/26592 bytes。init 与 BootInfo 保留的引导副本完全匹配后，两份 VFS bytes 分别交给 ELF/process loader。

PID 1 与 PID 2 通过 `sched_yield` cooperative 交错，也在各自无 syscall TSC 窗口被 100 Hz timer 双向抢占。PID 1 发出 Linux x86-64 `openat(AT_FDCWD, "/etc/slopos/system.conf", O_RDONLY)`，root namespace 解析 inode 18；PID 2 则依次打开 inode 20 `/etc/slopos/waybar.jsonc` 与 inode 17 `/etc/slopos/swww.env`。fast handler 保存对应 user frame并回到 block task，各自容量 8 的 fd table 都可返回 fd 3；`Ext4File` 存在按 PID 分隔的 backing array 中，因此同号 fd 不会碰撞。read 暂停各自上下文，异步 ext4/virtio completion 把 bytes 复制到对应 writable user stack、推进独立 descriptor offset并恢复原 RIP/RSP/GPR。PID 2 以最多 256-byte chunk 读取上限为 4096/512 bytes 的 Waybar/swww 配置，增量计算 hash并验证非空 EOF，最后经私有 syscall 发布 desktop policy；第二个私有 syscall 再用相同 suspend/Future/wake/resume 结构等待 desktop apply event。

PID 1 随后以 `O_RDWR` 打开 `/usr/share/slopos/write-probe.bin`，复用 fd 3。`lseek(3, 123, SEEK_SET)` 直接更新独立 descriptor offset；`write(3, patch, 64)` 的 input 故意横跨两个 user stack page。kernel 先验证完整 range，再逐页翻译各自的 physical frame并复制到 pending request，让 block task 对 inode 31 执行 read-modify-write、virtio write/flush 与 cache invalidation。用户态 read 也跨两页写回并验证 patch，随后以同样路径恢复 64 个 `P` bytes并再次读回。PID 1 显式 close fd 3，再进入常驻 `wait4(-1)`；PID 2 的每轮 Waybar/swww 读取也完整 close，因此稳定 runtime 不保留 backing object。所有需要 I/O 的同步 ABI 调用与阻塞式 process/event wait 都通过 suspend/completion 或 scheduler wake 实现，没有在 syscall handler 内 busy-wait。

启动 probe 另有一张 block-task 局部容量 8 的 fd table：它为规范化的 `/etc/./slopos/../slopos/system.conf` 分配 fd 3，使用 17-byte request 分五次读完同一 inode，seek 到 offset 7 再读取 11 bytes，最后 close。关闭后的 fd 3 会被复用于 inode 31 的 `ReadWrite` descriptor；它 seek 到 offset 123，写入 73 bytes，通过同一 fd 读回边界内容，再恢复原始 bytes。

`make test-vfs` 的 5 项宿主测试覆盖路径规范化、root/`/mnt`/`/mnt/data` 最长挂载匹配、fd offset 生命周期、EOF growth，以及 `ReadOnly`/`WriteOnly`/`ReadWrite` 权限错误。`make test-boot` 验证同一状态机驱动真实 ext4 cache 与 virtio DMA。

`slopos-process` 的每个 process slot 内嵌一张独立 `FileDescriptorTable`。`make test-process` 会生成 parent/child 两个 record，让它们对同一 `FileNode` 各自取得 fd 3，只 advance/seek parent offset，并确认 child 仍为 0；process exit 后所有 descriptor operation 都被拒绝。裸机 PID 1/2 已把各自的表用于真实 root ext4，且两者在交错执行期间同时持有 fd 3；fd 1 仍由 syscall handler 特判，`Ext4File` backing object 暂存在 block task 的固定二维数组中。

ext4 mount 后，block task 还用可失败的 component walker 按 user/system/fallback 候选发现 niri KDL、Waybar JSONC、Waybar CSS 与 swww environment。root image 默认命中 `/etc/slopos/niri.kdl`、`waybar.jsonc`、`waybar.css` 和 `swww.env`。四份文件先读入 inactive fixed bank并完整 parse，成功才发布新 generation；desktop task 整体 swap 后 acknowledge 并发布 `config-applied`。常驻 PID 2 收到新 generation 后通过普通 fd 重读系统 Waybar/swww，再提交下一代 policy。`RELOAD` 会通过 executor waker 让同一 block task 重读 VFS，任一文件缺失、超限、非法 UTF-8 或 parse 失败都保留上一代且不唤醒 PID 2。user/system fallback reload 仍由 kernel 发现，watcher 与通用配置 service 尚未搬到用户态。

当前边界：

- mount table 与按 PID 分隔的 ext4 backing-object array 仍只活在 block task；process fd table 已独立，但没有并发全局 vnode/reference layer；
- 只有一个 root filesystem，没有 mount/unmount 生命周期或引用计数；
- 用户 syscall 目前只有 root regular-file `O_RDONLY`/`O_RDWR openat`、最多 256-byte 且限于一页 code/两页 stack mapping 的 read/write、`SEEK_SET lseek`、close、`sched_yield`，以及特判 stdout write；没有 grow/truncate/stat、directory fd、dup、poll、mmap、owner/mode 权限或任意并发请求；
- fd write 可覆写已有 initialized block；在 descriptor 位于 EOF 时还可取得 append window，由 ext4 五-home transaction 分配一个连续 block，随后更新 node size/offset并经同一 fd 读回。truncate probe 把 offset/size 与 block metadata 一起恢复。当前只支持单块增长；create/unlink 也仍未抽象为通用 VFS namespace API。
- create transaction checkpoint 后，path walker 将新 inode 32 转为 `FileNode`，固定表复用读写 fd 3，空文件 read 返回 EOF；close 后才执行 unlink transaction。它证明 ext4 namespace mutation 与 descriptor 生命周期相接，但尚未抽象为可复用 VFS create/unlink API。
