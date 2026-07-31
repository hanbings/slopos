// SPDX-License-Identifier: 0BSD

use core::arch::x86_64::__cpuid;
use core::arch::{asm, global_asm};
use core::cell::UnsafeCell;
use core::ptr;
use slopos_desktop_protocol::{
    COMMIT_SIZE, DESKTOP_COMMIT_SYSCALL, DESKTOP_WAIT_SYSCALL, DesktopCommit, DesktopServiceEvent,
    EVENT_CONFIG_APPLIED, EVENT_POLICY_APPLIED, EVENT_SIZE, WAYLAND_EVENT_MAX_SIZE,
    WAYLAND_EVENT_WAIT_SYSCALL, WAYLAND_SURFACE_HEADER_SIZE, WAYLAND_SURFACE_MAX_SIZE,
    WAYLAND_SURFACE_SYSCALL, WaylandServerEvent, WaylandSurfaceCommit, WaylandSurfaceHeader,
};
use slopos_process::{
    ProcessError, ProcessImage, ProcessState, ProcessTable, build_linux_initial_stack,
};
use slopos_vfs::{AccessMode, FileNode, ReadWindow, VfsError, WriteWindow};

const INIT_PID: u32 = 1;
const DESKTOP_PID: u32 = 2;
pub const PROCESS_CAPACITY: usize = 4;
const PROCESS_FD_CAPACITY: usize = 8;
const INIT_EXPECTED_SYSCALLS: u64 = 18;
const DESKTOP_EXPECTED_SYSCALLS: u64 = 22;
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
const LINUX_SYS_SCHED_YIELD: u64 = 24;
const LINUX_SYS_EXIT: u64 = 60;
const LINUX_SYS_WAIT4: u64 = 61;
const LINUX_SYS_OPENAT: u64 = 257;
const LINUX_SEEK_SET: u64 = 0;
const LINUX_EBADF: i64 = -9;
const LINUX_ECHILD: i64 = -10;
const LINUX_EFAULT: i64 = -14;
const LINUX_EINVAL: i64 = -22;
const LINUX_ENAMETOOLONG: i64 = -36;
const INIT_MESSAGE: &[u8] = b"SLOPOS user write\n";
const DESKTOP_MESSAGE: &[u8] = b"SLOPOS desktop policy ready\n";
const INIT_ARGUMENTS: &[&[u8]] = &[b"/sbin/slop-init", b"--system"];
const DESKTOP_ARGUMENTS: &[&[u8]] = &[b"/sbin/slop-shell", b"--session"];
const INIT_ENVIRONMENT: &[&[u8]] = &[
    b"SLOPOS_SESSION=desktop",
    b"XDG_CURRENT_DESKTOP=SlopOS",
    b"WAYLAND_DISPLAY=wayland-0",
];
const DESKTOP_ENVIRONMENT: &[&[u8]] = &[
    b"SLOPOS_ROLE=desktop-shell",
    b"XDG_CURRENT_DESKTOP=SlopOS",
    b"WAYLAND_DISPLAY=wayland-0",
    b"SLOPOS_WAYBAR_OUTPUT=SLOPOS-1",
];
const PAGE_SIZE: u64 = 4096;
const IA32_EFER: u32 = 0xc000_0080;
const IA32_STAR: u32 = 0xc000_0081;
const IA32_LSTAR: u32 = 0xc000_0082;
const IA32_FMASK: u32 = 0xc000_0084;
const EFER_SYSCALL_ENABLE: u64 = 1;
const KERNEL_CODE_SELECTOR: u64 = 0x08;
const SYSRET_SELECTOR_BASE: u64 = 0x10;
const USER_DATA_SELECTOR: u64 = 0x1b;
const USER_CODE_SELECTOR: u64 = 0x23;
const STAR_VALUE: u64 = (SYSRET_SELECTOR_BASE << 48) | (KERNEL_CODE_SELECTOR << 32);
const RFLAGS_RESERVED_ONE: u64 = 1 << 1;
const RFLAGS_INTERRUPT_ENABLE: u64 = 1 << 9;
const RFLAGS_SYSCALL_MASK: u64 =
    (1 << 8) | (1 << 9) | (1 << 10) | (3 << 12) | (1 << 14) | (1 << 18);
const RFLAGS_USER_CLEAR: u64 = (3 << 12) | (1 << 14) | (1 << 16) | (1 << 17);

type KernelProcessTable = ProcessTable<PROCESS_CAPACITY, PROCESS_FD_CAPACITY>;

struct ProcessTableStorage(UnsafeCell<KernelProcessTable>);

// The bootstrap processor is the only process-table owner. Syscall entry runs
// with IF masked; async completion mutates only while the current process is
// suspended in the block task.
unsafe impl Sync for ProcessTableStorage {}

static PROCESS_TABLE: ProcessTableStorage =
    ProcessTableStorage(UnsafeCell::new(KernelProcessTable::new()));

#[derive(Clone, Copy)]
struct UserMapping {
    table_frames: [u64; 4],
    code_frames: [u64; crate::paging::USER_CODE_PAGES],
    stack_frames: [u64; crate::paging::USER_STACK_PAGES],
    code_start: u64,
    code_end: u64,
    stack_start: u64,
    stack_end: u64,
}

struct UserMappingStorage(UnsafeCell<[Option<UserMapping>; PROCESS_CAPACITY]>);

// A slot is installed before its process first runs and remains immutable
// while that process is alive or suspended in the block task.
unsafe impl Sync for UserMappingStorage {}

static USER_MAPPINGS: UserMappingStorage =
    UserMappingStorage(UnsafeCell::new([None; PROCESS_CAPACITY]));

#[derive(Clone, Copy)]
pub struct OpenAtRequest {
    pid: u32,
    path: [u8; PROCESS_SYSCALL_PATH_CAPACITY],
    path_length: usize,
    access_mode: AccessMode,
}

impl OpenAtRequest {
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub fn path(&self) -> &[u8] {
        &self.path[..self.path_length]
    }

    pub fn access_mode(&self) -> AccessMode {
        self.access_mode
    }
}

#[derive(Clone, Copy)]
pub struct ReadRequest {
    pub pid: u32,
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
    pub pid: u32,
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
    pub pid: u32,
    pub fd: u32,
}

#[derive(Clone, Copy)]
struct WaitRequest {
    pid: u32,
    #[allow(dead_code)]
    status_address: u64,
}

#[derive(Clone, Copy)]
pub struct DesktopWaitRequest {
    pid: u32,
    destination: u64,
    kind: u16,
    after_generation: u64,
}

impl DesktopWaitRequest {
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub const fn kind(&self) -> u16 {
        self.kind
    }

    pub const fn after_generation(&self) -> u64 {
        self.after_generation
    }
}

#[derive(Clone, Copy)]
pub struct WaylandWaitRequest {
    pid: u32,
    destination: u64,
    after_sequence: u64,
}

impl WaylandWaitRequest {
    pub const fn after_sequence(&self) -> u64 {
        self.after_sequence
    }
}

#[derive(Clone, Copy)]
enum PendingSyscall {
    OpenAt(OpenAtRequest),
    Read(ReadRequest),
    Write(WriteRequest),
    Close(CloseRequest),
    Yield,
    Wait(WaitRequest),
    DesktopWait(DesktopWaitRequest),
    WaylandWait(WaylandWaitRequest),
}

#[derive(Clone, Copy)]
pub enum ProcessEvent {
    OpenAt(OpenAtRequest),
    Read(ReadRequest),
    Write(WriteRequest),
    Close(CloseRequest),
    Yielded { pid: u32 },
    Preempted { pid: u32, tick: u64, count: u64 },
    Waiting { pid: u32 },
    DesktopWaiting(DesktopWaitRequest),
    WaylandWaiting(WaylandWaitRequest),
    Exited { pid: u32 },
}

struct PendingSyscallStorage(UnsafeCell<[Option<PendingSyscall>; PROCESS_CAPACITY]>);

// Each process owns at most one pending request. A request is written by the
// IF-masked fast entry and completed only after control returns to block task.
unsafe impl Sync for PendingSyscallStorage {}

static PENDING_SYSCALLS: PendingSyscallStorage =
    PendingSyscallStorage(UnsafeCell::new([None; PROCESS_CAPACITY]));

struct CurrentProcessStorage(UnsafeCell<Option<u32>>);

// Only the active CPL3 context and its IF-masked syscall/timer entries read
// this value; block-task scheduling runs after the continuation returns.
unsafe impl Sync for CurrentProcessStorage {}

static CURRENT_PROCESS: CurrentProcessStorage = CurrentProcessStorage(UnsafeCell::new(None));

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

#[repr(C)]
pub(crate) struct InterruptFrame {
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
    user_cs: u64,
    user_rflags: u64,
    user_rsp: u64,
    user_ss: u64,
}

const _: () = {
    assert!(core::mem::size_of::<SyscallFrame>() == 144);
    assert!(core::mem::offset_of!(SyscallFrame, user_rip) == 120);
    assert!(core::mem::offset_of!(SyscallFrame, user_rflags) == 128);
    assert!(core::mem::offset_of!(SyscallFrame, user_rsp) == 136);
    assert!(core::mem::size_of::<InterruptFrame>() == 160);
    assert!(core::mem::offset_of!(InterruptFrame, user_rip) == 120);
    assert!(core::mem::offset_of!(InterruptFrame, user_cs) == 128);
    assert!(core::mem::offset_of!(InterruptFrame, user_rflags) == 136);
    assert!(core::mem::offset_of!(InterruptFrame, user_rsp) == 144);
    assert!(core::mem::offset_of!(InterruptFrame, user_ss) == 152);
};

const EMPTY_SYSCALL_FRAME: SyscallFrame = SyscallFrame {
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
};

struct SyscallFrameStorage(UnsafeCell<[SyscallFrame; PROCESS_CAPACITY]>);

// Each saved frame belongs to one suspended process and is never accessed
// concurrently with that process's user-mode execution.
unsafe impl Sync for SyscallFrameStorage {}

static SAVED_SYSCALL_FRAMES: SyscallFrameStorage =
    SyscallFrameStorage(UnsafeCell::new([EMPTY_SYSCALL_FRAME; PROCESS_CAPACITY]));

struct PreemptionStorage {
    counts: UnsafeCell<[u64; PROCESS_CAPACITY]>,
    ticks: UnsafeCell<[u64; PROCESS_CAPACITY]>,
}

// The timer IRQ is the only writer while a process is running; the block-task
// continuation reads the selected PID only after the IRQ returned to it.
unsafe impl Sync for PreemptionStorage {}

static PREEMPTIONS: PreemptionStorage = PreemptionStorage {
    counts: UnsafeCell::new([0; PROCESS_CAPACITY]),
    ticks: UnsafeCell::new([0; PROCESS_CAPACITY]),
};

pub fn start_processes(
    init_image: &[u8],
    init_source: &str,
    init_path: &str,
    desktop_image: &[u8],
    desktop_source: &str,
    desktop_path: &str,
) -> ProcessEvent {
    reset_process_state();
    let init_pid = spawn_user_process(
        init_image,
        init_source,
        init_path,
        None,
        INIT_ARGUMENTS,
        INIT_ENVIRONMENT,
    );
    if init_pid != INIT_PID {
        crate::fatal("bootstrap process did not receive PID 1");
    }
    let desktop_pid = spawn_user_process(
        desktop_image,
        desktop_source,
        desktop_path,
        Some(init_pid),
        DESKTOP_ARGUMENTS,
        DESKTOP_ENVIRONMENT,
    );
    if desktop_pid != DESKTOP_PID {
        crate::fatal("desktop service process did not receive PID 2");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: table initialized capacity={PROCESS_CAPACITY} processes=2 pids={INIT_PID}/{DESKTOP_PID} roles=init/desktop-service states=ready/ready fd_capacity={PROCESS_FD_CAPACITY} per_process_fds=true"
    ));
    configure_fast_syscall();
    run_process(init_pid)
}

fn spawn_user_process(
    user_image: &[u8],
    source: &str,
    path: &str,
    parent: Option<u32>,
    arguments: &[&[u8]],
    environment: &[&[u8]],
) -> u32 {
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
        || segment.memory_size() > crate::paging::USER_CODE_PAGES as u64 * PAGE_SIZE
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
        arguments,
        environment,
        elf.entry(),
        PAGE_SIZE,
    )
    .unwrap_or_else(|_| crate::fatal("boot user initial stack construction failed"));
    let image = ProcessImage {
        address_space_root: address_space.root,
        entry: elf.entry(),
        stack_pointer: initial_stack.stack_pointer,
        stack_top: crate::paging::USER_STACK_TOP,
        user_memory_start: crate::paging::USER_CODE_BASE,
        user_memory_end: crate::paging::USER_CODE_BASE + segment.memory_size(),
    };
    let pid = process_table_mut()
        .spawn(parent, image)
        .unwrap_or_else(|_| crate::fatal("user process-table insertion failed"));
    set_user_mapping(
        pid,
        UserMapping {
            table_frames: address_space.table_frames,
            code_frames: address_space.code_frames,
            stack_frames: address_space.stack_frames,
            code_start: crate::paging::USER_CODE_BASE,
            code_end: crate::paging::USER_CODE_BASE + segment.memory_size(),
            stack_start: crate::paging::USER_STACK_BASE,
            stack_end: crate::paging::USER_STACK_TOP,
        },
    );
    let argv0 = core::str::from_utf8(arguments[0]).unwrap_or("<non-utf8>");
    let argv1 = core::str::from_utf8(arguments[1]).unwrap_or("<non-utf8>");
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={pid} parent={} source={source} path={path} argv1={argv1} format=elf64 entry={:#x} segments={} file_bytes={} load_bytes={} memory_bytes={} address_space={:#x} user_code={:#x} user_stack={:#x} code_frames={:#x}/{:#x}/{:#x} stack_frames={:#x}/{:#x}/{:#x} code=user-readonly stack=user-writable kernel=supervisor",
        parent.unwrap_or(0),
        elf.entry(),
        elf.load_segment_count(),
        user_image.len(),
        segment.file_size(),
        segment.memory_size(),
        address_space.root,
        crate::paging::USER_CODE_BASE,
        crate::paging::USER_STACK_TOP,
        address_space.code_frames[0],
        address_space.code_frames[1],
        address_space.code_frames[2],
        address_space.stack_frames[0],
        address_space.stack_frames[1],
        address_space.stack_frames[2]
    ));
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={pid} initial_stack abi=linux-x86_64 rsp={:#x} aligned=16 stack_pages={} argc={} argv0={argv0} argv1={argv1} envc={} auxv_pairs={} bytes={}",
        initial_stack.stack_pointer,
        crate::paging::USER_STACK_PAGES,
        initial_stack.argument_count,
        initial_stack.environment_count,
        initial_stack.auxiliary_pairs,
        initial_stack.used_bytes
    ));
    pid
}

fn run_process(pid: u32) -> ProcessEvent {
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("scheduled process disappeared"));
    if !matches!(process.state, ProcessState::Ready | ProcessState::Runnable) {
        crate::fatal("only a schedulable process can enter user mode");
    }
    if process.state == ProcessState::Runnable {
        match pending_syscall(pid) {
            Some(PendingSyscall::Yield) => {
                clear_pending_syscall(pid);
                saved_syscall_frame_mut(pid).rax = 0;
            }
            Some(_) => crate::fatal("runnable process retained an I/O request"),
            None => {}
        }
    } else if pending_syscall(pid).is_some() {
        crate::fatal("ready process has a pending syscall");
    }
    process_table_mut()
        .mark_running(pid)
        .unwrap_or_else(|_| crate::fatal("process ready-to-running transition failed"));
    set_current_process(Some(pid));
    // SAFETY: the process owns a validated address space. Ready processes use
    // their ELF entry/initial stack; Runnable processes use a frame captured
    // by the IF-masked syscall or timer entry. The shared resume path uses
    // IRETQ so timer preemption preserves RCX/R11 as ordinary user GPRs,
    // while syscall resumes retain the architecturally clobbered values that
    // SYSCALL placed in those registers.
    unsafe {
        if process.state == ProcessState::Ready {
            slopos_enter_user(
                process.image.address_space_root,
                process.image.entry,
                process.image.stack_pointer,
            );
        } else {
            slopos_resume_user(process.image.address_space_root, saved_syscall_frame(pid));
        }
    }
    set_current_process(None);
    process_event_after_user(pid)
}

pub fn open_file(pid: u32, node: FileNode, access_mode: AccessMode) -> Result<u32, ProcessError> {
    process_table_mut().open_file(pid, node, access_mode)
}

pub fn read_window(pid: u32, fd: u32, requested: usize) -> Result<ReadWindow, ProcessError> {
    process_table().read_window(pid, fd, requested)
}

pub fn write_window(pid: u32, fd: u32, requested: usize) -> Result<WriteWindow, ProcessError> {
    process_table().write_window(pid, fd, requested)
}

pub fn advance_fd(pid: u32, fd: u32, length: usize) -> Result<(), ProcessError> {
    process_table_mut().advance_fd(pid, fd, length)
}

pub fn seek_fd(pid: u32, fd: u32, offset: u64) -> Result<(), ProcessError> {
    process_table_mut().seek_fd(pid, fd, offset)
}

pub fn close_fd(pid: u32, fd: u32) -> Result<(), ProcessError> {
    process_table_mut().close_fd(pid, fd)
}

pub fn close_all_files(pid: u32) -> Result<usize, ProcessError> {
    process_table_mut().close_all_files(pid)
}

pub(crate) fn preempt_from_timer(frame: &InterruptFrame, tick: u64) -> bool {
    let pid = current_process()
        .unwrap_or_else(|| crate::fatal("CPL3 timer interrupt has no current process"));
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("timer interrupt process disappeared"));
    if process.state != ProcessState::Running
        || frame.user_cs != USER_CODE_SELECTOR
        || frame.user_ss != USER_DATA_SELECTOR
        || frame.user_rip < process.image.user_memory_start
        || frame.user_rip >= process.image.user_memory_end
        || frame.user_rsp < crate::paging::USER_STACK_BASE
        || frame.user_rsp > process.image.stack_top
        || frame.user_rflags & RFLAGS_RESERVED_ONE == 0
    {
        crate::fatal("timer interrupt user context failed validation");
    }
    if process_table().next_schedulable_after(pid).is_none() {
        return false;
    }
    if pending_syscall(pid).is_some() {
        crate::fatal("running process was preempted with a pending syscall");
    }
    let user_rflags =
        (frame.user_rflags & !RFLAGS_USER_CLEAR) | RFLAGS_RESERVED_ONE | RFLAGS_INTERRUPT_ENABLE;
    let saved = SyscallFrame {
        r15: frame.r15,
        r14: frame.r14,
        r13: frame.r13,
        r12: frame.r12,
        r11: frame.r11,
        r10: frame.r10,
        r9: frame.r9,
        r8: frame.r8,
        rbp: frame.rbp,
        rdi: frame.rdi,
        rsi: frame.rsi,
        rdx: frame.rdx,
        rcx: frame.rcx,
        rbx: frame.rbx,
        rax: frame.rax,
        user_rip: frame.user_rip,
        user_rflags,
        user_rsp: frame.user_rsp,
    };
    let index = pid_index(pid);
    // SAFETY: this IF-masked timer top half exclusively owns the running
    // process frame and its per-PID preemption counters.
    unsafe {
        (*SAVED_SYSCALL_FRAMES.0.get())[index] = saved;
        let counts = &mut *PREEMPTIONS.counts.get();
        counts[index] = counts[index]
            .checked_add(1)
            .unwrap_or_else(|| crate::fatal("process preemption counter overflow"));
        (*PREEMPTIONS.ticks.get())[index] = tick;
    }
    process_table_mut()
        .mark_runnable(pid)
        .unwrap_or_else(|_| crate::fatal("timer running-to-runnable transition failed"));
    true
}

#[allow(dead_code)]
pub fn reap_exited_process(pid: u32) -> Option<ProcessEvent> {
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("exited process disappeared before reap decision"));
    if process.state != ProcessState::Exited {
        crate::fatal("only an exited process can reach reap decision");
    }
    let Some(parent) = process.parent else {
        release_exited_process(pid);
        return None;
    };
    let request = match pending_syscall(parent) {
        Some(PendingSyscall::Wait(request)) if request.pid == parent => request,
        _ => {
            crate::serial::serialln(format_args!(
                "SLOPOS-PROCESS: pid={pid} state=zombie parent={parent} child_reap=deferred"
            ));
            return None;
        }
    };
    let exit_status = release_exited_process(pid);
    write_wait_status(parent, request.status_address, exit_status);
    clear_pending_syscall(parent);
    saved_syscall_frame_mut(parent).rax = u64::from(pid);
    process_table_mut()
        .mark_runnable(parent)
        .unwrap_or_else(|_| crate::fatal("waiting parent blocked-to-runnable transition failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={parent} wait4 child={pid} status={exit_status} child_reaped=true"
    ));
    Some(run_process(parent))
}

fn release_exited_process(pid: u32) -> i32 {
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("reaped process disappeared"));
    if process.state != ProcessState::Exited {
        crate::fatal("only an exited process can be reaped");
    }
    let exit_status = process
        .exit_status
        .unwrap_or_else(|| crate::fatal("exited process has no status"));
    process_table_mut()
        .reap(pid)
        .unwrap_or_else(|_| crate::fatal("process-table reap failed"));
    let mapping = take_user_mapping(pid)
        .unwrap_or_else(|| crate::fatal("reaped process has no user mapping"));
    let frames = crate::paging::release_user_address_space(crate::paging::UserAddressSpace {
        root: process.image.address_space_root,
        code_frames: mapping.code_frames,
        stack_frames: mapping.stack_frames,
        table_frames: mapping.table_frames,
    });
    let reuse_probe = crate::memory::allocate_frame()
        .unwrap_or_else(|| crate::fatal("released user frame was not reusable"));
    if reuse_probe != mapping.stack_frames[crate::paging::USER_STACK_PAGES - 1] {
        crate::fatal("frame allocator did not reuse the most recent user frame");
    }
    crate::memory::deallocate_frame(reuse_probe)
        .unwrap_or_else(|_| crate::fatal("reused user frame could not be returned"));
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={pid} state=reaped address_space_released=true frames={frames} reuse_probe={reuse_probe:#x}"
    ));
    exit_status
}

fn write_wait_status(parent: u32, status_address: u64, exit_status: i32) {
    let wait_status = ((exit_status & 0xff) << 8).to_ne_bytes();
    copy_to_user(parent, status_address, &wait_status)
        .unwrap_or_else(|| crate::fatal("wait4 status destination became invalid"));
}

pub fn resume_probe(pid: u32, result: i64, read_output: Option<&[u8]>) -> ProcessEvent {
    let pending = pending_syscall(pid)
        .unwrap_or_else(|| crate::fatal("process resume has no pending syscall"));
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
                copy_to_user(pid, request.destination, output)
                    .unwrap_or_else(|| crate::fatal("read completion user buffer became invalid"));
            }
        }
        PendingSyscall::OpenAt(_) | PendingSyscall::Write(_) | PendingSyscall::Close(_) => {
            if read_output.is_some() {
                crate::fatal("non-read completion carried output bytes");
            }
        }
        PendingSyscall::Yield
        | PendingSyscall::Wait(_)
        | PendingSyscall::DesktopWait(_)
        | PendingSyscall::WaylandWait(_) => {
            crate::fatal("scheduler syscall was sent through I/O completion")
        }
    }
    clear_pending_syscall(pid);
    let frame = saved_syscall_frame_mut(pid);
    frame.rax = result as u64;
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("resumed process disappeared"));
    if process.state != ProcessState::Blocked {
        crate::fatal("only a blocked process can complete an async syscall");
    }
    process_table_mut()
        .mark_runnable(pid)
        .unwrap_or_else(|_| crate::fatal("process blocked-to-runnable transition failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-SCHED: pid={pid} state=blocked->runnable reason=io-complete"
    ));
    run_process(pid)
}

pub fn resume_desktop_wait(
    request: DesktopWaitRequest,
    event: DesktopServiceEvent,
) -> ProcessEvent {
    let pid = request.pid;
    let pending = pending_syscall(pid)
        .unwrap_or_else(|| crate::fatal("desktop event resume has no pending syscall"));
    let PendingSyscall::DesktopWait(pending_request) = pending else {
        crate::fatal("desktop event resumed a different pending syscall");
    };
    if pending_request.pid != request.pid
        || pending_request.destination != request.destination
        || pending_request.kind != request.kind
        || pending_request.after_generation != request.after_generation
        || event.validate().is_err()
        || event.kind != request.kind
        || event.generation <= request.after_generation
    {
        crate::fatal("desktop event completion failed validation");
    }
    copy_to_user(pid, request.destination, &event.encode())
        .unwrap_or_else(|| crate::fatal("desktop event destination became invalid"));
    clear_pending_syscall(pid);
    saved_syscall_frame_mut(pid).rax = 0;
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("desktop event process disappeared"));
    if process.state != ProcessState::Blocked {
        crate::fatal("only a blocked desktop service can receive an event");
    }
    process_table_mut()
        .mark_runnable(pid)
        .unwrap_or_else(|_| crate::fatal("desktop service blocked-to-runnable transition failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-SCHED: pid={pid} state=blocked->runnable reason=desktop-event event={} generation={}",
        desktop_event_name(event.kind),
        event.generation,
    ));
    run_process(pid)
}

pub fn resume_wayland_wait(
    request: WaylandWaitRequest,
    event: WaylandServerEvent<'_>,
) -> ProcessEvent {
    let pid = request.pid;
    let pending = pending_syscall(pid)
        .unwrap_or_else(|| crate::fatal("Wayland event resume has no pending syscall"));
    let PendingSyscall::WaylandWait(pending_request) = pending else {
        crate::fatal("Wayland event resumed a different pending syscall");
    };
    if pending_request.pid != request.pid
        || pending_request.destination != request.destination
        || pending_request.after_sequence != request.after_sequence
        || event.validate().is_err()
        || event.header.sequence <= request.after_sequence
    {
        crate::fatal("Wayland event completion failed validation");
    }
    let mut encoded = [0; WAYLAND_EVENT_MAX_SIZE];
    let length = event
        .encode(&mut encoded)
        .unwrap_or_else(|_| crate::fatal("Wayland event encoding failed"));
    copy_to_user(pid, request.destination, &encoded[..length])
        .unwrap_or_else(|| crate::fatal("Wayland event destination became invalid"));
    crate::wayland_service::acknowledge_event(event.header.sequence);
    clear_pending_syscall(pid);
    saved_syscall_frame_mut(pid).rax = length as u64;
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("Wayland event process disappeared"));
    if process.state != ProcessState::Blocked {
        crate::fatal("only a blocked Wayland client can receive an event");
    }
    process_table_mut()
        .mark_runnable(pid)
        .unwrap_or_else(|_| crate::fatal("Wayland client blocked-to-runnable transition failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-SCHED: pid={pid} state=blocked->runnable reason=wayland-event kind={} sequence={} wire_bytes={}",
        event.header.kind,
        event.header.sequence,
        event.wire.len()
    ));
    run_process(pid)
}

const fn desktop_event_name(kind: u16) -> &'static str {
    match kind {
        EVENT_POLICY_APPLIED => "policy-applied",
        EVENT_CONFIG_APPLIED => "config-applied",
        _ => "invalid",
    }
}

pub fn schedule_next(after_pid: u32) -> ProcessEvent {
    schedule_next_if_any(after_pid)
        .unwrap_or_else(|| crate::fatal("cooperative scheduler found no runnable process"))
}

pub fn schedule_next_if_any(after_pid: u32) -> Option<ProcessEvent> {
    let pid = process_table().next_schedulable_after(after_pid)?;
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("scheduled process disappeared"));
    crate::serial::serialln(format_args!(
        "SLOPOS-SCHED: cooperative switch from={after_pid} to={pid} next_state={:?} independent_cr3=true",
        process.state
    ));
    match process.state {
        ProcessState::Ready | ProcessState::Runnable => Some(run_process(pid)),
        ProcessState::Running | ProcessState::Blocked | ProcessState::Exited => {
            crate::fatal("cooperative scheduler selected an invalid state")
        }
    }
}

pub fn schedule_after_preemption(after_pid: u32, tick: u64, count: u64) -> ProcessEvent {
    let pid = process_table()
        .next_schedulable_after(after_pid)
        .unwrap_or_else(|| crate::fatal("preemptive scheduler found no runnable process"));
    if pid == after_pid {
        crate::fatal("timer preemption did not select a different process");
    }
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("preemptive scheduler selected a missing process"));
    if count == 1 {
        crate::serial::serialln(format_args!(
            "SLOPOS-SCHED: timer preempt from={after_pid} to={pid} tick={tick} preemptions={count} next_state={:?} independent_cr3=true",
            process.state
        ));
    }
    match process.state {
        ProcessState::Ready | ProcessState::Runnable => run_process(pid),
        ProcessState::Running | ProcessState::Blocked | ProcessState::Exited => {
            crate::fatal("preemptive scheduler selected an invalid state")
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn slopos_syscall_handler(frame: &mut SyscallFrame) -> u64 {
    let pid =
        current_process().unwrap_or_else(|| crate::fatal("syscall entry has no current process"));
    let process = process_table()
        .snapshot(pid)
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
        .record_syscall(pid)
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
            let request = match copy_user_path(pid, frame.rsi, access_mode) {
                Ok(request) => request,
                Err(errno) => {
                    frame.rax = errno as u64;
                    return 0;
                }
            };
            let display = core::str::from_utf8(request.path()).unwrap_or("<non-utf8>");
            suspend_io_syscall(pid, frame, PendingSyscall::OpenAt(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=suspended nr=257 openat dirfd=-100 flags={} path={display} origin=cpl3",
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
            if !validate_user_range(pid, frame.rsi, requested, true) {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let user_pages = user_page_count(frame.rsi, requested)
                .unwrap_or_else(|| crate::fatal("validated read range has invalid page span"));
            suspend_io_syscall(
                pid,
                frame,
                PendingSyscall::Read(ReadRequest {
                    pid,
                    fd,
                    requested,
                    destination: frame.rsi,
                    user_pages,
                }),
            );
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=suspended nr=0 read fd={fd} requested={requested} user_pages={user_pages} origin=cpl3"
            ));
            2
        }
        LINUX_SYS_WRITE => {
            if frame.rdi == USER_STDOUT {
                let expected = match pid {
                    INIT_PID => INIT_MESSAGE,
                    DESKTOP_PID => DESKTOP_MESSAGE,
                    _ => crate::fatal("stdout write came from an unknown process"),
                };
                if frame.rdx != expected.len() as u64 {
                    frame.rax = LINUX_EINVAL as u64;
                    return 0;
                }
                let mut message = [0u8; DESKTOP_MESSAGE.len()];
                if copy_from_user(pid, frame.rsi, &mut message[..expected.len()]).is_none() {
                    frame.rax = LINUX_EFAULT as u64;
                    return 0;
                }
                if message[..expected.len()] != *expected {
                    crate::fatal("user write syscall payload is invalid");
                }
                frame.rax = expected.len() as u64;
                crate::serial::serialln(format_args!(
                    "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=sysretq nr=1 write fd=1 bytes={} origin=cpl3 result={}",
                    expected.len(),
                    expected.len()
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
            let Some(user_pages) = user_page_count(frame.rsi, requested) else {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            };
            let mut request = WriteRequest {
                pid,
                fd,
                input: [0; PROCESS_SYSCALL_IO_CAPACITY],
                input_length: requested,
                user_pages,
            };
            if copy_from_user(pid, frame.rsi, &mut request.input[..requested]).is_none() {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            suspend_io_syscall(pid, frame, PendingSyscall::Write(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=suspended nr=1 write fd={fd} requested={requested} user_pages={} origin=cpl3",
                request.user_pages()
            ));
            2
        }
        LINUX_SYS_CLOSE => {
            let Ok(fd) = u32::try_from(frame.rdi) else {
                frame.rax = LINUX_EBADF as u64;
                return 0;
            };
            suspend_io_syscall(pid, frame, PendingSyscall::Close(CloseRequest { pid, fd }));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=suspended nr=3 close fd={fd} origin=cpl3"
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
            match seek_fd(pid, fd, frame.rsi) {
                Ok(()) => {
                    frame.rax = frame.rsi;
                    crate::serial::serialln(format_args!(
                        "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=sysretq nr=8 lseek fd={fd} offset={} whence=0 async=false",
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
        LINUX_SYS_SCHED_YIELD => {
            save_pending_syscall(pid, frame, PendingSyscall::Yield);
            process_table_mut()
                .mark_runnable(pid)
                .unwrap_or_else(|_| crate::fatal("process running-to-runnable transition failed"));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=kernel nr=24 sched_yield state=runnable origin=cpl3"
            ));
            1
        }
        LINUX_SYS_WAIT4 => {
            if frame.rdi != u64::MAX || frame.rdx != 0 || frame.r10 != 0 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            if process_table().child_count(pid) == 0 {
                frame.rax = LINUX_ECHILD as u64;
                return 0;
            }
            if frame.rsi == 0 || !validate_user_range(pid, frame.rsi, 4, true) {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            if let Some(child) = process_table().first_exited_child(pid) {
                let exit_status = release_exited_process(child.pid);
                write_wait_status(pid, frame.rsi, exit_status);
                frame.rax = u64::from(child.pid);
                crate::serial::serialln(format_args!(
                    "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=sysretq nr=61 wait4 child={} state=completed-immediate origin=cpl3 result={}",
                    child.pid, child.pid
                ));
                crate::serial::serialln(format_args!(
                    "SLOPOS-PROCESS: pid={pid} wait4 child={} status={exit_status} child_reaped=true",
                    child.pid
                ));
                return 0;
            }
            suspend_io_syscall(
                pid,
                frame,
                PendingSyscall::Wait(WaitRequest {
                    pid,
                    status_address: frame.rsi,
                }),
            );
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=kernel nr=61 wait4 child=any state=blocked origin=cpl3"
            ));
            1
        }
        DESKTOP_COMMIT_SYSCALL => {
            if frame.rsi != COMMIT_SIZE as u64 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            let mut bytes = [0u8; COMMIT_SIZE];
            if copy_from_user(pid, frame.rdi, &mut bytes).is_none() {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let Ok(commit) = DesktopCommit::decode(&bytes) else {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            };
            match crate::desktop_service::submit(pid, commit) {
                Ok(generation) => {
                    frame.rax = 0;
                    crate::serial::serialln(format_args!(
                        "SLOPOS-SYSCALL: pid={pid} abi=slopos-desktop-v1 entry=syscall return=sysretq nr={DESKTOP_COMMIT_SYSCALL} commit_bytes={COMMIT_SIZE} generation={generation} origin=cpl3 result=0"
                    ));
                }
                Err(_) => {
                    frame.rax = LINUX_EINVAL as u64;
                }
            }
            0
        }
        DESKTOP_WAIT_SYSCALL => {
            let Ok(kind) = u16::try_from(frame.r10) else {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            };
            if pid != DESKTOP_PID
                || frame.rsi != EVENT_SIZE as u64
                || !matches!(kind, EVENT_POLICY_APPLIED | EVENT_CONFIG_APPLIED)
            {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            if !validate_user_range(pid, frame.rdi, EVENT_SIZE, true) {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let request = DesktopWaitRequest {
                pid,
                destination: frame.rdi,
                kind,
                after_generation: frame.rdx,
            };
            suspend_io_syscall(pid, frame, PendingSyscall::DesktopWait(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=slopos-desktop-v1 entry=syscall return=kernel nr={DESKTOP_WAIT_SYSCALL} wait_event={} after_generation={} event_bytes={EVENT_SIZE} state=blocked origin=cpl3",
                desktop_event_name(request.kind),
                request.after_generation,
            ));
            2
        }
        WAYLAND_SURFACE_SYSCALL => {
            if pid != DESKTOP_PID
                || frame.rsi < WAYLAND_SURFACE_HEADER_SIZE as u64
                || frame.rsi > WAYLAND_SURFACE_MAX_SIZE as u64
            {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            let mut header_bytes = [0; WAYLAND_SURFACE_HEADER_SIZE];
            if copy_from_user(pid, frame.rdi, &mut header_bytes).is_none() {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let Ok(header) = WaylandSurfaceHeader::decode(&header_bytes) else {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            };
            let Ok(total_size) = header.total_size() else {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            };
            if frame.rsi != total_size as u64 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            // SAFETY: syscall entry is IF-masked, PID 2 was checked above,
            // and submission consumes the borrowed bytes before returning.
            let staging = unsafe { crate::wayland_service::staging_buffer() };
            if copy_from_user(pid, frame.rdi, &mut staging[..total_size]).is_none() {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let Ok(commit) = WaylandSurfaceCommit::decode(&staging[..total_size]) else {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            };
            let wire_length = commit.wire.len();
            let pixel_length = commit.pixels.len();
            match crate::wayland_service::submit(pid, commit) {
                Ok(submission) => {
                    let (phase, sequence) = match submission {
                        crate::wayland_service::WaylandSubmission::Registry { event_sequence } => {
                            ("registry", event_sequence)
                        }
                        crate::wayland_service::WaylandSubmission::Configure {
                            event_sequence,
                            ..
                        } => ("initial-commit", event_sequence),
                        crate::wayland_service::WaylandSubmission::Surface {
                            generation, ..
                        } => ("configured-commit", generation),
                    };
                    frame.rax = 0;
                    crate::serial::serialln(format_args!(
                        "SLOPOS-SYSCALL: pid={pid} abi=slopos-wayland-bootstrap-v1 entry=syscall return=sysretq nr={WAYLAND_SURFACE_SYSCALL} phase={phase} envelope_bytes={total_size} wire_bytes={wire_length} pixel_bytes={pixel_length} sequence={sequence} origin=cpl3 result=0"
                    ));
                }
                Err(_) => frame.rax = LINUX_EINVAL as u64,
            }
            0
        }
        WAYLAND_EVENT_WAIT_SYSCALL => {
            if pid != DESKTOP_PID || frame.rsi != WAYLAND_EVENT_MAX_SIZE as u64 {
                frame.rax = LINUX_EINVAL as u64;
                return 0;
            }
            if !validate_user_range(pid, frame.rdi, WAYLAND_EVENT_MAX_SIZE, true) {
                frame.rax = LINUX_EFAULT as u64;
                return 0;
            }
            let request = WaylandWaitRequest {
                pid,
                destination: frame.rdi,
                after_sequence: frame.rdx,
            };
            suspend_io_syscall(pid, frame, PendingSyscall::WaylandWait(request));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=slopos-wayland-bootstrap-v1 entry=syscall return=kernel nr={WAYLAND_EVENT_WAIT_SYSCALL} wait_event=server-wire after_sequence={} event_capacity={WAYLAND_EVENT_MAX_SIZE} state=blocked origin=cpl3",
                request.after_sequence
            ));
            2
        }
        LINUX_SYS_EXIT => {
            if frame.rdi > u64::from(u8::MAX) {
                crate::fatal("user exit syscall status is invalid");
            }
            let status = frame.rdi as i32;
            process_table_mut()
                .exit(pid, status)
                .unwrap_or_else(|_| crate::fatal("process-table exit transition failed"));
            crate::serial::serialln(format_args!(
                "SLOPOS-SYSCALL: pid={pid} abi=linux-x86_64 entry=syscall return=kernel nr=60 exit status={status} origin=cpl3"
            ));
            1
        }
        _ => {
            frame.rax = (-38i64) as u64;
            0
        }
    }
}

fn process_event_after_user(pid: u32) -> ProcessEvent {
    let process = process_table()
        .snapshot(pid)
        .unwrap_or_else(|_| crate::fatal("returning process disappeared"));
    if process.state == ProcessState::Exited {
        let expected_syscalls = match pid {
            INIT_PID => INIT_EXPECTED_SYSCALLS,
            DESKTOP_PID => DESKTOP_EXPECTED_SYSCALLS,
            _ => crate::fatal("unknown process exited"),
        };
        let preemptions = preemption_count(pid);
        if process.exit_status != Some(0)
            || process.syscall_count != expected_syscalls
            || pending_syscall(pid).is_some()
            || preemptions == 0
        {
            crate::fatal("process returned without a successful process-table exit");
        }
        crate::serial::serialln(format_args!(
            "SLOPOS-PROCESS: pid={pid} state=exited status=0 syscalls={expected_syscalls} preemptions={preemptions} retained=true kernel_return=true"
        ));
        return ProcessEvent::Exited { pid };
    }
    if process.state == ProcessState::Runnable && pending_syscall(pid).is_none() {
        return ProcessEvent::Preempted {
            pid,
            tick: last_preemption_tick(pid),
            count: preemption_count(pid),
        };
    }
    match (
        process.state,
        pending_syscall(pid)
            .unwrap_or_else(|| crate::fatal("returning process has no pending syscall")),
    ) {
        (ProcessState::Blocked, PendingSyscall::OpenAt(request)) => ProcessEvent::OpenAt(request),
        (ProcessState::Blocked, PendingSyscall::Read(request)) => ProcessEvent::Read(request),
        (ProcessState::Blocked, PendingSyscall::Write(request)) => ProcessEvent::Write(request),
        (ProcessState::Blocked, PendingSyscall::Close(request)) => ProcessEvent::Close(request),
        (ProcessState::Blocked, PendingSyscall::Wait(request)) => {
            ProcessEvent::Waiting { pid: request.pid }
        }
        (ProcessState::Blocked, PendingSyscall::DesktopWait(request)) => {
            ProcessEvent::DesktopWaiting(request)
        }
        (ProcessState::Blocked, PendingSyscall::WaylandWait(request)) => {
            ProcessEvent::WaylandWaiting(request)
        }
        (ProcessState::Runnable, PendingSyscall::Yield) => ProcessEvent::Yielded { pid },
        _ => crate::fatal("process returned with an inconsistent scheduler state"),
    }
}

fn suspend_io_syscall(pid: u32, frame: &SyscallFrame, request: PendingSyscall) {
    save_pending_syscall(pid, frame, request);
    process_table_mut()
        .mark_blocked(pid)
        .unwrap_or_else(|_| crate::fatal("process running-to-blocked transition failed"));
}

fn save_pending_syscall(pid: u32, frame: &SyscallFrame, request: PendingSyscall) {
    if pending_syscall(pid).is_some() {
        crate::fatal("process issued a second syscall while one was pending");
    }
    let index = pid_index(pid);
    // SAFETY: syscall entry is IF-masked and is the only writer until it
    // returns to the block task.
    unsafe {
        (*SAVED_SYSCALL_FRAMES.0.get())[index] = *frame;
        (*PENDING_SYSCALLS.0.get())[index] = Some(request);
    }
}

fn copy_user_path(pid: u32, address: u64, access_mode: AccessMode) -> Result<OpenAtRequest, i64> {
    let mut request = OpenAtRequest {
        pid,
        path: [0; PROCESS_SYSCALL_PATH_CAPACITY],
        path_length: 0,
        access_mode,
    };
    for index in 0..PROCESS_SYSCALL_PATH_CAPACITY {
        let index = u64::try_from(index).map_err(|_| LINUX_EFAULT)?;
        let byte_address = address.checked_add(index).ok_or(LINUX_EFAULT)?;
        let mut byte = [0u8; 1];
        copy_from_user(pid, byte_address, &mut byte).ok_or(LINUX_EFAULT)?;
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

fn copy_from_user(pid: u32, address: u64, output: &mut [u8]) -> Option<()> {
    if !validate_user_range(pid, address, output.len(), false) {
        return None;
    }
    let mut copied = 0usize;
    while copied < output.len() {
        let cursor = address.checked_add(u64::try_from(copied).ok()?)?;
        let (pointer, length) = user_physical_chunk(pid, cursor, output.len() - copied, false)?;
        // SAFETY: validation covered the entire source range before mutation;
        // each chunk is bounded to a live, identity-mapped user frame.
        unsafe {
            ptr::copy_nonoverlapping(pointer.cast_const(), output[copied..].as_mut_ptr(), length)
        };
        copied += length;
    }
    Some(())
}

fn copy_to_user(pid: u32, address: u64, bytes: &[u8]) -> Option<()> {
    if !validate_user_range(pid, address, bytes.len(), true) {
        return None;
    }
    let mut copied = 0usize;
    while copied < bytes.len() {
        let cursor = address.checked_add(u64::try_from(copied).ok()?)?;
        let (pointer, length) = user_physical_chunk(pid, cursor, bytes.len() - copied, true)?;
        // SAFETY: validation covered the entire destination before mutation;
        // this process is suspended and each chunk lies in a writable frame.
        unsafe {
            ptr::copy_nonoverlapping(bytes[copied..].as_ptr(), pointer, length);
        }
        copied += length;
    }
    Some(())
}

fn validate_user_range(pid: u32, address: u64, length: usize, writable: bool) -> bool {
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
        let Some((_, chunk)) = user_physical_chunk(pid, cursor, length - checked, writable) else {
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
    pid: u32,
    address: u64,
    maximum_length: usize,
    writable: bool,
) -> Option<(*mut u8, usize)> {
    let mapping = user_mapping(pid)?;
    if maximum_length == 0 {
        return Some((core::ptr::null_mut(), 0));
    }
    if !writable && address >= mapping.code_start && address < mapping.code_end {
        let offset = address.checked_sub(mapping.code_start)?;
        let page_index = usize::try_from(offset / PAGE_SIZE).ok()?;
        let page_offset = offset % PAGE_SIZE;
        let physical = mapping
            .code_frames
            .get(page_index)?
            .checked_add(page_offset)?;
        let segment_remaining = usize::try_from(mapping.code_end - address).ok()?;
        let page_remaining = usize::try_from(PAGE_SIZE - page_offset).ok()?;
        return Some((
            physical as *mut u8,
            maximum_length.min(segment_remaining).min(page_remaining),
        ));
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

fn set_user_mapping(pid: u32, mapping: UserMapping) {
    let index = pid_index(pid);
    // SAFETY: each slot is installed before its process starts, while no
    // user-copy operation can overlap.
    unsafe { (*USER_MAPPINGS.0.get())[index] = Some(mapping) };
}

fn user_mapping(pid: u32) -> Option<UserMapping> {
    let index = pid_index(pid);
    // SAFETY: immutable after installation for the lifetime of this process.
    unsafe { (*USER_MAPPINGS.0.get())[index] }
}

fn take_user_mapping(pid: u32) -> Option<UserMapping> {
    let index = pid_index(pid);
    // SAFETY: called by the block task after the process exited and can no
    // longer issue user-copy operations.
    unsafe { (*USER_MAPPINGS.0.get())[index].take() }
}

fn pending_syscall(pid: u32) -> Option<PendingSyscall> {
    let index = pid_index(pid);
    // SAFETY: access alternates between IF-masked syscall entry and the block
    // task while this process is suspended.
    unsafe { (*PENDING_SYSCALLS.0.get())[index] }
}

fn clear_pending_syscall(pid: u32) {
    let index = pid_index(pid);
    // SAFETY: called by the exclusive block-task scheduler/completer.
    unsafe { (*PENDING_SYSCALLS.0.get())[index] = None };
}

fn saved_syscall_frame(pid: u32) -> *const SyscallFrame {
    let index = pid_index(pid);
    // SAFETY: run_process validated the Runnable state before retrieving this
    // process-owned frame, which remains stored throughout the resume.
    unsafe { core::ptr::addr_of!((*SAVED_SYSCALL_FRAMES.0.get())[index]) }
}

fn saved_syscall_frame_mut(pid: u32) -> &'static mut SyscallFrame {
    let index = pid_index(pid);
    // SAFETY: the block task is the sole accessor while this process is not
    // executing in user mode.
    unsafe { &mut (*SAVED_SYSCALL_FRAMES.0.get())[index] }
}

fn set_current_process(pid: Option<u32>) {
    // SAFETY: only the single-core scheduler writes this around CPL3 entry.
    unsafe { *CURRENT_PROCESS.0.get() = pid };
}

fn current_process() -> Option<u32> {
    // SAFETY: only read by IF-masked syscall/timer entry for the active process.
    unsafe { *CURRENT_PROCESS.0.get() }
}

fn preemption_count(pid: u32) -> u64 {
    let index = pid_index(pid);
    // SAFETY: the process is suspended in the block-task continuation.
    unsafe { (*PREEMPTIONS.counts.get())[index] }
}

fn last_preemption_tick(pid: u32) -> u64 {
    let index = pid_index(pid);
    // SAFETY: the process is suspended in the block-task continuation.
    unsafe { (*PREEMPTIONS.ticks.get())[index] }
}

fn pid_index(pid: u32) -> usize {
    let index = pid
        .checked_sub(1)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_else(|| crate::fatal("process PID cannot index kernel state"));
    if index >= PROCESS_CAPACITY {
        crate::fatal("process PID exceeds kernel state capacity");
    }
    index
}

fn reset_process_state() {
    // SAFETY: this runs once before either user process starts.
    unsafe {
        PROCESS_TABLE.0.get().write(KernelProcessTable::new());
        USER_MAPPINGS.0.get().write([None; PROCESS_CAPACITY]);
        PENDING_SYSCALLS.0.get().write([None; PROCESS_CAPACITY]);
        SAVED_SYSCALL_FRAMES
            .0
            .get()
            .write([EMPTY_SYSCALL_FRAME; PROCESS_CAPACITY]);
        PREEMPTIONS.counts.get().write([0; PROCESS_CAPACITY]);
        PREEMPTIONS.ticks.get().write([0; PROCESS_CAPACITY]);
        CURRENT_PROCESS.0.get().write(None);
    }
}

fn process_table() -> &'static KernelProcessTable {
    // SAFETY: process execution is single-core; mutation only occurs before
    // entry, inside an IF-masked handler, or while a process is suspended.
    unsafe { &*PROCESS_TABLE.0.get() }
}

fn process_table_mut() -> &'static mut KernelProcessTable {
    // SAFETY: the scheduler, IF-masked syscall/timer handlers, and block-task
    // syscall completer are mutually exclusive writers.
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
    push 0x1b
    push qword ptr [rax + 136]
    push qword ptr [rax + 128]
    push 0x23
    push qword ptr [rax + 120]
    mov r15, [rax + 0]
    mov r14, [rax + 8]
    mov r13, [rax + 16]
    mov r12, [rax + 24]
    mov r11, [rax + 32]
    mov r10, [rax + 40]
    mov r9, [rax + 48]
    mov r8, [rax + 56]
    mov rbp, [rax + 64]
    mov rdi, [rax + 72]
    mov rsi, [rax + 80]
    mov rdx, [rax + 88]
    mov rcx, [rax + 96]
    mov rbx, [rax + 104]
    mov rax, [rax + 112]
    iretq
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
    jmp slopos_return_to_kernel

    .global slopos_return_to_kernel
    .type slopos_return_to_kernel, @function
slopos_return_to_kernel:
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
    .size slopos_return_to_kernel, .-slopos_return_to_kernel
    .size slopos_syscall_entry, .-slopos_syscall_entry
"#
);
