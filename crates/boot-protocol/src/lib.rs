// SPDX-License-Identifier: 0BSD

#![no_std]

pub const BOOT_INFO_MAGIC: u64 = 0x534c_4f50_4f53_4249;
pub const BOOT_INFO_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PixelFormat {
    Rgb = 0,
    Bgr = 1,
    Bitmask = 2,
    Unknown = u32::MAX,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FramebufferInfo {
    pub base: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub pixel_format: PixelFormat,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MemoryMapInfo {
    pub base: u64,
    pub size: u64,
    pub descriptor_size: u32,
    pub descriptor_version: u32,
    pub descriptor_count: u32,
    pub _reserved: u32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct InitrdInfo {
    pub base: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct KernelImageInfo {
    pub physical_start: u64,
    pub physical_end: u64,
    pub entry: u64,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct BootInfo {
    pub magic: u64,
    pub version: u32,
    pub struct_size: u32,
    pub framebuffer: FramebufferInfo,
    pub memory_map: MemoryMapInfo,
    pub acpi_rsdp: u64,
    pub initrd: InitrdInfo,
    pub kernel: KernelImageInfo,
}
