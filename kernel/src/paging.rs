// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::ptr;
use slopos_boot_protocol::FramebufferInfo;

const ENTRY_COUNT: usize = 512;
const PAGE_SIZE: u64 = 4096;
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const WRITE_THROUGH: u64 = 1 << 3;
const CACHE_DISABLE: u64 = 1 << 4;
const USER_ACCESSIBLE: u64 = 1 << 2;
const HUGE: u64 = 1 << 7;
pub const USER_CODE_BASE: u64 = 0x4000_0000;
pub const USER_CODE_PAGES: usize = 2;
pub const USER_STACK_PAGES: usize = 2;
pub const USER_STACK_BASE: u64 = USER_CODE_BASE + USER_CODE_PAGES as u64 * PAGE_SIZE;
pub const USER_STACK_TOP: u64 = USER_STACK_BASE + USER_STACK_PAGES as u64 * PAGE_SIZE;

pub struct PagingStats {
    pub pml4: u64,
    pub page_table_frames: usize,
    pub huge_pages: usize,
}

#[derive(Clone, Copy)]
pub struct MmioRange {
    pub base: u64,
    pub size: u64,
}

#[derive(Clone, Copy)]
pub struct UserAddressSpace {
    pub root: u64,
    pub code_frames: [u64; USER_CODE_PAGES],
    pub stack_frames: [u64; USER_STACK_PAGES],
    pub table_frames: [u64; 4],
}

pub fn install(framebuffer: FramebufferInfo, mmio_ranges: &[MmioRange]) -> PagingStats {
    let pml4 = table_frame();
    let pdpt = table_frame();
    set_entry(pml4, 0, pdpt | PRESENT | WRITABLE);

    let mut page_table_frames = 2;
    let mut huge_pages = 0;

    // Identity-map the first GiB: kernel, inherited stack, ACPI, initrd,
    // UEFI memory map, heap, and all current QEMU RAM live in this range.
    let low_directory = table_frame();
    page_table_frames += 1;
    set_entry(pdpt, 0, low_directory | PRESENT | WRITABLE);
    for index in 0..ENTRY_COUNT {
        set_entry(
            low_directory,
            index,
            (index as u64 * HUGE_PAGE_SIZE) | PRESENT | WRITABLE | HUGE,
        );
        huge_pages += 1;
    }

    map_mmio_range(
        pml4,
        framebuffer.base,
        framebuffer.size,
        &mut page_table_frames,
        &mut huge_pages,
    );
    for range in mmio_ranges {
        if range.base == 0 || range.size == 0 {
            continue;
        }
        map_mmio_range(
            pml4,
            range.base,
            range.size,
            &mut page_table_frames,
            &mut huge_pages,
        );
    }

    // SAFETY: every active kernel, stack, boot-data and framebuffer address is
    // identity-mapped above; PML4 is a zeroed, aligned physical frame.
    unsafe {
        asm!("mov cr3, {root}", root = in(reg) pml4, options(nostack, preserves_flags));
    }
    let active_root: u64;
    // SAFETY: reading CR3 is side-effect free at ring 0.
    unsafe {
        asm!("mov {root}, cr3", root = out(reg) active_root, options(nostack, preserves_flags))
    };
    if active_root & !(PAGE_SIZE - 1) != pml4 {
        crate::fatal("CR3 page-table switch did not persist");
    }

    PagingStats {
        pml4,
        page_table_frames,
        huge_pages,
    }
}

pub fn create_user_address_space(image: &[u8], memory_size: u64) -> UserAddressSpace {
    if image.is_empty()
        || memory_size == 0
        || memory_size > USER_CODE_PAGES as u64 * PAGE_SIZE
        || image.len() > usize::try_from(memory_size).unwrap_or(0)
    {
        crate::fatal("user ELF segment does not fit the code mapping");
    }
    let active_root = current_root();
    let root = table_frame();
    for index in 0..ENTRY_COUNT {
        set_entry(root, index, get_entry(active_root, index));
    }

    let active_low_entry = get_entry(active_root, 0);
    if active_low_entry & PRESENT == 0 {
        crate::fatal("kernel low address space is missing");
    }
    let active_low_pdpt = active_low_entry & !(PAGE_SIZE - 1);
    let process_low_pdpt = table_frame();
    for index in 0..ENTRY_COUNT {
        set_entry(process_low_pdpt, index, get_entry(active_low_pdpt, index));
    }
    set_entry(
        root,
        0,
        process_low_pdpt | PRESENT | WRITABLE | USER_ACCESSIBLE,
    );

    let user_directory = table_frame();
    let user_table = table_frame();
    let code_frames = [data_frame(), data_frame()];
    let stack_frames = [data_frame(), data_frame()];
    for (index, frame) in code_frames.iter().copied().enumerate() {
        let source_start = index * PAGE_SIZE as usize;
        let source_end = core::cmp::min(source_start + PAGE_SIZE as usize, image.len());
        if source_start >= source_end {
            break;
        }
        // SAFETY: each code frame is exclusive and zeroed, and this chunk is
        // bounded to one page of the validated ELF segment.
        unsafe {
            ptr::copy_nonoverlapping(
                image[source_start..source_end].as_ptr(),
                frame as *mut u8,
                source_end - source_start,
            );
        }
    }
    let pdpt_index = ((USER_CODE_BASE >> 30) & 0x1ff) as usize;
    let directory_index = ((USER_CODE_BASE >> 21) & 0x1ff) as usize;
    let code_index = ((USER_CODE_BASE >> 12) & 0x1ff) as usize;
    let first_stack_index = code_index + USER_CODE_PAGES;
    let last_stack_index = first_stack_index + USER_STACK_PAGES - 1;
    if get_entry(process_low_pdpt, pdpt_index) & PRESENT != 0 || last_stack_index >= ENTRY_COUNT {
        crate::fatal("user virtual address range overlaps a kernel mapping");
    }
    set_entry(
        process_low_pdpt,
        pdpt_index,
        user_directory | PRESENT | WRITABLE | USER_ACCESSIBLE,
    );
    set_entry(
        user_directory,
        directory_index,
        user_table | PRESENT | WRITABLE | USER_ACCESSIBLE,
    );
    for (index, frame) in code_frames.iter().copied().enumerate() {
        set_entry(
            user_table,
            code_index + index,
            frame | PRESENT | USER_ACCESSIBLE,
        );
    }
    for (index, frame) in stack_frames.iter().copied().enumerate() {
        set_entry(
            user_table,
            first_stack_index + index,
            frame | PRESENT | WRITABLE | USER_ACCESSIBLE,
        );
    }

    UserAddressSpace {
        root,
        code_frames,
        stack_frames,
        table_frames: [root, process_low_pdpt, user_directory, user_table],
    }
}

pub fn release_user_address_space(address_space: UserAddressSpace) -> usize {
    let mut released = 0usize;
    for frame in address_space
        .table_frames
        .into_iter()
        .chain(address_space.code_frames)
        .chain(address_space.stack_frames)
    {
        crate::memory::deallocate_frame(frame)
            .unwrap_or_else(|_| crate::fatal("user address-space frame release failed"));
        released += 1;
    }
    released
}

fn current_root() -> u64 {
    let root: u64;
    // SAFETY: reading CR3 is side-effect free at ring 0.
    unsafe { asm!("mov {root}, cr3", root = out(reg) root, options(nostack, preserves_flags)) };
    root & !(PAGE_SIZE - 1)
}

fn map_mmio_range(
    pml4: u64,
    base: u64,
    size: u64,
    page_table_frames: &mut usize,
    huge_pages: &mut usize,
) {
    let start = base & !(HUGE_PAGE_SIZE - 1);
    let end = base.saturating_add(size).next_multiple_of(HUGE_PAGE_SIZE);
    let mut physical = start;
    while physical < end {
        if physical >> 48 != 0 {
            crate::fatal("MMIO range lies outside lower canonical address space");
        }
        let pml4_index = ((physical >> 39) & 0x1ff) as usize;
        let mut pdpt_entry = get_entry(pml4, pml4_index);
        if pdpt_entry & PRESENT == 0 {
            let new_pdpt = table_frame();
            *page_table_frames += 1;
            pdpt_entry = new_pdpt | PRESENT | WRITABLE;
            set_entry(pml4, pml4_index, pdpt_entry);
        }
        let target_pdpt = pdpt_entry & !(PAGE_SIZE - 1);
        let gib_index = ((physical >> 30) & 0x1ff) as usize;
        let mut directory_entry = get_entry(target_pdpt, gib_index);
        if directory_entry & PRESENT == 0 {
            let directory = table_frame();
            *page_table_frames += 1;
            directory_entry = directory | PRESENT | WRITABLE;
            set_entry(target_pdpt, gib_index, directory_entry);
        }
        let directory = directory_entry & !(PAGE_SIZE - 1);
        let index = ((physical >> 21) & 0x1ff) as usize;
        let mapping = get_entry(directory, index);
        if mapping & PRESENT == 0 {
            *huge_pages += 1;
        }
        if mapping & (WRITE_THROUGH | CACHE_DISABLE) != (WRITE_THROUGH | CACHE_DISABLE) {
            set_entry(
                directory,
                index,
                physical | PRESENT | WRITABLE | WRITE_THROUGH | CACHE_DISABLE | HUGE,
            );
        }
        physical = physical.saturating_add(HUGE_PAGE_SIZE);
    }
}

fn table_frame() -> u64 {
    data_frame()
}

fn data_frame() -> u64 {
    let frame = crate::memory::allocate_frame()
        .unwrap_or_else(|| crate::fatal("physical memory exhausted while allocating a page"));
    // SAFETY: frame is exclusive and accessible through the inherited identity map.
    unsafe { ptr::write_bytes(frame as *mut u8, 0, PAGE_SIZE as usize) };
    frame
}

fn set_entry(table: u64, index: usize, value: u64) {
    if index >= ENTRY_COUNT {
        crate::fatal("page-table index overflow");
    }
    // SAFETY: table is an exclusive, page-aligned 512-entry frame.
    unsafe { ptr::write_volatile((table as *mut u64).add(index), value) };
}

fn get_entry(table: u64, index: usize) -> u64 {
    if index >= ENTRY_COUNT {
        crate::fatal("page-table index overflow");
    }
    // SAFETY: table is an initialized, page-aligned 512-entry frame.
    unsafe { ptr::read_volatile((table as *const u64).add(index)) }
}
