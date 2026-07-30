// SPDX-License-Identifier: 0BSD

use core::arch::x86_64::__cpuid;
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use slopos_process::{ProcessImage, ProcessState, ProcessTable};

const PID: u32 = 1;
const PROCESS_CAPACITY: usize = 4;
const PROCESS_FD_CAPACITY: usize = 8;
const USER_STDOUT: u64 = 1;
const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_EXIT: u64 = 60;
const USER_MESSAGE: &[u8] = b"SLOPOS user write\n";
const PAGE_SIZE: u64 = 4096;
const IA32_EFER: u32 = 0xc000_0080;
const IA32_STAR: u32 = 0xc000_0081;
const IA32_LSTAR: u32 = 0xc000_0082;
const IA32_FMASK: u32 = 0xc000_0084;
const EFER_SYSCALL_ENABLE: u64 = 1;
const KERNEL_CODE_SELECTOR: u64 = 0x08;
const SYSRET_SELECTOR_BASE: u64 = 0x10;
const STAR_VALUE: u64 = (SYSRET_SELECTOR_BASE << 48) | (KERNEL_CODE_SELECTOR << 32);
const RFLAGS_RESERVED_ONE: u64 = 1 << 1;
const RFLAGS_INTERRUPT_ENABLE: u64 = 1 << 9;
const RFLAGS_SYSCALL_MASK: u64 =
    (1 << 8) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 14) | (1 << 18);
const RFLAGS_USER_CLEAR: u64 = (3 << 12) | (1 << 14) | (1 << 16) | (1 << 17);

type KernelProcessTable = ProcessTable<PROCESS_CAPACITY, PROCESS_FD_CAPACITY>;

struct ProcessTableStorage(UnsafeCell<KernelProcessTable>);

// The bootstrap processor is the only process-table owner in this synchronous
// milestone. Syscall entry runs with IF masked and cannot overlap mutation.
unsafe impl Sync for ProcessTableStorage {}

static PROCESS_TABLE: ProcessTableStorage =
    ProcessTableStorage(UnsafeCell::new(KernelProcessTable::new()));

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
    user_rip: u64,
    user_rflags: u64,
    user_rsp: u64,
}

pub fn run_probe(user_image: &[u8]) {
    reset_process_table();
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
    let address_space =
        crate::paging::create_user_address_space(segment.data(), segment.memory_size());
    let image = ProcessImage {
        address_space_root: address_space.root,
        entry: elf.entry(),
        stack_top: crate::paging::USER_STACK_TOP,
        user_memory_start: crate::paging::USER_CODE_BASE,
        user_memory_end: crate::paging::USER_CODE_BASE + segment.memory_size(),
    };
    let pid = process_table_mut()
        .spawn(None, image)
        .unwrap_or_else(|_| crate::fatal("PID 1 process-table insertion failed"));
    if pid != PID {
        crate::fatal("bootstrap process did not receive PID 1");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: table initialized capacity={PROCESS_CAPACITY} pid={PID} state=ready fd_capacity={PROCESS_FD_CAPACITY} per_process_fds=true"
    ));
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
    configure_fast_syscall();
    process_table_mut()
        .mark_running(PID)
        .unwrap_or_else(|_| crate::fatal("PID 1 ready-to-running transition failed"));
    // SAFETY: the process page table maps a validated one-page program and
    // stack under user permissions; GDT, TSS and syscall MSRs are live.
    unsafe {
        slopos_enter_user(
            address_space.root,
            elf.entry(),
            crate::paging::USER_STACK_TOP,
        );
    }
    let exited = process_table()
        .snapshot(PID)
        .unwrap_or_else(|_| crate::fatal("PID 1 disappeared from the process table"));
    if exited.state != ProcessState::Exited
        || exited.exit_status != Some(0)
        || exited.syscall_count != 2
    {
        crate::fatal("PID 1 returned without a successful process-table exit");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={PID} state=exited status=0 syscalls=2 retained=true kernel_return=true"
    ));
}

#[unsafe(no_mangle)]
extern "C" fn slopos_syscall_handler(frame: &mut SyscallFrame) -> u64 {
    let process = process_table()
        .snapshot(PID)
        .unwrap_or_else(|_| crate::fatal("syscall has no current process"));
    if process.state != ProcessState::Running
        || frame.user_rip < process.image.user_memory_start
        || frame.user_rip >= process.image.user_memory_end
        || frame.user_rsp < process.image.stack_top - PAGE_SIZE
        || frame.user_rsp > process.image.stack_top
        || frame.user_rflags & RFLAGS_RESERVED_ONE == 0
    {
        crate::fatal("syscall user context failed validation");
    }
    frame.user_rflags &= !RFLAGS_USER_CLEAR;
    frame.user_rflags |= RFLAGS_RESERVED_ONE | RFLAGS_INTERRUPT_ENABLE;
    process_table_mut()
        .record_syscall(PID)
        .unwrap_or_else(|_| crate::fatal("syscall process accounting failed"));
    match frame.rax {
        LINUX_SYS_WRITE => {
            if frame.rdi != USER_STDOUT || frame.rdx != USER_MESSAGE.len() as u64 {
                crate::fatal("user write syscall arguments are invalid");
            }
            let message = user_bytes(&process.image, frame.rsi, USER_MESSAGE.len())
                .unwrap_or_else(|| crate::fatal("user write range is outside PT_LOAD memory"));
            if message != USER_MESSAGE {
                crate::fatal("user write syscall payload is invalid");
            }
            frame.rax = USER_MESSAGE.len() as u64;
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=sysretq nr=1 write fd=1 bytes={} origin=cpl3 result={}",
                USER_MESSAGE.len(),
                USER_MESSAGE.len()
            ));
            0
        }
        LINUX_SYS_EXIT => {
            if frame.rdi > u64::from(u8::MAX) {
                crate::fatal("user exit syscall status is invalid");
            }
            let status = frame.rdi as i32;
            process_table_mut()
                .exit(PID, status)
                .unwrap_or_else(|_| crate::fatal("process-table exit transition failed"));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=kernel nr=60 exit status={status} origin=cpl3"
            ));
            1
        }
        _ => {
            frame.rax = (-38i64) as u64;
            0
        }
    }
}

fn user_bytes(image: &ProcessImage, address: u64, length: usize) -> Option<&'static [u8]> {
    let length = u64::try_from(length).ok()?;
    let end = address.checked_add(length)?;
    if address < image.user_memory_start || end > image.user_memory_end || end < address {
        return None;
    }
    // SAFETY: the range was checked against the validated PT_LOAD mapping, and
    // the process frame remains allocated for the lifetime of this boot.
    Some(unsafe { core::slice::from_raw_parts(address as *const u8, length as usize) })
}

fn reset_process_table() {
    // SAFETY: this runs once before PID 1 starts and no reference to the old
    // empty table exists.
    unsafe { PROCESS_TABLE.0.get().write(KernelProcessTable::new()) };
}

fn process_table() -> &'static KernelProcessTable {
    // SAFETY: process execution is single-core; mutation only occurs before
    // entry or inside the IF-masked syscall handler.
    unsafe { &*PROCESS_TABLE.0.get() }
}

fn process_table_mut() -> &'static mut KernelProcessTable {
    // SAFETY: the bootstrap path and syscall handler are exclusive writers in
    // this milestone, and syscall entry masks interrupts before Rust executes.
    unsafe { &mut *PROCESS_TABLE.0.get() }
}

fn configure_fast_syscall() {
    // SAFETY: CPUID is available on x86-64 and has no memory side effects.
    let maximum_extended = unsafe { __cpuid(0x8000_0000) }.eax;
    if maximum_extended < 0x8000_0001 {
        crate::fatal("processor has no extended syscall capability leaf");
    }
    // SAFETY: the extended feature leaf was checked above.
    let features = unsafe { __cpuid(0x8000_0001) };
    if features.edx & (1 << 11) == 0 {
        crate::fatal("processor does not support SYSCALL/SYSRET");
    }
    let entry = slopos_syscall_entry as usize as u64;
    // SAFETY: these architectural MSRs are present when CPUID advertises the
    // syscall extension; selectors match the live GDT layout.
    unsafe {
        write_msr(IA32_STAR, STAR_VALUE);
        write_msr(IA32_LSTAR, entry);
        write_msr(IA32_FMASK, RFLAGS_SYSCALL_MASK);
        write_msr(IA32_EFER, read_msr(IA32_EFER) | EFER_SYSCALL_ENABLE);
    }
    // SAFETY: readback of the same architectural MSRs is side-effect free.
    let (efer, star, lstar, fmask) = unsafe {
        (
            read_msr(IA32_EFER),
            read_msr(IA32_STAR),
            read_msr(IA32_LSTAR),
            read_msr(IA32_FMASK),
        )
    };
    if efer & EFER_SYSCALL_ENABLE == 0
        || star != STAR_VALUE
        || lstar != entry
        || fmask != RFLAGS_SYSCALL_MASK
    {
        crate::fatal("SYSCALL MSR configuration did not persist");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-SYSCALL: fast path ready instruction=syscall return=sysretq star={star:#x} lstar={lstar:#x} fmask={fmask:#x} efer_sce=true"
    ));
}

unsafe fn read_msr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: caller checked the architectural MSR's availability.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        )
    };
    (u64::from(high) << 32) | u64::from(low)
}

unsafe fn write_msr(msr: u32, value: u64) {
    // SAFETY: caller checked the architectural MSR's availability and value.
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags)
        )
    };
}

unsafe extern "C" {
    fn slopos_enter_user(root: u64, entry: u64, stack_top: u64);
    fn slopos_syscall_entry();
}

global_asm!(
    r#"
    .section .bss
    .align 8
slopos_user_kernel_rsp:
    .quad 0
slopos_user_kernel_cr3:
    .quad 0
slopos_user_rsp:
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

    .global slopos_syscall_entry
    .type slopos_syscall_entry, @function
slopos_syscall_entry:
    mov [rip + slopos_user_rsp], rsp
    mov rsp, [rip + slopos_user_kernel_rsp]
    push qword ptr [rip + slopos_user_rsp]
    push r11
    push rcx
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
    mov rdx, rsp
    and rsp, -16
    sub rsp, 16
    mov [rsp], rdx
    call slopos_syscall_handler
    mov rsp, [rsp]
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
    pop rcx
    pop r11
    pop rsp
    sysretq

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
    .size slopos_syscall_entry, .-slopos_syscall_entry
"#
);
