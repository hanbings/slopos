// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use slopos_boot_protocol::MemoryMapInfo;

const PAGE_SIZE: u64 = 4096;
const UEFI_CONVENTIONAL_MEMORY: u32 = 7;
const MAX_REGIONS: usize = 128;

#[derive(Clone, Copy)]
struct Region {
    next: u64,
    end: u64,
}

impl Region {
    const EMPTY: Self = Self { next: 0, end: 0 };
}

struct Allocator {
    regions: [Region; MAX_REGIONS],
    count: usize,
    current: usize,
}

impl Allocator {
    const fn empty() -> Self {
        Self {
            regions: [Region::EMPTY; MAX_REGIONS],
            count: 0,
            current: 0,
        }
    }

    fn allocate_frame(&mut self) -> Option<u64> {
        self.allocate_contiguous(1)
    }

    fn allocate_contiguous(&mut self, page_count: usize) -> Option<u64> {
        let byte_count = (page_count as u64).checked_mul(PAGE_SIZE)?;
        while self.current < self.count {
            let region = &mut self.regions[self.current];
            if region
                .next
                .checked_add(byte_count)
                .is_some_and(|end| end <= region.end)
            {
                let start = region.next;
                region.next += byte_count;
                return Some(start);
            }
            self.current += 1;
        }
        None
    }
}

struct AllocatorCell {
    locked: AtomicBool,
    value: UnsafeCell<Allocator>,
}

// The atomic lock serializes every mutable access to the allocator.
unsafe impl Sync for AllocatorCell {}

static ALLOCATOR: AllocatorCell = AllocatorCell {
    locked: AtomicBool::new(false),
    value: UnsafeCell::new(Allocator::empty()),
};

pub struct MemoryStats {
    pub conventional_regions: usize,
    pub free_frames: u64,
}

pub fn initialize(map: MemoryMapInfo) -> MemoryStats {
    let mut guard = ALLOCATOR.lock();
    guard.count = 0;
    guard.current = 0;
    let mut free_frames = 0u64;

    for index in 0..map.descriptor_count as usize {
        let offset = index * map.descriptor_size as usize;
        if offset + 40 > map.size as usize || guard.count == MAX_REGIONS {
            break;
        }
        // UEFI descriptors may have a firmware-specific stride, so fields are
        // read at their ABI offsets rather than by casting the whole map.
        let descriptor = (map.base as usize + offset) as *const u8;
        // SAFETY: the loader supplied map byte range and descriptor count/stride.
        let memory_type = unsafe { ptr::read_unaligned(descriptor.cast::<u32>()) };
        if memory_type != UEFI_CONVENTIONAL_MEMORY {
            continue;
        }
        // SAFETY: offsets 8 and 24 are present in every version-1 descriptor.
        let physical_start = unsafe { ptr::read_unaligned(descriptor.add(8).cast::<u64>()) };
        let page_count = unsafe { ptr::read_unaligned(descriptor.add(24).cast::<u64>()) };
        let start = physical_start.max(0x10_0000).next_multiple_of(PAGE_SIZE);
        let end = physical_start.saturating_add(page_count.saturating_mul(PAGE_SIZE));
        if start >= end {
            continue;
        }
        let region_index = guard.count;
        guard.regions[region_index] = Region { next: start, end };
        guard.count += 1;
        free_frames += (end - start) / PAGE_SIZE;
    }

    MemoryStats {
        conventional_regions: guard.count,
        free_frames,
    }
}

pub fn allocate_frame() -> Option<u64> {
    ALLOCATOR.lock().allocate_frame()
}

pub fn allocate_contiguous(page_count: usize) -> Option<u64> {
    if page_count == 0 {
        return None;
    }
    ALLOCATOR.lock().allocate_contiguous(page_count)
}

struct AllocatorGuard<'a> {
    cell: &'a AllocatorCell,
}

impl AllocatorCell {
    fn lock(&self) -> AllocatorGuard<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        AllocatorGuard { cell: self }
    }
}

impl core::ops::Deref for AllocatorGuard<'_> {
    type Target = Allocator;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the guard holds the allocator's exclusive atomic lock.
        unsafe { &*self.cell.value.get() }
    }
}

impl core::ops::DerefMut for AllocatorGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the guard holds the allocator's exclusive atomic lock.
        unsafe { &mut *self.cell.value.get() }
    }
}

impl Drop for AllocatorGuard<'_> {
    fn drop(&mut self) {
        self.cell.locked.store(false, Ordering::Release);
    }
}
