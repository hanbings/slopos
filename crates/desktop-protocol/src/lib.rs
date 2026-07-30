// SPDX-License-Identifier: 0BSD

#![no_std]

pub const DESKTOP_COMMIT_SYSCALL: u64 = 0x534c_0001;
pub const DESKTOP_PROTOCOL_MAGIC: u64 = 0x534c_4f50_4445_534b;
pub const DESKTOP_PROTOCOL_VERSION: u16 = 1;
pub const CAPABILITY_WAYBAR_PROVIDER: u32 = 1 << 0;
pub const CAPABILITY_SWWW_POLICY: u32 = 1 << 1;
pub const REQUIRED_CAPABILITIES: u32 = CAPABILITY_WAYBAR_PROVIDER | CAPABILITY_SWWW_POLICY;
pub const WALLPAPER_AURORA: u8 = 1;
pub const COMMIT_SIZE: usize = 40;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DesktopCommit {
    pub magic: u64,
    pub version: u16,
    pub size: u16,
    pub capabilities: u32,
    pub waybar_hash: u64,
    pub swww_hash: u64,
    pub cpu_usage: u8,
    pub memory_percentage: u8,
    pub wallpaper: u8,
    pub reserved: [u8; 5],
}

impl DesktopCommit {
    pub const fn new(
        waybar_hash: u64,
        swww_hash: u64,
        cpu_usage: u8,
        memory_percentage: u8,
        wallpaper: u8,
    ) -> Self {
        Self {
            magic: DESKTOP_PROTOCOL_MAGIC,
            version: DESKTOP_PROTOCOL_VERSION,
            size: COMMIT_SIZE as u16,
            capabilities: REQUIRED_CAPABILITIES,
            waybar_hash,
            swww_hash,
            cpu_usage,
            memory_percentage,
            wallpaper,
            reserved: [0; 5],
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != COMMIT_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        let commit = Self {
            magic: u64::from_le_bytes(
                bytes[0..8]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            version: u16::from_le_bytes(
                bytes[8..10]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            size: u16::from_le_bytes(
                bytes[10..12]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            capabilities: u32::from_le_bytes(
                bytes[12..16]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            waybar_hash: u64::from_le_bytes(
                bytes[16..24]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            swww_hash: u64::from_le_bytes(
                bytes[24..32]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            cpu_usage: bytes[32],
            memory_percentage: bytes[33],
            wallpaper: bytes[34],
            reserved: bytes[35..40]
                .try_into()
                .map_err(|_| ProtocolError::InvalidSize)?,
        };
        commit.validate()?;
        Ok(commit)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.magic != DESKTOP_PROTOCOL_MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        if self.version != DESKTOP_PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion);
        }
        if usize::from(self.size) != COMMIT_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        if self.capabilities != REQUIRED_CAPABILITIES {
            return Err(ProtocolError::InvalidCapabilities);
        }
        if self.waybar_hash == 0 || self.swww_hash == 0 {
            return Err(ProtocolError::InvalidConfigHash);
        }
        if self.cpu_usage > 100 || self.memory_percentage > 100 {
            return Err(ProtocolError::InvalidProviderValue);
        }
        if self.wallpaper != WALLPAPER_AURORA {
            return Err(ProtocolError::InvalidWallpaper);
        }
        if self.reserved != [0; 5] {
            return Err(ProtocolError::ReservedBits);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    InvalidMagic,
    InvalidVersion,
    InvalidSize,
    InvalidCapabilities,
    InvalidConfigHash,
    InvalidProviderValue,
    InvalidWallpaper,
    ReservedBits,
}

pub const fn config_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut index = 0usize;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        index += 1;
    }
    hash
}

const _: () = assert!(core::mem::size_of::<DesktopCommit>() == COMMIT_SIZE);

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(commit: DesktopCommit) -> [u8; COMMIT_SIZE] {
        let mut bytes = [0; COMMIT_SIZE];
        bytes[0..8].copy_from_slice(&commit.magic.to_le_bytes());
        bytes[8..10].copy_from_slice(&commit.version.to_le_bytes());
        bytes[10..12].copy_from_slice(&commit.size.to_le_bytes());
        bytes[12..16].copy_from_slice(&commit.capabilities.to_le_bytes());
        bytes[16..24].copy_from_slice(&commit.waybar_hash.to_le_bytes());
        bytes[24..32].copy_from_slice(&commit.swww_hash.to_le_bytes());
        bytes[32] = commit.cpu_usage;
        bytes[33] = commit.memory_percentage;
        bytes[34] = commit.wallpaper;
        bytes[35..40].copy_from_slice(&commit.reserved);
        bytes
    }

    #[test]
    fn decodes_a_versioned_desktop_commit() {
        let commit = DesktopCommit::new(
            config_hash(b"waybar"),
            config_hash(b"swww"),
            7,
            36,
            WALLPAPER_AURORA,
        );
        assert_eq!(DesktopCommit::decode(&encode(commit)), Ok(commit));
    }

    #[test]
    fn rejects_capability_and_value_drift() {
        let mut commit = DesktopCommit::new(1, 2, 0, 36, WALLPAPER_AURORA);
        commit.capabilities = CAPABILITY_WAYBAR_PROVIDER;
        assert_eq!(
            DesktopCommit::decode(&encode(commit)),
            Err(ProtocolError::InvalidCapabilities)
        );
        commit.capabilities = REQUIRED_CAPABILITIES;
        commit.memory_percentage = 101;
        assert_eq!(
            DesktopCommit::decode(&encode(commit)),
            Err(ProtocolError::InvalidProviderValue)
        );
    }

    #[test]
    fn hashes_configuration_bytes_deterministically() {
        assert_eq!(config_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(config_hash(b"SlopOS"), 0x1437_a4b8_2712_245f);
        assert_ne!(config_hash(b"waybar"), config_hash(b"swww"));
    }
}
