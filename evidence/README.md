# Runtime evidence

所有证据均可从源码重新生成，不提交 ESP、ELF、OVMF VARS 或 PPM 等大型生成物。

| 文件 | 生成方式 | 证明范围 |
|---|---|---|
| `serial.log` | `make test-boot` | OVMF/UEFI、ELF、`ExitBootServices`、XSDT/MADT、memory、eBPF、PCI/virtio INTx、ext4 superblock、async 与桌面循环 |
| `uefi-debugcon.log` | `make test-boot` | loader 独立 debugcon 日志 |
| `interaction-serial.log` | `make test-interaction` | PS/2 键盘执行 `STATUS`，鼠标拖动终端 |
| `page-fault-serial.log` | `make test-page-fault` | 自有页表的未映射访问、vector 14、error、RIP、CR2 与 fatal boundary |
| `desktop.png` | `scripts/capture-desktop.sh` | 当前三窗口图形桌面 |
| `terminal-status.png` | `make test-interaction` | 图形终端对键盘命令的实际响应 |
| `window-moved.png` | `make test-interaction` | 鼠标拖动后的窗口位置 |
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
  -serial file:evidence/serial.log
  -debugcon file:evidence/uefi-debugcon.log
  -global isa-debugcon.iobase=0x402
  -display none
  -monitor none
  -no-reboot
```

eBPF 的宿主边界测试由 `make test-ebpf` 执行；裸机证据是 `serial.log` 中的 `SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42`。它只证明文档所列子集，不证明 map、attach point 或 Linux eBPF 兼容性。

ACPI parser 的宿主测试由 `make test-acpi` 执行。裸机日志记录 QEMU MADT 的 1 个 processor、1 个 IOAPIC、5 个 interrupt override，并记录硬件读取到的 LAPIC/IOAPIC ID、24 条 redirection 和 ISA route `2/1/12`；随后出现 timer Future 与 PS/2 交互事件，证明新路由实际收到了 IRQ。

PCI 枚举器的宿主测试由 `make test-pci` 执行。裸机日志包含 QEMU q35 的设备总数和实际 virtio-blk BDF；当前证据为 `00:03.0`、device ID `1001`，完整 region 校验后的 capability mask `0x1e`（configuration type 1–4）。OVMF 分配的 modern BAR base 为 `0xc000000000`，因此 CR3 证据同时包含跨 PML4 slot 的 7 个 table frame。

virtio layout 测试由 `make test-virtio` 执行。4 项宿主测试包含 read/write/flush descriptor direction。裸机 `SLOPOS-VIRTIO` 证据来自真实 descriptor DMA 与 INTx→waker→Future：queue size 8，root device 报告 524288 sectors 并接受 flush；cache 以 62 hit/49 miss 减少重复 lookup，记录 2 次 write invalidation，并实际执行 1 个双请求批次；共完成 54 个 read/write/flush 请求，top half/queue interrupt 计数均为 53。

ext4 parser 测试由 `make test-ext4` 执行。裸机日志证明 4096-byte block、65536 blocks、32 inodes、2 groups、group 0 inode table 37、root extent 39 和 5 个 root entries；superblock/group/inode/directory checksum 均由内核校验。`multiblock.bin` 是 inode 24，`deep-extent.bin` 是 inode 21；后者从 root index 进入 leaf block 85，验证 extent-block checksum 后读取 logical block 8，并将 logical block 7 的 hole 零填充。inode 22 的两个目录块均经 checksum parser，目标条目在第二块解析为 inode 23。path walker 还从 inode 14 取得 inline target，并在同一父目录解析到 inode 17。

`write-probe.bin` 是 inode 25 / physical block 98。裸机日志证明 `ReadWrite` fd 3 在 offset 123 两次处理 73-byte payload，ext4 层执行两次整块 read-modify-write、两次 flush、两次 cache invalidation 及 flush 后 fd 读回，随后恢复原始全 `P` 内容。固定 metadata 后，e2fsprogs 1.47.2 镜像 SHA-256 为 `4aeb38e91e7436b303569e9bd48145e01458dcc513f8db230f20b90a5d4a1fe2`；启动测试后的 hash 相同且 `e2fsck -fn` 通过。这只证明已分配数据块的有界原位写，不证明 ext4 metadata/journal 写路径。

JBD2 宿主测试解析 big-endian v2 superblock，并拒绝 truncation、非法 geometry 和未知 feature；另有 round-trip/corruption 测试覆盖单 target descriptor、data escape/restore 和 commit header。裸机 marker 证明 journal inode 8 的单一 extent 从 physical block 32801 开始，superblock 报告 4096 blocks、first 1、sequence 1、start 0、users 1、零 feature words，UUID 与 ext4 匹配。它不证明 journal clean、磁盘 transaction replay 或 commit。

VFS 宿主测试由 `make test-vfs` 执行。4 项测试覆盖 path/mount/fd offset 与 access mode。裸机 `SLOPOS-VFS` marker 证明 normalized absolute path 经 root mount 解析到 filesystem 1，为 inode 16 分配 fd 3，以 5 个 chunk 读取 76 bytes，并在 offset 7 再读取 11 bytes；关闭后复用 fd 3，以读写模式完成 inode 25 的 73-byte write/read/restore。mount/fd table 当前仍只是 block task 局部的固定容量状态。
