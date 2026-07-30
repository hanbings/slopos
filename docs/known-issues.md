# Known issues

- 当前使用兼容 8259 PIC/PIT，而不是目标 APIC/IOAPIC/TSC-deadline。
- IDT 已有关键 exception diagnostic gate，但 double fault 仍没有独立 IST，page fault 只能诊断并停止，不能 demand-page。
- executor 固定为两个 task，没有 spawn、task ownership、cancellation 或 timer wheel。
- framebuffer 渲染是全屏重绘，没有 damage tracking、double buffering 或 virtio-gpu。
- 所有窗口和工具都在 ring 0，共享同一地址空间。
- 配置窗口只修改内存主题，不解析或持久化声明式配置。
- bootstrap image 不是文件系统；ext4/btrfs 尚未实现。
- 已有 frame allocator、自有 early page table 与 bump heap，但没有回收、用户地址空间、进程、用户态或 syscall。
- 当前页表为 early identity map，使用 2 MiB RWX huge page；尚未施加 W^X、NX、user/supervisor 或细粒度 kernel section 权限。
- 当前内核固定加载到物理 64 MiB，并依赖 OVMF 留下的 identity mapping；内核尚未建立自己的页表。
- ACPI 当前只验证 RSDP 1.0 checksum；尚未解析 XSDT/MADT。
- bitmap font 只覆盖 UI 使用的 ASCII 子集。
- `capture-desktop.sh` 使用 QEMU PPM；只有安装 `pnmtopng` 时才同时生成 PNG。
