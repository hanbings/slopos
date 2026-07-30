// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

use core::arch::asm;
use core::panic::PanicInfo;

const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 60;
const STDOUT: u64 = 1;
static MESSAGE: &[u8; 18] = b"SLOPOS user write\n";

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text._start")]
pub extern "C" fn _start() -> ! {
    let result: i64;
    // SAFETY: SlopOS configures the architectural x86-64 SYSCALL MSRs with the
    // Linux register convention before entering this executable.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") SYS_WRITE => result,
            in("rdi") STDOUT,
            in("rsi") MESSAGE.as_ptr(),
            in("rdx") MESSAGE.len(),
            out("rcx") _,
            out("r11") _,
        );
    }
    exit(if result == MESSAGE.len() as i64 { 0 } else { 1 })
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
