// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::ptr;

const PAGE_SIZE: usize = 4096;
const OBJECT_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedMemoryHandle {
    index: u16,
    generation: u16,
}

impl SharedMemoryHandle {
    pub const fn from_parts(index: u16, generation: u16) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u16 {
        self.index
    }

    pub const fn generation(self) -> u16 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedMemoryError {
    TableFull,
    InvalidHandle,
    InvalidLength,
    PermissionDenied,
    CounterOverflow,
    FrameRelease,
}

#[derive(Clone, Copy)]
struct SharedMemorySlot {
    generation: u16,
    owner_pid: u32,
    frame: u64,
    length: usize,
    references: u16,
    allocated: bool,
}

impl SharedMemorySlot {
    const EMPTY: Self = Self {
        generation: 0,
        owner_pid: 0,
        frame: 0,
        length: 0,
        references: 0,
        allocated: false,
    };
}

struct SharedMemoryTable {
    slots: [SharedMemorySlot; OBJECT_CAPACITY],
}

impl SharedMemoryTable {
    const fn new() -> Self {
        Self {
            slots: [SharedMemorySlot::EMPTY; OBJECT_CAPACITY],
        }
    }

    fn index(&self, handle: SharedMemoryHandle) -> Result<usize, SharedMemoryError> {
        let index = usize::from(handle.index);
        self.slots
            .get(index)
            .filter(|slot| slot.allocated && slot.generation == handle.generation)
            .map(|_| index)
            .ok_or(SharedMemoryError::InvalidHandle)
    }
}

struct SharedState(UnsafeCell<SharedMemoryTable>);

// SAFETY: allocation, syscall entry, socket completion and process cleanup are
// serialized on SlopOS's bootstrap processor. User mode cannot run while the
// kernel is inspecting a shared page on behalf of that same process.
unsafe impl Sync for SharedState {}

static STATE: SharedState = SharedState(UnsafeCell::new(SharedMemoryTable::new()));

fn state_mut() -> &'static mut SharedMemoryTable {
    // SAFETY: justified by SharedState's single-processor ownership contract.
    unsafe { &mut *STATE.0.get() }
}

pub fn create(owner_pid: u32) -> Result<SharedMemoryHandle, SharedMemoryError> {
    let state = state_mut();
    let index = state
        .slots
        .iter()
        .position(|slot| !slot.allocated)
        .ok_or(SharedMemoryError::TableFull)?;
    let generation = state.slots[index]
        .generation
        .checked_add(1)
        .ok_or(SharedMemoryError::CounterOverflow)?;
    let frame = crate::memory::allocate_frame().ok_or(SharedMemoryError::TableFull)?;
    // SAFETY: the allocator returned an exclusive identity-mapped page.
    unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE) };
    state.slots[index] = SharedMemorySlot {
        generation,
        owner_pid,
        frame,
        length: 0,
        references: 1,
        allocated: true,
    };
    Ok(SharedMemoryHandle {
        index: u16::try_from(index).map_err(|_| SharedMemoryError::TableFull)?,
        generation,
    })
}

pub fn create_initialized(
    owner_pid: u32,
    bytes: &[u8],
    nul_terminated: bool,
) -> Result<SharedMemoryHandle, SharedMemoryError> {
    let length = bytes
        .len()
        .checked_add(usize::from(nul_terminated))
        .ok_or(SharedMemoryError::InvalidLength)?;
    if bytes.is_empty() || length > PAGE_SIZE || (nul_terminated && bytes.contains(&0)) {
        return Err(SharedMemoryError::InvalidLength);
    }
    let handle = create(owner_pid)?;
    if let Err(error) = truncate(owner_pid, handle, length) {
        let _ = release(handle);
        return Err(error);
    }
    let (frame, _) = frame_and_length(handle)?;
    // SAFETY: create returned an exclusive zeroed page retained by this
    // object. `length <= PAGE_SIZE`, and the optional terminator remains zero.
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), frame as *mut u8, bytes.len()) };
    Ok(handle)
}

pub fn truncate(
    owner_pid: u32,
    handle: SharedMemoryHandle,
    length: usize,
) -> Result<(), SharedMemoryError> {
    if length == 0 || length > PAGE_SIZE {
        return Err(SharedMemoryError::InvalidLength);
    }
    let state = state_mut();
    let index = state.index(handle)?;
    let slot = &mut state.slots[index];
    if slot.owner_pid != owner_pid {
        return Err(SharedMemoryError::PermissionDenied);
    }
    slot.length = length;
    Ok(())
}

pub fn retain(handle: SharedMemoryHandle) -> Result<(), SharedMemoryError> {
    let state = state_mut();
    let index = state.index(handle)?;
    state.slots[index].references = state.slots[index]
        .references
        .checked_add(1)
        .ok_or(SharedMemoryError::CounterOverflow)?;
    Ok(())
}

pub fn release(handle: SharedMemoryHandle) -> Result<(), SharedMemoryError> {
    let state = state_mut();
    let index = state.index(handle)?;
    let slot = &mut state.slots[index];
    slot.references = slot
        .references
        .checked_sub(1)
        .ok_or(SharedMemoryError::InvalidHandle)?;
    if slot.references != 0 {
        return Ok(());
    }
    let frame = slot.frame;
    let generation = slot.generation;
    *slot = SharedMemorySlot {
        generation,
        ..SharedMemorySlot::EMPTY
    };
    crate::memory::deallocate_frame(frame).map_err(|_| SharedMemoryError::FrameRelease)
}

pub fn frame_and_length(handle: SharedMemoryHandle) -> Result<(u64, usize), SharedMemoryError> {
    let state = state_mut();
    let index = state.index(handle)?;
    let slot = state.slots[index];
    if slot.length == 0 {
        return Err(SharedMemoryError::InvalidLength);
    }
    Ok((slot.frame, slot.length))
}

pub fn bytes(handle: SharedMemoryHandle) -> Result<&'static [u8], SharedMemoryError> {
    let (frame, length) = frame_and_length(handle)?;
    // SAFETY: the object retains the identity-mapped frame. Callers inspect it
    // only while its owning process is suspended in the kernel, so user writes
    // cannot race this immutable borrow on the current single-core runtime.
    Ok(unsafe { core::slice::from_raw_parts(frame as *const u8, length) })
}

pub fn read_at(
    handle: SharedMemoryHandle,
    offset: usize,
    output: &mut [u8],
) -> Result<usize, SharedMemoryError> {
    let bytes = bytes(handle)?;
    let remaining = bytes
        .get(offset..)
        .ok_or(SharedMemoryError::InvalidLength)?;
    let length = output.len().min(remaining.len());
    output[..length].copy_from_slice(&remaining[..length]);
    Ok(length)
}
