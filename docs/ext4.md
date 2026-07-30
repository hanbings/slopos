# ext4 root disk probe

SlopOS 构建同时产生两个磁盘：

- `slopos-esp.img`：64 MiB FAT32 ESP，仅由 UEFI loader 使用；
- `slopos-root.ext4`：128 MiB ext4 root disk，由内核通过第二个 virtio-blk function 访问。

`scripts/make-rootfs.sh` 固定 4096-byte block、label、filesystem UUID、directory hash seed 和 e2fsprogs fake time，并关闭 lazy inode table/journal 初始化。当前 source tree 重复生成的 SHA-256 为 `4cf2289f4767ea338f170b065d682497f243a6adc731ce52023a2faf0549e814`。镜像包含 `/etc/slopos-release` 与声明式配置 seed，但生成物位于 `target/`，不提交仓库。

`slopos-ext4` 是无标准库、无分配 parser。当前读取标准 offset 1024 的完整 superblock，并验证：

- `0xef53` magic；
- block size、inode size 和 group descriptor size 边界；
- low/high 64-bit block/free-block count；
- per-group geometry、filesystem state 和 error policy；
- compat/incompat/read-only-compat feature masks；
- UUID、volume label、checksum type；
- 启用 `metadata_csum` 时，以 seed `~0` 计算 Castagnoli CRC32C 并核对 `s_checksum`。

宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation 和 checksum corruption。QEMU 证据来自第二块 virtio disk 的实际 1024-byte DMA/INTx/Future completion。

当前边界是 mount probe，不是完整文件系统：尚未解析 block group descriptor、block/inode bitmap、inode、extent tree、htree/directory entry、xattr、symlink、journal 或 orphan file；没有 VFS、page cache、权限、写入或 fsck。btrfs 完全未实现。
