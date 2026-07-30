# Known issues

- 当前已使用 xAPIC/IOAPIC，但 timer 仍来自 PIT；没有 LAPIC timer、TSC-deadline、MSI/MSI-X、interrupt affinity 或 application processor 启动。
- IDT 已有关键 exception diagnostic gate，但 double fault 仍没有独立 IST，page fault 只能诊断并停止，不能 demand-page。
- executor 固定为两个 task，没有 spawn、task ownership、cancellation 或 timer wheel。
- framebuffer 渲染是全屏重绘，没有 damage tracking、double buffering 或 virtio-gpu。
- 所有窗口和工具都在 ring 0，共享同一地址空间。
- 配置窗口只修改内存主题，不解析或持久化声明式配置。
- `initrd.slp` 仍不是文件系统；独立 ext4 disk 已有 kernel-internal read-only mount/file API，能跨 group 定位 inode、走 depth-0 inline extent、线性单块目录并读取多块 regular file，但尚无 deep extent/htree/symlink/global VFS namespace/write/journal；btrfs 未实现。
- 已有 frame allocator、自有 early page table 与 bump heap，但没有回收、用户地址空间、进程、用户态或 syscall。
- 当前页表为 early identity map，使用 2 MiB RWX huge page；尚未施加 W^X、NX、user/supervisor 或细粒度 kernel section 权限。
- 当前内核固定加载到物理 64 MiB；切换自有页表前的最早期初始化仍依赖 OVMF 留下的 identity mapping。
- ACPI 当前解析 RSDP、RSDT/XSDT 和 MADT；尚未解析 MCFG、HPET、FADT，固定容量 parser 上限也不是完整 ACPICA 实现。
- PCI 使用 configuration mechanism 1 并消费 firmware 已分配 BAR；尚未解析 bridge 拓扑或 MCFG，也没有 BAR sizing/resource allocation、MSI/MSI-X 或电源管理。
- virtio-blk 当前有两个固定请求槽，已通过一次双块 cache prefetch 验证两个同时在途的 descriptor chain；8-entry FIFO read cache 没有失效/回收/writeback。仍无通用 descriptor free list、任意生产者并发、写入、flush/discard、timeout 或错误恢复。
- bitmap font 只覆盖 UI 使用的 ASCII 子集。
- `capture-desktop.sh` 使用 QEMU PPM；只有安装 `pnmtopng` 时才同时生成 PNG。
