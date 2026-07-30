// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::fmt::{self, Write};

const COM1: u16 = 0x3f8;

pub fn init() {
    // SAFETY: COM1 is the documented serial range for the target QEMU machine.
    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xc7);
        outb(COM1 + 4, 0x0b);
    }
}

pub fn serialln(args: fmt::Arguments<'_>) {
    let mut serial = Serial;
    let _ = serial.write_fmt(args);
    let _ = serial.write_str("\n");
}

struct Serial;

impl Write for Serial {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                write_byte(b'\r');
            }
            write_byte(byte);
        }
        Ok(())
    }
}

fn write_byte(byte: u8) {
    while unsafe { inb(COM1 + 5) } & 0x20 == 0 {
        core::hint::spin_loop();
    }
    // SAFETY: writing a byte to the initialized COM1 transmit register is valid.
    unsafe { outb(COM1, byte) };
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: the caller is responsible for selecting a valid I/O port.
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
    // SAFETY: the caller is responsible for selecting a valid I/O port.
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
