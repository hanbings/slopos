# VFS namespace 与文件描述符

`slopos-vfs` 是独立的 `no_std`、无分配状态机，不依赖 ext4 或 virtio。它提供：

- 最多 16 个 component 的绝对路径解析，折叠重复 `/`、`.` 和 `..`，拒绝 NUL、超长 component 和相对路径；
- const-generic mount table，以 component 边界上的最长前缀选择 filesystem；
- 最多 256-byte 的规范化 mount path；
- const-generic fd table，从 fd 3 开始分配；
- 每个 descriptor 保存 filesystem/node identity、file size 和 offset；
- bounded read window、成功后 offset advance、absolute seek 与 close。

内核当前建立容量 4 的 namespace，把 ext4 注册为 filesystem 1 并挂到 `/`；容量 8 的 fd table 为 `/etc/./slopos/../slopos/system.conf` 分配 fd 3。裸机测试将规范化后的相对 component 交给 ext4，使用 17-byte request 分五次读完 76-byte 文件，再 seek 到 offset 7 读取 11 bytes，最后 close。

`make test-vfs` 的 3 项宿主测试覆盖路径规范化、root/`/mnt`/`/mnt/data` 最长挂载匹配，以及 fd read window/advance/seek/close/error。`make test-boot` 验证同一状态机驱动真实 ext4 cache 与 virtio DMA。

当前边界：

- mount table 和 fd table 只活在 block task 内，不是全局或每进程对象；
- 只有一个 root filesystem，没有 mount/unmount 生命周期或引用计数；
- 只有 regular-file read/seek/close，没有 directory fd、stat、dup、poll、mmap 或权限检查；
- 没有 syscall，因此用户程序尚不能访问这些 fd；
- VFS API 仍只读；底层 ext4 现有的整块原位 write probe 尚未暴露为 fd write。
