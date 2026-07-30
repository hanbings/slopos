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
    // SAFETY: SlopOS installs a DPL3 trap gate with the Linux x86-64 register
    // convention before entering this executable.
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") SYS_WRITE => result,
            in("rdi") STDOUT,
            in("rsi") MESSAGE.as_ptr(),
            in("rdx") MESSAGE.len(),
            options(nostack)
        );
    }
    exit(if result == MESSAGE.len() as i64 { 0 } else { 1 })
}

fn exit(status: u64) -> ! {
    // SAFETY: syscall 60 terminates the current SlopOS process and does not
    // return to this instruction stream.
    unsafe {
        asm!(
            "int 0x80",
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
