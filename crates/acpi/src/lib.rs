// SPDX-License-Identifier: 0BSD

#![no_std]

pub const SDT_HEADER_SIZE: usize = 36;
pub const MAX_TABLE_SIZE: usize = 1024 * 1024;
pub const MAX_IO_APICS: usize = 8;
pub const MAX_INTERRUPT_OVERRIDES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    Truncated,
    InvalidSignature,
    InvalidChecksum,
    InvalidLength,
    InvalidRootTable,
    InvalidMadtEntry,
    TooManyIoApics,
    TooManyInterruptOverrides,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootKind {
    Rsdt,
    Xsdt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rsdp {
    root_kind: RootKind,
    root_address: u64,
}

impl Rsdp {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        if bytes.len() < 20 {
            return Err(ParseError::Truncated);
        }
        if &bytes[0..8] != b"RSD PTR " {
            return Err(ParseError::InvalidSignature);
        }
        if !checksum_valid(&bytes[..20]) {
            return Err(ParseError::InvalidChecksum);
        }

        if bytes[15] >= 2 {
            if bytes.len() < 36 {
                return Err(ParseError::Truncated);
            }
            let length = read_u32(bytes, 20)? as usize;
            if !(36..=4096).contains(&length) || bytes.len() < length {
                return Err(ParseError::InvalidLength);
            }
            if !checksum_valid(&bytes[..length]) {
                return Err(ParseError::InvalidChecksum);
            }
            let address = read_u64(bytes, 24)?;
            if address == 0 {
                return Err(ParseError::InvalidRootTable);
            }
            Ok(Self {
                root_kind: RootKind::Xsdt,
                root_address: address,
            })
        } else {
            let address = u64::from(read_u32(bytes, 16)?);
            if address == 0 {
                return Err(ParseError::InvalidRootTable);
            }
            Ok(Self {
                root_kind: RootKind::Rsdt,
                root_address: address,
            })
        }
    }

    pub const fn root_kind(self) -> RootKind {
        self.root_kind
    }

    pub const fn root_address(self) -> u64 {
        self.root_address
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: usize,
    pub revision: u8,
}

pub fn sdt_length(header: &[u8]) -> Result<usize, ParseError> {
    if header.len() < SDT_HEADER_SIZE {
        return Err(ParseError::Truncated);
    }
    let length = read_u32(header, 4)? as usize;
    if !(SDT_HEADER_SIZE..=MAX_TABLE_SIZE).contains(&length) {
        return Err(ParseError::InvalidLength);
    }
    Ok(length)
}

pub fn parse_sdt(bytes: &[u8]) -> Result<SdtHeader, ParseError> {
    let length = sdt_length(bytes)?;
    if bytes.len() < length {
        return Err(ParseError::Truncated);
    }
    if !checksum_valid(&bytes[..length]) {
        return Err(ParseError::InvalidChecksum);
    }
    Ok(SdtHeader {
        signature: bytes[0..4].try_into().map_err(|_| ParseError::Truncated)?,
        length,
        revision: bytes[8],
    })
}

pub struct RootTable<'a> {
    bytes: &'a [u8],
    entry_width: usize,
    entry_count: usize,
}

impl<'a> RootTable<'a> {
    pub fn parse(bytes: &'a [u8], kind: RootKind) -> Result<Self, ParseError> {
        let header = parse_sdt(bytes)?;
        let (signature, entry_width) = match kind {
            RootKind::Rsdt => (*b"RSDT", 4),
            RootKind::Xsdt => (*b"XSDT", 8),
        };
        if header.signature != signature {
            return Err(ParseError::InvalidSignature);
        }
        let payload_length = header.length - SDT_HEADER_SIZE;
        if payload_length % entry_width != 0 {
            return Err(ParseError::InvalidRootTable);
        }
        Ok(Self {
            bytes: &bytes[..header.length],
            entry_width,
            entry_count: payload_length / entry_width,
        })
    }

    pub const fn len(&self) -> usize {
        self.entry_count
    }

    pub const fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    pub fn entry(&self, index: usize) -> Option<u64> {
        if index >= self.entry_count {
            return None;
        }
        let offset = SDT_HEADER_SIZE + index * self.entry_width;
        if self.entry_width == 8 {
            read_u64(self.bytes, offset).ok()
        } else {
            read_u32(self.bytes, offset).ok().map(u64::from)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IoApic {
    pub id: u8,
    pub address: u32,
    pub global_interrupt_base: u32,
}

impl IoApic {
    const EMPTY: Self = Self {
        id: 0,
        address: 0,
        global_interrupt_base: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptOverride {
    pub bus: u8,
    pub source: u8,
    pub global_interrupt: u32,
    pub flags: u16,
}

impl InterruptOverride {
    const EMPTY: Self = Self {
        bus: 0,
        source: 0,
        global_interrupt: 0,
        flags: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MadtInfo {
    pub local_apic_address: u64,
    pub pcat_compatible: bool,
    pub enabled_processors: usize,
    io_apics: [IoApic; MAX_IO_APICS],
    io_apic_count: usize,
    overrides: [InterruptOverride; MAX_INTERRUPT_OVERRIDES],
    override_count: usize,
}

impl MadtInfo {
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let header = parse_sdt(bytes)?;
        if header.signature != *b"APIC" {
            return Err(ParseError::InvalidSignature);
        }
        if header.length < 44 {
            return Err(ParseError::InvalidLength);
        }

        let mut info = Self {
            local_apic_address: u64::from(read_u32(bytes, 36)?),
            pcat_compatible: read_u32(bytes, 40)? & 1 != 0,
            enabled_processors: 0,
            io_apics: [IoApic::EMPTY; MAX_IO_APICS],
            io_apic_count: 0,
            overrides: [InterruptOverride::EMPTY; MAX_INTERRUPT_OVERRIDES],
            override_count: 0,
        };

        let mut offset = 44;
        while offset < header.length {
            if offset + 2 > header.length {
                return Err(ParseError::InvalidMadtEntry);
            }
            let entry_type = bytes[offset];
            let entry_length = bytes[offset + 1] as usize;
            if entry_length < 2 || offset + entry_length > header.length {
                return Err(ParseError::InvalidMadtEntry);
            }

            match entry_type {
                0 if entry_length >= 8 => {
                    if read_u32(bytes, offset + 4)? & 3 != 0 {
                        info.enabled_processors += 1;
                    }
                }
                1 if entry_length >= 12 => {
                    if info.io_apic_count == MAX_IO_APICS {
                        return Err(ParseError::TooManyIoApics);
                    }
                    info.io_apics[info.io_apic_count] = IoApic {
                        id: bytes[offset + 2],
                        address: read_u32(bytes, offset + 4)?,
                        global_interrupt_base: read_u32(bytes, offset + 8)?,
                    };
                    info.io_apic_count += 1;
                }
                2 if entry_length >= 10 => {
                    if info.override_count == MAX_INTERRUPT_OVERRIDES {
                        return Err(ParseError::TooManyInterruptOverrides);
                    }
                    info.overrides[info.override_count] = InterruptOverride {
                        bus: bytes[offset + 2],
                        source: bytes[offset + 3],
                        global_interrupt: read_u32(bytes, offset + 4)?,
                        flags: read_u16(bytes, offset + 8)?,
                    };
                    info.override_count += 1;
                }
                5 if entry_length >= 12 => {
                    info.local_apic_address = read_u64(bytes, offset + 4)?;
                }
                9 if entry_length >= 16 => {
                    if read_u32(bytes, offset + 8)? & 3 != 0 {
                        info.enabled_processors += 1;
                    }
                }
                _ => {}
            }
            offset += entry_length;
        }

        if info.local_apic_address == 0 || info.io_apic_count == 0 {
            return Err(ParseError::InvalidMadtEntry);
        }
        Ok(info)
    }

    pub fn io_apics(&self) -> &[IoApic] {
        &self.io_apics[..self.io_apic_count]
    }

    pub fn interrupt_overrides(&self) -> &[InterruptOverride] {
        &self.overrides[..self.override_count]
    }

    pub fn isa_route(&self, irq: u8) -> (u32, u16) {
        self.interrupt_overrides()
            .iter()
            .find(|entry| entry.bus == 0 && entry.source == irq)
            .map_or((u32::from(irq), 0), |entry| {
                (entry.global_interrupt, entry.flags)
            })
    }
}

fn checksum_valid(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ParseError> {
    let value = bytes.get(offset..offset + 2).ok_or(ParseError::Truncated)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ParseError> {
    let value = bytes.get(offset..offset + 4).ok_or(ParseError::Truncated)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ParseError> {
    let value = bytes.get(offset..offset + 8).ok_or(ParseError::Truncated)?;
    Ok(u64::from_le_bytes([
        value[0], value[1], value[2], value[3], value[4], value[5], value[6], value[7],
    ]))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn set_checksum(bytes: &mut [u8], checksum_offset: usize) {
        bytes[checksum_offset] = 0;
        bytes[checksum_offset] =
            0u8.wrapping_sub(bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)));
    }

    fn sdt(signature: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut table = vec![0u8; SDT_HEADER_SIZE + payload.len()];
        table[0..4].copy_from_slice(&signature);
        let length = table.len() as u32;
        table[4..8].copy_from_slice(&length.to_le_bytes());
        table[8] = 1;
        table[SDT_HEADER_SIZE..].copy_from_slice(payload);
        set_checksum(&mut table, 9);
        table
    }

    #[test]
    fn parses_acpi_two_rsdp_and_xsdt() {
        let mut rsdp = [0u8; 36];
        rsdp[0..8].copy_from_slice(b"RSD PTR ");
        rsdp[15] = 2;
        rsdp[20..24].copy_from_slice(&(36u32).to_le_bytes());
        rsdp[24..32].copy_from_slice(&0x1234_5678_9abc_def0u64.to_le_bytes());
        set_checksum(&mut rsdp[..20], 8);
        set_checksum(&mut rsdp, 32);
        let parsed = Rsdp::parse(&rsdp).unwrap();
        assert_eq!(parsed.root_kind(), RootKind::Xsdt);
        assert_eq!(parsed.root_address(), 0x1234_5678_9abc_def0);

        let root = sdt(
            *b"XSDT",
            &[0x00, 0x10, 0, 0, 0, 0, 0, 0, 0x00, 0x20, 0, 0, 0, 0, 0, 0],
        );
        let root = RootTable::parse(&root, RootKind::Xsdt).unwrap();
        assert_eq!(root.len(), 2);
        assert_eq!(root.entry(0), Some(0x1000));
        assert_eq!(root.entry(1), Some(0x2000));
    }

    #[test]
    fn parses_madt_topology_and_override() {
        let mut payload = vec![0u8; 8];
        payload[0..4].copy_from_slice(&0xfee0_0000u32.to_le_bytes());
        payload[4..8].copy_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&[0, 8, 0, 2, 1, 0, 0, 0]);
        payload.extend_from_slice(&[1, 12, 3, 0, 0x00, 0x00, 0xc0, 0xfe, 0, 0, 0, 0]);
        payload.extend_from_slice(&[2, 10, 0, 0, 2, 0, 0, 0, 0x0f, 0]);
        let table = sdt(*b"APIC", &payload);
        let info = MadtInfo::parse(&table).unwrap();
        assert_eq!(info.local_apic_address, 0xfee0_0000);
        assert!(info.pcat_compatible);
        assert_eq!(info.enabled_processors, 1);
        assert_eq!(
            info.io_apics(),
            &[IoApic {
                id: 3,
                address: 0xfec0_0000,
                global_interrupt_base: 0
            }]
        );
        assert_eq!(info.isa_route(0), (2, 0x000f));
        assert_eq!(info.isa_route(1), (1, 0));
    }

    #[test]
    fn rejects_bad_checksums_and_truncated_entries() {
        let mut root = sdt(*b"XSDT", &[0; 8]);
        root[10] ^= 1;
        assert_eq!(
            RootTable::parse(&root, RootKind::Xsdt).err(),
            Some(ParseError::InvalidChecksum)
        );

        let mut payload = vec![0u8; 8];
        payload[0..4].copy_from_slice(&0xfee0_0000u32.to_le_bytes());
        payload.extend_from_slice(&[1, 12, 0]);
        let table = sdt(*b"APIC", &payload);
        assert_eq!(
            MadtInfo::parse(&table).err(),
            Some(ParseError::InvalidMadtEntry)
        );
    }
}
