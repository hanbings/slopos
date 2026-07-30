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
      -> validate BootInfo and ACPI RSDP
      -> initialize COM1
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

`paging.rs` 从 frame allocator 建立新的 x86-64 PML4/PDPT/PD，以 2 MiB page identity-map 当前 RAM 和 GOP framebuffer，然后写入并读回 CR3。`heap.rs` 从 contiguous frames 保留 1 MiB，提供 alignment-aware、并发保护的 bump allocation；启动路径实际分配 128 bytes 并验证首尾。

`interrupts.rs` 安装自有 GDT/IDT，把 8259 PIC remap 到 `0x20`/`0x28`，配置 100 Hz PIT，并为 timer、keyboard、mouse 及关键 CPU exception 安装 gate。汇编 stub 只保存上下文、对齐栈并调用有界 Rust top half。PS/2 top half 读取一个字节、确认 PIC 并写入固定 SPSC ring；`desktop` future 负责扫描码和 mouse packet 的复杂解析。独立测试会访问未映射的 1 GiB 地址，实际验证 page-fault vector、error、RIP 和 CR2。

`executor.rs` 当前固定运行两个 pinned future，以原子 ready mask 作为 task queue，以 RawWaker 标识 task，并在空闲时执行 race-free `cli` 检查和 `sti; hlt`。`timer.rs` 的 future 由 PIT tick 唤醒。它仍缺动态 task arena、timer wheel、cancellation、async lock 和 SMP。

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
