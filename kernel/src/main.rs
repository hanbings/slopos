// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

mod acpi;
mod apic;
mod desktop;
mod desktop_config;
mod desktop_service;
mod executor;
mod font;
mod framebuffer;
mod fs;
mod heap;
mod interrupts;
mod memory;
mod paging;
mod pci;
mod process;
mod ps2;
mod serial;
mod timer;
mod virtio;

use core::arch::asm;
use core::panic::PanicInfo;
use desktop::Desktop;
use framebuffer::Framebuffer;
use serial::serialln;
use slopos_acpi::{MAX_IO_APICS, RootKind};
use slopos_boot_protocol::{BOOT_INFO_MAGIC, BOOT_INFO_VERSION, BootInfo};
use slopos_ebpf::{Instruction, NoHelpers};

/// SlopOS ELF entry point invoked by the UEFI loader.
///
/// # Safety
///
/// `boot_info_pointer` must refer to a live, initialized `BootInfo` allocation
/// created by the matching SlopOS boot-protocol version. All pointed-to ranges
/// must remain valid after UEFI boot services have exited.
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text._start")]
pub unsafe extern "sysv64" fn _start(boot_info_pointer: *const BootInfo) -> ! {
    // SAFETY: maskable interrupts remain disabled until SlopOS installs its own IDT.
    unsafe { asm!("cli", options(nomem, nostack, preserves_flags)) };
    serial::init();
    serialln(format_args!("SLOPOS-KERNEL: entry reached"));

    if boot_info_pointer.is_null() {
        fatal("null boot information pointer");
    }
    // SAFETY: the UEFI loader passes a page-aligned, initialized BootInfo allocation.
    let boot_info = unsafe { *boot_info_pointer };
    if boot_info.magic != BOOT_INFO_MAGIC {
        fatal("boot information magic mismatch");
    }
    if boot_info.version != BOOT_INFO_VERSION
        || boot_info.struct_size as usize != core::mem::size_of::<BootInfo>()
    {
        fatal("unsupported boot information version");
    }
    if boot_info.memory_map.base == 0
        || boot_info.memory_map.descriptor_count == 0
        || boot_info.memory_map.descriptor_size == 0
    {
        fatal("UEFI memory map is missing");
    }
    let platform =
        acpi::discover(boot_info.acpi_rsdp).unwrap_or_else(|| fatal("ACPI MADT is unavailable"));
    if boot_info.initrd.base == 0 || boot_info.initrd.size < 17 {
        fatal("bootstrap image is missing");
    }
    if boot_info.user_image.base == 0
        || boot_info.user_image.size < slopos_elf::ELF64_HEADER_SIZE as u64
        || boot_info.user_image.size > 4 * 1024 * 1024
        || boot_info
            .user_image
            .base
            .checked_add(boot_info.user_image.size)
            .is_none()
    {
        fatal("user ELF image is missing or invalid");
    }

    serialln(format_args!(
        "SLOPOS-KERNEL: boot info valid memory_descriptors={}",
        boot_info.memory_map.descriptor_count
    ));
    let root_name = match platform.root_kind {
        RootKind::Rsdt => "RSDT",
        RootKind::Xsdt => "XSDT",
    };
    serialln(format_args!(
        "SLOPOS-ACPI: {root_name} MADT validated cpus={} lapic={:#x} ioapics={} overrides={} irq11={}/{}",
        platform.madt.enabled_processors,
        platform.madt.local_apic_address,
        platform.madt.io_apics().len(),
        platform.madt.interrupt_overrides().len(),
        platform.madt.isa_route(11).0,
        platform.madt.isa_route(11).1
    ));
    serialln(format_args!(
        "SLOPOS-KERNEL: initrd available base={:#x} bytes={}",
        boot_info.initrd.base, boot_info.initrd.size
    ));
    serialln(format_args!(
        "SLOPOS-KERNEL: user ELF available base={:#x} bytes={}",
        boot_info.user_image.base, boot_info.user_image.size
    ));

    let pci_inventory = pci::discover();
    if pci_inventory.overflowed {
        fatal("PCI inventory capacity exhausted");
    }
    let virtio_count = pci_inventory.virtio_devices().count();
    let mut virtio_blocks = pci_inventory.virtio_blocks();
    let esp_block = *virtio_blocks
        .next()
        .unwrap_or_else(|| fatal("QEMU ESP block device was not enumerated"));
    let root_block = *virtio_blocks
        .next()
        .unwrap_or_else(|| fatal("QEMU ext4 root block device was not enumerated"));
    let virtio_block = root_block;
    let virtio_common = virtio_block
        .virtio_region(1)
        .unwrap_or_else(|| fatal("virtio common configuration is missing"));
    let virtio_notify = virtio_block
        .virtio_region(2)
        .unwrap_or_else(|| fatal("virtio notify configuration is missing"));
    let virtio_isr = virtio_block
        .virtio_region(3)
        .unwrap_or_else(|| fatal("virtio ISR configuration is missing"));
    let virtio_device = virtio_block
        .virtio_region(4)
        .unwrap_or_else(|| fatal("virtio device configuration is missing"));
    serialln(format_args!(
        "SLOPOS-PCI: mechanism1 devices={} virtio={} esp={:02x}:{:02x}.{} root={:02x}:{:02x}.{} id={:04x} caps={:#x} irq={}",
        pci_inventory.len(),
        virtio_count,
        esp_block.address.bus,
        esp_block.address.device,
        esp_block.address.function,
        root_block.address.bus,
        root_block.address.device,
        root_block.address.function,
        virtio_block.device_id,
        virtio_block.virtio_capability_mask,
        virtio_block.interrupt_line
    ));
    serialln(format_args!(
        "SLOPOS-PCI: bars={:08x}/{:08x}/{:08x}/{:08x}/{:08x}/{:08x} regions={:#x}/{:#x}/{:#x}/{:#x}",
        virtio_block.bars[0],
        virtio_block.bars[1],
        virtio_block.bars[2],
        virtio_block.bars[3],
        virtio_block.bars[4],
        virtio_block.bars[5],
        virtio_common.base,
        virtio_notify.base,
        virtio_isr.base,
        virtio_device.base
    ));

    let memory_stats = memory::initialize(boot_info.memory_map);
    let probe_frame =
        memory::allocate_frame().unwrap_or_else(|| fatal("no physical frame available"));
    // SAFETY: the allocator returned an exclusive conventional-memory page and
    // this early address space still identity-maps RAM inherited from OVMF.
    unsafe {
        let probe = probe_frame as *mut u64;
        probe.write_volatile(0x534c_4f50_4d45_4d21);
        if probe.read_volatile() != 0x534c_4f50_4d45_4d21 {
            fatal("physical frame readback failed");
        }
        probe.write_volatile(0);
    }
    serialln(format_args!(
        "SLOPOS-MM: frame allocator initialized regions={} free_frames={} probe={:#x}",
        memory_stats.conventional_regions, memory_stats.free_frames, probe_frame
    ));

    let mut mmio_ranges = [paging::MmioRange { base: 0, size: 0 }; MAX_IO_APICS + 5];
    mmio_ranges[0] = paging::MmioRange {
        base: platform.madt.local_apic_address,
        size: 4096,
    };
    for (index, io_apic) in platform.madt.io_apics().iter().enumerate() {
        mmio_ranges[index + 1] = paging::MmioRange {
            base: u64::from(io_apic.address),
            size: 4096,
        };
    }
    let interrupt_range_count = platform.madt.io_apics().len() + 1;
    for (index, region) in [virtio_common, virtio_notify, virtio_isr, virtio_device]
        .iter()
        .enumerate()
    {
        mmio_ranges[interrupt_range_count + index] = paging::MmioRange {
            base: region.base,
            size: u64::from(region.length),
        };
    }
    let paging_stats = paging::install(
        boot_info.framebuffer,
        &mmio_ranges[..interrupt_range_count + 4],
    );
    serialln(format_args!(
        "SLOPOS-MM: CR3 switched root={:#x} table_frames={} huge_pages={}",
        paging_stats.pml4, paging_stats.page_table_frames, paging_stats.huge_pages
    ));

    let heap_stats = heap::initialize();
    let heap_probe =
        heap::allocate(128, 64).unwrap_or_else(|| fatal("kernel heap probe allocation failed"));
    // SAFETY: the heap returned an exclusive 128-byte allocation.
    unsafe {
        core::ptr::write_bytes(heap_probe.as_ptr(), 0xa5, 128);
        if heap_probe.as_ptr().read_volatile() != 0xa5
            || heap_probe.as_ptr().add(127).read_volatile() != 0xa5
        {
            fatal("kernel heap allocation readback failed");
        }
    }
    serialln(format_args!(
        "SLOPOS-MM: kernel heap initialized base={:#x} bytes={} probe={:#x}",
        heap_stats.base,
        heap_stats.size,
        heap_probe.as_ptr() as usize
    ));

    verify_ebpf_runtime();

    let block_device = virtio::initialize_block(
        virtio_block,
        virtio_common,
        virtio_notify,
        virtio_isr,
        virtio_device,
    );

    let mut framebuffer = match Framebuffer::new(boot_info.framebuffer) {
        Some(framebuffer) => framebuffer,
        None => fatal("GOP framebuffer information is invalid"),
    };
    serialln(format_args!(
        "SLOPOS-KERNEL: framebuffer ownership accepted {}x{}",
        framebuffer.width(),
        framebuffer.height()
    ));

    let input = ps2::Controller::initialize();
    if input.mouse_present() {
        serialln(format_args!(
            "SLOPOS-INPUT: PS/2 keyboard and mouse IRQ queue armed"
        ));
    } else {
        serialln(format_args!(
            "SLOPOS-INPUT: keyboard enabled; mouse handshake incomplete"
        ));
    }
    interrupts::initialize(&platform.madt, virtio_block.interrupt_line);
    let user_image_size = usize::try_from(boot_info.user_image.size)
        .unwrap_or_else(|_| fatal("user ELF size exceeds address space"));
    // SAFETY: the UEFI loader owns this allocation, BootInfo bounds were
    // validated above, and LOADER_DATA remains identity-mapped after exit.
    let user_image = unsafe {
        core::slice::from_raw_parts(boot_info.user_image.base as *const u8, user_image_size)
    };
    let mut desktop = Desktop::new(framebuffer.width(), framebuffer.height());
    desktop.render(&mut framebuffer);
    serialln(format_args!(
        "SLOPOS-DESKTOP: interactive compositor loop entered windows=3"
    ));
    serialln(format_args!(
        "SLOPOS-DESKTOP: terminal, system monitor, and configuration windows ready"
    ));

    executor::run(
        desktop.run(&mut framebuffer, input),
        timer::diagnostics_task(),
        fs::mount_task(block_device, user_image),
    )
}

fn verify_ebpf_runtime() {
    const PROGRAM: [Instruction; 5] = [
        Instruction::new(0xb7, 2, 0, 0, 20),
        Instruction::new(0x07, 2, 0, 0, 22),
        Instruction::new(0x7b, 10, 2, -8, 0),
        Instruction::new(0x79, 0, 10, -8, 0),
        Instruction::new(0x95, 0, 0, 0, 0),
    ];

    let verified = match slopos_ebpf::verify(&PROGRAM, &[]) {
        Ok(program) => program,
        Err(_) => fatal("built-in eBPF program failed verification"),
    };
    let result = match slopos_ebpf::execute(&verified, &mut NoHelpers, 0) {
        Ok(value) => value,
        Err(_) => fatal("built-in eBPF program failed execution"),
    };
    if result != 42 {
        fatal("built-in eBPF program returned the wrong value");
    }
    serialln(format_args!(
        "SLOPOS-EBPF: verifier accepted instructions={} interpreter_result={result}",
        verified.len()
    ));
}

fn fatal(message: &str) -> ! {
    serialln(format_args!("SLOPOS-KERNEL: FATAL {message}"));
    loop {
        // SAFETY: interrupts are intentionally disabled on this fatal path.
        unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    serialln(format_args!("SLOPOS-KERNEL: PANIC {info}"));
    loop {
        // SAFETY: a panic cannot safely resume execution.
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}
