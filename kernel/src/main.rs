// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

mod desktop;
mod font;
mod framebuffer;
mod ps2;
mod serial;

use core::arch::asm;
use core::panic::PanicInfo;
use desktop::Desktop;
use framebuffer::Framebuffer;
use serial::serialln;
use slopos_boot_protocol::{BOOT_INFO_MAGIC, BOOT_INFO_VERSION, BootInfo};

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
    if boot_info.acpi_rsdp == 0 || !valid_rsdp(boot_info.acpi_rsdp) {
        fatal("ACPI RSDP is missing or invalid");
    }
    if boot_info.initrd.base == 0 || boot_info.initrd.size < 17 {
        fatal("bootstrap image is missing");
    }

    serialln(format_args!(
        "SLOPOS-KERNEL: boot info valid memory_descriptors={}",
        boot_info.memory_map.descriptor_count
    ));
    serialln(format_args!(
        "SLOPOS-KERNEL: ACPI RSDP validated at {:#x}",
        boot_info.acpi_rsdp
    ));
    serialln(format_args!(
        "SLOPOS-KERNEL: initrd available base={:#x} bytes={}",
        boot_info.initrd.base, boot_info.initrd.size
    ));

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
            "SLOPOS-INPUT: PS/2 keyboard and mouse enabled"
        ));
    } else {
        serialln(format_args!(
            "SLOPOS-INPUT: keyboard enabled; mouse handshake incomplete"
        ));
    }

    let mut desktop = Desktop::new(framebuffer.width(), framebuffer.height());
    desktop.render(&mut framebuffer);
    serialln(format_args!(
        "SLOPOS-DESKTOP: interactive compositor loop entered windows=3"
    ));
    serialln(format_args!(
        "SLOPOS-DESKTOP: terminal, system monitor, and configuration windows ready"
    ));

    desktop.run(&mut framebuffer, input)
}

fn valid_rsdp(address: u64) -> bool {
    // SAFETY: UEFI identified this address as an ACPI RSDP configuration table.
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, 20) };
    bytes[0..8] == *b"RSD PTR " && bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
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
