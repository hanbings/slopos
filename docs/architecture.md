# Architecture

本文只描述当前实际存在的代码。规划中的子系统在 [status.md](status.md) 和 [async-kernel.md](async-kernel.md) 中单独标记。

## 启动链

```text
OVMF
  -> EFI/BOOT/BOOTX64.EFI (SlopOS Rust loader)
      -> FAT SimpleFileSystem: /slopos/kernel.elf
      -> FAT SimpleFileSystem: /slopos/initrd.slp
      -> ACPI configuration table
      -> GOP framebuffer
      -> final UEFI memory map
      -> ExitBootServices
  -> ELF entry at physical 0x04000000 (SlopOS Rust kernel)
      -> validate RSDP/XSDT/MADT and discover interrupt controllers
      -> initialize COM1
      -> establish frame allocator, page tables, heap, and eBPF self-test
      -> accept GOP framebuffer ownership
      -> initialize PS/2 keyboard and mouse
      -> interactive early desktop loop
```

加载器不调用 GRUB、Linux 或其他操作系统。它使用 `uefi-raw` 的 ABI 类型，自己实现协议发现、UTF-16 路径、FAT 文件读取、GOP 模式选择、页分配、ELF64 校验与装载、memory map 取得和 `ExitBootServices` 重试。

内核是固定地址 `ET_EXEC` ELF64。链接脚本把它放在 64 MiB；加载器要求每个 `PT_LOAD` 的 virtual address 等于 physical address，先为整个映像分配连续的 `LOADER_CODE` 页，再清零并复制各 segment。入口必须落在已分配映像内。

## BootInfo ABI

`crates/boot-protocol` 是加载器和内核共享的 `no_std` crate。所有结构使用 `#[repr(C)]`，并通过 magic、版本号和结构大小进行校验。当前传递：

- GOP base、size、resolution、stride、pixel format；
- memory map base、总字节数、descriptor size/version/count；
- ACPI RSDP 地址；
- bootstrap image 地址和大小；
- 内核物理范围和入口。

memory map 使用 firmware 返回的 descriptor size，而不是假设 Rust 结构大小。加载器在最后一次所有分配完成后取得 map，并在 `ExitBootServices` 失败时进行一次不分配内存的重试。

## 当前图形与输入

`kernel/src/framebuffer.rs` 直接使用 volatile 32-bit framebuffer store，尊重 GOP stride 和 RGB/BGR 格式。`font.rs` 是项目内原创的 5×7 bitmap glyph 集。

`desktop.rs` 当前是内核态的早期合成 async task。窗口拥有几何、开关状态、类型和 z-order；输入路径提供焦点、标题栏拖动、右下角缩放、关闭、任务栏恢复和键盘命令处理。它没有声称实现 surface IPC、Wayland object、用户态 client 或进程隔离。

`memory.rs` 按 firmware 报告的 descriptor stride 解析 UEFI map，只收集 conventional memory，并提供并发保护的物理 frame/contiguous bump allocator。启动时实际分配一个 frame、volatile 写入、读回并清零。

`paging.rs` 从 frame allocator 建立新的 x86-64 PML4/PDPT/PD，以 2 MiB page identity-map 当前 RAM，并以 cache-disabled 映射覆盖 GOP framebuffer、LAPIC、IOAPIC 和 PCI BAR MMIO，然后写入并读回 CR3。映射器可建立多个 lower-canonical PML4 slot；这对 OVMF 分配在 768 GiB 的 virtio 64-bit BAR 是实际必需的。`heap.rs` 从 contiguous frames 保留 1 MiB，提供 alignment-aware、并发保护的 bump allocation；启动路径实际分配 128 bytes 并验证首尾。

`crates/acpi` 校验 ACPI 1.0/2.0 RSDP、RSDT/XSDT 和 SDT checksum，并解析 MADT 的 local APIC、I/O APIC、processor、local APIC override 与 interrupt-source override。`apic.rs` 通过 MADT 路由把 PIT、keyboard、mouse 送入 IOAPIC，屏蔽 8259，启用 xAPIC 并从 local APIC 发 EOI；QEMU 的 IRQ0 实际按 override 路由到 GSI 2。

`interrupts.rs` 安装自有 GDT/IDT，配置 100 Hz PIT，并为 timer、keyboard、mouse、APIC spurious 及关键 CPU exception 安装 gate。汇编 stub 只保存上下文、对齐栈并调用有界 Rust top half。PS/2 top half 读取一个字节、确认 local APIC 并写入固定 SPSC ring；`desktop` future 负责扫描码和 mouse packet 的复杂解析。独立测试会访问未映射的 1 GiB 地址，实际验证 page-fault vector、error、RIP 和 CR2。

`crates/pci` 通过 `ConfigAccess` trait 把枚举逻辑与硬件访问分离，扫描完整 bus/device/function 空间，识别 multifunction header，以 visited mask 避免 capability 链环，并解码 BAR 与 virtio vendor capability region。内核后端使用 PCI configuration mechanism 1 的 `0xcf8/0xcfc` port，并以 16-bit command write 启用 memory space/bus master 而不误清 status。

`virtio.rs` 走 modern PCI transport，协商 `VIRTIO_F_VERSION_1` 和设备提供的 `VIRTIO_BLK_F_FLUSH`，拒绝 read-only block device，为 queue 0 分配独立 descriptor/available/used frame，并建立两个各有 control/data frame 的请求槽。每个槽保留三个 descriptor；read data 标为 device-writable，write data 保持 device-readable，flush 只链接 header/status。单请求或双请求批次都只在 descriptor 与 available entries 完成后一次发布 index。INTx top half 只读取并清除 ISR、累计计数、wake block task 和 EOI；Future 在下半部等待目标 used index 并检查各槽 status。共享 `crates/virtio` 负责可宿主测试的 split-ring layout 与可偏移 descriptor 构造。

构建产生两个 disk：64 MiB FAT32 ESP 只供 UEFI loader，256 MiB `SLOPOS_ROOT` ext4 image 作为独立 root disk。`virtio.rs` 只负责 transport、DMA ring、IRQ completion 与两个有界 block buffer；`fs.rs` 持有 `Ext4Mount`/`Ext4File` 和 8-entry FIFO cache，cache frame 由物理 allocator 提供。inode table、group descriptor 和重复目录 block 命中时不发 DMA，但 parser/checksum 仍照常执行；两个全 miss 的连续文件块可经一次 available-index 发布成对预取。QEMU 镜像有 2 个 group，inode 21–25 实际走 group 1/inode table 38；inode 25 的已分配数据块用于 fd read-modify-write。隐藏 inode 8 的单一 extent 指向 4096-block journal；内核解析 block 0，并用三个独立 scratch frame 编码 logical blocks 1–3。发布顺序是 descriptor、data、flush、commit、flush，随后逐块读回并清零/flush 恢复；因为尚未更新 journal superblock 和 ext4 `needs_recovery`，这些 records 明确保持 inactive。

`crates/vfs` 是无分配、无标准库的 namespace 状态机：绝对路径最多 16 个 component，mount table 采用最长 component-prefix，fd table 从 3 开始分配并维护 vnode、size、offset 与 read/write access mode。内核把 ext4 注册为 filesystem 1 并挂到 `/`；启动验证通过 fd 3 以五个 chunk 读取 inode 16、seek 到 offset 7 再读 11 bytes，关闭后复用 fd 3 对 inode 25 的 offset 123 写入 73 bytes。当前这些表仍由 block task 局部持有，不是每进程或并发全局对象。

`executor.rs` 当前固定运行 input、timer、block 三个 pinned future，以原子 ready mask 作为 task queue，以 RawWaker 标识 task，并在空闲时执行 race-free `cli` 检查和 `sti; hlt`。它仍缺动态 task arena、timer wheel、cancellation、async lock 和 SMP。

`ebpf` 是与内核分离的 `no_std` crate。它把标准 little-endian 8-byte instruction 解码成固定布局，以前向数据流交集跟踪已初始化寄存器，拒绝 backward jump、越界分支、对 frame pointer 的写入、越界 stack access、未知 helper 和没有可达 `EXIT` 的路径。解释器拥有 11 个 64-bit 寄存器和 512-byte stack；启动路径验证并执行一段 ALU/stack 程序，要求结果为 42。具体指令和未实现边界见 [ebpf.md](ebpf.md)。

## 汇编用途与安全边界

没有独立汇编文件。内联汇编限于：

- `in` / `out`：COM1、QEMU debugcon 和 i8042 PS/2 port；
- `cli`：内核接管后、建立自己的 IDT 前屏蔽 maskable interrupt；
- `pause`：当前早期轮询和 fatal loop 的处理器提示；
- `hlt`：panic 后停止处理器。

调用约定：

- UEFI 入口和 firmware function pointer 使用 `extern "efiapi"`；
- loader 到 kernel 使用 `extern "sysv64"`；
- `BootInfo` 指针放在 SysV 第一个整数参数寄存器。

每个 I/O port wrapper 都是局部 `unsafe`，调用点说明目标平台假设。framebuffer 和 BootInfo 的 raw pointer 在转为引用或写入前均检查范围或依赖加载器独占分配不变量。
