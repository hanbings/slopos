// SPDX-License-Identifier: 0BSD

use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::mem::size_of;

const KERNEL_CODE_SELECTOR: u16 = 0x08;
const TIMER_VECTOR: usize = 0x20;
const KEYBOARD_VECTOR: usize = 0x21;
const MOUSE_VECTOR: usize = 0x2c;

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
static GDT: [u64; 3] = [0, 0x00af_9a00_0000_ffff, 0x00cf_9200_0000_ffff];

pub fn initialize() {
    // SAFETY: early kernel initialization runs once with interrupts disabled.
    unsafe {
        load_gdt();
        install_idt();
        remap_pic();
        configure_pit();
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-INTERRUPT: GDT IDT PIC PIT initialized timer_hz=100"
    ));
}

pub fn enable() {
    // SAFETY: initialize installed every unmasked IRQ gate before this call.
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

unsafe fn load_gdt() {
    let pointer = DescriptorTablePointer {
        limit: (size_of::<[u64; 3]>() - 1) as u16,
        base: GDT.as_ptr() as u64,
    };
    // SAFETY: descriptors 1 and 2 are valid long-mode kernel code/data segments.
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
            pointer = in(reg) &pointer,
            out("rax") _,
            options(preserves_flags)
        )
    };
}

unsafe fn install_idt() {
    let entries = IDT.0.get();
    // SAFETY: exclusive early initialization; these symbols are valid interrupt stubs.
    unsafe {
        (*entries)[TIMER_VECTOR] = IdtEntry::interrupt_gate(slopos_timer_interrupt as usize as u64);
        (*entries)[KEYBOARD_VECTOR] =
            IdtEntry::interrupt_gate(slopos_keyboard_interrupt as usize as u64);
        (*entries)[MOUSE_VECTOR] = IdtEntry::interrupt_gate(slopos_mouse_interrupt as usize as u64);
    }
    let pointer = DescriptorTablePointer {
        limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
        base: entries as u64,
    };
    // SAFETY: pointer covers the static IDT for the lifetime of the kernel.
    unsafe { asm!("lidt [{pointer}]", pointer = in(reg) &pointer, options(readonly, nostack)) };
}

unsafe fn remap_pic() {
    // ICW1: initialize both 8259s, followed by vector offsets and cascade wiring.
    unsafe {
        outb(0x20, 0x11);
        io_wait();
        outb(0xa0, 0x11);
        io_wait();
        outb(0x21, 0x20);
        io_wait();
        outb(0xa1, 0x28);
        io_wait();
        outb(0x21, 0x04);
        io_wait();
        outb(0xa1, 0x02);
        io_wait();
        outb(0x21, 0x01);
        io_wait();
        outb(0xa1, 0x01);
        io_wait();

        // Master IRQ0 timer, IRQ1 keyboard, IRQ2 cascade; slave IRQ4 mouse.
        outb(0x21, 0xf8);
        outb(0xa1, 0xef);
    }
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

fn end_of_interrupt(irq: u8) {
    // SAFETY: fixed PIC command ports; slave must be acknowledged before master.
    unsafe {
        if irq >= 8 {
            outb(0xa0, 0x20);
        }
        outb(0x20, 0x20);
    }
}

#[unsafe(no_mangle)]
extern "C" fn slopos_timer_handler() {
    crate::timer::interrupt_tick();
    end_of_interrupt(0);
}

#[unsafe(no_mangle)]
extern "C" fn slopos_keyboard_handler() {
    consume_ps2_irq();
    end_of_interrupt(1);
}

#[unsafe(no_mangle)]
extern "C" fn slopos_mouse_handler() {
    consume_ps2_irq();
    end_of_interrupt(12);
}

fn consume_ps2_irq() {
    // SAFETY: IRQ1/IRQ12 indicate data may be available in i8042 output.
    let status = unsafe { inb(0x64) };
    if status & 1 != 0 {
        let byte = unsafe { inb(0x60) };
        crate::ps2::enqueue_interrupt_byte(status & 0x20 != 0, byte);
    }
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

unsafe fn io_wait() {
    // SAFETY: port 0x80 is the conventional x86 delay port.
    unsafe { outb(0x80, 0) };
}

unsafe extern "C" {
    fn slopos_timer_interrupt();
    fn slopos_keyboard_interrupt();
    fn slopos_mouse_interrupt();
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
    SLOPOS_IRQ_STUB slopos_mouse_interrupt, slopos_mouse_handler
"#
);
