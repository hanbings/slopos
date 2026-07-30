// SPDX-License-Identifier: 0BSD

#![no_std]

pub const DESCRIPTOR_NEXT: u16 = 1;
pub const DESCRIPTOR_WRITE: u16 = 2;
pub const BLOCK_REQUEST_IN: u32 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Descriptor {
    pub address: u64,
    pub length: u32,
    pub flags: u16,
    pub next: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct BlockRequestHeader {
    pub request_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SplitQueueLayout {
    pub descriptor_bytes: usize,
    pub available_bytes: usize,
    pub used_bytes: usize,
}

impl SplitQueueLayout {
    pub const fn for_queue_size(queue_size: u16) -> Self {
        let count = queue_size as usize;
        Self {
            descriptor_bytes: count * 16,
            available_bytes: 6 + count * 2,
            used_bytes: 6 + count * 8,
        }
    }
}

pub const fn choose_queue_size(offered: u16, limit: u16) -> Option<u16> {
    let maximum = if offered < limit { offered } else { limit };
    if maximum < 4 {
        return None;
    }
    Some(1 << (15 - maximum.leading_zeros()))
}

pub const fn block_read_descriptors(
    header_address: u64,
    header_length: u32,
    data_address: u64,
    data_length: u32,
    status_address: u64,
) -> [Descriptor; 3] {
    [
        Descriptor {
            address: header_address,
            length: header_length,
            flags: DESCRIPTOR_NEXT,
            next: 1,
        },
        Descriptor {
            address: data_address,
            length: data_length,
            flags: DESCRIPTOR_NEXT | DESCRIPTOR_WRITE,
            next: 2,
        },
        Descriptor {
            address: status_address,
            length: 1,
            flags: DESCRIPTOR_WRITE,
            next: 0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn uses_specified_split_queue_layout() {
        assert_eq!(
            SplitQueueLayout::for_queue_size(8),
            SplitQueueLayout {
                descriptor_bytes: 128,
                available_bytes: 22,
                used_bytes: 70
            }
        );
        assert_eq!(size_of::<Descriptor>(), 16);
        assert_eq!(align_of::<Descriptor>(), 8);
        assert_eq!(size_of::<BlockRequestHeader>(), 16);
    }

    #[test]
    fn chooses_bounded_power_of_two_size() {
        assert_eq!(choose_queue_size(256, 8), Some(8));
        assert_eq!(choose_queue_size(7, 8), Some(4));
        assert_eq!(choose_queue_size(3, 8), None);
    }

    #[test]
    fn builds_three_descriptor_block_read_chain() {
        let descriptors = block_read_descriptors(0x1000, 16, 0x2000, 512, 0x3000);
        assert_eq!(
            descriptors,
            [
                Descriptor {
                    address: 0x1000,
                    length: 16,
                    flags: DESCRIPTOR_NEXT,
                    next: 1
                },
                Descriptor {
                    address: 0x2000,
                    length: 512,
                    flags: DESCRIPTOR_NEXT | DESCRIPTOR_WRITE,
                    next: 2
                },
                Descriptor {
                    address: 0x3000,
                    length: 1,
                    flags: DESCRIPTOR_WRITE,
                    next: 0
                }
            ]
        );
    }
}
