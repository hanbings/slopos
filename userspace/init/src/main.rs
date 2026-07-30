// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_EXIT: u64 = 60;
const SYS_OPENAT: u64 = 257;
const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const STDOUT: u64 = 1;
const EXPECTED_FD: i64 = 3;
static MESSAGE: &[u8; 18] = b"SLOPOS user write\n";
static CONFIG_PATH: &[u8; 24] = b"/etc/slopos/system.conf\0";
static EXPECTED_CONFIG: &[u8; 76] =
    b"# SlopOS declarative configuration seed\ntheme = \"ocean\"\nhostname = \"slopos\"\n";

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text._start")]
pub extern "C" fn _start() -> ! {
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
    let result = syscall3(
        SYS_WRITE,
        STDOUT,
        MESSAGE.as_ptr() as u64,
        MESSAGE.len() as u64,
    );
    exit(if result == MESSAGE.len() as i64 { 0 } else { 4 })
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
    exit(2)
}
