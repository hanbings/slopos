// SPDX-License-Identifier: 0BSD

#![no_std]

use slopos_vfs::{AccessMode, FileDescriptorTable, FileNode, ReadWindow, VfsError, WriteWindow};

pub type ProcessId = u32;

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
    pub stack_top: u64,
    pub user_memory_start: u64,
    pub user_memory_end: u64,
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

    pub fn close_fd(&mut self, pid: ProcessId, fd: u32) -> Result<(), ProcessError> {
        self.live_slot_mut(pid)?
            .descriptors
            .close(fd)
            .map_err(ProcessError::Vfs)
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

fn validate_image(image: ProcessImage) -> Result<(), ProcessError> {
    if image.address_space_root == 0
        || image.address_space_root & 0xfff != 0
        || image.user_memory_start >= image.user_memory_end
        || image.entry < image.user_memory_start
        || image.entry >= image.user_memory_end
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
    }
}
