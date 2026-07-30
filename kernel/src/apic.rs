// SPDX-License-Identifier: 0BSD

use core::arch::{asm, x86_64::__cpuid};
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};
use slopos_acpi::{IoApic, MadtInfo};

const IA32_APIC_BASE: u32 = 0x1b;
const APIC_GLOBAL_ENABLE: u64 = 1 << 11;
const APIC_X2_MODE: u64 = 1 << 10;
const APIC_BASE_MASK: u64 = 0xffff_f000;
const LAPIC_ID: usize = 0x020;
const LAPIC_TASK_PRIORITY: usize = 0x080;
const LAPIC_EOI: usize = 0x0b0;
const LAPIC_SPURIOUS: usize = 0x0f0;
const LAPIC_LVT_TIMER: usize = 0x320;
const LAPIC_LVT_THERMAL: usize = 0x330;
const LAPIC_LVT_PERFORMANCE: usize = 0x340;
const LAPIC_LVT_LINT0: usize = 0x350;
const LAPIC_LVT_LINT1: usize = 0x360;
const LAPIC_LVT_ERROR: usize = 0x370;
const LVT_MASKED: u32 = 1 << 16;
const SPURIOUS_VECTOR: u32 = 0xff;
const IOAPIC_REGISTER_SELECT: usize = 0x00;
const IOAPIC_WINDOW: usize = 0x10;
const IOAPIC_ID: u32 = 0x00;
const IOAPIC_VERSION: u32 = 0x01;
const IOAPIC_REDIRECTION_BASE: u32 = 0x10;

static LOCAL_APIC_BASE: AtomicU64 = AtomicU64::new(0);

pub struct ApicStats {
    pub local_id: u8,
    pub io_id: u8,
    pub redirection_entries: u16,
    pub timer_gsi: u32,
    pub keyboard_gsi: u32,
    pub mouse_gsi: u32,
    pub virtio_gsi: u32,
}

pub fn initialize(madt: &MadtInfo, virtio_interrupt_line: u8) -> ApicStats {
    // SAFETY: CPUID is always available in the x86-64 execution mode.
    let features = unsafe { __cpuid(1) };
    if features.edx & (1 << 9) == 0 {
        crate::fatal("processor does not expose xAPIC");
    }
    if madt.local_apic_address & 0xfff != 0
        || madt.local_apic_address == 0
        || madt.local_apic_address > APIC_BASE_MASK
    {
        crate::fatal("MADT local APIC address is invalid");
    }

    // SAFETY: ring-0 MSR access selects the MADT-advertised, page-mapped xAPIC
    // MMIO base and leaves the BSP flag and unrelated architectural bits intact.
    unsafe {
        let current = read_msr(IA32_APIC_BASE);
        let configured = (current & !APIC_BASE_MASK & !APIC_X2_MODE)
            | (madt.local_apic_address & APIC_BASE_MASK)
            | APIC_GLOBAL_ENABLE;
        write_msr(IA32_APIC_BASE, configured);
    }
    LOCAL_APIC_BASE.store(madt.local_apic_address, Ordering::Release);

    write_local(LAPIC_TASK_PRIORITY, 0);
    for register in [
        LAPIC_LVT_TIMER,
        LAPIC_LVT_THERMAL,
        LAPIC_LVT_PERFORMANCE,
        LAPIC_LVT_LINT0,
        LAPIC_LVT_LINT1,
        LAPIC_LVT_ERROR,
    ] {
        write_local(register, LVT_MASKED);
    }
    write_local(LAPIC_SPURIOUS, (1 << 8) | SPURIOUS_VECTOR);
    let local_id = (read_local(LAPIC_ID) >> 24) as u8;

    // The legacy PIC must not also deliver the same ISA lines once the IOAPIC
    // redirection table is active.
    // SAFETY: these are the fixed 8259 data ports on the PC platform.
    unsafe {
        outb(0x21, 0xff);
        outb(0xa1, 0xff);
    }

    for io_apic in madt.io_apics() {
        let maximum = io_apic_maximum_entry(*io_apic);
        for entry in 0..=maximum {
            write_redirection(*io_apic, entry, 0, 1 << 16);
        }
    }

    let (timer_gsi, timer_flags) = madt.isa_route(0);
    let (keyboard_gsi, keyboard_flags) = madt.isa_route(1);
    let (mouse_gsi, mouse_flags) = madt.isa_route(12);
    route(madt, local_id, timer_gsi, timer_flags, 0x20);
    route(madt, local_id, keyboard_gsi, keyboard_flags, 0x21);
    route(madt, local_id, mouse_gsi, mouse_flags, 0x2c);
    if virtio_interrupt_line == 0 || virtio_interrupt_line == 0xff {
        crate::fatal("virtio PCI INTx line is invalid");
    }
    let (virtio_gsi, virtio_flags) = madt.isa_route(virtio_interrupt_line);
    route(madt, local_id, virtio_gsi, virtio_flags, 0x2b);

    let first = *madt
        .io_apics()
        .first()
        .unwrap_or_else(|| crate::fatal("MADT contains no IOAPIC"));
    ApicStats {
        local_id,
        io_id: (read_io(first, IOAPIC_ID) >> 24) as u8,
        redirection_entries: u16::from(io_apic_maximum_entry(first)) + 1,
        timer_gsi,
        keyboard_gsi,
        mouse_gsi,
        virtio_gsi,
    }
}

pub fn end_of_interrupt() {
    if LOCAL_APIC_BASE.load(Ordering::Acquire) != 0 {
        write_local(LAPIC_EOI, 0);
    }
}

fn route(madt: &MadtInfo, destination: u8, gsi: u32, flags: u16, vector: u8) {
    let io_apic = madt
        .io_apics()
        .iter()
        .copied()
        .find(|controller| {
            let maximum = u32::from(io_apic_maximum_entry(*controller));
            gsi >= controller.global_interrupt_base
                && gsi <= controller.global_interrupt_base.saturating_add(maximum)
        })
        .unwrap_or_else(|| crate::fatal("no IOAPIC covers an ISA interrupt"));
    let entry = (gsi - io_apic.global_interrupt_base) as u8;
    let polarity = flags & 0b11;
    let trigger = (flags >> 2) & 0b11;
    let mut low = u32::from(vector);
    match polarity {
        0 | 1 => {}
        3 => low |= 1 << 13,
        _ => crate::fatal("MADT interrupt polarity is reserved"),
    }
    match trigger {
        0 | 1 => {}
        3 => low |= 1 << 15,
        _ => crate::fatal("MADT interrupt trigger mode is reserved"),
    }
    write_redirection(io_apic, entry, u32::from(destination) << 24, low);
}

fn io_apic_maximum_entry(io_apic: IoApic) -> u8 {
    ((read_io(io_apic, IOAPIC_VERSION) >> 16) & 0xff) as u8
}

fn write_redirection(io_apic: IoApic, entry: u8, high: u32, low: u32) {
    let register = IOAPIC_REDIRECTION_BASE + u32::from(entry) * 2;
    write_io(io_apic, register, low | (1 << 16));
    write_io(io_apic, register + 1, high);
    write_io(io_apic, register, low);
}

fn read_local(offset: usize) -> u32 {
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    if base == 0 {
        crate::fatal("local APIC used before initialization");
    }
    // SAFETY: paging identity-mapped the MADT LAPIC page as uncached MMIO.
    unsafe { ptr::read_volatile((base as usize + offset) as *const u32) }
}

fn write_local(offset: usize, value: u32) {
    let base = LOCAL_APIC_BASE.load(Ordering::Acquire);
    if base == 0 {
        crate::fatal("local APIC used before initialization");
    }
    // SAFETY: paging identity-mapped the MADT LAPIC page as uncached MMIO.
    unsafe { ptr::write_volatile((base as usize + offset) as *mut u32, value) };
}

fn read_io(io_apic: IoApic, register: u32) -> u32 {
    let base = io_apic.address as usize;
    // SAFETY: paging identity-mapped every MADT IOAPIC page as uncached MMIO;
    // IOREGSEL and IOWIN are accessed with the mandated volatile sequence.
    unsafe {
        ptr::write_volatile((base + IOAPIC_REGISTER_SELECT) as *mut u32, register);
        ptr::read_volatile((base + IOAPIC_WINDOW) as *const u32)
    }
}

fn write_io(io_apic: IoApic, register: u32, value: u32) {
    let base = io_apic.address as usize;
    // SAFETY: same serialized IOAPIC register-window protocol as read_io.
    unsafe {
        ptr::write_volatile((base + IOAPIC_REGISTER_SELECT) as *mut u32, register);
        ptr::write_volatile((base + IOAPIC_WINDOW) as *mut u32, value);
    }
}

unsafe fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: caller is ring 0 and supplies an architectural MSR.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        )
    };
    (u64::from(high) << 32) | u64::from(low)
}

unsafe fn write_msr(msr: u32, value: u64) {
    // SAFETY: caller validated the architectural APIC base value.
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        )
    };
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: caller selects a fixed PC interrupt-controller port.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        )
    };
}
