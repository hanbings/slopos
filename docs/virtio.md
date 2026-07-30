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
8. 先安装 vector `0x2b` 的 IOAPIC route，再向 available ring 发布 header/data/status 三 descriptor chain，读取 root disk 的 sector 2–3；
9. 按 capability 的 notify multiplier 写 queue notify；
10. INTx top half 读取 ISR、累计 queue interrupt、wake block task 并发 local APIC EOI；
11. block Future 检查 used index 与 block status，再把 1024-byte payload 交给 ext4 parser。

共享 `slopos-virtio` crate 对 split-ring byte layout、power-of-two queue size 和三 descriptor block-read chain 做宿主单元测试。裸机 `make test-boot` 则验证实际 MMIO、feature negotiation、bus-master DMA、一次 INTx/ISR 和 Future completion；当前 root disk 为 128 MiB，即 262144 个 512-byte sector。

q35 的 slot 3 INTA 映射到 PIRQ H。当前 OVMF 仍把 PIRQ H 路由到 legacy IRQ11，MADT 对 IRQ11 指定 GSI 11、flags 13；内核按这一 firmware route 配置 active-high level entry。请求必须在 entry unmask 之后提交，否则完成边沿可能在接管期间丢失。

限制：只支持 queue 0 的单个只读请求；queue frame 永久占用，没有 descriptor free list。没有 MSI-X、通用 PIRQ/ACPI `_PRT` parser、并发请求、写入、flush、discard、topology、block cache、timeout 或文件系统。
