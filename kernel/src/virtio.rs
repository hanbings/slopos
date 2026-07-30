// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::future::Future;
use core::mem::size_of;
use core::pin::Pin;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering, fence};
use core::task::{Context, Poll};
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
const BLOCK_STATUS_OK: u8 = 0;
const QUEUE_LIMIT: u16 = 8;
const SECTOR_SIZE: usize = 512;
const DATA_PAGE_SIZE: usize = 4096;

static ISR_BASE: AtomicU64 = AtomicU64::new(0);
static INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static QUEUE_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);

pub struct BlockDevice {
    common_base: usize,
    descriptor_page: usize,
    available_page: usize,
    used_page: usize,
    control_page: usize,
    data_page: usize,
    notify_address: usize,
    queue_size: u16,
    capacity_sectors: u64,
    available_index: u16,
}

pub fn initialize_block(
    device: Device,
    common: VirtioRegion,
    notify: VirtioRegion,
    isr: VirtioRegion,
    device_configuration: VirtioRegion,
) -> BlockDevice {
    if common.length < 56 || notify.length < 2 || isr.length < 1 || device_configuration.length < 8
    {
        crate::fatal("virtio PCI capability region is too short");
    }
    ISR_BASE.store(isr.base, Ordering::Release);
    INTERRUPT_COUNT.store(0, Ordering::Relaxed);
    QUEUE_INTERRUPT_COUNT.store(0, Ordering::Relaxed);
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
    let control_page = allocate_zeroed_frame();
    let data_page = allocate_zeroed_frame();
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

    // SAFETY: all queue pages are exclusive, zeroed physical frames
    // identity-mapped in the current address space and remain live forever.
    unsafe {
        let available = available_page as *mut u16;
        ptr::write_volatile(available, 0);
        let notify_offset = queue_notify_offset
            .checked_mul(u64::from(notify.notify_multiplier))
            .unwrap_or_else(|| fail(common_base, "virtio notify offset overflow"));
        if notify_offset + 2 > u64::from(notify.length) {
            fail(common_base, "virtio notify address is outside capability");
        }
        let notify_address = (notify.base + notify_offset) as usize;
        BlockDevice {
            common_base,
            descriptor_page,
            available_page,
            used_page,
            control_page,
            data_page,
            notify_address,
            queue_size,
            capacity_sectors,
            available_index: 0,
        }
    }
}

pub fn interrupt_top_half() {
    let base = ISR_BASE.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // SAFETY: initialize_block validated and published the mapped one-byte ISR
    // capability before the IOAPIC route can be enabled.
    let status = unsafe { ptr::read_volatile(base as *const u8) };
    INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
    if status & 1 != 0 {
        QUEUE_INTERRUPT_COUNT.fetch_add(1, Ordering::Relaxed);
        crate::executor::wake_task(crate::executor::BLOCK_TASK);
    }
}

pub fn interrupt_counts() -> (u64, u64) {
    (
        INTERRUPT_COUNT.load(Ordering::Acquire),
        QUEUE_INTERRUPT_COUNT.load(Ordering::Acquire),
    )
}

impl BlockDevice {
    pub(crate) async fn read(&mut self, sector: u64, byte_count: usize) {
        if byte_count == 0
            || byte_count > DATA_PAGE_SIZE
            || byte_count % SECTOR_SIZE != 0
            || sector
                .checked_add((byte_count / SECTOR_SIZE) as u64)
                .is_none_or(|end| end > self.capacity_sectors)
        {
            fail(
                self.common_base,
                "virtio block read is outside device bounds",
            );
        }
        let expected_used_index = self.available_index.wrapping_add(1);
        // SAFETY: a previous request completed before this reusable descriptor
        // chain and its exclusive control/data pages are rewritten.
        unsafe {
            ptr::write_volatile(
                self.control_page as *mut BlockRequestHeader,
                BlockRequestHeader {
                    request_type: BLOCK_REQUEST_IN,
                    reserved: 0,
                    sector,
                },
            );
            ptr::write_bytes(self.data_page as *mut u8, 0, byte_count);
            let status = self.control_page + size_of::<BlockRequestHeader>();
            ptr::write_volatile(status as *mut u8, 0xff);
            let chain = block_read_descriptors(
                self.control_page as u64,
                size_of::<BlockRequestHeader>() as u32,
                self.data_page as u64,
                byte_count as u32,
                status as u64,
            );
            let descriptors = self.descriptor_page as *mut Descriptor;
            for (index, descriptor) in chain.into_iter().enumerate() {
                ptr::write_volatile(descriptors.add(index), descriptor);
            }
            let ring_slot = usize::from(self.available_index % self.queue_size);
            ptr::write_volatile(
                (self.available_page + 4 + ring_slot * size_of::<u16>()) as *mut u16,
                0,
            );
            fence(Ordering::Release);
            ptr::write_volatile((self.available_page + 2) as *mut u16, expected_used_index);
            self.available_index = expected_used_index;
            fence(Ordering::SeqCst);
            ptr::write_volatile(self.notify_address as *mut u16, 0);
        }
        Completion {
            used_page: self.used_page,
            expected_used_index,
        }
        .await;
        fence(Ordering::Acquire);
        // SAFETY: the used index for this chain is visible, so the device has
        // completed its write to the one-byte status field.
        if unsafe {
            ptr::read_volatile((self.control_page + size_of::<BlockRequestHeader>()) as *const u8)
        } != BLOCK_STATUS_OK
        {
            fail(self.common_base, "virtio block request returned an error");
        }
    }

    pub(crate) fn data(&self, byte_count: usize) -> &[u8] {
        if byte_count > DATA_PAGE_SIZE {
            self.fail("virtio block buffer view is out of bounds");
        }
        // SAFETY: data_page is a permanently live DMA frame and callers only
        // request the completed prefix, bounded by DATA_PAGE_SIZE.
        unsafe { core::slice::from_raw_parts(self.data_page as *const u8, byte_count) }
    }

    pub(crate) const fn queue_size(&self) -> u16 {
        self.queue_size
    }

    pub(crate) const fn capacity_sectors(&self) -> u64 {
        self.capacity_sectors
    }

    pub(crate) const fn request_count(&self) -> u16 {
        self.available_index
    }

    pub(crate) fn fail(&self, message: &'static str) -> ! {
        fail(self.common_base, message)
    }
}

struct Completion {
    used_page: usize,
    expected_used_index: u16,
}

impl Future for Completion {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: used_page is a live, exclusive split used-ring frame.
        let used_index = unsafe { ptr::read_volatile((self.used_page + 2) as *const u16) };
        if used_index == self.expected_used_index {
            Poll::Ready(())
        } else {
            Poll::Pending
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
