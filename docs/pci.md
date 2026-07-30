# PCI enumeration

当前 PCI 层是一个不分配内存的只读枚举里程碑。

共享 `slopos-pci` crate：

- 以 `ConfigAccess` trait 隔离 configuration transport；
- 扫描 256 bus × 32 device，并只在 function 0 标记 multifunction 时扫描 function 1–7；
- 保存 BDF、vendor/device、class tuple、command/status、header、subsystem 与 interrupt pin/line；
- 对 type-0 header 遍历 capability linked list；
- 以 64-bit visited mask 截断循环或重复 capability；
- 识别 virtio transitional/modern device ID，并记录 vendor capability 的 configuration type mask；
- 使用固定 64-device inventory，容量溢出显式报错。

内核的第一种 transport 是 x86 PCI configuration mechanism 1：对 `0xcf8` 写入 BDF/offset，再从 `0xcfc` 读取 32-bit configuration value。扫描发生在 interrupts disabled 的单核启动阶段，因此当前 port pair 不需要额外锁。

`make test-pci` 用伪 config space 验证 multifunction、modern/transitional virtio ID、vendor capability 与循环链。`make test-boot` 在 QEMU q35 上必须实际发现承载 ESP 的 virtio-blk function；当前为 `00:03.0`、ID `1af4:1001`，capability mask `0x3e`。

未实现：MCFG/ECAM、bridge-aware resource traversal、BAR sizing/mapping、configuration write、bus mastering、MSI/MSI-X、hotplug、power management，以及任何 virtqueue 数据传输。
