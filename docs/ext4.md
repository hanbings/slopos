# ext4 root disk probe

SlopOS 构建同时产生两个磁盘：

- `slopos-esp.img`：64 MiB FAT32 ESP，仅由 UEFI loader 使用；
- `slopos-root.ext4`：256 MiB、2 block-group ext4 root disk，由内核通过第二个 virtio-blk function 访问。

`scripts/make-rootfs.sh` 固定 4096-byte block、32 个 inode、label、filesystem UUID、所有 superblock copy 的 directory hash seed 和 e2fsprogs fake time，并关闭 lazy inode table/journal 初始化。脚本在临时 staging tree 中加入 6144-byte `Z` payload、九块 `D` payload 和一块 `P` write probe；`D` 文件打出四个交替 hole，五个初始化 run 会强制产生 depth-1 extent leaf。另一个目录通过长名称和 hard link 扩展为两个不连续数据块；`/etc/current-release` 是目标为 `slopos-release` 的 fast symlink。最后归一化全部 inode metadata。当前 e2fsprogs 1.47.2 下两次生成逐字节一致，SHA-256 为 `4aeb38e91e7436b303569e9bd48145e01458dcc513f8db230f20b90a5d4a1fe2`。生成物位于 `target/`，不提交仓库。

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
- inode checksum、mode/uid/gid/size/flags 与 inode 内 extent root；
- depth 0–5 header、index entry、leaf extent、`ee_len=0x8000` 特例、排序和范围；
- inline/external extent run 的 logical-block lookup、连续块与 hole 区分；
- 外部 extent block 以 filesystem seed、inode number/generation 和 tail offset 校验 CRC32C；
- symlink inode type、零 block fast-link 条件与 inode 内 1–60 byte target；
- variable-length directory entry、directory checksum tail 与按字节名称查找。

13 项宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation、五类 checksum corruption、extent length/index/depth、fast symlink、未知 incompat feature、dirty state 和 htree 拒绝。当前只接受镜像实际用到的 incompat bits：`filetype`、`extent`、`64bit`、`flex_bg`、`metadata_csum_seed`；需要 journal replay 的脏盘不会继续。

裸机路径选择第二个 virtio-blk 设备（第一个仍是 ESP）。superblock 报告 65536 blocks、32 inodes、2 groups；group 0/1 inode table 分别是 block 37/38。component walker 打开 inode 24（group 1）并成对预取它的两个数据块；随后打开 inode 21，从 root index 读取 leaf block 85，校验 metadata checksum，由第五个 extent 映射 physical block 92，并把 logical block 7 的 hole 返回为 4096 个零。在 inode 22 的 8192-byte 线性目录中，walker 于 logical block 1 找到指向 inode 23 的 `tail-29`。最后读取 inode 14 内的 `slopos-release`，回到同一父目录定位 inode 17。VFS fd 路径再次以 chunk/seek 读取 inode 16。

inode 25 是 4096-byte write probe，映射到 physical block 98。内核先通过 cache 验证全 `P` 内容，再把整个已分配块覆写为 `0xa5`，等待 write 与 flush completion，失效 cache 后重新 DMA 读回；随后以同一路径恢复全 `P` 并再次读回。测试后的 image SHA-256 与生成值一致，`e2fsck -fn` 五阶段通过。最终 cache 为 60 hit/47 miss/2 invalidation；连同两次 write、两次 flush 和未缓存 superblock，共 52 个请求由 51 次 queue interrupt 完成。

kernel `fs.rs` 已把文件系统从 virtio transport 分离，以 `Ext4Mount`/`Ext4File` 承载 mount/open 结果；每个 inode lookup 根据 `inodes_per_group` 选择并校验 descriptor。extent walker 以单调递减的预期 depth 异步读取最多五层节点；当前真实镜像验证到 depth 1。线性目录 walker 对 `ceil(size/block_size)` 个块逐一执行 extent mapping、cache read、checksum parse 和名称查找；hole/unwritten file block 使用共享零页。当前 symlink follow 有意限制为最终 path component、单个相对 component、inline target 和 regular-file 目标；尚未实现绝对/多段/中间/外部 symlink 或 loop limit。cache 使用 8 个永久 frame 和 FIFO victim。写路径只允许覆写 regular file 已分配、已初始化、完整的 4096-byte block；它不改变 inode size/extent、位图、目录或 journal。上层 VFS 仍是 root-only read/seek 状态机；尚无 htree、xattr、orphan file、权限、通用 metadata write/recovery 或内核 fsck。btrfs 完全未实现。
