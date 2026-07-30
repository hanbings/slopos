// SPDX-License-Identifier: 0BSD

use core::future::pending;
use slopos_ext4::{
    DIRECTORY_ENTRY_DIRECTORY, DIRECTORY_ENTRY_REGULAR_FILE, DirectoryBlock, Extent,
    GroupDescriptor, Inode, ROOT_INODE, SUPERBLOCK_SIZE, Superblock, validate_path_component,
};

use crate::virtio::BlockDevice;

const SUPERBLOCK_SECTOR: u64 = 2;
const SECTOR_SIZE: u64 = 512;
const BLOCK_SIZE: usize = 4096;
const EXPECTED_RELEASE: &[u8] = include_bytes!("../../rootfs/etc/slopos-release");
const EXPECTED_SYSTEM_CONFIGURATION: &[u8] = include_bytes!("../../rootfs/etc/slopos/system.conf");
const RELEASE_PATH: [&[u8]; 2] = [b"etc", b"slopos-release"];
const CONFIGURATION_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"system.conf"];
const MULTIBLOCK_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"multiblock.bin"];

pub async fn mount_task(mut device: BlockDevice) -> ! {
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: modern block queue ready size={} capacity_sectors={}",
        device.queue_size(),
        device.capacity_sectors()
    ));
    let mount = ReadOnlyMount::mount(&mut device).await;
    let superblock = &mount.superblock;
    let volume_name = core::str::from_utf8(superblock.volume_name())
        .unwrap_or_else(|_| device.fail("ext4 volume label is not UTF-8"));
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: superblock valid label={volume_name} block_size={} blocks={} inodes={} groups={} features={:#x}/{:#x}/{:#x}",
        superblock.block_size,
        superblock.block_count,
        superblock.inode_count,
        superblock.group_count(),
        superblock.feature_compat,
        superblock.feature_incompat,
        superblock.feature_read_only_compat
    ));

    let root = mount.probe_root(&mut device).await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: root directory valid group_inode_table={} inode={} extent_block={} entries={} etc_inode={} lost_found_inode={} metadata_checksums=group/inode/directory",
        mount.group0.inode_table_block,
        ROOT_INODE,
        root.extent_block,
        root.entry_count,
        root.etc_inode,
        root.lost_and_found_inode
    ));

    let release = mount.open_file(&mut device, &RELEASE_PATH).await;
    let release_size = {
        let bytes = mount.read_file_block(&mut device, &release, 0).await;
        if bytes != EXPECTED_RELEASE {
            device.fail("ext4 slopos-release content mismatch");
        }
        bytes.len()
    };

    let configuration = mount.open_file(&mut device, &CONFIGURATION_PATH).await;
    let configuration_size = {
        let bytes = mount.read_file_block(&mut device, &configuration, 0).await;
        if bytes != EXPECTED_SYSTEM_CONFIGURATION {
            device.fail("ext4 system.conf content mismatch");
        }
        bytes.len()
    };
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: async path read valid release_inode={} release_bytes={release_size} config_inode={} config_bytes={configuration_size} paths=/etc/slopos-release,/etc/slopos/system.conf",
        release.inode.number, configuration.inode.number
    ));

    let multiblock = mount.open_file(&mut device, &MULTIBLOCK_PATH).await;
    if multiblock.inode.size != 6144 {
        device.fail("ext4 multiblock file size mismatch");
    }
    let mut multiblock_bytes = 0usize;
    for logical_block in 0..2 {
        let bytes = mount
            .read_file_block(&mut device, &multiblock, logical_block)
            .await;
        if bytes.iter().any(|byte| *byte != b'Z') {
            device.fail("ext4 multiblock file content mismatch");
        }
        multiblock_bytes += bytes.len();
    }
    let multiblock_group = mount
        .superblock
        .inode_group(multiblock.inode.number)
        .unwrap_or_else(|_| device.fail("ext4 multiblock inode group is invalid"));
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: multiblock file valid inode={} inode_group={multiblock_group} bytes={multiblock_bytes} logical_blocks=2 path=/usr/share/slopos/multiblock.bin",
        multiblock.inode.number,
    ));
    let (interrupts, queue_interrupts) = crate::virtio::interrupt_counts();
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: async block sequence complete requests={} interrupts={interrupts} queue_interrupts={queue_interrupts}",
        device.request_count()
    ));
    pending::<()>().await;
    unreachable!()
}

struct ReadOnlyMount {
    superblock: Superblock,
    group0: GroupDescriptor,
}

impl ReadOnlyMount {
    async fn mount(device: &mut BlockDevice) -> Self {
        device.read(SUPERBLOCK_SECTOR, SUPERBLOCK_SIZE).await;
        let superblock = Superblock::parse(device.data(SUPERBLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("ext4 superblock validation failed"));
        if superblock.block_size as usize != BLOCK_SIZE {
            device.fail("ext4 mount requires a 4096-byte block size");
        }
        let group0 = read_group_descriptor(device, &superblock, 0).await;
        Self { superblock, group0 }
    }

    async fn probe_root(&self, device: &mut BlockDevice) -> RootProbe {
        let inode = self.read_inode(device, ROOT_INODE).await;
        let extent = self.directory_extent(device, &inode);
        let sector = block_to_sector(device, &self.superblock, extent.physical_block);
        device.read(sector, BLOCK_SIZE).await;
        let directory = DirectoryBlock::parse(device.data(BLOCK_SIZE), &inode, &self.superblock)
            .unwrap_or_else(|_| device.fail("ext4 root directory validation failed"));
        let etc = directory
            .find(b"etc")
            .unwrap_or_else(|| device.fail("ext4 root directory is missing etc"));
        let lost_and_found = directory
            .find(b"lost+found")
            .unwrap_or_else(|| device.fail("ext4 root directory is missing lost+found"));
        if etc.file_type != DIRECTORY_ENTRY_DIRECTORY
            || lost_and_found.file_type != DIRECTORY_ENTRY_DIRECTORY
        {
            device.fail("ext4 root entry type is invalid");
        }
        RootProbe {
            extent_block: extent.physical_block,
            entry_count: directory.entry_count(),
            etc_inode: etc.inode,
            lost_and_found_inode: lost_and_found.inode,
        }
    }

    async fn open_file(&self, device: &mut BlockDevice, components: &[&[u8]]) -> ReadOnlyFile {
        let inode = self.resolve_path(device, components).await;
        if !inode.is_regular_file() {
            device.fail("ext4 open target is not a regular file");
        }
        ReadOnlyFile { inode }
    }

    async fn resolve_path(&self, device: &mut BlockDevice, components: &[&[u8]]) -> Inode {
        if components.is_empty() {
            device.fail("ext4 path has no components");
        }
        let mut current = self.read_inode(device, ROOT_INODE).await;
        for component in components {
            if validate_path_component(component).is_err() {
                device.fail("ext4 path component is invalid");
            }
            let extent = self.directory_extent(device, &current);
            let sector = block_to_sector(device, &self.superblock, extent.physical_block);
            device.read(sector, BLOCK_SIZE).await;
            let directory =
                DirectoryBlock::parse(device.data(BLOCK_SIZE), &current, &self.superblock)
                    .unwrap_or_else(|_| device.fail("ext4 path directory is invalid"));
            let entry = directory
                .find(component)
                .unwrap_or_else(|| device.fail("ext4 path component was not found"));
            let inode_number = entry.inode;
            let file_type = entry.file_type;
            current = self.read_inode(device, inode_number).await;
            if (file_type == DIRECTORY_ENTRY_DIRECTORY && !current.is_directory())
                || (file_type == DIRECTORY_ENTRY_REGULAR_FILE && !current.is_regular_file())
                || (file_type != DIRECTORY_ENTRY_DIRECTORY
                    && file_type != DIRECTORY_ENTRY_REGULAR_FILE)
            {
                device.fail("ext4 directory entry type mismatch");
            }
        }
        current
    }

    async fn read_inode(&self, device: &mut BlockDevice, inode_number: u32) -> Inode {
        let group_index = self
            .superblock
            .inode_group(inode_number)
            .unwrap_or_else(|_| device.fail("ext4 inode number is invalid"));
        let group = if group_index == 0 {
            self.group0
        } else {
            read_group_descriptor(device, &self.superblock, group_index).await
        };
        let location = self
            .superblock
            .inode_location(inode_number, &group)
            .unwrap_or_else(|_| device.fail("ext4 inode location is invalid"));
        let sector = block_to_sector(device, &self.superblock, location.block);
        device.read(sector, BLOCK_SIZE).await;
        let offset = location.offset as usize;
        let end = offset + usize::from(self.superblock.inode_size);
        if end > BLOCK_SIZE {
            device.fail("ext4 inode crosses the read block");
        }
        Inode::parse(
            &device.data(BLOCK_SIZE)[offset..end],
            inode_number,
            &self.superblock,
        )
        .unwrap_or_else(|_| device.fail("ext4 inode validation failed"))
    }

    async fn read_file_block<'a>(
        &self,
        device: &'a mut BlockDevice,
        file: &ReadOnlyFile,
        logical_block: u32,
    ) -> &'a [u8] {
        let file_block_count = file
            .inode
            .size
            .div_ceil(u64::from(self.superblock.block_size));
        if u64::from(logical_block) >= file_block_count {
            device.fail("ext4 file read is past end of file");
        }
        let extent = file
            .inode
            .extent_for_logical_block(logical_block)
            .unwrap_or_else(|_| device.fail("ext4 file extent is unsupported"))
            .unwrap_or_else(|| device.fail("sparse ext4 file reads are unsupported"));
        if extent.unwritten {
            device.fail("ext4 regular file extent is invalid");
        }
        let physical_block =
            extent.physical_block + u64::from(logical_block - extent.logical_block);
        if physical_block >= self.superblock.block_count {
            device.fail("ext4 regular file block is outside the filesystem");
        }
        let sector = block_to_sector(device, &self.superblock, physical_block);
        device.read(sector, BLOCK_SIZE).await;
        let byte_offset = u64::from(logical_block) * u64::from(self.superblock.block_size);
        let remaining = file.inode.size - byte_offset;
        let length = remaining.min(u64::from(self.superblock.block_size)) as usize;
        &device.data(BLOCK_SIZE)[..length]
    }

    fn directory_extent(&self, device: &BlockDevice, inode: &Inode) -> Extent {
        let extent = inode
            .first_extent()
            .unwrap_or_else(|_| device.fail("ext4 directory extent is unsupported"));
        if !inode.is_directory()
            || inode.size != u64::from(self.superblock.block_size)
            || extent.logical_block != 0
            || extent.unwritten
            || extent.physical_block >= self.superblock.block_count
        {
            device.fail("ext4 directory extent is invalid");
        }
        extent
    }
}

struct ReadOnlyFile {
    inode: Inode,
}

struct RootProbe {
    extent_block: u64,
    entry_count: usize,
    etc_inode: u32,
    lost_and_found_inode: u32,
}

async fn read_group_descriptor(
    device: &mut BlockDevice,
    superblock: &Superblock,
    group_index: u32,
) -> GroupDescriptor {
    let location = superblock
        .group_descriptor_location(group_index)
        .unwrap_or_else(|_| device.fail("ext4 group descriptor location is invalid"));
    let sector = block_to_sector(device, superblock, location.block);
    device.read(sector, BLOCK_SIZE).await;
    let offset = location.offset as usize;
    let end = offset + usize::from(superblock.descriptor_size);
    if end > BLOCK_SIZE {
        device.fail("ext4 group descriptor crosses the read block");
    }
    let descriptor = GroupDescriptor::parse(
        &device.data(BLOCK_SIZE)[offset..end],
        group_index,
        superblock,
    )
    .unwrap_or_else(|_| device.fail("ext4 group descriptor validation failed"));
    if group_index != 0 {
        crate::serial::serialln(format_args!(
            "SLOPOS-EXT4: group descriptor valid group={group_index} inode_table={}",
            descriptor.inode_table_block
        ));
    }
    descriptor
}

fn block_to_sector(device: &BlockDevice, superblock: &Superblock, block: u64) -> u64 {
    let byte_offset = block
        .checked_mul(u64::from(superblock.block_size))
        .unwrap_or_else(|| device.fail("ext4 block offset overflow"));
    if byte_offset % SECTOR_SIZE != 0 {
        device.fail("ext4 block is not sector aligned");
    }
    byte_offset / SECTOR_SIZE
}
