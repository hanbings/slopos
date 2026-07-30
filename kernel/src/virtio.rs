// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::mem::size_of;
use core::ptr;
use core::sync::atomic::{Ordering, fence};
use slopos_pci::{Device, VirtioRegion};
use slopos_virtio::{
    BLOCK_REQUEST_IN, BlockRequestHeader, Descriptor, block_read_descriptors, choose_queue_size,
};

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FEATURES_OK: u8 = 8;
const STATUS_FAILED: u8 = 128;
const FEATURE_VERSION_1: u32 = 1;
const COMMON_DEVICE_FEATURE_SELECT: usize = 0;
const COMMON_DEVICE_FEATURE: usize = 4;
const COMMON_DRIVER_FEATURE_SELECT: usize = 8;
const COMMON_DRIVER_FEATURE: usize = 12;
const COMMON_NUM_QUEUES: usize = 18;
const COMMON_DEVICE_STATUS: usize = 20;
const COMMON_CONFIG_GENERATION: usize = 21;
const COMMON_QUEUE_SELECT: usize = 22;
const COMMON_QUEUE_SIZE: usize = 24;
const COMMON_QUEUE_ENABLE: usize = 28;
const COMMON_QUEUE_NOTIFY_OFFSET: usize = 30;
const COMMON_QUEUE_DESCRIPTOR: usize = 32;
const COMMON_QUEUE_DRIVER: usize = 40;
const COMMON_QUEUE_DEVICE: usize = 48;
const AVAILABLE_NO_INTERRUPT: u16 = 1;
const BLOCK_STATUS_OK: u8 = 0;
const SECTOR_SIZE: usize = 512;
const QUEUE_LIMIT: u16 = 8;
const WAIT_LIMIT: usize = 100_000_000;

pub struct BlockStats {
    pub queue_size: u16,
    pub capacity_sectors: u64,
    pub sector_signature: [u8; 2],
}

pub fn initialize_block(
    device: Device,
    common: VirtioRegion,
    notify: VirtioRegion,
    device_configuration: VirtioRegion,
) -> BlockStats {
    if common.length < 56 || notify.length < 2 || device_configuration.length < 8 {
        crate::fatal("virtio PCI capability region is too short");
    }
    crate::pci::enable_memory_bus_master(device);
    let common_base = common.base as usize;

    write_u8(common_base, COMMON_DEVICE_STATUS, 0);
    for _ in 0..100_000 {
        if read_u8(common_base, COMMON_DEVICE_STATUS) == 0 {
            break;
        }
        spin();
    }
    if read_u8(common_base, COMMON_DEVICE_STATUS) != 0 {
        crate::fatal("virtio device reset timed out");
    }
    write_u8(
        common_base,
        COMMON_DEVICE_STATUS,
        STATUS_ACKNOWLEDGE | STATUS_DRIVER,
    );

    write_u32(common_base, COMMON_DEVICE_FEATURE_SELECT, 1);
    let high_features = read_u32(common_base, COMMON_DEVICE_FEATURE);
    if high_features & FEATURE_VERSION_1 == 0 {
        fail(common_base, "virtio modern VERSION_1 feature is absent");
    }
    write_u32(common_base, COMMON_DRIVER_FEATURE_SELECT, 0);
    write_u32(common_base, COMMON_DRIVER_FEATURE, 0);
    write_u32(common_base, COMMON_DRIVER_FEATURE_SELECT, 1);
    write_u32(common_base, COMMON_DRIVER_FEATURE, FEATURE_VERSION_1);
    write_u8(
        common_base,
        COMMON_DEVICE_STATUS,
        STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
    );
    if read_u8(common_base, COMMON_DEVICE_STATUS) & STATUS_FEATURES_OK == 0 {
        fail(common_base, "virtio feature negotiation was rejected");
    }

    if read_u16(common_base, COMMON_NUM_QUEUES) == 0 {
        fail(common_base, "virtio block exposes no queues");
    }
    write_u16(common_base, COMMON_QUEUE_SELECT, 0);
    let offered_queue_size = read_u16(common_base, COMMON_QUEUE_SIZE);
    let queue_size = choose_queue_size(offered_queue_size, QUEUE_LIMIT)
        .unwrap_or_else(|| fail(common_base, "virtio queue is too small"));
    write_u16(common_base, COMMON_QUEUE_SIZE, queue_size);

    let descriptor_page = allocate_zeroed_frame();
    let available_page = allocate_zeroed_frame();
    let used_page = allocate_zeroed_frame();
    let request_page = allocate_zeroed_frame();
    write_u64(common_base, COMMON_QUEUE_DESCRIPTOR, descriptor_page as u64);
    write_u64(common_base, COMMON_QUEUE_DRIVER, available_page as u64);
    write_u64(common_base, COMMON_QUEUE_DEVICE, used_page as u64);
    write_u16(common_base, COMMON_QUEUE_ENABLE, 1);
    if read_u16(common_base, COMMON_QUEUE_ENABLE) != 1 {
        fail(common_base, "virtio queue enable did not persist");
    }
    let queue_notify_offset = u64::from(read_u16(common_base, COMMON_QUEUE_NOTIFY_OFFSET));

    write_u8(
        common_base,
        COMMON_DEVICE_STATUS,
        STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
    );
    if read_u8(common_base, COMMON_DEVICE_STATUS) & STATUS_DRIVER_OK == 0 {
        fail(common_base, "virtio DRIVER_OK did not persist");
    }

    let capacity_sectors = read_stable_capacity(common_base, device_configuration.base as usize);
    if capacity_sectors == 0 {
        fail(common_base, "virtio block capacity is zero");
    }

    // SAFETY: all queue/request pages are exclusive, zeroed physical frames
    // identity-mapped in the current address space and remain live forever.
    unsafe {
        let request_header = request_page as *mut BlockRequestHeader;
        ptr::write_volatile(
            request_header,
            BlockRequestHeader {
                request_type: BLOCK_REQUEST_IN,
                reserved: 0,
                sector: 0,
            },
        );
        let data = (request_page + size_of::<BlockRequestHeader>()) as *mut u8;
        ptr::write_bytes(data, 0, SECTOR_SIZE);
        let status = data.add(SECTOR_SIZE);
        ptr::write_volatile(status, 0xff);

        let descriptors = descriptor_page as *mut Descriptor;
        let chain = block_read_descriptors(
            request_page as u64,
            size_of::<BlockRequestHeader>() as u32,
            data as u64,
            SECTOR_SIZE as u32,
            status as u64,
        );
        for (index, descriptor) in chain.into_iter().enumerate() {
            ptr::write_volatile(descriptors.add(index), descriptor);
        }

        let available = available_page as *mut u16;
        ptr::write_volatile(available, AVAILABLE_NO_INTERRUPT);
        ptr::write_volatile(available.add(2), 0);
        fence(Ordering::Release);
        ptr::write_volatile(available.add(1), 1);
        fence(Ordering::SeqCst);

        let notify_offset = queue_notify_offset
            .checked_mul(u64::from(notify.notify_multiplier))
            .unwrap_or_else(|| fail(common_base, "virtio notify offset overflow"));
        if notify_offset + 2 > u64::from(notify.length) {
            fail(common_base, "virtio notify address is outside capability");
        }
        ptr::write_volatile((notify.base + notify_offset) as *mut u16, 0);

        let used_index = (used_page + 2) as *const u16;
        let mut completed = false;
        for _ in 0..WAIT_LIMIT {
            if ptr::read_volatile(used_index) == 1 {
                completed = true;
                break;
            }
            spin();
        }
        if !completed {
            fail(common_base, "virtio block request timed out");
        }
        fence(Ordering::Acquire);
        if ptr::read_volatile(status) != BLOCK_STATUS_OK {
            fail(common_base, "virtio block request returned an error");
        }
        let signature = [
            ptr::read_volatile(data.add(510)),
            ptr::read_volatile(data.add(511)),
        ];
        if signature != [0x55, 0xaa] {
            fail(common_base, "virtio sector zero boot signature mismatch");
        }
        BlockStats {
            queue_size,
            capacity_sectors,
            sector_signature: signature,
        }
    }
}

fn allocate_zeroed_frame() -> usize {
    let frame = crate::memory::allocate_frame()
        .unwrap_or_else(|| crate::fatal("out of frames for virtio queue"));
    // SAFETY: allocator returned an exclusive 4 KiB identity-mapped frame.
    unsafe { ptr::write_bytes(frame as *mut u8, 0, 4096) };
    frame as usize
}

fn read_stable_capacity(common_base: usize, device_base: usize) -> u64 {
    for _ in 0..100 {
        let before = read_u8(common_base, COMMON_CONFIG_GENERATION);
        let capacity = read_u64(device_base, 0);
        let after = read_u8(common_base, COMMON_CONFIG_GENERATION);
        if before == after {
            return capacity;
        }
    }
    fail(
        common_base,
        "virtio configuration generation did not stabilize",
    )
}

fn fail(common_base: usize, message: &'static str) -> ! {
    let status = read_u8(common_base, COMMON_DEVICE_STATUS);
    write_u8(common_base, COMMON_DEVICE_STATUS, status | STATUS_FAILED);
    crate::fatal(message)
}

fn read_u8(base: usize, offset: usize) -> u8 {
    // SAFETY: caller validated the mapped virtio capability extent.
    unsafe { ptr::read_volatile((base + offset) as *const u8) }
}

fn read_u16(base: usize, offset: usize) -> u16 {
    // SAFETY: virtio common fields have their specified natural alignment.
    unsafe { ptr::read_volatile((base + offset) as *const u16) }
}

fn read_u32(base: usize, offset: usize) -> u32 {
    // SAFETY: virtio common fields have their specified natural alignment.
    unsafe { ptr::read_volatile((base + offset) as *const u32) }
}

fn read_u64(base: usize, offset: usize) -> u64 {
    // SAFETY: virtio device configuration capacity is naturally aligned.
    unsafe { ptr::read_volatile((base + offset) as *const u64) }
}

fn write_u8(base: usize, offset: usize, value: u8) {
    // SAFETY: caller validated the mapped virtio capability extent.
    unsafe { ptr::write_volatile((base + offset) as *mut u8, value) };
}

fn write_u16(base: usize, offset: usize, value: u16) {
    // SAFETY: virtio common fields have their specified natural alignment.
    unsafe { ptr::write_volatile((base + offset) as *mut u16, value) };
}

fn write_u32(base: usize, offset: usize, value: u32) {
    // SAFETY: virtio common fields have their specified natural alignment.
    unsafe { ptr::write_volatile((base + offset) as *mut u32, value) };
}

fn write_u64(base: usize, offset: usize, value: u64) {
    // SAFETY: virtio queue address fields are naturally aligned.
    unsafe { ptr::write_volatile((base + offset) as *mut u64, value) };
}

fn spin() {
    // SAFETY: PAUSE is a side-effect-free hint in bounded polling loops.
    unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
}
