// SPDX-License-Identifier: 0BSD

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_BITS: usize = 15;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PngError {
    InvalidSignature,
    TruncatedChunk,
    InvalidChunkType,
    InvalidChunkCrc,
    MissingHeader,
    DuplicateHeader,
    InvalidHeader,
    UnsupportedColor,
    UnsupportedInterlace,
    MissingPalette,
    InvalidPaletteIndex,
    MissingImageData,
    InvalidChunkOrder,
    MissingEnd,
    TrailingData,
    OutputTooLarge,
    InvalidZlibHeader,
    InvalidDeflate,
    InvalidAdler32,
    InvalidFilter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedPng {
    width: u16,
    height: u16,
    rgb_length: usize,
}

impl DecodedPng {
    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn rgb_length(self) -> usize {
        self.rgb_length
    }
}

#[derive(Clone, Copy)]
struct PngMetadata {
    width: usize,
    height: usize,
    channels: usize,
    bit_depth: u8,
    color_type: u8,
    idat_length: usize,
    palette: [u32; 256],
    palette_length: usize,
    transparency: Transparency,
}

#[derive(Clone, Copy)]
enum Transparency {
    None,
    Gray(u8),
    Rgb(u8, u8, u8),
}

pub fn decode_png_rgb(input: &mut [u8], output: &mut [u8]) -> Result<DecodedPng, PngError> {
    let metadata = validate_png(input)?;
    let bits_per_pixel = metadata
        .channels
        .checked_mul(usize::from(metadata.bit_depth))
        .ok_or(PngError::OutputTooLarge)?;
    let row_bytes = metadata
        .width
        .checked_mul(bits_per_pixel)
        .and_then(|bits| bits.checked_add(7))
        .map(|bits| bits / 8)
        .ok_or(PngError::OutputTooLarge)?;
    let filter_bytes_per_pixel = bits_per_pixel.div_ceil(8).max(1);
    let decompressed_length = row_bytes
        .checked_add(1)
        .and_then(|stride| stride.checked_mul(metadata.height))
        .ok_or(PngError::OutputTooLarge)?;
    let rgb_length = metadata
        .width
        .checked_mul(metadata.height)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or(PngError::OutputTooLarge)?;
    if decompressed_length.max(rgb_length) > output.len() {
        return Err(PngError::OutputTooLarge);
    }
    compact_idat(input, metadata.idat_length)?;
    let produced = inflate_zlib(
        &input[..metadata.idat_length],
        &mut output[..decompressed_length],
    )?;
    if produced != decompressed_length {
        return Err(PngError::InvalidDeflate);
    }
    unfilter_scanlines(
        &mut output[..decompressed_length],
        row_bytes,
        metadata.height,
        filter_bytes_per_pixel,
    )?;
    compact_rgb(output, &metadata)?;
    Ok(DecodedPng {
        width: metadata.width as u16,
        height: metadata.height as u16,
        rgb_length,
    })
}

fn validate_png(input: &[u8]) -> Result<PngMetadata, PngError> {
    if input.get(..PNG_SIGNATURE.len()) != Some(PNG_SIGNATURE) {
        return Err(PngError::InvalidSignature);
    }
    let mut offset = PNG_SIGNATURE.len();
    let mut header = None;
    let mut idat_length = 0usize;
    let mut saw_palette = false;
    let mut saw_transparency = false;
    let mut saw_idat = false;
    let mut ended_idat = false;
    let mut saw_end = false;
    let mut palette = [0xff00_0000u32; 256];
    let mut palette_length = 0usize;
    let mut transparency = Transparency::None;
    while offset < input.len() {
        let chunk = png_chunk(input, offset)?;
        let kind = chunk.kind;
        if !kind.iter().all(u8::is_ascii_alphabetic) || !kind[2].is_ascii_uppercase() {
            return Err(PngError::InvalidChunkType);
        }
        match kind {
            b"IHDR" => {
                if offset != PNG_SIGNATURE.len() {
                    return Err(PngError::InvalidChunkOrder);
                }
                if header.is_some() {
                    return Err(PngError::DuplicateHeader);
                }
                header = Some(parse_header(chunk.data)?);
            }
            b"IDAT" => {
                if header.is_none() || ended_idat {
                    return Err(PngError::InvalidChunkOrder);
                }
                saw_idat = true;
                idat_length = idat_length
                    .checked_add(chunk.data.len())
                    .ok_or(PngError::OutputTooLarge)?;
            }
            b"IEND" => {
                if !chunk.data.is_empty() || !saw_idat {
                    return Err(PngError::InvalidChunkOrder);
                }
                saw_end = true;
                offset = chunk.next;
                break;
            }
            b"PLTE" => {
                let (color_type, bit_depth) = header
                    .as_ref()
                    .map(|(_, _, color_type, bit_depth, _)| (*color_type, *bit_depth))
                    .ok_or(PngError::InvalidChunkOrder)?;
                if saw_palette
                    || saw_transparency
                    || saw_idat
                    || !matches!(color_type, 2 | 3 | 6)
                    || chunk.data.is_empty()
                    || chunk.data.len() > 256 * 3
                    || chunk.data.len() % 3 != 0
                    || color_type == 3 && chunk.data.len() / 3 > 1usize << usize::from(bit_depth)
                {
                    return Err(PngError::InvalidChunkOrder);
                }
                palette_length = chunk.data.len() / 3;
                for (index, rgb) in chunk.data.chunks_exact(3).enumerate() {
                    palette[index] |=
                        u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]);
                }
                saw_palette = true;
            }
            b"tRNS" => {
                let (color_type, bit_depth) = header
                    .as_ref()
                    .map(|(_, _, color_type, bit_depth, _)| (*color_type, *bit_depth))
                    .ok_or(PngError::InvalidChunkOrder)?;
                if saw_transparency || saw_idat {
                    return Err(PngError::InvalidChunkOrder);
                }
                transparency = match color_type {
                    0 if chunk.data.len() == 2
                        && chunk.data[0] == 0
                        && u16::from(chunk.data[1]) < 1u16 << bit_depth =>
                    {
                        Transparency::Gray(chunk.data[1])
                    }
                    2 if chunk.data.len() == 6
                        && chunk.data[0] == 0
                        && chunk.data[2] == 0
                        && chunk.data[4] == 0 =>
                    {
                        Transparency::Rgb(chunk.data[1], chunk.data[3], chunk.data[5])
                    }
                    3 if saw_palette
                        && !chunk.data.is_empty()
                        && chunk.data.len() <= palette_length =>
                    {
                        for (entry, alpha) in palette.iter_mut().zip(chunk.data.iter().copied()) {
                            *entry = (*entry & 0x00ff_ffff) | u32::from(alpha) << 24;
                        }
                        Transparency::None
                    }
                    _ => return Err(PngError::UnsupportedColor),
                };
                saw_transparency = true;
            }
            _ => {
                if saw_idat {
                    ended_idat = true;
                }
                if kind[0].is_ascii_uppercase() {
                    return Err(PngError::InvalidChunkOrder);
                }
            }
        }
        offset = chunk.next;
    }
    if !saw_end {
        return Err(PngError::MissingEnd);
    }
    if offset != input.len() {
        return Err(PngError::TrailingData);
    }
    if idat_length == 0 {
        return Err(PngError::MissingImageData);
    }
    let (width, height, color_type, bit_depth, channels) = header.ok_or(PngError::MissingHeader)?;
    if color_type == 3 && !saw_palette {
        return Err(PngError::MissingPalette);
    }
    Ok(PngMetadata {
        width,
        height,
        channels,
        bit_depth,
        color_type,
        idat_length,
        palette,
        palette_length,
        transparency,
    })
}

struct PngChunk<'a> {
    kind: &'a [u8; 4],
    data: &'a [u8],
    next: usize,
}

fn png_chunk(input: &[u8], offset: usize) -> Result<PngChunk<'_>, PngError> {
    let header = input
        .get(offset..offset.checked_add(8).ok_or(PngError::TruncatedChunk)?)
        .ok_or(PngError::TruncatedChunk)?;
    let length = usize::try_from(u32::from_be_bytes(
        header[..4]
            .try_into()
            .map_err(|_| PngError::TruncatedChunk)?,
    ))
    .map_err(|_| PngError::OutputTooLarge)?;
    let data_start = offset + 8;
    let data_end = data_start
        .checked_add(length)
        .ok_or(PngError::TruncatedChunk)?;
    let next = data_end.checked_add(4).ok_or(PngError::TruncatedChunk)?;
    let data = input
        .get(data_start..data_end)
        .ok_or(PngError::TruncatedChunk)?;
    let stored_crc = input.get(data_end..next).ok_or(PngError::TruncatedChunk)?;
    let kind: &[u8; 4] = header[4..8]
        .try_into()
        .map_err(|_| PngError::TruncatedChunk)?;
    let expected_crc = u32::from_be_bytes(
        stored_crc
            .try_into()
            .map_err(|_| PngError::TruncatedChunk)?,
    );
    let mut crc = crc32(u32::MAX, kind);
    crc = crc32(crc, data);
    if !crc != expected_crc {
        return Err(PngError::InvalidChunkCrc);
    }
    Ok(PngChunk { kind, data, next })
}

fn parse_header(data: &[u8]) -> Result<(usize, usize, u8, u8, usize), PngError> {
    if data.len() != 13 {
        return Err(PngError::InvalidHeader);
    }
    let width = usize::try_from(u32::from_be_bytes(
        data[..4].try_into().map_err(|_| PngError::InvalidHeader)?,
    ))
    .map_err(|_| PngError::OutputTooLarge)?;
    let height = usize::try_from(u32::from_be_bytes(
        data[4..8].try_into().map_err(|_| PngError::InvalidHeader)?,
    ))
    .map_err(|_| PngError::OutputTooLarge)?;
    if width == 0 || height == 0 || width > usize::from(u16::MAX) || height > usize::from(u16::MAX)
    {
        return Err(PngError::InvalidHeader);
    }
    let channels = match (data[9], data[8]) {
        (0, 1 | 2 | 4 | 8) => 1,
        (2, 8) => 3,
        (3, 1 | 2 | 4 | 8) => 1,
        (4, 8) => 2,
        (6, 8) => 4,
        _ => return Err(PngError::UnsupportedColor),
    };
    if data[10] != 0 || data[11] != 0 {
        return Err(PngError::InvalidHeader);
    }
    if data[12] != 0 {
        return Err(PngError::UnsupportedInterlace);
    }
    Ok((width, height, data[9], data[8], channels))
}

fn compact_idat(input: &mut [u8], expected: usize) -> Result<(), PngError> {
    let mut offset = PNG_SIGNATURE.len();
    let mut destination = 0usize;
    while offset < input.len() {
        let header = input
            .get(offset..offset + 8)
            .ok_or(PngError::TruncatedChunk)?;
        let length = usize::try_from(u32::from_be_bytes(
            header[..4]
                .try_into()
                .map_err(|_| PngError::TruncatedChunk)?,
        ))
        .map_err(|_| PngError::OutputTooLarge)?;
        let kind: [u8; 4] = header[4..8]
            .try_into()
            .map_err(|_| PngError::TruncatedChunk)?;
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(length)
            .ok_or(PngError::TruncatedChunk)?;
        let next = data_end.checked_add(4).ok_or(PngError::TruncatedChunk)?;
        if kind == *b"IDAT" {
            let destination_end = destination
                .checked_add(length)
                .ok_or(PngError::OutputTooLarge)?;
            input.copy_within(data_start..data_end, destination);
            destination = destination_end;
        }
        if kind == *b"IEND" {
            break;
        }
        offset = next;
    }
    if destination != expected {
        return Err(PngError::MissingImageData);
    }
    Ok(())
}

fn crc32(seed: u32, bytes: &[u8]) -> u32 {
    let mut crc = seed;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    crc
}

fn inflate_zlib(input: &[u8], output: &mut [u8]) -> Result<usize, PngError> {
    if input.len() < 6 {
        return Err(PngError::InvalidZlibHeader);
    }
    let cmf = input[0];
    let flags = input[1];
    if cmf & 0x0f != 8
        || cmf >> 4 > 7
        || (u16::from(cmf) << 8 | u16::from(flags)) % 31 != 0
        || flags & 0x20 != 0
    {
        return Err(PngError::InvalidZlibHeader);
    }
    let checksum_start = input.len() - 4;
    let mut bits = BitReader::new(&input[2..checksum_start]);
    let mut output_length = 0usize;
    loop {
        let final_block = bits.read_bits(1)? != 0;
        match bits.read_bits(2)? {
            0 => inflate_stored(&mut bits, output, &mut output_length)?,
            1 => {
                let (literal, distance) = fixed_huffman()?;
                inflate_huffman(&mut bits, output, &mut output_length, &literal, &distance)?;
            }
            2 => {
                let (literal, distance) = dynamic_huffman(&mut bits)?;
                inflate_huffman(&mut bits, output, &mut output_length, &literal, &distance)?;
            }
            _ => return Err(PngError::InvalidDeflate),
        }
        if final_block {
            break;
        }
    }
    if bits.position != bits.bytes.len() {
        return Err(PngError::InvalidDeflate);
    }
    let expected = u32::from_be_bytes(
        input[checksum_start..]
            .try_into()
            .map_err(|_| PngError::InvalidZlibHeader)?,
    );
    if adler32(&output[..output_length]) != expected {
        return Err(PngError::InvalidAdler32);
    }
    Ok(output_length)
}

struct BitReader<'a> {
    bytes: &'a [u8],
    position: usize,
    buffer: u32,
    count: u8,
}

impl<'a> BitReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            buffer: 0,
            count: 0,
        }
    }

    fn read_bits(&mut self, count: u8) -> Result<u16, PngError> {
        while self.count < count {
            let byte = *self
                .bytes
                .get(self.position)
                .ok_or(PngError::InvalidDeflate)?;
            self.buffer |= u32::from(byte) << self.count;
            self.count += 8;
            self.position += 1;
        }
        let mask = if count == 0 { 0 } else { (1u32 << count) - 1 };
        let value = self.buffer & mask;
        self.buffer >>= count;
        self.count -= count;
        Ok(value as u16)
    }

    fn align_byte(&mut self) {
        self.buffer = 0;
        self.count = 0;
    }

    fn read_aligned_u16(&mut self) -> Result<u16, PngError> {
        let bytes = self
            .bytes
            .get(self.position..self.position + 2)
            .ok_or(PngError::InvalidDeflate)?;
        self.position += 2;
        Ok(u16::from_le_bytes(
            bytes.try_into().map_err(|_| PngError::InvalidDeflate)?,
        ))
    }

    fn read_aligned_bytes(&mut self, length: usize) -> Result<&'a [u8], PngError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(PngError::InvalidDeflate)?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or(PngError::InvalidDeflate)?;
        self.position = end;
        Ok(bytes)
    }
}

fn inflate_stored(
    bits: &mut BitReader<'_>,
    output: &mut [u8],
    output_length: &mut usize,
) -> Result<(), PngError> {
    bits.align_byte();
    let length = bits.read_aligned_u16()?;
    if bits.read_aligned_u16()? != !length {
        return Err(PngError::InvalidDeflate);
    }
    let bytes = bits.read_aligned_bytes(usize::from(length))?;
    let end = output_length
        .checked_add(bytes.len())
        .ok_or(PngError::OutputTooLarge)?;
    output
        .get_mut(*output_length..end)
        .ok_or(PngError::OutputTooLarge)?
        .copy_from_slice(bytes);
    *output_length = end;
    Ok(())
}

struct Huffman<const N: usize> {
    counts: [u16; MAX_BITS + 1],
    symbols: [u16; N],
}

impl<const N: usize> Huffman<N> {
    fn build(lengths: &[u8]) -> Result<Self, PngError> {
        if lengths.len() > N {
            return Err(PngError::InvalidDeflate);
        }
        let mut counts = [0u16; MAX_BITS + 1];
        for &length in lengths {
            if usize::from(length) > MAX_BITS {
                return Err(PngError::InvalidDeflate);
            }
            counts[usize::from(length)] += 1;
        }
        if counts[0] as usize == lengths.len() {
            return Err(PngError::InvalidDeflate);
        }
        let mut left = 1i32;
        for &count in counts.iter().skip(1) {
            left = (left << 1) - i32::from(count);
            if left < 0 {
                return Err(PngError::InvalidDeflate);
            }
        }
        let mut offsets = [0u16; MAX_BITS + 1];
        for bits in 1..MAX_BITS {
            offsets[bits + 1] = offsets[bits] + counts[bits];
        }
        let mut symbols = [0u16; N];
        for (symbol, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let offset = &mut offsets[usize::from(length)];
                symbols[usize::from(*offset)] = symbol as u16;
                *offset += 1;
            }
        }
        Ok(Self { counts, symbols })
    }

    fn decode(&self, bits: &mut BitReader<'_>) -> Result<u16, PngError> {
        let mut code = 0u16;
        let mut first = 0u16;
        let mut index = 0u16;
        for length in 1..=MAX_BITS {
            code |= bits.read_bits(1)?;
            let count = self.counts[length];
            if code < first + count {
                return self
                    .symbols
                    .get(usize::from(index + code - first))
                    .copied()
                    .ok_or(PngError::InvalidDeflate);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        Err(PngError::InvalidDeflate)
    }
}

fn fixed_huffman() -> Result<(Huffman<288>, Huffman<32>), PngError> {
    let mut literal_lengths = [0u8; 288];
    literal_lengths[..144].fill(8);
    literal_lengths[144..256].fill(9);
    literal_lengths[256..280].fill(7);
    literal_lengths[280..].fill(8);
    let distance_lengths = [5u8; 32];
    Ok((
        Huffman::build(&literal_lengths)?,
        Huffman::build(&distance_lengths)?,
    ))
}

fn dynamic_huffman(bits: &mut BitReader<'_>) -> Result<(Huffman<288>, Huffman<32>), PngError> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];

    let literal_count = usize::from(bits.read_bits(5)?) + 257;
    let distance_count = usize::from(bits.read_bits(5)?) + 1;
    let code_count = usize::from(bits.read_bits(4)?) + 4;
    if literal_count > 286 || distance_count > 32 {
        return Err(PngError::InvalidDeflate);
    }
    let mut code_lengths = [0u8; 19];
    for &symbol in &ORDER[..code_count] {
        code_lengths[symbol] = bits.read_bits(3)? as u8;
    }
    let code_huffman = Huffman::<19>::build(&code_lengths)?;
    let total = literal_count + distance_count;
    let mut lengths = [0u8; 320];
    let mut index = 0usize;
    while index < total {
        match code_huffman.decode(bits)? {
            length @ 0..=15 => {
                lengths[index] = length as u8;
                index += 1;
            }
            16 => {
                if index == 0 {
                    return Err(PngError::InvalidDeflate);
                }
                let repeat = usize::from(bits.read_bits(2)?) + 3;
                if index + repeat > total {
                    return Err(PngError::InvalidDeflate);
                }
                let previous = lengths[index - 1];
                lengths[index..index + repeat].fill(previous);
                index += repeat;
            }
            17 => {
                let repeat = usize::from(bits.read_bits(3)?) + 3;
                if index + repeat > total {
                    return Err(PngError::InvalidDeflate);
                }
                lengths[index..index + repeat].fill(0);
                index += repeat;
            }
            18 => {
                let repeat = usize::from(bits.read_bits(7)?) + 11;
                if index + repeat > total {
                    return Err(PngError::InvalidDeflate);
                }
                lengths[index..index + repeat].fill(0);
                index += repeat;
            }
            _ => return Err(PngError::InvalidDeflate),
        }
    }
    if lengths[256] == 0 {
        return Err(PngError::InvalidDeflate);
    }
    Ok((
        Huffman::build(&lengths[..literal_count])?,
        Huffman::build(&lengths[literal_count..total])?,
    ))
}

fn inflate_huffman(
    bits: &mut BitReader<'_>,
    output: &mut [u8],
    output_length: &mut usize,
    literal: &Huffman<288>,
    distance: &Huffman<32>,
) -> Result<(), PngError> {
    const LENGTH_BASE: [u16; 29] = [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
        131, 163, 195, 227, 258,
    ];
    const LENGTH_EXTRA: [u8; 29] = [
        0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
    ];
    const DISTANCE_BASE: [u16; 30] = [
        1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
        2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
    ];
    const DISTANCE_EXTRA: [u8; 30] = [
        0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
        13, 13,
    ];

    loop {
        match literal.decode(bits)? {
            value @ 0..=255 => {
                let slot = output
                    .get_mut(*output_length)
                    .ok_or(PngError::OutputTooLarge)?;
                *slot = value as u8;
                *output_length += 1;
            }
            256 => return Ok(()),
            symbol @ 257..=285 => {
                let length_index = usize::from(symbol - 257);
                let length = usize::from(LENGTH_BASE[length_index])
                    + usize::from(bits.read_bits(LENGTH_EXTRA[length_index])?);
                let distance_symbol = usize::from(distance.decode(bits)?);
                if distance_symbol >= DISTANCE_BASE.len() {
                    return Err(PngError::InvalidDeflate);
                }
                let distance = usize::from(DISTANCE_BASE[distance_symbol])
                    + usize::from(bits.read_bits(DISTANCE_EXTRA[distance_symbol])?);
                if distance == 0 || distance > *output_length {
                    return Err(PngError::InvalidDeflate);
                }
                for _ in 0..length {
                    let source = *output_length - distance;
                    let byte = *output.get(source).ok_or(PngError::InvalidDeflate)?;
                    let destination = output
                        .get_mut(*output_length)
                        .ok_or(PngError::OutputTooLarge)?;
                    *destination = byte;
                    *output_length += 1;
                }
            }
            _ => return Err(PngError::InvalidDeflate),
        }
    }
}

fn adler32(bytes: &[u8]) -> u32 {
    const MODULUS: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in bytes {
        a = (a + u32::from(byte)) % MODULUS;
        b = (b + a) % MODULUS;
    }
    b << 16 | a
}

fn unfilter_scanlines(
    bytes: &mut [u8],
    row_bytes: usize,
    height: usize,
    bytes_per_pixel: usize,
) -> Result<(), PngError> {
    let stride = row_bytes + 1;
    for row in 0..height {
        let start = row * stride + 1;
        let filter = bytes[start - 1];
        for column in 0..row_bytes {
            let index = start + column;
            let left = if column >= bytes_per_pixel {
                bytes[index - bytes_per_pixel]
            } else {
                0
            };
            let up = if row > 0 { bytes[index - stride] } else { 0 };
            let upper_left = if row > 0 && column >= bytes_per_pixel {
                bytes[index - stride - bytes_per_pixel]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, upper_left),
                _ => return Err(PngError::InvalidFilter),
            };
            bytes[index] = bytes[index].wrapping_add(predictor);
        }
    }
    Ok(())
}

fn paeth(left: u8, up: u8, upper_left: u8) -> u8 {
    let left = i32::from(left);
    let up = i32::from(up);
    let upper_left = i32::from(upper_left);
    let estimate = left + up - upper_left;
    let left_distance = (estimate - left).abs();
    let up_distance = (estimate - up).abs();
    let upper_left_distance = (estimate - upper_left).abs();
    if left_distance <= up_distance && left_distance <= upper_left_distance {
        left as u8
    } else if up_distance <= upper_left_distance {
        up as u8
    } else {
        upper_left as u8
    }
}

fn compact_rgb(bytes: &mut [u8], metadata: &PngMetadata) -> Result<(), PngError> {
    let width = metadata.width;
    let height = metadata.height;
    if metadata.color_type == 3 {
        let row_bytes = (width * usize::from(metadata.bit_depth)).div_ceil(8);
        let source_stride = row_bytes + 1;
        let mask = u8::MAX >> (8 - metadata.bit_depth);
        for row in (0..height).rev() {
            let source_start = row * source_stride + 1;
            for pixel in (0..width).rev() {
                let bit = pixel * usize::from(metadata.bit_depth);
                let source = bytes[source_start + bit / 8];
                let shift = 8 - usize::from(metadata.bit_depth) - bit % 8;
                let index = usize::from(source >> shift & mask);
                if index >= metadata.palette_length {
                    return Err(PngError::InvalidPaletteIndex);
                }
                let color = metadata.palette[index];
                let destination = (row * width + pixel) * 3;
                let alpha = (color >> 24) as u8;
                bytes[destination] = alpha_blend_black((color >> 16) as u8, alpha);
                bytes[destination + 1] = alpha_blend_black((color >> 8) as u8, alpha);
                bytes[destination + 2] = alpha_blend_black(color as u8, alpha);
            }
        }
        return Ok(());
    }

    if metadata.color_type == 0 && metadata.bit_depth < 8 {
        let bit_depth = usize::from(metadata.bit_depth);
        let row_bytes = (width * bit_depth).div_ceil(8);
        let source_stride = row_bytes + 1;
        let mask = u8::MAX >> (8 - metadata.bit_depth);
        for row in (0..height).rev() {
            let source_start = row * source_stride + 1;
            for pixel in (0..width).rev() {
                let bit = pixel * bit_depth;
                let source = bytes[source_start + bit / 8];
                let shift = 8 - bit_depth - bit % 8;
                let sample = source >> shift & mask;
                let value = if matches!(metadata.transparency, Transparency::Gray(key) if sample == key)
                {
                    0
                } else {
                    scale_gray_sample(sample, mask)
                };
                let destination = (row * width + pixel) * 3;
                bytes[destination] = value;
                bytes[destination + 1] = value;
                bytes[destination + 2] = value;
            }
        }
        return Ok(());
    }

    let channels = metadata.channels;
    let source_stride = width * channels + 1;
    if matches!(metadata.color_type, 0 | 4) {
        for row in (0..height).rev() {
            let source_start = row * source_stride + 1;
            for pixel in (0..width).rev() {
                let source = source_start + pixel * channels;
                let destination = (row * width + pixel) * 3;
                let gray = bytes[source];
                let value = if metadata.color_type == 4 {
                    alpha_blend_black(gray, bytes[source + 1])
                } else if matches!(metadata.transparency, Transparency::Gray(key) if gray == key) {
                    0
                } else {
                    gray
                };
                bytes[destination] = value;
                bytes[destination + 1] = value;
                bytes[destination + 2] = value;
            }
        }
        return Ok(());
    }

    let mut destination = 0usize;
    for row in 0..height {
        let source_start = row * source_stride + 1;
        for pixel in 0..width {
            let source = source_start + pixel * channels;
            let (red, green, blue) = match channels {
                3 => {
                    let color = (bytes[source], bytes[source + 1], bytes[source + 2]);
                    if matches!(
                        metadata.transparency,
                        Transparency::Rgb(red, green, blue) if color == (red, green, blue)
                    ) {
                        (0, 0, 0)
                    } else {
                        color
                    }
                }
                4 => {
                    let alpha = bytes[source + 3];
                    (
                        alpha_blend_black(bytes[source], alpha),
                        alpha_blend_black(bytes[source + 1], alpha),
                        alpha_blend_black(bytes[source + 2], alpha),
                    )
                }
                _ => unreachable!(),
            };
            bytes[destination] = red;
            bytes[destination + 1] = green;
            bytes[destination + 2] = blue;
            destination += 3;
        }
    }
    Ok(())
}

fn alpha_blend_black(value: u8, alpha: u8) -> u8 {
    ((u16::from(value) * u16::from(alpha) + 127) / 255) as u8
}

fn scale_gray_sample(sample: u8, maximum: u8) -> u8 {
    ((u16::from(sample) * 255 + u16::from(maximum) / 2) / u16::from(maximum)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const STORED_ZLIB: &[u8] =
        b"\x78\x01\x01\x18\x00\xe7\xffstored block: 0123456789\x67\x69\x07\x24";
    const FIXED_ZLIB: &[u8] = b"\x78\x01\x2b\x2e\xc9\x2f\x4a\x4d\x51\x48\xca\xc9\x4f\xce\xb6\x52\x30\x30\x34\x32\x36\x31\x35\x33\xb7\xb0\x2c\xa6\x92\x38\x00\xa1\x9f\x1c\x8d";
    const DYNAMIC_ZLIB: &[u8] =
        b"\x78\xda\xed\xc1\x01\x0d\x00\x00\x00\xc2\xa0\xbd\x7f\x69\x7b\x38\xa0\x00\x00\x00\x80\x77\x03\x1f\x80\x10\x01";
    const RGB_PNG: [u8; 93] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x03\x00\x00\x00\x02\x08\x02\x00\x00\x00\x12\x16\xf1\x4d\x00\x00\x00\x0cIDAT\x78\xda\x63\xfc\xcf\xc0\xc0\x08\xc6\x2c\xdc\x22\x1b\x87\x74\x0b\x00\x00\x00\x0cIDAT\x72\x1a\xc6\x72\x72\x01\xd1\x00\x32\xd0\x04\x84\xad\x12\x25\x6a\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const RGBA_PNG: [u8; 74] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x01\x08\x06\x00\x00\x00\xf4\x22\x7f\x8a\x00\x00\x00\x11IDAT\x78\x9c\x63\xf8\xcf\xc0\xd0\xc0\x70\x22\xe5\x3f\x00\x0e\xa0\x03\xab\x46\xf7\xf4\xd2\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY_PNG: [u8; 71] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x00\x00\x00\x00\x57\xdd\x52\xf8\x00\x00\x00\x0eIDAT\x78\xda\x63\xe0\x12\x61\x90\xd3\x00\x00\x00\xec\x00\x65\xfd\x90\x12\xa5\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY_ALPHA_PNG: [u8; 75] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x02\x08\x04\x00\x00\x00\xd8\xbf\xc5\xaf\x00\x00\x00\x12IDAT\x78\xda\x63\x48\xf9\x7f\xa2\x81\xc1\xc8\xe1\x3f\x03\x00\x17\x84\x04\x1d\x93\x8f\x7d\xc2\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const PALETTE_PNG: [u8; 105] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x03\x00\x00\x00\x02\x02\x03\x00\x00\x00\xe0\x1a\x8e\x89\x00\x00\x00\x09PLTE\xff\x00\x00\x00\xff\x00\x00\x00\xff\x2d\x4a\xcd\x8a\x00\x00\x00\x03tRNS\xff\x80\x00\x7f\x6d\x68\x78\x00\x00\x00\x0cIDAT\x78\xda\x63\x90\x60\x98\x00\x00\x00\xdc\x00\xa9\x52\x1a\x13\x8f\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const INVALID_PALETTE_INDEX_PNG: [u8; 85] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x02\x03\x00\x00\x00\x62\x7b\x2c\x1a\x00\x00\x00\x06PLTE\xff\x00\x00\x00\xff\x00\xd2\x87\xef\x71\x00\x00\x00\x0aIDAT\x78\xda\x63\x68\x00\x00\x00\x82\x00\x81\xda\x45\x08\x3b\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const RGB_TRANSPARENT_PNG: [u8; 90] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x01\x08\x02\x00\x00\x00\x7b\x40\xe8\xdd\x00\x00\x00\x06tRNS\x00\x0a\x00\x14\x00\x1e\xc5\x36\x29\xff\x00\x00\x00\x0fIDAT\x78\xda\x63\xe0\x12\x91\xd3\x30\xb2\x01\x00\x02\x37\x00\xd3\xe2\x2d\xed\x9f\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY_TRANSPARENT_PNG: [u8; 82] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x02\x00\x00\x00\x01\x08\x00\x00\x00\x00\xd1\x49\x20\x56\x00\x00\x00\x02tRNS\x00\x14\x6c\x49\x19\x45\x00\x00\x00\x0bIDAT\x78\xda\x63\xe0\x12\x01\x00\x00\x2b\x00\x1f\x04\xc8\xf0\xc2\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY1_PNG: [u8; 71] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x09\x00\x00\x00\x02\x01\x00\x00\x00\x00\xa2\x2d\xcb\x7e\x00\x00\x00\x0eIDAT\x78\xda\x63\x88\x6a\x60\x5c\x1a\x0d\x00\x05\x70\x01\xdc\x45\x01\xa4\xc6\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY2_TRANSPARENT_PNG: [u8; 85] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x05\x00\x00\x00\x02\x02\x00\x00\x00\x00\xff\xb1\x51\x20\x00\x00\x00\x02tRNS\x00\x02\x98\x9d\xac\x14\x00\x00\x00\x0eIDAT\x78\xda\x63\x90\x76\x60\xba\xd1\x00\x00\x03\xc3\x01\xb6\x14\x89\x83\xb4\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY4_TRANSPARENT_PNG: [u8; 87] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x05\x00\x00\x00\x02\x04\x00\x00\x00\x00\x70\xf1\xa4\x80\x00\x00\x00\x02tRNS\x00\x05\x06\xf9\x39\xb7\x00\x00\x00\x10IDAT\x78\xda\x63\x60\x5d\x2f\xc0\xf2\x55\x60\x3d\x00\x08\x53\x02\x7d\x11\x4d\xa8\x78\x00\x00\x00\x00IEND\xae\x42\x60\x82";
    const GRAY2_INVALID_TRANSPARENCY_PNG: [u8; 81] = *b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x02\x00\x00\x00\x00\x70\xce\x83\xf4\x00\x00\x00\x02tRNS\x00\x04\x71\xfe\x09\x21\x00\x00\x00\x0aIDAT\x78\xda\x63\x60\x00\x00\x00\x02\x00\x01\xe5\x27\xde\xfc\x00\x00\x00\x00IEND\xae\x42\x60\x82";

    #[test]
    fn inflates_stored_fixed_and_dynamic_blocks() {
        let phrase = b"stored block: 0123456789";
        let mut output = [0u8; 4096];
        assert_eq!(
            inflate_zlib(STORED_ZLIB, &mut output).unwrap(),
            phrase.len()
        );
        assert_eq!(&output[..phrase.len()], phrase);

        let length = inflate_zlib(FIXED_ZLIB, &mut output).unwrap();
        assert_eq!(length, phrase.len() * 4);
        for repeated in output[..length].chunks_exact(phrase.len()) {
            assert_eq!(repeated, phrase);
        }
        let mut trailing = [0u8; FIXED_ZLIB.len() + 1];
        let checksum = FIXED_ZLIB.len() - 4;
        trailing[..checksum].copy_from_slice(&FIXED_ZLIB[..checksum]);
        trailing[checksum] = 0;
        trailing[checksum + 1..].copy_from_slice(&FIXED_ZLIB[checksum..]);
        assert_eq!(
            inflate_zlib(&trailing, &mut output),
            Err(PngError::InvalidDeflate)
        );

        assert_eq!(
            inflate_zlib(DYNAMIC_ZLIB, &mut output).unwrap(),
            output.len()
        );
        assert!(output.iter().all(|byte| *byte == 1));
    }

    #[test]
    fn decodes_multichunk_filtered_rgb_and_rgba_pngs() {
        let mut rgb_png = RGB_PNG;
        let mut output = [0u8; 128];
        let decoded = decode_png_rgb(&mut rgb_png, &mut output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
        assert_eq!(decoded.rgb_length(), 18);
        assert_eq!(
            &output[..decoded.rgb_length()],
            b"\xff\x00\x00\x00\xff\x00\x00\x00\xff\x0a\x14\x1e\x28\x32\x3c\x46\x50\x5a"
        );

        let mut rgba_png = RGBA_PNG;
        let decoded = decode_png_rgb(&mut rgba_png, &mut output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 1));
        assert_eq!(&output[..decoded.rgb_length()], b"\x80\x00\x00\x00\xc8\x64");

        let mut gray_png = GRAY_PNG;
        assert_eq!(
            decode_png_rgb(&mut gray_png, &mut output[..11]),
            Err(PngError::OutputTooLarge)
        );
        let decoded = decode_png_rgb(&mut gray_png, &mut output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        assert_eq!(
            &output[..decoded.rgb_length()],
            b"\x0a\x0a\x0a\x14\x14\x14\x1e\x1e\x1e\x28\x28\x28"
        );

        let mut gray_alpha_png = GRAY_ALPHA_PNG;
        let decoded = decode_png_rgb(&mut gray_alpha_png, &mut output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        assert_eq!(
            &output[..decoded.rgb_length()],
            b"\x64\x64\x64\x64\x64\x64\x0d\x0d\x0d\x00\x00\x00"
        );

        let mut palette_png = PALETTE_PNG;
        let decoded = decode_png_rgb(&mut palette_png, &mut output).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (3, 2));
        assert_eq!(
            &output[..decoded.rgb_length()],
            b"\xff\x00\x00\x00\x80\x00\x00\x00\x00\x00\x00\x00\x00\x80\x00\xff\x00\x00"
        );

        let mut rgb_transparent_png = RGB_TRANSPARENT_PNG;
        let decoded = decode_png_rgb(&mut rgb_transparent_png, &mut output).unwrap();
        assert_eq!(&output[..decoded.rgb_length()], b"\x00\x00\x00\x28\x32\x3c");

        let mut gray_transparent_png = GRAY_TRANSPARENT_PNG;
        let decoded = decode_png_rgb(&mut gray_transparent_png, &mut output).unwrap();
        assert_eq!(&output[..decoded.rgb_length()], b"\x0a\x0a\x0a\x00\x00\x00");

        for (png, expected) in [
            (
                GRAY1_PNG.as_slice(),
                [
                    0, 255, 0, 255, 255, 0, 255, 0, 255, 255, 0, 255, 0, 0, 255, 0, 255, 0,
                ]
                .as_slice(),
            ),
            (
                GRAY2_TRANSPARENT_PNG.as_slice(),
                [0, 85, 0, 255, 85, 255, 255, 0, 255, 255].as_slice(),
            ),
            (
                GRAY4_TRANSPARENT_PNG.as_slice(),
                [0, 0, 170, 255, 17, 255, 170, 0, 170, 187].as_slice(),
            ),
        ] {
            let mut input = [0u8; GRAY4_TRANSPARENT_PNG.len()];
            input[..png.len()].copy_from_slice(png);
            let decoded = decode_png_rgb(&mut input[..png.len()], &mut output).unwrap();
            assert_eq!(decoded.rgb_length(), expected.len() * 3);
            for (rgb, gray) in output[..decoded.rgb_length()]
                .chunks_exact(3)
                .zip(expected.iter().copied())
            {
                assert_eq!(rgb, [gray, gray, gray]);
            }
        }
    }

    #[test]
    fn rejects_corrupt_png_boundaries() {
        let mut png = RGB_PNG;
        png[20] ^= 1;
        let mut output = [0u8; 128];
        assert_eq!(
            decode_png_rgb(&mut png, &mut output),
            Err(PngError::InvalidChunkCrc)
        );

        let mut png = RGB_PNG;
        assert_eq!(
            decode_png_rgb(&mut png, &mut output[..10]),
            Err(PngError::OutputTooLarge)
        );

        let mut png = INVALID_PALETTE_INDEX_PNG;
        assert_eq!(
            decode_png_rgb(&mut png, &mut output),
            Err(PngError::InvalidPaletteIndex)
        );

        let mut png = GRAY2_INVALID_TRANSPARENCY_PNG;
        assert_eq!(
            decode_png_rgb(&mut png, &mut output),
            Err(PngError::UnsupportedColor)
        );
    }
}
