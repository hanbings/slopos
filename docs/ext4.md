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

宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation、四类 checksum corruption、未知 incompat feature、dirty state 和 htree 拒绝。当前只接受镜像实际用到的 incompat bits：`filetype`、`extent`、`64bit`、`flex_bg`、`metadata_csum_seed`；需要 journal replay 的脏盘不会继续。

裸机路径选择第二个 virtio-blk 设备（第一个仍是 ESP）。初始 mount probe 读取 superblock、block 1、block 49 和 block 18；随后 component walker 分别解析 `etc/slopos-release` 与 `etc/slopos/system.conf`，每级都异步读取并验证 directory/inode，最终读取数据 extent，并将 40/76 bytes 与构建源逐字节核对。总计 18 次 request，每次都在上一 used-ring completion 和 device status OK 后复用 descriptor chain。

kernel `fs.rs` 已把文件系统从 virtio transport 分离，以 `ReadOnlyMount` 持有已校验 superblock/group，并以 `ReadOnlyFile` 承载 open 结果；path component validation、open 和 read 都在 block task 中异步推进。当前仍不是通用 VFS：只处理 group 0、单个 inode-table block、inode 内 depth-0 的第一个 extent，以及单块线性目录/regular file。尚无多 group、extent index block、htree、symlink、xattr、journal 或 orphan file；没有 namespace、mount table、page cache、权限、写入或内核 fsck。btrfs 完全未实现。
