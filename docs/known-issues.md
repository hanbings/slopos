# Known issues

- 当前已使用 xAPIC/IOAPIC，但 timer 仍来自 PIT；没有 LAPIC timer、TSC-deadline、MSI/MSI-X、interrupt affinity 或 application processor 启动。
- IDT 已有关键 exception diagnostic gate，但 double fault 仍没有独立 IST，page fault 只能诊断并停止，不能 demand-page。
- executor 固定为三个 task，没有 spawn、task ownership、cancellation 或 timer wheel。
- framebuffer 渲染是全屏重绘，没有 damage tracking、double buffering 或 virtio-gpu。
- 三个 surface 已使用 niri 式滚动 column layout，但仍全部在 ring 0；没有 Wayland client、workspace/multi-output、floating/tabbed layout、window rule、bind、animation、overview 或 IPC。
- niri KDL、Waybar JSONC 与 swww environment 默认值来自编译时 asset，尚未按 XDG/VFS 路径 live reload。顶部栏 provider 仍固定且没有 module option/CSS/action/per-output。swww 风格状态机只解码两个 embedded P3/PNM，运行在 kernel desktop 内；没有独立 IPC/layer-shell process、任意 VFS image、常见压缩格式、animated image、多 output 或精确帧时序。
- `initrd.slp` 仍不是文件系统；独立 ext4 disk 已有 fd 原位写与严格单块 EOF append/truncate、最多八 tag 的 active JBD2 transaction、block/inode allocation、线性目录 create→fd open→unlink 和启动时 replay。scanner 当前只接受零 feature、单个非 wrap transaction；mutation 仍是启动回归流程，尚无可复用 namespace API/syscall、多块或非连续 growth、revoke、多 transaction、通用权限或 btrfs。
- 已有 frame allocator、自有 early page table、bump heap 和一个同步 PID 1 user address space，但没有回收、process table、thread、scheduler、preemption 或通用 syscall/VFS 接口。
- kernel 仍使用 2 MiB RWX identity huge page；PID 1 的 private page table 已隔离 supervisor kernel map，并区分 user read-only code 与 user-writable stack，但尚未施加 NX、完整 W^X 或细粒度 kernel section 权限。
- 当前内核固定加载到物理 64 MiB；切换自有页表前的最早期初始化仍依赖 OVMF 留下的 identity mapping。
- ACPI 当前解析 RSDP、RSDT/XSDT 和 MADT；尚未解析 MCFG、HPET、FADT，固定容量 parser 上限也不是完整 ACPICA 实现。
- PCI 使用 configuration mechanism 1 并消费 firmware 已分配 BAR；尚未解析 bridge 拓扑或 MCFG，也没有 BAR sizing/resource allocation、MSI/MSI-X 或电源管理。
- virtio-blk 当前有两个固定请求槽，已验证双块 read prefetch、单块 write、flush 和 cache invalidation。仍无通用 descriptor free list、任意生产者并发、discard/write-zeroes、writeback、timeout 或错误恢复。
- bitmap font 只覆盖 UI 使用的 ASCII 子集。
- `capture-desktop.sh` 使用 QEMU PPM；只有安装 `pnmtopng` 时才同时生成 PNG。
