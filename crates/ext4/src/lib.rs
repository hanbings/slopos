// SPDX-License-Identifier: 0BSD

#![no_std]

pub const SUPERBLOCK_SIZE: usize = 1024;
pub const SUPERBLOCK_MAGIC: u16 = 0xef53;
pub const FEATURE_INCOMPAT_64BIT: u32 = 0x0080;
pub const FEATURE_READ_ONLY_COMPAT_METADATA_CHECKSUM: u32 = 0x0400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    Truncated,
    InvalidMagic,
    InvalidBlockSize,
    InvalidInodeSize,
    InvalidDescriptorSize,
    UnsupportedChecksum,
    InvalidChecksum,
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
        if feature_read_only_compat & FEATURE_READ_ONLY_COMPAT_METADATA_CHECKSUM != 0 {
            if checksum_type != 1 {
                return Err(ParseError::UnsupportedChecksum);
            }
            if crc32c(u32::MAX, &bytes[..1020]) != checksum {
                return Err(ParseError::InvalidChecksum);
            }
        }
        Ok(Self {
            inode_count: read_u32(bytes, 0)?,
            block_count: block_count_low | (block_count_high << 32),
            free_block_count: free_block_count_low | (free_block_count_high << 32),
            free_inode_count: read_u32(bytes, 16)?,
            first_data_block: read_u32(bytes, 20)?,
            block_size,
            blocks_per_group: read_u32(bytes, 32)?,
            inodes_per_group: read_u32(bytes, 40)?,
            state: read_u16(bytes, 58)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_superblock() -> [u8; SUPERBLOCK_SIZE] {
        let mut bytes = [0u8; SUPERBLOCK_SIZE];
        bytes[0..4].copy_from_slice(&8192u32.to_le_bytes());
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
        bytes[254..256].copy_from_slice(&64u16.to_le_bytes());
        bytes[336..340].copy_from_slice(&1u32.to_le_bytes());
        bytes[344..348].copy_from_slice(&2u32.to_le_bytes());
        bytes[373] = 1;
        let checksum = crc32c(u32::MAX, &bytes[..1020]);
        bytes[1020..1024].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn parses_geometry_features_and_high_counts() {
        let superblock = Superblock::parse(&valid_superblock()).unwrap();
        assert_eq!(superblock.block_size, 4096);
        assert_eq!(superblock.block_count, (1u64 << 32) | 32768);
        assert_eq!(superblock.free_block_count, (2u64 << 32) | 30000);
        assert_eq!(superblock.inode_size, 256);
        assert_eq!(superblock.descriptor_size, 64);
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
    fn requires_complete_superblock() {
        assert_eq!(Superblock::parse(&[0u8; 512]), Err(ParseError::Truncated));
    }

    #[test]
    fn rejects_corrupted_metadata_checksum() {
        let mut bytes = valid_superblock();
        bytes[120] ^= 1;
        assert_eq!(Superblock::parse(&bytes), Err(ParseError::InvalidChecksum));
    }
}
