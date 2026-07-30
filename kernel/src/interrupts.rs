// SPDX-License-Identifier: 0BSD

use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::mem::size_of;
use slopos_acpi::MadtInfo;

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const TASK_STATE_SELECTOR: u16 = 0x28;
const DIVIDE_ERROR_VECTOR: usize = 0;
const INVALID_OPCODE_VECTOR: usize = 6;
const DOUBLE_FAULT_VECTOR: usize = 8;
const GENERAL_PROTECTION_VECTOR: usize = 13;
const PAGE_FAULT_VECTOR: usize = 14;
const TIMER_VECTOR: usize = 0x20;
const KEYBOARD_VECTOR: usize = 0x21;
const VIRTIO_VECTOR: usize = 0x2b;
const MOUSE_VECTOR: usize = 0x2c;
const SPURIOUS_VECTOR: usize = 0xff;
const TSS_STACK_PAGES: usize = 4;

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[derive(Clone, Copy)]
#[repr(C, packed)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        attributes: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt_gate(handler: u64) -> Self {
        Self {
            offset_low: handler as u16,
            selector: KERNEL_CODE_SELECTOR,
            ist: 0,
            attributes: 0x8e,
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

struct IdtStorage(UnsafeCell<[IdtEntry; 256]>);

// IDT mutation only occurs once with interrupts disabled, before it is loaded.
unsafe impl Sync for IdtStorage {}

static IDT: IdtStorage = IdtStorage(UnsafeCell::new([IdtEntry::MISSING; 256]));

struct GdtStorage(UnsafeCell<[u64; 7]>);

// GDT mutation only occurs during single-core early initialization.
unsafe impl Sync for GdtStorage {}

static GDT: GdtStorage = GdtStorage(UnsafeCell::new([
    0,
    0x00af_9a00_0000_ffff,
    0x00cf_9200_0000_ffff,
    0x00cf_f200_0000_ffff,
    0x00af_fa00_0000_ffff,
    0,
    0,
]));

#[repr(C, packed)]
struct TaskStateSegment {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist1: u64,
    ist2: u64,
    ist3: u64,
    ist4: u64,
    ist5: u64,
    ist6: u64,
    ist7: u64,
    reserved2: u64,
    reserved3: u16,
    io_map_offset: u16,
}

struct TssStorage(UnsafeCell<TaskStateSegment>);

// TSS mutation only occurs before LTR; the CPU reads it thereafter.
unsafe impl Sync for TssStorage {}

static TSS: TssStorage = TssStorage(UnsafeCell::new(TaskStateSegment {
    reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved1: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    reserved2: 0,
    reserved3: 0,
    io_map_offset: 0,
}));

pub fn initialize(madt: &MadtInfo, virtio_interrupt_line: u8) {
    // SAFETY: early kernel initialization runs once with interrupts disabled.
    unsafe {
        load_gdt();
        install_idt();
        configure_pit();
    }
    let stats = crate::apic::initialize(madt, virtio_interrupt_line);
    crate::serial::serialln(format_args!(
        "SLOPOS-INTERRUPT: GDT IDT LAPIC IOAPIC PIT initialized timer_hz=100 lapic_id={} ioapic_id={} redirections={} routes={}/{}/{}/{}",
        stats.local_id,
        stats.io_id,
        stats.redirection_entries,
        stats.timer_gsi,
        stats.keyboard_gsi,
        stats.mouse_gsi,
        stats.virtio_gsi
    ));
}

pub fn enable() {
    // SAFETY: initialize installed every unmasked IRQ gate before this call.
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

pub fn trigger_page_fault() -> ! {
    crate::serial::serialln(format_args!(
        "SLOPOS-EXCEPTION: injecting page fault at 0x40000000"
    ));
    // SAFETY: this address is intentionally absent from SlopOS's new page
    // tables. The read validates the installed vector-14 diagnostic path.
    unsafe {
        core::ptr::read_volatile(0x4000_0000 as *const u64);
    }
    crate::fatal("page-fault injection unexpectedly returned")
}

unsafe fn load_gdt() {
    if size_of::<TaskStateSegment>() != 104 {
        crate::fatal("x86-64 task state segment layout mismatch");
    }
    let stack_base = crate::memory::allocate_contiguous(TSS_STACK_PAGES)
        .unwrap_or_else(|| crate::fatal("cannot allocate ring-0 privilege stack"));
    let stack_top = stack_base + (TSS_STACK_PAGES * 4096) as u64;
    let tss = TSS.0.get();
    // SAFETY: this is exclusive early initialization and packed fields are
    // written through raw pointers with unaligned stores.
    unsafe {
        core::ptr::addr_of_mut!((*tss).rsp0).write_unaligned(stack_top);
        core::ptr::addr_of_mut!((*tss).io_map_offset)
            .write_unaligned(size_of::<TaskStateSegment>() as u16);
    }
    let tss_base = tss as u64;
    let tss_limit = (size_of::<TaskStateSegment>() - 1) as u64;
    let tss_low = (tss_limit & 0xffff)
        | ((tss_base & 0x00ff_ffff) << 16)
        | (0x89 << 40)
        | (((tss_limit >> 16) & 0xf) << 48)
        | ((tss_base & 0xff00_0000) << 32);
    let entries = GDT.0.get();
    // SAFETY: entries 5/6 are reserved for this live static TSS.
    unsafe {
        (*entries)[5] = tss_low;
        (*entries)[6] = tss_base >> 32;
    }
    let pointer = DescriptorTablePointer {
        limit: (size_of::<[u64; 7]>() - 1) as u16,
        base: entries as u64,
    };
    // SAFETY: the GDT contains kernel/user code/data and a live 64-bit TSS.
    unsafe {
        asm!(
            "lgdt [{pointer}]",
            "push 0x08",
            "lea rax, [rip + 2f]",
            "push rax",
            "retfq",
            "2:",
            "mov ax, 0x10",
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            "xor eax, eax",
            "mov fs, ax",
            "mov gs, ax",
            "mov ax, {tss_selector}",
            "ltr ax",
            pointer = in(reg) &pointer,
            tss_selector = const TASK_STATE_SELECTOR,
            out("rax") _,
            options(preserves_flags)
        )
    };
}

unsafe fn install_idt() {
    let entries = IDT.0.get();
    // SAFETY: exclusive early initialization; these symbols are valid interrupt stubs.
    unsafe {
        (*entries)[DIVIDE_ERROR_VECTOR] =
            IdtEntry::interrupt_gate(slopos_divide_error as usize as u64);
        (*entries)[INVALID_OPCODE_VECTOR] =
            IdtEntry::interrupt_gate(slopos_invalid_opcode as usize as u64);
        (*entries)[DOUBLE_FAULT_VECTOR] =
            IdtEntry::interrupt_gate(slopos_double_fault as usize as u64);
        (*entries)[GENERAL_PROTECTION_VECTOR] =
            IdtEntry::interrupt_gate(slopos_general_protection as usize as u64);
        (*entries)[PAGE_FAULT_VECTOR] = IdtEntry::interrupt_gate(slopos_page_fault as usize as u64);
        (*entries)[TIMER_VECTOR] = IdtEntry::interrupt_gate(slopos_timer_interrupt as usize as u64);
        (*entries)[KEYBOARD_VECTOR] =
            IdtEntry::interrupt_gate(slopos_keyboard_interrupt as usize as u64);
        (*entries)[VIRTIO_VECTOR] =
            IdtEntry::interrupt_gate(slopos_virtio_interrupt as usize as u64);
        (*entries)[MOUSE_VECTOR] = IdtEntry::interrupt_gate(slopos_mouse_interrupt as usize as u64);
        (*entries)[SPURIOUS_VECTOR] =
            IdtEntry::interrupt_gate(slopos_spurious_interrupt as usize as u64);
    }
    let pointer = DescriptorTablePointer {
        limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: entries as u64,
    };
    // SAFETY: pointer covers the static IDT for the lifetime of the kernel.
    unsafe { asm!("lidt [{pointer}]", pointer = in(reg) &pointer, options(readonly, nostack)) };
}

unsafe fn configure_pit() {
    const DIVISOR: u16 = 11_932;
    // PIT channel 0, low/high byte, square-wave generator.
    unsafe {
        outb(0x43, 0x36);
        outb(0x40, DIVISOR as u8);
        outb(0x40, (DIVISOR >> 8) as u8);
    }
}

#[unsafe(no_mangle)]
extern "C" fn slopos_timer_handler() {
    crate::timer::interrupt_tick();
    crate::apic::end_of_interrupt();
}

#[unsafe(no_mangle)]
extern "C" fn slopos_keyboard_handler() {
    consume_ps2_irq();
    crate::apic::end_of_interrupt();
}

#[unsafe(no_mangle)]
extern "C" fn slopos_mouse_handler() {
    consume_ps2_irq();
    crate::apic::end_of_interrupt();
}

#[unsafe(no_mangle)]
extern "C" fn slopos_virtio_handler() {
    crate::virtio::interrupt_top_half();
    crate::apic::end_of_interrupt();
}

fn consume_ps2_irq() {
    // SAFETY: IRQ1/IRQ12 indicate data may be available in i8042 output.
    let status = unsafe { inb(0x64) };
    if status & 1 != 0 {
        let byte = unsafe { inb(0x60) };
        crate::ps2::enqueue_interrupt_byte(status & 0x20 != 0, byte);
    }
}

#[unsafe(no_mangle)]
extern "C" fn slopos_exception_handler(stack: *const u64) -> ! {
    // Common stub layout: 9 saved caller-clobbered registers, vector, error,
    // then CPU-pushed RIP, CS, and RFLAGS.
    let vector = unsafe { stack.add(9).read() };
    let error = unsafe { stack.add(10).read() };
    let instruction_pointer = unsafe { stack.add(11).read() };
    let cr2: u64;
    // SAFETY: reading CR2 is side-effect free at ring 0.
    unsafe { asm!("mov {value}, cr2", value = out(reg) cr2, options(nostack, preserves_flags)) };
    crate::serial::serialln(format_args!(
        "SLOPOS-EXCEPTION: vector={vector} error={error:#x} rip={instruction_pointer:#x} cr2={cr2:#x}"
    ));
    crate::fatal("unhandled CPU exception")
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: caller selects a platform I/O port with the required semantics.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        )
    };
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: caller selects a platform I/O port with the required semantics.
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        )
    };
    value
}

unsafe extern "C" {
    fn slopos_divide_error();
    fn slopos_invalid_opcode();
    fn slopos_double_fault();
    fn slopos_general_protection();
    fn slopos_page_fault();
    fn slopos_timer_interrupt();
    fn slopos_keyboard_interrupt();
    fn slopos_virtio_interrupt();
    fn slopos_mouse_interrupt();
    fn slopos_spurious_interrupt();
}

// Each stub saves all SysV caller-clobbered integer registers, realigns an
// arbitrary interrupted stack for a Rust call, invokes a bounded top half, and
// restores the exact interrupted context before IRETQ. IRQ gates run at ring 0
// and do not switch GS or privilege stacks in this phase.
global_asm!(
    r#"
    .macro SLOPOS_IRQ_STUB name, handler
    .global \name
    .type \name, @function
\name:
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    cld
    mov rax, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp], rax
    call \handler
    mov rsp, [rsp]
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    iretq
    .size \name, .-\name
    .endm

    SLOPOS_IRQ_STUB slopos_timer_interrupt, slopos_timer_handler
    SLOPOS_IRQ_STUB slopos_keyboard_interrupt, slopos_keyboard_handler
    SLOPOS_IRQ_STUB slopos_virtio_interrupt, slopos_virtio_handler
    SLOPOS_IRQ_STUB slopos_mouse_interrupt, slopos_mouse_handler

    .global slopos_spurious_interrupt
    .type slopos_spurious_interrupt, @function
slopos_spurious_interrupt:
    iretq
    .size slopos_spurious_interrupt, .-slopos_spurious_interrupt
"#
);

// Exceptions with and without hardware error codes are normalized to the same
// stack shape before entering a non-returning Rust diagnostic handler.
global_asm!(
    r#"
    .macro SLOPOS_EXCEPTION_NOERR name, vector
    .global \name
    .type \name, @function
\name:
    push 0
    push \vector
    jmp slopos_exception_common
    .size \name, .-\name
    .endm

    .macro SLOPOS_EXCEPTION_ERR name, vector
    .global \name
    .type \name, @function
\name:
    push \vector
    jmp slopos_exception_common
    .size \name, .-\name
    .endm

slopos_exception_common:
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    cld
    mov rdi, rsp
    and rsp, -16
    call slopos_exception_handler
    ud2

    SLOPOS_EXCEPTION_NOERR slopos_divide_error, 0
    SLOPOS_EXCEPTION_NOERR slopos_invalid_opcode, 6
    SLOPOS_EXCEPTION_ERR slopos_double_fault, 8
    SLOPOS_EXCEPTION_ERR slopos_general_protection, 13
    SLOPOS_EXCEPTION_ERR slopos_page_fault, 14
"#
);
