// SPDX-License-Identifier: 0BSD

#![no_std]

pub const ELF64_HEADER_SIZE: usize = 64;
pub const ELF64_PROGRAM_HEADER_SIZE: usize = 56;
pub const MAX_PROGRAM_HEADERS: usize = 64;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

const ELF_CLASS_64: u8 = 2;
const ELF_DATA_LITTLE_ENDIAN: u8 = 1;
const ELF_VERSION_CURRENT: u8 = 1;
const ELF_OS_ABI_SYSTEM_V: u8 = 0;
const ELF_OS_ABI_LINUX: u8 = 3;
const ELF_TYPE_EXECUTABLE: u16 = 2;
const ELF_MACHINE_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseError {
    Truncated,
    InvalidMagic,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedIdentVersion,
    UnsupportedAbi,
    UnsupportedFileType,
    UnsupportedMachine,
    UnsupportedVersion,
    InvalidHeaderSize,
    MissingProgramHeaders,
    InvalidProgramHeaderSize,
    TooManyProgramHeaders,
    ProgramHeaderTableOutOfBounds,
    MissingLoadSegment,
    EmptyLoadSegment,
    FileSizeExceedsMemorySize,
    FileRangeOutOfBounds,
    AddressOverflow,
    NonCanonicalAddress,
    InvalidAlignment,
    MisalignedLoadSegment,
    WritableExecutableSegment,
    OverlappingLoadSegments,
    EntryOutsideExecutableSegment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProgramHeader {
    kind: u32,
    flags: u32,
    offset: u64,
    virtual_address: u64,
    file_size: u64,
    memory_size: u64,
    alignment: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoadSegment<'a> {
    data: &'a [u8],
    virtual_address: u64,
    memory_size: u64,
    flags: u32,
    alignment: u64,
}

impl<'a> LoadSegment<'a> {
    pub const fn data(self) -> &'a [u8] {
        self.data
    }

    pub const fn virtual_address(self) -> u64 {
        self.virtual_address
    }

    pub const fn file_size(self) -> usize {
        self.data.len()
    }

    pub const fn memory_size(self) -> u64 {
        self.memory_size
    }

    pub const fn alignment(self) -> u64 {
        self.alignment
    }

    pub const fn readable(self) -> bool {
        self.flags & PF_R != 0
    }

    pub const fn writable(self) -> bool {
        self.flags & PF_W != 0
    }

    pub const fn executable(self) -> bool {
        self.flags & PF_X != 0
    }
}

#[derive(Clone, Copy)]
pub struct ElfFile<'a> {
    bytes: &'a [u8],
    entry: u64,
    program_header_offset: usize,
    program_header_count: usize,
    load_segment_count: usize,
}

impl<'a> ElfFile<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < ELF64_HEADER_SIZE {
            return Err(ParseError::Truncated);
        }
        if bytes[0..4] != *b"\x7fELF" {
            return Err(ParseError::InvalidMagic);
        }
        if bytes[4] != ELF_CLASS_64 {
            return Err(ParseError::UnsupportedClass);
        }
        if bytes[5] != ELF_DATA_LITTLE_ENDIAN {
            return Err(ParseError::UnsupportedEndian);
        }
        if bytes[6] != ELF_VERSION_CURRENT {
            return Err(ParseError::UnsupportedIdentVersion);
        }
        if bytes[7] != ELF_OS_ABI_SYSTEM_V && bytes[7] != ELF_OS_ABI_LINUX {
            return Err(ParseError::UnsupportedAbi);
        }
        if read_u16(bytes, 16)? != ELF_TYPE_EXECUTABLE {
            return Err(ParseError::UnsupportedFileType);
        }
        if read_u16(bytes, 18)? != ELF_MACHINE_X86_64 {
            return Err(ParseError::UnsupportedMachine);
        }
        if read_u32(bytes, 20)? != u32::from(ELF_VERSION_CURRENT) {
            return Err(ParseError::UnsupportedVersion);
        }
        if usize::from(read_u16(bytes, 52)?) != ELF64_HEADER_SIZE {
            return Err(ParseError::InvalidHeaderSize);
        }

        let program_header_offset = usize::try_from(read_u64(bytes, 32)?)
            .map_err(|_| ParseError::ProgramHeaderTableOutOfBounds)?;
        let program_header_size = usize::from(read_u16(bytes, 54)?);
        let program_header_count = usize::from(read_u16(bytes, 56)?);
        if program_header_count == 0 {
            return Err(ParseError::MissingProgramHeaders);
        }
        if program_header_size != ELF64_PROGRAM_HEADER_SIZE {
            return Err(ParseError::InvalidProgramHeaderSize);
        }
        if program_header_count > MAX_PROGRAM_HEADERS {
            return Err(ParseError::TooManyProgramHeaders);
        }
        let table_size = program_header_size
            .checked_mul(program_header_count)
            .ok_or(ParseError::ProgramHeaderTableOutOfBounds)?;
        let table_end = program_header_offset
            .checked_add(table_size)
            .ok_or(ParseError::ProgramHeaderTableOutOfBounds)?;
        if program_header_offset < ELF64_HEADER_SIZE || table_end > bytes.len() {
            return Err(ParseError::ProgramHeaderTableOutOfBounds);
        }

        let file = Self {
            bytes,
            entry: read_u64(bytes, 24)?,
            program_header_offset,
            program_header_count,
            load_segment_count: 0,
        };
        file.validate_load_segments()
    }

    pub const fn entry(self) -> u64 {
        self.entry
    }

    pub const fn load_segment_count(self) -> usize {
        self.load_segment_count
    }

    pub const fn is_empty(self) -> bool {
        self.load_segment_count == 0
    }

    pub const fn load_segments(self) -> LoadSegments<'a> {
        LoadSegments {
            file: self,
            next_program_header: 0,
            remaining_load_segments: self.load_segment_count,
        }
    }

    fn validate_load_segments(mut self) -> Result<Self, ParseError> {
        let mut entry_is_executable = false;
        for index in 0..self.program_header_count {
            let header = self.program_header(index)?;
            if header.kind != PT_LOAD {
                continue;
            }
            self.validate_load_segment(header)?;
            self.load_segment_count += 1;

            let memory_end = header
                .virtual_address
                .checked_add(header.memory_size)
                .ok_or(ParseError::AddressOverflow)?;
            if header.flags & PF_X != 0
                && self.entry >= header.virtual_address
                && self.entry < memory_end
            {
                entry_is_executable = true;
            }

            for previous in 0..index {
                let other = self.program_header(previous)?;
                if other.kind == PT_LOAD && segments_overlap(header, other)? {
                    return Err(ParseError::OverlappingLoadSegments);
                }
            }
        }
        if self.load_segment_count == 0 {
            return Err(ParseError::MissingLoadSegment);
        }
        if !entry_is_executable {
            return Err(ParseError::EntryOutsideExecutableSegment);
        }
        Ok(self)
    }

    fn validate_load_segment(self, header: ProgramHeader) -> Result<(), ParseError> {
        if header.memory_size == 0 {
            return Err(ParseError::EmptyLoadSegment);
        }
        if header.file_size > header.memory_size {
            return Err(ParseError::FileSizeExceedsMemorySize);
        }
        if header.flags & (PF_W | PF_X) == PF_W | PF_X {
            return Err(ParseError::WritableExecutableSegment);
        }
        if header.alignment > 1 {
            if !header.alignment.is_power_of_two() {
                return Err(ParseError::InvalidAlignment);
            }
            if header.virtual_address % header.alignment != header.offset % header.alignment {
                return Err(ParseError::MisalignedLoadSegment);
            }
        }

        let file_start =
            usize::try_from(header.offset).map_err(|_| ParseError::FileRangeOutOfBounds)?;
        let file_size =
            usize::try_from(header.file_size).map_err(|_| ParseError::FileRangeOutOfBounds)?;
        let file_end = file_start
            .checked_add(file_size)
            .ok_or(ParseError::FileRangeOutOfBounds)?;
        if file_end > self.bytes.len() {
            return Err(ParseError::FileRangeOutOfBounds);
        }

        let memory_end = header
            .virtual_address
            .checked_add(header.memory_size)
            .ok_or(ParseError::AddressOverflow)?;
        if !is_canonical(header.virtual_address) || !is_canonical(memory_end - 1) {
            return Err(ParseError::NonCanonicalAddress);
        }
        Ok(())
    }

    fn program_header(self, index: usize) -> Result<ProgramHeader, ParseError> {
        if index >= self.program_header_count {
            return Err(ParseError::ProgramHeaderTableOutOfBounds);
        }
        let start = self
            .program_header_offset
            .checked_add(
                index
                    .checked_mul(ELF64_PROGRAM_HEADER_SIZE)
                    .ok_or(ParseError::ProgramHeaderTableOutOfBounds)?,
            )
            .ok_or(ParseError::ProgramHeaderTableOutOfBounds)?;
        Ok(ProgramHeader {
            kind: read_u32(self.bytes, start)?,
            flags: read_u32(self.bytes, start + 4)?,
            offset: read_u64(self.bytes, start + 8)?,
            virtual_address: read_u64(self.bytes, start + 16)?,
            file_size: read_u64(self.bytes, start + 32)?,
            memory_size: read_u64(self.bytes, start + 40)?,
            alignment: read_u64(self.bytes, start + 48)?,
        })
    }

    fn load_segment(self, header: ProgramHeader) -> Option<LoadSegment<'a>> {
        let start = usize::try_from(header.offset).ok()?;
        let size = usize::try_from(header.file_size).ok()?;
        let end = start.checked_add(size)?;
        Some(LoadSegment {
            data: self.bytes.get(start..end)?,
            virtual_address: header.virtual_address,
            memory_size: header.memory_size,
            flags: header.flags,
            alignment: header.alignment,
        })
    }
}

pub struct LoadSegments<'a> {
    file: ElfFile<'a>,
    next_program_header: usize,
    remaining_load_segments: usize,
}

impl<'a> Iterator for LoadSegments<'a> {
    type Item = LoadSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next_program_header < self.file.program_header_count {
            let index = self.next_program_header;
            self.next_program_header += 1;
            let header = self.file.program_header(index).ok()?;
            if header.kind == PT_LOAD {
                self.remaining_load_segments = self.remaining_load_segments.saturating_sub(1);
                return self.file.load_segment(header);
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (
            self.remaining_load_segments,
            Some(self.remaining_load_segments),
        )
    }
}

impl ExactSizeIterator for LoadSegments<'_> {}

fn segments_overlap(left: ProgramHeader, right: ProgramHeader) -> Result<bool, ParseError> {
    let left_end = left
        .virtual_address
        .checked_add(left.memory_size)
        .ok_or(ParseError::AddressOverflow)?;
    let right_end = right
        .virtual_address
        .checked_add(right.memory_size)
        .ok_or(ParseError::AddressOverflow)?;
    Ok(left.virtual_address < right_end && right.virtual_address < left_end)
}

const fn is_canonical(address: u64) -> bool {
    address <= 0x0000_7fff_ffff_ffff || address >= 0xffff_8000_0000_0000
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

    const PROGRAM_OFFSET: usize = 0x1000;
    const PROGRAM_ADDRESS: u64 = 0x4000_0000;

    fn executable() -> Vec<u8> {
        let mut bytes = vec![0u8; PROGRAM_OFFSET + 8];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = ELF_CLASS_64;
        bytes[5] = ELF_DATA_LITTLE_ENDIAN;
        bytes[6] = ELF_VERSION_CURRENT;
        put_u16(&mut bytes, 16, ELF_TYPE_EXECUTABLE);
        put_u16(&mut bytes, 18, ELF_MACHINE_X86_64);
        put_u32(&mut bytes, 20, u32::from(ELF_VERSION_CURRENT));
        put_u64(&mut bytes, 24, PROGRAM_ADDRESS);
        put_u64(&mut bytes, 32, ELF64_HEADER_SIZE as u64);
        put_u16(&mut bytes, 52, ELF64_HEADER_SIZE as u16);
        put_u16(&mut bytes, 54, ELF64_PROGRAM_HEADER_SIZE as u16);
        put_u16(&mut bytes, 56, 1);

        let header = ELF64_HEADER_SIZE;
        put_u32(&mut bytes, header, PT_LOAD);
        put_u32(&mut bytes, header + 4, PF_R | PF_X);
        put_u64(&mut bytes, header + 8, PROGRAM_OFFSET as u64);
        put_u64(&mut bytes, header + 16, PROGRAM_ADDRESS);
        put_u64(&mut bytes, header + 24, PROGRAM_ADDRESS);
        put_u64(&mut bytes, header + 32, 8);
        put_u64(&mut bytes, header + 40, 16);
        put_u64(&mut bytes, header + 48, 4096);
        bytes[PROGRAM_OFFSET..].copy_from_slice(b"program!");
        bytes
    }

    #[test]
    fn parses_x86_64_executable_and_bss_tail() {
        let bytes = executable();
        let file = ElfFile::parse(&bytes).unwrap();
        assert_eq!(file.entry(), PROGRAM_ADDRESS);
        assert_eq!(file.load_segment_count(), 1);
        assert_eq!(file.load_segments().len(), 1);
        let segment = file.load_segments().next().unwrap();
        assert_eq!(segment.data(), b"program!");
        assert_eq!(segment.virtual_address(), PROGRAM_ADDRESS);
        assert_eq!(segment.file_size(), 8);
        assert_eq!(segment.memory_size(), 16);
        assert_eq!(segment.alignment(), 4096);
        assert!(segment.readable());
        assert!(!segment.writable());
        assert!(segment.executable());
    }

    #[test]
    fn rejects_invalid_ident_and_target() {
        let mut bytes = executable();
        bytes[0] = 0;
        assert_eq!(ElfFile::parse(&bytes).err(), Some(ParseError::InvalidMagic));
        bytes = executable();
        bytes[4] = 1;
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::UnsupportedClass)
        );
        bytes = executable();
        bytes[5] = 2;
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::UnsupportedEndian)
        );
        bytes = executable();
        put_u16(&mut bytes, 18, 3);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::UnsupportedMachine)
        );
    }

    #[test]
    fn rejects_truncated_program_header_table() {
        let mut bytes = executable();
        let truncated_offset = bytes.len() as u64 - 4;
        put_u64(&mut bytes, 32, truncated_offset);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::ProgramHeaderTableOutOfBounds)
        );
    }

    #[test]
    fn rejects_file_size_larger_than_memory_size() {
        let mut bytes = executable();
        put_u64(&mut bytes, ELF64_HEADER_SIZE + 40, 4);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::FileSizeExceedsMemorySize)
        );
    }

    #[test]
    fn rejects_load_data_outside_file() {
        let mut bytes = executable();
        put_u64(&mut bytes, ELF64_HEADER_SIZE + 32, 9);
        put_u64(&mut bytes, ELF64_HEADER_SIZE + 40, 9);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::FileRangeOutOfBounds)
        );
    }

    #[test]
    fn rejects_invalid_alignment_and_congruence() {
        let mut bytes = executable();
        put_u64(&mut bytes, ELF64_HEADER_SIZE + 48, 24);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::InvalidAlignment)
        );
        bytes = executable();
        put_u64(&mut bytes, ELF64_HEADER_SIZE + 16, PROGRAM_ADDRESS + 1);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::MisalignedLoadSegment)
        );
    }

    #[test]
    fn rejects_writable_executable_segment() {
        let mut bytes = executable();
        put_u32(&mut bytes, ELF64_HEADER_SIZE + 4, PF_R | PF_W | PF_X);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::WritableExecutableSegment)
        );
    }

    #[test]
    fn rejects_entry_outside_executable_segment() {
        let mut bytes = executable();
        put_u64(&mut bytes, 24, PROGRAM_ADDRESS + 16);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::EntryOutsideExecutableSegment)
        );
    }

    #[test]
    fn rejects_overlapping_load_segments() {
        let mut bytes = executable();
        put_u16(&mut bytes, 56, 2);
        bytes.resize(PROGRAM_OFFSET + ELF64_PROGRAM_HEADER_SIZE, 0);
        let second = ELF64_HEADER_SIZE + ELF64_PROGRAM_HEADER_SIZE;
        put_u32(&mut bytes, second, PT_LOAD);
        put_u32(&mut bytes, second + 4, PF_R);
        put_u64(&mut bytes, second + 8, PROGRAM_OFFSET as u64 + 8);
        put_u64(&mut bytes, second + 16, PROGRAM_ADDRESS + 8);
        put_u64(&mut bytes, second + 24, PROGRAM_ADDRESS + 8);
        put_u64(&mut bytes, second + 32, 8);
        put_u64(&mut bytes, second + 40, 16);
        put_u64(&mut bytes, second + 48, 4096);
        assert_eq!(
            ElfFile::parse(&bytes).err(),
            Some(ParseError::OverlappingLoadSegments)
        );
    }

    #[test]
    fn accepts_non_load_program_headers() {
        let mut bytes = executable();
        put_u16(&mut bytes, 56, 2);
        let second = ELF64_HEADER_SIZE + ELF64_PROGRAM_HEADER_SIZE;
        put_u32(&mut bytes, second, 4);
        let file = ElfFile::parse(&bytes).unwrap();
        assert_eq!(file.load_segment_count(), 1);
    }

    fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
}
