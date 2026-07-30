# ext4 root disk probe

SlopOS 构建同时产生两个磁盘：

- `slopos-esp.img`：64 MiB FAT32 ESP，仅由 UEFI loader 使用；
- `slopos-root.ext4`：256 MiB、2 block-group ext4 root disk，由内核通过第二个 virtio-blk function 访问。

`scripts/make-rootfs.sh` 固定 4096-byte block、32 个 inode、label、filesystem UUID、所有 superblock copy 的 directory hash seed 和 e2fsprogs fake time，并关闭 lazy inode table/journal 初始化。脚本在临时 staging tree 中加入 6144-byte `Z` payload，再归一化全部 inode metadata。当前 e2fsprogs 1.47.2 下重复生成的 SHA-256 为 `a7b049322d8dd873efa1edf634da66be23f93a9bade170941966e7f9dc968e2d`。生成物位于 `target/`，不提交仓库。

`slopos-ext4` 是无标准库、无分配 parser。当前验证：

- `0xef53` magic；
- block size、inode size 和 group descriptor size 边界；
- low/high 64-bit block/free-block count；
- per-group geometry、filesystem state 和 error policy；
- compat/incompat/read-only-compat feature masks；
- UUID、volume label、checksum type；
- 启用 `metadata_csum` 时，核对 superblock Castagnoli CRC32C；
- group descriptor checksum、64-bit inode table address 与 root inode 定位；
- group count、descriptor block/offset 和 inode-to-group 计算；
- inode checksum、mode/uid/gid/size/flags 与 depth-0 extent；
- inline extent run 的 logical-block lookup、连续块与 hole 区分；
- variable-length directory entry、directory checksum tail 与按字节名称查找。

宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation、四类 checksum corruption、未知 incompat feature、dirty state 和 htree 拒绝。当前只接受镜像实际用到的 incompat bits：`filetype`、`extent`、`64bit`、`flex_bg`、`metadata_csum_seed`；需要 journal replay 的脏盘不会继续。

裸机路径选择第二个 virtio-blk 设备（第一个仍是 ESP）。superblock 报告 65536 blocks、32 inodes、2 groups；group 0/1 inode table 分别是 block 37/38。component walker 最终打开 inode 20（group 1），校验 group 1 descriptor 后读取它的两个数据块。8-entry FIFO cache 记录 18 hit/14 miss；superblock 保持独立读取，所以实际设备 request/IRQ 均为 15。

kernel `fs.rs` 已把文件系统从 virtio transport 分离，以 `ReadOnlyMount`/`ReadOnlyFile` 承载 mount/open 结果；每个 inode lookup 根据 `inodes_per_group` 选择并校验 descriptor。cache 使用 8 个永久 frame 和 FIFO victim，当前只读阶段不需要 dirty/writeback。当前仍不是通用 VFS：只处理 depth-0 inline extents 和单块线性目录。尚无 extent index block、htree、symlink、xattr、journal 或 orphan file；没有 namespace、mount table、权限、写入或内核 fsck。btrfs 完全未实现。
