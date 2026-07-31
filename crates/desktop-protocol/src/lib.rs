// SPDX-License-Identifier: 0BSD

#![no_std]

pub const DESKTOP_COMMIT_SYSCALL: u64 = 0x534c_0001;
pub const DESKTOP_WAIT_SYSCALL: u64 = 0x534c_0002;
pub const WAYLAND_SURFACE_SYSCALL: u64 = 0x534c_0003;
pub const WAYLAND_EVENT_WAIT_SYSCALL: u64 = 0x534c_0004;
/// Temporary inline backing-store staging until AF_UNIX `SCM_RIGHTS` and mmap
/// can carry a real `wl_shm` file descriptor.
pub const WAYLAND_BACKING_STAGE_SYSCALL: u64 = 0x534c_0005;
pub const DESKTOP_PROTOCOL_MAGIC: u64 = 0x534c_4f50_4445_534b;
pub const DESKTOP_PROTOCOL_VERSION: u16 = 1;
pub const WAYLAND_SURFACE_MAGIC: u64 = 0x534c_4f50_574c_5355;
pub const WAYLAND_SURFACE_VERSION: u16 = 1;
pub const WAYLAND_EVENT_MAGIC: u64 = 0x534c_4f50_574c_4556;
pub const WAYLAND_EVENT_VERSION: u16 = 1;
pub const CAPABILITY_WAYBAR_PROVIDER: u32 = 1 << 0;
pub const CAPABILITY_SWWW_POLICY: u32 = 1 << 1;
pub const CAPABILITY_WAYLAND_SURFACE: u32 = 1 << 2;
pub const REQUIRED_CAPABILITIES: u32 =
    CAPABILITY_WAYBAR_PROVIDER | CAPABILITY_SWWW_POLICY | CAPABILITY_WAYLAND_SURFACE;
pub const WALLPAPER_AURORA: u8 = 1;
pub const COMMIT_SIZE: usize = 40;
pub const EVENT_SIZE: usize = 32;
pub const WAYLAND_SURFACE_HEADER_SIZE: usize = 32;
pub const WAYLAND_SURFACE_MAX_WIRE_SIZE: usize = 768;
pub const WAYLAND_SURFACE_MAX_PIXEL_SIZE: usize = 3_072;
pub const WAYLAND_SURFACE_MAX_SIZE: usize =
    WAYLAND_SURFACE_HEADER_SIZE + WAYLAND_SURFACE_MAX_WIRE_SIZE + WAYLAND_SURFACE_MAX_PIXEL_SIZE;
pub const WAYLAND_EVENT_HEADER_SIZE: usize = 32;
pub const WAYLAND_EVENT_MAX_WIRE_SIZE: usize = 512;
pub const WAYLAND_EVENT_MAX_SIZE: usize = WAYLAND_EVENT_HEADER_SIZE + WAYLAND_EVENT_MAX_WIRE_SIZE;
/// Private bootstrap descriptor paired with the inline pixel snapshot.
///
/// The request stream itself uses normal Wayland framing. This value is not an
/// operating-system file descriptor and must not be exposed as one to clients.
pub const WAYLAND_INLINE_SHM_FD: i32 = 0x534c;
pub const WAYLAND_NO_FILE_DESCRIPTOR: i32 = -1;
pub const WAYLAND_EVENT_REGISTRY: u16 = 1;
pub const WAYLAND_EVENT_CONFIGURE: u16 = 2;
pub const WAYLAND_EVENT_PRESENTED: u16 = 3;
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaylandSurfaceHeader {
    pub magic: u64,
    pub version: u16,
    pub size: u16,
    pub wire_length: u32,
    pub pixel_length: u32,
    pub file_descriptor: i32,
    pub reserved: u64,
}

impl WaylandSurfaceHeader {
    pub const fn new(wire_length: u32, pixel_length: u32) -> Self {
        Self {
            magic: WAYLAND_SURFACE_MAGIC,
            version: WAYLAND_SURFACE_VERSION,
            size: WAYLAND_SURFACE_HEADER_SIZE as u16,
            wire_length,
            pixel_length,
            file_descriptor: if pixel_length == 0 {
                WAYLAND_NO_FILE_DESCRIPTOR
            } else {
                WAYLAND_INLINE_SHM_FD
            },
            reserved: 0,
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != WAYLAND_SURFACE_HEADER_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        let header = Self {
            magic: read_u64(bytes, 0)?,
            version: read_u16(bytes, 8)?,
            size: read_u16(bytes, 10)?,
            wire_length: read_u32(bytes, 12)?,
            pixel_length: read_u32(bytes, 16)?,
            file_descriptor: read_u32(bytes, 20)? as i32,
            reserved: read_u64(bytes, 24)?,
        };
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.magic != WAYLAND_SURFACE_MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        if self.version != WAYLAND_SURFACE_VERSION {
            return Err(ProtocolError::InvalidVersion);
        }
        if usize::from(self.size) != WAYLAND_SURFACE_HEADER_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        let wire_length =
            usize::try_from(self.wire_length).map_err(|_| ProtocolError::InvalidSize)?;
        let pixel_length =
            usize::try_from(self.pixel_length).map_err(|_| ProtocolError::InvalidSize)?;
        if wire_length == 0
            || wire_length > WAYLAND_SURFACE_MAX_WIRE_SIZE
            || wire_length % 4 != 0
            || pixel_length > WAYLAND_SURFACE_MAX_PIXEL_SIZE
            || pixel_length % 4 != 0
        {
            return Err(ProtocolError::InvalidSize);
        }
        let expected_descriptor = if pixel_length == 0 {
            WAYLAND_NO_FILE_DESCRIPTOR
        } else {
            WAYLAND_INLINE_SHM_FD
        };
        if self.file_descriptor != expected_descriptor {
            return Err(ProtocolError::InvalidFileDescriptor);
        }
        if self.reserved != 0 {
            return Err(ProtocolError::ReservedBits);
        }
        self.total_size()?;
        Ok(())
    }

    pub fn total_size(&self) -> Result<usize, ProtocolError> {
        let wire_length =
            usize::try_from(self.wire_length).map_err(|_| ProtocolError::InvalidSize)?;
        let pixel_length =
            usize::try_from(self.pixel_length).map_err(|_| ProtocolError::InvalidSize)?;
        WAYLAND_SURFACE_HEADER_SIZE
            .checked_add(wire_length)
            .and_then(|size| size.checked_add(pixel_length))
            .filter(|size| *size <= WAYLAND_SURFACE_MAX_SIZE)
            .ok_or(ProtocolError::InvalidSize)
    }

    pub fn encode(self) -> [u8; WAYLAND_SURFACE_HEADER_SIZE] {
        let mut bytes = [0; WAYLAND_SURFACE_HEADER_SIZE];
        bytes[0..8].copy_from_slice(&self.magic.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.version.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.size.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.wire_length.to_le_bytes());
        bytes[16..20].copy_from_slice(&self.pixel_length.to_le_bytes());
        bytes[20..24].copy_from_slice(&self.file_descriptor.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaylandEventHeader {
    pub magic: u64,
    pub version: u16,
    pub size: u16,
    pub kind: u16,
    pub flags: u16,
    pub sequence: u64,
    pub wire_length: u32,
    pub reserved: u32,
}

impl WaylandEventHeader {
    pub const fn new(kind: u16, sequence: u64, wire_length: u32) -> Self {
        Self {
            magic: WAYLAND_EVENT_MAGIC,
            version: WAYLAND_EVENT_VERSION,
            size: WAYLAND_EVENT_HEADER_SIZE as u16,
            kind,
            flags: 0,
            sequence,
            wire_length,
            reserved: 0,
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() != WAYLAND_EVENT_HEADER_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        let header = Self {
            magic: read_u64(bytes, 0)?,
            version: read_u16(bytes, 8)?,
            size: read_u16(bytes, 10)?,
            kind: read_u16(bytes, 12)?,
            flags: read_u16(bytes, 14)?,
            sequence: read_u64(bytes, 16)?,
            wire_length: read_u32(bytes, 24)?,
            reserved: read_u32(bytes, 28)?,
        };
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.magic != WAYLAND_EVENT_MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        if self.version != WAYLAND_EVENT_VERSION {
            return Err(ProtocolError::InvalidVersion);
        }
        if usize::from(self.size) != WAYLAND_EVENT_HEADER_SIZE {
            return Err(ProtocolError::InvalidSize);
        }
        if !matches!(
            self.kind,
            WAYLAND_EVENT_REGISTRY | WAYLAND_EVENT_CONFIGURE | WAYLAND_EVENT_PRESENTED
        ) {
            return Err(ProtocolError::InvalidEvent);
        }
        if self.sequence == 0 {
            return Err(ProtocolError::InvalidGeneration);
        }
        let wire_length =
            usize::try_from(self.wire_length).map_err(|_| ProtocolError::InvalidSize)?;
        if wire_length == 0 || wire_length > WAYLAND_EVENT_MAX_WIRE_SIZE || wire_length % 4 != 0 {
            return Err(ProtocolError::InvalidSize);
        }
        if self.flags != 0 || self.reserved != 0 {
            return Err(ProtocolError::ReservedBits);
        }
        Ok(())
    }

    pub fn total_size(&self) -> Result<usize, ProtocolError> {
        WAYLAND_EVENT_HEADER_SIZE
            .checked_add(self.wire_length as usize)
            .filter(|size| *size <= WAYLAND_EVENT_MAX_SIZE)
            .ok_or(ProtocolError::InvalidSize)
    }

    pub fn encode(self) -> [u8; WAYLAND_EVENT_HEADER_SIZE] {
        let mut bytes = [0; WAYLAND_EVENT_HEADER_SIZE];
        bytes[0..8].copy_from_slice(&self.magic.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.version.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.size.to_le_bytes());
        bytes[12..14].copy_from_slice(&self.kind.to_le_bytes());
        bytes[14..16].copy_from_slice(&self.flags.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        bytes[24..28].copy_from_slice(&self.wire_length.to_le_bytes());
        bytes[28..32].copy_from_slice(&self.reserved.to_le_bytes());
        bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaylandServerEvent<'a> {
    pub header: WaylandEventHeader,
    pub wire: &'a [u8],
}

impl<'a> WaylandServerEvent<'a> {
    pub fn new(kind: u16, sequence: u64, wire: &'a [u8]) -> Result<Self, ProtocolError> {
        let wire_length = u32::try_from(wire.len()).map_err(|_| ProtocolError::InvalidSize)?;
        let event = Self {
            header: WaylandEventHeader::new(kind, sequence, wire_length),
            wire,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let header = WaylandEventHeader::decode(
            bytes
                .get(..WAYLAND_EVENT_HEADER_SIZE)
                .ok_or(ProtocolError::InvalidSize)?,
        )?;
        if bytes.len() != header.total_size()? {
            return Err(ProtocolError::InvalidSize);
        }
        let event = Self {
            header,
            wire: bytes
                .get(WAYLAND_EVENT_HEADER_SIZE..)
                .ok_or(ProtocolError::InvalidSize)?,
        };
        event.validate()?;
        Ok(event)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.header.validate()?;
        if self.wire.len() != self.header.wire_length as usize {
            return Err(ProtocolError::InvalidSize);
        }
        Ok(())
    }

    pub fn encode(&self, destination: &mut [u8]) -> Result<usize, ProtocolError> {
        self.validate()?;
        let total_size = self.header.total_size()?;
        if destination.len() < total_size {
            return Err(ProtocolError::InvalidSize);
        }
        destination[..WAYLAND_EVENT_HEADER_SIZE].copy_from_slice(&self.header.encode());
        destination[WAYLAND_EVENT_HEADER_SIZE..total_size].copy_from_slice(self.wire);
        Ok(total_size)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaylandSurfaceCommit<'a> {
    pub header: WaylandSurfaceHeader,
    pub wire: &'a [u8],
    pub pixels: &'a [u8],
}

impl<'a> WaylandSurfaceCommit<'a> {
    pub fn new(wire: &'a [u8], pixels: &'a [u8]) -> Result<Self, ProtocolError> {
        let wire_length = u32::try_from(wire.len()).map_err(|_| ProtocolError::InvalidSize)?;
        let pixel_length = u32::try_from(pixels.len()).map_err(|_| ProtocolError::InvalidSize)?;
        let commit = Self {
            header: WaylandSurfaceHeader::new(wire_length, pixel_length),
            wire,
            pixels,
        };
        commit.validate()?;
        Ok(commit)
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let header_bytes = bytes
            .get(..WAYLAND_SURFACE_HEADER_SIZE)
            .ok_or(ProtocolError::InvalidSize)?;
        let header = WaylandSurfaceHeader::decode(header_bytes)?;
        if bytes.len() != header.total_size()? {
            return Err(ProtocolError::InvalidSize);
        }
        let wire_end = WAYLAND_SURFACE_HEADER_SIZE
            .checked_add(header.wire_length as usize)
            .ok_or(ProtocolError::InvalidSize)?;
        let commit = Self {
            header,
            wire: bytes
                .get(WAYLAND_SURFACE_HEADER_SIZE..wire_end)
                .ok_or(ProtocolError::InvalidSize)?,
            pixels: bytes.get(wire_end..).ok_or(ProtocolError::InvalidSize)?,
        };
        commit.validate()?;
        Ok(commit)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.header.validate()?;
        if self.wire.len() != self.header.wire_length as usize
            || self.pixels.len() != self.header.pixel_length as usize
        {
            return Err(ProtocolError::InvalidSize);
        }
        Ok(())
    }

    pub fn encode(&self, destination: &mut [u8]) -> Result<usize, ProtocolError> {
        self.validate()?;
        let total_size = self.header.total_size()?;
        if destination.len() < total_size {
            return Err(ProtocolError::InvalidSize);
        }
        destination[..WAYLAND_SURFACE_HEADER_SIZE].copy_from_slice(&self.header.encode());
        let wire_end = WAYLAND_SURFACE_HEADER_SIZE + self.wire.len();
        destination[WAYLAND_SURFACE_HEADER_SIZE..wire_end].copy_from_slice(self.wire);
        destination[wire_end..total_size].copy_from_slice(self.pixels);
        Ok(total_size)
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
    InvalidFileDescriptor,
    ReservedBits,
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ProtocolError> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(ProtocolError::InvalidSize)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidSize)?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ProtocolError> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(ProtocolError::InvalidSize)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidSize)?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ProtocolError> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or(ProtocolError::InvalidSize)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidSize)?,
    ))
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
const _: () = assert!(core::mem::size_of::<WaylandSurfaceHeader>() == WAYLAND_SURFACE_HEADER_SIZE);
const _: () = assert!(core::mem::size_of::<WaylandEventHeader>() == WAYLAND_EVENT_HEADER_SIZE);

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

    #[test]
    fn round_trips_a_bounded_wayland_surface_envelope() {
        let wire = [0x57; 64];
        let pixels = [0x9a; 128];
        let commit = WaylandSurfaceCommit::new(&wire, &pixels).unwrap();
        let mut encoded = [0; WAYLAND_SURFACE_MAX_SIZE];
        let length = commit.encode(&mut encoded).unwrap();
        assert_eq!(
            length,
            WAYLAND_SURFACE_HEADER_SIZE + wire.len() + pixels.len()
        );
        assert_eq!(WaylandSurfaceCommit::decode(&encoded[..length]), Ok(commit));
    }

    #[test]
    fn accepts_a_wire_only_wayland_request_batch() {
        let wire = [0x57; 8];
        let commit = WaylandSurfaceCommit::new(&wire, &[]).unwrap();
        assert_eq!(commit.header.file_descriptor, WAYLAND_NO_FILE_DESCRIPTOR);
        let mut encoded = [0; WAYLAND_SURFACE_MAX_SIZE];
        let length = commit.encode(&mut encoded).unwrap();
        assert_eq!(WaylandSurfaceCommit::decode(&encoded[..length]), Ok(commit));
    }

    #[test]
    fn rejects_wayland_envelope_length_descriptor_and_reserved_drift() {
        let wire = [0x57; 8];
        let pixels = [0x9a; 4];
        let commit = WaylandSurfaceCommit::new(&wire, &pixels).unwrap();
        let mut encoded = [0; 64];
        let length = commit.encode(&mut encoded).unwrap();
        assert_eq!(
            WaylandSurfaceCommit::decode(&encoded[..length - 1]),
            Err(ProtocolError::InvalidSize)
        );

        let mut header = commit.header;
        header.file_descriptor += 1;
        assert_eq!(
            WaylandSurfaceHeader::decode(&header.encode()),
            Err(ProtocolError::InvalidFileDescriptor)
        );
        header.file_descriptor = WAYLAND_INLINE_SHM_FD;
        header.reserved = 1;
        assert_eq!(
            WaylandSurfaceHeader::decode(&header.encode()),
            Err(ProtocolError::ReservedBits)
        );
    }

    #[test]
    fn rejects_unaligned_or_oversized_wayland_sections() {
        assert_eq!(
            WaylandSurfaceCommit::new(&[0; 6], &[0; 4]),
            Err(ProtocolError::InvalidSize)
        );
        assert_eq!(
            WaylandSurfaceCommit::new(&[0; 8], &[0; WAYLAND_SURFACE_MAX_PIXEL_SIZE + 4]),
            Err(ProtocolError::InvalidSize)
        );
    }

    #[test]
    fn round_trips_bounded_wayland_server_events() {
        let wire = [0x45; 64];
        for kind in [
            WAYLAND_EVENT_REGISTRY,
            WAYLAND_EVENT_CONFIGURE,
            WAYLAND_EVENT_PRESENTED,
        ] {
            let event = WaylandServerEvent::new(kind, u64::from(kind), &wire).unwrap();
            let mut encoded = [0; WAYLAND_EVENT_MAX_SIZE];
            let length = event.encode(&mut encoded).unwrap();
            assert_eq!(WaylandServerEvent::decode(&encoded[..length]), Ok(event));
        }
    }

    #[test]
    fn rejects_wayland_server_event_sequence_and_length_drift() {
        let mut header = WaylandEventHeader::new(WAYLAND_EVENT_CONFIGURE, 0, 8);
        assert_eq!(
            WaylandEventHeader::decode(&header.encode()),
            Err(ProtocolError::InvalidGeneration)
        );
        header.sequence = 1;
        header.wire_length = 6;
        assert_eq!(
            WaylandEventHeader::decode(&header.encode()),
            Err(ProtocolError::InvalidSize)
        );
        header.wire_length = 8;
        header.flags = 1;
        assert_eq!(
            WaylandEventHeader::decode(&header.encode()),
            Err(ProtocolError::ReservedBits)
        );
    }
}
