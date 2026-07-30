# Known issues

- 当前使用兼容 8259 PIC/PIT，而不是目标 APIC/IOAPIC/TSC-deadline。
- IDT 当前只安装三个硬件 IRQ gate；异常、page fault 和 double-fault IST 尚未实现。
- executor 固定为两个 task，没有 spawn、task ownership、cancellation 或 timer wheel。
- framebuffer 渲染是全屏重绘，没有 damage tracking、double buffering 或 virtio-gpu。
- 所有窗口和工具都在 ring 0，共享同一地址空间。
- 配置窗口只修改内存主题，不解析或持久化声明式配置。
- bootstrap image 不是文件系统；ext4/btrfs 尚未实现。
- 已有物理 frame allocator，但没有 heap、自有页表、进程、用户态或 syscall。
- 当前内核固定加载到物理 64 MiB，并依赖 OVMF 留下的 identity mapping；内核尚未建立自己的页表。
- ACPI 当前只验证 RSDP 1.0 checksum；尚未解析 XSDT/MADT。
- bitmap font 只覆盖 UI 使用的 ASCII 子集。
- `capture-desktop.sh` 使用 QEMU PPM；只有安装 `pnmtopng` 时才同时生成 PNG。
