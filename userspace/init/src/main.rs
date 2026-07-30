// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::panic::PanicInfo;

const USER_ENTRY: u64 = 0x4000_0000;
const USER_STACK_BASE: u64 = 0x4000_1000;
const USER_STACK_TOP: u64 = 0x4000_2000;
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
const SYS_LSEEK: u64 = 8;
const SYS_EXIT: u64 = 60;
const SYS_OPENAT: u64 = 257;
const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const O_RDWR: u64 = 2;
const SEEK_SET: u64 = 0;
const STDOUT: u64 = 1;
const EXPECTED_FD: i64 = 3;
static MESSAGE: &[u8; 18] = b"SLOPOS user write\n";
static CONFIG_PATH: &[u8; 24] = b"/etc/slopos/system.conf\0";
static EXPECTED_CONFIG: &[u8; 76] =
    b"# SlopOS declarative configuration seed\ntheme = \"ocean\"\nhostname = \"slopos\"\n";
static WRITE_PATH: &[u8; 34] = b"/usr/share/slopos/write-probe.bin\0";
static PATCH: &[u8; 16] = &[0xa5; 16];
static ORIGINAL: &[u8; 16] = b"PPPPPPPPPPPPPPPP";
static EXPECTED_ARGV: [&[u8]; 2] = [b"/sbin/slop-init", b"--system"];
static EXPECTED_ENVIRONMENT: [&[u8]; INITIAL_ENVC] = [
    b"SLOPOS_SESSION=desktop",
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
    call slopos_init_main
    ud2
    .size _start, .-_start
"#
);

#[unsafe(no_mangle)]
pub extern "C" fn slopos_init_main(initial_stack: *const u64) -> ! {
    if !initial_stack_is_valid(initial_stack) {
        exit(9);
    }
    let fd = syscall4(
        SYS_OPENAT,
        AT_FDCWD as u64,
        CONFIG_PATH.as_ptr() as u64,
        O_RDONLY,
        0,
    );
    if fd != EXPECTED_FD {
        exit(1);
    }
    let mut configuration = [0u8; EXPECTED_CONFIG.len()];
    let bytes = syscall3(
        SYS_READ,
        fd as u64,
        configuration.as_mut_ptr() as u64,
        configuration.len() as u64,
    );
    if bytes != EXPECTED_CONFIG.len() as i64 || configuration != *EXPECTED_CONFIG {
        exit(2);
    }
    if syscall1(SYS_CLOSE, fd as u64) != 0 {
        exit(3);
    }
    let fd = syscall4(
        SYS_OPENAT,
        AT_FDCWD as u64,
        WRITE_PATH.as_ptr() as u64,
        O_RDWR,
        0,
    );
    if fd != EXPECTED_FD {
        exit(4);
    }
    if !seek(fd, 123)
        || syscall3(
            SYS_WRITE,
            fd as u64,
            PATCH.as_ptr() as u64,
            PATCH.len() as u64,
        ) != PATCH.len() as i64
        || !seek(fd, 123)
    {
        exit(5);
    }
    let mut verification = [0u8; PATCH.len()];
    if syscall3(
        SYS_READ,
        fd as u64,
        verification.as_mut_ptr() as u64,
        verification.len() as u64,
    ) != verification.len() as i64
        || verification != *PATCH
        || !seek(fd, 123)
        || syscall3(
            SYS_WRITE,
            fd as u64,
            ORIGINAL.as_ptr() as u64,
            ORIGINAL.len() as u64,
        ) != ORIGINAL.len() as i64
        || !seek(fd, 123)
    {
        exit(6);
    }
    verification.fill(0);
    if syscall3(
        SYS_READ,
        fd as u64,
        verification.as_mut_ptr() as u64,
        verification.len() as u64,
    ) != verification.len() as i64
        || verification != *ORIGINAL
        || syscall1(SYS_CLOSE, fd as u64) != 0
    {
        exit(7);
    }
    let result = syscall3(
        SYS_WRITE,
        STDOUT,
        MESSAGE.as_ptr() as u64,
        MESSAGE.len() as u64,
    );
    exit(if result == MESSAGE.len() as i64 { 0 } else { 8 })
}

fn initial_stack_is_valid(initial_stack: *const u64) -> bool {
    let address = initial_stack as u64;
    let Some(end) = address.checked_add((INITIAL_STACK_WORDS * size_of::<u64>()) as u64) else {
        return false;
    };
    if address & 15 != 0 || address < USER_STACK_BASE || end > USER_STACK_TOP {
        return false;
    }
    // SAFETY: the range above is bounded to the single user stack page mapped
    // by the kernel before entry.
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
    if address < USER_STACK_BASE || end > USER_STACK_TOP {
        return false;
    }
    // SAFETY: address..end was bounded to the mapped stack page.
    let actual = unsafe { core::slice::from_raw_parts(address as *const u8, expected.len() + 1) };
    actual[..expected.len()] == *expected && actual[expected.len()] == 0
}

fn seek(fd: i64, offset: u64) -> bool {
    syscall3(SYS_LSEEK, fd as u64, offset, SEEK_SET) == offset as i64
}

fn syscall1(number: u64, first: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the architectural x86-64 SYSCALL MSRs with the
    // Linux register convention before entering this executable.
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

fn syscall3(number: u64, first: u64, second: u64, third: u64) -> i64 {
    let result: i64;
    // SAFETY: SlopOS configures the architectural x86-64 SYSCALL MSRs with the
    // Linux register convention before entering this executable.
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
    // SAFETY: SlopOS configures the architectural x86-64 SYSCALL MSRs with the
    // Linux register convention before entering this executable.
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
    // SAFETY: syscall 60 terminates the current SlopOS process and does not
    // return to this instruction stream.
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
    exit(10)
}
