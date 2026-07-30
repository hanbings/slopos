# virtio modern block transport

当前内核已在 QEMU q35 的 `1af4:1001` function 上实际使用 modern PCI transport。虽然 device ID 属于 transitional 范围，设备同时提供规范的 modern vendor capabilities，因此驱动不使用 legacy I/O layout。

启动流程：

1. 从 PCI capability 链取得 common、notify、ISR 与 device configuration region；
2. 解析其 BAR，给 OVMF 分配在 `0xc000000000` 的 64-bit MMIO 建立 cache-disabled identity map；
3. 以 16-bit PCI command write 启用 memory space 与 bus mastering，并禁用当前未处理的 INTx；
4. reset device，设置 `ACKNOWLEDGE`/`DRIVER`；
5. 只协商 `VIRTIO_F_VERSION_1`，核验 `FEATURES_OK`；
6. 为 queue 0 选择 8-entry split ring，分别分配并清零 descriptor、available、used frame；
7. 写入 queue physical address，启用 queue 和 `DRIVER_OK`；
8. 构造 header/data/status 三 descriptor chain，向 available ring 发布 sector-0 read；
9. 按 capability 的 notify multiplier 写 queue notify；
10. 有界等待 used index，核验 block status 与 sector 末尾 `55aa`。

共享 `slopos-virtio` crate 对 split-ring byte layout、power-of-two queue size 和三 descriptor block-read chain 做宿主单元测试。裸机 `make test-boot` 则验证实际 MMIO、feature negotiation、bus-master DMA 与设备完成；当前磁盘为 64 MiB，即 131072 个 512-byte sector。

限制：只支持 queue 0 的单个只读请求；queue frame 永久占用，没有 descriptor free list。INTx 被禁用，used-ring 等待是 polling；没有 MSI-X、IRQ-to-waker、并发请求、写入、flush、discard、topology、block cache 或文件系统。
