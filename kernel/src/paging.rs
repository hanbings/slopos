// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::ptr;
use slopos_boot_protocol::FramebufferInfo;

const ENTRY_COUNT: usize = 512;
const PAGE_SIZE: u64 = 4096;
const HUGE_PAGE_SIZE: u64 = 2 * 1024 * 1024;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const HUGE: u64 = 1 << 7;

pub struct PagingStats {
    pub pml4: u64,
    pub page_table_frames: usize,
    pub huge_pages: usize,
}

pub fn install(framebuffer: FramebufferInfo) -> PagingStats {
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

    let framebuffer_start = framebuffer.base & !(HUGE_PAGE_SIZE - 1);
    let framebuffer_end = framebuffer
        .base
        .saturating_add(framebuffer.size)
        .next_multiple_of(HUGE_PAGE_SIZE);
    let first_gib_index = (framebuffer_start >> 30) as usize;
    let last_gib_index = ((framebuffer_end.saturating_sub(1)) >> 30) as usize;
    for gib_index in first_gib_index..=last_gib_index {
        if gib_index == 0 {
            continue;
        }
        if gib_index >= ENTRY_COUNT {
            crate::fatal("framebuffer lies outside lower PML4 slot");
        }
        let directory = table_frame();
        page_table_frames += 1;
        set_entry(pdpt, gib_index, directory | PRESENT | WRITABLE);
        let region_start = gib_index as u64 * 1024 * 1024 * 1024;
        for index in 0..ENTRY_COUNT {
            let physical = region_start + index as u64 * HUGE_PAGE_SIZE;
            if physical >= framebuffer_start && physical < framebuffer_end {
                set_entry(directory, index, physical | PRESENT | WRITABLE | HUGE);
                huge_pages += 1;
            }
        }
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

fn table_frame() -> u64 {
    let frame = crate::memory::allocate_frame()
        .unwrap_or_else(|| crate::fatal("physical memory exhausted while building page tables"));
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
