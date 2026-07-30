// SPDX-License-Identifier: 0BSD

#![no_std]

pub const DESKTOP_COMMIT_SYSCALL: u64 = 0x534c_0001;
pub const DESKTOP_WAIT_SYSCALL: u64 = 0x534c_0002;
pub const DESKTOP_PROTOCOL_MAGIC: u64 = 0x534c_4f50_4445_534b;
pub const DESKTOP_PROTOCOL_VERSION: u16 = 1;
pub const CAPABILITY_WAYBAR_PROVIDER: u32 = 1 << 0;
pub const CAPABILITY_SWWW_POLICY: u32 = 1 << 1;
pub const REQUIRED_CAPABILITIES: u32 = CAPABILITY_WAYBAR_PROVIDER | CAPABILITY_SWWW_POLICY;
pub const WALLPAPER_AURORA: u8 = 1;
pub const COMMIT_SIZE: usize = 40;
pub const EVENT_SIZE: usize = 32;
pub const EVENT_POLICY_APPLIED: u16 = 1;
pub const EVENT_CONFIG_APPLIED: u16 = 2;
pub const CONFIG_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const CONFIG_HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

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

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DesktopServiceEvent {
    pub magic: u64,
    pub version: u16,
    pub size: u16,
    pub kind: u16,
    pub flags: u16,
    pub generation: u64,
    pub capabilities: u32,
    pub reserved: [u8; 4],
}

impl DesktopServiceEvent {
    pub const fn policy_applied(generation: u64) -> Self {
        Self::new(EVENT_POLICY_APPLIED, generation)
    }

    pub const fn config_applied(generation: u64) -> Self {
        Self::new(EVENT_CONFIG_APPLIED, generation)
    }

    const fn new(kind: u16, generation: u64) -> Self {
        Self {
            magic: DESKTOP_PROTOCOL_MAGIC,
            version: DESKTOP_PROTOCOL_VERSION,
            size: EVENT_SIZE as u16,
            kind,
            flags: 0,
            generation,
            capabilities: REQUIRED_CAPABILITIES,
            reserved: [0; 4],
        }
    }

    pub fn encode(self) -> [u8; EVENT_SIZE] {
        let mut bytes = [0; EVENT_SIZE];
        bytes[0..8].copy_from_slice(&self.magic.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.version.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.size.to_le_bytes());
        bytes[12..14].copy_from_slice(&self.kind.to_le_bytes());
        bytes[14..16].copy_from_slice(&self.flags.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.generation.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.capabilities.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.reserved);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != EVENT_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        let event = Self {
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
            kind: u16::from_le_bytes(
                bytes[12..14]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            flags: u16::from_le_bytes(
                bytes[14..16]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            generation: u64::from_le_bytes(
                bytes[16..24]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            capabilities: u32::from_le_bytes(
                bytes[24..28]
                    .try_into()
                    .map_err(|_| ProtocolError::InvalidSize)?,
            ),
            reserved: bytes[28..32]
                .try_into()
                .map_err(|_| ProtocolError::InvalidSize)?,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.magic != DESKTOP_PROTOCOL_MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        if self.version != DESKTOP_PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion);
        }
        if usize::from(self.size) != EVENT_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        if self.kind != EVENT_POLICY_APPLIED && self.kind != EVENT_CONFIG_APPLIED {
            return Err(ProtocolError::InvalidEvent);
        }
        if self.flags != 0 || self.reserved != [0; 4] {
            return Err(ProtocolError::ReservedBits);
        }
        if self.generation == 0 {
            return Err(ProtocolError::InvalidGeneration);
        }
        if self.capabilities != REQUIRED_CAPABILITIES {
            return Err(ProtocolError::InvalidCapabilities);
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
    InvalidEvent,
    InvalidGeneration,
    ReservedBits,
}

pub const fn config_hash(bytes: &[u8]) -> u64 {
    config_hash_extend(CONFIG_HASH_OFFSET, bytes)
}

pub const fn config_hash_extend(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut index = 0usize;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(CONFIG_HASH_PRIME);
        index += 1;
    }
    hash
}

const _: () = assert!(core::mem::size_of::<DesktopCommit>() == COMMIT_SIZE);
const _: () = assert!(core::mem::size_of::<DesktopServiceEvent>() == EVENT_SIZE);

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
        assert_eq!(config_hash(b""), CONFIG_HASH_OFFSET);
        assert_eq!(config_hash(b"SlopOS"), 0x1437_a4b8_2712_245f);
        assert_ne!(config_hash(b"waybar"), config_hash(b"swww"));
        let first = config_hash_extend(CONFIG_HASH_OFFSET, b"Slop");
        assert_eq!(config_hash_extend(first, b"OS"), config_hash(b"SlopOS"));
    }

    #[test]
    fn round_trips_desktop_service_events() {
        for event in [
            DesktopServiceEvent::policy_applied(7),
            DesktopServiceEvent::config_applied(11),
        ] {
            assert_eq!(DesktopServiceEvent::decode(&event.encode()), Ok(event));
        }
    }

    #[test]
    fn rejects_event_generation_flags_and_capabilities() {
        let mut event = DesktopServiceEvent::policy_applied(1);
        event.generation = 0;
        assert_eq!(
            DesktopServiceEvent::decode(&event.encode()),
            Err(ProtocolError::InvalidGeneration)
        );
        event.generation = 1;
        event.flags = 1;
        assert_eq!(
            DesktopServiceEvent::decode(&event.encode()),
            Err(ProtocolError::ReservedBits)
        );
        event.flags = 0;
        event.capabilities = CAPABILITY_WAYBAR_PROVIDER;
        assert_eq!(
            DesktopServiceEvent::decode(&event.encode()),
            Err(ProtocolError::InvalidCapabilities)
        );
        event.capabilities = REQUIRED_CAPABILITIES;
        event.kind = 3;
        assert_eq!(
            DesktopServiceEvent::decode(&event.encode()),
            Err(ProtocolError::InvalidEvent)
        );
    }
}
