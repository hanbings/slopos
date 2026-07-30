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
    BLOCK_FEATURE_FLUSH, BLOCK_FEATURE_READ_ONLY, BLOCK_REQUEST_FLUSH, BLOCK_REQUEST_IN,
    BLOCK_REQUEST_OUT, BlockRequestHeader, Descriptor, block_flush_descriptors_at,
    block_read_descriptors_at, block_write_descriptors_at, choose_queue_size,
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
const REQUEST_SLOT_COUNT: usize = 2;
const DESCRIPTORS_PER_REQUEST: u16 = 3;

static ISR_BASE: AtomicU64 = AtomicU64::new(0);
static INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);
static QUEUE_INTERRUPT_COUNT: AtomicU64 = AtomicU64::new(0);

pub struct BlockDevice {
    common_base: usize,
    descriptor_page: usize,
    available_page: usize,
    used_page: usize,
    request_slots: [RequestSlot; REQUEST_SLOT_COUNT],
    notify_address: usize,
    queue_size: u16,
    capacity_sectors: u64,
    flush_supported: bool,
    available_index: u16,
}

#[derive(Clone, Copy)]
struct RequestSlot {
    control_page: usize,
    data_page: usize,
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

    write_u32(common_base, COMMON_DEVICE_FEATURE_SELECT, 0);
    let low_features = read_u32(common_base, COMMON_DEVICE_FEATURE);
    if low_features & BLOCK_FEATURE_READ_ONLY != 0 {
        fail(common_base, "virtio block device is read-only");
    }
    let negotiated_low_features = low_features & BLOCK_FEATURE_FLUSH;
    write_u32(common_base, COMMON_DEVICE_FEATURE_SELECT, 1);
    let high_features = read_u32(common_base, COMMON_DEVICE_FEATURE);
    if high_features & FEATURE_VERSION_1 == 0 {
        fail(common_base, "virtio modern VERSION_1 feature is absent");
    }
    write_u32(common_base, COMMON_DRIVER_FEATURE_SELECT, 0);
    write_u32(common_base, COMMON_DRIVER_FEATURE, negotiated_low_features);
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
    if usize::from(queue_size) < REQUEST_SLOT_COUNT * DESCRIPTORS_PER_REQUEST as usize {
        fail(common_base, "virtio queue cannot hold two block requests");
    }
    write_u16(common_base, COMMON_QUEUE_SIZE, queue_size);

    let descriptor_page = allocate_zeroed_frame();
    let available_page = allocate_zeroed_frame();
    let used_page = allocate_zeroed_frame();
    let request_slots = [
        RequestSlot {
            control_page: allocate_zeroed_frame(),
            data_page: allocate_zeroed_frame(),
        },
        RequestSlot {
            control_page: allocate_zeroed_frame(),
            data_page: allocate_zeroed_frame(),
        },
    ];
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
            request_slots,
            notify_address,
            queue_size,
            capacity_sectors,
            flush_supported: negotiated_low_features & BLOCK_FEATURE_FLUSH != 0,
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
        self.validate_transfer(sector, byte_count);
        self.prepare_read(0, sector, byte_count);
        let expected_used_index = self.publish(&[0]);
        Completion {
            used_page: self.used_page,
            expected_used_index,
        }
        .await;
        fence(Ordering::Acquire);
        self.validate_status(0);
    }

    pub(crate) async fn read_pair(
        &mut self,
        first_sector: u64,
        second_sector: u64,
        byte_count: usize,
    ) {
        self.validate_transfer(first_sector, byte_count);
        self.validate_transfer(second_sector, byte_count);
        self.prepare_read(0, first_sector, byte_count);
        self.prepare_read(1, second_sector, byte_count);
        let expected_used_index = self.publish(&[0, DESCRIPTORS_PER_REQUEST]);
        Completion {
            used_page: self.used_page,
            expected_used_index,
        }
        .await;
        fence(Ordering::Acquire);
        self.validate_status(0);
        self.validate_status(1);
    }

    pub(crate) async fn write(&mut self, sector: u64, data: &[u8]) {
        self.validate_transfer(sector, data.len());
        self.prepare_write(0, sector, data);
        let expected_used_index = self.publish(&[0]);
        Completion {
            used_page: self.used_page,
            expected_used_index,
        }
        .await;
        fence(Ordering::Acquire);
        self.validate_status(0);
    }

    pub(crate) async fn flush(&mut self) {
        if !self.flush_supported {
            self.fail("virtio block device does not support flush");
        }
        self.prepare_flush(0);
        let expected_used_index = self.publish(&[0]);
        Completion {
            used_page: self.used_page,
            expected_used_index,
        }
        .await;
        fence(Ordering::Acquire);
        self.validate_status(0);
    }

    pub(crate) fn data(&self, byte_count: usize) -> &[u8] {
        self.slot_data(0, byte_count)
    }

    pub(crate) fn pair_data(&self, slot: usize, byte_count: usize) -> &[u8] {
        if slot >= REQUEST_SLOT_COUNT {
            self.fail("virtio request slot is out of bounds");
        }
        self.slot_data(slot, byte_count)
    }

    fn slot_data(&self, slot: usize, byte_count: usize) -> &[u8] {
        if byte_count > DATA_PAGE_SIZE {
            self.fail("virtio block buffer view is out of bounds");
        }
        // SAFETY: data_page is a permanently live DMA frame and callers only
        // request the completed prefix, bounded by DATA_PAGE_SIZE.
        unsafe {
            core::slice::from_raw_parts(self.request_slots[slot].data_page as *const u8, byte_count)
        }
    }

    fn validate_transfer(&self, sector: u64, byte_count: usize) {
        if byte_count == 0
            || byte_count > DATA_PAGE_SIZE
            || byte_count % SECTOR_SIZE != 0
            || sector
                .checked_add((byte_count / SECTOR_SIZE) as u64)
                .is_none_or(|end| end > self.capacity_sectors)
        {
            self.fail("virtio block transfer is outside device bounds");
        }
    }

    fn prepare_read(&self, slot_index: usize, sector: u64, byte_count: usize) {
        let slot = self.request_slots[slot_index];
        let head = slot_index as u16 * DESCRIPTORS_PER_REQUEST;
        // SAFETY: this slot is not in flight; it owns its control/data pages
        // and three descriptor entries until the next used-index completion.
        unsafe {
            ptr::write_volatile(
                slot.control_page as *mut BlockRequestHeader,
                BlockRequestHeader {
                    request_type: BLOCK_REQUEST_IN,
                    reserved: 0,
                    sector,
                },
            );
            ptr::write_bytes(slot.data_page as *mut u8, 0, byte_count);
            let status = slot.control_page + size_of::<BlockRequestHeader>();
            ptr::write_volatile(status as *mut u8, 0xff);
            let chain = block_read_descriptors_at(
                head,
                slot.control_page as u64,
                size_of::<BlockRequestHeader>() as u32,
                slot.data_page as u64,
                byte_count as u32,
                status as u64,
            );
            let descriptors = self.descriptor_page as *mut Descriptor;
            for (offset, descriptor) in chain.into_iter().enumerate() {
                ptr::write_volatile(descriptors.add(usize::from(head) + offset), descriptor);
            }
        }
    }

    fn prepare_write(&self, slot_index: usize, sector: u64, data: &[u8]) {
        let slot = self.request_slots[slot_index];
        let head = slot_index as u16 * DESCRIPTORS_PER_REQUEST;
        // SAFETY: this slot is not in flight and the copied input remains in
        // its permanently allocated DMA page through completion.
        unsafe {
            ptr::write_volatile(
                slot.control_page as *mut BlockRequestHeader,
                BlockRequestHeader {
                    request_type: BLOCK_REQUEST_OUT,
                    reserved: 0,
                    sector,
                },
            );
            ptr::copy_nonoverlapping(data.as_ptr(), slot.data_page as *mut u8, data.len());
            let status = slot.control_page + size_of::<BlockRequestHeader>();
            ptr::write_volatile(status as *mut u8, 0xff);
            let chain = block_write_descriptors_at(
                head,
                slot.control_page as u64,
                size_of::<BlockRequestHeader>() as u32,
                slot.data_page as u64,
                data.len() as u32,
                status as u64,
            );
            let descriptors = self.descriptor_page as *mut Descriptor;
            for (offset, descriptor) in chain.into_iter().enumerate() {
                ptr::write_volatile(descriptors.add(usize::from(head) + offset), descriptor);
            }
        }
    }

    fn prepare_flush(&self, slot_index: usize) {
        let slot = self.request_slots[slot_index];
        let head = slot_index as u16 * DESCRIPTORS_PER_REQUEST;
        // SAFETY: this slot is not in flight and the two-entry flush chain
        // remains owned by the driver until its used-index completion.
        unsafe {
            ptr::write_volatile(
                slot.control_page as *mut BlockRequestHeader,
                BlockRequestHeader {
                    request_type: BLOCK_REQUEST_FLUSH,
                    reserved: 0,
                    sector: 0,
                },
            );
            let status = slot.control_page + size_of::<BlockRequestHeader>();
            ptr::write_volatile(status as *mut u8, 0xff);
            let chain = block_flush_descriptors_at(
                head,
                slot.control_page as u64,
                size_of::<BlockRequestHeader>() as u32,
                status as u64,
            );
            let descriptors = self.descriptor_page as *mut Descriptor;
            for (offset, descriptor) in chain.into_iter().enumerate() {
                ptr::write_volatile(descriptors.add(usize::from(head) + offset), descriptor);
            }
        }
    }

    fn publish(&mut self, heads: &[u16]) -> u16 {
        if heads.is_empty() || heads.len() > REQUEST_SLOT_COUNT {
            self.fail("virtio publish batch size is invalid");
        }
        // SAFETY: each head names a prepared, disjoint descriptor chain and
        // available ring slots are owned by the driver until index publication.
        unsafe {
            for (offset, head) in heads.iter().enumerate() {
                let ring_index = self.available_index.wrapping_add(offset as u16);
                let ring_slot = usize::from(ring_index % self.queue_size);
                ptr::write_volatile(
                    (self.available_page + 4 + ring_slot * size_of::<u16>()) as *mut u16,
                    *head,
                );
            }
            let expected_used_index = self.available_index.wrapping_add(heads.len() as u16);
            fence(Ordering::Release);
            ptr::write_volatile((self.available_page + 2) as *mut u16, expected_used_index);
            self.available_index = expected_used_index;
            fence(Ordering::SeqCst);
            ptr::write_volatile(self.notify_address as *mut u16, 0);
            expected_used_index
        }
    }

    fn validate_status(&self, slot_index: usize) {
        let status = self.request_slots[slot_index].control_page + size_of::<BlockRequestHeader>();
        // SAFETY: Completion observed the used index covering this slot, so
        // the device's status-byte write is visible.
        if unsafe { ptr::read_volatile(status as *const u8) } != BLOCK_STATUS_OK {
            self.fail("virtio block request returned an error");
        }
    }

    pub(crate) const fn queue_size(&self) -> u16 {
        self.queue_size
    }

    pub(crate) const fn capacity_sectors(&self) -> u64 {
        self.capacity_sectors
    }

    pub(crate) const fn flush_supported(&self) -> bool {
        self.flush_supported
    }

    pub(crate) const fn request_count(&self) -> u16 {
        self.available_index
    }

    pub(crate) const fn max_in_flight(&self) -> usize {
        REQUEST_SLOT_COUNT
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
