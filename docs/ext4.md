# ext4 root disk probe

SlopOS 构建同时产生两个磁盘：

- `slopos-esp.img`：64 MiB FAT32 ESP，仅由 UEFI loader 使用；
- `slopos-root.ext4`：128 MiB ext4 root disk，由内核通过第二个 virtio-blk function 访问。

`scripts/make-rootfs.sh` 固定 4096-byte block、label、filesystem UUID、directory hash seed 和 e2fsprogs fake time，并关闭 lazy inode table/journal 初始化。脚本还把 source inode 的 mode、uid/gid、atime/ctime/mtime/crtime 归一化，避免 checkout 的 uid/umask/mtime 污染镜像。当前 e2fsprogs 1.47.2 下重复生成的 SHA-256 为 `29c8f332ce7c9aca80f4c0253ddebaa8a92985b42125e5a006d7f97887b74811`。镜像包含 `/etc/slopos-release` 与声明式配置 seed，但生成物位于 `target/`，不提交仓库。

`slopos-ext4` 是无标准库、无分配 parser。当前验证：

- `0xef53` magic；
- block size、inode size 和 group descriptor size 边界；
- low/high 64-bit block/free-block count；
- per-group geometry、filesystem state 和 error policy；
- compat/incompat/read-only-compat feature masks；
- UUID、volume label、checksum type；
- 启用 `metadata_csum` 时，核对 superblock Castagnoli CRC32C；
- group descriptor checksum、64-bit inode table address 与 root inode 定位；
- inode checksum、mode/uid/gid/size/flags 与 depth-0 extent；
- variable-length directory entry、directory checksum tail 与按字节名称查找。

宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation，以及 superblock/group/inode/directory checksum corruption。裸机路径选择第二个 virtio-blk 设备（第一个仍是 ESP），顺序读取 sector 2–3、block 1、block 49 和 block 18；每次 request 都在上一 used-ring completion 和 device status OK 后复用 descriptor chain。真实镜像证据确认 inode table 49、root inode 2、extent block 18，以及 inode 13 的 `etc` 和 inode 11 的 `lost+found`。

当前边界仍是 mount probe：只处理 group 0、单个 inode-table block、inode 内 depth-0 的第一个 extent 和一个线性 root directory block。尚无多 group、extent index block、htree、通用路径遍历、文件内容、xattr、symlink、journal 或 orphan file；没有 VFS、page cache、权限、写入或 fsck。btrfs 完全未实现。
