// SPDX-License-Identifier: 0BSD

#![no_std]

pub const SUPERBLOCK_SIZE: usize = 1024;
pub const SUPERBLOCK_MAGIC: u16 = 0xef53;
pub const FEATURE_INCOMPAT_64BIT: u32 = 0x0080;
pub const FEATURE_INCOMPAT_CHECKSUM_SEED: u32 = 0x2000;
pub const FEATURE_READ_ONLY_COMPAT_METADATA_CHECKSUM: u32 = 0x0400;
pub const INODE_FLAG_EXTENTS: u32 = 0x0008_0000;
pub const INODE_FLAG_DIRECTORY_INDEX: u32 = 0x0000_1000;
pub const ROOT_INODE: u32 = 2;
pub const JOURNAL_INODE: u32 = 8;
pub const JOURNAL_SUPERBLOCK_SIZE: usize = 1024;
pub const JOURNAL_MAGIC: u32 = 0xc03b_3998;
pub const JOURNAL_DESCRIPTOR_BLOCK: u32 = 1;
pub const JOURNAL_COMMIT_BLOCK: u32 = 2;
pub const DIRECTORY_ENTRY_REGULAR_FILE: u8 = 1;
pub const DIRECTORY_ENTRY_DIRECTORY: u8 = 2;
pub const DIRECTORY_ENTRY_SYMLINK: u8 = 7;
const FEATURE_INCOMPAT_FILETYPE: u32 = 0x0002;
const FEATURE_INCOMPAT_EXTENTS: u32 = 0x0040;
const FEATURE_INCOMPAT_FLEX_BG: u32 = 0x0200;
const SUPPORTED_INCOMPAT_FEATURES: u32 = FEATURE_INCOMPAT_FILETYPE
    | FEATURE_INCOMPAT_EXTENTS
    | FEATURE_INCOMPAT_64BIT
    | FEATURE_INCOMPAT_FLEX_BG
    | FEATURE_INCOMPAT_CHECKSUM_SEED;
const FILESYSTEM_STATE_CLEAN: u16 = 0x0001;
const DIRECTORY_MODE: u16 = 0x4000;
const REGULAR_FILE_MODE: u16 = 0x8000;
const SYMLINK_MODE: u16 = 0xa000;
const MODE_TYPE_MASK: u16 = 0xf000;
const EXTENT_HEADER_MAGIC: u16 = 0xf30a;
const EXTENT_HEADER_SIZE: usize = 12;
const EXTENT_ENTRY_SIZE: usize = 12;
const EXTENT_TAIL_SIZE: usize = 4;
const MAX_EXTENT_DEPTH: u16 = 5;
const DIRECTORY_CHECKSUM_FILE_TYPE: u8 = 0xde;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    Truncated,
    InvalidMagic,
    InvalidBlockSize,
    InvalidInodeSize,
    InvalidDescriptorSize,
    InvalidGeometry,
    UnsupportedChecksum,
    UnsupportedFeature,
    DirtyFilesystem,
    InvalidChecksum,
    InvalidInode,
    InvalidExtent,
    UnsupportedExtentDepth,
    UnsupportedDirectoryIndex,
    NotDirectory,
    InvalidDirectory,
    InvalidPathComponent,
    NotSymlink,
    UnsupportedSymlink,
    InvalidSymlink,
    InvalidJournal,
}

pub fn validate_path_component(component: &[u8]) -> Result<(), ParseError> {
    if component.is_empty()
        || component.len() > 255
        || component == b"."
        || component == b".."
        || component.iter().any(|byte| *byte == 0 || *byte == b'/')
    {
        Err(ParseError::InvalidPathComponent)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Superblock {
    pub inode_count: u32,
    pub block_count: u64,
    pub free_block_count: u64,
    pub free_inode_count: u32,
    pub first_data_block: u32,
    pub block_size: u32,
    pub blocks_per_group: u32,
    pub inodes_per_group: u32,
    pub state: u16,
    pub errors: u16,
    pub revision: u32,
    pub inode_size: u16,
    pub descriptor_size: u16,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_read_only_compat: u32,
    pub uuid: [u8; 16],
    pub volume_name: [u8; 16],
    pub checksum_type: u8,
    pub checksum: u32,
    pub checksum_seed: u32,
    pub journal_inode: u32,
}

impl Superblock {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < SUPERBLOCK_SIZE {
            return Err(ParseError::Truncated);
        }
        if read_u16(bytes, 56)? != SUPERBLOCK_MAGIC {
            return Err(ParseError::InvalidMagic);
        }
        let logarithm = read_u32(bytes, 24)?;
        if logarithm > 6 {
            return Err(ParseError::InvalidBlockSize);
        }
        let block_size = 1024u32
            .checked_shl(logarithm)
            .ok_or(ParseError::InvalidBlockSize)?;
        let revision = read_u32(bytes, 76)?;
        let inode_size = if revision == 0 {
            128
        } else {
            read_u16(bytes, 88)?
        };
        if inode_size < 128 || !inode_size.is_power_of_two() || u32::from(inode_size) > block_size {
            return Err(ParseError::InvalidInodeSize);
        }

        let feature_incompat = read_u32(bytes, 96)?;
        let feature_read_only_compat = read_u32(bytes, 100)?;
        if feature_incompat & !SUPPORTED_INCOMPAT_FEATURES != 0 {
            return Err(ParseError::UnsupportedFeature);
        }
        let descriptor_size = if feature_incompat & FEATURE_INCOMPAT_64BIT != 0 {
            read_u16(bytes, 254)?
        } else {
            32
        };
        if descriptor_size < 32
            || !descriptor_size.is_power_of_two()
            || u32::from(descriptor_size) > block_size
        {
            return Err(ParseError::InvalidDescriptorSize);
        }
        let block_count_low = u64::from(read_u32(bytes, 4)?);
        let free_block_count_low = u64::from(read_u32(bytes, 12)?);
        let (block_count_high, free_block_count_high) =
            if feature_incompat & FEATURE_INCOMPAT_64BIT != 0 {
                (
                    u64::from(read_u32(bytes, 336)?),
                    u64::from(read_u32(bytes, 344)?),
                )
            } else {
                (0, 0)
            };

        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&bytes[104..120]);
        let mut volume_name = [0u8; 16];
        volume_name.copy_from_slice(&bytes[120..136]);
        let checksum_type = bytes[373];
        let checksum = read_u32(bytes, 1020)?;
        let checksum_seed = if feature_incompat & FEATURE_INCOMPAT_CHECKSUM_SEED != 0 {
            read_u32(bytes, 624)?
        } else {
            crc32c(u32::MAX, &uuid)
        };
        if feature_read_only_compat & FEATURE_READ_ONLY_COMPAT_METADATA_CHECKSUM != 0 {
            if checksum_type != 1 {
                return Err(ParseError::UnsupportedChecksum);
            }
            if crc32c(u32::MAX, &bytes[..1020]) != checksum {
                return Err(ParseError::InvalidChecksum);
            }
        }
        let inode_count = read_u32(bytes, 0)?;
        let blocks_per_group = read_u32(bytes, 32)?;
        let inodes_per_group = read_u32(bytes, 40)?;
        let block_count = block_count_low | (block_count_high << 32);
        let free_block_count = free_block_count_low | (free_block_count_high << 32);
        let first_data_block = read_u32(bytes, 20)?;
        if inode_count == 0
            || block_count <= u64::from(first_data_block)
            || free_block_count > block_count
            || blocks_per_group == 0
            || inodes_per_group == 0
        {
            return Err(ParseError::InvalidGeometry);
        }
        let state = read_u16(bytes, 58)?;
        if state & FILESYSTEM_STATE_CLEAN == 0 {
            return Err(ParseError::DirtyFilesystem);
        }
        Ok(Self {
            inode_count,
            block_count,
            free_block_count,
            free_inode_count: read_u32(bytes, 16)?,
            first_data_block,
            block_size,
            blocks_per_group,
            inodes_per_group,
            state,
            errors: read_u16(bytes, 60)?,
            revision,
            inode_size,
            descriptor_size,
            feature_compat: read_u32(bytes, 92)?,
            feature_incompat,
            feature_read_only_compat,
            uuid,
            volume_name,
            checksum_type,
            checksum,
            checksum_seed,
            journal_inode: read_u32(bytes, 224)?,
        })
    }

    pub fn volume_name(&self) -> &[u8] {
        let length = self
            .volume_name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.volume_name.len());
        &self.volume_name[..length]
    }

    pub const fn has_metadata_checksums(&self) -> bool {
        self.feature_read_only_compat & FEATURE_READ_ONLY_COMPAT_METADATA_CHECKSUM != 0
    }

    pub const fn group_descriptor_block(&self) -> u64 {
        self.first_data_block as u64 + 1
    }

    pub fn group_count(&self) -> u64 {
        let data_blocks = self
            .block_count
            .saturating_sub(u64::from(self.first_data_block));
        data_blocks.div_ceil(u64::from(self.blocks_per_group))
    }

    pub fn inode_group(&self, inode_number: u32) -> Result<u32, ParseError> {
        if inode_number == 0 || inode_number > self.inode_count {
            return Err(ParseError::InvalidInode);
        }
        Ok((inode_number - 1) / self.inodes_per_group)
    }

    pub fn group_descriptor_location(
        &self,
        group_index: u32,
    ) -> Result<GroupDescriptorLocation, ParseError> {
        if u64::from(group_index) >= self.group_count() {
            return Err(ParseError::InvalidGeometry);
        }
        let byte_offset = u64::from(group_index)
            .checked_mul(u64::from(self.descriptor_size))
            .ok_or(ParseError::InvalidGeometry)?;
        Ok(GroupDescriptorLocation {
            block: self.group_descriptor_block() + byte_offset / u64::from(self.block_size),
            offset: (byte_offset % u64::from(self.block_size)) as u32,
        })
    }

    pub fn inode_location(
        &self,
        inode_number: u32,
        group: &GroupDescriptor,
    ) -> Result<InodeLocation, ParseError> {
        if inode_number == 0 || inode_number > self.inode_count {
            return Err(ParseError::InvalidInode);
        }
        let zero_based = inode_number - 1;
        let group_index = zero_based / self.inodes_per_group;
        if group_index != group.group_index {
            return Err(ParseError::InvalidInode);
        }
        let index_in_group = u64::from(zero_based % self.inodes_per_group);
        let byte_offset = group
            .inode_table_block
            .checked_mul(u64::from(self.block_size))
            .and_then(|start| {
                index_in_group
                    .checked_mul(u64::from(self.inode_size))
                    .and_then(|offset| start.checked_add(offset))
            })
            .ok_or(ParseError::InvalidInode)?;
        Ok(InodeLocation {
            block: byte_offset / u64::from(self.block_size),
            offset: (byte_offset % u64::from(self.block_size)) as u32,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalSuperblock {
    pub block_size: u32,
    pub max_length: u32,
    pub first_log_block: u32,
    pub sequence: u32,
    pub start: u32,
    pub error: u32,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_read_only_compat: u32,
    pub uuid: [u8; 16],
    pub user_count: u32,
}

impl JournalSuperblock {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < JOURNAL_SUPERBLOCK_SIZE {
            return Err(ParseError::Truncated);
        }
        if read_be_u32(bytes, 0)? != JOURNAL_MAGIC || read_be_u32(bytes, 4)? != 4 {
            return Err(ParseError::InvalidMagic);
        }
        if read_be_u32(bytes, 8)? != 0 {
            return Err(ParseError::InvalidJournal);
        }
        let block_size = read_be_u32(bytes, 12)?;
        let max_length = read_be_u32(bytes, 16)?;
        let first_log_block = read_be_u32(bytes, 20)?;
        let start = read_be_u32(bytes, 28)?;
        if block_size < 1024
            || !block_size.is_power_of_two()
            || first_log_block == 0
            || first_log_block >= max_length
            || (start != 0 && (start < first_log_block || start >= max_length))
        {
            return Err(ParseError::InvalidJournal);
        }
        let feature_compat = read_be_u32(bytes, 36)?;
        let feature_incompat = read_be_u32(bytes, 40)?;
        let feature_read_only_compat = read_be_u32(bytes, 44)?;
        if feature_compat != 0 || feature_incompat != 0 || feature_read_only_compat != 0 {
            return Err(ParseError::UnsupportedFeature);
        }
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&bytes[48..64]);
        let user_count = read_be_u32(bytes, 64)?;
        if user_count == 0 {
            return Err(ParseError::InvalidJournal);
        }
        Ok(Self {
            block_size,
            max_length,
            first_log_block,
            sequence: read_be_u32(bytes, 24)?,
            start,
            error: read_be_u32(bytes, 32)?,
            feature_compat,
            feature_incompat,
            feature_read_only_compat,
            uuid,
            user_count,
        })
    }
}

const JOURNAL_TAG_ESCAPE: u16 = 0x0001;
const JOURNAL_TAG_SAME_UUID: u16 = 0x0002;
const JOURNAL_TAG_DELETED: u16 = 0x0004;
const JOURNAL_TAG_LAST: u16 = 0x0008;
const JOURNAL_TAG_SUPPORTED: u16 = JOURNAL_TAG_ESCAPE | JOURNAL_TAG_LAST;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalDescriptor {
    pub sequence: u32,
    pub target_block: u32,
    pub escaped: bool,
    pub uuid: [u8; 16],
}

impl JournalDescriptor {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < 36 {
            return Err(ParseError::Truncated);
        }
        if read_be_u32(bytes, 0)? != JOURNAL_MAGIC
            || read_be_u32(bytes, 4)? != JOURNAL_DESCRIPTOR_BLOCK
        {
            return Err(ParseError::InvalidMagic);
        }
        let sequence = read_be_u32(bytes, 8)?;
        let checksum = read_be_u16(bytes, 16)?;
        let flags = read_be_u16(bytes, 18)?;
        if sequence == 0
            || checksum != 0
            || flags & JOURNAL_TAG_LAST == 0
            || flags & (JOURNAL_TAG_SAME_UUID | JOURNAL_TAG_DELETED) != 0
            || flags & !JOURNAL_TAG_SUPPORTED != 0
        {
            return Err(ParseError::InvalidJournal);
        }
        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&bytes[20..36]);
        Ok(Self {
            sequence,
            target_block: read_be_u32(bytes, 12)?,
            escaped: flags & JOURNAL_TAG_ESCAPE != 0,
            uuid,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JournalCommit {
    pub sequence: u32,
}

impl JournalCommit {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < 12 {
            return Err(ParseError::Truncated);
        }
        if read_be_u32(bytes, 0)? != JOURNAL_MAGIC || read_be_u32(bytes, 4)? != JOURNAL_COMMIT_BLOCK
        {
            return Err(ParseError::InvalidMagic);
        }
        let sequence = read_be_u32(bytes, 8)?;
        if sequence == 0 {
            return Err(ParseError::InvalidJournal);
        }
        Ok(Self { sequence })
    }
}

pub fn encode_single_block_journal_transaction(
    descriptor_block: &mut [u8],
    journal_data_block: &mut [u8],
    commit_block: &mut [u8],
    sequence: u32,
    target_block: u32,
    uuid: &[u8; 16],
    home_block: &[u8],
) -> Result<(), ParseError> {
    if descriptor_block.len() != home_block.len()
        || journal_data_block.len() != home_block.len()
        || commit_block.len() != home_block.len()
        || home_block.len() < 1024
        || !home_block.len().is_power_of_two()
    {
        return Err(ParseError::InvalidJournal);
    }
    if sequence == 0 {
        return Err(ParseError::InvalidJournal);
    }
    descriptor_block.fill(0);
    journal_data_block.copy_from_slice(home_block);
    commit_block.fill(0);

    write_be_u32(descriptor_block, 0, JOURNAL_MAGIC)?;
    write_be_u32(descriptor_block, 4, JOURNAL_DESCRIPTOR_BLOCK)?;
    write_be_u32(descriptor_block, 8, sequence)?;
    write_be_u32(descriptor_block, 12, target_block)?;
    write_be_u16(descriptor_block, 16, 0)?;
    let escaped = home_block[..4] == JOURNAL_MAGIC.to_be_bytes();
    let flags = JOURNAL_TAG_LAST | if escaped { JOURNAL_TAG_ESCAPE } else { 0 };
    write_be_u16(descriptor_block, 18, flags)?;
    descriptor_block[20..36].copy_from_slice(uuid);
    if escaped {
        journal_data_block[..4].fill(0);
    }

    write_be_u32(commit_block, 0, JOURNAL_MAGIC)?;
    write_be_u32(commit_block, 4, JOURNAL_COMMIT_BLOCK)?;
    write_be_u32(commit_block, 8, sequence)?;
    Ok(())
}

pub fn decode_journal_data_block(
    output: &mut [u8],
    journal_data_block: &[u8],
    descriptor: &JournalDescriptor,
) -> Result<(), ParseError> {
    if output.len() != journal_data_block.len() || output.len() < 4 {
        return Err(ParseError::InvalidJournal);
    }
    output.copy_from_slice(journal_data_block);
    if descriptor.escaped {
        output[..4].copy_from_slice(&JOURNAL_MAGIC.to_be_bytes());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupDescriptor {
    pub group_index: u32,
    pub block_bitmap_block: u64,
    pub inode_bitmap_block: u64,
    pub inode_table_block: u64,
    pub free_block_count: u32,
    pub free_inode_count: u32,
    pub used_directory_count: u32,
    pub checksum: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupDescriptorLocation {
    pub block: u64,
    pub offset: u32,
}

impl GroupDescriptor {
    pub fn parse(
        bytes: &[u8],
        group_index: u32,
        superblock: &Superblock,
    ) -> Result<Self, ParseError> {
        let descriptor_size = usize::from(superblock.descriptor_size);
        if bytes.len() < descriptor_size {
            return Err(ParseError::Truncated);
        }
        let descriptor = &bytes[..descriptor_size];
        let checksum = read_u16(descriptor, 30)?;
        if superblock.has_metadata_checksums() {
            let mut computed = crc32c(superblock.checksum_seed, &group_index.to_le_bytes());
            computed = crc32c(computed, &descriptor[..30]);
            computed = crc32c(computed, &[0, 0]);
            computed = crc32c(computed, &descriptor[32..]);
            if computed as u16 != checksum {
                return Err(ParseError::InvalidChecksum);
            }
        }
        let high = if superblock.feature_incompat & FEATURE_INCOMPAT_64BIT != 0 {
            (
                u64::from(read_u32(descriptor, 32)?),
                u64::from(read_u32(descriptor, 36)?),
                u64::from(read_u32(descriptor, 40)?),
            )
        } else {
            (0, 0, 0)
        };
        let inode_table_block = u64::from(read_u32(descriptor, 8)?) | (high.2 << 32);
        if inode_table_block == 0 || inode_table_block >= superblock.block_count {
            return Err(ParseError::InvalidGeometry);
        }
        Ok(Self {
            group_index,
            block_bitmap_block: u64::from(read_u32(descriptor, 0)?) | (high.0 << 32),
            inode_bitmap_block: u64::from(read_u32(descriptor, 4)?) | (high.1 << 32),
            inode_table_block,
            free_block_count: u32::from(read_u16(descriptor, 12)?)
                | (u32::from(read_u16_or_zero(descriptor, 44)) << 16),
            free_inode_count: u32::from(read_u16(descriptor, 14)?)
                | (u32::from(read_u16_or_zero(descriptor, 46)) << 16),
            used_directory_count: u32::from(read_u16(descriptor, 16)?)
                | (u32::from(read_u16_or_zero(descriptor, 48)) << 16),
            checksum,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeLocation {
    pub block: u64,
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Inode {
    pub number: u32,
    pub mode: u16,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub block_count_512: u64,
    pub flags: u32,
    pub generation: u32,
    extent_root: [u8; 60],
}

impl Inode {
    pub fn parse(
        bytes: &[u8],
        inode_number: u32,
        superblock: &Superblock,
    ) -> Result<Self, ParseError> {
        let inode_size = usize::from(superblock.inode_size);
        if inode_number == 0 || inode_number > superblock.inode_count {
            return Err(ParseError::InvalidInode);
        }
        if bytes.len() < inode_size {
            return Err(ParseError::Truncated);
        }
        let inode = &bytes[..inode_size];
        let generation = read_u32(inode, 100)?;
        if superblock.has_metadata_checksums()
            && inode_checksum(inode, inode_number, generation, superblock)?
                != inode_stored_checksum(inode)?
        {
            return Err(ParseError::InvalidChecksum);
        }
        let mode = read_u16(inode, 0)?;
        let flags = read_u32(inode, 32)?;
        let mut extent_root = [0u8; 60];
        extent_root.copy_from_slice(&inode[40..100]);
        Ok(Self {
            number: inode_number,
            mode,
            uid: u32::from(read_u16(inode, 2)?) | (u32::from(read_u16_or_zero(inode, 120)) << 16),
            gid: u32::from(read_u16(inode, 24)?) | (u32::from(read_u16_or_zero(inode, 122)) << 16),
            size: u64::from(read_u32(inode, 4)?) | (u64::from(read_u32(inode, 108)?) << 32),
            block_count_512: u64::from(read_u32(inode, 28)?)
                | (u64::from(read_u16_or_zero(inode, 116)) << 32),
            flags,
            generation,
            extent_root,
        })
    }

    pub const fn is_directory(&self) -> bool {
        self.mode & MODE_TYPE_MASK == DIRECTORY_MODE
    }

    pub const fn is_regular_file(&self) -> bool {
        self.mode & MODE_TYPE_MASK == REGULAR_FILE_MODE
    }

    pub const fn is_symlink(&self) -> bool {
        self.mode & MODE_TYPE_MASK == SYMLINK_MODE
    }

    pub fn inline_symlink(&self) -> Result<&[u8], ParseError> {
        if !self.is_symlink() {
            return Err(ParseError::NotSymlink);
        }
        let length = usize::try_from(self.size).map_err(|_| ParseError::UnsupportedSymlink)?;
        if length == 0 || length > self.extent_root.len() || self.block_count_512 != 0 {
            return Err(ParseError::UnsupportedSymlink);
        }
        let target = &self.extent_root[..length];
        if target.contains(&0) {
            return Err(ParseError::InvalidSymlink);
        }
        Ok(target)
    }

    pub fn first_extent(&self) -> Result<Extent, ParseError> {
        self.extent_at(0)
    }

    pub fn extent_depth(&self) -> Result<u16, ParseError> {
        Ok(self.extent_header()?.depth)
    }

    pub fn extent_for_logical_block(
        &self,
        logical_block: u32,
    ) -> Result<Option<Extent>, ParseError> {
        let header = self.extent_header()?;
        if header.depth != 0 {
            return Err(ParseError::UnsupportedExtentDepth);
        }
        extent_for_logical_block(&self.extent_root, header, logical_block)
    }

    pub fn extent_index_for_logical_block(
        &self,
        logical_block: u32,
    ) -> Result<Option<ExtentIndex>, ParseError> {
        let header = self.extent_header()?;
        if header.depth == 0 {
            return Err(ParseError::InvalidExtent);
        }
        extent_index_for_logical_block(&self.extent_root, header, logical_block)
    }

    fn extent_header(&self) -> Result<ExtentHeader, ParseError> {
        if self.flags & INODE_FLAG_EXTENTS == 0 {
            return Err(ParseError::InvalidExtent);
        }
        parse_extent_header(&self.extent_root, 4)
    }

    fn extent_at(&self, index: usize) -> Result<Extent, ParseError> {
        let header = self.extent_header()?;
        if header.depth != 0 || index >= header.entries {
            return Err(ParseError::InvalidExtent);
        }
        parse_extent(&self.extent_root, index)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Extent {
    pub logical_block: u32,
    pub physical_block: u64,
    pub block_count: u16,
    pub unwritten: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtentIndex {
    pub logical_block: u32,
    pub child_block: u64,
}

pub struct ExtentNode<'a> {
    bytes: &'a [u8],
    header: ExtentHeader,
}

impl<'a> ExtentNode<'a> {
    pub fn parse(
        bytes: &'a [u8],
        expected_depth: u16,
        inode: &Inode,
        superblock: &Superblock,
    ) -> Result<Self, ParseError> {
        let block_size = superblock.block_size as usize;
        if bytes.len() < block_size {
            return Err(ParseError::Truncated);
        }
        if expected_depth >= MAX_EXTENT_DEPTH {
            return Err(ParseError::UnsupportedExtentDepth);
        }
        let bytes = &bytes[..block_size];
        let maximum = (block_size - EXTENT_HEADER_SIZE) / EXTENT_ENTRY_SIZE;
        let header = parse_extent_header(bytes, maximum)?;
        if header.depth != expected_depth {
            return Err(ParseError::InvalidExtent);
        }
        let tail_offset = EXTENT_HEADER_SIZE + header.maximum * EXTENT_ENTRY_SIZE;
        if tail_offset + EXTENT_TAIL_SIZE > block_size {
            return Err(ParseError::InvalidExtent);
        }
        if superblock.has_metadata_checksums() {
            let mut checksum = crc32c(superblock.checksum_seed, &inode.number.to_le_bytes());
            checksum = crc32c(checksum, &inode.generation.to_le_bytes());
            checksum = crc32c(checksum, &bytes[..tail_offset]);
            if read_u32(bytes, tail_offset)? != checksum {
                return Err(ParseError::InvalidChecksum);
            }
        }
        let node = Self { bytes, header };
        node.validate_entries(superblock)?;
        Ok(node)
    }

    pub const fn depth(&self) -> u16 {
        self.header.depth
    }

    pub fn extent_for_logical_block(
        &self,
        logical_block: u32,
    ) -> Result<Option<Extent>, ParseError> {
        if self.header.depth != 0 {
            return Err(ParseError::InvalidExtent);
        }
        extent_for_logical_block(self.bytes, self.header, logical_block)
    }

    pub fn extent_index_for_logical_block(
        &self,
        logical_block: u32,
    ) -> Result<Option<ExtentIndex>, ParseError> {
        if self.header.depth == 0 {
            return Err(ParseError::InvalidExtent);
        }
        extent_index_for_logical_block(self.bytes, self.header, logical_block)
    }

    fn validate_entries(&self, superblock: &Superblock) -> Result<(), ParseError> {
        if self.header.depth == 0 {
            validate_extents(self.bytes, self.header)?;
            for index in 0..self.header.entries {
                let extent = parse_extent(self.bytes, index)?;
                let end = extent
                    .physical_block
                    .checked_add(u64::from(extent.block_count))
                    .ok_or(ParseError::InvalidExtent)?;
                if extent.physical_block == 0 || end > superblock.block_count {
                    return Err(ParseError::InvalidExtent);
                }
            }
        } else {
            validate_extent_indexes(self.bytes, self.header)?;
            for index in 0..self.header.entries {
                let extent_index = parse_extent_index(self.bytes, index)?;
                if extent_index.child_block == 0
                    || extent_index.child_block >= superblock.block_count
                {
                    return Err(ParseError::InvalidExtent);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct ExtentHeader {
    entries: usize,
    maximum: usize,
    depth: u16,
}

fn parse_extent_header(bytes: &[u8], maximum_allowed: usize) -> Result<ExtentHeader, ParseError> {
    if read_u16(bytes, 0)? != EXTENT_HEADER_MAGIC {
        return Err(ParseError::InvalidExtent);
    }
    let entries = usize::from(read_u16(bytes, 2)?);
    let maximum = usize::from(read_u16(bytes, 4)?);
    let depth = read_u16(bytes, 6)?;
    if maximum == 0 || maximum > maximum_allowed || entries > maximum || (depth > 0 && entries == 0)
    {
        return Err(ParseError::InvalidExtent);
    }
    if depth > MAX_EXTENT_DEPTH {
        return Err(ParseError::UnsupportedExtentDepth);
    }
    Ok(ExtentHeader {
        entries,
        maximum,
        depth,
    })
}

fn extent_for_logical_block(
    bytes: &[u8],
    header: ExtentHeader,
    logical_block: u32,
) -> Result<Option<Extent>, ParseError> {
    validate_extents(bytes, header)?;
    for index in 0..header.entries {
        let extent = parse_extent(bytes, index)?;
        let end = extent
            .logical_block
            .checked_add(u32::from(extent.block_count))
            .ok_or(ParseError::InvalidExtent)?;
        if logical_block >= extent.logical_block && logical_block < end {
            return Ok(Some(extent));
        }
    }
    Ok(None)
}

fn validate_extents(bytes: &[u8], header: ExtentHeader) -> Result<(), ParseError> {
    let mut previous_end = 0u32;
    for index in 0..header.entries {
        let extent = parse_extent(bytes, index)?;
        let end = extent
            .logical_block
            .checked_add(u32::from(extent.block_count))
            .ok_or(ParseError::InvalidExtent)?;
        if index != 0 && extent.logical_block < previous_end {
            return Err(ParseError::InvalidExtent);
        }
        previous_end = end;
    }
    Ok(())
}

fn parse_extent(bytes: &[u8], index: usize) -> Result<Extent, ParseError> {
    let offset = EXTENT_HEADER_SIZE + index * EXTENT_ENTRY_SIZE;
    let raw_length = read_u16(bytes, offset + 4)?;
    if raw_length == 0 {
        return Err(ParseError::InvalidExtent);
    }
    let (block_count, unwritten) = if raw_length <= 0x8000 {
        (raw_length, false)
    } else {
        (raw_length - 0x8000, true)
    };
    let physical_block =
        u64::from(read_u32(bytes, offset + 8)?) | (u64::from(read_u16(bytes, offset + 6)?) << 32);
    if physical_block.checked_add(u64::from(block_count)).is_none() {
        return Err(ParseError::InvalidExtent);
    }
    Ok(Extent {
        logical_block: read_u32(bytes, offset)?,
        physical_block,
        block_count,
        unwritten,
    })
}

fn extent_index_for_logical_block(
    bytes: &[u8],
    header: ExtentHeader,
    logical_block: u32,
) -> Result<Option<ExtentIndex>, ParseError> {
    validate_extent_indexes(bytes, header)?;
    let mut selected = None;
    for index in 0..header.entries {
        let extent_index = parse_extent_index(bytes, index)?;
        if extent_index.logical_block > logical_block {
            break;
        }
        selected = Some(extent_index);
    }
    Ok(selected)
}

fn validate_extent_indexes(bytes: &[u8], header: ExtentHeader) -> Result<(), ParseError> {
    let mut previous = 0u32;
    for index in 0..header.entries {
        let extent_index = parse_extent_index(bytes, index)?;
        if index != 0 && extent_index.logical_block <= previous {
            return Err(ParseError::InvalidExtent);
        }
        previous = extent_index.logical_block;
    }
    Ok(())
}

fn parse_extent_index(bytes: &[u8], index: usize) -> Result<ExtentIndex, ParseError> {
    let offset = EXTENT_HEADER_SIZE + index * EXTENT_ENTRY_SIZE;
    Ok(ExtentIndex {
        logical_block: read_u32(bytes, offset)?,
        child_block: u64::from(read_u32(bytes, offset + 4)?)
            | (u64::from(read_u16(bytes, offset + 8)?) << 32),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryEntry<'a> {
    pub inode: u32,
    pub file_type: u8,
    pub name: &'a [u8],
}

pub struct DirectoryBlock<'a> {
    bytes: &'a [u8],
    entries_end: usize,
    entry_count: usize,
}

impl<'a> DirectoryBlock<'a> {
    pub fn parse(
        bytes: &'a [u8],
        inode: &Inode,
        superblock: &Superblock,
    ) -> Result<Self, ParseError> {
        if !inode.is_directory() {
            return Err(ParseError::NotDirectory);
        }
        if inode.flags & INODE_FLAG_DIRECTORY_INDEX != 0 {
            return Err(ParseError::UnsupportedDirectoryIndex);
        }
        if bytes.len() != superblock.block_size as usize {
            return Err(ParseError::InvalidDirectory);
        }
        let entries_end = if superblock.has_metadata_checksums() {
            if bytes.len() < 12 {
                return Err(ParseError::Truncated);
            }
            let tail = bytes.len() - 12;
            if read_u32(bytes, tail)? != 0
                || read_u16(bytes, tail + 4)? != 12
                || bytes[tail + 6] != 0
                || bytes[tail + 7] != DIRECTORY_CHECKSUM_FILE_TYPE
            {
                return Err(ParseError::InvalidDirectory);
            }
            let mut checksum = crc32c(superblock.checksum_seed, &inode.number.to_le_bytes());
            checksum = crc32c(checksum, &inode.generation.to_le_bytes());
            checksum = crc32c(checksum, &bytes[..tail]);
            if checksum != read_u32(bytes, bytes.len() - 4)? {
                return Err(ParseError::InvalidChecksum);
            }
            tail
        } else {
            bytes.len()
        };
        let entry_count = validate_directory_entries(bytes, entries_end)?;
        Ok(Self {
            bytes,
            entries_end,
            entry_count,
        })
    }

    pub const fn entry_count(&self) -> usize {
        self.entry_count
    }

    pub fn find(&self, name: &[u8]) -> Option<DirectoryEntry<'a>> {
        let mut offset = 0;
        while offset < self.entries_end {
            let inode = read_u32(self.bytes, offset).ok()?;
            let record_length = usize::from(read_u16(self.bytes, offset + 4).ok()?);
            let name_length = usize::from(self.bytes[offset + 6]);
            if inode != 0 && &self.bytes[offset + 8..offset + 8 + name_length] == name {
                return Some(DirectoryEntry {
                    inode,
                    file_type: self.bytes[offset + 7],
                    name: &self.bytes[offset + 8..offset + 8 + name_length],
                });
            }
            offset += record_length;
        }
        None
    }
}

fn validate_directory_entries(bytes: &[u8], entries_end: usize) -> Result<usize, ParseError> {
    let mut offset = 0;
    let mut count = 0;
    while offset < entries_end {
        if offset + 8 > entries_end {
            return Err(ParseError::InvalidDirectory);
        }
        let inode = read_u32(bytes, offset)?;
        let record_length = usize::from(read_u16(bytes, offset + 4)?);
        let name_length = usize::from(bytes[offset + 6]);
        if record_length < 8
            || record_length & 3 != 0
            || offset + record_length > entries_end
            || name_length > record_length - 8
        {
            return Err(ParseError::InvalidDirectory);
        }
        if inode != 0 {
            count += 1;
        }
        offset += record_length;
    }
    if offset != entries_end {
        return Err(ParseError::InvalidDirectory);
    }
    Ok(count)
}

fn inode_stored_checksum(inode: &[u8]) -> Result<u32, ParseError> {
    let low = u32::from(read_u16(inode, 124)?);
    let high = if inode.len() > 131 && read_u16(inode, 128)? >= 4 {
        u32::from(read_u16(inode, 130)?)
    } else {
        0
    };
    Ok(low | (high << 16))
}

fn inode_checksum(
    inode: &[u8],
    inode_number: u32,
    generation: u32,
    superblock: &Superblock,
) -> Result<u32, ParseError> {
    if inode.len() < 126 {
        return Err(ParseError::Truncated);
    }
    let has_high = inode.len() > 131 && read_u16(inode, 128)? >= 4;
    let mut checksum = crc32c(superblock.checksum_seed, &inode_number.to_le_bytes());
    checksum = crc32c(checksum, &generation.to_le_bytes());
    checksum = crc32c(checksum, &inode[..124]);
    checksum = crc32c(checksum, &[0, 0]);
    if has_high {
        checksum = crc32c(checksum, &inode[126..130]);
        checksum = crc32c(checksum, &[0, 0]);
        checksum = crc32c(checksum, &inode[132..]);
    } else {
        checksum = crc32c(checksum, &inode[126..]);
    }
    Ok(checksum)
}

fn crc32c(seed: u32, bytes: &[u8]) -> u32 {
    let mut checksum = seed;
    for byte in bytes {
        checksum ^= u32::from(*byte);
        for _ in 0..8 {
            checksum = if checksum & 1 != 0 {
                (checksum >> 1) ^ 0x82f6_3b78
            } else {
                checksum >> 1
            };
        }
    }
    checksum
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ParseError> {
    let value = bytes.get(offset..offset + 2).ok_or(ParseError::Truncated)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    let value = bytes.get(offset..offset + 4).ok_or(ParseError::Truncated)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    let value = bytes.get(offset..offset + 4).ok_or(ParseError::Truncated)?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_be_u16(bytes: &[u8], offset: usize) -> Result<u16, ParseError> {
    let value = bytes.get(offset..offset + 2).ok_or(ParseError::Truncated)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn write_be_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), ParseError> {
    bytes
        .get_mut(offset..offset + 2)
        .ok_or(ParseError::Truncated)?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn write_be_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), ParseError> {
    bytes
        .get_mut(offset..offset + 4)
        .ok_or(ParseError::Truncated)?
        .copy_from_slice(&value.to_be_bytes());
    Ok(())
}

fn read_u16_or_zero(bytes: &[u8], offset: usize) -> u16 {
    read_u16(bytes, offset).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_superblock() -> [u8; SUPERBLOCK_SIZE] {
        let mut bytes = [0u8; SUPERBLOCK_SIZE];
        bytes[0..4].copy_from_slice(&16384u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&32768u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&30000u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&8000u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&2u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&32768u32.to_le_bytes());
        bytes[40..44].copy_from_slice(&8192u32.to_le_bytes());
        bytes[56..58].copy_from_slice(&SUPERBLOCK_MAGIC.to_le_bytes());
        bytes[58..60].copy_from_slice(&1u16.to_le_bytes());
        bytes[60..62].copy_from_slice(&1u16.to_le_bytes());
        bytes[76..80].copy_from_slice(&1u32.to_le_bytes());
        bytes[88..90].copy_from_slice(&256u16.to_le_bytes());
        bytes[92..96].copy_from_slice(&0x3cu32.to_le_bytes());
        bytes[96..100].copy_from_slice(&FEATURE_INCOMPAT_64BIT.to_le_bytes());
        bytes[100..104].copy_from_slice(&0x46bu32.to_le_bytes());
        bytes[104..120].copy_from_slice(&[0x53; 16]);
        bytes[120..131].copy_from_slice(b"SLOPOS_ROOT");
        bytes[224..228].copy_from_slice(&JOURNAL_INODE.to_le_bytes());
        bytes[254..256].copy_from_slice(&64u16.to_le_bytes());
        bytes[336..340].copy_from_slice(&3u32.to_le_bytes());
        bytes[344..348].copy_from_slice(&2u32.to_le_bytes());
        bytes[373] = 1;
        let checksum = crc32c(u32::MAX, &bytes[..1020]);
        bytes[1020..1024].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    fn valid_group_descriptor(superblock: &Superblock) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[0..4].copy_from_slice(&17u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&33u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&49u32.to_le_bytes());
        bytes[12..14].copy_from_slice(&26_563u16.to_le_bytes());
        bytes[14..16].copy_from_slice(&32_752u16.to_le_bytes());
        bytes[16..18].copy_from_slice(&4u16.to_le_bytes());
        let mut checksum = crc32c(superblock.checksum_seed, &0u32.to_le_bytes());
        checksum = crc32c(checksum, &bytes[..30]);
        checksum = crc32c(checksum, &[0, 0]);
        checksum = crc32c(checksum, &bytes[32..]);
        bytes[30..32].copy_from_slice(&(checksum as u16).to_le_bytes());
        bytes
    }

    fn valid_root_inode(superblock: &Superblock) -> [u8; 256] {
        let mut bytes = [0u8; 256];
        bytes[0..2].copy_from_slice(&0x41edu16.to_le_bytes());
        bytes[4..8].copy_from_slice(&4096u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&4u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&8u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&INODE_FLAG_EXTENTS.to_le_bytes());
        bytes[40..42].copy_from_slice(&EXTENT_HEADER_MAGIC.to_le_bytes());
        bytes[42..44].copy_from_slice(&1u16.to_le_bytes());
        bytes[44..46].copy_from_slice(&4u16.to_le_bytes());
        bytes[52..56].copy_from_slice(&0u32.to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[58..60].copy_from_slice(&0u16.to_le_bytes());
        bytes[60..64].copy_from_slice(&18u32.to_le_bytes());
        bytes[100..104].copy_from_slice(&7u32.to_le_bytes());
        bytes[128..130].copy_from_slice(&32u16.to_le_bytes());
        let checksum = inode_checksum(&bytes, ROOT_INODE, 7, superblock).unwrap();
        bytes[124..126].copy_from_slice(&(checksum as u16).to_le_bytes());
        bytes[130..132].copy_from_slice(&((checksum >> 16) as u16).to_le_bytes());
        bytes
    }

    fn valid_root_directory(superblock: &Superblock, inode: &Inode) -> [u8; 4096] {
        let mut bytes = [0u8; 4096];
        write_directory_entry(&mut bytes, 0, ROOT_INODE, 12, 2, b".");
        write_directory_entry(&mut bytes, 12, ROOT_INODE, 12, 2, b"..");
        write_directory_entry(&mut bytes, 24, 13, 4060, 2, b"etc");
        let tail = bytes.len() - 12;
        bytes[tail + 4..tail + 6].copy_from_slice(&12u16.to_le_bytes());
        bytes[tail + 7] = DIRECTORY_CHECKSUM_FILE_TYPE;
        let mut checksum = crc32c(superblock.checksum_seed, &inode.number.to_le_bytes());
        checksum = crc32c(checksum, &inode.generation.to_le_bytes());
        checksum = crc32c(checksum, &bytes[..tail]);
        bytes[tail + 8..tail + 12].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    fn write_directory_entry(
        bytes: &mut [u8],
        offset: usize,
        inode: u32,
        record_length: u16,
        file_type: u8,
        name: &[u8],
    ) {
        bytes[offset..offset + 4].copy_from_slice(&inode.to_le_bytes());
        bytes[offset + 4..offset + 6].copy_from_slice(&record_length.to_le_bytes());
        bytes[offset + 6] = name.len() as u8;
        bytes[offset + 7] = file_type;
        bytes[offset + 8..offset + 8 + name.len()].copy_from_slice(name);
    }

    fn refresh_superblock_checksum(bytes: &mut [u8; SUPERBLOCK_SIZE]) {
        let checksum = crc32c(u32::MAX, &bytes[..1020]);
        bytes[1020..1024].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn parses_geometry_features_and_high_counts() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        assert_eq!(superblock.block_size, 4096);
        assert_eq!(superblock.block_count, (3u64 << 32) | 32768);
        assert_eq!(superblock.free_block_count, (2u64 << 32) | 30000);
        assert_eq!(superblock.inode_size, 256);
        assert_eq!(superblock.descriptor_size, 64);
        assert_eq!(superblock.journal_inode, JOURNAL_INODE);
        assert_eq!(superblock.volume_name(), b"SLOPOS_ROOT");
        assert_eq!(
            superblock.checksum,
            crc32c(u32::MAX, &valid_superblock()[..1020])
        );
    }

    #[test]
    fn rejects_bad_magic_and_geometry() {
        let mut bytes = valid_superblock();
        bytes[56] = 0;
        assert_eq!(Superblock::parse(&bytes), Err(ParseError::InvalidMagic));
        let mut bytes = valid_superblock();
        bytes[24..28].copy_from_slice(&7u32.to_le_bytes());
        assert_eq!(Superblock::parse(&bytes), Err(ParseError::InvalidBlockSize));
        let mut bytes = valid_superblock();
        bytes[88..90].copy_from_slice(&192u16.to_le_bytes());
        assert_eq!(Superblock::parse(&bytes), Err(ParseError::InvalidInodeSize));
    }

    #[test]
    fn safely_rejects_unsupported_or_dirty_filesystems() {
        let mut bytes = valid_superblock();
        bytes[96..100].copy_from_slice(&(FEATURE_INCOMPAT_64BIT | 0x0008).to_le_bytes());
        assert_eq!(
            Superblock::parse(&bytes),
            Err(ParseError::UnsupportedFeature)
        );

        let mut bytes = valid_superblock();
        bytes[58..60].copy_from_slice(&0u16.to_le_bytes());
        refresh_superblock_checksum(&mut bytes);
        assert_eq!(Superblock::parse(&bytes), Err(ParseError::DirtyFilesystem));
    }

    #[test]
    fn validates_path_components_without_normalizing_them() {
        assert_eq!(validate_path_component(b"system.conf"), Ok(()));
        assert_eq!(
            validate_path_component(b""),
            Err(ParseError::InvalidPathComponent)
        );
        assert_eq!(
            validate_path_component(b".."),
            Err(ParseError::InvalidPathComponent)
        );
        assert_eq!(
            validate_path_component(b"etc/slopos"),
            Err(ParseError::InvalidPathComponent)
        );
        assert_eq!(
            validate_path_component(b"nul\0byte"),
            Err(ParseError::InvalidPathComponent)
        );
        assert_eq!(
            validate_path_component(&[b'a'; 256]),
            Err(ParseError::InvalidPathComponent)
        );
    }

    #[test]
    fn requires_complete_superblock() {
        assert_eq!(Superblock::parse(&[0u8; 512]), Err(ParseError::Truncated));
    }

    #[test]
    fn parses_big_endian_journal_superblock() {
        let mut bytes = [0u8; JOURNAL_SUPERBLOCK_SIZE];
        bytes[0..4].copy_from_slice(&JOURNAL_MAGIC.to_be_bytes());
        bytes[4..8].copy_from_slice(&4u32.to_be_bytes());
        bytes[12..16].copy_from_slice(&4096u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&4096u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_be_bytes());
        bytes[24..28].copy_from_slice(&7u32.to_be_bytes());
        bytes[28..32].copy_from_slice(&0u32.to_be_bytes());
        bytes[48..64].copy_from_slice(&[0x53; 16]);
        bytes[64..68].copy_from_slice(&1u32.to_be_bytes());

        let journal = JournalSuperblock::parse(&bytes).unwrap();
        assert_eq!(journal.block_size, 4096);
        assert_eq!(journal.max_length, 4096);
        assert_eq!(journal.first_log_block, 1);
        assert_eq!(journal.sequence, 7);
        assert_eq!(journal.start, 0);
        assert_eq!(journal.uuid, [0x53; 16]);
    }

    #[test]
    fn rejects_invalid_or_unsupported_journals() {
        assert_eq!(
            JournalSuperblock::parse(&[0u8; 512]),
            Err(ParseError::Truncated)
        );
        let mut bytes = [0u8; JOURNAL_SUPERBLOCK_SIZE];
        bytes[0..4].copy_from_slice(&JOURNAL_MAGIC.to_be_bytes());
        bytes[4..8].copy_from_slice(&4u32.to_be_bytes());
        bytes[12..16].copy_from_slice(&4096u32.to_be_bytes());
        bytes[16..20].copy_from_slice(&4096u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_be_bytes());
        bytes[48..64].copy_from_slice(&[0x53; 16]);
        bytes[64..68].copy_from_slice(&1u32.to_be_bytes());
        bytes[8..12].copy_from_slice(&1u32.to_be_bytes());
        assert_eq!(
            JournalSuperblock::parse(&bytes),
            Err(ParseError::InvalidJournal)
        );
        bytes[8..12].fill(0);
        bytes[40..44].copy_from_slice(&0x10u32.to_be_bytes());
        assert_eq!(
            JournalSuperblock::parse(&bytes),
            Err(ParseError::UnsupportedFeature)
        );
        bytes[40..44].fill(0);
        bytes[20..24].copy_from_slice(&4096u32.to_be_bytes());
        assert_eq!(
            JournalSuperblock::parse(&bytes),
            Err(ParseError::InvalidJournal)
        );
    }

    #[test]
    fn round_trips_single_block_journal_transaction() {
        let mut descriptor = [0u8; 4096];
        let mut journal_data = [0u8; 4096];
        let mut commit = [0u8; 4096];
        let mut home = [b'M'; 4096];
        let uuid = [0x53; 16];
        encode_single_block_journal_transaction(
            &mut descriptor,
            &mut journal_data,
            &mut commit,
            9,
            37,
            &uuid,
            &home,
        )
        .unwrap();
        let parsed_descriptor = JournalDescriptor::parse(&descriptor).unwrap();
        assert_eq!(
            parsed_descriptor,
            JournalDescriptor {
                sequence: 9,
                target_block: 37,
                escaped: false,
                uuid,
            }
        );
        assert_eq!(JournalCommit::parse(&commit).unwrap().sequence, 9);
        let mut decoded = [0u8; 4096];
        decode_journal_data_block(&mut decoded, &journal_data, &parsed_descriptor).unwrap();
        assert_eq!(decoded, home);

        home[..4].copy_from_slice(&JOURNAL_MAGIC.to_be_bytes());
        encode_single_block_journal_transaction(
            &mut descriptor,
            &mut journal_data,
            &mut commit,
            10,
            38,
            &uuid,
            &home,
        )
        .unwrap();
        let parsed_descriptor = JournalDescriptor::parse(&descriptor).unwrap();
        assert!(parsed_descriptor.escaped);
        assert_eq!(journal_data[..4], [0; 4]);
        decode_journal_data_block(&mut decoded, &journal_data, &parsed_descriptor).unwrap();
        assert_eq!(decoded, home);
    }

    #[test]
    fn rejects_corrupted_journal_transaction_headers() {
        let mut descriptor = [0u8; 4096];
        let mut journal_data = [0u8; 4096];
        let mut commit = [0u8; 4096];
        encode_single_block_journal_transaction(
            &mut descriptor,
            &mut journal_data,
            &mut commit,
            9,
            37,
            &[0x53; 16],
            &[b'M'; 4096],
        )
        .unwrap();
        descriptor[18..20].copy_from_slice(&JOURNAL_TAG_SAME_UUID.to_be_bytes());
        assert_eq!(
            JournalDescriptor::parse(&descriptor),
            Err(ParseError::InvalidJournal)
        );
        commit[8..12].fill(0);
        assert_eq!(
            JournalCommit::parse(&commit),
            Err(ParseError::InvalidJournal)
        );
        assert_eq!(
            encode_single_block_journal_transaction(
                &mut descriptor,
                &mut journal_data,
                &mut commit,
                1,
                37,
                &[0x53; 16],
                &[0u8; 2048],
            ),
            Err(ParseError::InvalidJournal)
        );
    }

    #[test]
    fn rejects_corrupted_metadata_checksum() {
        let mut bytes = valid_superblock();
        bytes[120] ^= 1;
        assert_eq!(Superblock::parse(&bytes), Err(ParseError::InvalidChecksum));
    }

    #[test]
    fn validates_group_and_locates_root_inode() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        assert_eq!(superblock.inode_group(ROOT_INODE), Ok(0));
        assert_eq!(superblock.inode_group(8193), Ok(1));
        assert_eq!(
            superblock.group_descriptor_location(0),
            Ok(GroupDescriptorLocation {
                block: 1,
                offset: 0
            })
        );
        assert_eq!(
            superblock.group_descriptor_location(1),
            Ok(GroupDescriptorLocation {
                block: 1,
                offset: 64
            })
        );
        let descriptor = valid_group_descriptor(&superblock);
        let group = GroupDescriptor::parse(&descriptor, 0, &superblock).unwrap();
        assert_eq!(group.inode_table_block, 49);
        assert_eq!(
            superblock.inode_location(ROOT_INODE, &group),
            Ok(InodeLocation {
                block: 49,
                offset: 256
            })
        );
        let mut corrupted = descriptor;
        corrupted[8] ^= 1;
        assert_eq!(
            GroupDescriptor::parse(&corrupted, 0, &superblock),
            Err(ParseError::InvalidChecksum)
        );
    }

    #[test]
    fn validates_root_inode_extent_and_directory_tail() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        let inode_bytes = valid_root_inode(&superblock);
        let inode = Inode::parse(&inode_bytes, ROOT_INODE, &superblock).unwrap();
        assert!(inode.is_directory());
        assert_eq!(
            inode.first_extent(),
            Ok(Extent {
                logical_block: 0,
                physical_block: 18,
                block_count: 1,
                unwritten: false
            })
        );
        let directory_bytes = valid_root_directory(&superblock, &inode);
        let directory = DirectoryBlock::parse(&directory_bytes, &inode, &superblock).unwrap();
        assert_eq!(directory.entry_count(), 3);
        assert_eq!(directory.find(b"etc").unwrap().inode, 13);
        assert_eq!(directory.find(b"missing"), None);

        let mut indexed_inode = inode;
        indexed_inode.flags |= INODE_FLAG_DIRECTORY_INDEX;
        assert!(matches!(
            DirectoryBlock::parse(&directory_bytes, &indexed_inode, &superblock),
            Err(ParseError::UnsupportedDirectoryIndex)
        ));

        let mut corrupted = directory_bytes;
        corrupted[32] ^= 1;
        assert!(matches!(
            DirectoryBlock::parse(&corrupted, &inode, &superblock),
            Err(ParseError::InvalidChecksum)
        ));
    }

    #[test]
    fn rejects_corrupted_inode_checksum() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        let mut inode = valid_root_inode(&superblock);
        inode[60] ^= 1;
        assert_eq!(
            Inode::parse(&inode, ROOT_INODE, &superblock),
            Err(ParseError::InvalidChecksum)
        );
    }

    #[test]
    fn reads_inline_symbolic_link_target() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        let mut inode = valid_root_inode(&superblock);
        let target = b"slopos-release";
        inode[0..2].copy_from_slice(&0xa1ffu16.to_le_bytes());
        inode[4..8].copy_from_slice(&(target.len() as u32).to_le_bytes());
        inode[28..32].copy_from_slice(&0u32.to_le_bytes());
        inode[32..36].copy_from_slice(&0u32.to_le_bytes());
        inode[40..100].fill(0);
        inode[40..40 + target.len()].copy_from_slice(target);
        let checksum = inode_checksum(&inode, ROOT_INODE, 7, &superblock).unwrap();
        inode[124..126].copy_from_slice(&(checksum as u16).to_le_bytes());
        inode[130..132].copy_from_slice(&((checksum >> 16) as u16).to_le_bytes());

        let inode = Inode::parse(&inode, ROOT_INODE, &superblock).unwrap();
        assert!(inode.is_symlink());
        assert_eq!(inode.inline_symlink(), Ok(target.as_slice()));
        assert_eq!(inode.extent_depth(), Err(ParseError::InvalidExtent),);
    }

    #[test]
    fn maps_inline_extent_runs_and_holes() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        let inode_bytes = valid_root_inode(&superblock);
        let mut inode = Inode::parse(&inode_bytes, ROOT_INODE, &superblock).unwrap();
        inode.extent_root[2..4].copy_from_slice(&2u16.to_le_bytes());
        inode.extent_root[16..18].copy_from_slice(&2u16.to_le_bytes());
        inode.extent_root[24..28].copy_from_slice(&4u32.to_le_bytes());
        inode.extent_root[28..30].copy_from_slice(&1u16.to_le_bytes());
        inode.extent_root[30..32].copy_from_slice(&0u16.to_le_bytes());
        inode.extent_root[32..36].copy_from_slice(&30u32.to_le_bytes());

        assert_eq!(
            inode.extent_for_logical_block(1).unwrap(),
            Some(Extent {
                logical_block: 0,
                physical_block: 18,
                block_count: 2,
                unwritten: false
            })
        );
        assert_eq!(inode.extent_for_logical_block(2).unwrap(), None);
        assert_eq!(
            inode.extent_for_logical_block(4).unwrap(),
            Some(Extent {
                logical_block: 4,
                physical_block: 30,
                block_count: 1,
                unwritten: false
            })
        );
    }

    #[test]
    fn decodes_initialized_and_unwritten_extent_lengths() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        let inode_bytes = valid_root_inode(&superblock);
        let mut inode = Inode::parse(&inode_bytes, ROOT_INODE, &superblock).unwrap();
        inode.extent_root[16..18].copy_from_slice(&0x8000u16.to_le_bytes());
        assert_eq!(
            inode.extent_for_logical_block(32_767).unwrap(),
            Some(Extent {
                logical_block: 0,
                physical_block: 18,
                block_count: 32_768,
                unwritten: false,
            })
        );

        inode.extent_root[16..18].copy_from_slice(&0x8001u16.to_le_bytes());
        assert_eq!(
            inode.extent_for_logical_block(0).unwrap(),
            Some(Extent {
                logical_block: 0,
                physical_block: 18,
                block_count: 1,
                unwritten: true,
            })
        );
    }

    #[test]
    fn traverses_checksummed_depth_one_extent_leaf() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        let inode_bytes = valid_root_inode(&superblock);
        let mut inode = Inode::parse(&inode_bytes, ROOT_INODE, &superblock).unwrap();
        inode.extent_root[2..4].copy_from_slice(&1u16.to_le_bytes());
        inode.extent_root[6..8].copy_from_slice(&1u16.to_le_bytes());
        inode.extent_root[12..16].copy_from_slice(&0u32.to_le_bytes());
        inode.extent_root[16..20].copy_from_slice(&99u32.to_le_bytes());
        inode.extent_root[20..22].copy_from_slice(&0u16.to_le_bytes());

        assert_eq!(inode.extent_depth(), Ok(1));
        assert_eq!(
            inode.extent_index_for_logical_block(8),
            Ok(Some(ExtentIndex {
                logical_block: 0,
                child_block: 99,
            }))
        );
        assert_eq!(
            inode.extent_for_logical_block(8),
            Err(ParseError::UnsupportedExtentDepth)
        );

        let mut leaf = [0u8; 4096];
        leaf[0..2].copy_from_slice(&EXTENT_HEADER_MAGIC.to_le_bytes());
        leaf[2..4].copy_from_slice(&5u16.to_le_bytes());
        leaf[4..6].copy_from_slice(&340u16.to_le_bytes());
        for (index, logical_block) in [0u32, 2, 4, 6, 8].into_iter().enumerate() {
            let offset = EXTENT_HEADER_SIZE + index * EXTENT_ENTRY_SIZE;
            leaf[offset..offset + 4].copy_from_slice(&logical_block.to_le_bytes());
            leaf[offset + 4..offset + 6].copy_from_slice(&1u16.to_le_bytes());
            leaf[offset + 8..offset + 12]
                .copy_from_slice(&(100 + u64::from(logical_block)).to_le_bytes()[..4]);
        }
        let tail_offset = EXTENT_HEADER_SIZE + 340 * EXTENT_ENTRY_SIZE;
        let mut checksum = crc32c(superblock.checksum_seed, &inode.number.to_le_bytes());
        checksum = crc32c(checksum, &inode.generation.to_le_bytes());
        checksum = crc32c(checksum, &leaf[..tail_offset]);
        leaf[tail_offset..tail_offset + 4].copy_from_slice(&checksum.to_le_bytes());

        let node = ExtentNode::parse(&leaf, 0, &inode, &superblock).unwrap();
        assert_eq!(node.depth(), 0);
        assert_eq!(node.extent_for_logical_block(7), Ok(None));
        assert_eq!(
            node.extent_for_logical_block(8),
            Ok(Some(Extent {
                logical_block: 8,
                physical_block: 108,
                block_count: 1,
                unwritten: false,
            }))
        );

        leaf[tail_offset] ^= 1;
        assert!(matches!(
            ExtentNode::parse(&leaf, 0, &inode, &superblock),
            Err(ParseError::InvalidChecksum)
        ));
    }
}
