// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::future::Future;
#[cfg(feature = "journal-replay-injection")]
use core::future::pending;
use core::pin::Pin;
use core::ptr;
use core::task::{Context, Poll};
use slopos_desktop_protocol::EVENT_CONFIG_APPLIED;
use slopos_ext4::{
    DIRECTORY_ENTRY_DIRECTORY, DIRECTORY_ENTRY_REGULAR_FILE, DIRECTORY_ENTRY_SYMLINK,
    DirectoryBlock, Extent, ExtentNode, FEATURE_INCOMPAT_RECOVER, GroupDescriptor,
    INODE_FLAG_DIRECTORY_INDEX, Inode, JOURNAL_INODE, JournalCommit, JournalDescriptor,
    JournalDescriptorBlock, JournalSuperblock, ParseError, ROOT_INODE, SUPERBLOCK_SIZE, Superblock,
    decode_journal_tag_data, encode_journal_commit_block, encode_journal_data_block,
    encode_journal_descriptor_block, encode_single_block_journal_transaction,
    initialize_empty_regular_inode, insert_linear_directory_entry, remove_linear_directory_entry,
    resize_regular_file_by_one_block, set_block_allocation, set_inode_allocation, set_inode_size,
    set_journal_superblock_state, set_superblock_free_block_count, set_superblock_free_inode_count,
    set_superblock_recovery, validate_path_component,
};
use slopos_vfs::{
    AbsolutePath, AccessMode, DescriptorObject, FIRST_FILE_DESCRIPTOR, FileDescriptorTable,
    FileNode, MountTable, ReadWindow, WriteWindow,
};

use crate::virtio::BlockDevice;

const SUPERBLOCK_SECTOR: u64 = 2;
const SUPERBLOCK_OFFSET: usize = 1024;
const SECTOR_SIZE: u64 = 512;
const BLOCK_SIZE: usize = 4096;
const CACHE_ENTRY_COUNT: usize = 8;
const MULTI_TRANSACTION_MAX_BLOCKS: usize = 8;
const ALLOCATION_TRANSACTION_BLOCKS: usize = 5;
const ALLOCATION_PROBE_BLOCK: u64 = 120;
const CREATE_TRANSACTION_BLOCKS: usize = 5;
const CREATE_PROBE_INODE: u32 = 32;
const CREATE_PROBE_NAME: &[u8] = b"create-probe";
const CREATE_PROBE_TIMESTAMP: u32 = 0x6a6a_9400;
static ZERO_BLOCK: [u8; BLOCK_SIZE] = [0; BLOCK_SIZE];
static JOURNAL_PROBE_BLOCK: [u8; BLOCK_SIZE] = [b'J'; BLOCK_SIZE];
static ALLOCATION_PROBE_BLOCK_DATA: [u8; BLOCK_SIZE] = [b'G'; BLOCK_SIZE];
static WRITE_PROBE_ORIGINAL_BLOCK: [u8; BLOCK_SIZE] = [b'P'; BLOCK_SIZE];
const EXPECTED_RELEASE: &[u8] = include_bytes!("../../rootfs/etc/slopos-release");
const EXPECTED_SYSTEM_CONFIGURATION: &[u8] = include_bytes!("../../rootfs/etc/slopos/system.conf");
const RELEASE_PATH: [&[u8]; 2] = [b"etc", b"slopos-release"];
const CONFIGURATION_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"system.conf"];
const MULTIBLOCK_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"vfs-wallpaper.png"];
const DEEP_EXTENT_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"deep-extent.bin"];
const CROSS_BLOCK_PATH: [&[u8]; 5] = [b"usr", b"share", b"slopos", b"large-directory", b"tail-29"];
const SYMLINK_PATH: [&[u8]; 2] = [b"etc", b"current-release"];
const WRITE_PROBE_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", b"write-probe.bin"];
const CREATE_PROBE_PATH: [&[u8]; 4] = [b"usr", b"share", b"slopos", CREATE_PROBE_NAME];
const ROOT_FILESYSTEM_ID: u16 = 1;
const VFS_TEST_PATH: &[u8] = b"/etc/./slopos/../slopos/system.conf";
const INIT_EXECUTABLE_PATH: [&[u8]; 2] = [b"sbin", b"slop-init"];
const INIT_EXECUTABLE_DISPLAY: &str = "/sbin/slop-init";
const DESKTOP_EXECUTABLE_PATH: [&[u8]; 2] = [b"sbin", b"slop-shell"];
const DESKTOP_EXECUTABLE_DISPLAY: &str = "/sbin/slop-shell";
const INIT_EXECUTABLE_CAPACITY: usize = 32 * 1024;
const DESKTOP_EXECUTABLE_CAPACITY: usize = 40 * 1024;
const PROCESS_FILE_CAPACITY: usize = 8;
const LINUX_ENOENT: i64 = -2;
const LINUX_EBADF: i64 = -9;
const LINUX_EINVAL: i64 = -22;
const LINUX_EMFILE: i64 = -24;
type ProcessOpenFiles =
    [[Option<Ext4File>; PROCESS_FILE_CAPACITY]; crate::process::PROCESS_CAPACITY];

struct UserProcessRuntime {
    namespace: MountTable<4>,
    open_files: ProcessOpenFiles,
    desktop_wait: crate::process::DesktopWaitRequest,
}

enum BlockRuntimeEvent {
    Desktop(slopos_desktop_protocol::DesktopServiceEvent),
    Reload { inject_invalid: bool },
    Wallpaper,
}

struct NextBlockRuntimeEvent {
    desktop_wait: crate::process::DesktopWaitRequest,
}

impl Future for NextBlockRuntimeEvent {
    type Output = BlockRuntimeEvent;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(event) = crate::desktop_service::event_after(
            self.desktop_wait.kind(),
            self.desktop_wait.after_generation(),
        ) {
            return Poll::Ready(BlockRuntimeEvent::Desktop(event));
        }
        if crate::desktop_config::take_reload_request() {
            return Poll::Ready(BlockRuntimeEvent::Reload {
                inject_invalid: crate::desktop_config::take_invalid_reload_request(),
            });
        }
        if crate::wallpaper_file::request_ready() {
            return Poll::Ready(BlockRuntimeEvent::Wallpaper);
        }
        Poll::Pending
    }
}
const NIRI_USER_PATH: [&[u8]; 5] = [b"home", b"slop", b".config", b"niri", b"config.kdl"];
const NIRI_SYSTEM_PATH: [&[u8]; 3] = [b"etc", b"niri", b"config.kdl"];
const NIRI_FALLBACK_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"niri.kdl"];
const WAYBAR_USER_JSONC_PATH: [&[u8]; 5] =
    [b"home", b"slop", b".config", b"waybar", b"config.jsonc"];
const WAYBAR_USER_CONFIG_PATH: [&[u8]; 5] = [b"home", b"slop", b".config", b"waybar", b"config"];
const WAYBAR_SYSTEM_JSONC_PATH: [&[u8]; 4] = [b"etc", b"xdg", b"waybar", b"config.jsonc"];
const WAYBAR_SYSTEM_CONFIG_PATH: [&[u8]; 4] = [b"etc", b"xdg", b"waybar", b"config"];
const WAYBAR_FALLBACK_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"waybar.jsonc"];
const WAYBAR_USER_STYLE_PATH: [&[u8]; 5] = [b"home", b"slop", b".config", b"waybar", b"style.css"];
const WAYBAR_SYSTEM_STYLE_PATH: [&[u8]; 4] = [b"etc", b"xdg", b"waybar", b"style.css"];
const WAYBAR_STYLE_FALLBACK_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"waybar.css"];
const SWWW_USER_ENV_PATH: [&[u8]; 5] = [b"home", b"slop", b".config", b"swww", b"env"];
const SWWW_SYSTEM_ENV_PATH: [&[u8]; 3] = [b"etc", b"swww", b"env"];
const SWWW_FALLBACK_PATH: [&[u8]; 3] = [b"etc", b"slopos", b"swww.env"];

struct InitExecutableStorage(UnsafeCell<[u8; INIT_EXECUTABLE_CAPACITY]>);
struct DesktopExecutableStorage(UnsafeCell<[u8; DESKTOP_EXECUTABLE_CAPACITY]>);

// The block task is the sole writer and starts both user processes before it
// resumes filesystem work. Their executable bytes remain immutable afterward.
unsafe impl Sync for InitExecutableStorage {}
unsafe impl Sync for DesktopExecutableStorage {}

static INIT_EXECUTABLE: InitExecutableStorage =
    InitExecutableStorage(UnsafeCell::new([0; INIT_EXECUTABLE_CAPACITY]));
static DESKTOP_EXECUTABLE: DesktopExecutableStorage =
    DesktopExecutableStorage(UnsafeCell::new([0; DESKTOP_EXECUTABLE_CAPACITY]));

#[derive(Clone, Copy)]
struct ConfigCandidate {
    components: &'static [&'static [u8]],
    display: &'static str,
}

const NIRI_CONFIG_CANDIDATES: &[ConfigCandidate] = &[
    ConfigCandidate {
        components: &NIRI_USER_PATH,
        display: "/home/slop/.config/niri/config.kdl",
    },
    ConfigCandidate {
        components: &NIRI_SYSTEM_PATH,
        display: "/etc/niri/config.kdl",
    },
    ConfigCandidate {
        components: &NIRI_FALLBACK_PATH,
        display: "/etc/slopos/niri.kdl",
    },
];
const WAYBAR_CONFIG_CANDIDATES: &[ConfigCandidate] = &[
    ConfigCandidate {
        components: &WAYBAR_USER_JSONC_PATH,
        display: "/home/slop/.config/waybar/config.jsonc",
    },
    ConfigCandidate {
        components: &WAYBAR_USER_CONFIG_PATH,
        display: "/home/slop/.config/waybar/config",
    },
    ConfigCandidate {
        components: &WAYBAR_SYSTEM_JSONC_PATH,
        display: "/etc/xdg/waybar/config.jsonc",
    },
    ConfigCandidate {
        components: &WAYBAR_SYSTEM_CONFIG_PATH,
        display: "/etc/xdg/waybar/config",
    },
    ConfigCandidate {
        components: &WAYBAR_FALLBACK_PATH,
        display: "/etc/slopos/waybar.jsonc",
    },
];
const WAYBAR_STYLE_CANDIDATES: &[ConfigCandidate] = &[
    ConfigCandidate {
        components: &WAYBAR_USER_STYLE_PATH,
        display: "/home/slop/.config/waybar/style.css",
    },
    ConfigCandidate {
        components: &WAYBAR_SYSTEM_STYLE_PATH,
        display: "/etc/xdg/waybar/style.css",
    },
    ConfigCandidate {
        components: &WAYBAR_STYLE_FALLBACK_PATH,
        display: "/etc/slopos/waybar.css",
    },
];
const SWWW_CONFIG_CANDIDATES: &[ConfigCandidate] = &[
    ConfigCandidate {
        components: &SWWW_USER_ENV_PATH,
        display: "/home/slop/.config/swww/env",
    },
    ConfigCandidate {
        components: &SWWW_SYSTEM_ENV_PATH,
        display: "/etc/swww/env",
    },
    ConfigCandidate {
        components: &SWWW_FALLBACK_PATH,
        display: "/etc/slopos/swww.env",
    },
];

pub async fn mount_task(mut device: BlockDevice, boot_user_image: &'static [u8]) -> ! {
    crate::serial::serialln(format_args!(
        "SLOPOS-VIRTIO: modern block queue ready size={} capacity_sectors={} flush={}",
        device.queue_size(),
        device.capacity_sectors(),
        device.flush_supported()
    ));
    let mut mount = Ext4Mount::mount(&mut device).await;
    if let Some(recovery) = mount.recovery {
        if recovery.replayed {
            crate::serial::serialln(format_args!(
                "SLOPOS-EXT4: journal recovery replayed sequence={} start={} tags={} first_target_block={} escaped={} home_readback=true next_sequence={} records_cleared=true recovery=false",
                recovery.sequence,
                recovery.start,
                recovery.tag_count,
                recovery.target_block,
                recovery.escaped,
                recovery.next_sequence
            ));
        } else {
            crate::serial::serialln(format_args!(
                "SLOPOS-EXT4: journal recovery completed sequence={} start=0 replayed=false recovery=false",
                recovery.sequence
            ));
        }
    }
    let mut user_processes =
        load_and_start_user_processes(&mut mount, &mut device, boot_user_image).await;
    #[cfg(feature = "journal-replay-injection")]
    if mount.recovery.is_none() {
        let file = mount.open_file(&mut device, &WRITE_PROBE_PATH).await;
        let journal = mount.probe_journal(&mut device).await;
        let allocation = mount
            .inject_committed_allocation_transaction(&mut device, &journal, &file)
            .await;
        crate::serial::serialln(format_args!(
            "SLOPOS-EXT4: allocation crash injected sequence={} start={} tags=5 targets={}/{}/{}/{}/{} old_state=allocated/grown new_state=free/original crash_point=after_commit_before_home writes=14 flushes=5",
            journal.superblock.sequence,
            journal.superblock.first_log_block,
            allocation.targets[0],
            allocation.targets[1],
            allocation.targets[2],
            allocation.targets[3],
            allocation.targets[4]
        ));
        pending::<()>().await;
        unreachable!();
    }
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

    load_and_publish_desktop_config(&mut mount, &mut device, true, false).await;

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
    let first_wallpaper_block = mount.read_file_block(&mut device, &multiblock, 0).await;
    if first_wallpaper_block.len() != BLOCK_SIZE
        || !first_wallpaper_block.starts_with(b"\x89PNG\r\n\x1a\n")
        || &first_wallpaper_block[12..16] != b"IHDR"
    {
        device.fail("ext4 multiblock PNG wallpaper first block mismatch");
    }
    let first_wallpaper_block_length = first_wallpaper_block.len();
    let second_wallpaper_block = mount.read_file_block(&mut device, &multiblock, 1).await;
    if second_wallpaper_block.len() != 2048
        || &second_wallpaper_block[second_wallpaper_block.len() - 12..]
            != b"\x00\x00\x00\x00IEND\xae\x42\x60\x82"
    {
        device.fail("ext4 multiblock PNG wallpaper second block mismatch");
    }
    let multiblock_bytes = first_wallpaper_block_length + second_wallpaper_block.len();
    let multiblock_group = mount
        .superblock
        .inode_group(multiblock.inode.number)
        .unwrap_or_else(|_| device.fail("ext4 multiblock inode group is invalid"));
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: multiblock file valid inode={} inode_group={multiblock_group} bytes={multiblock_bytes} logical_blocks=2 format=PNG ancillary_padding=valid path=/usr/share/slopos/vfs-wallpaper.png",
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
    let write_probe_initial_invalidations = mount.cache.invalidations;
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
    if mount.cache.invalidations != write_probe_initial_invalidations + 2 {
        device.fail("ext4 write probe cache invalidation count mismatch");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: writable descriptor valid fd={write_fd} inode={} physical_block={physical_block} offset=123 bytes={} writes=2 flushes=2 cache_invalidations={} restored=true path=/usr/share/slopos/write-probe.bin",
        write_probe.inode.number,
        write_bytes.len(),
        mount.cache.invalidations - write_probe_initial_invalidations
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
    let journal_records = mount
        .stage_inactive_journal_records(&mut device, &journal, physical_block)
        .await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: journal records staged sequence={} target_block={} descriptor_block={} data_block={} commit_block={} writes=6 flushes=3 verified=true restored=true active=false",
        journal_records.sequence,
        journal_records.target_block,
        journal_records.descriptor_block,
        journal_records.data_block,
        journal_records.commit_block
    ));
    let journal_state = mount
        .probe_journal_state_transition(&mut device, &journal)
        .await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: journal state transition recovery=true sequence={} start={} readback=valid checkpoint_start=0 restored=true transactions=0 writes=4 flushes=4",
        journal_state.sequence, journal_state.active_start
    ));
    let active_transaction = mount
        .probe_active_journal_transaction(&mut device, &journal, &write_probe, physical_block)
        .await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: active journal transaction valid sequence={} target_block={} recovery=true start=1 records=descriptor/data/commit replayable_readback=true home_checkpointed=true next_sequence={} test_sequence_rewound=true restored=true writes=13 flushes=10",
        active_transaction.sequence,
        active_transaction.target_block,
        active_transaction.next_sequence
    ));
    let metadata_transaction = mount
        .probe_inode_metadata_transactions(&mut device, &journal, &write_probe)
        .await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: metadata journal transactions valid inode={} inode_table_block={} size=4095/4096 checksums=valid transactions=2 sequences={}/{} final_sequence={} test_sequence_rewound=true restored=true writes=23 flushes=17",
        write_probe.inode.number,
        metadata_transaction.target_block,
        metadata_transaction.first_sequence,
        metadata_transaction.second_sequence,
        metadata_transaction.final_sequence
    ));
    let allocation_transaction = mount
        .probe_block_allocation_transactions(
            &mut device,
            &journal,
            &write_probe,
            &mut descriptors,
            write_fd,
        )
        .await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: fd append journal transactions valid fd={} inode={} block={} bitmap_block={} group_descriptor_block={} inode_table_block={} append_bytes=4096 size=4096/8192/4096 extent_blocks=1/2/1 checksums=superblock/group/bitmap/inode/data transactions=2 sequences={}/{} final_sequence={} test_sequence_rewound=true restored=true",
        write_fd,
        write_probe.inode.number,
        allocation_transaction.allocated_block,
        allocation_transaction.bitmap_block,
        allocation_transaction.group_descriptor_block,
        allocation_transaction.inode_table_block,
        allocation_transaction.first_sequence,
        allocation_transaction.second_sequence,
        allocation_transaction.final_sequence
    ));
    descriptors
        .close(write_fd)
        .unwrap_or_else(|_| device.fail("VFS writable descriptor close failed"));
    let create_transaction = mount
        .probe_file_creation_transactions(
            &mut device,
            &journal,
            write_probe.parent_inode,
            &mut descriptors,
        )
        .await;
    crate::serial::serialln(format_args!(
        "SLOPOS-EXT4: VFS create journal transactions valid fd={} inode={} parent_inode={} inode_bitmap_block={} group_descriptor_block={} inode_table_block={} directory_block={} free_inodes={}/{}/{} size=0 access=readwrite checksums=superblock/group/bitmap/inode/directory transactions=2 sequences={}/{} final_sequence={} test_sequence_rewound=true restored=true path=/usr/share/slopos/create-probe",
        create_transaction.fd,
        CREATE_PROBE_INODE,
        write_probe.parent_inode,
        create_transaction.inode_bitmap_block,
        create_transaction.group_descriptor_block,
        create_transaction.inode_table_block,
        create_transaction.directory_block,
        create_transaction.free_inodes,
        create_transaction.allocated_free_inodes,
        create_transaction.free_inodes,
        create_transaction.first_sequence,
        create_transaction.second_sequence,
        create_transaction.final_sequence
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
    loop {
        match (NextBlockRuntimeEvent {
            desktop_wait: user_processes.desktop_wait,
        })
        .await
        {
            BlockRuntimeEvent::Desktop(event) => {
                let process_event =
                    crate::process::resume_desktop_wait(user_processes.desktop_wait, event);
                user_processes.desktop_wait = drive_user_processes(
                    &mut mount,
                    &mut device,
                    &user_processes.namespace,
                    &mut user_processes.open_files,
                    process_event,
                )
                .await;
                crate::serial::serialln(format_args!(
                    "SLOPOS-PROCESS: desktop service parked event=config-applied after_generation={} init=wait4 resources=retained",
                    user_processes.desktop_wait.after_generation()
                ));
            }
            BlockRuntimeEvent::Reload { inject_invalid } => {
                load_and_publish_desktop_config(&mut mount, &mut device, false, inject_invalid)
                    .await;
            }
            BlockRuntimeEvent::Wallpaper => {
                let request = crate::wallpaper_file::take_request()
                    .unwrap_or_else(|| device.fail("swww VFS request signal lost its payload"));
                load_and_publish_wallpaper(&mut mount, &mut device, request).await;
            }
        }
    }
}

async fn load_and_start_user_processes(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    boot_user_image: &[u8],
) -> UserProcessRuntime {
    let init_executable = mount
        .try_open_file(device, &INIT_EXECUTABLE_PATH)
        .await
        .unwrap_or_else(|| device.fail("root VFS init executable was not found"));
    let init_size = usize::try_from(init_executable.inode.size)
        .unwrap_or_else(|_| device.fail("root VFS init executable exceeds address space"));
    if init_size == 0 || init_size > INIT_EXECUTABLE_CAPACITY {
        device.fail("root VFS init executable has an invalid size");
    }
    // SAFETY: the block task is the only writer. Neither backing storage is
    // mutated after the process images are constructed below.
    let init_storage = unsafe { &mut *INIT_EXECUTABLE.0.get() };
    init_storage.fill(0);
    let mut copied = 0usize;
    let mut logical_block = 0u32;
    while copied < init_size {
        let bytes = mount
            .read_file_block(device, &init_executable, logical_block)
            .await;
        let length = bytes.len().min(init_size - copied);
        init_storage[copied..copied + length].copy_from_slice(&bytes[..length]);
        copied += length;
        logical_block += 1;
    }
    let init_image = &init_storage[..init_size];
    if init_image != boot_user_image {
        device.fail("root VFS init executable differs from the boot copy");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: executable loaded path={INIT_EXECUTABLE_DISPLAY} inode={} bytes={init_size} blocks={logical_block} matches_boot=true",
        init_executable.inode.number
    ));

    let desktop_executable = mount
        .try_open_file(device, &DESKTOP_EXECUTABLE_PATH)
        .await
        .unwrap_or_else(|| device.fail("root VFS desktop executable was not found"));
    let desktop_size = usize::try_from(desktop_executable.inode.size)
        .unwrap_or_else(|_| device.fail("root VFS desktop executable exceeds address space"));
    if desktop_size == 0 || desktop_size > DESKTOP_EXECUTABLE_CAPACITY {
        device.fail("root VFS desktop executable has an invalid size");
    }
    // SAFETY: same single-writer lifetime as INIT_EXECUTABLE above.
    let desktop_storage = unsafe { &mut *DESKTOP_EXECUTABLE.0.get() };
    desktop_storage.fill(0);
    copied = 0;
    logical_block = 0;
    while copied < desktop_size {
        let bytes = mount
            .read_file_block(device, &desktop_executable, logical_block)
            .await;
        let length = bytes.len().min(desktop_size - copied);
        desktop_storage[copied..copied + length].copy_from_slice(&bytes[..length]);
        copied += length;
        logical_block += 1;
    }
    let desktop_image = &desktop_storage[..desktop_size];
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: executable loaded path={DESKTOP_EXECUTABLE_DISPLAY} inode={} bytes={desktop_size} blocks={logical_block} matches_boot=not-required role=desktop-service",
        desktop_executable.inode.number
    ));

    let root_path =
        AbsolutePath::parse(b"/").unwrap_or_else(|_| device.fail("process VFS root is invalid"));
    let mut namespace = MountTable::<4>::new();
    namespace
        .mount(&root_path, ROOT_FILESYSTEM_ID)
        .unwrap_or_else(|_| device.fail("process VFS root mount failed"));
    let mut open_files: ProcessOpenFiles = core::array::from_fn(|_| core::array::from_fn(|_| None));
    let event = crate::process::start_processes(
        init_image,
        "vfs",
        INIT_EXECUTABLE_DISPLAY,
        desktop_image,
        "vfs",
        DESKTOP_EXECUTABLE_DISPLAY,
    );
    let desktop_wait =
        drive_user_processes(mount, device, &namespace, &mut open_files, event).await;
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: userspace runtime parked init=wait4 desktop=config-applied after_generation={} resources=retained",
        desktop_wait.after_generation()
    ));
    UserProcessRuntime {
        namespace,
        open_files,
        desktop_wait,
    }
}

async fn drive_user_processes(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    namespace: &MountTable<4>,
    open_files: &mut ProcessOpenFiles,
    mut event: crate::process::ProcessEvent,
) -> crate::process::DesktopWaitRequest {
    let mut parked_desktop = None;
    loop {
        event = match event {
            crate::process::ProcessEvent::OpenAt(request) => {
                complete_process_openat(mount, device, namespace, open_files, request).await
            }
            crate::process::ProcessEvent::Read(request) => {
                complete_process_read(mount, device, open_files, request).await
            }
            crate::process::ProcessEvent::Write(request) => {
                complete_process_write(mount, device, open_files, request).await
            }
            crate::process::ProcessEvent::Close(request) => {
                complete_process_close(open_files, request)
            }
            crate::process::ProcessEvent::Yielded { pid } => crate::process::schedule_next(pid),
            crate::process::ProcessEvent::Preempted { pid, tick, count } => {
                crate::process::schedule_after_preemption(pid, tick, count)
            }
            crate::process::ProcessEvent::Waiting { pid } => {
                if let Some(request) = parked_desktop.take() {
                    return request;
                }
                crate::process::schedule_next(pid)
            }
            crate::process::ProcessEvent::DesktopWaiting(request) => {
                if request.kind() == EVENT_CONFIG_APPLIED {
                    if let Some(next) = crate::process::schedule_next_if_any(request.pid()) {
                        parked_desktop = Some(request);
                        next
                    } else {
                        return request;
                    }
                } else {
                    let event = crate::desktop_service::next_event(
                        request.kind(),
                        request.after_generation(),
                    )
                    .await;
                    crate::process::resume_desktop_wait(request, event)
                }
            }
            crate::process::ProcessEvent::WaylandWaiting(request) => {
                let event = crate::wayland_service::next_event(request.after_sequence()).await;
                crate::process::resume_wayland_wait(request, event)
            }
            crate::process::ProcessEvent::Exited { pid } => {
                release_exited_process_files(device, open_files, pid);
                device.fail("persistent user process exited");
            }
        };
    }
}

async fn complete_process_openat(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    namespace: &MountTable<4>,
    open_files: &mut ProcessOpenFiles,
    request: crate::process::OpenAtRequest,
) -> crate::process::ProcessEvent {
    let pid = request.pid();
    let process_index = process_index(pid)
        .unwrap_or_else(|| device.fail("process PID is outside the VFS backing table"));
    let path = match AbsolutePath::parse(request.path()) {
        Ok(path) => path,
        Err(_) => return crate::process::resume_probe(pid, LINUX_EINVAL, None),
    };
    let resolution = match namespace.resolve(&path) {
        Ok(resolution) => resolution,
        Err(_) => return crate::process::resume_probe(pid, LINUX_ENOENT, None),
    };
    let Some(file) = mount
        .try_open_file(device, &path.components()[resolution.matched_components..])
        .await
    else {
        return crate::process::resume_probe(pid, LINUX_ENOENT, None);
    };
    let inode = file.inode.number;
    let size = file.inode.size;
    let access_mode = request.access_mode();
    let fd = match crate::process::open_file(
        pid,
        FileNode {
            filesystem_id: resolution.filesystem_id,
            node_id: u64::from(inode),
            size,
        },
        access_mode,
    ) {
        Ok(fd) => fd,
        Err(_) => return crate::process::resume_probe(pid, LINUX_EMFILE, None),
    };
    let slot = process_file_slot(fd)
        .unwrap_or_else(|| device.fail("process VFS descriptor is outside backing table"));
    if open_files[process_index][slot].is_some() {
        device.fail("process VFS backing slot was already occupied");
    }
    open_files[process_index][slot] = Some(file);
    let display = core::str::from_utf8(request.path()).unwrap_or("<non-utf8>");
    let access = match access_mode {
        AccessMode::ReadOnly => "readonly",
        AccessMode::WriteOnly => "writeonly",
        AccessMode::ReadWrite => "readwrite",
    };
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: process open complete pid={pid} fd={fd} inode={inode} bytes={size} access={access} async=true path={display}"
    ));
    crate::process::resume_probe(pid, i64::from(fd), None)
}

async fn complete_process_read(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    open_files: &ProcessOpenFiles,
    request: crate::process::ReadRequest,
) -> crate::process::ProcessEvent {
    let pid = request.pid;
    match crate::process::descriptor_object(pid, request.fd) {
        Ok(DescriptorObject::LocalSocket { index, generation }) => {
            let handle = slopos_ipc::SocketHandle::from_parts(index, generation);
            let mut output = [0u8; crate::process::PROCESS_SYSCALL_IO_CAPACITY];
            let bytes = match crate::local_socket_service::recv(
                pid,
                handle,
                &mut output[..request.requested],
            )
            .await
            {
                Ok(bytes) => bytes,
                Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
            };
            crate::serial::serialln(format_args!(
                "SLOPOS-IPC: process read complete pid={pid} fd={} family=AF_UNIX type=SOCK_STREAM requested={} bytes={bytes} user_pages={} async=true",
                request.fd,
                request.requested,
                request.user_pages()
            ));
            return crate::process::resume_probe(pid, bytes as i64, Some(&output[..bytes]));
        }
        Ok(DescriptorObject::File(_)) => {}
        Ok(DescriptorObject::SharedMemory { .. }) => {
            return crate::process::resume_probe(pid, LINUX_EBADF, None);
        }
        Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
    }
    let process_index = process_index(pid)
        .unwrap_or_else(|| device.fail("process PID is outside the VFS backing table"));
    let window = match crate::process::read_window(pid, request.fd, request.requested) {
        Ok(window) => window,
        Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
    };
    let Some(slot) = process_file_slot(request.fd) else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    let Some(file) = open_files[process_index][slot].as_ref() else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    if window.node.filesystem_id != ROOT_FILESYSTEM_ID
        || window.node.node_id != u64::from(file.inode.number)
    {
        device.fail("process VFS read vnode does not match backing file");
    }
    let mut output = [0u8; crate::process::PROCESS_SYSCALL_IO_CAPACITY];
    let bytes = read_process_file_range(
        mount,
        device,
        file,
        window,
        &mut output[..request.requested],
    )
    .await;
    crate::process::advance_fd(pid, request.fd, bytes)
        .unwrap_or_else(|_| device.fail("process VFS read offset advance failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: process read complete pid={pid} fd={} inode={} offset={} requested={} bytes={bytes} user_pages={} cross_page={} async=true",
        request.fd,
        file.inode.number,
        window.offset,
        request.requested,
        request.user_pages(),
        request.user_pages() > 1
    ));
    crate::process::resume_probe(pid, bytes as i64, Some(&output[..bytes]))
}

async fn complete_process_write(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    open_files: &ProcessOpenFiles,
    request: crate::process::WriteRequest,
) -> crate::process::ProcessEvent {
    let pid = request.pid;
    match crate::process::descriptor_object(pid, request.fd) {
        Ok(DescriptorObject::LocalSocket { index, generation }) => {
            let handle = slopos_ipc::SocketHandle::from_parts(index, generation);
            let result = match request.shared_rights() {
                Some(shared) if request.is_sendmsg() => {
                    crate::local_socket_service::send_with_rights(
                        pid,
                        handle,
                        request.input(),
                        shared,
                    )
                }
                None if !request.is_sendmsg() => {
                    crate::local_socket_service::send(pid, handle, request.input())
                }
                _ => return crate::process::resume_probe(pid, LINUX_EBADF, None),
            };
            let bytes = match result {
                Ok(bytes) => bytes,
                Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
            };
            let operation = if request.is_sendmsg() {
                "sendmsg/SCM_RIGHTS"
            } else {
                "write"
            };
            crate::serial::serialln(format_args!(
                "SLOPOS-IPC: process write complete pid={pid} fd={} family=AF_UNIX type=SOCK_STREAM operation={operation} requested={} bytes={bytes} user_pages={} async=true",
                request.fd,
                request.input().len(),
                request.user_pages()
            ));
            return crate::process::resume_probe(pid, bytes as i64, None);
        }
        Ok(DescriptorObject::File(_)) => {}
        Ok(DescriptorObject::SharedMemory { .. }) => {
            return crate::process::resume_probe(pid, LINUX_EBADF, None);
        }
        Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
    }
    let process_index = process_index(pid)
        .unwrap_or_else(|| device.fail("process PID is outside the VFS backing table"));
    let input = request.input();
    let window = match crate::process::write_window(pid, request.fd, input.len()) {
        Ok(window) => window,
        Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
    };
    let Some(slot) = process_file_slot(request.fd) else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    let Some(file) = open_files[process_index][slot].as_ref() else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    if window.node.filesystem_id != ROOT_FILESYSTEM_ID
        || window.node.node_id != u64::from(file.inode.number)
    {
        device.fail("process VFS write vnode does not match backing file");
    }
    let bytes = write_process_file_range(mount, device, file, window, input).await;
    crate::process::advance_fd(pid, request.fd, bytes)
        .unwrap_or_else(|_| device.fail("process VFS write offset advance failed"));
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: process write complete pid={pid} fd={} inode={} offset={} requested={} bytes={bytes} user_pages={} cross_page={} async=true flushed=true",
        request.fd,
        file.inode.number,
        window.offset,
        input.len(),
        request.user_pages(),
        request.user_pages() > 1
    ));
    crate::process::resume_probe(pid, bytes as i64, None)
}

fn complete_process_close(
    open_files: &mut ProcessOpenFiles,
    request: crate::process::CloseRequest,
) -> crate::process::ProcessEvent {
    let pid = request.pid;
    match crate::process::descriptor_object(pid, request.fd) {
        Ok(DescriptorObject::LocalSocket { index, generation }) => {
            let handle = slopos_ipc::SocketHandle::from_parts(index, generation);
            if crate::local_socket_service::close(pid, handle).is_err()
                || crate::process::close_fd(pid, request.fd).is_err()
            {
                return crate::process::resume_probe(pid, LINUX_EBADF, None);
            }
            crate::serial::serialln(format_args!(
                "SLOPOS-IPC: process close complete pid={pid} fd={} family=AF_UNIX type=SOCK_STREAM async=false",
                request.fd
            ));
            return crate::process::resume_probe(pid, 0, None);
        }
        Ok(DescriptorObject::SharedMemory { index, generation }) => {
            let handle =
                crate::shared_memory_service::SharedMemoryHandle::from_parts(index, generation);
            if crate::shared_memory_service::release(handle).is_err()
                || crate::process::close_fd(pid, request.fd).is_err()
            {
                return crate::process::resume_probe(pid, LINUX_EBADF, None);
            }
            crate::serial::serialln(format_args!(
                "SLOPOS-IPC: process close complete pid={pid} fd={} object=memfd shared={index}:{generation} async=false",
                request.fd
            ));
            return crate::process::resume_probe(pid, 0, None);
        }
        Ok(DescriptorObject::File(_)) => {}
        Err(_) => return crate::process::resume_probe(pid, LINUX_EBADF, None),
    }
    let Some(process_index) = process_index(pid) else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    let Some(slot) = process_file_slot(request.fd) else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    let Some(file) = open_files[process_index][slot].as_ref() else {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    };
    let inode = file.inode.number;
    if crate::process::close_fd(pid, request.fd).is_err() {
        return crate::process::resume_probe(pid, LINUX_EBADF, None);
    }
    open_files[process_index][slot] = None;
    crate::serial::serialln(format_args!(
        "SLOPOS-VFS: process close complete pid={pid} fd={} inode={inode} async=false",
        request.fd
    ));
    crate::process::resume_probe(pid, 0, None)
}

fn release_exited_process_files(device: &BlockDevice, open_files: &mut ProcessOpenFiles, pid: u32) {
    let process_index = process_index(pid)
        .unwrap_or_else(|| device.fail("exited PID is outside the VFS backing table"));
    let mut objects = [None; PROCESS_FILE_CAPACITY];
    let object_count = crate::process::descriptor_objects(pid, &mut objects)
        .unwrap_or_else(|_| device.fail("exited process descriptor snapshot failed"));
    let mut socket_objects = 0usize;
    let mut shared_objects = 0usize;
    for object in objects[..object_count].iter().flatten() {
        match object {
            DescriptorObject::LocalSocket { index, generation } => {
                let handle = slopos_ipc::SocketHandle::from_parts(*index, *generation);
                crate::local_socket_service::close(pid, handle)
                    .unwrap_or_else(|_| device.fail("exited process socket cleanup failed"));
                socket_objects += 1;
            }
            DescriptorObject::SharedMemory { index, generation } => {
                let handle = crate::shared_memory_service::SharedMemoryHandle::from_parts(
                    *index,
                    *generation,
                );
                crate::shared_memory_service::release(handle)
                    .unwrap_or_else(|_| device.fail("exited process memfd cleanup failed"));
                shared_objects += 1;
            }
            DescriptorObject::File(_) => {}
        }
    }
    let descriptors = crate::process::close_all_files(pid)
        .unwrap_or_else(|_| device.fail("exited process descriptor cleanup failed"));
    let mut backing_objects = 0usize;
    for file in &mut open_files[process_index] {
        if file.take().is_some() {
            backing_objects += 1;
        }
    }
    if descriptors != backing_objects + socket_objects + shared_objects {
        device.fail("exited process descriptor/backing cleanup diverged");
    }
    crate::serial::serialln(format_args!(
        "SLOPOS-PROCESS: pid={pid} exit resources released descriptors={descriptors} backing_objects={backing_objects} socket_objects={socket_objects} shared_objects={shared_objects} address_space_release=pending-reap"
    ));
}

async fn read_process_file_range(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    file: &Ext4File,
    window: ReadWindow,
    output: &mut [u8],
) -> usize {
    if output.len() < window.length {
        device.fail("process VFS output buffer is shorter than read window");
    }
    let block_size = u64::try_from(BLOCK_SIZE)
        .unwrap_or_else(|_| device.fail("process VFS block size conversion failed"));
    let mut copied = 0usize;
    while copied < window.length {
        let absolute_offset = window
            .offset
            .checked_add(
                u64::try_from(copied)
                    .unwrap_or_else(|_| device.fail("process VFS read offset conversion failed")),
            )
            .unwrap_or_else(|| device.fail("process VFS read offset overflow"));
        let logical_block = u32::try_from(absolute_offset / block_size)
            .unwrap_or_else(|_| device.fail("process VFS logical block overflow"));
        let block_offset = usize::try_from(absolute_offset % block_size)
            .unwrap_or_else(|_| device.fail("process VFS block offset conversion failed"));
        let block = mount.read_file_block(device, file, logical_block).await;
        let length = (window.length - copied).min(block.len() - block_offset);
        output[copied..copied + length]
            .copy_from_slice(&block[block_offset..block_offset + length]);
        copied += length;
    }
    copied
}

async fn write_process_file_range(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    file: &Ext4File,
    window: WriteWindow,
    input: &[u8],
) -> usize {
    if input.len() < window.length {
        device.fail("process VFS input buffer is shorter than write window");
    }
    let block_size = u64::try_from(BLOCK_SIZE)
        .unwrap_or_else(|_| device.fail("process VFS block size conversion failed"));
    let mut block_buffer = [0u8; BLOCK_SIZE];
    let mut copied = 0usize;
    while copied < window.length {
        let absolute_offset = window
            .offset
            .checked_add(
                u64::try_from(copied)
                    .unwrap_or_else(|_| device.fail("process VFS write offset conversion failed")),
            )
            .unwrap_or_else(|| device.fail("process VFS write offset overflow"));
        let logical_block = u32::try_from(absolute_offset / block_size)
            .unwrap_or_else(|_| device.fail("process VFS writable block index overflow"));
        let block_offset = usize::try_from(absolute_offset % block_size)
            .unwrap_or_else(|_| device.fail("process VFS write block offset conversion failed"));
        let block = mount.read_file_block(device, file, logical_block).await;
        if block.len() != BLOCK_SIZE {
            device.fail("process VFS partial-block EOF writes are unsupported");
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
    copied
}

fn process_file_slot(fd: u32) -> Option<usize> {
    let descriptor = fd.checked_sub(FIRST_FILE_DESCRIPTOR)?;
    let slot = usize::try_from(descriptor).ok()?;
    (slot < PROCESS_FILE_CAPACITY).then_some(slot)
}

fn process_index(pid: u32) -> Option<usize> {
    let index = usize::try_from(pid.checked_sub(1)?).ok()?;
    (index < crate::process::PROCESS_CAPACITY).then_some(index)
}

async fn load_and_publish_desktop_config(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    initial: bool,
    inject_invalid: bool,
) {
    let mut writer = match crate::desktop_config::begin_write() {
        Ok(writer) => writer,
        Err(error) => {
            crate::serial::serialln(format_args!(
                "SLOPOS-CONFIG: VFS load deferred initial={initial} error={error:?} retained_generation={}",
                crate::desktop_config::current_generation()
            ));
            return;
        }
    };

    let Some((niri, niri_path)) =
        open_config_candidate(mount, device, NIRI_CONFIG_CANDIDATES).await
    else {
        writer.cancel();
        config_source_missing(initial, "niri");
        return;
    };
    if niri.inode.size > 4096 {
        writer.cancel();
        config_source_invalid(initial, niri_path, "file-too-large");
        return;
    }
    let niri_bytes = mount.read_file_block(device, &niri, 0).await;
    if let Err(error) = writer.write_niri(niri_bytes, niri_path) {
        writer.cancel();
        config_source_invalid(initial, niri_path, config_error_name(error));
        return;
    }

    let Some((waybar, waybar_path)) =
        open_config_candidate(mount, device, WAYBAR_CONFIG_CANDIDATES).await
    else {
        writer.cancel();
        config_source_missing(initial, "waybar");
        return;
    };
    if waybar.inode.size > 4096 {
        writer.cancel();
        config_source_invalid(initial, waybar_path, "file-too-large");
        return;
    }
    let waybar_bytes = mount.read_file_block(device, &waybar, 0).await;
    if let Err(error) = writer.write_waybar(waybar_bytes, waybar_path) {
        writer.cancel();
        config_source_invalid(initial, waybar_path, config_error_name(error));
        return;
    }

    let Some((style, style_path)) =
        open_config_candidate(mount, device, WAYBAR_STYLE_CANDIDATES).await
    else {
        writer.cancel();
        config_source_missing(initial, "waybar-style");
        return;
    };
    if style.inode.size > 4096 {
        writer.cancel();
        config_source_invalid(initial, style_path, "file-too-large");
        return;
    }
    let style_bytes = mount.read_file_block(device, &style, 0).await;
    if let Err(error) = writer.write_waybar_style(style_bytes, style_path) {
        writer.cancel();
        config_source_invalid(initial, style_path, config_error_name(error));
        return;
    }
    if inject_invalid
        && let Err(error) =
            writer.write_waybar_style(b"window#waybar { color: invalid; }", "<invalid-probe>")
    {
        writer.cancel();
        config_source_invalid(initial, "<invalid-probe>", config_error_name(error));
        return;
    }

    let Some((swww, swww_path)) =
        open_config_candidate(mount, device, SWWW_CONFIG_CANDIDATES).await
    else {
        writer.cancel();
        config_source_missing(initial, "swww");
        return;
    };
    if swww.inode.size > 512 {
        writer.cancel();
        config_source_invalid(initial, swww_path, "file-too-large");
        return;
    }
    let swww_bytes = mount.read_file_block(device, &swww, 0).await;
    if let Err(error) = writer.write_swww(swww_bytes, swww_path) {
        writer.cancel();
        config_source_invalid(initial, swww_path, config_error_name(error));
        return;
    }

    match writer.publish() {
        Ok(generation) => crate::serial::serialln(format_args!(
            "SLOPOS-CONFIG: VFS load published initial={initial} generation={generation} atomic=true paths={niri_path},{waybar_path},{style_path},{swww_path}"
        )),
        Err(error) => crate::serial::serialln(format_args!(
            "SLOPOS-CONFIG: VFS load rejected initial={initial} error={} retained_generation={}",
            config_error_name(error),
            crate::desktop_config::current_generation()
        )),
    }
}

async fn load_and_publish_wallpaper(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    request: crate::wallpaper_file::WallpaperFileRequest,
) {
    let generation = request.generation();
    let requested = request.request_path();
    let resolved = request.resolved_path();
    let path = match AbsolutePath::parse(request.resolved_path_bytes()) {
        Ok(path) if path.component_count() != 0 => path,
        _ => {
            crate::wallpaper_file::publish_error(
                request,
                crate::wallpaper_file::WallpaperFileError::InvalidPath,
            );
            wallpaper_file_rejected(generation, requested, resolved, "invalid-path");
            return;
        }
    };
    let Some(file) = mount.try_open_file(device, path.components()).await else {
        crate::wallpaper_file::publish_error(
            request,
            crate::wallpaper_file::WallpaperFileError::NotFound,
        );
        wallpaper_file_rejected(generation, requested, resolved, "not-found");
        return;
    };
    let size = usize::try_from(file.inode.size).unwrap_or(usize::MAX);
    if size == 0 {
        crate::wallpaper_file::publish_error(
            request,
            crate::wallpaper_file::WallpaperFileError::InvalidPpm,
        );
        wallpaper_file_rejected(generation, requested, resolved, "invalid-ppm");
        return;
    }
    if size > crate::wallpaper_file::WALLPAPER_FILE_CAPACITY {
        crate::wallpaper_file::publish_error(
            request,
            crate::wallpaper_file::WallpaperFileError::FileTooLarge,
        );
        wallpaper_file_rejected(generation, requested, resolved, "file-size");
        return;
    }

    let mut writer = crate::wallpaper_file::begin_result(request);
    let mut copied = 0usize;
    let mut logical_block = 0u32;
    while copied < size {
        let bytes = mount.read_file_block(device, &file, logical_block).await;
        let length = bytes.len().min(size - copied);
        if length == 0 || !writer.write(&bytes[..length]) {
            writer.publish_error(crate::wallpaper_file::WallpaperFileError::FileTooLarge);
            wallpaper_file_rejected(generation, requested, resolved, "file-size");
            return;
        }
        copied += length;
        logical_block += 1;
    }
    match writer.publish() {
        Ok(publication) => crate::serial::serialln(format_args!(
            "SLOPOS-SWWW-VFS: load published generation={} request={requested} resolved={resolved} inode={} bytes={copied} blocks={logical_block} format={} async=true",
            publication.generation(),
            file.inode.number,
            publication.format().name(),
        )),
        Err(error) => wallpaper_file_rejected(
            generation,
            requested,
            resolved,
            wallpaper_file_error_name(error),
        ),
    }
}

fn wallpaper_file_rejected(generation: u64, requested: &str, resolved: &str, error: &str) {
    crate::serial::serialln(format_args!(
        "SLOPOS-SWWW-VFS: load rejected generation={generation} request={requested} resolved={resolved} error={error} retained=previous"
    ));
}

const fn wallpaper_file_error_name(
    error: crate::wallpaper_file::WallpaperFileError,
) -> &'static str {
    match error {
        crate::wallpaper_file::WallpaperFileError::InvalidPath => "invalid-path",
        crate::wallpaper_file::WallpaperFileError::NotFound => "not-found",
        crate::wallpaper_file::WallpaperFileError::FileTooLarge => "file-size",
        crate::wallpaper_file::WallpaperFileError::InvalidPpm => "invalid-ppm",
        crate::wallpaper_file::WallpaperFileError::InvalidPng => "invalid-png",
    }
}

async fn open_config_candidate(
    mount: &mut Ext4Mount,
    device: &mut BlockDevice,
    candidates: &[ConfigCandidate],
) -> Option<(Ext4File, &'static str)> {
    for candidate in candidates {
        if let Some(file) = mount.try_open_file(device, candidate.components).await {
            return Some((file, candidate.display));
        }
    }
    None
}

fn config_source_missing(initial: bool, kind: &str) {
    crate::serial::serialln(format_args!(
        "SLOPOS-CONFIG: VFS load rejected initial={initial} missing={kind} retained_generation={}",
        crate::desktop_config::current_generation()
    ));
}

fn config_source_invalid(initial: bool, path: &str, error: &str) {
    crate::serial::serialln(format_args!(
        "SLOPOS-CONFIG: VFS load rejected initial={initial} path={path} error={error} retained_generation={}",
        crate::desktop_config::current_generation()
    ));
}

const fn config_error_name(error: crate::desktop_config::ConfigPublishError) -> &'static str {
    match error {
        crate::desktop_config::ConfigPublishError::Busy => "busy",
        crate::desktop_config::ConfigPublishError::InvalidFile => "invalid-file",
        crate::desktop_config::ConfigPublishError::InvalidNiri => "invalid-niri",
        crate::desktop_config::ConfigPublishError::InvalidWaybar => "invalid-waybar",
        crate::desktop_config::ConfigPublishError::UnsupportedBarPosition => {
            "unsupported-bar-position"
        }
        crate::desktop_config::ConfigPublishError::InvalidWaybarStyle => "invalid-waybar-style",
        crate::desktop_config::ConfigPublishError::InvalidSwww => "invalid-swww",
    }
}

struct Ext4Mount {
    superblock: Superblock,
    group0: GroupDescriptor,
    cache: BlockCache,
    recovery: Option<JournalRecovery>,
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
        let (superblock, recovery_required) = match Superblock::parse(device.data(SUPERBLOCK_SIZE))
        {
            Ok(superblock) => (superblock, false),
            Err(ParseError::DirtyFilesystem) => {
                let superblock = Superblock::parse_for_recovery(device.data(SUPERBLOCK_SIZE))
                    .unwrap_or_else(|_| device.fail("ext4 recovery superblock is invalid"));
                if superblock.feature_incompat & FEATURE_INCOMPAT_RECOVER == 0 {
                    device.fail("ext4 dirty state has no replayable journal");
                }
                (superblock, true)
            }
            Err(_) => device.fail("ext4 superblock validation failed"),
        };
        if superblock.block_size as usize != BLOCK_SIZE {
            device.fail("ext4 mount requires a 4096-byte block size");
        }
        let mut cache = BlockCache::new();
        let group0 = read_group_descriptor(device, &superblock, 0, &mut cache).await;
        let mut mount = Self {
            superblock,
            group0,
            cache,
            recovery: None,
        };
        if recovery_required {
            let journal = mount.probe_journal(device).await;
            mount.recovery = Some(mount.replay_journal_transaction(device, &journal).await);
        }
        mount
    }

    async fn replay_journal_transaction(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
    ) -> JournalRecovery {
        let start = journal.superblock.start;
        if start == 0 {
            let filesystem_sector = block_to_sector(device, &self.superblock, 0);
            let mut scratch = JournalStateScratch::new();
            device.read(filesystem_sector, BLOCK_SIZE).await;
            scratch
                .filesystem_block_mut()
                .copy_from_slice(device.data(BLOCK_SIZE));
            set_superblock_recovery(
                &mut scratch.filesystem_block_mut()
                    [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
                false,
            )
            .unwrap_or_else(|_| device.fail("inactive JBD2 recovery cleanup failed"));
            device
                .write(filesystem_sector, scratch.filesystem_block())
                .await;
            device.flush().await;
            device.read(filesystem_sector, BLOCK_SIZE).await;
            let clean_superblock = Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            )
            .unwrap_or_else(|_| device.fail("inactive JBD2 recovery readback failed"));
            if device.data(BLOCK_SIZE) != scratch.filesystem_block() {
                device.fail("inactive JBD2 recovery cleanup mismatch");
            }
            self.superblock = clean_superblock;
            return JournalRecovery {
                replayed: false,
                sequence: journal.superblock.sequence,
                start,
                tag_count: 0,
                target_block: 0,
                escaped: false,
                next_sequence: journal.superblock.sequence,
            };
        }
        let descriptor_block = journal
            .physical_block
            .checked_add(u64::from(start))
            .unwrap_or_else(|| device.fail("JBD2 recovery descriptor block overflow"));
        let journal_end = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.max_length))
            .unwrap_or_else(|| device.fail("JBD2 recovery extent overflow"));

        let filesystem_sector = block_to_sector(device, &self.superblock, 0);
        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let mut scratch = JournalReplayScratch::new();
        device.read(filesystem_sector, BLOCK_SIZE).await;
        scratch
            .state
            .filesystem_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        if Superblock::parse(
            &scratch.state.filesystem_block()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
        ) != Err(ParseError::DirtyFilesystem)
        {
            device.fail("JBD2 recovery requires the ext4 recovery flag");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        scratch
            .state
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        if JournalSuperblock::parse(scratch.state.journal_block()) != Ok(journal.superblock) {
            device.fail("JBD2 recovery superblock changed during mount");
        }

        device
            .read(
                block_to_sector(device, &self.superblock, descriptor_block),
                BLOCK_SIZE,
            )
            .await;
        scratch_frame_bytes_mut(scratch.descriptor_frame).copy_from_slice(device.data(BLOCK_SIZE));
        let descriptor =
            JournalDescriptorBlock::parse(scratch_frame_bytes(scratch.descriptor_frame))
                .unwrap_or_else(|_| device.fail("JBD2 recovery descriptor validation failed"));
        if descriptor.sequence != journal.superblock.sequence
            || descriptor.uuid != self.superblock.uuid
            || descriptor.tag_count == 0
            || descriptor.tag_count > MULTI_TRANSACTION_MAX_BLOCKS
        {
            device.fail("JBD2 recovery descriptor identity mismatch");
        }
        let tag_count = descriptor.tag_count;
        let commit_block = descriptor_block
            .checked_add(1 + u64::try_from(tag_count).unwrap_or(u64::MAX))
            .unwrap_or_else(|| device.fail("JBD2 recovery commit block overflow"));
        if commit_block >= journal_end {
            device.fail("wrapped JBD2 recovery transaction is unsupported");
        }
        for index in 0..tag_count {
            let data_block = descriptor_block
                .checked_add(1 + u64::try_from(index).unwrap_or(u64::MAX))
                .unwrap_or_else(|| device.fail("JBD2 recovery data block overflow"));
            device
                .read(
                    block_to_sector(device, &self.superblock, data_block),
                    BLOCK_SIZE,
                )
                .await;
            scratch
                .data_block_mut(index)
                .copy_from_slice(device.data(BLOCK_SIZE));
        }
        device
            .read(
                block_to_sector(device, &self.superblock, commit_block),
                BLOCK_SIZE,
            )
            .await;
        scratch_frame_bytes_mut(scratch.commit_frame).copy_from_slice(device.data(BLOCK_SIZE));
        let commit = JournalCommit::parse(scratch_frame_bytes(scratch.commit_frame))
            .unwrap_or_else(|_| device.fail("JBD2 recovery commit validation failed"));
        if commit.sequence != journal.superblock.sequence {
            device.fail("JBD2 recovery commit sequence mismatch");
        }

        let mut target_blocks = [0u64; MULTI_TRANSACTION_MAX_BLOCKS];
        let mut escaped_tags = [false; MULTI_TRANSACTION_MAX_BLOCKS];
        for index in 0..tag_count {
            let tag = descriptor
                .tag(index)
                .unwrap_or_else(|_| device.fail("JBD2 recovery tag validation failed"));
            let target_block = u64::from(tag.target_block);
            if target_block >= self.superblock.block_count
                || (target_block >= journal.physical_block && target_block < journal_end)
                || target_blocks[..index].contains(&target_block)
            {
                device.fail("JBD2 recovery target block is unsafe");
            }
            decode_journal_tag_data(
                scratch_frame_bytes_mut(scratch.home_frames[index]),
                scratch_frame_bytes(scratch.data_frames[index]),
                &tag,
            )
            .unwrap_or_else(|_| device.fail("JBD2 recovery data decoding failed"));
            target_blocks[index] = target_block;
            escaped_tags[index] = tag.escaped;
        }
        if let Some(index) = target_blocks[..tag_count]
            .iter()
            .position(|target| *target == 0)
        {
            let superblock_bytes =
                &scratch.home_block(index)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE];
            if Superblock::parse(superblock_bytes) != Err(ParseError::DirtyFilesystem)
                || Superblock::parse_for_recovery(superblock_bytes).is_err()
            {
                device.fail("JBD2 recovery superblock home is not active");
            }
        }

        for (index, target_block) in target_blocks[..tag_count].iter().enumerate() {
            device
                .write(
                    block_to_sector(device, &self.superblock, *target_block),
                    scratch.home_block(index),
                )
                .await;
        }
        device.flush().await;
        for (index, target_block) in target_blocks[..tag_count].iter().enumerate() {
            device
                .read(
                    block_to_sector(device, &self.superblock, *target_block),
                    BLOCK_SIZE,
                )
                .await;
            if device.data(BLOCK_SIZE) != scratch.home_block(index) {
                device.fail("JBD2 recovery home-block readback mismatch");
            }
            self.cache.invalidate(*target_block);
        }

        let next_sequence = journal
            .superblock
            .sequence
            .checked_add(1)
            .unwrap_or_else(|| device.fail("JBD2 recovery sequence overflow"));
        set_journal_superblock_state(scratch.state.journal_block_mut(), next_sequence, 0)
            .unwrap_or_else(|_| device.fail("JBD2 recovery checkpoint update failed"));
        for offset in 0..tag_count + 2 {
            let block = descriptor_block
                .checked_add(u64::try_from(offset).unwrap_or(u64::MAX))
                .unwrap_or_else(|| device.fail("JBD2 recovery clear block overflow"));
            device
                .write(
                    block_to_sector(device, &self.superblock, block),
                    &ZERO_BLOCK,
                )
                .await;
        }
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;
        self.cache.invalidate(journal.physical_block);

        let superblock_home_index = target_blocks[..tag_count]
            .iter()
            .position(|target| *target == 0);
        let final_filesystem_block = if let Some(index) = superblock_home_index {
            scratch.home_block_mut(index)
        } else {
            scratch.state.filesystem_block_mut()
        };
        set_superblock_recovery(
            &mut final_filesystem_block[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            false,
        )
        .unwrap_or_else(|_| device.fail("ext4 recovery flag clearing failed"));
        device
            .write(filesystem_sector, final_filesystem_block)
            .await;
        device.flush().await;

        device.read(journal_sector, BLOCK_SIZE).await;
        let checkpointed = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("JBD2 recovery checkpoint readback failed"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || checkpointed.sequence != next_sequence
            || checkpointed.start != 0
        {
            device.fail("JBD2 recovery checkpoint state mismatch");
        }
        device.read(filesystem_sector, BLOCK_SIZE).await;
        let clean_superblock = Superblock::parse(
            &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
        )
        .unwrap_or_else(|_| device.fail("ext4 recovery cleanup validation failed"));
        let expected_filesystem_block = if let Some(index) = superblock_home_index {
            scratch.home_block(index)
        } else {
            scratch.state.filesystem_block()
        };
        if device.data(BLOCK_SIZE) != expected_filesystem_block {
            device.fail("ext4 recovery cleanup readback mismatch");
        }
        self.superblock = clean_superblock;

        JournalRecovery {
            replayed: true,
            sequence: journal.superblock.sequence,
            start,
            tag_count,
            target_block: u32::try_from(target_blocks[0])
                .unwrap_or_else(|_| device.fail("JBD2 recovery target conversion failed")),
            escaped: escaped_tags[0],
            next_sequence,
        }
    }

    #[cfg(feature = "journal-replay-injection")]
    async fn inject_committed_allocation_transaction(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        file: &Ext4File,
    ) -> AllocationLayout {
        if journal.superblock.start != 0 {
            device.fail("allocation replay injection requires a clean journal");
        }
        let (blocks, layout) = self
            .prepare_block_allocation_transaction(device, file, &ALLOCATION_PROBE_BLOCK_DATA)
            .await;
        let targets = layout.targets;
        let descriptor_block = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.first_log_block))
            .unwrap_or_else(|| device.fail("allocation replay descriptor overflow"));
        let commit_block = descriptor_block
            .checked_add(1 + ALLOCATION_TRANSACTION_BLOCKS as u64)
            .unwrap_or_else(|| device.fail("allocation replay commit overflow"));
        let journal_end = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.max_length))
            .unwrap_or_else(|| device.fail("allocation replay journal extent overflow"));
        if commit_block >= journal_end {
            device.fail("allocation replay injection exceeds journal extent");
        }
        for (index, target) in targets.iter().enumerate() {
            if *target >= self.superblock.block_count
                || (*target >= journal.physical_block && *target < journal_end)
                || targets[..index].contains(target)
            {
                device.fail("allocation replay target is unsafe");
            }
        }

        let clean_superblock = self.superblock;
        let filesystem_sector = block_to_sector(device, &clean_superblock, 0);
        let journal_sector = block_to_sector(device, &clean_superblock, journal.physical_block);
        let mut scratch = MultiJournalScratch::<ALLOCATION_TRANSACTION_BLOCKS>::new();
        device.read(journal_sector, BLOCK_SIZE).await;
        scratch
            .state
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        if JournalSuperblock::parse(scratch.state.journal_block()) != Ok(journal.superblock) {
            device.fail("allocation replay journal superblock changed");
        }
        for offset in 0..ALLOCATION_TRANSACTION_BLOCKS + 2 {
            let block = descriptor_block
                .checked_add(offset as u64)
                .unwrap_or_else(|| device.fail("allocation replay record overflow"));
            device
                .read(
                    block_to_sector(device, &clean_superblock, block),
                    BLOCK_SIZE,
                )
                .await;
            if device.data(BLOCK_SIZE).iter().any(|byte| *byte != 0) {
                device.fail("allocation replay record block is not empty");
            }
        }

        for (index, target) in targets.iter().enumerate() {
            device
                .write(
                    block_to_sector(device, &clean_superblock, *target),
                    blocks.modified.block(index),
                )
                .await;
        }
        device.flush().await;
        for (index, target) in targets.iter().enumerate() {
            device
                .read(
                    block_to_sector(device, &clean_superblock, *target),
                    BLOCK_SIZE,
                )
                .await;
            if device.data(BLOCK_SIZE) != blocks.modified.block(index) {
                device.fail("allocation replay old home readback mismatch");
            }
        }

        self.superblock = Superblock::parse(
            &blocks.modified.block(0)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
        )
        .unwrap_or_else(|_| device.fail("allocation replay old superblock is invalid"));
        scratch
            .state
            .filesystem_block_mut()
            .copy_from_slice(blocks.modified.block(0));
        set_superblock_recovery(
            &mut scratch.state.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            true,
        )
        .unwrap_or_else(|_| device.fail("allocation replay recovery update failed"));
        scratch
            .desired_superblock_mut()
            .copy_from_slice(blocks.original.block(0));
        set_superblock_recovery(
            &mut scratch.desired_superblock_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            true,
        )
        .unwrap_or_else(|_| device.fail("allocation replay desired superblock update failed"));

        let mut target_blocks_u32 = [0u32; ALLOCATION_TRANSACTION_BLOCKS];
        let mut escaped_tags = [false; ALLOCATION_TRANSACTION_BLOCKS];
        for index in 0..ALLOCATION_TRANSACTION_BLOCKS {
            target_blocks_u32[index] = u32::try_from(targets[index])
                .unwrap_or_else(|_| device.fail("allocation replay target exceeds 32 bits"));
            let desired_frame = if targets[index] == 0 {
                scratch.desired_superblock_frame
            } else {
                blocks.original.frames[index]
            };
            escaped_tags[index] = encode_journal_data_block(
                scratch_frame_bytes_mut(scratch.data_frames[index]),
                scratch_frame_bytes(desired_frame),
            )
            .unwrap_or_else(|_| device.fail("allocation replay data encoding failed"));
        }
        encode_journal_descriptor_block(
            scratch.descriptor_mut(),
            journal.superblock.sequence,
            &target_blocks_u32,
            &escaped_tags,
            &self.superblock.uuid,
        )
        .unwrap_or_else(|_| device.fail("allocation replay descriptor encoding failed"));
        encode_journal_commit_block(scratch.commit_mut(), journal.superblock.sequence)
            .unwrap_or_else(|_| device.fail("allocation replay commit encoding failed"));

        device
            .write(filesystem_sector, scratch.state.filesystem_block())
            .await;
        device.flush().await;
        set_journal_superblock_state(
            scratch.state.journal_block_mut(),
            journal.superblock.sequence,
            journal.superblock.first_log_block,
        )
        .unwrap_or_else(|_| device.fail("allocation replay state update failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, descriptor_block),
                scratch.descriptor(),
            )
            .await;
        for index in 0..ALLOCATION_TRANSACTION_BLOCKS {
            let data_block = descriptor_block
                .checked_add(1 + index as u64)
                .unwrap_or_else(|| device.fail("allocation replay data block overflow"));
            device
                .write(
                    block_to_sector(device, &self.superblock, data_block),
                    scratch.data(index),
                )
                .await;
        }
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, commit_block),
                scratch.commit(),
            )
            .await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.state.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            ) != Err(ParseError::DirtyFilesystem)
        {
            device.fail("allocation replay dirty state readback mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let active = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("allocation replay state readback failed"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || active.sequence != journal.superblock.sequence
            || active.start != journal.superblock.first_log_block
        {
            device.fail("allocation replay active state mismatch");
        }
        for offset in 0..ALLOCATION_TRANSACTION_BLOCKS + 2 {
            let block = descriptor_block
                .checked_add(offset as u64)
                .unwrap_or_else(|| device.fail("allocation replay readback overflow"));
            let expected = if offset == 0 {
                scratch.descriptor()
            } else if offset == ALLOCATION_TRANSACTION_BLOCKS + 1 {
                scratch.commit()
            } else {
                scratch.data(offset - 1)
            };
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE) != expected {
                device.fail("allocation replay record readback mismatch");
            }
        }
        for (index, target) in targets.iter().enumerate() {
            let expected = if *target == 0 {
                scratch.state.filesystem_block()
            } else {
                blocks.modified.block(index)
            };
            device
                .read(
                    block_to_sector(device, &self.superblock, *target),
                    BLOCK_SIZE,
                )
                .await;
            if device.data(BLOCK_SIZE) != expected {
                device.fail("allocation replay active old home changed");
            }
        }
        layout
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
            || (self.superblock.feature_incompat & FEATURE_INCOMPAT_RECOVER == 0
                && superblock.start != 0)
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

    async fn stage_inactive_journal_records(
        &self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        target_block: u64,
    ) -> JournalRecordProbe {
        let sequence = journal.superblock.sequence;
        let descriptor_block = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.first_log_block))
            .unwrap_or_else(|| device.fail("JBD2 descriptor block overflow"));
        let data_block = descriptor_block
            .checked_add(1)
            .unwrap_or_else(|| device.fail("JBD2 data block overflow"));
        let commit_block = descriptor_block
            .checked_add(2)
            .unwrap_or_else(|| device.fail("JBD2 commit block overflow"));
        let journal_end = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.max_length))
            .unwrap_or_else(|| device.fail("JBD2 extent end overflow"));
        if commit_block >= journal_end {
            device.fail("JBD2 probe transaction exceeds journal extent");
        }
        for block in [descriptor_block, data_block, commit_block] {
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE).iter().any(|byte| *byte != 0) {
                device.fail("JBD2 probe transaction block is not empty");
            }
        }

        let mut scratch = JournalScratch::new();
        let target_block = u32::try_from(target_block)
            .unwrap_or_else(|_| device.fail("JBD2 target block exceeds 32 bits"));
        {
            let (descriptor, data, commit) = scratch.buffers();
            encode_single_block_journal_transaction(
                descriptor,
                data,
                commit,
                sequence,
                target_block,
                &self.superblock.uuid,
                &JOURNAL_PROBE_BLOCK,
            )
            .unwrap_or_else(|_| device.fail("JBD2 transaction encoding failed"));
        }
        let descriptor = JournalDescriptor::parse(scratch.descriptor())
            .unwrap_or_else(|_| device.fail("JBD2 descriptor self-validation failed"));
        let commit = JournalCommit::parse(scratch.commit())
            .unwrap_or_else(|_| device.fail("JBD2 commit self-validation failed"));
        if descriptor.sequence != sequence
            || descriptor.target_block != target_block
            || descriptor.uuid != self.superblock.uuid
            || commit.sequence != sequence
        {
            device.fail("JBD2 transaction identity mismatch");
        }

        device
            .write(
                block_to_sector(device, &self.superblock, descriptor_block),
                scratch.descriptor(),
            )
            .await;
        device
            .write(
                block_to_sector(device, &self.superblock, data_block),
                scratch.data(),
            )
            .await;
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, commit_block),
                scratch.commit(),
            )
            .await;
        device.flush().await;

        for (block, expected) in [
            (descriptor_block, scratch.descriptor()),
            (data_block, scratch.data()),
            (commit_block, scratch.commit()),
        ] {
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE) != expected {
                device.fail("JBD2 staged record readback mismatch");
            }
        }

        for block in [descriptor_block, data_block, commit_block] {
            device
                .write(
                    block_to_sector(device, &self.superblock, block),
                    &ZERO_BLOCK,
                )
                .await;
        }
        device.flush().await;
        JournalRecordProbe {
            sequence,
            target_block,
            descriptor_block,
            data_block,
            commit_block,
        }
    }

    async fn probe_journal_state_transition(
        &self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
    ) -> JournalStateProbe {
        let filesystem_sector = block_to_sector(device, &self.superblock, 0);
        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let mut scratch = JournalStateScratch::new();

        device.read(filesystem_sector, BLOCK_SIZE).await;
        scratch
            .filesystem_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        device.read(journal_sector, BLOCK_SIZE).await;
        scratch
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));

        set_superblock_recovery(
            &mut scratch.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            true,
        )
        .unwrap_or_else(|_| device.fail("ext4 recovery flag activation failed"));
        device
            .write(filesystem_sector, scratch.filesystem_block())
            .await;
        device.flush().await;

        set_journal_superblock_state(
            scratch.journal_block_mut(),
            journal.superblock.sequence,
            journal.superblock.first_log_block,
        )
        .unwrap_or_else(|_| device.fail("JBD2 state activation failed"));
        device.write(journal_sector, scratch.journal_block()).await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            ) != Err(ParseError::DirtyFilesystem)
        {
            device.fail("ext4 recovery flag readback mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let active = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("JBD2 active state readback failed"));
        if device.data(BLOCK_SIZE) != scratch.journal_block()
            || active.sequence != journal.superblock.sequence
            || active.start != journal.superblock.first_log_block
        {
            device.fail("JBD2 active state readback mismatch");
        }

        set_journal_superblock_state(scratch.journal_block_mut(), journal.superblock.sequence, 0)
            .unwrap_or_else(|_| device.fail("JBD2 checkpoint state update failed"));
        device.write(journal_sector, scratch.journal_block()).await;
        device.flush().await;
        set_superblock_recovery(
            &mut scratch.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            false,
        )
        .unwrap_or_else(|_| device.fail("ext4 recovery flag clearing failed"));
        device
            .write(filesystem_sector, scratch.filesystem_block())
            .await;
        device.flush().await;

        device.read(journal_sector, BLOCK_SIZE).await;
        let restored_journal = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("JBD2 restored state validation failed"));
        if device.data(BLOCK_SIZE) != scratch.journal_block() || restored_journal.start != 0 {
            device.fail("JBD2 restored state readback mismatch");
        }
        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            )
            .is_err()
        {
            device.fail("ext4 restored superblock readback mismatch");
        }
        JournalStateProbe {
            sequence: active.sequence,
            active_start: active.start,
        }
    }

    async fn probe_active_journal_transaction(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        file: &Ext4File,
        target_block: u64,
    ) -> ActiveJournalProbe {
        let filesystem_sector = block_to_sector(device, &self.superblock, 0);
        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let descriptor_block = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.first_log_block))
            .unwrap_or_else(|| device.fail("active JBD2 descriptor block overflow"));
        let data_block = descriptor_block
            .checked_add(1)
            .unwrap_or_else(|| device.fail("active JBD2 data block overflow"));
        let commit_block = descriptor_block
            .checked_add(2)
            .unwrap_or_else(|| device.fail("active JBD2 commit block overflow"));
        let journal_end = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.max_length))
            .unwrap_or_else(|| device.fail("active JBD2 extent end overflow"));
        if commit_block >= journal_end {
            device.fail("active JBD2 transaction exceeds journal extent");
        }

        let mut scratch = ActiveJournalScratch::new();
        device.read(filesystem_sector, BLOCK_SIZE).await;
        scratch
            .state
            .filesystem_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        device.read(journal_sector, BLOCK_SIZE).await;
        scratch
            .state
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        for block in [descriptor_block, data_block, commit_block] {
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE).iter().any(|byte| *byte != 0) {
                device.fail("active JBD2 record block is not empty");
            }
        }

        let sequence = journal.superblock.sequence;
        let target_block = u32::try_from(target_block)
            .unwrap_or_else(|_| device.fail("active JBD2 target block exceeds 32 bits"));
        {
            let (descriptor, data, commit) = scratch.records.buffers();
            encode_single_block_journal_transaction(
                descriptor,
                data,
                commit,
                sequence,
                target_block,
                &self.superblock.uuid,
                &JOURNAL_PROBE_BLOCK,
            )
            .unwrap_or_else(|_| device.fail("active JBD2 transaction encoding failed"));
        }

        set_superblock_recovery(
            &mut scratch.state.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            true,
        )
        .unwrap_or_else(|_| device.fail("active ext4 recovery flag activation failed"));
        device
            .write(filesystem_sector, scratch.state.filesystem_block())
            .await;
        device.flush().await;

        set_journal_superblock_state(
            scratch.state.journal_block_mut(),
            sequence,
            journal.superblock.first_log_block,
        )
        .unwrap_or_else(|_| device.fail("active JBD2 state activation failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;

        device
            .write(
                block_to_sector(device, &self.superblock, descriptor_block),
                scratch.records.descriptor(),
            )
            .await;
        device
            .write(
                block_to_sector(device, &self.superblock, data_block),
                scratch.records.data(),
            )
            .await;
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, commit_block),
                scratch.records.commit(),
            )
            .await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.state.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            ) != Err(ParseError::DirtyFilesystem)
        {
            device.fail("active ext4 recovery state readback mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let active_state = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("active JBD2 state readback failed"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || active_state.sequence != sequence
            || active_state.start != journal.superblock.first_log_block
        {
            device.fail("active JBD2 state readback mismatch");
        }

        device
            .read(
                block_to_sector(device, &self.superblock, descriptor_block),
                BLOCK_SIZE,
            )
            .await;
        let descriptor = JournalDescriptor::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("active JBD2 descriptor readback failed"));
        if device.data(BLOCK_SIZE) != scratch.records.descriptor()
            || descriptor.sequence != sequence
            || descriptor.target_block != target_block
            || descriptor.uuid != self.superblock.uuid
        {
            device.fail("active JBD2 descriptor identity mismatch");
        }
        device
            .read(
                block_to_sector(device, &self.superblock, data_block),
                BLOCK_SIZE,
            )
            .await;
        if device.data(BLOCK_SIZE) != scratch.records.data() {
            device.fail("active JBD2 data readback mismatch");
        }
        device
            .read(
                block_to_sector(device, &self.superblock, commit_block),
                BLOCK_SIZE,
            )
            .await;
        let commit = JournalCommit::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("active JBD2 commit readback failed"));
        if device.data(BLOCK_SIZE) != scratch.records.commit() || commit.sequence != sequence {
            device.fail("active JBD2 commit identity mismatch");
        }

        device
            .write(
                block_to_sector(device, &self.superblock, u64::from(target_block)),
                scratch.records.data(),
            )
            .await;
        device.flush().await;
        self.cache.invalidate(u64::from(target_block));
        if self
            .read_file_block(device, file, 0)
            .await
            .iter()
            .any(|byte| *byte != b'J')
        {
            device.fail("active JBD2 home-block checkpoint readback mismatch");
        }

        let next_sequence = sequence
            .checked_add(1)
            .unwrap_or_else(|| device.fail("active JBD2 sequence overflow"));
        set_journal_superblock_state(scratch.state.journal_block_mut(), next_sequence, 0)
            .unwrap_or_else(|_| device.fail("active JBD2 checkpoint update failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;
        set_superblock_recovery(
            &mut scratch.state.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            false,
        )
        .unwrap_or_else(|_| device.fail("active ext4 recovery flag clearing failed"));
        device
            .write(filesystem_sector, scratch.state.filesystem_block())
            .await;
        device.flush().await;

        for block in [descriptor_block, data_block, commit_block] {
            device
                .write(
                    block_to_sector(device, &self.superblock, block),
                    &ZERO_BLOCK,
                )
                .await;
        }
        device.flush().await;

        let restored_block = self
            .overwrite_existing_file_block(device, file, 0, &WRITE_PROBE_ORIGINAL_BLOCK)
            .await;
        if restored_block != u64::from(target_block)
            || self
                .read_file_block(device, file, 0)
                .await
                .iter()
                .any(|byte| *byte != b'P')
        {
            device.fail("active JBD2 target restoration failed");
        }

        set_journal_superblock_state(scratch.state.journal_block_mut(), sequence, 0)
            .unwrap_or_else(|_| device.fail("active JBD2 test sequence rewind failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.state.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            )
            .is_err()
        {
            device.fail("active ext4 final state mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let restored_state = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("active JBD2 final state validation failed"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || restored_state.sequence != sequence
            || restored_state.start != 0
        {
            device.fail("active JBD2 final state mismatch");
        }

        ActiveJournalProbe {
            sequence,
            target_block,
            next_sequence,
        }
    }

    async fn probe_inode_metadata_transactions(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        file: &Ext4File,
    ) -> MetadataJournalProbe {
        let group_index = self
            .superblock
            .inode_group(file.inode.number)
            .unwrap_or_else(|_| device.fail("metadata probe inode number is invalid"));
        let group = if group_index == 0 {
            self.group0
        } else {
            read_group_descriptor(device, &self.superblock, group_index, &mut self.cache).await
        };
        let location = self
            .superblock
            .inode_location(file.inode.number, &group)
            .unwrap_or_else(|_| device.fail("metadata probe inode location is invalid"));
        let inode_offset = location.offset as usize;
        let inode_end = inode_offset + usize::from(self.superblock.inode_size);
        if inode_end > BLOCK_SIZE {
            device.fail("metadata probe inode crosses its table block");
        }

        let mut metadata = MetadataJournalScratch::new();
        device
            .read(
                block_to_sector(device, &self.superblock, location.block),
                BLOCK_SIZE,
            )
            .await;
        metadata
            .original_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        metadata
            .modified_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        set_inode_size(
            &mut metadata.modified_block_mut()[inode_offset..inode_end],
            file.inode.number,
            &self.superblock,
            file.inode.size - 1,
        )
        .unwrap_or_else(|_| device.fail("metadata probe inode encoding failed"));
        let modified_inode = Inode::parse(
            &metadata.modified_block()[inode_offset..inode_end],
            file.inode.number,
            &self.superblock,
        )
        .unwrap_or_else(|_| device.fail("metadata probe inode self-validation failed"));
        if modified_inode.size != file.inode.size - 1 {
            device.fail("metadata probe inode size mismatch");
        }

        let first_sequence = journal.superblock.sequence;
        let second_sequence = self
            .commit_single_block_journal_transaction(
                device,
                journal,
                first_sequence,
                location.block,
                metadata.modified_block(),
            )
            .await;
        let updated_inode = self.read_inode(device, file.inode.number).await;
        if updated_inode.size != file.inode.size - 1 {
            device.fail("metadata journal checkpoint did not update inode size");
        }

        let final_sequence = self
            .commit_single_block_journal_transaction(
                device,
                journal,
                second_sequence,
                location.block,
                metadata.original_block(),
            )
            .await;
        let restored_inode = self.read_inode(device, file.inode.number).await;
        if restored_inode != file.inode {
            device.fail("metadata journal restoration did not restore inode");
        }

        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let mut rewind = JournalStateScratch::new();
        device.read(journal_sector, BLOCK_SIZE).await;
        rewind
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        let checkpointed = JournalSuperblock::parse(rewind.journal_block())
            .unwrap_or_else(|_| device.fail("metadata journal checkpoint state is invalid"));
        if checkpointed.sequence != final_sequence || checkpointed.start != 0 {
            device.fail("metadata journal final sequence mismatch");
        }
        set_journal_superblock_state(rewind.journal_block_mut(), first_sequence, 0)
            .unwrap_or_else(|_| device.fail("metadata journal sequence rewind failed"));
        device.write(journal_sector, rewind.journal_block()).await;
        device.flush().await;
        device.read(journal_sector, BLOCK_SIZE).await;
        let restored_journal = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("metadata journal rewind validation failed"));
        if device.data(BLOCK_SIZE) != rewind.journal_block()
            || restored_journal.sequence != first_sequence
            || restored_journal.start != 0
        {
            device.fail("metadata journal rewind readback mismatch");
        }

        device
            .read(
                block_to_sector(device, &self.superblock, location.block),
                BLOCK_SIZE,
            )
            .await;
        if device.data(BLOCK_SIZE) != metadata.original_block() {
            device.fail("metadata inode table block was not restored");
        }

        MetadataJournalProbe {
            target_block: location.block,
            first_sequence,
            second_sequence,
            final_sequence,
        }
    }

    async fn probe_block_allocation_transactions(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        file: &Ext4File,
        descriptors: &mut FileDescriptorTable<8>,
        fd: u32,
    ) -> AllocationJournalProbe {
        descriptors
            .seek(fd, file.inode.size)
            .unwrap_or_else(|_| device.fail("allocation probe append seek failed"));
        let append_window = descriptors
            .append_window(fd, ALLOCATION_PROBE_BLOCK_DATA.len())
            .unwrap_or_else(|_| device.fail("allocation probe append window failed"));
        if append_window.node.filesystem_id != ROOT_FILESYSTEM_ID
            || append_window.node.node_id != u64::from(file.inode.number)
            || append_window.offset != file.inode.size
            || append_window.length != BLOCK_SIZE
        {
            device.fail("allocation probe append descriptor mismatch");
        }
        let (blocks, layout) = self
            .prepare_block_allocation_transaction(device, file, &ALLOCATION_PROBE_BLOCK_DATA)
            .await;
        let targets = layout.targets;
        let group_descriptor_offset = layout.group_descriptor_offset;
        let group_descriptor_end = layout.group_descriptor_end;

        let first_sequence = journal.superblock.sequence;
        let second_sequence = self
            .commit_multi_block_journal_transaction(
                device,
                journal,
                first_sequence,
                &targets,
                &blocks.modified,
            )
            .await;
        self.group0 = GroupDescriptor::parse(
            &blocks.modified.block(1)[group_descriptor_offset..group_descriptor_end],
            0,
            &self.superblock,
        )
        .unwrap_or_else(|_| device.fail("allocation probe updated group descriptor is invalid"));
        let grown_inode = self.read_inode(device, file.inode.number).await;
        if grown_inode.size != file.inode.size + u64::from(self.superblock.block_size)
            || grown_inode.block_count_512 != file.inode.block_count_512 + 8
        {
            device.fail("allocation probe grown inode readback mismatch");
        }
        let grown_file = Ext4File {
            inode: grown_inode,
            parent_inode: file.parent_inode,
            directory_block: file.directory_block,
            followed_symlink: file.followed_symlink,
        };
        descriptors
            .set_size(fd, grown_file.inode.size)
            .unwrap_or_else(|_| device.fail("allocation probe descriptor growth failed"));
        descriptors
            .advance(fd, append_window.length)
            .unwrap_or_else(|_| device.fail("allocation probe append advance failed"));
        descriptors
            .seek(fd, file.inode.size)
            .unwrap_or_else(|_| device.fail("allocation probe readback seek failed"));
        let mut appended_data = [0u8; BLOCK_SIZE];
        if read_descriptor(
            self,
            device,
            descriptors,
            fd,
            &grown_file,
            &mut appended_data,
        )
        .await
            != BLOCK_SIZE
            || appended_data != ALLOCATION_PROBE_BLOCK_DATA
        {
            device.fail("allocation probe fd append readback mismatch");
        }

        descriptors
            .seek(fd, file.inode.size)
            .unwrap_or_else(|_| device.fail("allocation probe truncate seek failed"));
        let final_sequence = self
            .commit_multi_block_journal_transaction(
                device,
                journal,
                second_sequence,
                &targets,
                &blocks.original,
            )
            .await;
        descriptors
            .set_size(fd, file.inode.size)
            .unwrap_or_else(|_| device.fail("allocation probe descriptor truncation failed"));
        self.group0 = GroupDescriptor::parse(
            &blocks.original.block(1)[group_descriptor_offset..group_descriptor_end],
            0,
            &self.superblock,
        )
        .unwrap_or_else(|_| device.fail("allocation probe restored group descriptor is invalid"));
        let restored_inode = self.read_inode(device, file.inode.number).await;
        if restored_inode != file.inode {
            device.fail("allocation probe inode restoration failed");
        }
        device
            .read(
                block_to_sector(device, &self.superblock, ALLOCATION_PROBE_BLOCK),
                BLOCK_SIZE,
            )
            .await;
        if device.data(BLOCK_SIZE) != blocks.original.block(4) {
            device.fail("allocation probe data block restoration failed");
        }

        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let mut rewind = JournalStateScratch::new();
        device.read(journal_sector, BLOCK_SIZE).await;
        rewind
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        let checkpointed = JournalSuperblock::parse(rewind.journal_block())
            .unwrap_or_else(|_| device.fail("allocation probe journal state is invalid"));
        if checkpointed.sequence != final_sequence || checkpointed.start != 0 {
            device.fail("allocation probe journal final sequence mismatch");
        }
        set_journal_superblock_state(rewind.journal_block_mut(), first_sequence, 0)
            .unwrap_or_else(|_| device.fail("allocation probe journal rewind failed"));
        device.write(journal_sector, rewind.journal_block()).await;
        device.flush().await;
        device.read(journal_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != rewind.journal_block() {
            device.fail("allocation probe journal rewind readback mismatch");
        }

        AllocationJournalProbe {
            allocated_block: ALLOCATION_PROBE_BLOCK,
            bitmap_block: targets[2],
            group_descriptor_block: targets[1],
            inode_table_block: targets[3],
            first_sequence,
            second_sequence,
            final_sequence,
        }
    }

    async fn prepare_block_allocation_transaction(
        &mut self,
        device: &mut BlockDevice,
        file: &Ext4File,
        new_data: &[u8],
    ) -> (AllocationJournalScratch, AllocationLayout) {
        if new_data.len() != BLOCK_SIZE {
            device.fail("allocation probe data must fill one block");
        }
        let group_index = self
            .superblock
            .inode_group(file.inode.number)
            .unwrap_or_else(|_| device.fail("allocation probe inode number is invalid"));
        let inode_group = if group_index == 0 {
            self.group0
        } else {
            read_group_descriptor(device, &self.superblock, group_index, &mut self.cache).await
        };
        let inode_location = self
            .superblock
            .inode_location(file.inode.number, &inode_group)
            .unwrap_or_else(|_| device.fail("allocation probe inode location is invalid"));
        let inode_offset = inode_location.offset as usize;
        let inode_end = inode_offset + usize::from(self.superblock.inode_size);
        if inode_end > BLOCK_SIZE {
            device.fail("allocation probe inode crosses its table block");
        }
        let group_descriptor_location = self
            .superblock
            .group_descriptor_location(0)
            .unwrap_or_else(|_| device.fail("allocation probe group descriptor is invalid"));
        let group_descriptor_offset = group_descriptor_location.offset as usize;
        let group_descriptor_end =
            group_descriptor_offset + usize::from(self.superblock.descriptor_size);
        if group_descriptor_end > BLOCK_SIZE {
            device.fail("allocation probe group descriptor crosses its block");
        }
        let targets = [
            0,
            group_descriptor_location.block,
            self.group0.block_bitmap_block,
            inode_location.block,
            ALLOCATION_PROBE_BLOCK,
        ];
        for (index, target) in targets.iter().enumerate() {
            if targets[..index].contains(target) {
                device.fail("allocation probe home blocks overlap");
            }
        }

        let mut blocks = AllocationJournalScratch::new();
        for (index, target) in targets.iter().enumerate() {
            device
                .read(
                    block_to_sector(device, &self.superblock, *target),
                    BLOCK_SIZE,
                )
                .await;
            blocks
                .original
                .block_mut(index)
                .copy_from_slice(device.data(BLOCK_SIZE));
            blocks
                .modified
                .block_mut(index)
                .copy_from_slice(device.data(BLOCK_SIZE));
        }

        let free_blocks = self.superblock.free_block_count;
        let allocated_free_blocks = free_blocks
            .checked_sub(1)
            .unwrap_or_else(|| device.fail("allocation probe has no free blocks"));
        set_superblock_free_block_count(
            &mut blocks.modified.block_mut(0)
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            allocated_free_blocks,
        )
        .unwrap_or_else(|_| device.fail("allocation probe superblock update failed"));
        {
            let descriptor_frame = blocks.modified.frames[1];
            let bitmap_frame = blocks.modified.frames[2];
            set_block_allocation(
                scratch_frame_bytes_mut(bitmap_frame),
                &mut scratch_frame_bytes_mut(descriptor_frame)
                    [group_descriptor_offset..group_descriptor_end],
                0,
                ALLOCATION_PROBE_BLOCK,
                true,
                &self.superblock,
            )
            .unwrap_or_else(|_| device.fail("allocation probe bitmap update failed"));
        }
        resize_regular_file_by_one_block(
            &mut blocks.modified.block_mut(3)[inode_offset..inode_end],
            file.inode.number,
            &self.superblock,
            ALLOCATION_PROBE_BLOCK,
            true,
        )
        .unwrap_or_else(|_| device.fail("allocation probe inode growth failed"));
        blocks.modified.block_mut(4).copy_from_slice(new_data);

        (
            blocks,
            AllocationLayout {
                targets,
                group_descriptor_offset,
                group_descriptor_end,
            },
        )
    }

    async fn probe_file_creation_transactions(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        parent_inode_number: u32,
        descriptors: &mut FileDescriptorTable<8>,
    ) -> CreateJournalProbe {
        let group_index = self
            .superblock
            .inode_group(CREATE_PROBE_INODE)
            .unwrap_or_else(|_| device.fail("create probe inode group is invalid"));
        let group =
            read_group_descriptor(device, &self.superblock, group_index, &mut self.cache).await;
        let group_descriptor_location = self
            .superblock
            .group_descriptor_location(group_index)
            .unwrap_or_else(|_| device.fail("create probe group descriptor is invalid"));
        let group_descriptor_offset = group_descriptor_location.offset as usize;
        let group_descriptor_end =
            group_descriptor_offset + usize::from(self.superblock.descriptor_size);
        if group_descriptor_end > BLOCK_SIZE {
            device.fail("create probe group descriptor crosses its block");
        }
        let inode_location = self
            .superblock
            .inode_location(CREATE_PROBE_INODE, &group)
            .unwrap_or_else(|_| device.fail("create probe inode location is invalid"));
        let inode_offset = inode_location.offset as usize;
        let inode_end = inode_offset + usize::from(self.superblock.inode_size);
        if inode_end > BLOCK_SIZE {
            device.fail("create probe inode crosses its table block");
        }
        let parent_inode = self.read_inode(device, parent_inode_number).await;
        let directory_block = self.directory_extent(device, &parent_inode).physical_block;
        let targets = [
            0,
            group_descriptor_location.block,
            group.inode_bitmap_block,
            inode_location.block,
            directory_block,
        ];
        for (index, target) in targets.iter().enumerate() {
            if targets[..index].contains(target) {
                device.fail("create probe home blocks overlap");
            }
        }

        let mut original = BlockSet::<CREATE_TRANSACTION_BLOCKS>::new();
        let mut modified = BlockSet::<CREATE_TRANSACTION_BLOCKS>::new();
        for (index, target) in targets.iter().enumerate() {
            device
                .read(
                    block_to_sector(device, &self.superblock, *target),
                    BLOCK_SIZE,
                )
                .await;
            original
                .block_mut(index)
                .copy_from_slice(device.data(BLOCK_SIZE));
            modified
                .block_mut(index)
                .copy_from_slice(device.data(BLOCK_SIZE));
        }

        let free_inodes = self.superblock.free_inode_count;
        let allocated_free_inodes = free_inodes
            .checked_sub(1)
            .unwrap_or_else(|| device.fail("create probe has no free inodes"));
        set_superblock_free_inode_count(
            &mut modified.block_mut(0)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            allocated_free_inodes,
        )
        .unwrap_or_else(|_| device.fail("create probe superblock update failed"));
        {
            let descriptor_frame = modified.frames[1];
            let bitmap_frame = modified.frames[2];
            set_inode_allocation(
                scratch_frame_bytes_mut(bitmap_frame),
                &mut scratch_frame_bytes_mut(descriptor_frame)
                    [group_descriptor_offset..group_descriptor_end],
                group_index,
                CREATE_PROBE_INODE,
                true,
                &self.superblock,
            )
            .unwrap_or_else(|_| device.fail("create probe inode bitmap update failed"));
        }
        initialize_empty_regular_inode(
            &mut modified.block_mut(3)[inode_offset..inode_end],
            CREATE_PROBE_INODE,
            &self.superblock,
            0o644,
            CREATE_PROBE_TIMESTAMP,
            1,
        )
        .unwrap_or_else(|_| device.fail("create probe inode initialization failed"));
        insert_linear_directory_entry(
            modified.block_mut(4),
            &parent_inode,
            &self.superblock,
            CREATE_PROBE_NAME,
            CREATE_PROBE_INODE,
            DIRECTORY_ENTRY_REGULAR_FILE,
        )
        .unwrap_or_else(|_| device.fail("create probe directory insertion failed"));

        let first_sequence = journal.superblock.sequence;
        let second_sequence = self
            .commit_multi_block_journal_transaction(
                device,
                journal,
                first_sequence,
                &targets,
                &modified,
            )
            .await;
        let allocated_group = GroupDescriptor::parse(
            &modified.block(1)[group_descriptor_offset..group_descriptor_end],
            group_index,
            &self.superblock,
        )
        .unwrap_or_else(|_| device.fail("create probe updated group descriptor is invalid"));
        if self.superblock.free_inode_count != allocated_free_inodes
            || allocated_group.free_inode_count + 1 != group.free_inode_count
            || allocated_group.unused_inode_count + 1 != group.unused_inode_count
        {
            device.fail("create probe allocation counts are invalid");
        }
        let created = self.open_file(device, &CREATE_PROBE_PATH).await;
        if created.inode.number != CREATE_PROBE_INODE
            || created.inode.size != 0
            || created.inode.block_count_512 != 0
            || created.inode.extent_depth() != Ok(0)
            || created.inode.extent_for_logical_block(0) != Ok(None)
        {
            device.fail("create probe file readback is invalid");
        }
        let fd = descriptors
            .open_with_mode(
                FileNode {
                    filesystem_id: ROOT_FILESYSTEM_ID,
                    node_id: u64::from(created.inode.number),
                    size: created.inode.size,
                },
                AccessMode::ReadWrite,
            )
            .unwrap_or_else(|_| device.fail("create probe descriptor allocation failed"));
        let mut empty_read = [0u8; 1];
        if fd != 3
            || read_descriptor(self, device, descriptors, fd, &created, &mut empty_read).await != 0
        {
            device.fail("create probe descriptor readback failed");
        }
        descriptors
            .close(fd)
            .unwrap_or_else(|_| device.fail("create probe descriptor close failed"));

        set_superblock_free_inode_count(
            &mut modified.block_mut(0)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            free_inodes,
        )
        .unwrap_or_else(|_| device.fail("create probe superblock restoration failed"));
        {
            let descriptor_frame = modified.frames[1];
            let bitmap_frame = modified.frames[2];
            set_inode_allocation(
                scratch_frame_bytes_mut(bitmap_frame),
                &mut scratch_frame_bytes_mut(descriptor_frame)
                    [group_descriptor_offset..group_descriptor_end],
                group_index,
                CREATE_PROBE_INODE,
                false,
                &self.superblock,
            )
            .unwrap_or_else(|_| device.fail("create probe inode bitmap restoration failed"));
        }
        modified.block_mut(3)[inode_offset..inode_end]
            .copy_from_slice(&original.block(3)[inode_offset..inode_end]);
        remove_linear_directory_entry(
            modified.block_mut(4),
            &parent_inode,
            &self.superblock,
            CREATE_PROBE_NAME,
            CREATE_PROBE_INODE,
        )
        .unwrap_or_else(|_| device.fail("create probe directory removal failed"));
        for index in 0..CREATE_TRANSACTION_BLOCKS {
            if modified.block(index) != original.block(index) {
                device.fail("create probe encoded unlink did not restore its home block");
            }
        }

        let final_sequence = self
            .commit_multi_block_journal_transaction(
                device,
                journal,
                second_sequence,
                &targets,
                &modified,
            )
            .await;
        let restored_parent = self.read_inode(device, parent_inode_number).await;
        if self
            .find_directory_entry(device, &restored_parent, CREATE_PROBE_NAME)
            .await
            .is_some()
        {
            device.fail("create probe directory entry survived unlink");
        }
        if self.superblock.free_inode_count != free_inodes {
            device.fail("create probe superblock free inode count was not restored");
        }

        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let mut rewind = JournalStateScratch::new();
        device.read(journal_sector, BLOCK_SIZE).await;
        rewind
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        let checkpointed = JournalSuperblock::parse(rewind.journal_block())
            .unwrap_or_else(|_| device.fail("create probe journal state is invalid"));
        if checkpointed.sequence != final_sequence || checkpointed.start != 0 {
            device.fail("create probe journal final sequence mismatch");
        }
        set_journal_superblock_state(rewind.journal_block_mut(), first_sequence, 0)
            .unwrap_or_else(|_| device.fail("create probe journal rewind failed"));
        device.write(journal_sector, rewind.journal_block()).await;
        device.flush().await;
        device.read(journal_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != rewind.journal_block() {
            device.fail("create probe journal rewind readback mismatch");
        }

        CreateJournalProbe {
            fd,
            inode_bitmap_block: group.inode_bitmap_block,
            group_descriptor_block: group_descriptor_location.block,
            inode_table_block: inode_location.block,
            directory_block,
            free_inodes,
            allocated_free_inodes,
            first_sequence,
            second_sequence,
            final_sequence,
        }
    }

    async fn commit_multi_block_journal_transaction<const N: usize>(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        sequence: u32,
        target_blocks: &[u64; N],
        home_blocks: &BlockSet<N>,
    ) -> u32 {
        if N == 0 || N > MULTI_TRANSACTION_MAX_BLOCKS {
            device.fail("multi-block JBD2 transaction capacity is unsupported");
        }
        let filesystem_sector = block_to_sector(device, &self.superblock, 0);
        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let descriptor_block = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.first_log_block))
            .unwrap_or_else(|| device.fail("multi-block JBD2 descriptor overflow"));
        let commit_block = descriptor_block
            .checked_add(1 + u64::try_from(N).unwrap_or(u64::MAX))
            .unwrap_or_else(|| device.fail("multi-block JBD2 commit overflow"));
        let journal_end = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.max_length))
            .unwrap_or_else(|| device.fail("multi-block JBD2 extent overflow"));
        if commit_block >= journal_end {
            device.fail("multi-block JBD2 transaction exceeds journal extent");
        }
        for (index, target) in target_blocks.iter().enumerate() {
            if *target >= self.superblock.block_count
                || (*target >= journal.physical_block && *target < journal_end)
                || target_blocks[..index].contains(target)
            {
                device.fail("multi-block JBD2 target is unsafe");
            }
        }

        let mut scratch = MultiJournalScratch::<N>::new();
        device.read(filesystem_sector, BLOCK_SIZE).await;
        scratch
            .state
            .filesystem_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        if Superblock::parse(
            &scratch.state.filesystem_block()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
        )
        .is_err()
        {
            device.fail("multi-block JBD2 transaction requires a clean filesystem");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        scratch
            .state
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        let initial_journal = JournalSuperblock::parse(scratch.state.journal_block())
            .unwrap_or_else(|_| device.fail("multi-block JBD2 initial state is invalid"));
        if initial_journal.sequence != sequence || initial_journal.start != 0 {
            device.fail("multi-block JBD2 initial sequence mismatch");
        }
        for offset in 0..N + 2 {
            let block = descriptor_block
                .checked_add(u64::try_from(offset).unwrap_or(u64::MAX))
                .unwrap_or_else(|| device.fail("multi-block JBD2 record block overflow"));
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE).iter().any(|byte| *byte != 0) {
                device.fail("multi-block JBD2 record block is not empty");
            }
        }

        set_superblock_recovery(
            &mut scratch.state.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            true,
        )
        .unwrap_or_else(|_| device.fail("multi-block JBD2 recovery activation failed"));
        let superblock_target = target_blocks.iter().position(|target| *target == 0);
        if let Some(index) = superblock_target {
            scratch
                .desired_superblock_mut()
                .copy_from_slice(home_blocks.block(index));
            set_superblock_recovery(
                &mut scratch.desired_superblock_mut()
                    [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
                true,
            )
            .unwrap_or_else(|_| device.fail("multi-block JBD2 home superblock activation failed"));
        }

        let mut target_blocks_u32 = [0u32; N];
        let mut escaped_tags = [false; N];
        for index in 0..N {
            target_blocks_u32[index] = u32::try_from(target_blocks[index])
                .unwrap_or_else(|_| device.fail("multi-block JBD2 target exceeds 32 bits"));
            let home_frame = if Some(index) == superblock_target {
                scratch.desired_superblock_frame
            } else {
                home_blocks.frames[index]
            };
            escaped_tags[index] = encode_journal_data_block(
                scratch_frame_bytes_mut(scratch.data_frames[index]),
                scratch_frame_bytes(home_frame),
            )
            .unwrap_or_else(|_| device.fail("multi-block JBD2 data encoding failed"));
        }
        encode_journal_descriptor_block(
            scratch.descriptor_mut(),
            sequence,
            &target_blocks_u32,
            &escaped_tags,
            &self.superblock.uuid,
        )
        .unwrap_or_else(|_| device.fail("multi-block JBD2 descriptor encoding failed"));
        encode_journal_commit_block(scratch.commit_mut(), sequence)
            .unwrap_or_else(|_| device.fail("multi-block JBD2 commit encoding failed"));

        device
            .write(filesystem_sector, scratch.state.filesystem_block())
            .await;
        device.flush().await;
        set_journal_superblock_state(
            scratch.state.journal_block_mut(),
            sequence,
            journal.superblock.first_log_block,
        )
        .unwrap_or_else(|_| device.fail("multi-block JBD2 state activation failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, descriptor_block),
                scratch.descriptor(),
            )
            .await;
        for index in 0..N {
            let data_block = descriptor_block
                .checked_add(1 + u64::try_from(index).unwrap_or(u64::MAX))
                .unwrap_or_else(|| device.fail("multi-block JBD2 data location overflow"));
            device
                .write(
                    block_to_sector(device, &self.superblock, data_block),
                    scratch.data(index),
                )
                .await;
        }
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, commit_block),
                scratch.commit(),
            )
            .await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.state.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            ) != Err(ParseError::DirtyFilesystem)
        {
            device.fail("multi-block JBD2 active filesystem state mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let active_journal = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("multi-block JBD2 active state is invalid"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || active_journal.sequence != sequence
            || active_journal.start != journal.superblock.first_log_block
        {
            device.fail("multi-block JBD2 active state mismatch");
        }
        for offset in 0..N + 2 {
            let block = descriptor_block
                .checked_add(u64::try_from(offset).unwrap_or(u64::MAX))
                .unwrap_or_else(|| device.fail("multi-block JBD2 readback block overflow"));
            let expected = if offset == 0 {
                scratch.descriptor()
            } else if offset == N + 1 {
                scratch.commit()
            } else {
                scratch.data(offset - 1)
            };
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE) != expected {
                device.fail("multi-block JBD2 record readback mismatch");
            }
        }

        for (index, target) in target_blocks.iter().enumerate() {
            let home = if Some(index) == superblock_target {
                scratch.desired_superblock()
            } else {
                home_blocks.block(index)
            };
            device
                .write(block_to_sector(device, &self.superblock, *target), home)
                .await;
        }
        device.flush().await;
        for (index, target) in target_blocks.iter().enumerate() {
            let expected = if Some(index) == superblock_target {
                scratch.desired_superblock()
            } else {
                home_blocks.block(index)
            };
            device
                .read(
                    block_to_sector(device, &self.superblock, *target),
                    BLOCK_SIZE,
                )
                .await;
            if device.data(BLOCK_SIZE) != expected {
                let first_difference = device
                    .data(BLOCK_SIZE)
                    .iter()
                    .zip(expected.iter())
                    .position(|(actual, wanted)| actual != wanted)
                    .unwrap_or(BLOCK_SIZE);
                crate::serial::serialln(format_args!(
                    "SLOPOS-EXT4: multi-block home mismatch index={index} target_block={target} first_difference={first_difference} actual={:#x} expected={:#x}",
                    device
                        .data(BLOCK_SIZE)
                        .get(first_difference)
                        .copied()
                        .unwrap_or(0),
                    expected.get(first_difference).copied().unwrap_or(0)
                ));
                device.fail("multi-block JBD2 home readback mismatch");
            }
            self.cache.invalidate(*target);
        }

        let next_sequence = sequence
            .checked_add(1)
            .unwrap_or_else(|| device.fail("multi-block JBD2 sequence overflow"));
        set_journal_superblock_state(scratch.state.journal_block_mut(), next_sequence, 0)
            .unwrap_or_else(|_| device.fail("multi-block JBD2 checkpoint update failed"));
        for offset in 0..N + 2 {
            let block = descriptor_block
                .checked_add(u64::try_from(offset).unwrap_or(u64::MAX))
                .unwrap_or_else(|| device.fail("multi-block JBD2 clear block overflow"));
            device
                .write(
                    block_to_sector(device, &self.superblock, block),
                    &ZERO_BLOCK,
                )
                .await;
        }
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;
        self.cache.invalidate(journal.physical_block);

        let final_filesystem_block = if superblock_target.is_some() {
            scratch.desired_superblock_mut()
        } else {
            scratch.state.filesystem_block_mut()
        };
        set_superblock_recovery(
            &mut final_filesystem_block[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            false,
        )
        .unwrap_or_else(|_| device.fail("multi-block JBD2 recovery clearing failed"));
        device
            .write(filesystem_sector, final_filesystem_block)
            .await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        let clean_superblock = Superblock::parse(
            &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
        )
        .unwrap_or_else(|_| device.fail("multi-block JBD2 final superblock is invalid"));
        let expected_filesystem_block = if superblock_target.is_some() {
            scratch.desired_superblock()
        } else {
            scratch.state.filesystem_block()
        };
        if device.data(BLOCK_SIZE) != expected_filesystem_block {
            device.fail("multi-block JBD2 final superblock mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let final_journal = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("multi-block JBD2 final state is invalid"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || final_journal.sequence != next_sequence
            || final_journal.start != 0
        {
            device.fail("multi-block JBD2 final state mismatch");
        }
        self.superblock = clean_superblock;
        next_sequence
    }

    async fn commit_single_block_journal_transaction(
        &mut self,
        device: &mut BlockDevice,
        journal: &JournalProbe,
        sequence: u32,
        target_block: u64,
        home_data: &[u8],
    ) -> u32 {
        if home_data.len() != BLOCK_SIZE {
            device.fail("JBD2 home data must be one filesystem block");
        }
        let filesystem_sector = block_to_sector(device, &self.superblock, 0);
        let journal_sector = block_to_sector(device, &self.superblock, journal.physical_block);
        let descriptor_block = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.first_log_block))
            .unwrap_or_else(|| device.fail("JBD2 transaction descriptor overflow"));
        let data_block = descriptor_block
            .checked_add(1)
            .unwrap_or_else(|| device.fail("JBD2 transaction data overflow"));
        let commit_block = descriptor_block
            .checked_add(2)
            .unwrap_or_else(|| device.fail("JBD2 transaction commit overflow"));
        let journal_end = journal
            .physical_block
            .checked_add(u64::from(journal.superblock.max_length))
            .unwrap_or_else(|| device.fail("JBD2 transaction extent overflow"));
        if commit_block >= journal_end {
            device.fail("JBD2 transaction exceeds journal extent");
        }

        let mut scratch = ActiveJournalScratch::new();
        device.read(filesystem_sector, BLOCK_SIZE).await;
        scratch
            .state
            .filesystem_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        if Superblock::parse(
            &scratch.state.filesystem_block()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
        )
        .is_err()
        {
            device.fail("JBD2 transaction requires a clean ext4 superblock");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        scratch
            .state
            .journal_block_mut()
            .copy_from_slice(device.data(BLOCK_SIZE));
        let initial_journal = JournalSuperblock::parse(scratch.state.journal_block())
            .unwrap_or_else(|_| device.fail("JBD2 transaction initial state is invalid"));
        if initial_journal.sequence != sequence || initial_journal.start != 0 {
            device.fail("JBD2 transaction initial sequence mismatch");
        }
        for block in [descriptor_block, data_block, commit_block] {
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE).iter().any(|byte| *byte != 0) {
                device.fail("JBD2 transaction record block is not empty");
            }
        }

        let target_block = u32::try_from(target_block)
            .unwrap_or_else(|_| device.fail("JBD2 transaction target exceeds 32 bits"));
        {
            let (descriptor, data, commit) = scratch.records.buffers();
            encode_single_block_journal_transaction(
                descriptor,
                data,
                commit,
                sequence,
                target_block,
                &self.superblock.uuid,
                home_data,
            )
            .unwrap_or_else(|_| device.fail("JBD2 transaction encoding failed"));
        }

        set_superblock_recovery(
            &mut scratch.state.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            true,
        )
        .unwrap_or_else(|_| device.fail("JBD2 transaction recovery activation failed"));
        device
            .write(filesystem_sector, scratch.state.filesystem_block())
            .await;
        device.flush().await;
        set_journal_superblock_state(
            scratch.state.journal_block_mut(),
            sequence,
            journal.superblock.first_log_block,
        )
        .unwrap_or_else(|_| device.fail("JBD2 transaction state activation failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;

        device
            .write(
                block_to_sector(device, &self.superblock, descriptor_block),
                scratch.records.descriptor(),
            )
            .await;
        device
            .write(
                block_to_sector(device, &self.superblock, data_block),
                scratch.records.data(),
            )
            .await;
        device.flush().await;
        device
            .write(
                block_to_sector(device, &self.superblock, commit_block),
                scratch.records.commit(),
            )
            .await;
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.state.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            ) != Err(ParseError::DirtyFilesystem)
        {
            device.fail("JBD2 transaction recovery readback mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let active_journal = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("JBD2 transaction active state is invalid"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || active_journal.sequence != sequence
            || active_journal.start != journal.superblock.first_log_block
        {
            device.fail("JBD2 transaction active state mismatch");
        }
        for (block, expected) in [
            (descriptor_block, scratch.records.descriptor()),
            (data_block, scratch.records.data()),
            (commit_block, scratch.records.commit()),
        ] {
            device
                .read(block_to_sector(device, &self.superblock, block), BLOCK_SIZE)
                .await;
            if device.data(BLOCK_SIZE) != expected {
                device.fail("JBD2 transaction record readback mismatch");
            }
        }
        let descriptor = JournalDescriptor::parse(scratch.records.descriptor())
            .unwrap_or_else(|_| device.fail("JBD2 transaction descriptor is invalid"));
        let commit = JournalCommit::parse(scratch.records.commit())
            .unwrap_or_else(|_| device.fail("JBD2 transaction commit is invalid"));
        if descriptor.sequence != sequence
            || descriptor.target_block != target_block
            || descriptor.uuid != self.superblock.uuid
            || commit.sequence != sequence
        {
            device.fail("JBD2 transaction identity mismatch");
        }

        device
            .write(
                block_to_sector(device, &self.superblock, u64::from(target_block)),
                home_data,
            )
            .await;
        device.flush().await;
        device
            .read(
                block_to_sector(device, &self.superblock, u64::from(target_block)),
                BLOCK_SIZE,
            )
            .await;
        if device.data(BLOCK_SIZE) != home_data {
            device.fail("JBD2 transaction home-block readback mismatch");
        }
        self.cache.invalidate(u64::from(target_block));

        let next_sequence = sequence
            .checked_add(1)
            .unwrap_or_else(|| device.fail("JBD2 transaction sequence overflow"));
        set_journal_superblock_state(scratch.state.journal_block_mut(), next_sequence, 0)
            .unwrap_or_else(|_| device.fail("JBD2 transaction checkpoint update failed"));
        device
            .write(journal_sector, scratch.state.journal_block())
            .await;
        device.flush().await;
        set_superblock_recovery(
            &mut scratch.state.filesystem_block_mut()
                [SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            false,
        )
        .unwrap_or_else(|_| device.fail("JBD2 transaction recovery clearing failed"));
        device
            .write(filesystem_sector, scratch.state.filesystem_block())
            .await;
        device.flush().await;
        for block in [descriptor_block, data_block, commit_block] {
            device
                .write(
                    block_to_sector(device, &self.superblock, block),
                    &ZERO_BLOCK,
                )
                .await;
        }
        device.flush().await;

        device.read(filesystem_sector, BLOCK_SIZE).await;
        if device.data(BLOCK_SIZE) != scratch.state.filesystem_block()
            || Superblock::parse(
                &device.data(BLOCK_SIZE)[SUPERBLOCK_OFFSET..SUPERBLOCK_OFFSET + SUPERBLOCK_SIZE],
            )
            .is_err()
        {
            device.fail("JBD2 transaction final ext4 state mismatch");
        }
        device.read(journal_sector, BLOCK_SIZE).await;
        let final_journal = JournalSuperblock::parse(device.data(BLOCK_SIZE))
            .unwrap_or_else(|_| device.fail("JBD2 transaction final state is invalid"));
        if device.data(BLOCK_SIZE) != scratch.state.journal_block()
            || final_journal.sequence != next_sequence
            || final_journal.start != 0
        {
            device.fail("JBD2 transaction final state mismatch");
        }
        next_sequence
    }

    async fn open_file(&mut self, device: &mut BlockDevice, components: &[&[u8]]) -> Ext4File {
        self.try_open_file(device, components)
            .await
            .unwrap_or_else(|| device.fail("ext4 path component was not found"))
    }

    async fn try_open_file(
        &mut self,
        device: &mut BlockDevice,
        components: &[&[u8]],
    ) -> Option<Ext4File> {
        let (inode, parent_inode, directory_block, followed_symlink) =
            self.try_resolve_path(device, components).await?;
        if !inode.is_regular_file() {
            device.fail("ext4 open target is not a regular file");
        }
        Some(Ext4File {
            inode,
            parent_inode,
            directory_block,
            followed_symlink,
        })
    }

    async fn try_resolve_path(
        &mut self,
        device: &mut BlockDevice,
        components: &[&[u8]],
    ) -> Option<(Inode, u32, u32, u32)> {
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
                .await?;
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
                    .await?;
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
        Some((current, parent_inode, directory_block, followed_symlink))
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

struct JournalRecordProbe {
    sequence: u32,
    target_block: u32,
    descriptor_block: u64,
    data_block: u64,
    commit_block: u64,
}

struct JournalStateProbe {
    sequence: u32,
    active_start: u32,
}

#[derive(Clone, Copy)]
struct JournalRecovery {
    replayed: bool,
    sequence: u32,
    start: u32,
    tag_count: usize,
    target_block: u32,
    escaped: bool,
    next_sequence: u32,
}

struct ActiveJournalProbe {
    sequence: u32,
    target_block: u32,
    next_sequence: u32,
}

struct MetadataJournalProbe {
    target_block: u64,
    first_sequence: u32,
    second_sequence: u32,
    final_sequence: u32,
}

struct AllocationJournalProbe {
    allocated_block: u64,
    bitmap_block: u64,
    group_descriptor_block: u64,
    inode_table_block: u64,
    first_sequence: u32,
    second_sequence: u32,
    final_sequence: u32,
}

struct CreateJournalProbe {
    fd: u32,
    inode_bitmap_block: u64,
    group_descriptor_block: u64,
    inode_table_block: u64,
    directory_block: u64,
    free_inodes: u32,
    allocated_free_inodes: u32,
    first_sequence: u32,
    second_sequence: u32,
    final_sequence: u32,
}

#[derive(Clone, Copy)]
struct AllocationLayout {
    targets: [u64; ALLOCATION_TRANSACTION_BLOCKS],
    group_descriptor_offset: usize,
    group_descriptor_end: usize,
}

struct ActiveJournalScratch {
    records: JournalScratch,
    state: JournalStateScratch,
}

impl ActiveJournalScratch {
    fn new() -> Self {
        Self {
            records: JournalScratch::new(),
            state: JournalStateScratch::new(),
        }
    }
}

struct JournalReplayScratch {
    descriptor_frame: usize,
    data_frames: [usize; MULTI_TRANSACTION_MAX_BLOCKS],
    commit_frame: usize,
    home_frames: [usize; MULTI_TRANSACTION_MAX_BLOCKS],
    state: JournalStateScratch,
}

impl JournalReplayScratch {
    fn new() -> Self {
        Self {
            descriptor_frame: allocate_scratch_frame(),
            data_frames: core::array::from_fn(|_| allocate_scratch_frame()),
            commit_frame: allocate_scratch_frame(),
            home_frames: core::array::from_fn(|_| allocate_scratch_frame()),
            state: JournalStateScratch::new(),
        }
    }

    fn data_block_mut(&mut self, index: usize) -> &mut [u8] {
        scratch_frame_bytes_mut(self.data_frames[index])
    }

    fn home_block(&self, index: usize) -> &[u8] {
        scratch_frame_bytes(self.home_frames[index])
    }

    fn home_block_mut(&mut self, index: usize) -> &mut [u8] {
        scratch_frame_bytes_mut(self.home_frames[index])
    }
}

struct MultiJournalScratch<const N: usize> {
    descriptor_frame: usize,
    data_frames: [usize; N],
    commit_frame: usize,
    state: JournalStateScratch,
    desired_superblock_frame: usize,
}

impl<const N: usize> MultiJournalScratch<N> {
    fn new() -> Self {
        Self {
            descriptor_frame: allocate_scratch_frame(),
            data_frames: core::array::from_fn(|_| allocate_scratch_frame()),
            commit_frame: allocate_scratch_frame(),
            state: JournalStateScratch::new(),
            desired_superblock_frame: allocate_scratch_frame(),
        }
    }

    fn descriptor(&self) -> &[u8] {
        scratch_frame_bytes(self.descriptor_frame)
    }

    fn descriptor_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.descriptor_frame)
    }

    fn data(&self, index: usize) -> &[u8] {
        scratch_frame_bytes(self.data_frames[index])
    }

    fn commit(&self) -> &[u8] {
        scratch_frame_bytes(self.commit_frame)
    }

    fn commit_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.commit_frame)
    }

    fn desired_superblock(&self) -> &[u8] {
        scratch_frame_bytes(self.desired_superblock_frame)
    }

    fn desired_superblock_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.desired_superblock_frame)
    }
}

struct BlockSet<const N: usize> {
    frames: [usize; N],
}

impl<const N: usize> BlockSet<N> {
    fn new() -> Self {
        Self {
            frames: core::array::from_fn(|_| allocate_scratch_frame()),
        }
    }

    fn block(&self, index: usize) -> &[u8] {
        scratch_frame_bytes(self.frames[index])
    }

    fn block_mut(&mut self, index: usize) -> &mut [u8] {
        scratch_frame_bytes_mut(self.frames[index])
    }
}

struct AllocationJournalScratch {
    original: BlockSet<ALLOCATION_TRANSACTION_BLOCKS>,
    modified: BlockSet<ALLOCATION_TRANSACTION_BLOCKS>,
}

impl AllocationJournalScratch {
    fn new() -> Self {
        Self {
            original: BlockSet::new(),
            modified: BlockSet::new(),
        }
    }
}

struct MetadataJournalScratch {
    original_frame: usize,
    modified_frame: usize,
}

impl MetadataJournalScratch {
    fn new() -> Self {
        Self {
            original_frame: allocate_scratch_frame(),
            modified_frame: allocate_scratch_frame(),
        }
    }

    fn original_block(&self) -> &[u8] {
        scratch_frame_bytes(self.original_frame)
    }

    fn original_block_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.original_frame)
    }

    fn modified_block(&self) -> &[u8] {
        scratch_frame_bytes(self.modified_frame)
    }

    fn modified_block_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.modified_frame)
    }
}

struct JournalScratch {
    descriptor_frame: usize,
    data_frame: usize,
    commit_frame: usize,
}

impl JournalScratch {
    fn new() -> Self {
        Self {
            descriptor_frame: allocate_scratch_frame(),
            data_frame: allocate_scratch_frame(),
            commit_frame: allocate_scratch_frame(),
        }
    }

    fn buffers(&mut self) -> (&mut [u8], &mut [u8], &mut [u8]) {
        // SAFETY: all three fields are distinct, permanently live frames
        // exclusively owned by this scratch object.
        unsafe {
            (
                core::slice::from_raw_parts_mut(self.descriptor_frame as *mut u8, BLOCK_SIZE),
                core::slice::from_raw_parts_mut(self.data_frame as *mut u8, BLOCK_SIZE),
                core::slice::from_raw_parts_mut(self.commit_frame as *mut u8, BLOCK_SIZE),
            )
        }
    }

    fn descriptor(&self) -> &[u8] {
        scratch_frame_bytes(self.descriptor_frame)
    }

    fn data(&self) -> &[u8] {
        scratch_frame_bytes(self.data_frame)
    }

    fn commit(&self) -> &[u8] {
        scratch_frame_bytes(self.commit_frame)
    }
}

struct JournalStateScratch {
    filesystem_frame: usize,
    journal_frame: usize,
}

impl JournalStateScratch {
    fn new() -> Self {
        Self {
            filesystem_frame: allocate_scratch_frame(),
            journal_frame: allocate_scratch_frame(),
        }
    }

    fn filesystem_block(&self) -> &[u8] {
        scratch_frame_bytes(self.filesystem_frame)
    }

    fn filesystem_block_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.filesystem_frame)
    }

    fn journal_block(&self) -> &[u8] {
        scratch_frame_bytes(self.journal_frame)
    }

    fn journal_block_mut(&mut self) -> &mut [u8] {
        scratch_frame_bytes_mut(self.journal_frame)
    }
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

fn allocate_scratch_frame() -> usize {
    let frame = crate::memory::allocate_frame()
        .unwrap_or_else(|| crate::fatal("out of frames for journal scratch"));
    // SAFETY: allocator returned an exclusive identity-mapped 4 KiB frame.
    unsafe { ptr::write_bytes(frame as *mut u8, 0, BLOCK_SIZE) };
    frame as usize
}

fn scratch_frame_bytes(frame: usize) -> &'static [u8] {
    // SAFETY: journal scratch frames remain permanently allocated.
    unsafe { core::slice::from_raw_parts(frame as *const u8, BLOCK_SIZE) }
}

fn scratch_frame_bytes_mut(frame: usize) -> &'static mut [u8] {
    // SAFETY: callers reach each exclusively owned frame through a mutable
    // scratch object, and journal scratch frames remain permanently allocated.
    unsafe { core::slice::from_raw_parts_mut(frame as *mut u8, BLOCK_SIZE) }
}
