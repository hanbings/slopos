// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::panic::PanicInfo;
use slopos_desktop_protocol::{
    COMMIT_SIZE, CONFIG_HASH_OFFSET, DESKTOP_COMMIT_SYSCALL, DESKTOP_WAIT_SYSCALL, DesktopCommit,
    DesktopServiceEvent, EVENT_CONFIG_APPLIED, EVENT_POLICY_APPLIED, EVENT_SIZE, WALLPAPER_AURORA,
    WAYLAND_EVENT_MAX_WIRE_SIZE, WAYLAND_SURFACE_MAX_WIRE_SIZE, config_hash_extend,
};
use slopos_wayland::{
    ArgumentReader, CORE_GLOBALS, DISPLAY_OBJECT_ID, Frame, MessageBuilder, WireError,
};

const USER_ENTRY: u64 = 0x4000_0000;
const INITIAL_STACK_BASE: u64 = 0x4000_5000;
const USER_STACK_TOP: u64 = 0x4000_6000;
const INITIAL_STACK_WORDS: usize = 27;
const INITIAL_ARGC: u64 = 2;
const INITIAL_ENVC: usize = 4;
const LINUX_AT_NULL: u64 = 0;
const LINUX_AT_PAGESZ: u64 = 6;
const LINUX_AT_ENTRY: u64 = 9;
const LINUX_AT_UID: u64 = 11;
const LINUX_AT_EUID: u64 = 12;
const LINUX_AT_GID: u64 = 13;
const LINUX_AT_EGID: u64 = 14;
const LINUX_AT_SECURE: u64 = 23;
const LINUX_AT_EXECFN: u64 = 31;
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_MMAP: u64 = 9;
const SYS_SCHED_YIELD: u64 = 24;
const SYS_SOCKET: u64 = 41;
const SYS_CONNECT: u64 = 42;
const SYS_SENDMSG: u64 = 46;
const SYS_EXIT: u64 = 60;
const SYS_FTRUNCATE: u64 = 77;
const SYS_OPENAT: u64 = 257;
const SYS_MEMFD_CREATE: u64 = 319;
const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const STDOUT: u64 = 1;
const FIRST_DYNAMIC_FD: i64 = 3;
const AF_UNIX: u64 = 1;
const SOCK_STREAM: u64 = 1;
const PROT_READ_WRITE: u64 = 3;
const MAP_SHARED: u64 = 1;
const SOL_SOCKET: i32 = 1;
const SCM_RIGHTS: i32 = 1;
const SHARED_MAPPING_ADDRESS: u64 = USER_STACK_TOP;
const SHARED_MAPPING_LENGTH: usize = 4096;
const PREEMPTION_TSC_WINDOW: u64 = 100_000_000;
const CONFIG_READ_CAPACITY: usize = 256;
const SOCKET_WRITE_CAPACITY: usize = 256;
const WAYBAR_FILE_CAPACITY: usize = 4096;
const SWWW_ENV_FILE_CAPACITY: usize = 512;
const REGISTRY: u32 = 2;
const COMPOSITOR: u32 = 3;
const SHM: u32 = 4;
const WM_BASE: u32 = 5;
const SURFACE: u32 = 6;
const POOL: u32 = 7;
const BUFFER: u32 = 8;
const XDG_SURFACE: u32 = 9;
const TOPLEVEL: u32 = 10;
const FRAME_CALLBACK: u32 = 11;
const SEAT: u32 = 12;
const OUTPUT: u32 = 13;
const POINTER: u32 = 14;
const SURFACE_WIDTH: usize = 32;
const SURFACE_HEIGHT: usize = 24;
const SURFACE_PIXEL_LENGTH: usize = SURFACE_WIDTH * SURFACE_HEIGHT * 4;
static MESSAGE: &[u8; 28] = b"SLOPOS desktop policy ready\n";
static WAYBAR_PATH: &[u8; 25] = b"/etc/slopos/waybar.jsonc\0";
static SWWW_PATH: &[u8; 21] = b"/etc/slopos/swww.env\0";
static WAYLAND_SOCKET_PATH: &[u8; 21] = b"/run/slopos/wayland-0";
static WAYLAND_MEMFD_NAME: &[u8; 19] = b"slopos-wayland-shm\0";
static EXPECTED_ARGV: [&[u8]; 2] = [b"/sbin/slop-shell", b"--session"];
static EXPECTED_ENVIRONMENT: [&[u8]; INITIAL_ENVC] = [
    b"SLOPOS_ROLE=desktop-shell",
    b"XDG_CURRENT_DESKTOP=SlopOS",
    b"WAYLAND_DISPLAY=wayland-0",
    b"SLOPOS_WAYBAR_OUTPUT=SLOPOS-1",
];

#[repr(C)]
struct IoVec {
    base: *const u8,
    length: usize,
}

#[repr(C)]
struct MessageHeader {
    name: *const u8,
    name_length: u32,
    name_padding: u32,
    vectors: *const IoVec,
    vector_count: usize,
    control: *const RightsControl,
    control_length: usize,
    flags: i32,
    flags_padding: u32,
}

#[repr(C, align(8))]
struct RightsControl {
    length: usize,
    level: i32,
    kind: i32,
    fd: i32,
    padding: u32,
}

const _: () = assert!(size_of::<IoVec>() == 16);
const _: () = assert!(size_of::<MessageHeader>() == 56);
const _: () = assert!(size_of::<RightsControl>() == 24);

global_asm!(
    r#"
    .section .text._start,"ax",@progbits
    .global _start
    .type _start, @function
_start:
    mov rdi, rsp
    and rsp, -16
    call slopos_desktop_main
    ud2
    .size _start, .-_start
"#
);

#[unsafe(no_mangle)]
pub extern "C" fn slopos_desktop_main(initial_stack: *const u64) -> ! {
    if !initial_stack_is_valid(initial_stack) || syscall0(SYS_SCHED_YIELD) != 0 {
        exit(1);
    }
    exercise_preemption();
    let mut policy_generation = 0;
    let mut config_generation = 0;
    let mut announced = false;
    let mut wayland_socket = None;
    loop {
        let expected_config_fd = if wayland_socket.is_some() {
            FIRST_DYNAMIC_FD + 1
        } else {
            FIRST_DYNAMIC_FD
        };
        let commit = load_policy(policy_generation == 0, expected_config_fd);
        if syscall2(
            DESKTOP_COMMIT_SYSCALL,
            (&raw const commit) as u64,
            COMMIT_SIZE as u64,
        ) != 0
        {
            exit(8);
        }
        policy_generation =
            wait_for_event(EVENT_POLICY_APPLIED, policy_generation).unwrap_or_else(|| exit(9));
        if wayland_socket.is_none() {
            let fd = connect_wayland_socket();
            submit_wayland_surface(fd);
            wayland_socket = Some(fd);
        }
        if !announced {
            let result = syscall3(
                SYS_WRITE,
                STDOUT,
                MESSAGE.as_ptr() as u64,
                MESSAGE.len() as u64,
            );
            if result != MESSAGE.len() as i64 {
                exit(10);
            }
            announced = true;
        }
        config_generation =
            wait_for_event(EVENT_CONFIG_APPLIED, config_generation).unwrap_or_else(|| exit(11));
    }
}

fn connect_wayland_socket() -> i64 {
    let fd = syscall3(SYS_SOCKET, AF_UNIX, SOCK_STREAM, 0);
    if fd != FIRST_DYNAMIC_FD {
        exit(13);
    }
    let mut address = [0u8; 2 + WAYLAND_SOCKET_PATH.len() + 1];
    address[..2].copy_from_slice(&(AF_UNIX as u16).to_ne_bytes());
    address[2..2 + WAYLAND_SOCKET_PATH.len()].copy_from_slice(WAYLAND_SOCKET_PATH);
    if syscall3(
        SYS_CONNECT,
        fd as u64,
        address.as_ptr() as u64,
        address.len() as u64,
    ) != 0
    {
        exit(14);
    }
    fd
}

fn submit_wayland_surface(socket: i64) {
    let mut wire = [0; WAYLAND_SURFACE_MAX_WIRE_SIZE];
    let mut wire_length = 0;
    append_message(
        &mut wire,
        &mut wire_length,
        DISPLAY_OBJECT_ID,
        1,
        |message| message.object(REGISTRY),
    )
    .unwrap_or_else(|_| exit(15));
    send_wayland_wire(socket, &wire[..wire_length]);
    wait_registry(socket).unwrap_or_else(|| exit(16));

    wire_length = build_initial_surface_wire(&mut wire).unwrap_or_else(|| exit(17));
    send_wayland_wire(socket, &wire[..wire_length]);
    let configure_serial = wait_configure(socket).unwrap_or_else(|| exit(18));
    let (backing_fd, pixels) = create_wayland_backing();
    submit_configured_surface(socket, configure_serial, backing_fd, pixels);
    wait_presented(socket, 1).unwrap_or_else(|| exit(19));
    submit_repeated_surface(socket, pixels);
    wait_presented(socket, 2).unwrap_or_else(|| exit(20));
    if syscall1(SYS_CLOSE, backing_fd as u64) != 0 {
        exit(28);
    }
}

fn send_wayland_wire(socket: i64, mut wire: &[u8]) {
    while !wire.is_empty() {
        let length = wire.len().min(SOCKET_WRITE_CAPACITY);
        if syscall3(
            SYS_WRITE,
            socket as u64,
            wire.as_ptr() as u64,
            length as u64,
        ) != length as i64
        {
            exit(21);
        }
        wire = &wire[length..];
    }
}

fn send_wayland_wire_with_fd(socket: i64, wire: &[u8], fd: i64) {
    let vector = IoVec {
        base: wire.as_ptr(),
        length: wire.len(),
    };
    let control = RightsControl {
        length: 20,
        level: SOL_SOCKET,
        kind: SCM_RIGHTS,
        fd: fd as i32,
        padding: 0,
    };
    let header = MessageHeader {
        name: core::ptr::null(),
        name_length: 0,
        name_padding: 0,
        vectors: &raw const vector,
        vector_count: 1,
        control: &raw const control,
        control_length: size_of::<RightsControl>(),
        flags: 0,
        flags_padding: 0,
    };
    if syscall3(SYS_SENDMSG, socket as u64, (&raw const header) as u64, 0) != wire.len() as i64 {
        exit(27);
    }
}

fn create_wayland_backing() -> (i64, &'static mut [u8]) {
    let fd = syscall2(SYS_MEMFD_CREATE, WAYLAND_MEMFD_NAME.as_ptr() as u64, 0);
    if fd != FIRST_DYNAMIC_FD + 1 {
        exit(24);
    }
    if syscall2(SYS_FTRUNCATE, fd as u64, SURFACE_PIXEL_LENGTH as u64) != 0 {
        exit(25);
    }
    let address = syscall6(
        SYS_MMAP,
        0,
        SHARED_MAPPING_LENGTH as u64,
        PROT_READ_WRITE,
        MAP_SHARED,
        fd as u64,
        0,
    );
    if address != SHARED_MAPPING_ADDRESS as i64 {
        exit(26);
    }
    // SAFETY: mmap installed one writable shared page at this fixed address;
    // the memfd was truncated to exactly the surface backing length.
    let pixels = unsafe {
        core::slice::from_raw_parts_mut(SHARED_MAPPING_ADDRESS as *mut u8, SURFACE_PIXEL_LENGTH)
    };
    (fd, pixels)
}

fn submit_configured_surface(
    socket: i64,
    configure_serial: u32,
    backing_fd: i64,
    pixels: &mut [u8],
) {
    let mut wire = [0u8; WAYLAND_SURFACE_MAX_WIRE_SIZE];
    let wire_length = build_configured_surface_wire(
        &mut wire,
        configure_serial,
        SURFACE_WIDTH as i32,
        SURFACE_HEIGHT as i32,
        SURFACE_PIXEL_LENGTH as i32,
    )
    .unwrap_or_else(|| exit(22));
    fill_surface_pixels(pixels, false);
    send_wayland_wire_with_fd(socket, &wire[..wire_length], backing_fd);
}

fn submit_repeated_surface(socket: i64, pixels: &mut [u8]) {
    let mut wire = [0u8; WAYLAND_SURFACE_MAX_WIRE_SIZE];
    let wire_length = build_repeated_surface_wire(&mut wire).unwrap_or_else(|| exit(23));
    fill_surface_pixels(pixels, true);
    send_wayland_wire(socket, &wire[..wire_length]);
}

fn fill_surface_pixels(pixels: &mut [u8], second_frame: bool) {
    if pixels.len() != SURFACE_PIXEL_LENGTH {
        exit(29);
    }
    for y in 0..SURFACE_HEIGHT {
        for x in 0..SURFACE_WIDTH {
            let color: u32 =
                if x == 0 || y == 0 || x + 1 == SURFACE_WIDTH || y + 1 == SURFACE_HEIGHT {
                    0x0010_131f
                } else if x == 15 || x == 16 || y == 11 || y == 12 {
                    0x00f8_f8f2
                } else if y < SURFACE_HEIGHT / 2 && x < SURFACE_WIDTH / 2 {
                    if second_frame {
                        0x008b_e9fd
                    } else {
                        0x0000_d4ff
                    }
                } else if y < SURFACE_HEIGHT / 2 {
                    if second_frame {
                        0x00ff_5555
                    } else {
                        0x00ff_79c6
                    }
                } else if x < SURFACE_WIDTH / 2 {
                    if second_frame {
                        0x00bd_93f9
                    } else {
                        0x0050_fa7b
                    }
                } else if second_frame {
                    0x00ff_b86c
                } else {
                    0x00f1_fa8c
                };
            let offset = (y * SURFACE_WIDTH + x) * 4;
            pixels[offset..offset + 4].copy_from_slice(&color.to_le_bytes());
        }
    }
}

fn build_initial_surface_wire(wire: &mut [u8]) -> Option<usize> {
    let mut cursor = 0;
    for (global, object) in [
        (CORE_GLOBALS[0], COMPOSITOR),
        (CORE_GLOBALS[1], SHM),
        (CORE_GLOBALS[2], SEAT),
        (CORE_GLOBALS[3], OUTPUT),
        (CORE_GLOBALS[4], WM_BASE),
    ] {
        append_message(wire, &mut cursor, REGISTRY, 0, |message| {
            message.uint(global.name)?;
            message.string(global.interface.name())?;
            message.uint(global.version)?;
            message.object(object)
        })
        .ok()?;
    }
    append_message(wire, &mut cursor, SEAT, 0, |message| {
        message.object(POINTER)
    })
    .ok()?;
    append_message(wire, &mut cursor, COMPOSITOR, 0, |message| {
        message.object(SURFACE)
    })
    .ok()?;
    append_message(wire, &mut cursor, WM_BASE, 2, |message| {
        message.object(XDG_SURFACE)?;
        message.object(SURFACE)
    })
    .ok()?;
    append_message(wire, &mut cursor, XDG_SURFACE, 1, |message| {
        message.object(TOPLEVEL)
    })
    .ok()?;
    append_message(wire, &mut cursor, TOPLEVEL, 2, |message| {
        message.string("SlopOS Userspace")
    })
    .ok()?;
    append_message(wire, &mut cursor, TOPLEVEL, 3, |message| {
        message.string("slopos-system")
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 6, |_| Ok(())).ok()?;
    Some(cursor)
}

fn build_configured_surface_wire(
    wire: &mut [u8],
    configure_serial: u32,
    width: i32,
    height: i32,
    pixel_length: i32,
) -> Option<usize> {
    let mut cursor = 0;
    append_message(wire, &mut cursor, XDG_SURFACE, 4, |message| {
        message.uint(configure_serial)
    })
    .ok()?;
    append_message(wire, &mut cursor, SHM, 0, |message| {
        message.object(POOL)?;
        message.int(pixel_length)
    })
    .ok()?;
    append_message(wire, &mut cursor, POOL, 0, |message| {
        message.object(BUFFER)?;
        message.int(0)?;
        message.int(width)?;
        message.int(height)?;
        message.int(width * 4)?;
        message.uint(1)
    })
    .ok()?;
    append_message(wire, &mut cursor, XDG_SURFACE, 3, |message| {
        message.int(0)?;
        message.int(0)?;
        message.int(width)?;
        message.int(height)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 1, |message| {
        message.object(BUFFER)?;
        message.int(0)?;
        message.int(0)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 9, |message| {
        message.int(0)?;
        message.int(0)?;
        message.int(width)?;
        message.int(height)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 3, |message| {
        message.object(FRAME_CALLBACK)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 6, |_| Ok(())).ok()?;
    Some(cursor)
}

fn build_repeated_surface_wire(wire: &mut [u8]) -> Option<usize> {
    let mut cursor = 0;
    append_message(wire, &mut cursor, SURFACE, 1, |message| {
        message.object(BUFFER)?;
        message.int(0)?;
        message.int(0)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 9, |message| {
        message.int(0)?;
        message.int(0)?;
        message.int(SURFACE_WIDTH as i32)?;
        message.int(SURFACE_HEIGHT as i32)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 3, |message| {
        message.object(FRAME_CALLBACK)
    })
    .ok()?;
    append_message(wire, &mut cursor, SURFACE, 6, |_| Ok(())).ok()?;
    Some(cursor)
}

fn wait_registry(socket: i64) -> Option<()> {
    let mut event_bytes = [0; WAYLAND_EVENT_MAX_WIRE_SIZE];
    let mut wire = receive_wayland_event(socket, &mut event_bytes)?;
    for global in CORE_GLOBALS {
        let (frame, remaining) = Frame::decode(wire).ok()?;
        if frame.header.object_id != REGISTRY || frame.header.opcode != 0 {
            return None;
        }
        let mut arguments = ArgumentReader::new(frame.payload);
        if arguments.uint().ok()? != global.name
            || arguments.string().ok()? != global.interface.name()
            || arguments.uint().ok()? != global.version
            || arguments.finish().is_err()
        {
            return None;
        }
        wire = remaining;
    }
    wire.is_empty().then_some(())
}

fn wait_configure(socket: i64) -> Option<u32> {
    let mut event_bytes = [0; WAYLAND_EVENT_MAX_WIRE_SIZE];
    let mut wire = receive_wayland_event(socket, &mut event_bytes)?;

    let mut arguments = take_event(&mut wire, SEAT, 0)?;
    if arguments.uint().ok()? != 1 || arguments.finish().is_err() {
        return None;
    }
    take_event(&mut wire, SEAT, 1)?;
    take_event(&mut wire, OUTPUT, 0)?;
    let mut arguments = take_event(&mut wire, OUTPUT, 1)?;
    if arguments.uint().ok()? != 3
        || arguments.int().ok()? != 1024
        || arguments.int().ok()? != 768
        || arguments.int().ok()? != 60_000
        || arguments.finish().is_err()
    {
        return None;
    }
    let mut arguments = take_event(&mut wire, OUTPUT, 3)?;
    if arguments.int().ok()? != 1 || arguments.finish().is_err() {
        return None;
    }
    take_event(&mut wire, OUTPUT, 4)?;
    take_event(&mut wire, OUTPUT, 5)?;
    if take_event(&mut wire, OUTPUT, 2)?.finish().is_err() {
        return None;
    }

    for format in [0, 1] {
        let mut arguments = take_event(&mut wire, SHM, 0)?;
        if arguments.uint().ok()? != format || arguments.finish().is_err() {
            return None;
        }
    }
    let mut arguments = take_event(&mut wire, TOPLEVEL, 0)?;
    if arguments.int().ok()? != 32
        || arguments.int().ok()? != 24
        || !arguments.array().ok()?.is_empty()
        || arguments.finish().is_err()
    {
        return None;
    }
    let mut arguments = take_event(&mut wire, XDG_SURFACE, 0)?;
    let serial = arguments.uint().ok()?;
    if serial == 0 || arguments.finish().is_err() || !wire.is_empty() {
        return None;
    }
    Some(serial)
}

fn wait_presented(socket: i64, callback_data: u32) -> Option<()> {
    let mut event_bytes = [0; WAYLAND_EVENT_MAX_WIRE_SIZE];
    let mut wire = receive_wayland_event(socket, &mut event_bytes)?;
    if callback_data == 1 {
        let mut enter = take_event(&mut wire, POINTER, 0)?;
        if enter.uint().ok()? != 2
            || enter.object().ok()? != SURFACE
            || enter.fixed().ok()? != 16 << 8
            || enter.fixed().ok()? != 12 << 8
            || enter.finish().is_err()
        {
            return None;
        }
    }
    if take_event(&mut wire, BUFFER, 0)?.finish().is_err() {
        return None;
    }
    let mut done_arguments = take_event(&mut wire, FRAME_CALLBACK, 0)?;
    let mut delete_arguments = take_event(&mut wire, DISPLAY_OBJECT_ID, 1)?;
    if done_arguments.uint().ok()? != callback_data
        || done_arguments.finish().is_err()
        || delete_arguments.object().ok()? != FRAME_CALLBACK
        || delete_arguments.finish().is_err()
        || !wire.is_empty()
    {
        return None;
    }
    Some(())
}

fn take_event<'a>(wire: &mut &'a [u8], object_id: u32, opcode: u16) -> Option<ArgumentReader<'a>> {
    let (frame, remaining) = Frame::decode(wire).ok()?;
    if frame.header.object_id != object_id || frame.header.opcode != opcode {
        return None;
    }
    *wire = remaining;
    Some(ArgumentReader::new(frame.payload))
}

fn receive_wayland_event(
    socket: i64,
    destination: &mut [u8; WAYLAND_EVENT_MAX_WIRE_SIZE],
) -> Option<&[u8]> {
    let length = syscall3(
        SYS_READ,
        socket as u64,
        destination.as_mut_ptr() as u64,
        WAYLAND_EVENT_MAX_WIRE_SIZE as u64,
    );
    if length <= 0 || length as usize > destination.len() {
        return None;
    }
    Some(&destination[..length as usize])
}

fn append_message(
    bytes: &mut [u8],
    cursor: &mut usize,
    object_id: u32,
    opcode: u16,
    build: impl FnOnce(&mut MessageBuilder<'_>) -> Result<(), WireError>,
) -> Result<(), WireError> {
    let mut message = MessageBuilder::new(&mut bytes[*cursor..], object_id, opcode)?;
    build(&mut message)?;
    *cursor += message.finish()?.len();
    Ok(())
}

fn load_policy(yield_after_open: bool, expected_fd: i64) -> DesktopCommit {
    let fd = open(WAYBAR_PATH);
    if fd != expected_fd || (yield_after_open && syscall0(SYS_SCHED_YIELD) != 0) {
        exit(2);
    }
    let Some(waybar_hash) = read_config_hash(fd, WAYBAR_FILE_CAPACITY) else {
        exit(3);
    };
    if syscall1(SYS_CLOSE, fd as u64) != 0 {
        exit(4);
    }
    let fd = open(SWWW_PATH);
    if fd != expected_fd {
        exit(5);
    }
    let Some(swww_hash) = read_config_hash(fd, SWWW_ENV_FILE_CAPACITY) else {
        exit(6);
    };
    if syscall1(SYS_CLOSE, fd as u64) != 0 {
        exit(7);
    }
    DesktopCommit::new(waybar_hash, swww_hash, 0, 36, WALLPAPER_AURORA)
}

fn wait_for_event(kind: u16, after_generation: u64) -> Option<u64> {
    let mut event_bytes = [0u8; EVENT_SIZE];
    if syscall4(
        DESKTOP_WAIT_SYSCALL,
        event_bytes.as_mut_ptr() as u64,
        EVENT_SIZE as u64,
        after_generation,
        u64::from(kind),
    ) != 0
    {
        return None;
    }
    let event = DesktopServiceEvent::decode(&event_bytes).ok()?;
    (event.kind == kind && event.generation > after_generation).then_some(event.generation)
}

fn open(path: &[u8]) -> i64 {
    syscall4(
        SYS_OPENAT,
        AT_FDCWD as u64,
        path.as_ptr() as u64,
        O_RDONLY,
        0,
    )
}

fn read_config_hash(fd: i64, capacity: usize) -> Option<u64> {
    let mut buffer = [0u8; CONFIG_READ_CAPACITY];
    let mut length = 0usize;
    let mut hash = CONFIG_HASH_OFFSET;
    loop {
        let requested = if length == capacity {
            1
        } else {
            core::cmp::min(buffer.len(), capacity - length)
        };
        let bytes = syscall3(
            SYS_READ,
            fd as u64,
            buffer.as_mut_ptr() as u64,
            requested as u64,
        );
        if bytes < 0 || bytes as usize > requested {
            return None;
        }
        let bytes = bytes as usize;
        if bytes == 0 {
            return (length != 0).then_some(hash);
        }
        if length == capacity {
            return None;
        }
        hash = config_hash_extend(hash, &buffer[..bytes]);
        length += bytes;
    }
}

fn exercise_preemption() {
    let start = read_timestamp_counter();
    while read_timestamp_counter().wrapping_sub(start) < PREEMPTION_TSC_WINDOW {
        core::hint::spin_loop();
    }
}

fn read_timestamp_counter() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: TSC is architectural on x86-64 and the instruction has no
    // memory or stack side effects.
    unsafe {
        asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}

fn initial_stack_is_valid(initial_stack: *const u64) -> bool {
    let address = initial_stack as u64;
    let Some(end) = address.checked_add((INITIAL_STACK_WORDS * size_of::<u64>()) as u64) else {
        return false;
    };
    if address & 15 != 0 || address < INITIAL_STACK_BASE || end > USER_STACK_TOP {
        return false;
    }
    // SAFETY: the kernel constructed this bounded table in the upper stack page.
    let words = unsafe { core::slice::from_raw_parts(initial_stack, INITIAL_STACK_WORDS) };
    if words[0] != INITIAL_ARGC
        || words[3] != 0
        || words[8] != 0
        || !stack_string_equals(words[1], EXPECTED_ARGV[0])
        || !stack_string_equals(words[2], EXPECTED_ARGV[1])
    {
        return false;
    }
    for (index, expected) in EXPECTED_ENVIRONMENT.iter().enumerate() {
        if !stack_string_equals(words[4 + index], expected) {
            return false;
        }
    }
    words[9..]
        == [
            LINUX_AT_PAGESZ,
            4096,
            LINUX_AT_ENTRY,
            USER_ENTRY,
            LINUX_AT_UID,
            0,
            LINUX_AT_EUID,
            0,
            LINUX_AT_GID,
            0,
            LINUX_AT_EGID,
            0,
            LINUX_AT_SECURE,
            0,
            LINUX_AT_EXECFN,
            words[1],
            LINUX_AT_NULL,
            0,
        ]
}

fn stack_string_equals(address: u64, expected: &[u8]) -> bool {
    let Ok(length) = u64::try_from(expected.len()) else {
        return false;
    };
    let Some(end) = address
        .checked_add(length)
        .and_then(|end| end.checked_add(1))
    else {
        return false;
    };
    if address < INITIAL_STACK_BASE || end > USER_STACK_TOP {
        return false;
    }
    // SAFETY: address..end is within the mapped upper stack page.
    let actual = unsafe { core::slice::from_raw_parts(address as *const u8, expected.len() + 1) };
    actual[..expected.len()] == *expected && actual[expected.len()] == 0
}

fn syscall0(number: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the Linux x86-64 register convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            out("rcx") _,
            out("r11") _,
        );
    }
    result
}

fn syscall1(number: u64, first: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the Linux x86-64 register convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") first,
            out("rcx") _,
            out("r11") _,
        );
    }
    result
}

fn syscall2(number: u64, first: u64, second: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS private desktop ABI follows the same x86-64 entry convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") first,
            in("rsi") second,
            out("rcx") _,
            out("r11") _,
        );
    }
    result
}

fn syscall3(number: u64, first: u64, second: u64, third: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the Linux x86-64 register convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") first,
            in("rsi") second,
            in("rdx") third,
            out("rcx") _,
            out("r11") _,
        );
    }
    result
}

fn syscall4(number: u64, first: u64, second: u64, third: u64, fourth: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the Linux x86-64 register convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") first,
            in("rsi") second,
            in("rdx") third,
            in("r10") fourth,
            out("rcx") _,
            out("r11") _,
        );
    }
    result
}

fn syscall6(
    number: u64,
    first: u64,
    second: u64,
    third: u64,
    fourth: u64,
    fifth: u64,
    sixth: u64,
) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the Linux x86-64 register convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") first,
            in("rsi") second,
            in("rdx") third,
            in("r10") fourth,
            in("r8") fifth,
            in("r9") sixth,
            out("rcx") _,
            out("r11") _,
        );
    }
    result
}

fn exit(status: u64) -> ! {
    // SAFETY: syscall 60 terminates this process and never returns.
    unsafe {
        asm!(
            "syscall",
            in("rax") SYS_EXIT,
            in("rdi") status,
            options(noreturn)
        )
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    exit(12)
}
