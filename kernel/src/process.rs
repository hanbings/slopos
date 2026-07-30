// SPDX-License-Identifier: 0BSD

use core::arch::x86_64::__cpuid;
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::ptr;
use slopos_process::{
    ProcessError, ProcessImage, ProcessState, ProcessTable, build_linux_initial_stack,
};
use slopos_vfs::{AccessMode, FileNode, ReadWindow, VfsError, WriteWindow};

const PID: u32 = 1;
const PROCESS_CAPACITY: usize = 4;
const PROCESS_FD_CAPACITY: usize = 8;
const PROCESS_EXPECTED_SYSCALLS: u64 = 14;
const PROCESS_SYSCALL_PATH_CAPACITY: usize = 128;
pub const PROCESS_SYSCALL_IO_CAPACITY: usize = 256;
const LINUX_AT_FDCWD: u64 = (-100i64) as u64;
const LINUX_O_RDONLY: u64 = 0;
const LINUX_O_RDWR: u64 = 2;
const LINUX_SYS_READ: u64 = 0;
const USER_STDOUT: u64 = 1;
const LINUX_SYS_WRITE: u64 = 1;
const LINUX_SYS_CLOSE: u64 = 3;
const LINUX_SYS_LSEEK: u64 = 8;
const LINUX_SYS_EXIT: u64 = 60;
const LINUX_SYS_OPENAT: u64 = 257;
const LINUX_SEEK_SET: u64 = 0;
const LINUX_EBADF: i64 = -9;
const LINUX_EFAULT: i64 = -14;
const LINUX_EINVAL: i64 = -22;
const LINUX_ENAMETOOLONG: i64 = -36;
const USER_MESSAGE: &[u8] = b"SLOPOS user write\n";
const INIT_ARGUMENTS: &[&[u8]] = &[b"/sbin/slop-init", b"--system"];
const INIT_ENVIRONMENT: &[&[u8]] = &[
    b"SLOPOS_SESSION=desktop",
    b"XDG_CURRENT_DESKTOP=SlopOS",
    b"WAYLAND_DISPLAY=wayland-0",
];
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
    stack_frames: [u64; crate::paging::USER_STACK_PAGES],
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
    access_mode: AccessMode,
}

impl OpenAtRequest {
    pub fn path(&self) -> &[u8] {
        &self.path[..self.path_length]
    }

    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }
}

#[derive(Clone, Copy)]
pub struct ReadRequest {
    pub fd: u32,
    pub requested: usize,
    destination: u64,
    user_pages: u8,
}

impl ReadRequest {
    pub const fn user_pages(&self) -> u8 {
        self.user_pages
    }
}

#[derive(Clone, Copy)]
pub struct WriteRequest {
    pub fd: u32,
    input: [u8; PROCESS_SYSCALL_IO_CAPACITY],
    input_length: usize,
    user_pages: u8,
}

impl WriteRequest {
    pub fn input(&self) -> &[u8] {
        &self.input[..self.input_length]
    }

    pub const fn user_pages(&self) -> u8 {
        self.user_pages
    }
}

#[derive(Clone, Copy)]
pub struct CloseRequest {
    pub fd: u32,
}

#[derive(Clone, Copy)]
enum PendingSyscall {
    OpenAt(OpenAtRequest),
    Read(ReadRequest),
    Write(WriteRequest),
    Close(CloseRequest),
}

#[derive(Clone, Copy)]
pub enum ProcessEvent {
    OpenAt(OpenAtRequest),
    Read(ReadRequest),
    Write(WriteRequest),
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
    let stack_base = crate::paging::USER_STACK_TOP - PAGE_SIZE;
    // SAFETY: create_user_address_space returned a fresh, zeroed physical
    // stack frame that remains identity-mapped by the kernel and is not yet
    // reachable by user mode.
    let stack = unsafe {
        core::slice::from_raw_parts_mut(
            address_space.stack_frames[crate::paging::USER_STACK_PAGES - 1] as *mut u8,
            PAGE_SIZE as usize,
        )
    };
    let initial_stack = build_linux_initial_stack(
        stack,
        stack_base,
        INIT_ARGUMENTS,
        INIT_ENVIRONMENT,
        elf.entry(),
        PAGE_SIZE,
    )
    .unwrap_or_else(|_| crate::fatal("boot user initial stack construction failed"));
    set_user_mapping(UserMapping {
        code_frame: address_space.code_frame,
        stack_frames: address_space.stack_frames,
        code_start: crate::paging::USER_CODE_BASE,
        code_end: crate::paging::USER_CODE_BASE + segment.memory_size(),
        stack_start: crate::paging::USER_STACK_BASE,
        stack_end: crate::paging::USER_STACK_TOP,
    });
    clear_pending_syscall();
    let image = ProcessImage {
        address_space_root: address_space.root,
        entry: elf.entry(),
        stack_pointer: initial_stack.stack_pointer,
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
        "SLOPOS-PROCESS: pid={PID} source={source} path={path} format=elf64 entry={:#x} segments={} file_bytes={} load_bytes={} memory_bytes={} address_space={:#x} user_code={:#x} user_stack={:#x} code_frame={:#x} stack_frames={:#x}/{:#x} code=user-readonly stack=user-writable kernel=supervisor",
        elf.entry(),
        elf.load_segment_count(),
        user_image.len(),
        segment.file_size(),
        segment.memory_size(),
        address_space.root,
        crate::paging::USER_CODE_BASE,
        crate::paging::USER_STACK_TOP,
        address_space.code_frame,
        address_space.stack_frames[0],
        address_space.stack_frames[1]
    ));
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={PID} initial_stack abi=linux-x86_64 rsp={:#x} aligned=16 stack_pages={} argc={} argv0=/sbin/slop-init envc={} auxv_pairs={} bytes={}",
        initial_stack.stack_pointer,
        crate::paging::USER_STACK_PAGES,
        initial_stack.argument_count,
        initial_stack.environment_count,
        initial_stack.auxiliary_pairs,
        initial_stack.used_bytes
    ));
    configure_fast_syscall();
    process_table_mut()
        .mark_running(PID)
        .unwrap_or_else(|_| crate::fatal("PID 1 ready-to-running transition failed"));
    // SAFETY: the process page table maps a validated one-page program and
    // stack under user permissions; GDT, TSS and syscall MSRs are live.
    unsafe {
        slopos_enter_user(address_space.root, elf.entry(), initial_stack.stack_pointer);
    }
    process_event_after_user()
}

pub fn open_file(node: FileNode, access_mode: AccessMode) -> Result<u32, ProcessError> {
    process_table_mut().open_file(PID, node, access_mode)
}

pub fn read_window(fd: u32, requested: usize) -> Result<ReadWindow, ProcessError> {
    process_table().read_window(PID, fd, requested)
}

pub fn write_window(fd: u32, requested: usize) -> Result<WriteWindow, ProcessError> {
    process_table().write_window(PID, fd, requested)
}

pub fn advance_fd(fd: u32, length: usize) -> Result<(), ProcessError> {
    process_table_mut().advance_fd(PID, fd, length)
}

pub fn seek_fd(fd: u32, offset: u64) -> Result<(), ProcessError> {
    process_table_mut().seek_fd(PID, fd, offset)
}

pub fn close_fd(fd: u32) -> Result<(), ProcessError> {
    process_table_mut().close_fd(PID, fd)
}

pub fn close_all_files() -> Result<usize, ProcessError> {
    process_table_mut().close_all_files(PID)
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
        PendingSyscall::OpenAt(_) | PendingSyscall::Write(_) | PendingSyscall::Close(_) => {
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
        || frame.user_rsp < crate::paging::USER_STACK_BASE
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
            if frame.rdi != LINUX_AT_FDCWD || frame.r10 != 0 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            let access_mode = match frame.rdx {
                LINUX_O_RDONLY => AccessMode::ReadOnly,
                LINUX_O_RDWR => AccessMode::ReadWrite,
                _ => {
                    frame.rax = LINUX_EINVAL as u64;
                    return 0;
                }
            };
            let request = match copy_user_path(frame.rsi, access_mode) {
                Ok(request) => request,
                Err(errno) => {
                    frame.rax = errno as u64;
                    return 0;
                }
            };
            let display = core::str::from_utf8(request.path()).unwrap_or("<non-utf8>");
            suspend_syscall(frame, PendingSyscall::OpenAt(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=suspended nr=257 openat dirfd=-100 flags={} path={display} origin=cpl3",
                frame.rdx
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
            if !validate_user_range(frame.rsi, requested, true) {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let user_pages = user_page_count(frame.rsi, requested)
                .unwrap_or_else(|| crate::fatal("validated read range has invalid page span"));
            suspend_syscall(
                frame,
                PendingSyscall::Read(ReadRequest {
                    fd,
                    requested,
                    destination: frame.rsi,
                    user_pages,
                }),
            );
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=suspended nr=0 read fd={fd} requested={requested} user_pages={user_pages} origin=cpl3"
            ));
            2
        }
        LINUX_SYS_WRITE => {
            if frame.rdi == USER_STDOUT {
                if frame.rdx != USER_MESSAGE.len() as u64 {
                    frame.rax = LINUX_EINVAL as u64;
                    return 0;
                }
                let mut message = [0u8; USER_MESSAGE.len()];
                if copy_from_user(frame.rsi, &mut message).is_none() {
                    frame.rax = LINUX_EFAULT as u64;
                    return 0;
                }
                if message != USER_MESSAGE {
                    crate::fatal("user write syscall payload is invalid");
                }
                frame.rax = USER_MESSAGE.len() as u64;
                crate::serial::serialln(format_args!(
                    "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=sysretq nr=1 write fd=1 bytes={} origin=cpl3 result={}",
                    USER_MESSAGE.len(),
                    USER_MESSAGE.len()
                ));
                return 0;
            }
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
            let mut request = WriteRequest {
                fd,
                input: [0; PROCESS_SYSCALL_IO_CAPACITY],
                input_length: requested,
                user_pages: user_page_count(frame.rsi, requested)
                    .unwrap_or_else(|| crate::fatal("write range has invalid page span")),
            };
            if copy_from_user(frame.rsi, &mut request.input[..requested]).is_none() {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            suspend_syscall(frame, PendingSyscall::Write(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=suspended nr=1 write fd={fd} requested={requested} user_pages={} origin=cpl3",
                request.user_pages()
            ));
            2
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
        LINUX_SYS_LSEEK => {
            let Ok(fd) = u32::try_from(frame.rdi) else {
                frame.rax = LINUX_EBADF as u64;
                return 0;
            };
            if frame.rdx != LINUX_SEEK_SET || frame.rsi > i64::MAX as u64 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            match seek_fd(fd, frame.rsi) {
                Ok(()) => {
                    frame.rax = frame.rsi;
                    crate::serial::serialln(format_args!(
                        "SLOPOS-SYSCALL: pid={PID} abi=linux-x86_64 entry=syscall return=sysretq nr=8 lseek fd={fd} offset={} whence=0 async=false",
                        frame.rsi
                    ));
                }
                Err(ProcessError::Vfs(VfsError::BadFileDescriptor)) => {
                    frame.rax = LINUX_EBADF as u64;
                }
                Err(_) => {
                    frame.rax = LINUX_EINVAL as u64;
                }
            }
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
        PendingSyscall::Write(request) => ProcessEvent::Write(request),
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

fn copy_user_path(address: u64, access_mode: AccessMode) -> Result<OpenAtRequest, i64> {
    let mut request = OpenAtRequest {
        path: [0; PROCESS_SYSCALL_PATH_CAPACITY],
        path_length: 0,
        access_mode,
    };
    for index in 0..PROCESS_SYSCALL_PATH_CAPACITY {
        let index = u64::try_from(index).map_err(|_| LINUX_EFAULT)?;
        let byte_address = address.checked_add(index).ok_or(LINUX_EFAULT)?;
        let mut byte = [0u8; 1];
        copy_from_user(byte_address, &mut byte).ok_or(LINUX_EFAULT)?;
        let byte = byte[0];
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

fn copy_from_user(address: u64, output: &mut [u8]) -> Option<()> {
    if !validate_user_range(address, output.len(), false) {
        return None;
    }
    let mut copied = 0usize;
    while copied < output.len() {
        let cursor = address.checked_add(u64::try_from(copied).ok()?)?;
        let (pointer, length) = user_physical_chunk(cursor, output.len() - copied, false)?;
        // SAFETY: validation covered the entire source range before mutation;
        // each chunk is bounded to a live, identity-mapped user frame.
        unsafe {
            ptr::copy_nonoverlapping(pointer.cast_const(), output[copied..].as_mut_ptr(), length)
        };
        copied += length;
    }
    Some(())
}

fn copy_to_user(address: u64, bytes: &[u8]) -> Option<()> {
    if !validate_user_range(address, bytes.len(), true) {
        return None;
    }
    let mut copied = 0usize;
    while copied < bytes.len() {
        let cursor = address.checked_add(u64::try_from(copied).ok()?)?;
        let (pointer, length) = user_physical_chunk(cursor, bytes.len() - copied, true)?;
        // SAFETY: validation covered the entire destination before mutation;
        // PID 1 is suspended and each chunk lies in a writable mapped frame.
        unsafe {
            ptr::copy_nonoverlapping(bytes[copied..].as_ptr(), pointer, length);
        }
        copied += length;
    }
    Some(())
}

fn validate_user_range(address: u64, length: usize, writable: bool) -> bool {
    let Ok(total_length) = u64::try_from(length) else {
        return false;
    };
    if address.checked_add(total_length).is_none() {
        return false;
    }
    let mut checked = 0usize;
    while checked < length {
        let Ok(offset) = u64::try_from(checked) else {
            return false;
        };
        let Some(cursor) = address.checked_add(offset) else {
            return false;
        };
        let Some((_, chunk)) = user_physical_chunk(cursor, length - checked, writable) else {
            return false;
        };
        checked += chunk;
    }
    true
}

fn user_page_count(address: u64, length: usize) -> Option<u8> {
    let length = u64::try_from(length).ok()?;
    if length == 0 {
        return Some(0);
    }
    let last = address.checked_add(length - 1)?;
    let first_page = address / PAGE_SIZE;
    let last_page = last / PAGE_SIZE;
    u8::try_from(last_page.checked_sub(first_page)?.checked_add(1)?).ok()
}

fn user_physical_chunk(
    address: u64,
    maximum_length: usize,
    writable: bool,
) -> Option<(*mut u8, usize)> {
    let mapping = user_mapping()?;
    if maximum_length == 0 {
        return Some((core::ptr::null_mut(), 0));
    }
    if !writable && address >= mapping.code_start && address < mapping.code_end {
        let offset = address.checked_sub(mapping.code_start)?;
        let physical = mapping.code_frame.checked_add(offset)?;
        let remaining = usize::try_from(mapping.code_end - address).ok()?;
        return Some((physical as *mut u8, maximum_length.min(remaining)));
    }
    if address < mapping.stack_start || address >= mapping.stack_end {
        return None;
    }
    let stack_offset = address.checked_sub(mapping.stack_start)?;
    let page_index = usize::try_from(stack_offset / PAGE_SIZE).ok()?;
    let page_offset = stack_offset % PAGE_SIZE;
    let physical = mapping
        .stack_frames
        .get(page_index)?
        .checked_add(page_offset)?;
    let page_remaining = usize::try_from(PAGE_SIZE - page_offset).ok()?;
    Some((physical as *mut u8, maximum_length.min(page_remaining)))
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
