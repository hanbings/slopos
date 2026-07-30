// SPDX-License-Identifier: 0BSD

use core::arch::global_asm;
use core::sync::atomic::{AtomicU8, Ordering};

const PID: u32 = 1;
const USER_STDOUT: u64 = 1;
const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_EXIT: u64 = 60;
const USER_MESSAGE_OFFSET: usize = 0x80;
const USER_MESSAGE: &[u8] = b"SLOPOS user write\n";
const USER_MESSAGE_ADDRESS: u64 = crate::paging::USER_CODE_BASE + USER_MESSAGE_OFFSET as u64;
const USER_PROGRAM: [u8; 58] = [
    0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1 (write)
    0xbf, 0x01, 0x00, 0x00, 0x00, // mov edi, 1 (stdout)
    0x48, 0xbe, 0x80, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, // mov rsi, message
    0xba, 0x12, 0x00, 0x00, 0x00, // mov edx, 18
    0xcd, 0x80, // int 0x80
    0x48, 0x83, 0xf8, 0x12, // cmp rax, 18
    0x75, 0x0b, // jne failure
    0xb8, 0x3c, 0x00, 0x00, 0x00, // mov eax, 60 (exit)
    0x31, 0xff, // xor edi, edi
    0xcd, 0x80, // int 0x80
    0x0f, 0x0b, // ud2 if exit returned
    0xb8, 0x3c, 0x00, 0x00, 0x00, // failure: mov eax, 60
    0xbf, 0x01, 0x00, 0x00, 0x00, // mov edi, 1
    0xcd, 0x80, // int 0x80
    0x0f, 0x0b, // ud2
];

static SYSCALL_STATE: AtomicU8 = AtomicU8::new(0);

const fn user_image() -> [u8; 4096] {
    let mut image = [0u8; 4096];
    let mut index = 0;
    while index < USER_PROGRAM.len() {
        image[index] = USER_PROGRAM[index];
        index += 1;
    }
    index = 0;
    while index < USER_MESSAGE.len() {
        image[USER_MESSAGE_OFFSET + index] = USER_MESSAGE[index];
        index += 1;
    }
    image
}

static USER_IMAGE: [u8; 4096] = user_image();

#[repr(C)]
struct SyscallFrame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

pub fn run_probe() {
    SYSCALL_STATE.store(0, Ordering::Release);
    let address_space = crate::paging::create_user_address_space(&USER_IMAGE);
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={PID} address_space={:#x} user_code={:#x} user_stack={:#x} code_frame={:#x} stack_frame={:#x} code=user-readonly stack=user-writable kernel=supervisor",
        address_space.root,
        crate::paging::USER_CODE_BASE,
        crate::paging::USER_STACK_TOP,
        address_space.code_frame,
        address_space.stack_frame
    ));
    // SAFETY: the process page table maps a validated one-page program and
    // stack under user permissions; GDT, TSS and the DPL3 syscall gate are live.
    unsafe {
        slopos_enter_user(
            address_space.root,
            crate::paging::USER_CODE_BASE,
            crate::paging::USER_STACK_TOP,
        );
    }
    if SYSCALL_STATE.load(Ordering::Acquire) != 2 {
        crate::fatal("user process returned without a successful exit syscall");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={PID} exited status=0 syscalls=2 kernel_return=true"
    ));
}

#[unsafe(no_mangle)]
extern "C" fn slopos_syscall_handler(frame: &mut SyscallFrame) -> u64 {
    if frame.cs & 3 != 3 || frame.ss & 3 != 3 {
        crate::fatal("syscall trap did not originate at CPL3");
    }
    match frame.rax {
        LINUX_SYS_WRITE => {
            if frame.rdi != USER_STDOUT
                || frame.rsi != USER_MESSAGE_ADDRESS
                || frame.rdx != USER_MESSAGE.len() as u64
            {
                crate::fatal("user write syscall arguments are invalid");
            }
            // SAFETY: the exact checked range is wholly inside the mapped user
            // code page and cannot cross a page boundary.
            let message =
                unsafe { core::slice::from_raw_parts(frame.rsi as *const u8, USER_MESSAGE.len()) };
            if message != USER_MESSAGE
                || SYSCALL_STATE
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                crate::fatal("user write syscall payload or ordering is invalid");
            }
            frame.rax = USER_MESSAGE.len() as u64;
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=x86_64 trap=int80 nr=1 write fd=1 bytes={} cpl=3 result={}",
                USER_MESSAGE.len(),
                USER_MESSAGE.len()
            ));
            0
        }
        LINUX_SYS_EXIT => {
            if frame.rdi != 0
                || SYSCALL_STATE
                    .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                crate::fatal("user exit syscall status or ordering is invalid");
            }
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=x86_64 trap=int80 nr=60 exit status=0 cpl=3"
            ));
            1
        }
        _ => {
            frame.rax = (-38i64) as u64;
            0
        }
    }
}

unsafe extern "C" {
    fn slopos_enter_user(root: u64, entry: u64, stack_top: u64);
}

global_asm!(
    r#"
    .section .bss
    .align 8
slopos_user_kernel_rsp:
    .quad 0
slopos_user_kernel_cr3:
    .quad 0

    .section .text
    .global slopos_enter_user
    .type slopos_enter_user, @function
slopos_enter_user:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rip + slopos_user_kernel_rsp], rsp
    mov rax, cr3
    mov [rip + slopos_user_kernel_cr3], rax
    mov cr3, rdi
    push 0x1b
    push rdx
    push 0x202
    push 0x23
    push rsi
    iretq
    .size slopos_enter_user, .-slopos_enter_user

    .global slopos_syscall_interrupt
    .type slopos_syscall_interrupt, @function
slopos_syscall_interrupt:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15
    cld
    mov rdi, rsp
    call slopos_syscall_handler
    test rax, rax
    jnz slopos_user_exit
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

slopos_user_exit:
    mov rax, [rip + slopos_user_kernel_cr3]
    mov cr3, rax
    mov rsp, [rip + slopos_user_kernel_rsp]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
    .size slopos_syscall_interrupt, .-slopos_syscall_interrupt
"#
);
