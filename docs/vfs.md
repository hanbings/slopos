# VFS namespace 与文件描述符

`slopos-vfs` 是独立的 `no_std`、无分配状态机，不依赖 ext4 或 virtio。它提供：

- 最多 16 个 component 的绝对路径解析，折叠重复 `/`、`.` 和 `..`，拒绝 NUL、超长 component 和相对路径；
- const-generic mount table，以 component 边界上的最长前缀选择 filesystem；
- 最多 256-byte 的规范化 mount path；
- const-generic fd table，从 fd 3 开始分配；
- 每个 descriptor 保存 filesystem/node identity、file size、offset 和 read/write access mode；
- bounded read/write window、成功后 offset advance、absolute seek 与 close。

内核当前建立容量 4 的 namespace，把 ext4 注册为 filesystem 1 并挂到 `/`；容量 8 的 fd table 为 `/etc/./slopos/../slopos/system.conf` 分配只读 fd 3。裸机测试将规范化后的相对 component 交给 ext4，使用 17-byte request 分五次读完 76-byte 文件，再 seek 到 offset 7 读取 11 bytes，最后 close。关闭后的 fd 3 会被复用于 inode 25 的 `ReadWrite` descriptor；它 seek 到 offset 123，写入 73 bytes，通过同一 fd 读回边界内容，再恢复原始 bytes。

`make test-vfs` 的 4 项宿主测试覆盖路径规范化、root/`/mnt`/`/mnt/data` 最长挂载匹配、fd offset 生命周期，以及 `ReadOnly`/`WriteOnly`/`ReadWrite` 权限错误。`make test-boot` 验证同一状态机驱动真实 ext4 cache 与 virtio DMA。

当前边界：

- mount table 和 fd table 只活在 block task 内，不是全局或每进程对象；
- 只有一个 root filesystem，没有 mount/unmount 生命周期或引用计数；
- 只有 regular-file read/原位 write/seek/close，没有 directory fd、stat、dup、poll、mmap、owner/mode 权限或文件增长；
- 没有 syscall，因此用户程序尚不能访问这些 fd；
- fd write 只覆盖已有 initialized extent 中完整存在的 block；底层 ext4 已有 create/unlink 与 block growth probe，但尚未接到 VFS create/truncate/append API。
