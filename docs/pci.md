# PCI enumeration

当前共享 PCI 层是不分配内存的枚举与 capability 解析器；内核在驱动初始化时会进行一个受限 command write。

共享 `slopos-pci` crate：

- 以 `ConfigAccess` trait 隔离 configuration transport；
- 扫描 256 bus × 32 device，并只在 function 0 标记 multifunction 时扫描 function 1–7；
- 保存 BDF、vendor/device、class tuple、command/status、header、subsystem 与 interrupt pin/line；
- 保存六个 type-0 BAR，解析 32-bit/64-bit memory BAR base；
- 对 type-0 header 遍历 capability linked list；
- 以 64-bit visited mask 截断循环或重复 capability；
- 识别 virtio transitional/modern device ID，并校验、记录 common/notify/ISR/device vendor capability 的 BAR、offset、length 和 notify multiplier；
- 使用固定 64-device inventory，容量溢出显式报错。

内核的第一种 transport 是 x86 PCI configuration mechanism 1：对 `0xcf8` 写入 BDF/offset，再从 `0xcfc` 读取 32-bit configuration value。扫描发生在 interrupts disabled 的单核启动阶段，因此当前 port pair 不需要额外锁。驱动启用 device 时只对 command register 做 16-bit write，以免把读回的 write-one-to-clear status 位写回。

`make test-pci` 用伪 config space 验证 multifunction、modern/transitional virtio ID、vendor capability 与循环链。`make test-boot` 在 QEMU q35 上必须实际发现承载 ESP 的 virtio-blk function；当前为 `00:03.0`、ID `1af4:1001`，经完整 region 校验后的 capability mask `0x1e`。

未实现：MCFG/ECAM、bridge-aware resource traversal、BAR sizing/resource allocation、MSI/MSI-X、hotplug 和 power management。virtio 数据路径见 [virtio.md](virtio.md)。
