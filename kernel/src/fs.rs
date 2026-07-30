// SPDX-License-Identifier: 0BSD

use core::future::pending;
use core::ptr;
use slopos_ext4::{
    DIRECTORY_ENTRY_DIRECTORY, DIRECTORY_ENTRY_REGULAR_FILE, DIRECTORY_ENTRY_SYMLINK,
    DirectoryBlock, Extent, ExtentNode, GroupDescriptor, INODE_FLAG_DIRECTORY_INDEX, Inode,
    JOURNAL_INODE, JournalSuperblock, ROOT_INODE, SUPERBLOCK_SIZE, Superblock,
    validate_path_component,
};
use slopos_vfs::{AbsolutePath, AccessMode, FileDescriptorTable, FileNode, MountTable};

use crate::virtio::BlockDevice;

const SUPERBLOCK_SECTOR: u64 = 2;
const SECTOR_SIZE: u64 = 512;
const BLOCK_SIZE: usize = 4096;
const CACHE_ENTRY_COUNT: usize = 8;
static ZERO_BLOCK: [u8; BLOCK_SIZE] = [0; BLOCK_SIZE];
const EXPECTED_RELEASE: &[u8] = include_bytes!("../../rootfs/etc/slopos-release");
const EXPECTED_SYSTEM_CONFIGURATION: &[u8] = include_bytes!("../../rootfs/etc/slopos/system.conf");
const RELEASE_PATH: [&[u8]; 2] = [b"etc", b"slopos-release"];
const CONFIGURATION_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"system.conf"];
const MULTIBLOCK_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"multiblock.bin"];
const DEEP_EXTENT_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"deep-extent.bin"];
const CROSS_BLOCK_PATH: [&[u8]; 5] = [b"usr", b"share", b"slopos", b"large-directory", b"tail-29"];
const SYMLINK_PATH: [&[u8]; 2] = [b"etc", b"current-release"];
const WRITE_PROBE_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"write-probe.bin"];
const ROOT_FILESYSTEM_ID: u16 = 1;
const VFS_TEST_PATH: &[u8] = b"/etc/./slopos/../slopos/system.conf";

pub async fn mount_task(mut device: BlockDevice) -> ! {
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: modern block queue ready size={} capacity_sectors={} flush={}",
        device.queue_size(),
        device.capacity_sectors(),
        device.flush_supported()
    ));
    let mut mount = Ext4Mount::mount(&mut device).await;
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
    mount
        .prefetch_file_pair(&mut device, &multiblock, 0, 1)
        .await;
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

    let deep_extent = mount.open_file(&mut device, &DEEP_EXTENT_PATH).await;
    if deep_extent.inode.size != 36_864 || deep_extent.inode.extent_depth() != Ok(1) {
        device.fail("ext4 depth-one extent inode is invalid");
    }
    let leaf_block = deep_extent
        .inode
        .extent_index_for_logical_block(8)
        .unwrap_or_else(|_| device.fail("ext4 extent root index is invalid"))
        .unwrap_or_else(|| device.fail("ext4 extent root index is missing"))
        .child_block;
    let deep_bytes = mount.read_file_block(&mut device, &deep_extent, 8).await;
    if deep_bytes.len() != BLOCK_SIZE || deep_bytes.iter().any(|byte| *byte != b'D') {
        device.fail("ext4 depth-one extent file content mismatch");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: depth-one extent valid inode={} leaf_block={leaf_block} logical_block=8 bytes={} metadata_checksum=valid path=/usr/share/slopos/deep-extent.bin",
        deep_extent.inode.number,
        deep_bytes.len()
    ));
    let hole_bytes = mount.read_file_block(&mut device, &deep_extent, 7).await;
    if hole_bytes.len() != BLOCK_SIZE || hole_bytes.iter().any(|byte| *byte != 0) {
        device.fail("ext4 sparse hole did not read as zeros");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: sparse read valid inode={} logical_block=7 zero_bytes={}",
        deep_extent.inode.number,
        hole_bytes.len()
    ));

    let cross_block = mount.open_file(&mut device, &CROSS_BLOCK_PATH).await;
    if cross_block.directory_block != 1 {
        device.fail("ext4 cross-block directory entry was not in logical block one");
    }
    let cross_block_bytes = mount.read_file_block(&mut device, &cross_block, 0).await;
    if cross_block_bytes != EXPECTED_RELEASE {
        device.fail("ext4 cross-block directory file content mismatch");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: cross-block directory valid directory_inode={} directory_blocks=2 entry_block={} target_inode={} target_bytes={} metadata_checksums=valid path=/usr/share/slopos/large-directory/tail-29",
        cross_block.parent_inode,
        cross_block.directory_block,
        cross_block.inode.number,
        cross_block_bytes.len()
    ));

    let symlink = mount.open_file(&mut device, &SYMLINK_PATH).await;
    if symlink.followed_symlink == 0 {
        device.fail("ext4 fast symlink was not followed");
    }
    let symlink_bytes = mount.read_file_block(&mut device, &symlink, 0).await;
    if symlink_bytes != EXPECTED_RELEASE {
        device.fail("ext4 fast symlink target content mismatch");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: fast symlink valid link_inode={} target_inode={} target_bytes={} target=slopos-release path=/etc/current-release",
        symlink.followed_symlink,
        symlink.inode.number,
        symlink_bytes.len()
    ));

    let root_path =
        AbsolutePath::parse(b"/").unwrap_or_else(|_| device.fail("VFS root path is invalid"));
    let path = AbsolutePath::parse(VFS_TEST_PATH)
        .unwrap_or_else(|_| device.fail("VFS test path is invalid"));
    let mut namespace = MountTable::<4>::new();
    namespace
        .mount(&root_path, ROOT_FILESYSTEM_ID)
        .unwrap_or_else(|_| device.fail("VFS root mount failed"));
    let resolution = namespace
        .resolve(&path)
        .unwrap_or_else(|_| device.fail("VFS path had no mount"));
    if resolution.filesystem_id != ROOT_FILESYSTEM_ID || resolution.matched_components != 0 {
        device.fail("VFS root mount resolution mismatch");
    }
    let vfs_file = mount
        .open_file(
            &mut device,
            &path.components()[resolution.matched_components..],
        )
        .await;
    let mut descriptors = FileDescriptorTable::<8>::new();
    let fd = descriptors
        .open(FileNode {
            filesystem_id: resolution.filesystem_id,
            node_id: u64::from(vfs_file.inode.number),
            size: vfs_file.inode.size,
        })
        .unwrap_or_else(|_| device.fail("VFS file descriptor allocation failed"));
    let mut vfs_bytes = [0u8; EXPECTED_SYSTEM_CONFIGURATION.len()];
    let mut copied = 0;
    let mut chunk_reads = 0;
    while copied < vfs_bytes.len() {
        let end = (copied + 17).min(vfs_bytes.len());
        let read = read_descriptor(
            &mut mount,
            &mut device,
            &mut descriptors,
            fd,
            &vfs_file,
            &mut vfs_bytes[copied..end],
        )
        .await;
        if read != end - copied {
            device.fail("VFS descriptor returned a short read");
        }
        copied = end;
        chunk_reads += 1;
    }
    if vfs_bytes != EXPECTED_SYSTEM_CONFIGURATION {
        device.fail("VFS descriptor content mismatch");
    }
    descriptors
        .seek(fd, 7)
        .unwrap_or_else(|_| device.fail("VFS descriptor seek failed"));
    let mut seek_bytes = [0u8; 11];
    if read_descriptor(
        &mut mount,
        &mut device,
        &mut descriptors,
        fd,
        &vfs_file,
        &mut seek_bytes,
    )
    .await
        != seek_bytes.len()
        || seek_bytes != EXPECTED_SYSTEM_CONFIGURATION[7..18]
    {
        device.fail("VFS seek/read result mismatch");
    }
    descriptors
        .close(fd)
        .unwrap_or_else(|_| device.fail("VFS descriptor close failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: namespace valid mounts={} root_fs={} fd={fd} inode={} bytes={} chunk_reads={chunk_reads} seek_offset=7 seek_bytes={}",
        namespace.len(),
        resolution.filesystem_id,
        vfs_file.inode.number,
        vfs_bytes.len(),
        seek_bytes.len()
    ));

    let write_probe = mount.open_file(&mut device, &WRITE_PROBE_PATH).await;
    if write_probe.inode.size != u64::from(mount.superblock.block_size) {
        device.fail("ext4 write probe size mismatch");
    }
    let original = mount.read_file_block(&mut device, &write_probe, 0).await;
    if original.len() != BLOCK_SIZE || original.iter().any(|byte| *byte != b'P') {
        device.fail("ext4 write probe initial content mismatch");
    }
    let physical_block = mount
        .inode_physical_block(&mut device, &write_probe.inode, 0)
        .await
        .unwrap_or_else(|| device.fail("ext4 write probe is sparse"));
    let write_fd = descriptors
        .open_with_mode(
            FileNode {
                filesystem_id: ROOT_FILESYSTEM_ID,
                node_id: u64::from(write_probe.inode.number),
                size: write_probe.inode.size,
            },
            AccessMode::ReadWrite,
        )
        .unwrap_or_else(|_| device.fail("VFS writable descriptor allocation failed"));
    if write_fd != fd {
        device.fail("VFS closed descriptor was not reused");
    }
    descriptors
        .seek(write_fd, 123)
        .unwrap_or_else(|_| device.fail("VFS writable descriptor seek failed"));
    let write_bytes = [0xa5; 73];
    if write_descriptor(
        &mut mount,
        &mut device,
        &mut descriptors,
        write_fd,
        &write_probe,
        &write_bytes,
    )
    .await
        != write_bytes.len()
    {
        device.fail("VFS writable descriptor returned a short write");
    }
    descriptors
        .seek(write_fd, 100)
        .unwrap_or_else(|_| device.fail("VFS write verification seek failed"));
    let mut persisted = [0u8; 119];
    if read_descriptor(
        &mut mount,
        &mut device,
        &mut descriptors,
        write_fd,
        &write_probe,
        &mut persisted,
    )
    .await
        != persisted.len()
        || persisted[..23].iter().any(|byte| *byte != b'P')
        || persisted[23..96].iter().any(|byte| *byte != 0xa5)
        || persisted[96..].iter().any(|byte| *byte != b'P')
    {
        device.fail("VFS partial write did not persist after flush");
    }
    descriptors
        .seek(write_fd, 123)
        .unwrap_or_else(|_| device.fail("VFS restore seek failed"));
    let restore_bytes = [b'P'; 73];
    if write_descriptor(
        &mut mount,
        &mut device,
        &mut descriptors,
        write_fd,
        &write_probe,
        &restore_bytes,
    )
    .await
        != restore_bytes.len()
    {
        device.fail("VFS writable descriptor returned a short restore");
    }
    descriptors
        .seek(write_fd, 100)
        .unwrap_or_else(|_| device.fail("VFS restore verification seek failed"));
    let mut restored = [0u8; 119];
    if read_descriptor(
        &mut mount,
        &mut device,
        &mut descriptors,
        write_fd,
        &write_probe,
        &mut restored,
    )
    .await
        != restored.len()
        || restored.iter().any(|byte| *byte != b'P')
    {
        device.fail("VFS partial write restoration failed");
    }
    if mount.cache.invalidations != 2 {
        device.fail("ext4 write probe cache invalidation count mismatch");
    }
    descriptors
        .close(write_fd)
        .unwrap_or_else(|_| device.fail("VFS writable descriptor close failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: writable descriptor valid fd={write_fd} inode={} physical_block={physical_block} offset=123 bytes={} writes=2 flushes=2 cache_invalidations={} restored=true path=/usr/share/slopos/write-probe.bin",
        write_probe.inode.number,
        write_bytes.len(),
        mount.cache.invalidations
    ));

    let journal = mount.probe_journal(&mut device).await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: journal superblock valid inode={JOURNAL_INODE} physical_block={} blocks={} first={} sequence={} start={} users={} features={:#x}/{:#x}/{:#x} uuid=match endian=big",
        journal.physical_block,
        journal.superblock.max_length,
        journal.superblock.first_log_block,
        journal.superblock.sequence,
        journal.superblock.start,
        journal.superblock.user_count,
        journal.superblock.feature_compat,
        journal.superblock.feature_incompat,
        journal.superblock.feature_read_only_compat
    ));

    crate::serial::serialln(format_args!(
        "SLOPOS-FS: block cache entries={CACHE_ENTRY_COUNT} hits={} misses={} batched_pairs={} invalidations={}",
        mount.cache.hits, mount.cache.misses, mount.cache.batched_pairs, mount.cache.invalidations
    ));
    let (interrupts, queue_interrupts) = crate::virtio::interrupt_counts();
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: bounded block sequence complete requests={} max_in_flight={} interrupts={interrupts} queue_interrupts={queue_interrupts}",
        device.request_count(),
        device.max_in_flight()
    ));
    pending::<()>().await;
    unreachable!()
}

struct Ext4Mount {
    superblock: Superblock,
    group0: GroupDescriptor,
    cache: BlockCache,
}

async fn read_descriptor(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    descriptors: &mut FileDescriptorTable<8>,
    fd: u32,
    file: &Ext4File,
    output: &mut [u8],
) -> usize {
    let window = descriptors
        .read_window(fd, output.len())
        .unwrap_or_else(|_| device.fail("VFS descriptor read window failed"));
    if window.node.filesystem_id != ROOT_FILESYSTEM_ID
        || window.node.node_id != u64::from(file.inode.number)
    {
        device.fail("VFS descriptor vnode mismatch");
    }
    let mut copied = 0usize;
    while copied < window.length {
        let copied_offset = u64::try_from(copied)
            .unwrap_or_else(|_| device.fail("VFS read offset conversion failed"));
        let absolute_offset = window
            .offset
            .checked_add(copied_offset)
            .unwrap_or_else(|| device.fail("VFS read offset overflow"));
        let block_size = u64::try_from(BLOCK_SIZE)
            .unwrap_or_else(|_| device.fail("VFS block size conversion failed"));
        let logical_block = u32::try_from(absolute_offset / block_size)
            .unwrap_or_else(|_| device.fail("VFS file block index overflow"));
        let block_offset = usize::try_from(absolute_offset % block_size)
            .unwrap_or_else(|_| device.fail("VFS block offset conversion failed"));
        let block = mount.read_file_block(device, file, logical_block).await;
        let length = (window.length - copied).min(block.len() - block_offset);
        output[copied..copied + length]
            .copy_from_slice(&block[block_offset..block_offset + length]);
        copied += length;
    }
    descriptors
        .advance(fd, copied)
        .unwrap_or_else(|_| device.fail("VFS descriptor offset advance failed"));
    copied
}

async fn write_descriptor(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    descriptors: &mut FileDescriptorTable<8>,
    fd: u32,
    file: &Ext4File,
    input: &[u8],
) -> usize {
    let window = descriptors
        .write_window(fd, input.len())
        .unwrap_or_else(|_| device.fail("VFS descriptor write window failed"));
    if window.node.filesystem_id != ROOT_FILESYSTEM_ID
        || window.node.node_id != u64::from(file.inode.number)
    {
        device.fail("VFS writable descriptor vnode mismatch");
    }
    let block_size = u64::try_from(BLOCK_SIZE)
        .unwrap_or_else(|_| device.fail("VFS block size conversion failed"));
    let mut block_buffer = [0u8; BLOCK_SIZE];
    let mut copied = 0usize;
    while copied < window.length {
        let copied_offset = u64::try_from(copied)
            .unwrap_or_else(|_| device.fail("VFS write offset conversion failed"));
        let absolute_offset = window
            .offset
            .checked_add(copied_offset)
            .unwrap_or_else(|| device.fail("VFS write offset overflow"));
        let logical_block = u32::try_from(absolute_offset / block_size)
            .unwrap_or_else(|_| device.fail("VFS writable file block index overflow"));
        let block_offset = usize::try_from(absolute_offset % block_size)
            .unwrap_or_else(|_| device.fail("VFS writable block offset conversion failed"));
        let block = mount.read_file_block(device, file, logical_block).await;
        if block.len() != BLOCK_SIZE {
            device.fail("VFS partial-block EOF writes are unsupported");
        }
        block_buffer.copy_from_slice(block);
        let length = (window.length - copied).min(BLOCK_SIZE - block_offset);
        block_buffer[block_offset..block_offset + length]
            .copy_from_slice(&input[copied..copied + length]);
        mount
            .overwrite_existing_file_block(device, file, logical_block, &block_buffer)
            .await;
        copied += length;
    }
    descriptors
        .advance(fd, copied)
        .unwrap_or_else(|_| device.fail("VFS writable descriptor offset advance failed"));
    copied
}

impl Ext4Mount {
    async fn mount(device: &mut BlockDevice) -> Self {
        device.read(SUPERBLOCK_SECTOR, SUPERBLOCK_SIZE).await;
        let superblock = Superblock::parse(device.data(SUPERBLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("ext4 superblock validation failed"));
        if superblock.block_size as usize != BLOCK_SIZE {
            device.fail("ext4 mount requires a 4096-byte block size");
        }
        let mut cache = BlockCache::new();
        let group0 = read_group_descriptor(device, &superblock, 0, &mut cache).await;
        Self {
            superblock,
            group0,
            cache,
        }
    }

    async fn probe_root(&mut self, device: &mut BlockDevice) -> RootProbe {
        let inode = self.read_inode(device, ROOT_INODE).await;
        let extent = self.directory_extent(device, &inode);
        let block = self
            .cache
            .read_block(device, &self.superblock, extent.physical_block)
            .await;
        let directory = DirectoryBlock::parse(block, &inode, &self.superblock)
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

    async fn probe_journal(&mut self, device: &mut BlockDevice) -> JournalProbe {
        if self.superblock.journal_inode != JOURNAL_INODE {
            device.fail("ext4 internal journal inode is unsupported");
        }
        let inode = self.read_inode(device, JOURNAL_INODE).await;
        if !inode.is_regular_file() || inode.extent_depth() != Ok(0) {
            device.fail("ext4 journal inode layout is unsupported");
        }
        let extent = inode
            .extent_for_logical_block(0)
            .unwrap_or_else(|_| device.fail("ext4 journal extent is invalid"))
            .unwrap_or_else(|| device.fail("ext4 journal extent is missing"));
        let expected_blocks = inode.size / u64::from(self.superblock.block_size);
        if inode.size % u64::from(self.superblock.block_size) != 0
            || extent.logical_block != 0
            || extent.unwritten
            || u64::from(extent.block_count) != expected_blocks
        {
            device.fail("ext4 journal extent geometry is unsupported");
        }
        let physical_block = extent.physical_block;
        let block = self
            .cache
            .read_block(device, &self.superblock, physical_block)
            .await;
        let superblock = JournalSuperblock::parse(block)
            .unwrap_or_else(|_| device.fail("JBD2 superblock validation failed"));
        if superblock.block_size != self.superblock.block_size
            || u64::from(superblock.max_length) != expected_blocks
            || superblock.first_log_block != 1
            || superblock.sequence == 0
            || superblock.start != 0
            || superblock.error != 0
            || superblock.user_count != 1
            || superblock.uuid != self.superblock.uuid
        {
            device.fail("JBD2 journal geometry or identity mismatch");
        }
        JournalProbe {
            physical_block,
            superblock,
        }
    }

    async fn open_file(&mut self, device: &mut BlockDevice, components: &[&[u8]]) -> Ext4File {
        let (inode, parent_inode, directory_block, followed_symlink) =
            self.resolve_path(device, components).await;
        if !inode.is_regular_file() {
            device.fail("ext4 open target is not a regular file");
        }
        Ext4File {
            inode,
            parent_inode,
            directory_block,
            followed_symlink,
        }
    }

    async fn resolve_path(
        &mut self,
        device: &mut BlockDevice,
        components: &[&[u8]],
    ) -> (Inode, u32, u32, u32) {
        if components.is_empty() {
            device.fail("ext4 path has no components");
        }
        let mut current = self.read_inode(device, ROOT_INODE).await;
        let mut parent_inode = ROOT_INODE;
        let mut directory_block = 0;
        let mut followed_symlink = 0;
        for (component_index, component) in components.iter().enumerate() {
            if validate_path_component(component).is_err() {
                device.fail("ext4 path component is invalid");
            }
            let parent = current;
            parent_inode = current.number;
            let (inode_number, file_type, entry_block) = self
                .find_directory_entry(device, &current, component)
                .await
                .unwrap_or_else(|| device.fail("ext4 path component was not found"));
            directory_block = entry_block;
            current = self.read_inode(device, inode_number).await;
            if file_type == DIRECTORY_ENTRY_SYMLINK {
                if component_index + 1 != components.len() || !current.is_symlink() {
                    device.fail("ext4 symbolic link position or type is unsupported");
                }
                let target = current
                    .inline_symlink()
                    .unwrap_or_else(|_| device.fail("ext4 symbolic link target is unsupported"));
                if validate_path_component(target).is_err() {
                    device.fail("ext4 symbolic link target is not one relative component");
                }
                let mut target_buffer = [0u8; 60];
                target_buffer[..target.len()].copy_from_slice(target);
                let target_length = target.len();
                followed_symlink = current.number;
                let (target_inode, target_type, target_block) = self
                    .find_directory_entry(device, &parent, &target_buffer[..target_length])
                    .await
                    .unwrap_or_else(|| device.fail("ext4 symbolic link target was not found"));
                if target_type != DIRECTORY_ENTRY_REGULAR_FILE {
                    device.fail("ext4 symbolic link target is not a regular file");
                }
                directory_block = target_block;
                current = self.read_inode(device, target_inode).await;
                if !current.is_regular_file() {
                    device.fail("ext4 symbolic link target inode type mismatch");
                }
                continue;
            }
            if (file_type == DIRECTORY_ENTRY_DIRECTORY && !current.is_directory())
                || (file_type == DIRECTORY_ENTRY_REGULAR_FILE && !current.is_regular_file())
                || (file_type != DIRECTORY_ENTRY_DIRECTORY
                    && file_type != DIRECTORY_ENTRY_REGULAR_FILE)
            {
                device.fail("ext4 directory entry type mismatch");
            }
        }
        (current, parent_inode, directory_block, followed_symlink)
    }

    async fn find_directory_entry(
        &mut self,
        device: &mut BlockDevice,
        inode: &Inode,
        component: &[u8],
    ) -> Option<(u32, u8, u32)> {
        if !inode.is_directory() {
            device.fail("ext4 path component parent is not a directory");
        }
        if inode.flags & INODE_FLAG_DIRECTORY_INDEX != 0 {
            device.fail("indexed ext4 directories are unsupported");
        }
        let block_count = inode.size.div_ceil(u64::from(self.superblock.block_size));
        if block_count == 0 || block_count > u64::from(u32::MAX) {
            device.fail("ext4 directory size is invalid");
        }
        for logical_block in 0..block_count as u32 {
            let physical_block = self
                .inode_physical_block(device, inode, logical_block)
                .await
                .unwrap_or_else(|| device.fail("ext4 directory contains a sparse block"));
            let block = self
                .cache
                .read_block(device, &self.superblock, physical_block)
                .await;
            let directory = DirectoryBlock::parse(block, inode, &self.superblock)
                .unwrap_or_else(|_| device.fail("ext4 path directory block is invalid"));
            if let Some(entry) = directory.find(component) {
                return Some((entry.inode, entry.file_type, logical_block));
            }
        }
        None
    }

    async fn read_inode(&mut self, device: &mut BlockDevice, inode_number: u32) -> Inode {
        let group_index = self
            .superblock
            .inode_group(inode_number)
            .unwrap_or_else(|_| device.fail("ext4 inode number is invalid"));
        let group = if group_index == 0 {
            self.group0
        } else {
            read_group_descriptor(device, &self.superblock, group_index, &mut self.cache).await
        };
        let location = self
            .superblock
            .inode_location(inode_number, &group)
            .unwrap_or_else(|_| device.fail("ext4 inode location is invalid"));
        let block = self
            .cache
            .read_block(device, &self.superblock, location.block)
            .await;
        let offset = location.offset as usize;
        let end = offset + usize::from(self.superblock.inode_size);
        if end > BLOCK_SIZE {
            device.fail("ext4 inode crosses the read block");
        }
        Inode::parse(&block[offset..end], inode_number, &self.superblock)
            .unwrap_or_else(|_| device.fail("ext4 inode validation failed"))
    }

    async fn read_file_block<'a>(
        &'a mut self,
        device: &mut BlockDevice,
        file: &Ext4File,
        logical_block: u32,
    ) -> &'a [u8] {
        let byte_offset = u64::from(logical_block) * u64::from(self.superblock.block_size);
        let remaining = file.inode.size - byte_offset;
        let length = remaining.min(u64::from(self.superblock.block_size)) as usize;
        let physical_block = self
            .inode_physical_block(device, &file.inode, logical_block)
            .await;
        if let Some(physical_block) = physical_block {
            let block = self
                .cache
                .read_block(device, &self.superblock, physical_block)
                .await;
            &block[..length]
        } else {
            &ZERO_BLOCK[..length]
        }
    }

    async fn prefetch_file_pair(
        &mut self,
        device: &mut BlockDevice,
        file: &Ext4File,
        first_logical_block: u32,
        second_logical_block: u32,
    ) {
        let first = self
            .inode_physical_block(device, &file.inode, first_logical_block)
            .await
            .unwrap_or_else(|| device.fail("ext4 paired prefetch starts in a hole"));
        let second = self
            .inode_physical_block(device, &file.inode, second_logical_block)
            .await
            .unwrap_or_else(|| device.fail("ext4 paired prefetch ends in a hole"));
        self.cache
            .prefetch_pair(device, &self.superblock, first, second)
            .await;
    }

    async fn overwrite_existing_file_block(
        &mut self,
        device: &mut BlockDevice,
        file: &Ext4File,
        logical_block: u32,
        data: &[u8],
    ) -> u64 {
        let byte_offset = u64::from(logical_block)
            .checked_mul(u64::from(self.superblock.block_size))
            .unwrap_or_else(|| device.fail("ext4 write offset overflow"));
        let write_end = byte_offset
            .checked_add(u64::from(self.superblock.block_size))
            .unwrap_or_else(|| device.fail("ext4 write end overflow"));
        if data.len() != BLOCK_SIZE || write_end > file.inode.size {
            device.fail("ext4 in-place write must cover an existing full block");
        }
        let physical_block = self
            .inode_physical_block(device, &file.inode, logical_block)
            .await
            .unwrap_or_else(|| device.fail("ext4 in-place write targets a hole"));
        let sector = block_to_sector(device, &self.superblock, physical_block);
        device.write(sector, data).await;
        device.flush().await;
        self.cache.invalidate(physical_block);
        physical_block
    }

    async fn inode_physical_block(
        &mut self,
        device: &mut BlockDevice,
        inode: &Inode,
        logical_block: u32,
    ) -> Option<u64> {
        let file_block_count = inode.size.div_ceil(u64::from(self.superblock.block_size));
        if u64::from(logical_block) >= file_block_count {
            device.fail("ext4 inode read is past end of file");
        }
        let extent = self.file_extent(device, inode, logical_block).await?;
        if extent.unwritten {
            return None;
        }
        let physical_block =
            extent.physical_block + u64::from(logical_block - extent.logical_block);
        let extent_end = extent
            .physical_block
            .checked_add(u64::from(extent.block_count))
            .unwrap_or_else(|| device.fail("ext4 regular file extent overflows"));
        if physical_block >= self.superblock.block_count || extent_end > self.superblock.block_count
        {
            device.fail("ext4 regular file block is outside the filesystem");
        }
        Some(physical_block)
    }

    async fn file_extent(
        &mut self,
        device: &mut BlockDevice,
        inode: &Inode,
        logical_block: u32,
    ) -> Option<Extent> {
        let mut depth = inode
            .extent_depth()
            .unwrap_or_else(|_| device.fail("ext4 file extent root is invalid"));
        if depth == 0 {
            return inode
                .extent_for_logical_block(logical_block)
                .unwrap_or_else(|_| device.fail("ext4 inline extent is invalid"));
        }

        let mut child_block = inode
            .extent_index_for_logical_block(logical_block)
            .unwrap_or_else(|_| device.fail("ext4 extent root index is invalid"))
            .unwrap_or_else(|| device.fail("ext4 extent root index is missing"))
            .child_block;
        loop {
            if child_block == 0 || child_block >= self.superblock.block_count {
                device.fail("ext4 extent node block is outside the filesystem");
            }
            let block = self
                .cache
                .read_block(device, &self.superblock, child_block)
                .await;
            depth -= 1;
            let node = ExtentNode::parse(block, depth, inode, &self.superblock)
                .unwrap_or_else(|_| device.fail("ext4 extent node validation failed"));
            if node.depth() == 0 {
                return node
                    .extent_for_logical_block(logical_block)
                    .unwrap_or_else(|_| device.fail("ext4 extent leaf is invalid"));
            }
            child_block = node
                .extent_index_for_logical_block(logical_block)
                .unwrap_or_else(|_| device.fail("ext4 extent index node is invalid"))
                .unwrap_or_else(|| device.fail("ext4 extent index entry is missing"))
                .child_block;
        }
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

struct Ext4File {
    inode: Inode,
    parent_inode: u32,
    directory_block: u32,
    followed_symlink: u32,
}

struct RootProbe {
    extent_block: u64,
    entry_count: usize,
    etc_inode: u32,
    lost_and_found_inode: u32,
}

struct JournalProbe {
    physical_block: u64,
    superblock: JournalSuperblock,
}

#[derive(Clone, Copy)]
struct CacheEntry {
    block: u64,
    frame: usize,
    valid: bool,
}

impl CacheEntry {
    const EMPTY: Self = Self {
        block: 0,
        frame: 0,
        valid: false,
    };
}

struct BlockCache {
    entries: [CacheEntry; CACHE_ENTRY_COUNT],
    next_victim: usize,
    hits: u64,
    misses: u64,
    batched_pairs: u64,
    invalidations: u64,
}

impl BlockCache {
    fn new() -> Self {
        let mut entries = [CacheEntry::EMPTY; CACHE_ENTRY_COUNT];
        for entry in &mut entries {
            let frame = crate::memory::allocate_frame()
                .unwrap_or_else(|| crate::fatal("out of frames for filesystem block cache"));
            // SAFETY: the allocator returned an exclusive identity-mapped frame.
            unsafe { ptr::write_bytes(frame as *mut u8, 0, BLOCK_SIZE) };
            entry.frame = frame as usize;
        }
        Self {
            entries,
            next_victim: 0,
            hits: 0,
            misses: 0,
            batched_pairs: 0,
            invalidations: 0,
        }
    }

    async fn read_block<'a>(
        &'a mut self,
        device: &mut BlockDevice,
        superblock: &Superblock,
        block: u64,
    ) -> &'a [u8] {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.valid && entry.block == block)
        {
            self.hits += 1;
            return self.entry_bytes(index);
        }

        let sector = block_to_sector(device, superblock, block);
        device.read(sector, BLOCK_SIZE).await;
        let victim = self.next_victim;
        self.next_victim = (self.next_victim + 1) % CACHE_ENTRY_COUNT;
        let frame = self.entries[victim].frame;
        // SAFETY: source is the completed device buffer and destination is the
        // exclusive cache frame for this victim entry; both span one block.
        unsafe {
            ptr::copy_nonoverlapping(
                device.data(BLOCK_SIZE).as_ptr(),
                frame as *mut u8,
                BLOCK_SIZE,
            )
        };
        self.entries[victim].block = block;
        self.entries[victim].valid = true;
        self.misses += 1;
        self.entry_bytes(victim)
    }

    async fn prefetch_pair(
        &mut self,
        device: &mut BlockDevice,
        superblock: &Superblock,
        first_block: u64,
        second_block: u64,
    ) {
        if first_block == second_block {
            device.fail("ext4 paired prefetch blocks are not distinct");
        }
        let first_cached = self
            .entries
            .iter()
            .any(|entry| entry.valid && entry.block == first_block);
        let second_cached = self
            .entries
            .iter()
            .any(|entry| entry.valid && entry.block == second_block);
        if first_cached || second_cached {
            return;
        }

        let first_sector = block_to_sector(device, superblock, first_block);
        let second_sector = block_to_sector(device, superblock, second_block);
        device
            .read_pair(first_sector, second_sector, BLOCK_SIZE)
            .await;

        let first_victim = self.next_victim;
        let second_victim = (first_victim + 1) % CACHE_ENTRY_COUNT;
        self.next_victim = (second_victim + 1) % CACHE_ENTRY_COUNT;
        for (slot, victim, block) in [
            (0, first_victim, first_block),
            (1, second_victim, second_block),
        ] {
            let frame = self.entries[victim].frame;
            // SAFETY: each completed DMA slot and each selected cache frame
            // are disjoint live 4 KiB buffers.
            unsafe {
                ptr::copy_nonoverlapping(
                    device.pair_data(slot, BLOCK_SIZE).as_ptr(),
                    frame as *mut u8,
                    BLOCK_SIZE,
                )
            };
            self.entries[victim].block = block;
            self.entries[victim].valid = true;
        }
        self.misses += 2;
        self.batched_pairs += 1;
    }

    fn entry_bytes(&self, index: usize) -> &[u8] {
        // SAFETY: every cache entry owns a permanently live 4 KiB frame.
        unsafe { core::slice::from_raw_parts(self.entries[index].frame as *const u8, BLOCK_SIZE) }
    }

    fn invalidate(&mut self, block: u64) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.valid && entry.block == block)
        else {
            return false;
        };
        entry.valid = false;
        self.invalidations += 1;
        true
    }
}

async fn read_group_descriptor(
    device: &mut BlockDevice,
    superblock: &Superblock,
    group_index: u32,
    cache: &mut BlockCache,
) -> GroupDescriptor {
    let location = superblock
        .group_descriptor_location(group_index)
        .unwrap_or_else(|_| device.fail("ext4 group descriptor location is invalid"));
    let block = cache.read_block(device, superblock, location.block).await;
    let offset = location.offset as usize;
    let end = offset + usize::from(superblock.descriptor_size);
    if end > BLOCK_SIZE {
        device.fail("ext4 group descriptor crosses the read block");
    }
    let descriptor = GroupDescriptor::parse(&block[offset..end], group_index, superblock)
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
