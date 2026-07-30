# virtio modern block transport

当前内核已在 QEMU q35 的 `1af4:1001` function 上实际使用 modern PCI transport。虽然 device ID 属于 transitional 范围，设备同时提供规范的 modern vendor capabilities，因此驱动不使用 legacy I/O layout。

启动流程：

1. 从 PCI capability 链取得 common、notify、ISR 与 device configuration region；
2. 解析其 BAR，给 OVMF 分配在 `0xc000000000` 的 64-bit MMIO 建立 cache-disabled identity map；
3. 以 16-bit PCI command write 启用 memory space 与 bus mastering，并禁用当前未处理的 INTx；
4. reset device，设置 `ACKNOWLEDGE`/`DRIVER`；
5. 要求 `VIRTIO_F_VERSION_1`，拒绝 `VIRTIO_BLK_F_RO`，并在设备提供时协商 `VIRTIO_BLK_F_FLUSH`，核验 `FEATURES_OK`；
6. 为 queue 0 选择 8-entry split ring，分别分配并清零 descriptor、available、used frame，并为两个请求槽各分配 control/data frame；
7. 写入 queue physical address，启用 queue 和 `DRIVER_OK`；
8. 安装 vector `0x2b` 的 IOAPIC route 后，由 block task 向 available ring 发布 chain；read 的 data descriptor 是 device-writable，write 的 data descriptor 是 device-readable，flush 只含 header/status；两个固定槽分别保留 descriptor `0..2` 与 `3..5`；
9. 按 capability 的 notify multiplier 写 queue notify；
10. INTx top half 读取 ISR、累计 queue interrupt、wake block task 并发 local APIC EOI；
11. transport Future 检查目标 used index 与各槽 block status；独立 fs mount task 可等待一个请求，或一次发布两个 chain 并在二者全部完成后复用槽。

共享 `slopos-virtio` crate 的 4 项宿主测试覆盖 split-ring byte layout、power-of-two queue size、可偏移 block-read chain，以及 device-readable write/data-free flush chain。实现遵守 [OASIS VirtIO 1.3 block device operation](https://docs.oasis-open.org/virtio/virtio/v1.3/virtio-v1.3.html#x1-3080006) 的 request type、512-byte transfer 和 descriptor direction 约束。

文件系统以一个双块 cache prefetch 实际同时发布两个请求；VFS partial-write、JBD2 record/state 和 active data/metadata/allocation transaction probes 均串行使用 write/flush。8-entry cache 最终记录 65 hit/59 miss/11 invalidation；所有 direct journal/state I/O 绕过 cache。裸机 `make test-boot` 验证 clean boot 的 318 个请求由 317 次 INTx/ISR/Future completion 唤醒完成；`make test-journal-replay` 的 recovery boot 则完成 330/329。当前 root disk 为 256 MiB，即 524288 个 512-byte sector。

q35 的 slot 3 INTA 映射到 PIRQ H。当前 OVMF 仍把 PIRQ H 路由到 legacy IRQ11，MADT 对 IRQ11 指定 GSI 11、flags 13；内核按这一 firmware route 配置 active-high level entry。请求必须在 entry unmask 之后提交，否则完成边沿可能在接管期间丢失。

限制：只支持 queue 0、两个固定请求槽，write/flush 仍由单一 block task 串行提交；尚无通用 descriptor free list、任意生产者并发或 backpressure。queue/cache frame 永久占用。没有 MSI-X、通用 PIRQ/ACPI `_PRT` parser、discard、write-zeroes、topology、writeback、timeout 或错误恢复。
