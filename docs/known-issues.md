# Known issues

- 当前已使用 xAPIC/IOAPIC，但 timer 仍来自 PIT；没有 LAPIC timer、TSC-deadline、MSI/MSI-X、interrupt affinity 或 application processor 启动。
- IDT 已有关键 exception diagnostic gate，但 double fault 仍没有独立 IST，page fault 只能诊断并停止，不能 demand-page。
- executor 固定为三个 task，没有 spawn、task ownership、cancellation 或 timer wheel。
- framebuffer 渲染是全屏重绘，没有 damage tracking、double buffering 或 virtio-gpu。
- 所有窗口和工具都在 ring 0，共享同一地址空间。
- 配置窗口只修改内存主题，不解析或持久化声明式配置。
- `initrd.slp` 仍不是文件系统；独立 ext4 disk 已有 fd 原位数据写、最多八 tag 的 active JBD2 transaction、block/inode allocation、inline extent growth、线性目录 create/unlink 和启动时 replay。scanner 当前只接受零 feature、单个非 wrap transaction；这些 mutation 仍是内核 probe，尚无面向 fd 的 create/grow/truncate、revoke、多 transaction、通用权限或 btrfs。
- 已有 frame allocator、自有 early page table 与 bump heap，但没有回收、用户地址空间、进程、用户态或 syscall。
- 当前页表为 early identity map，使用 2 MiB RWX huge page；尚未施加 W^X、NX、user/supervisor 或细粒度 kernel section 权限。
- 当前内核固定加载到物理 64 MiB；切换自有页表前的最早期初始化仍依赖 OVMF 留下的 identity mapping。
- ACPI 当前解析 RSDP、RSDT/XSDT 和 MADT；尚未解析 MCFG、HPET、FADT，固定容量 parser 上限也不是完整 ACPICA 实现。
- PCI 使用 configuration mechanism 1 并消费 firmware 已分配 BAR；尚未解析 bridge 拓扑或 MCFG，也没有 BAR sizing/resource allocation、MSI/MSI-X 或电源管理。
- virtio-blk 当前有两个固定请求槽，已验证双块 read prefetch、单块 write、flush 和 cache invalidation。仍无通用 descriptor free list、任意生产者并发、discard/write-zeroes、writeback、timeout 或错误恢复。
- bitmap font 只覆盖 UI 使用的 ASCII 子集。
- `capture-desktop.sh` 使用 QEMU PPM；只有安装 `pnmtopng` 时才同时生成 PNG。
