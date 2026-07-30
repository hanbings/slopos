// SPDX-License-Identifier: 0BSD

use core::arch::x86_64::__cpuid;
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::ptr;
use slopos_process::{ProcessError, ProcessImage, ProcessState, ProcessTable};
use slopos_vfs::{AccessMode, FileNode, ReadWindow};

const PID: u32 = 1;
const PROCESS_CAPACITY: usize = 4;
const PROCESS_FD_CAPACITY: usize = 8;
const PROCESS_EXPECTED_SYSCALLS: u64 = 5;
const PROCESS_SYSCALL_PATH_CAPACITY: usize = 128;
pub const PROCESS_SYSCALL_IO_CAPACITY: usize = 256;
const LINUX_AT_FDCWD: u64 = (-100i64) as u64;
const LINUX_O_RDONLY: u64 = 0;
const LINUX_SYS_READ: u64 = 0;
const USER_STDOUT: u64 = 1;
const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_CLOSE: u64 = 3;
const LINUX_SYS_EXIT: u64 = 60;
const LINUX_SYS_OPENAT: u64 = 257;
const LINUX_EBADF: i64 = -9;
const LINUX_EFAULT: i64 = -14;
const LINUX_EINVAL: i64 = -22;
const LINUX_ENAMETOOLONG: i64 = -36;
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

// The bootstrap processor is the only process-table owner. Syscall entry runs
// with IF masked; async completion mutates only while PID 1 is suspended.
unsafe impl Sync for ProcessTableStorage {}

static PROCESS_TABLE: ProcessTableStorage =
    ProcessTableStorage(UnsafeCell::new(KernelProcessTable::new()));

#[derive(Clone, Copy)]
struct UserMapping {
    code_frame: u64,
    stack_frame: u64,
    code_start: u64,
    code_end: u64,
    stack_start: u64,
    stack_end: u64,
}

struct UserMappingStorage(UnsafeCell<Option<UserMapping>>);

// The mapping is installed before entering PID 1 and is immutable while the
// process runs or a suspended syscall is completed by the block task.
unsafe impl Sync for UserMappingStorage {}

static USER_MAPPING: UserMappingStorage = UserMappingStorage(UnsafeCell::new(None));

#[derive(Clone, Copy)]
pub struct OpenAtRequest {
    path: [u8; PROCESS_SYSCALL_PATH_CAPACITY],
    path_length: usize,
}

impl OpenAtRequest {
    pub fn path(&self) -> &[u8] {
        &self.path[..self.path_length]
    }
}

#[derive(Clone, Copy)]
pub struct ReadRequest {
    pub fd: u32,
    pub requested: usize,
    destination: u64,
}

#[derive(Clone, Copy)]
pub struct CloseRequest {
    pub fd: u32,
}

#[derive(Clone, Copy)]
enum PendingSyscall {
    OpenAt(OpenAtRequest),
    Read(ReadRequest),
    Close(CloseRequest),
}

#[derive(Clone, Copy)]
pub enum ProcessEvent {
    OpenAt(OpenAtRequest),
    Read(ReadRequest),
    Close(CloseRequest),
    Exited,
}

struct PendingSyscallStorage(UnsafeCell<Option<PendingSyscall>>);

// PID 1 is the only user process. A pending request is written by the
// IF-masked fast entry and completed only after control returns to block task.
unsafe impl Sync for PendingSyscallStorage {}

static PENDING_SYSCALL: PendingSyscallStorage = PendingSyscallStorage(UnsafeCell::new(None));

#[repr(C)]
#[derive(Clone, Copy)]
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

struct SyscallFrameStorage(UnsafeCell<SyscallFrame>);

// The saved frame belongs to the single suspended PID 1 and is never accessed
// concurrently with its user-mode execution.
unsafe impl Sync for SyscallFrameStorage {}

static SAVED_SYSCALL_FRAME: SyscallFrameStorage =
    SyscallFrameStorage(UnsafeCell::new(SyscallFrame {
        r15: 0,
        r14: 0,
        r13: 0,
        r12: 0,
        r11: 0,
        r10: 0,
        r9: 0,
        r8: 0,
        rbp: 0,
        rdi: 0,
        rsi: 0,
        rdx: 0,
        rcx: 0,
        rbx: 0,
        rax: 0,
        user_rip: 0,
        user_rflags: 0,
        user_rsp: 0,
    }));

pub fn start_probe(user_image: &[u8], source: &str, path: &str) -> ProcessEvent {
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
    set_user_mapping(UserMapping {
        code_frame: address_space.code_frame,
        stack_frame: address_space.stack_frame,
        code_start: crate::paging::USER_CODE_BASE,
        code_end: crate::paging::USER_CODE_BASE + segment.memory_size(),
        stack_start: crate::paging::USER_STACK_TOP - PAGE_SIZE,
        stack_end: crate::paging::USER_STACK_TOP,
    });
    clear_pending_syscall();
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
        "SLOPOS-PROCESS: pid={PID} source={source} path={path} format=elf64 entry={:#x} segments={} file_bytes={} load_bytes={} memory_bytes={} address_space={:#x} user_code={:#x} user_stack={:#x} code_frame={:#x} stack_frame={:#x} code=user-readonly stack=user-writable kernel=supervisor",
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
    process_event_after_user()
}

pub fn open_file(node: FileNode) -> Result<u32, ProcessError> {
    process_table_mut().open_file(PID, node, AccessMode::ReadOnly)
}

pub fn read_window(fd: u32, requested: usize) -> Result<ReadWindow, ProcessError> {
    process_table().read_window(PID, fd, requested)
}

pub fn advance_fd(fd: u32, length: usize) -> Result<(), ProcessError> {
    process_table_mut().advance_fd(PID, fd, length)
}

pub fn close_fd(fd: u32) -> Result<(), ProcessError> {
    process_table_mut().close_fd(PID, fd)
}

pub fn resume_probe(result: i64, read_output: Option<&[u8]>) -> ProcessEvent {
    let pending =
        pending_syscall().unwrap_or_else(|| crate::fatal("process resume has no pending syscall"));
    match pending {
        PendingSyscall::Read(request) => {
            if result < 0 {
                if read_output.is_some_and(|output| !output.is_empty()) {
                    crate::fatal("failed read completion carried output bytes");
                }
            } else {
                let length = usize::try_from(result)
                    .unwrap_or_else(|_| crate::fatal("read completion length overflow"));
                let output = read_output
                    .unwrap_or_else(|| crate::fatal("read completion omitted output bytes"));
                if length > request.requested || output.len() != length {
                    crate::fatal("read completion output length mismatch");
                }
                copy_to_user(request.destination, output)
                    .unwrap_or_else(|| crate::fatal("read completion user buffer became invalid"));
            }
        }
        PendingSyscall::OpenAt(_) | PendingSyscall::Close(_) => {
            if read_output.is_some() {
                crate::fatal("non-read completion carried output bytes");
            }
        }
    }
    clear_pending_syscall();
    // SAFETY: PID 1 is suspended in the saved fast-syscall frame, and the
    // block task is its exclusive completer.
    let frame = unsafe { &mut *SAVED_SYSCALL_FRAME.0.get() };
    frame.rax = result as u64;
    let process = process_table()
        .snapshot(PID)
        .unwrap_or_else(|_| crate::fatal("resumed process disappeared"));
    if process.state != ProcessState::Running {
        crate::fatal("only a running process can resume from an async syscall");
    }
    // SAFETY: the frame was captured by the IF-masked fast entry, its user
    // ranges were validated, and the process page table remains live.
    unsafe {
        slopos_resume_user(process.image.address_space_root, frame);
    }
    process_event_after_user()
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
        LINUX_SYS_OPENAT => {
            if frame.rdi != LINUX_AT_FDCWD || frame.rdx != LINUX_O_RDONLY || frame.r10 != 0 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            let request = match copy_user_path(frame.rsi) {
                Ok(request) => request,
                Err(errno) => {
                    frame.rax = errno as u64;
                    return 0;
                }
            };
            let display = core::str::from_utf8(request.path()).unwrap_or("<non-utf8>");
            suspend_syscall(frame, PendingSyscall::OpenAt(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=suspended nr=257 openat dirfd=-100 flags=0 path={display} origin=cpl3"
            ));
            2
        }
        LINUX_SYS_READ => {
            let Ok(fd) = u32::try_from(frame.rdi) else {
                frame.rax = LINUX_EBADF as u64;
                return 0;
            };
            let requested = usize::try_from(frame.rdx)
                .unwrap_or(usize::MAX)
                .min(PROCESS_SYSCALL_IO_CAPACITY);
            if requested == 0 {
                frame.rax = 0;
                return 0;
            }
            if user_physical_pointer(frame.rsi, requested, true).is_none() {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            suspend_syscall(
                frame,
                PendingSyscall::Read(ReadRequest {
                    fd,
                    requested,
                    destination: frame.rsi,
                }),
            );
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=suspended nr=0 read fd={fd} requested={requested} origin=cpl3"
            ));
            2
        }
        LINUX_SYS_WRITE => {
            if frame.rdi != USER_STDOUT {
                frame.rax = LINUX_EBADF as u64;
                return 0;
            }
            if frame.rdx != USER_MESSAGE.len() as u64 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            let message = user_bytes(frame.rsi, USER_MESSAGE.len())
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
        LINUX_SYS_CLOSE => {
            let Ok(fd) = u32::try_from(frame.rdi) else {
                frame.rax = LINUX_EBADF as u64;
                return 0;
            };
            suspend_syscall(frame, PendingSyscall::Close(CloseRequest { fd }));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=suspended nr=3 close fd={fd} origin=cpl3"
            ));
            2
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

fn process_event_after_user() -> ProcessEvent {
    let process = process_table()
        .snapshot(PID)
        .unwrap_or_else(|_| crate::fatal("PID 1 disappeared from the process table"));
    if process.state == ProcessState::Exited {
        if process.exit_status != Some(0)
            || process.syscall_count != PROCESS_EXPECTED_SYSCALLS
            || pending_syscall().is_some()
        {
            crate::fatal("PID 1 returned without a successful process-table exit");
        }
        crate::serial::serialln(format_args!(
            "SLOPOS-PROCESS: pid={PID} state=exited status=0 syscalls={PROCESS_EXPECTED_SYSCALLS} retained=true kernel_return=true"
        ));
        return ProcessEvent::Exited;
    }
    match pending_syscall()
        .unwrap_or_else(|| crate::fatal("running PID 1 returned without a pending syscall"))
    {
        PendingSyscall::OpenAt(request) => ProcessEvent::OpenAt(request),
        PendingSyscall::Read(request) => ProcessEvent::Read(request),
        PendingSyscall::Close(request) => ProcessEvent::Close(request),
    }
}

fn suspend_syscall(frame: &SyscallFrame, request: PendingSyscall) {
    if pending_syscall().is_some() {
        crate::fatal("process issued a second syscall while one was pending");
    }
    // SAFETY: syscall entry is IF-masked and is the only writer until it
    // returns to the block task.
    unsafe {
        SAVED_SYSCALL_FRAME.0.get().write(*frame);
        PENDING_SYSCALL.0.get().write(Some(request));
    }
}

fn copy_user_path(address: u64) -> Result<OpenAtRequest, i64> {
    let mut request = OpenAtRequest {
        path: [0; PROCESS_SYSCALL_PATH_CAPACITY],
        path_length: 0,
    };
    for index in 0..PROCESS_SYSCALL_PATH_CAPACITY {
        let index = u64::try_from(index).map_err(|_| LINUX_EFAULT)?;
        let byte_address = address.checked_add(index).ok_or(LINUX_EFAULT)?;
        let byte = user_bytes(byte_address, 1)
            .and_then(|bytes| bytes.first().copied())
            .ok_or(LINUX_EFAULT)?;
        if byte == 0 {
            if request.path_length == 0 {
                return Err(LINUX_EINVAL);
            }
            return Ok(request);
        }
        request.path[index as usize] = byte;
        request.path_length += 1;
    }
    Err(LINUX_ENAMETOOLONG)
}

fn user_bytes(address: u64, length: usize) -> Option<&'static [u8]> {
    let pointer = user_physical_pointer(address, length, false)?;
    // SAFETY: the translated physical frame is permanently identity-mapped and
    // the requested range was bounded to one live user page.
    Some(unsafe { core::slice::from_raw_parts(pointer.cast_const(), length) })
}

fn copy_to_user(address: u64, bytes: &[u8]) -> Option<()> {
    let pointer = user_physical_pointer(address, bytes.len(), true)?;
    // SAFETY: the destination was bounded to the writable user stack frame,
    // which remains exclusively owned by suspended PID 1.
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len()) };
    Some(())
}

fn user_physical_pointer(address: u64, length: usize, writable: bool) -> Option<*mut u8> {
    let mapping = user_mapping()?;
    let length = u64::try_from(length).ok()?;
    let end = address.checked_add(length)?;
    let (virtual_start, physical_start) =
        if !writable && address >= mapping.code_start && end <= mapping.code_end {
            (mapping.code_start, mapping.code_frame)
        } else if address >= mapping.stack_start && end <= mapping.stack_end {
            (mapping.stack_start, mapping.stack_frame)
        } else {
            return None;
        };
    let offset = address.checked_sub(virtual_start)?;
    let physical = physical_start.checked_add(offset)?;
    Some(physical as *mut u8)
}

fn set_user_mapping(mapping: UserMapping) {
    // SAFETY: installed once before PID 1 starts, while no user-copy operation
    // can overlap.
    unsafe { USER_MAPPING.0.get().write(Some(mapping)) };
}

fn user_mapping() -> Option<UserMapping> {
    // SAFETY: immutable after installation for this single-process milestone.
    unsafe { *USER_MAPPING.0.get() }
}

fn pending_syscall() -> Option<PendingSyscall> {
    // SAFETY: access alternates between IF-masked syscall entry and the block
    // task while PID 1 is suspended.
    unsafe { *PENDING_SYSCALL.0.get() }
}

fn clear_pending_syscall() {
    // SAFETY: called before first entry or by the exclusive block-task
    // completer while PID 1 is suspended.
    unsafe { PENDING_SYSCALL.0.get().write(None) };
}

fn reset_process_table() {
    // SAFETY: this runs once before PID 1 starts and no reference to the old
    // empty table exists.
    unsafe { PROCESS_TABLE.0.get().write(KernelProcessTable::new()) };
}

fn process_table() -> &'static KernelProcessTable {
    // SAFETY: process execution is single-core; mutation only occurs before
    // entry, inside the IF-masked handler, or while PID 1 is suspended.
    unsafe { &*PROCESS_TABLE.0.get() }
}

fn process_table_mut() -> &'static mut KernelProcessTable {
    // SAFETY: the bootstrap path, IF-masked syscall handler, and block-task
    // syscall completer are mutually exclusive writers in this milestone.
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
    fn slopos_resume_user(root: u64, frame: *const SyscallFrame);
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

    .global slopos_resume_user
    .type slopos_resume_user, @function
slopos_resume_user:
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
    mov rax, rsi
    mov r15, [rax + 0]
    mov r14, [rax + 8]
    mov r13, [rax + 16]
    mov r12, [rax + 24]
    mov r10, [rax + 40]
    mov r9, [rax + 48]
    mov r8, [rax + 56]
    mov rbp, [rax + 64]
    mov rdi, [rax + 72]
    mov rsi, [rax + 80]
    mov rdx, [rax + 88]
    mov rbx, [rax + 104]
    mov rcx, [rax + 120]
    mov r11, [rax + 128]
    mov rsp, [rax + 136]
    mov rax, [rax + 112]
    sysretq
    .size slopos_resume_user, .-slopos_resume_user

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
