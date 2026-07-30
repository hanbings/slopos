// SPDX-License-Identifier: 0BSD

use core::arch::asm;

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;
const COMMAND: u16 = 0x64;
const WAIT_LIMIT: usize = 100_000;

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Character(u8),
    Enter,
    Backspace,
    Tab,
    Escape,
}

#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub dx: i16,
    pub dy: i16,
    pub buttons: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    Key(Key),
    Mouse(MouseEvent),
}

pub struct Controller {
    shift: bool,
    extended: bool,
    mouse_packet: [u8; 3],
    mouse_index: usize,
    mouse_present: bool,
}

impl Controller {
    pub fn initialize() -> Self {
        drain_output();
        let mut mouse_present = false;

        if write_command(0xa8) && write_command(0xae) {
            let config = if write_command(0x20) {
                read_data_wait().map_or(0, |(_, value)| value)
            } else {
                0
            };
            if write_command(0x60) {
                // Keep IRQ delivery disabled until the kernel has installed an IDT,
                // but enable both clocks. Preserve firmware's translation setting.
                let _ = write_data(config & !0x23);
            }
            mouse_present = mouse_command(0xf6) && mouse_command(0xf4);
            let _ = keyboard_command(0xf4);
        }
        drain_output();

        Self {
            shift: false,
            extended: false,
            mouse_packet: [0; 3],
            mouse_index: 0,
            mouse_present,
        }
    }

    pub const fn mouse_present(&self) -> bool {
        self.mouse_present
    }

    pub fn poll(&mut self) -> Option<InputEvent> {
        // SAFETY: reading controller status and data is the only way to consume
        // bytes from the legacy PS/2 device selected for the QEMU target.
        let status = unsafe { inb(STATUS) };
        if status & 1 == 0 {
            return None;
        }
        let byte = unsafe { inb(DATA) };
        if status & 0x20 != 0 {
            self.consume_mouse(byte)
        } else {
            self.consume_keyboard(byte)
        }
    }

    fn consume_mouse(&mut self, byte: u8) -> Option<InputEvent> {
        if self.mouse_index == 0 && byte & 0x08 == 0 {
            return None;
        }
        self.mouse_packet[self.mouse_index] = byte;
        self.mouse_index += 1;
        if self.mouse_index != 3 {
            return None;
        }
        self.mouse_index = 0;

        let flags = self.mouse_packet[0];
        if flags & 0xc0 != 0 {
            return None;
        }
        let dx = self.mouse_packet[1] as i8 as i16;
        let dy = -(self.mouse_packet[2] as i8 as i16);
        Some(InputEvent::Mouse(MouseEvent {
            dx,
            dy,
            buttons: flags & 0x07,
        }))
    }

    fn consume_keyboard(&mut self, byte: u8) -> Option<InputEvent> {
        if byte == 0xe0 {
            self.extended = true;
            return None;
        }
        let released = byte & 0x80 != 0;
        let code = byte & 0x7f;
        if code == 0x2a || code == 0x36 {
            self.shift = !released;
            return None;
        }
        if released {
            self.extended = false;
            return None;
        }
        if self.extended {
            self.extended = false;
            return None;
        }
        let key = match code {
            0x01 => Key::Escape,
            0x0e => Key::Backspace,
            0x0f => Key::Tab,
            0x1c => Key::Enter,
            _ => Key::Character(scancode_character(code, self.shift)?),
        };
        Some(InputEvent::Key(key))
    }
}

fn scancode_character(code: u8, shift: bool) -> Option<u8> {
    let value = match code {
        0x02 => {
            if shift {
                b'!'
            } else {
                b'1'
            }
        }
        0x03 => {
            if shift {
                b'@'
            } else {
                b'2'
            }
        }
        0x04 => {
            if shift {
                b'#'
            } else {
                b'3'
            }
        }
        0x05 => {
            if shift {
                b'$'
            } else {
                b'4'
            }
        }
        0x06 => {
            if shift {
                b'%'
            } else {
                b'5'
            }
        }
        0x07 => {
            if shift {
                b'^'
            } else {
                b'6'
            }
        }
        0x08 => {
            if shift {
                b'&'
            } else {
                b'7'
            }
        }
        0x09 => {
            if shift {
                b'*'
            } else {
                b'8'
            }
        }
        0x0a => {
            if shift {
                b'('
            } else {
                b'9'
            }
        }
        0x0b => {
            if shift {
                b')'
            } else {
                b'0'
            }
        }
        0x0c => {
            if shift {
                b'_'
            } else {
                b'-'
            }
        }
        0x0d => {
            if shift {
                b'+'
            } else {
                b'='
            }
        }
        0x10 => b'q',
        0x11 => b'w',
        0x12 => b'e',
        0x13 => b'r',
        0x14 => b't',
        0x15 => b'y',
        0x16 => b'u',
        0x17 => b'i',
        0x18 => b'o',
        0x19 => b'p',
        0x1a => {
            if shift {
                b'{'
            } else {
                b'['
            }
        }
        0x1b => {
            if shift {
                b'}'
            } else {
                b']'
            }
        }
        0x1e => b'a',
        0x1f => b's',
        0x20 => b'd',
        0x21 => b'f',
        0x22 => b'g',
        0x23 => b'h',
        0x24 => b'j',
        0x25 => b'k',
        0x26 => b'l',
        0x27 => {
            if shift {
                b':'
            } else {
                b';'
            }
        }
        0x28 => {
            if shift {
                b'"'
            } else {
                b'\''
            }
        }
        0x2b => {
            if shift {
                b'|'
            } else {
                b'\\'
            }
        }
        0x2c => b'z',
        0x2d => b'x',
        0x2e => b'c',
        0x2f => b'v',
        0x30 => b'b',
        0x31 => b'n',
        0x32 => b'm',
        0x33 => {
            if shift {
                b'<'
            } else {
                b','
            }
        }
        0x34 => {
            if shift {
                b'>'
            } else {
                b'.'
            }
        }
        0x35 => {
            if shift {
                b'?'
            } else {
                b'/'
            }
        }
        0x39 => b' ',
        _ => return None,
    };
    Some(if shift && value.is_ascii_lowercase() {
        value.to_ascii_uppercase()
    } else {
        value
    })
}

fn keyboard_command(command: u8) -> bool {
    if !write_data(command) {
        return false;
    }
    matches!(read_data_wait(), Some((status, 0xfa)) if status & 0x20 == 0)
}

fn mouse_command(command: u8) -> bool {
    if !write_command(0xd4) || !write_data(command) {
        return false;
    }
    matches!(read_data_wait(), Some((status, 0xfa)) if status & 0x20 != 0)
}

fn drain_output() {
    for _ in 0..32 {
        // SAFETY: controller ports are fixed on the target platform.
        if unsafe { inb(STATUS) } & 1 == 0 {
            break;
        }
        let _ = unsafe { inb(DATA) };
    }
}

fn read_data_wait() -> Option<(u8, u8)> {
    for _ in 0..WAIT_LIMIT {
        // SAFETY: controller ports are fixed on the target platform.
        let status = unsafe { inb(STATUS) };
        if status & 1 != 0 {
            return Some((status, unsafe { inb(DATA) }));
        }
        core::hint::spin_loop();
    }
    None
}

fn write_command(command: u8) -> bool {
    if !wait_input_empty() {
        return false;
    }
    // SAFETY: the controller command register is fixed on x86 PCs.
    unsafe { outb(COMMAND, command) };
    true
}

fn write_data(value: u8) -> bool {
    if !wait_input_empty() {
        return false;
    }
    // SAFETY: the controller data register is fixed on x86 PCs.
    unsafe { outb(DATA, value) };
    true
}

fn wait_input_empty() -> bool {
    for _ in 0..WAIT_LIMIT {
        // SAFETY: the controller status register is fixed on x86 PCs.
        if unsafe { inb(STATUS) } & 2 == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
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
