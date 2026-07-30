// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::panic::PanicInfo;
use slopos_desktop_protocol::{
    COMMIT_SIZE, DESKTOP_COMMIT_SYSCALL, DESKTOP_WAIT_SYSCALL, DesktopCommit, DesktopServiceEvent,
    EVENT_POLICY_APPLIED, EVENT_SIZE, WALLPAPER_AURORA, config_hash,
};

const USER_ENTRY: u64 = 0x4000_0000;
const INITIAL_STACK_BASE: u64 = 0x4000_2000;
const USER_STACK_TOP: u64 = 0x4000_3000;
const INITIAL_STACK_WORDS: usize = 26;
const INITIAL_ARGC: u64 = 2;
const INITIAL_ENVC: usize = 3;
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
const SYS_SCHED_YIELD: u64 = 24;
const SYS_EXIT: u64 = 60;
const SYS_OPENAT: u64 = 257;
const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const STDOUT: u64 = 1;
const EXPECTED_FD: i64 = 3;
const PREEMPTION_TSC_WINDOW: u64 = 100_000_000;
static MESSAGE: &[u8; 28] = b"SLOPOS desktop policy ready\n";
static WAYBAR_PATH: &[u8; 25] = b"/etc/slopos/waybar.jsonc\0";
static SWWW_PATH: &[u8; 21] = b"/etc/slopos/swww.env\0";
static EXPECTED_WAYBAR: &[u8; 904] = include_bytes!("../../../assets/waybar-config.jsonc");
static EXPECTED_SWWW: &[u8; 172] = include_bytes!("../../../assets/swww.env");
static EXPECTED_ARGV: [&[u8]; 2] = [b"/sbin/slop-shell", b"--session"];
static EXPECTED_ENVIRONMENT: [&[u8]; INITIAL_ENVC] = [
    b"SLOPOS_ROLE=desktop-shell",
    b"XDG_CURRENT_DESKTOP=SlopOS",
    b"WAYLAND_DISPLAY=wayland-0",
];

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
    let fd = open(WAYBAR_PATH);
    if fd != EXPECTED_FD || syscall0(SYS_SCHED_YIELD) != 0 {
        exit(2);
    }
    let mut waybar = [0u8; EXPECTED_WAYBAR.len()];
    if !read_exact(fd, &mut waybar) || waybar != *EXPECTED_WAYBAR {
        exit(3);
    }
    if syscall1(SYS_CLOSE, fd as u64) != 0 {
        exit(4);
    }
    let fd = open(SWWW_PATH);
    if fd != EXPECTED_FD {
        exit(5);
    }
    let mut swww = [0u8; EXPECTED_SWWW.len()];
    if !read_exact(fd, &mut swww) || swww != *EXPECTED_SWWW {
        exit(6);
    }
    if syscall1(SYS_CLOSE, fd as u64) != 0 {
        exit(7);
    }
    let commit = DesktopCommit::new(
        config_hash(&waybar),
        config_hash(&swww),
        0,
        36,
        WALLPAPER_AURORA,
    );
    if syscall2(
        DESKTOP_COMMIT_SYSCALL,
        (&raw const commit) as u64,
        COMMIT_SIZE as u64,
    ) != 0
    {
        exit(8);
    }
    let mut event_bytes = [0u8; EVENT_SIZE];
    if syscall3(
        DESKTOP_WAIT_SYSCALL,
        event_bytes.as_mut_ptr() as u64,
        EVENT_SIZE as u64,
        0,
    ) != 0
    {
        exit(9);
    }
    let Ok(event) = DesktopServiceEvent::decode(&event_bytes) else {
        exit(10);
    };
    if event.kind != EVENT_POLICY_APPLIED || event.generation != 1 {
        exit(11);
    }
    let result = syscall3(
        SYS_WRITE,
        STDOUT,
        MESSAGE.as_ptr() as u64,
        MESSAGE.len() as u64,
    );
    exit(if result == MESSAGE.len() as i64 {
        0
    } else {
        12
    })
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

fn read_exact(fd: i64, output: &mut [u8]) -> bool {
    let mut copied = 0usize;
    while copied < output.len() {
        let remaining = output.len() - copied;
        let bytes = syscall3(
            SYS_READ,
            fd as u64,
            output[copied..].as_mut_ptr() as u64,
            remaining as u64,
        );
        if bytes <= 0 || bytes as usize > remaining {
            return false;
        }
        copied += bytes as usize;
    }
    let mut extra = 0u8;
    syscall3(SYS_READ, fd as u64, (&raw mut extra) as u64, 1) == 0
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
        || words[7] != 0
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
    words[8..]
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
    exit(13)
}
