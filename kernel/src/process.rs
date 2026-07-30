// SPDX-License-Identifier: 0BSD

use core::arch::global_asm;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

const PID: u32 = 1;
const USER_STDOUT: u64 = 1;
const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_EXIT: u64 = 60;
const USER_MESSAGE: &[u8] = b"SLOPOS user write\n";

static SYSCALL_STATE: AtomicU8 = AtomicU8::new(0);
static USER_MEMORY_END: AtomicU64 = AtomicU64::new(0);

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

pub fn run_probe(user_image: &[u8]) {
    SYSCALL_STATE.store(0, Ordering::Release);
    let elf = slopos_elf::ElfFile::parse(user_image)
        .unwrap_or_else(|_| crate::fatal("boot user ELF failed validation"));
    if elf.load_segment_count() != 1 {
        crate::fatal("boot user ELF has an unexpected PT_LOAD count");
    }
    let segment = elf
        .load_segments()
        .next()
        .unwrap_or_else(|| crate::fatal("boot user ELF has no PT_LOAD segment"));
    if segment.virtual_address() != crate::paging::USER_CODE_BASE
        || segment.memory_size() == 0
        || segment.memory_size() > 4096
        || !segment.readable()
        || segment.writable()
        || !segment.executable()
    {
        crate::fatal("boot user ELF segment policy mismatch");
    }
    USER_MEMORY_END.store(
        crate::paging::USER_CODE_BASE + segment.memory_size(),
        Ordering::Release,
    );
    let address_space =
        crate::paging::create_user_address_space(segment.data(), segment.memory_size());
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={PID} source=boot format=elf64 entry={:#x} segments={} file_bytes={} load_bytes={} memory_bytes={} address_space={:#x} user_code={:#x} user_stack={:#x} code_frame={:#x} stack_frame={:#x} code=user-readonly stack=user-writable kernel=supervisor",
        elf.entry(),
        elf.load_segment_count(),
        user_image.len(),
        segment.file_size(),
        segment.memory_size(),
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
            elf.entry(),
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
            if frame.rdi != USER_STDOUT || frame.rdx != USER_MESSAGE.len() as u64 {
                crate::fatal("user write syscall arguments are invalid");
            }
            let message = user_bytes(frame.rsi, USER_MESSAGE.len())
                .unwrap_or_else(|| crate::fatal("user write range is outside PT_LOAD memory"));
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

fn user_bytes(address: u64, length: usize) -> Option<&'static [u8]> {
    let length = u64::try_from(length).ok()?;
    let end = address.checked_add(length)?;
    if address < crate::paging::USER_CODE_BASE
        || end > USER_MEMORY_END.load(Ordering::Acquire)
        || end < address
    {
        return None;
    }
    // SAFETY: the range was checked against the validated PT_LOAD mapping, and
    // the process frame remains allocated for the lifetime of this boot.
    Some(unsafe { core::slice::from_raw_parts(address as *const u8, length as usize) })
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
