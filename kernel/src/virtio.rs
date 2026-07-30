// SPDX-License-Identifier: 0BSD

use core::arch::asm;
use core::future::{Future, pending};
use core::mem::size_of;
use core::pin::Pin;
use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering, fence};
use core::task::{Context, Poll};
use slopos_ext4::{
    DIRECTORY_ENTRY_DIRECTORY, DIRECTORY_ENTRY_REGULAR_FILE, DirectoryBlock, Extent,
    GroupDescriptor, Inode, ROOT_INODE, SUPERBLOCK_SIZE, Superblock,
};
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
const SUPERBLOCK_SECTOR: u64 = 2;
const QUEUE_LIMIT: u16 = 8;
const SECTOR_SIZE: usize = 512;
const DATA_PAGE_SIZE: usize = 4096;
const EXPECTED_RELEASE: &[u8] = include_bytes!("../../rootfs/etc/slopos-release");
const EXPECTED_SYSTEM_CONFIGURATION: &[u8] = include_bytes!("../../rootfs/etc/slopos/system.conf");
const RELEASE_PATH: [&[u8]; 2] = [b"etc", b"slopos-release"];
const CONFIGURATION_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"system.conf"];

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

pub async fn completion_task(mut device: BlockDevice) -> ! {
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: modern block queue ready size={} capacity_sectors={}",
        device.queue_size, device.capacity_sectors
    ));
    device.read(SUPERBLOCK_SECTOR, SUPERBLOCK_SIZE).await;
    let superblock = Superblock::parse(device.data(SUPERBLOCK_SIZE))
        .unwrap_or_else(|_| fail(device.common_base, "ext4 superblock validation failed"));
    let volume_name = core::str::from_utf8(superblock.volume_name())
        .unwrap_or_else(|_| fail(device.common_base, "ext4 volume label is not UTF-8"));
    if superblock.block_size as usize != DATA_PAGE_SIZE {
        fail(
            device.common_base,
            "ext4 probe requires a 4096-byte block size",
        );
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: superblock valid label={volume_name} block_size={} blocks={} inodes={} features={:#x}/{:#x}/{:#x}",
        superblock.block_size,
        superblock.block_count,
        superblock.inode_count,
        superblock.feature_compat,
        superblock.feature_incompat,
        superblock.feature_read_only_compat
    ));

    let descriptor_block = superblock.group_descriptor_block();
    let descriptor_sector = block_to_sector(&device, &superblock, descriptor_block);
    device.read(descriptor_sector, DATA_PAGE_SIZE).await;
    let group =
        GroupDescriptor::parse(device.data(DATA_PAGE_SIZE), 0, &superblock).unwrap_or_else(|_| {
            fail(
                device.common_base,
                "ext4 group descriptor validation failed",
            )
        });

    let root_inode = read_inode(&mut device, &superblock, &group, ROOT_INODE).await;
    let root_extent = directory_extent(&device, &superblock, &root_inode);

    let directory_sector = block_to_sector(&device, &superblock, root_extent.physical_block);
    device.read(directory_sector, DATA_PAGE_SIZE).await;
    let directory = DirectoryBlock::parse(device.data(DATA_PAGE_SIZE), &root_inode, &superblock)
        .unwrap_or_else(|_| fail(device.common_base, "ext4 root directory validation failed"));
    let etc = directory
        .find(b"etc")
        .unwrap_or_else(|| fail(device.common_base, "ext4 root directory is missing etc"));
    let lost_and_found = directory.find(b"lost+found").unwrap_or_else(|| {
        fail(
            device.common_base,
            "ext4 root directory is missing lost+found",
        )
    });
    if etc.file_type != DIRECTORY_ENTRY_DIRECTORY
        || lost_and_found.file_type != DIRECTORY_ENTRY_DIRECTORY
    {
        fail(device.common_base, "ext4 root entry type is invalid");
    }
    let root_entry_count = directory.entry_count();
    let etc_inode_number = etc.inode;
    let lost_and_found_inode_number = lost_and_found.inode;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: root directory valid group_inode_table={} inode={} extent_block={} entries={} etc_inode={} lost_found_inode={} metadata_checksums=group/inode/directory",
        group.inode_table_block,
        ROOT_INODE,
        root_extent.physical_block,
        root_entry_count,
        etc_inode_number,
        lost_and_found_inode_number
    ));

    let release_inode = resolve_path(&mut device, &superblock, &group, &RELEASE_PATH).await;
    let release_inode_number = release_inode.number;
    let release_size = {
        let bytes = read_small_file(&mut device, &superblock, &release_inode).await;
        if bytes != EXPECTED_RELEASE {
            fail(device.common_base, "ext4 slopos-release content mismatch");
        }
        bytes.len()
    };

    let configuration_inode =
        resolve_path(&mut device, &superblock, &group, &CONFIGURATION_PATH).await;
    let configuration_inode_number = configuration_inode.number;
    let configuration_size = {
        let bytes = read_small_file(&mut device, &superblock, &configuration_inode).await;
        if bytes != EXPECTED_SYSTEM_CONFIGURATION {
            fail(device.common_base, "ext4 system.conf content mismatch");
        }
        bytes.len()
    };
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: async path read valid release_inode={release_inode_number} release_bytes={release_size} config_inode={configuration_inode_number} config_bytes={configuration_size} paths=/etc/slopos-release,/etc/slopos/system.conf"
    ));
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: async block sequence complete requests={} interrupts={} queue_interrupts={}",
        device.available_index,
        INTERRUPT_COUNT.load(Ordering::Acquire),
        QUEUE_INTERRUPT_COUNT.load(Ordering::Acquire)
    ));
    pending::<()>().await;
    unreachable!()
}

async fn read_inode(
    device: &mut BlockDevice,
    superblock: &Superblock,
    group: &GroupDescriptor,
    inode_number: u32,
) -> Inode {
    let location = superblock
        .inode_location(inode_number, group)
        .unwrap_or_else(|_| fail(device.common_base, "ext4 inode location is invalid"));
    let sector = block_to_sector(device, superblock, location.block);
    device.read(sector, DATA_PAGE_SIZE).await;
    let offset = location.offset as usize;
    let end = offset + usize::from(superblock.inode_size);
    if end > DATA_PAGE_SIZE {
        fail(device.common_base, "ext4 inode crosses the read block");
    }
    Inode::parse(
        &device.data(DATA_PAGE_SIZE)[offset..end],
        inode_number,
        superblock,
    )
    .unwrap_or_else(|_| fail(device.common_base, "ext4 inode validation failed"))
}

async fn resolve_path(
    device: &mut BlockDevice,
    superblock: &Superblock,
    group: &GroupDescriptor,
    components: &[&[u8]],
) -> Inode {
    if components.is_empty() {
        fail(device.common_base, "ext4 path has no components");
    }
    let mut current = read_inode(device, superblock, group, ROOT_INODE).await;
    for component in components {
        if component.is_empty()
            || component.len() > 255
            || *component == b"."
            || *component == b".."
            || component.iter().any(|byte| *byte == 0 || *byte == b'/')
        {
            fail(device.common_base, "ext4 path component is invalid");
        }
        let extent = directory_extent(device, superblock, &current);
        let sector = block_to_sector(device, superblock, extent.physical_block);
        device.read(sector, DATA_PAGE_SIZE).await;
        let directory = DirectoryBlock::parse(device.data(DATA_PAGE_SIZE), &current, superblock)
            .unwrap_or_else(|_| fail(device.common_base, "ext4 path directory is invalid"));
        let entry = directory
            .find(component)
            .unwrap_or_else(|| fail(device.common_base, "ext4 path component was not found"));
        let inode_number = entry.inode;
        let file_type = entry.file_type;
        current = read_inode(device, superblock, group, inode_number).await;
        if (file_type == DIRECTORY_ENTRY_DIRECTORY && !current.is_directory())
            || (file_type == DIRECTORY_ENTRY_REGULAR_FILE && !current.is_regular_file())
            || (file_type != DIRECTORY_ENTRY_DIRECTORY && file_type != DIRECTORY_ENTRY_REGULAR_FILE)
        {
            fail(device.common_base, "ext4 directory entry type mismatch");
        }
    }
    current
}

fn directory_extent(device: &BlockDevice, superblock: &Superblock, inode: &Inode) -> Extent {
    let extent = inode
        .first_extent()
        .unwrap_or_else(|_| fail(device.common_base, "ext4 directory extent is unsupported"));
    if !inode.is_directory()
        || inode.size != u64::from(superblock.block_size)
        || extent.logical_block != 0
        || extent.unwritten
        || extent.physical_block >= superblock.block_count
    {
        fail(device.common_base, "ext4 directory extent is invalid");
    }
    extent
}

async fn read_small_file<'a>(
    device: &'a mut BlockDevice,
    superblock: &Superblock,
    inode: &Inode,
) -> &'a [u8] {
    let extent = inode
        .first_extent()
        .unwrap_or_else(|_| fail(device.common_base, "ext4 file extent is unsupported"));
    if !inode.is_regular_file()
        || inode.size > u64::from(superblock.block_size)
        || extent.logical_block != 0
        || extent.unwritten
        || extent.physical_block >= superblock.block_count
    {
        fail(device.common_base, "ext4 regular file extent is invalid");
    }
    let sector = block_to_sector(device, superblock, extent.physical_block);
    device.read(sector, DATA_PAGE_SIZE).await;
    &device.data(DATA_PAGE_SIZE)[..inode.size as usize]
}

impl BlockDevice {
    async fn read(&mut self, sector: u64, byte_count: usize) {
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

    fn data(&self, byte_count: usize) -> &[u8] {
        // SAFETY: data_page is a permanently live DMA frame and callers only
        // request the completed prefix, bounded by DATA_PAGE_SIZE.
        unsafe { core::slice::from_raw_parts(self.data_page as *const u8, byte_count) }
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

fn block_to_sector(device: &BlockDevice, superblock: &Superblock, block: u64) -> u64 {
    let byte_offset = block
        .checked_mul(u64::from(superblock.block_size))
        .unwrap_or_else(|| fail(device.common_base, "ext4 block offset overflow"));
    if byte_offset % SECTOR_SIZE as u64 != 0 {
        fail(device.common_base, "ext4 block is not sector aligned");
    }
    byte_offset / SECTOR_SIZE as u64
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
