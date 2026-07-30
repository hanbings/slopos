# Runtime evidence

所有证据均可从源码重新生成，不提交 ESP、ELF、OVMF VARS 或 PPM 等大型生成物。

| 文件 | 生成方式 | 证明范围 |
|---|---|---|
| `serial.log` | `make test-boot` | OVMF/UEFI、ELF、`ExitBootServices`、XSDT/MADT、memory、CPL3 PID 1、eBPF、PCI/virtio INTx、ext4、async 与桌面循环 |
| `uefi-debugcon.log` | `make test-boot` | loader 独立 debugcon 日志 |
| `interaction-serial.log` | `make test-interaction` | PS/2 键盘触发 VFS 配置 reload/rollback、执行 `STATUS`/swww，鼠标横拖 viewport，Super bind 关闭窗口/切换 workspace |
| `page-fault-serial.log` | `make test-page-fault` | 自有页表的未映射访问、vector 14、error、RIP、CR2 与 fatal boundary |
| `journal-injection-serial.log` | `make test-journal-replay` phase 1 | commit 已 flush、home 尚未 checkpoint 的 dirty disk |
| `journal-replay-serial.log` | `make test-journal-replay` phase 2 | 普通 kernel mount-time replay、清理、继续完整启动 |
| `desktop.png` | `scripts/capture-desktop.sh` | niri 式 column strip 与 Waybar 式顶部三区域 |
| `terminal-status.png` | `make test-interaction` | 图形终端对键盘命令的实际响应 |
| `window-moved.png` | `make test-interaction` | titlebar drag 后 terminal 离屏、后续 column 进入 viewport |
| `workspace-config.png` | `make test-interaction` | `slopos-config` window rule 与 `Mod+Down` 切入 named workspace |
| `wallpaper-switched.png` / `wallpaper-only.png` | `make test-interaction` | Sunset transition 后的窗口背景，以及关闭全部 tile 后的完整壁纸 |
| `qemu-test.log` | `make test-boot` | QEMU stderr/stdout；正常测试通常为空 |

核心启动命令由 `scripts/test-boot.sh` 固定，等价参数为：

```text
qemu-system-x86_64
  -machine q35,accel=tcg
  -cpu qemu64
  -m 256M
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd
  -drive if=pflash,format=raw,file=target/OVMF_VARS_4M.test.fd
  -drive if=virtio,format=raw,file=target/slopos-esp.img
  -drive if=virtio,format=raw,file=target/slopos-root.ext4
  -serial file:evidence/serial.log
  -debugcon file:evidence/uefi-debugcon.log
  -global isa-debugcon.iobase=0x402
  -display none
  -monitor none
  -no-reboot
```

用户进程证据从 UEFI 日志开始：loader 从 ESP 读取 4848-byte `/slopos/init.elf`，BootInfo v2 令 kernel 在 `LOADER_DATA` allocation 保留同一大小的校验副本。executor/block task 挂载 root 后，`SLOPOS-VFS` marker 证明从 inode 23 的 `/sbin/slop-init` 跨两个逻辑块读取 4848 bytes，且 `matches_boot=true`。process-table marker 记录 capacity 4、PID 1 Ready、每进程 fd capacity 8；load marker 记录 `source=vfs path=/sbin/slop-init format=elf64`、entry `0x40000000`、1 个 segment、66 load/memory bytes，以及独立 CR3、user code/stack、不同 physical frame 与 `code=user-readonly stack=user-writable kernel=supervisor`。fast-path marker 证明 CPUID 检查后的 MSR readback 为 `STAR=0x10000800000000`、`FMASK=0x47700`、`EFER.SCE=true`，且 LSTAR 指向 kernel entry；两个 syscall marker 分别证明 write 经 `SYSRETQ` 返回 18、exit status 0 回到 kernel。最终 retained record 为 Exited/two syscalls。`make test-process` 的 4 项宿主测试另行覆盖 PID/lifecycle/reap 和 per-process fd isolation；交互、page-fault 和 journal 两阶段测试都重复经过硬件 fast path。它证明固定 root 路径的 VFS ELF bytes 被实际执行，但仍只覆盖与引导副本完全相同的单页静态 ELF、受限 user-range check 与单一同步 running process，不证明任意路径/多 segment exec、调度或 root ext4 fd syscall。

ELF parser 的宿主测试由 `make test-elf` 执行。10 项测试验证 ELF64 little-endian/x86-64/`ET_EXEC` header、program-header table、`PT_LOAD` data/BSS view，并拒绝 truncation、越界、`p_filesz > p_memsz`、非法 alignment/congruence、重叠 segment、W+X 与不属于 executable segment 的 entry。parser 无分配且 `no_std`；section header 与 dynamic linking 不参与当前装载。

shell 状态机由 `make test-shell` 验证。8 项 layout 测试覆盖 niri KDL、稳定 strip coordinate、focus/scroll/stack/close；3 项 niri shell 测试覆盖 named workspace、bind/rule parse/reject、ordered override 和跨 workspace move；4 项 Waybar JSONC/format 与 3 项 CSS 测试覆盖 module list/option、replacement、cascade、color/box/border 与拒绝边界；7 项 swww 测试覆盖 CLI/environment parse、daemon lifecycle/output、P3/PNM 边界、两个嵌入式 asset 和 transition mask/blend。裸机先从 `/etc/slopos/{niri.kdl,waybar.jsonc,waybar.css,swww.env}` 读取 VFS 配置并原子发布 generation 1；apply marker 记录 3 个 workspace、6 个 Waybar module config 与 12 条 CSS rule。交互日志用 `RELOAD` 发布/应用 generation 2，再用诊断 `RELOAD BAD` 注入非法 CSS，记录 `invalid-waybar-style retained_generation=2` 且没有 generation 3 apply。随后日志记录 Sunset `img`、5 帧 center transition、返回当前 image 的 `query`、kill/restart、无 transition 换图、titlebar drag、两次 workspace 1 close、`Mod+Down` 切入 `config` 与 workspace 2 close。`desktop.png`/`terminal-status.png` 是 Aurora，并显示 CSS 驱动的 CPU/Memory/Clock 色块；`wallpaper-switched.png` 是 Sunset，`window-moved.png` 显示 Sunset 和横移后的 main columns，`workspace-config.png` 显示 window rule 放置的 Config 及 active workspace 2，`wallpaper-only.png` 显示关闭所有 tile 后仍由 Waybar 覆盖的完整壁纸。它不证明自动文件监听、完整 niri/Waybar、独立 swww process、Wayland layer-shell 或其他图片格式。

eBPF 的宿主边界测试由 `make test-ebpf` 执行；裸机证据是 `serial.log` 中的 `SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42`。它只证明文档所列子集，不证明 map、attach point 或 Linux eBPF 兼容性。

ACPI parser 的宿主测试由 `make test-acpi` 执行。裸机日志记录 QEMU MADT 的 1 个 processor、1 个 IOAPIC、5 个 interrupt override，并记录硬件读取到的 LAPIC/IOAPIC ID、24 条 redirection 和 ISA route `2/1/12`；随后出现 timer Future 与 PS/2 交互事件，证明新路由实际收到了 IRQ。

PCI 枚举器的宿主测试由 `make test-pci` 执行。裸机日志包含 QEMU q35 的设备总数和实际 virtio-blk BDF；当前证据为 `00:03.0`、device ID `1001`，完整 region 校验后的 capability mask `0x1e`（configuration type 1–4）。OVMF 分配的 modern BAR base 为 `0xc000000000`，因此 CR3 证据同时包含跨 PML4 slot 的 7 个 table frame。

virtio layout 测试由 `make test-virtio` 执行。4 项宿主测试包含 read/write/flush descriptor direction。裸机 `SLOPOS-VIRTIO` 证据来自真实 descriptor DMA 与 INTx→waker→Future：queue size 8，root device 报告 524288 sectors 并接受 flush；cache 为 134 hit/85 miss/16 invalidation，并执行 1 个双请求批次；含 PID 1 ELF、桌面配置读取及 active data/metadata/block-allocation/create journal transaction 在内共完成 463 个请求，top half/queue interrupt 计数均为 462。

ext4 parser 测试由 `make test-ext4` 执行。裸机日志证明 4096-byte block、65536 blocks、32 inodes、2 groups、group 0 inode table 37、root extent 39 和 6 个 root entries；superblock/group/inode/directory checksum 均由内核校验。`/sbin/slop-init` 是 inode 23、4848 bytes/two blocks。`multiblock.bin` 是 inode 30，`deep-extent.bin` 是 inode 27；后者从 root index 进入 leaf block 92，验证 extent-block checksum 后读取 logical block 8，并将 logical block 7 的 hole 零填充。inode 28 的两个目录块均经 checksum parser，目标条目在第二块解析为 inode 29。path walker 还从 inode 14 取得 inline target，并在同一父目录解析到 inode 21。

`write-probe.bin` 是 inode 31 / physical block 105。裸机日志证明 `ReadWrite` fd 3 在 offset 123 两次处理 73-byte payload，ext4 层执行两次整块 read-modify-write、两次 flush、两次 cache invalidation 及 flush 后 fd 读回，随后恢复原始全 `P` 内容。固定 metadata 后，e2fsprogs 1.47.2 镜像 SHA-256 为 `738c3d0b09a70540fc529eb48b55511264ac99544d92e48637ca1f5908767737`；启动测试后的 hash 相同且 `e2fsck -fn` 报告 31/32 files、4232/65536 blocks。这只证明已分配数据块的有界原位写，不证明 ext4 metadata/journal 写路径。

JBD2 宿主测试解析 big-endian v2 superblock，并拒绝 truncation、非法 geometry 和未知 feature；round-trip/corruption 测试覆盖 descriptor/data/commit，状态测试覆盖 ext4 recovery-bit CRC32C 恢复与 JBD2 sequence/start 转换。裸机 marker 证明 journal inode 8 的单一 extent 从 physical block 32801 开始，superblock 报告 4096 blocks、first 1、sequence 1、start 0、users 1、零 feature words，UUID 与 ext4 匹配。它不证明 journal clean 或 replay。

第二个裸机 marker 证明 sequence 1 / target block 105 的 descriptor/data/commit records 被写到 32802–32804；descriptor+data 和 commit 分别由 flush 隔开，三块读回一致，之后清零恢复。marker 明确带 `active=false`，因为尚未写 journal state 或 ext4 recovery bit。

第三个 marker 独立证明 ext4 recovery bit/checksum 与 JBD2 sequence 1/start 1 被持久化并读回；普通 ext4 parser 在 active 状态拒绝 mount。清理先归零 journal start，再清 recovery bit，最终宿主 hash/fsck 证明恢复。`transactions=0` 表示它尚未与上一组 records 组合。

第四个 marker 证明真正组合的单块 active data transaction：recovery/start 和 descriptor/data/commit 均跨 flush 持久化，DMA readback 验证此时可 replay；home block 105 checkpoint 后推进 sequence 2/start 0 并清 recovery。测试收尾清 records、恢复全 `P` home block并将 sequence 回卷到 1，因此启动后的 image SHA-256 仍为固定值且 `e2fsck -fn` 通过。

第五个 marker 证明 inode 31 所在 inode-table block 38 也作为 JBD2 home target：sequence 1 transaction 把 size/checksum 更新为 4095/valid，sequence 2 transaction 恢复 4096/valid，最终 journal sequence 为 3。两次 cache 失效后的 inode parser 均接受整块 metadata；测试回卷 sequence 后，固定 image hash 与 `e2fsck -fn` 再次证明完整恢复。

第六个 marker 证明 fd 3 的 append/truncate 与五 tag allocation transaction 同步覆盖 blocks 0/1/33/38/106。descriptor 在 EOF 4096 取得 4096-byte append window；内核把 superblock/group free count 各减一、更新 block bitmap CRC32C 与 descriptor checksum、增长 inode size/i_blocks/extent，并把 node size 扩为 8192、offset 推进到 EOF。新增 logical block 1 经普通 fd read 路径读回全 `G`；第二笔 transaction 释放 block，descriptor truncate 回 4096，五块逐字节恢复。

第七个 marker 证明 VFS create/unlink transaction 同步覆盖 blocks 0/1/36/38/90。全局/group 1 free inode 与 `itable_unused` 由 1→0，inode bitmap CRC、group checksum、inode 32 checksum 和 directory tail checksum 全部重算；正常 path walker 打开 size 0 的 `create-probe`，固定表为它复用读写 fd 3 且 read 返回 EOF。close 后第二笔 transaction 经共享 directory remover 与 inode-bitmap encoder 回到原始五块。最终固定 hash/fsck 排除 inode、目录项或计数泄漏。

两阶段 recovery 证据来自独立 injection/replay 日志。phase 1 marker 明确记录 sequence 1/start 1、五个 targets 0/1/33/38/106、`allocated/grown` 旧状态、`free/original` 新状态与 `after_commit_before_home` 停止点；宿主同时确认 recovery feature、free blocks 61303、bitmap、inode 31 size/blockcount 与全 G data。phase 2 普通 kernel 在任何 ext4 path read 前报告五 tag replay、全部 home readback、next sequence 2、records cleared 和 recovery false，随后从恢复后的 `/sbin/slop-init` 执行 PID 1，并用 sequence 2 继续配置加载及全部 probes；最终 marker 为 498 requests/497 queue interrupts。宿主把五个 crash home 与注入前快照逐块比较，确认 free blocks 61304、block 106 释放并运行五阶段 fsck；脚本最后恢复固定-hash 标准镜像。

VFS 宿主测试由 `make test-vfs` 执行。5 项测试覆盖 path/mount/fd offset、access mode 与 EOF growth。裸机第一个 `SLOPOS-VFS` marker 证明从固定 root path 读出 inode 23 的两块 ELF并通过引导副本比对；namespace marker 再证明 normalized absolute path 经 root mount 解析到 filesystem 1，为 inode 18 分配 fd 3，以 5 个 chunk 读取 76 bytes，并在 offset 7 再读取 11 bytes。关闭后复用 fd 3，以读写模式完成 inode 31 的 73-byte write/read/restore。后续 marker 覆盖 append/truncate 与 create/open/close/unlink；`SLOPOS-CONFIG` markers 还证明同一 root ext4 walker 发现四份配置、发布 generation 1/2 并在非法 CSS 时保留 generation 2。mount/fd table 当前仍只是 block task 局部的固定容量状态。
