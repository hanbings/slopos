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

20 项宿主测试覆盖高 32-bit count、动态 inode/descriptor size、bad magic、非法 geometry、truncation、checksum corruption、extent tree、fast symlink、未知 feature、dirty state、htree 拒绝，以及 big-endian JBD2 superblock/transaction。inode size 更新器同时写低/高 32 bits、重算 metadata checksum，round-trip 后逐字节恢复并拒绝损坏输入。状态测试会置位 ext4 `needs_recovery`、重算 CRC32C，并确认普通 parser 拒绝；清除后整个 superblock 逐字节恢复。JBD2 `s_sequence/s_start` 更新器同样验证 active→checkpointed→原始状态的 big-endian round-trip。

裸机路径选择第二个 virtio-blk 设备（第一个仍是 ESP）。superblock 报告 65536 blocks、32 inodes、2 groups；group 0/1 inode table 分别是 block 37/38。component walker 打开 inode 24（group 1）并成对预取它的两个数据块；随后打开 inode 21，从 root index 读取 leaf block 85，校验 metadata checksum，由第五个 extent 映射 physical block 92，并把 logical block 7 的 hole 返回为 4096 个零。在 inode 22 的 8192-byte 线性目录中，walker 于 logical block 1 找到指向 inode 23 的 `tail-29`。最后读取 inode 14 内的 `slopos-release`，回到同一父目录定位 inode 17。VFS fd 路径再次以 chunk/seek 读取 inode 16。

inode 25 是 4096-byte write probe，映射到 physical block 98。内核先通过 cache 验证全 `P` 内容，再由读写 fd 在 offset 123 覆写 73 个 `0xa5` bytes。ext4 层对所在块 read-modify-write，等待 write 与 flush completion，失效 cache 后按 fd 读回前后 `P` 边界；随后以同一路径恢复并再次读回。测试后的 image SHA-256 与生成值一致，`e2fsck -fn` 五阶段通过。

JBD2 journal 位于隐藏 inode 8，size 16 MiB，单一 initialized extent 映射 filesystem block 32801–36896。内核校验 inode checksum/extent 后按 [Linux ext4 JBD2 文档](https://www.kernel.org/doc/html/latest/filesystems/ext4/journal.html) 以 big-endian 解析首块：v2 superblock、4096-byte block、maxlen 4096、first 1、sequence 1、start 0、users 1、UUID 与 ext4 相同，feature words 全零。`start=0` 单独并不证明 journal clean；当前只证明生成镜像的 journal 几何属于 writer 将支持的边界，尚未扫描/replay transaction。

共享 crate 还能无分配地编码/解析当前零-feature 格式的单块 transaction：一个含 UUID、target block、`LAST_TAG`/可选 `ESCAPE` 的 descriptor block，一个 journal data block，以及同 sequence 的 commit block。宿主 round-trip 会覆盖 home block 以 JBD2 magic 开头时的 escape/restore，并拒绝 `SAME_UUID` 首 tag、零 sequence、错误尺寸和损坏 header。状态更新器安全修改 JBD2 superblock 与 ext4 recovery bit；内核已经把这些更新与 record/home-block ordering 组合成 active data-block checkpoint。

裸机随后确认 journal logical blocks 1–3 初始全零，把 target physical block 98、sequence 1 的 descriptor/data/commit 写到 physical blocks 32802/32803/32804。descriptor 与 data 完成后 flush，commit 完成后再次 flush；三个 record 再经独立 DMA readback 与内存编码逐字节比较。最后写回三个零块并第三次 flush，宿主确认 image hash 恢复且 `e2fsck` 通过。此阶段故意不修改 JBD2 superblock 或 ext4 recovery bit，marker 因此报告 `active=false`；它证明 record layout/ordering/persistence，不证明崩溃 replay。

独立状态 probe 读取并保留 filesystem block 0 与 JBD2 superblock。它先置 `needs_recovery`、更新 ext4 checksum、write+flush，再把 JBD2 `start` 设为 1 并 write+flush。DMA 读回同时证明普通 `Superblock::parse` 返回 `DirtyFilesystem`、JBD2 parser 接受 sequence 1/start 1。清理严格先把 journal start 归零并 flush，再清 recovery bit、重算 checksum 并 flush；最终两块读回均与 scratch 中恢复后的原值相同。该 probe 报告 `transactions=0`，只验证状态顺序。

完整 active probe 随后把同一组能力组合为 sequence 1 / target physical block 98 的事务。它先持久化 recovery/start，再发布 descriptor+data、flush、发布 commit、flush；独立 DMA readback 同时验证 dirty ext4 state、active JBD2 superblock 和三个 replayable records。home data block 写入并 flush 后，cache invalidation/readback 证明 checkpoint 生效；随后 JBD2 推进到 sequence 2/start 0，清 ext4 recovery，再清 records。测试清理恢复全 `P` home block并将 journal sequence 回卷到 1，最终 block 0 与 journal superblock 再次解析为 clean。

metadata probe 读取 inode 25 所在的完整 inode-table block 38，以共享编码器把 size 从 4096 改为 4095 并重算 checksum。通用单块 transaction engine 用 sequence 1 journal/checkpoint 该 metadata block，cache 失效后的 `Inode::parse` 验证修改；第二笔 sequence 2 transaction journal 原始 block，并再次解析 size 4096 与原 inode 完全相等。JBD2 此时已推进到 sequence 3，测试再回卷至 1。整个启动序列总计 193 个请求、192 次 queue interrupt，cache 为 64 hit/54 miss/5 invalidation；image hash 与 fsck 保持不变。它证明单个 inode-table block 的 metadata transaction，但尚未实现从故意中断状态启动后的 replay，也未更新 bitmap、extent 或 directory。

`make test-journal-replay` 提供独立两阶段 crash-consistency 回归。第一阶段以 feature kernel 先把 physical block 98 持久化为旧 home `J`，再置 recovery/start、发布目标为新 home `P` 的 descriptor/data/commit，并在 commit flush 后、home checkpoint 前停止 QEMU；宿主确认 `needs_recovery` 和全 `J` home。第二阶段使用普通 kernel 与同一 root disk 重启：mount recovery 联合解析三块 record，验证 sequence 1/UUID/target 98，重放全 `P` home 并读回，然后清 records、推进 sequence 2/start 0，最后清 ext4 recovery bit/checksum。启动继续穿过既有 VFS 与 transaction probes并进入桌面，总计 205 requests/204 queue interrupts；宿主确认 home 全 `P`、无 recovery feature 且 `e2fsck -fn` 通过。脚本最终重建标准 root image 并核对固定 SHA-256。

kernel `fs.rs` 已把文件系统从 virtio transport 分离，以 `Ext4Mount`/`Ext4File` 承载 mount/open 结果；每个 inode lookup 根据 `inodes_per_group` 选择并校验 descriptor。extent walker 以单调递减的预期 depth 异步读取最多五层节点；当前真实镜像验证到 depth 1。线性目录 walker 对 `ceil(size/block_size)` 个块逐一执行 extent mapping、cache read、checksum parse 和名称查找；hole/unwritten file block 使用共享零页。当前 symlink follow 有意限制为最终 path component、单个相对 component、inline target 和 regular-file 目标；尚未实现绝对/多段/中间/外部 symlink 或 loop limit。cache 使用 8 个永久 frame 和 FIFO victim。VFS 写路径只允许覆写 regular file 已分配、已初始化、完整的 4096-byte block；metadata engine 当前只由受限 size/checksum probe 驱动，尚未接入文件增长。replay scanner 只接受零-feature、单 tag、非 wrap 的一笔 transaction，并拒绝 block 0、越界或 journal 自覆盖 target。上层 VFS 仍是 root-only 固定 size 状态机；尚无 bitmap/extent/directory transaction、htree、xattr、orphan recovery、权限或内核 fsck。btrfs 完全未实现。
