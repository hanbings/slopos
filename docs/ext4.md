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

28 项宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation、checksum corruption、extent tree、fast symlink、未知 feature、dirty state、htree 拒绝，以及 big-endian JBD2 superblock/transaction。allocation 测试按 [Linux ext4 group descriptor/bitmap 布局](https://www.kernel.org/doc/html/latest/filesystems/ext4/group_descr.html) 切换 block/inode bit，更新 bitmap CRC32C、group/superblock free count、`itable_unused` 与 checksum；inode 测试覆盖 size/`i_blocks`/inline `ee_len` 修改和空 regular inode 初始化；[线性目录项测试](https://www.kernel.org/doc/html/latest/filesystems/ext4/directory.html) 拆分/合并 `rec_len` 并更新 tail checksum。各组 allocate→free 或 insert→remove 后均逐字节恢复。多 tag 测试覆盖 `SAME_UUID`、逐 tag escape 与 `LAST_TAG`。

裸机路径选择第二个 virtio-blk 设备（第一个仍是 ESP）。superblock 报告 65536 blocks、32 inodes、2 groups；group 0/1 inode table 分别是 block 37/38。component walker 打开 inode 24（group 1）并成对预取它的两个数据块；随后打开 inode 21，从 root index 读取 leaf block 85，校验 metadata checksum，由第五个 extent 映射 physical block 92，并把 logical block 7 的 hole 返回为 4096 个零。在 inode 22 的 8192-byte 线性目录中，walker 于 logical block 1 找到指向 inode 23 的 `tail-29`。最后读取 inode 14 内的 `slopos-release`，回到同一父目录定位 inode 17。VFS fd 路径再次以 chunk/seek 读取 inode 16。

inode 25 是 4096-byte write probe，映射到 physical block 98。内核先通过 cache 验证全 `P` 内容，再由读写 fd 在 offset 123 覆写 73 个 `0xa5` bytes。ext4 层对所在块 read-modify-write，等待 write 与 flush completion，失效 cache 后按 fd 读回前后 `P` 边界；随后以同一路径恢复并再次读回。测试后的 image SHA-256 与生成值一致，`e2fsck -fn` 五阶段通过。

JBD2 journal 位于隐藏 inode 8，size 16 MiB，单一 initialized extent 映射 filesystem block 32801–36896。内核校验 inode checksum/extent 后按 [Linux ext4 JBD2 文档](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html) 以 big-endian 解析首块：v2 superblock、4096-byte block、maxlen 4096、first 1、sequence 1、start 0、users 1、UUID 与 ext4 相同，feature words 全零。`start=0` 单独并不证明 journal clean；当前只证明生成镜像的 journal 几何属于 writer 将支持的边界，尚未扫描/replay transaction。

共享 crate 还能无分配地编码/解析当前零-feature 格式的单块 transaction：一个含 UUID、target block、`LAST_TAG`/可选 `ESCAPE` 的 descriptor block，一个 journal data block，以及同 sequence 的 commit block。宿主 round-trip 会覆盖 home block 以 JBD2 magic 开头时的 escape/restore，并拒绝 `SAME_UUID` 首 tag、零 sequence、错误尺寸和损坏 header。状态更新器安全修改 JBD2 superblock 与 ext4 recovery bit；内核已经把这些更新与 record/home-block ordering 组合成 active data-block checkpoint。

裸机随后确认 journal logical blocks 1–3 初始全零，把 target physical block 98、sequence 1 的 descriptor/data/commit 写到 physical blocks 32802/32803/32804。descriptor 与 data 完成后 flush，commit 完成后再次 flush；三个 record 再经独立 DMA readback 与内存编码逐字节比较。最后写回三个零块并第三次 flush，宿主确认 image hash 恢复且 `e2fsck` 通过。此阶段故意不修改 JBD2 superblock 或 ext4 recovery bit，marker 因此报告 `active=false`；它证明 record layout/ordering/persistence，不证明崩溃 replay。

独立状态 probe 读取并保留 filesystem block 0 与 JBD2 superblock。它先置 `needs_recovery`、更新 ext4 checksum、write+flush，再把 JBD2 `start` 设为 1 并 write+flush。DMA 读回同时证明普通 `Superblock::parse` 返回 `DirtyFilesystem`、JBD2 parser 接受 sequence 1/start 1。清理严格先把 journal start 归零并 flush，再清 recovery bit、重算 checksum 并 flush；最终两块读回均与 scratch 中恢复后的原值相同。该 probe 报告 `transactions=0`，只验证状态顺序。

完整 active probe 随后把同一组能力组合为 sequence 1 / target physical block 98 的事务。它先持久化 recovery/start，再发布 descriptor+data、flush、发布 commit、flush；独立 DMA readback 同时验证 dirty ext4 state、active JBD2 superblock 和三个 replayable records。home data block 写入并 flush 后，cache invalidation/readback 证明 checkpoint 生效；随后 JBD2 推进到 sequence 2/start 0，清 ext4 recovery，再清 records。测试清理恢复全 `P` home block并将 journal sequence 回卷到 1，最终 block 0 与 journal superblock 再次解析为 clean。

metadata probe 读取 inode 25 所在的完整 inode-table block 38，以共享编码器把 size 从 4096 改为 4095 并重算 checksum。通用单块 transaction engine 用 sequence 1 journal/checkpoint 该 metadata block，cache 失效后的 `Inode::parse` 验证修改；第二笔 sequence 2 transaction journal 原始 block，并再次解析 size 4096 与原 inode 完全相等。

allocation probe 进一步读取五个 home block。第一笔 sequence 1 transaction 把 superblock free count 61311→61310，group 0 free count 32672→32671，在 block bitmap 33 标记 physical block 99，向该块写入全 `G`，并把 inode 25 的 size/i_blocks/inline extent 扩为 8192/16/2；每一级 checksum 都在编码后解析，commit 后 cache miss 重新读取 logical block 1。第二笔 sequence 2 transaction journal 原始五块并释放 block 99，测试回卷 sequence 后磁盘逐字节恢复。

create probe 的五个 home 是 superblock 0、group descriptor block 1、group 1 inode bitmap 36、inode table 38 与 directory data 83。第一笔 transaction 把全局/group free inode 从 7→6、`itable_unused` 7→6，分配 inode 26，编码 mode 0644、links 1、size/i_blocks 0/0 与空 depth-0 extent header，并在 inode 20 的 checksum 目录中插入 `create-probe`；正常 `resolve_path/open_file` 随即读回该空文件。第二笔 transaction 以共享 remover 合并目录 slack、释放 bit 并恢复原始 inode slot。整个 clean boot 总计 447 requests/446 queue interrupts，cache 为 74 hit/69 miss/16 invalidation；固定 image hash 与 fsck 五阶段通过。

`make test-journal-replay` 提供独立两阶段 crash-consistency 回归。第一阶段以 feature kernel 把 blocks 0/1/33/38/99 持久化为“free count -1、bitmap 99 allocated、inode 25 size 8192/extent 2、data 全 G”的旧 home，再置 recovery/start、发布目标为五块原始 free 状态的 descriptor/data/commit，并在 commit flush 后、home checkpoint 前停止 QEMU。宿主直接确认 `needs_recovery`、free blocks 61310、bitmap 99、inode size/blockcount 8192/16 与全 G data。第二阶段使用普通 kernel 与同一 root disk 重启：mount recovery 验证 sequence 1、UUID 与五个 tag，依次重放并读回全部 home，然后清七个 records、推进 sequence 2/start 0，最后清 ext4 recovery bit/checksum。启动继续穿过既有 VFS、allocation 与 create probes 并进入桌面，总计 477 requests/476 queue interrupts；宿主逐块比较五个 crash home 与注入前快照，确认 free blocks 61311、block 99 free、inode 4096/8、无 recovery feature且 `e2fsck -fn` 通过。脚本最终重建标准 root image 并核对固定 SHA-256。

kernel `fs.rs` 已把文件系统从 virtio transport 分离，以 `Ext4Mount`/`Ext4File` 承载 mount/open 结果；每个 inode lookup 根据 `inodes_per_group` 选择并校验 descriptor。extent walker 以单调递减的预期 depth 异步读取最多五层节点；当前真实镜像验证到 depth 1。线性目录 walker 对 `ceil(size/block_size)` 个块逐一执行 extent mapping、cache read、checksum parse 和名称查找；hole/unwritten file block 使用共享零页。当前 symlink follow 有意限制为最终 path component、单个相对 component、inline target 和 regular-file 目标；尚未实现绝对/多段/中间/外部 symlink 或 loop limit。cache 使用 8 个永久 frame 和 FIFO victim。VFS 写路径仍只允许覆写已有完整 block；create 与 allocation engine 仍是内核 probe，尚未接入 descriptor create/append。replay scanner 最多接受八 tag 的零-feature、非 wrap 单 transaction，并拒绝越界、重复或 journal 自覆盖 target。上层 VFS 仍是 root-only 固定 size 状态机；尚无 htree mutation、xattr、orphan recovery、权限或内核 fsck。btrfs 完全未实现。
