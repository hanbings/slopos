// SPDX-License-Identifier: 0BSD

#![no_std]

use slopos_vfs::{AccessMode, FileDescriptorTable, FileNode, ReadWindow, VfsError, WriteWindow};

pub type ProcessId = u32;

pub const MAX_INITIAL_ARGUMENTS: usize = 16;
pub const MAX_INITIAL_ENVIRONMENT: usize = 16;
pub const LINUX_AUXV_PAIRS: usize = 9;

const LINUX_AT_NULL: u64 = 0;
const LINUX_AT_PAGESZ: u64 = 6;
const LINUX_AT_ENTRY: u64 = 9;
const LINUX_AT_UID: u64 = 11;
const LINUX_AT_EUID: u64 = 12;
const LINUX_AT_GID: u64 = 13;
const LINUX_AT_EGID: u64 = 14;
const LINUX_AT_SECURE: u64 = 23;
const LINUX_AT_EXECFN: u64 = 31;
const INITIAL_STACK_ALIGNMENT: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    Ready,
    Running,
    Exited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessImage {
    pub address_space_root: u64,
    pub entry: u64,
    pub stack_pointer: u64,
    pub stack_top: u64,
    pub user_memory_start: u64,
    pub user_memory_end: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitialStackLayout {
    pub stack_pointer: u64,
    pub argument_count: usize,
    pub environment_count: usize,
    pub auxiliary_pairs: usize,
    pub used_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialStackError {
    MissingExecutableName,
    TooManyArguments,
    TooManyEnvironmentEntries,
    EmptyString,
    EmbeddedNull,
    AddressOverflow,
    StackTooSmall,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessSnapshot {
    pub pid: ProcessId,
    pub parent: Option<ProcessId>,
    pub state: ProcessState,
    pub image: ProcessImage,
    pub exit_status: Option<i32>,
    pub syscall_count: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessError {
    InvalidImage,
    TableFull,
    PidExhausted,
    NotFound,
    InvalidState,
    CounterOverflow,
    Vfs(VfsError),
}

struct ProcessSlot<const FDS: usize> {
    pid: ProcessId,
    parent: Option<ProcessId>,
    state: Option<ProcessState>,
    image: ProcessImage,
    exit_status: Option<i32>,
    syscall_count: u64,
    descriptors: FileDescriptorTable<FDS>,
}

impl<const FDS: usize> ProcessSlot<FDS> {
    const fn empty() -> Self {
        Self {
            pid: 0,
            parent: None,
            state: None,
            image: ProcessImage {
                address_space_root: 0,
                entry: 0,
                stack_pointer: 0,
                stack_top: 0,
                user_memory_start: 0,
                user_memory_end: 0,
            },
            exit_status: None,
            syscall_count: 0,
            descriptors: FileDescriptorTable::new(),
        }
    }

    fn snapshot(&self) -> Option<ProcessSnapshot> {
        Some(ProcessSnapshot {
            pid: self.pid,
            parent: self.parent,
            state: self.state?,
            image: self.image,
            exit_status: self.exit_status,
            syscall_count: self.syscall_count,
        })
    }
}

pub struct ProcessTable<const N: usize, const FDS: usize> {
    slots: [ProcessSlot<FDS>; N],
    next_pid: ProcessId,
    count: usize,
}

impl<const N: usize, const FDS: usize> ProcessTable<N, FDS> {
    pub const fn new() -> Self {
        Self {
            slots: [const { ProcessSlot::empty() }; N],
            next_pid: 1,
            count: 0,
        }
    }

    pub fn spawn(
        &mut self,
        parent: Option<ProcessId>,
        image: ProcessImage,
    ) -> Result<ProcessId, ProcessError> {
        validate_image(image)?;
        if let Some(parent) = parent
            && self.slot(parent).is_none()
        {
            return Err(ProcessError::NotFound);
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.state.is_none())
            .ok_or(ProcessError::TableFull)?;
        let pid = self.next_pid;
        self.next_pid = self
            .next_pid
            .checked_add(1)
            .ok_or(ProcessError::PidExhausted)?;
        *slot = ProcessSlot {
            pid,
            parent,
            state: Some(ProcessState::Ready),
            image,
            exit_status: None,
            syscall_count: 0,
            descriptors: FileDescriptorTable::new(),
        };
        self.count += 1;
        Ok(pid)
    }

    pub fn mark_running(&mut self, pid: ProcessId) -> Result<(), ProcessError> {
        let slot = self.slot_mut(pid).ok_or(ProcessError::NotFound)?;
        if slot.state != Some(ProcessState::Ready) {
            return Err(ProcessError::InvalidState);
        }
        slot.state = Some(ProcessState::Running);
        Ok(())
    }

    pub fn record_syscall(&mut self, pid: ProcessId) -> Result<u64, ProcessError> {
        let slot = self.running_slot_mut(pid)?;
        slot.syscall_count = slot
            .syscall_count
            .checked_add(1)
            .ok_or(ProcessError::CounterOverflow)?;
        Ok(slot.syscall_count)
    }

    pub fn exit(&mut self, pid: ProcessId, status: i32) -> Result<(), ProcessError> {
        let slot = self.running_slot_mut(pid)?;
        slot.state = Some(ProcessState::Exited);
        slot.exit_status = Some(status);
        Ok(())
    }

    pub fn reap(&mut self, pid: ProcessId) -> Result<ProcessSnapshot, ProcessError> {
        let slot = self.slot_mut(pid).ok_or(ProcessError::NotFound)?;
        if slot.state != Some(ProcessState::Exited) {
            return Err(ProcessError::InvalidState);
        }
        let snapshot = slot.snapshot().ok_or(ProcessError::NotFound)?;
        *slot = ProcessSlot::empty();
        self.count -= 1;
        Ok(snapshot)
    }

    pub fn snapshot(&self, pid: ProcessId) -> Result<ProcessSnapshot, ProcessError> {
        self.slot(pid)
            .and_then(ProcessSlot::snapshot)
            .ok_or(ProcessError::NotFound)
    }

    pub fn open_file(
        &mut self,
        pid: ProcessId,
        node: FileNode,
        access_mode: AccessMode,
    ) -> Result<u32, ProcessError> {
        self.live_slot_mut(pid)?
            .descriptors
            .open_with_mode(node, access_mode)
            .map_err(ProcessError::Vfs)
    }

    pub fn read_window(
        &self,
        pid: ProcessId,
        fd: u32,
        requested: usize,
    ) -> Result<ReadWindow, ProcessError> {
        self.live_slot(pid)?
            .descriptors
            .read_window(fd, requested)
            .map_err(ProcessError::Vfs)
    }

    pub fn write_window(
        &self,
        pid: ProcessId,
        fd: u32,
        requested: usize,
    ) -> Result<WriteWindow, ProcessError> {
        self.live_slot(pid)?
            .descriptors
            .write_window(fd, requested)
            .map_err(ProcessError::Vfs)
    }

    pub fn advance_fd(
        &mut self,
        pid: ProcessId,
        fd: u32,
        length: usize,
    ) -> Result<(), ProcessError> {
        self.live_slot_mut(pid)?
            .descriptors
            .advance(fd, length)
            .map_err(ProcessError::Vfs)
    }

    pub fn seek_fd(&mut self, pid: ProcessId, fd: u32, offset: u64) -> Result<(), ProcessError> {
        self.live_slot_mut(pid)?
            .descriptors
            .seek(fd, offset)
            .map_err(ProcessError::Vfs)
    }

    pub fn close_fd(&mut self, pid: ProcessId, fd: u32) -> Result<(), ProcessError> {
        self.live_slot_mut(pid)?
            .descriptors
            .close(fd)
            .map_err(ProcessError::Vfs)
    }

    pub fn close_all_files(&mut self, pid: ProcessId) -> Result<usize, ProcessError> {
        let slot = self.slot_mut(pid).ok_or(ProcessError::NotFound)?;
        if slot.state != Some(ProcessState::Exited) {
            return Err(ProcessError::InvalidState);
        }
        Ok(slot.descriptors.close_all())
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub const fn capacity(&self) -> usize {
        N
    }

    pub const fn fd_capacity(&self) -> usize {
        FDS
    }

    fn slot(&self, pid: ProcessId) -> Option<&ProcessSlot<FDS>> {
        self.slots
            .iter()
            .find(|slot| slot.state.is_some() && slot.pid == pid)
    }

    fn slot_mut(&mut self, pid: ProcessId) -> Option<&mut ProcessSlot<FDS>> {
        self.slots
            .iter_mut()
            .find(|slot| slot.state.is_some() && slot.pid == pid)
    }

    fn running_slot_mut(&mut self, pid: ProcessId) -> Result<&mut ProcessSlot<FDS>, ProcessError> {
        let slot = self.slot_mut(pid).ok_or(ProcessError::NotFound)?;
        if slot.state != Some(ProcessState::Running) {
            return Err(ProcessError::InvalidState);
        }
        Ok(slot)
    }

    fn live_slot(&self, pid: ProcessId) -> Result<&ProcessSlot<FDS>, ProcessError> {
        let slot = self.slot(pid).ok_or(ProcessError::NotFound)?;
        if slot.state == Some(ProcessState::Exited) {
            return Err(ProcessError::InvalidState);
        }
        Ok(slot)
    }

    fn live_slot_mut(&mut self, pid: ProcessId) -> Result<&mut ProcessSlot<FDS>, ProcessError> {
        let slot = self.slot_mut(pid).ok_or(ProcessError::NotFound)?;
        if slot.state == Some(ProcessState::Exited) {
            return Err(ProcessError::InvalidState);
        }
        Ok(slot)
    }
}

impl<const N: usize, const FDS: usize> Default for ProcessTable<N, FDS> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_linux_initial_stack(
    stack: &mut [u8],
    stack_base: u64,
    arguments: &[&[u8]],
    environment: &[&[u8]],
    entry: u64,
    page_size: u64,
) -> Result<InitialStackLayout, InitialStackError> {
    if arguments.is_empty() {
        return Err(InitialStackError::MissingExecutableName);
    }
    if arguments.len() > MAX_INITIAL_ARGUMENTS {
        return Err(InitialStackError::TooManyArguments);
    }
    if environment.len() > MAX_INITIAL_ENVIRONMENT {
        return Err(InitialStackError::TooManyEnvironmentEntries);
    }
    let stack_top = stack_base
        .checked_add(u64::try_from(stack.len()).map_err(|_| InitialStackError::AddressOverflow)?)
        .ok_or(InitialStackError::AddressOverflow)?;
    let mut argument_pointers = [0u64; MAX_INITIAL_ARGUMENTS];
    let mut environment_pointers = [0u64; MAX_INITIAL_ENVIRONMENT];
    let mut cursor = stack.len();

    for index in (0..environment.len()).rev() {
        environment_pointers[index] =
            push_initial_string(stack, stack_base, &mut cursor, environment[index])?;
    }
    for index in (0..arguments.len()).rev() {
        argument_pointers[index] =
            push_initial_string(stack, stack_base, &mut cursor, arguments[index])?;
    }

    let table_words = 1usize
        .checked_add(arguments.len())
        .and_then(|words| words.checked_add(1))
        .and_then(|words| words.checked_add(environment.len()))
        .and_then(|words| words.checked_add(1))
        .and_then(|words| words.checked_add(LINUX_AUXV_PAIRS * 2))
        .ok_or(InitialStackError::StackTooSmall)?;
    let table_bytes = table_words
        .checked_mul(core::mem::size_of::<u64>())
        .ok_or(InitialStackError::StackTooSmall)?;
    let table_unaligned = cursor
        .checked_sub(table_bytes)
        .ok_or(InitialStackError::StackTooSmall)?;
    let table_start = table_unaligned & !(INITIAL_STACK_ALIGNMENT - 1);
    let stack_pointer = stack_base
        .checked_add(u64::try_from(table_start).map_err(|_| InitialStackError::AddressOverflow)?)
        .ok_or(InitialStackError::AddressOverflow)?;
    if stack_pointer & (INITIAL_STACK_ALIGNMENT as u64 - 1) != 0 || stack_pointer >= stack_top {
        return Err(InitialStackError::StackTooSmall);
    }

    let auxiliary = [
        (LINUX_AT_PAGESZ, page_size),
        (LINUX_AT_ENTRY, entry),
        (LINUX_AT_UID, 0),
        (LINUX_AT_EUID, 0),
        (LINUX_AT_GID, 0),
        (LINUX_AT_EGID, 0),
        (LINUX_AT_SECURE, 0),
        (LINUX_AT_EXECFN, argument_pointers[0]),
        (LINUX_AT_NULL, 0),
    ];
    let mut word = 0usize;
    write_initial_word(stack, table_start, &mut word, arguments.len() as u64)?;
    for pointer in &argument_pointers[..arguments.len()] {
        write_initial_word(stack, table_start, &mut word, *pointer)?;
    }
    write_initial_word(stack, table_start, &mut word, 0)?;
    for pointer in &environment_pointers[..environment.len()] {
        write_initial_word(stack, table_start, &mut word, *pointer)?;
    }
    write_initial_word(stack, table_start, &mut word, 0)?;
    for (kind, value) in auxiliary {
        write_initial_word(stack, table_start, &mut word, kind)?;
        write_initial_word(stack, table_start, &mut word, value)?;
    }
    if word != table_words {
        return Err(InitialStackError::StackTooSmall);
    }

    Ok(InitialStackLayout {
        stack_pointer,
        argument_count: arguments.len(),
        environment_count: environment.len(),
        auxiliary_pairs: LINUX_AUXV_PAIRS,
        used_bytes: stack.len() - table_start,
    })
}

fn push_initial_string(
    stack: &mut [u8],
    stack_base: u64,
    cursor: &mut usize,
    value: &[u8],
) -> Result<u64, InitialStackError> {
    if value.is_empty() {
        return Err(InitialStackError::EmptyString);
    }
    if value.contains(&0) {
        return Err(InitialStackError::EmbeddedNull);
    }
    let length = value
        .len()
        .checked_add(1)
        .ok_or(InitialStackError::StackTooSmall)?;
    *cursor = cursor
        .checked_sub(length)
        .ok_or(InitialStackError::StackTooSmall)?;
    let end = cursor
        .checked_add(value.len())
        .ok_or(InitialStackError::StackTooSmall)?;
    stack
        .get_mut(*cursor..end)
        .ok_or(InitialStackError::StackTooSmall)?
        .copy_from_slice(value);
    *stack.get_mut(end).ok_or(InitialStackError::StackTooSmall)? = 0;
    stack_base
        .checked_add(u64::try_from(*cursor).map_err(|_| InitialStackError::AddressOverflow)?)
        .ok_or(InitialStackError::AddressOverflow)
}

fn write_initial_word(
    stack: &mut [u8],
    table_start: usize,
    word: &mut usize,
    value: u64,
) -> Result<(), InitialStackError> {
    let offset = word
        .checked_mul(core::mem::size_of::<u64>())
        .and_then(|offset| table_start.checked_add(offset))
        .ok_or(InitialStackError::StackTooSmall)?;
    let end = offset
        .checked_add(core::mem::size_of::<u64>())
        .ok_or(InitialStackError::StackTooSmall)?;
    stack
        .get_mut(offset..end)
        .ok_or(InitialStackError::StackTooSmall)?
        .copy_from_slice(&value.to_le_bytes());
    *word += 1;
    Ok(())
}

fn validate_image(image: ProcessImage) -> Result<(), ProcessError> {
    if image.address_space_root == 0
        || image.address_space_root & 0xfff != 0
        || image.user_memory_start >= image.user_memory_end
        || image.entry < image.user_memory_start
        || image.entry >= image.user_memory_end
        || image.stack_pointer <= image.user_memory_end
        || image.stack_pointer > image.stack_top
        || image.stack_top <= image.user_memory_end
    {
        return Err(ProcessError::InvalidImage);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMAGE: ProcessImage = ProcessImage {
        address_space_root: 0x1000,
        entry: 0x4000_0000,
        stack_pointer: 0x4000_1f00,
        stack_top: 0x4000_2000,
        user_memory_start: 0x4000_0000,
        user_memory_end: 0x4000_0100,
    };

    #[test]
    fn allocates_bounded_pids_and_validates_images() {
        let mut table = ProcessTable::<2, 4>::new();
        assert_eq!(table.spawn(None, IMAGE), Ok(1));
        assert_eq!(table.spawn(Some(1), IMAGE), Ok(2));
        assert_eq!(table.len(), 2);
        assert_eq!(table.capacity(), 2);
        assert_eq!(table.fd_capacity(), 4);
        assert_eq!(table.spawn(None, IMAGE), Err(ProcessError::TableFull));

        let mut invalid = IMAGE;
        invalid.entry = invalid.user_memory_end;
        let mut empty = ProcessTable::<1, 1>::new();
        assert_eq!(empty.spawn(None, invalid), Err(ProcessError::InvalidImage));
        assert_eq!(empty.spawn(Some(99), IMAGE), Err(ProcessError::NotFound));
    }

    #[test]
    fn builds_aligned_linux_initial_stack_with_arguments_environment_and_auxv() {
        let mut stack = [0u8; 512];
        let layout = build_linux_initial_stack(
            &mut stack,
            0x4000_1000,
            &[b"/sbin/slop-init", b"--system"],
            &[
                b"SLOPOS_SESSION=desktop",
                b"XDG_CURRENT_DESKTOP=SlopOS",
                b"WAYLAND_DISPLAY=wayland-0",
            ],
            IMAGE.entry,
            4096,
        )
        .unwrap();
        assert_eq!(layout.stack_pointer & 15, 0);
        assert_eq!(layout.argument_count, 2);
        assert_eq!(layout.environment_count, 3);
        assert_eq!(layout.auxiliary_pairs, LINUX_AUXV_PAIRS);
        assert!(layout.used_bytes < stack.len());

        let table_offset = usize::try_from(layout.stack_pointer - 0x4000_1000).unwrap();
        let mut words = [0u64; 26];
        for (word, bytes) in words.iter_mut().zip(stack[table_offset..].chunks_exact(8)) {
            *word = u64::from_le_bytes(bytes.try_into().unwrap());
        }
        assert_eq!(words[0], 2);
        assert_eq!(stack_string(&stack, words[1]), b"/sbin/slop-init");
        assert_eq!(stack_string(&stack, words[2]), b"--system");
        assert_eq!(words[3], 0);
        assert_eq!(stack_string(&stack, words[4]), b"SLOPOS_SESSION=desktop");
        assert_eq!(
            stack_string(&stack, words[5]),
            b"XDG_CURRENT_DESKTOP=SlopOS"
        );
        assert_eq!(stack_string(&stack, words[6]), b"WAYLAND_DISPLAY=wayland-0");
        assert_eq!(words[7], 0);
        assert_eq!(
            &words[8..26],
            &[
                LINUX_AT_PAGESZ,
                4096,
                LINUX_AT_ENTRY,
                IMAGE.entry,
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
        );
    }

    #[test]
    fn rejects_invalid_or_oversized_linux_initial_stacks() {
        let mut stack = [0u8; 128];
        assert_eq!(
            build_linux_initial_stack(&mut stack, 0x1000, &[], &[], IMAGE.entry, 4096),
            Err(InitialStackError::MissingExecutableName)
        );
        assert_eq!(
            build_linux_initial_stack(
                &mut stack,
                0x1000,
                &[b"/sbin/slop-init", b""],
                &[],
                IMAGE.entry,
                4096,
            ),
            Err(InitialStackError::EmptyString)
        );
        assert_eq!(
            build_linux_initial_stack(
                &mut stack,
                0x1000,
                &[b"/sbin/slop\0init"],
                &[],
                IMAGE.entry,
                4096,
            ),
            Err(InitialStackError::EmbeddedNull)
        );
        assert_eq!(
            build_linux_initial_stack(
                &mut [0u8; 64],
                0x1000,
                &[b"/sbin/slop-init"],
                &[b"SLOPOS_SESSION=desktop"],
                IMAGE.entry,
                4096,
            ),
            Err(InitialStackError::StackTooSmall)
        );
    }

    #[test]
    fn enforces_ready_running_exited_and_reaped_transitions() {
        let mut table = ProcessTable::<1, 1>::new();
        let pid = table.spawn(None, IMAGE).unwrap();
        assert_eq!(table.snapshot(pid).unwrap().state, ProcessState::Ready);
        assert_eq!(table.record_syscall(pid), Err(ProcessError::InvalidState));
        table.mark_running(pid).unwrap();
        assert_eq!(table.record_syscall(pid), Ok(1));
        assert_eq!(table.record_syscall(pid), Ok(2));
        table.exit(pid, 7).unwrap();
        let exited = table.snapshot(pid).unwrap();
        assert_eq!(exited.state, ProcessState::Exited);
        assert_eq!(exited.exit_status, Some(7));
        assert_eq!(exited.syscall_count, 2);
        assert_eq!(table.mark_running(pid), Err(ProcessError::InvalidState));
        assert_eq!(table.reap(pid).unwrap(), exited);
        assert!(table.is_empty());
        assert_eq!(table.snapshot(pid), Err(ProcessError::NotFound));
    }

    #[test]
    fn keeps_file_descriptor_tables_per_process() {
        let mut table = ProcessTable::<2, 2>::new();
        let first = table.spawn(None, IMAGE).unwrap();
        let second = table.spawn(Some(first), IMAGE).unwrap();
        let node = FileNode {
            filesystem_id: 1,
            node_id: 42,
            size: 16,
        };
        let first_fd = table.open_file(first, node, AccessMode::ReadOnly).unwrap();
        let second_fd = table.open_file(second, node, AccessMode::ReadOnly).unwrap();
        assert_eq!(first_fd, 3);
        assert_eq!(second_fd, 3);
        table.advance_fd(first, first_fd, 6).unwrap();
        assert_eq!(table.read_window(first, first_fd, 4).unwrap().offset, 6);
        assert_eq!(table.read_window(second, second_fd, 4).unwrap().offset, 0);
        table.seek_fd(first, first_fd, 2).unwrap();
        assert_eq!(table.read_window(first, first_fd, 4).unwrap().offset, 2);
        assert_eq!(
            table.seek_fd(first, first_fd, 17),
            Err(ProcessError::Vfs(VfsError::InvalidOffset))
        );
        table.close_fd(first, first_fd).unwrap();
        assert_eq!(
            table.read_window(first, first_fd, 1),
            Err(ProcessError::Vfs(VfsError::BadFileDescriptor))
        );
    }

    #[test]
    fn rejects_descriptor_access_after_exit() {
        let mut table = ProcessTable::<1, 1>::new();
        let pid = table.spawn(None, IMAGE).unwrap();
        table.mark_running(pid).unwrap();
        let fd = table
            .open_file(
                pid,
                FileNode {
                    filesystem_id: 1,
                    node_id: 1,
                    size: 0,
                },
                AccessMode::ReadOnly,
            )
            .unwrap();
        assert_eq!(fd, 3);
        assert_eq!(table.close_all_files(pid), Err(ProcessError::InvalidState));
        table.exit(pid, 0).unwrap();
        assert_eq!(
            table.open_file(
                pid,
                FileNode {
                    filesystem_id: 1,
                    node_id: 1,
                    size: 0,
                },
                AccessMode::ReadOnly,
            ),
            Err(ProcessError::InvalidState)
        );
        assert_eq!(table.close_all_files(pid), Ok(1));
        assert_eq!(table.close_all_files(pid), Ok(0));
    }

    fn stack_string(stack: &[u8], pointer: u64) -> &[u8] {
        let start = usize::try_from(pointer - 0x4000_1000).unwrap();
        let end = stack[start..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|length| start + length)
            .unwrap();
        &stack[start..end]
    }
}
