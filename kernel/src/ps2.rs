// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU16, AtomicU64, AtomicUsize, Ordering};
use core::task::{Context, Poll};

use crate::desktop_config::DesktopConfigSources;
use crate::desktop_service::DesktopServiceSnapshot;

const DATA: u16 = 0x60;
const STATUS: u16 = 0x64;
const COMMAND: u16 = 0x64;
const WAIT_LIMIT: usize = 100_000;
const QUEUE_CAPACITY: usize = 128;

static INPUT_QUEUE: [AtomicU16; QUEUE_CAPACITY] = [const { AtomicU16::new(0) }; QUEUE_CAPACITY];
static QUEUE_HEAD: AtomicUsize = AtomicUsize::new(0);
static QUEUE_TAIL: AtomicUsize = AtomicUsize::new(0);
static DROPPED_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub struct RawInputByte {
    mouse: bool,
    value: u8,
}

#[derive(Clone, Copy, Debug)]
pub enum Key {
    Character(u8),
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Enter,
    Backspace,
    Tab,
    Escape,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub logo: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: KeyModifiers,
}

#[derive(Clone, Copy, Debug)]
pub struct MouseEvent {
    pub dx: i16,
    pub dy: i16,
    pub wheel: i8,
    pub buttons: u8,
    pub modifiers: KeyModifiers,
}

#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
}

pub enum DesktopEvent {
    Input(RawInputByte),
    ConfigUpdate(DesktopConfigSources),
    ServiceUpdate(DesktopServiceSnapshot),
}

pub struct Controller {
    shift: bool,
    control: bool,
    alt: bool,
    logo: bool,
    extended: bool,
    mouse_packet: [u8; 4],
    mouse_packet_length: usize,
    mouse_index: usize,
    mouse_present: bool,
    mouse_wheel_present: bool,
}

impl Controller {
    pub fn initialize() -> Self {
        drain_output();
        let mut mouse_present = false;
        let mut mouse_wheel_present = false;

        if write_command(0xa8) && write_command(0xae) {
            let config = if write_command(0x20) {
                read_data_wait().map_or(0, |(_, value)| value)
            } else {
                0
            };
            if write_command(0x60) {
                // Keep IRQ delivery disabled until the kernel has installed an IDT,
                // but enable both clocks. Preserve firmware's translation setting.
                let _ = write_data((config & !0x33) | 0x03);
            }
            if mouse_command(0xf6) {
                mouse_wheel_present = enable_mouse_wheel()
                    && mouse_device_id().is_some_and(|device_id| matches!(device_id, 3 | 4));
                mouse_present = mouse_command(0xf4);
            }
            let _ = keyboard_command(0xf4);
        }
        drain_output();

        Self {
            shift: false,
            control: false,
            alt: false,
            logo: false,
            extended: false,
            mouse_packet: [0; 4],
            mouse_packet_length: if mouse_wheel_present { 4 } else { 3 },
            mouse_index: 0,
            mouse_present,
            mouse_wheel_present,
        }
    }

    pub const fn mouse_present(&self) -> bool {
        self.mouse_present
    }

    pub const fn mouse_wheel_present(&self) -> bool {
        self.mouse_wheel_present
    }

    pub fn consume(&mut self, byte: RawInputByte) -> Option<InputEvent> {
        if byte.mouse {
            self.consume_mouse(byte.value)
        } else {
            self.consume_keyboard(byte.value)
        }
    }

    fn consume_mouse(&mut self, byte: u8) -> Option<InputEvent> {
        if self.mouse_index == 0 && byte & 0x08 == 0 {
            return None;
        }
        self.mouse_packet[self.mouse_index] = byte;
        self.mouse_index += 1;
        if self.mouse_index != self.mouse_packet_length {
            return None;
        }
        self.mouse_index = 0;

        let flags = self.mouse_packet[0];
        if flags & 0xc0 != 0 {
            return None;
        }
        let dx = self.mouse_packet[1] as i8 as i16;
        let dy = -(self.mouse_packet[2] as i8 as i16);
        let wheel = if self.mouse_packet_length == 4 {
            let value = (self.mouse_packet[3] & 0x0f) as i8;
            if value & 0x08 != 0 { value - 16 } else { value }
        } else {
            0
        };
        Some(InputEvent::Mouse(MouseEvent {
            dx,
            dy,
            wheel,
            buttons: flags & 0x07,
            modifiers: KeyModifiers {
                shift: self.shift,
                control: self.control,
                alt: self.alt,
                logo: self.logo,
            },
        }))
    }

    fn consume_keyboard(&mut self, byte: u8) -> Option<InputEvent> {
        if byte == 0xe0 {
            self.extended = true;
            return None;
        }
        let extended = self.extended;
        self.extended = false;
        let released = byte & 0x80 != 0;
        let code = byte & 0x7f;
        if !extended && (code == 0x2a || code == 0x36) {
            self.shift = !released;
            return None;
        }
        if code == 0x1d {
            self.control = !released;
            return None;
        }
        if code == 0x38 {
            self.alt = !released;
            return None;
        }
        if extended && matches!(code, 0x5b | 0x5c) {
            self.logo = !released;
            return None;
        }
        if released {
            return None;
        }
        let key = match (extended, code) {
            (true, 0x48) => Key::Up,
            (true, 0x50) => Key::Down,
            (true, 0x4b) => Key::Left,
            (true, 0x4d) => Key::Right,
            (true, 0x49) => Key::PageUp,
            (true, 0x51) => Key::PageDown,
            (true, 0x1c) => Key::Enter,
            (true, _) => return None,
            (false, 0x01) => Key::Escape,
            (false, 0x0e) => Key::Backspace,
            (false, 0x0f) => Key::Tab,
            (false, 0x1c) => Key::Enter,
            (false, code) => Key::Character(scancode_character(code, self.shift)?),
        };
        Some(InputEvent::Key(KeyEvent {
            key,
            modifiers: KeyModifiers {
                shift: self.shift,
                control: self.control,
                alt: self.alt,
                logo: self.logo,
            },
        }))
    }
}

pub fn enqueue_interrupt_byte(mouse: bool, value: u8) {
    let head = QUEUE_HEAD.load(Ordering::Relaxed);
    let next = (head + 1) % QUEUE_CAPACITY;
    if next == QUEUE_TAIL.load(Ordering::Acquire) {
        DROPPED_BYTES.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let encoded = u16::from(value) | if mouse { 1 << 8 } else { 0 };
    INPUT_QUEUE[head].store(encoded, Ordering::Relaxed);
    QUEUE_HEAD.store(next, Ordering::Release);
    crate::executor::wake_task(crate::executor::INPUT_TASK);
}

pub async fn next_desktop_event(config_generation: u64, service_generation: u64) -> DesktopEvent {
    NextDesktopEvent {
        config_generation,
        service_generation,
    }
    .await
}

pub fn dropped_bytes() -> u64 {
    DROPPED_BYTES.load(Ordering::Relaxed)
}

struct NextDesktopEvent {
    config_generation: u64,
    service_generation: u64,
}

impl Future for NextDesktopEvent {
    type Output = DesktopEvent;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(sources) = crate::desktop_config::latest_after(self.config_generation) {
            return Poll::Ready(DesktopEvent::ConfigUpdate(sources));
        }
        if let Some(snapshot) = crate::desktop_service::latest_after(self.service_generation) {
            return Poll::Ready(DesktopEvent::ServiceUpdate(snapshot));
        }
        let tail = QUEUE_TAIL.load(Ordering::Relaxed);
        if tail == QUEUE_HEAD.load(Ordering::Acquire) {
            return Poll::Pending;
        }
        let encoded = INPUT_QUEUE[tail].load(Ordering::Relaxed);
        QUEUE_TAIL.store((tail + 1) % QUEUE_CAPACITY, Ordering::Release);
        Poll::Ready(DesktopEvent::Input(RawInputByte {
            mouse: encoded & (1 << 8) != 0,
            value: encoded as u8,
        }))
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

fn enable_mouse_wheel() -> bool {
    for rate in [200, 100, 80] {
        if !mouse_command(0xf3) || !mouse_command(rate) {
            return false;
        }
    }
    true
}

fn mouse_device_id() -> Option<u8> {
    if !mouse_command(0xf2) {
        return None;
    }
    match read_data_wait() {
        Some((status, device_id)) if status & 0x20 != 0 => Some(device_id),
        _ => None,
    }
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
