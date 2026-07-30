// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, Ordering};

const HEAP_PAGES: usize = 256;
const PAGE_SIZE: usize = 4096;

struct HeapState {
    next: usize,
    end: usize,
}

struct Heap {
    locked: AtomicBool,
    state: UnsafeCell<HeapState>,
}

// All state access is serialized by the atomic lock.
unsafe impl Sync for Heap {}

static HEAP: Heap = Heap {
    locked: AtomicBool::new(false),
    state: UnsafeCell::new(HeapState { next: 0, end: 0 }),
};

pub struct HeapStats {
    pub base: u64,
    pub size: usize,
}

pub fn initialize() -> HeapStats {
    let base = crate::memory::allocate_contiguous(HEAP_PAGES)
        .unwrap_or_else(|| crate::fatal("cannot reserve contiguous kernel heap"));
    let mut state = HEAP.lock();
    state.next = base as usize;
    state.end = base as usize + HEAP_PAGES * PAGE_SIZE;
    HeapStats {
        base,
        size: HEAP_PAGES * PAGE_SIZE,
    }
}

pub fn allocate(size: usize, alignment: usize) -> Option<NonNull<u8>> {
    if size == 0 || !alignment.is_power_of_two() {
        return None;
    }
    let mut state = HEAP.lock();
    let aligned = state.next.checked_add(alignment - 1)? & !(alignment - 1);
    let end = aligned.checked_add(size)?;
    if end > state.end {
        return None;
    }
    state.next = end;
    NonNull::new(aligned as *mut u8)
}

struct HeapGuard<'a> {
    heap: &'a Heap,
}

impl Heap {
    fn lock(&self) -> HeapGuard<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        HeapGuard { heap: self }
    }
}

impl core::ops::Deref for HeapGuard<'_> {
    type Target = HeapState;

    fn deref(&self) -> &Self::Target {
        // SAFETY: this guard holds the heap's exclusive lock.
        unsafe { &*self.heap.state.get() }
    }
}

impl core::ops::DerefMut for HeapGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: this guard holds the heap's exclusive lock.
        unsafe { &mut *self.heap.state.get() }
    }
}

impl Drop for HeapGuard<'_> {
    fn drop(&mut self) {
        self.heap.locked.store(false, Ordering::Release);
    }
}
