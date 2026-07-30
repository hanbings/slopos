// SPDX-License-Identifier: 0BSD

#![no_main]
#![no_std]

use core::arch::asm;
use core::ffi::c_void;
use core::fmt::{self, Write};
use core::mem::{self, size_of};
use core::panic::PanicInfo;
use core::ptr;
use slopos_boot_protocol::{
    BOOT_INFO_MAGIC, BOOT_INFO_VERSION, BootInfo, FramebufferInfo, InitrdInfo, KernelImageInfo,
    MemoryMapInfo, PixelFormat as BootPixelFormat,
};
use uefi_raw::protocol::console::{
    GraphicsOutputModeInformation, GraphicsOutputProtocol, GraphicsPixelFormat,
};
use uefi_raw::protocol::file_system::{
    FileAttribute, FileInfo, FileMode, FileProtocolV1, SimpleFileSystemProtocol,
};
use uefi_raw::protocol::loaded_image::LoadedImageProtocol;
use uefi_raw::table::boot::{AllocateType, BootServices, MemoryDescriptor, MemoryType};
use uefi_raw::table::system::SystemTable;
use uefi_raw::{Guid, Handle, Status, guid};

const PAGE_SIZE: usize = 4096;
const ACPI_GUID: Guid = guid!("eb9d2d30-2d88-11d3-9a16-0090273fc14d");
const ACPI2_GUID: Guid = guid!("8868e871-e4f1-11d3-bc22-0080c73c8881");

static KERNEL_PATH: &[u16] = &[
    b'\\' as u16,
    b's' as u16,
    b'l' as u16,
    b'o' as u16,
    b'p' as u16,
    b'o' as u16,
    b's' as u16,
    b'\\' as u16,
    b'k' as u16,
    b'e' as u16,
    b'r' as u16,
    b'n' as u16,
    b'e' as u16,
    b'l' as u16,
    b'.' as u16,
    b'e' as u16,
    b'l' as u16,
    b'f' as u16,
    0,
];
static INITRD_PATH: &[u16] = &[
    b'\\' as u16,
    b's' as u16,
    b'l' as u16,
    b'o' as u16,
    b'p' as u16,
    b'o' as u16,
    b's' as u16,
    b'\\' as u16,
    b'i' as u16,
    b'n' as u16,
    b'i' as u16,
    b't' as u16,
    b'r' as u16,
    b'd' as u16,
    b'.' as u16,
    b's' as u16,
    b'l' as u16,
    b'p' as u16,
    0,
];

/// UEFI application entry point.
///
/// # Safety
///
/// Firmware must pass a valid image handle and a live UEFI system table whose
/// boot-services table follows the x86-64 UEFI calling convention.
#[unsafe(no_mangle)]
pub unsafe extern "efiapi" fn efi_main(
    image_handle: Handle,
    system_table: *mut SystemTable,
) -> Status {
    uart_init();
    serialln(format_args!("SLOPOS-UEFI: loader entered"));
    // SAFETY: firmware invokes efi_main with live image and system-table handles.
    unsafe { loader_main(image_handle, system_table) }
}

unsafe fn loader_main(image_handle: Handle, system_table: *mut SystemTable) -> ! {
    if system_table.is_null() || unsafe { (*system_table).boot_services.is_null() } {
        fatal("invalid UEFI system table");
    }
    if unsafe { (*system_table).header.signature } != SystemTable::SIGNATURE {
        fatal("UEFI system table signature mismatch");
    }
    let boot_services = unsafe { &*(*system_table).boot_services };
    serialln(format_args!("SLOPOS-UEFI: raw protocol layer initialized"));

    let acpi_rsdp = unsafe { find_acpi_rsdp(&*system_table) };
    if acpi_rsdp == 0 {
        fatal("ACPI RSDP was not published by firmware");
    }
    serialln(format_args!("SLOPOS-UEFI: ACPI RSDP={acpi_rsdp:#x}"));

    let framebuffer = unsafe { discover_framebuffer(boot_services) };
    serialln(format_args!(
        "SLOPOS-UEFI: GOP {}x{} stride={} base={:#x}",
        framebuffer.width, framebuffer.height, framebuffer.stride, framebuffer.base
    ));

    let kernel_file = unsafe { read_boot_file(boot_services, image_handle, KERNEL_PATH) }
        .unwrap_or_else(|message| fatal(message));
    serialln(format_args!(
        "SLOPOS-UEFI: kernel file loaded bytes={}",
        kernel_file.size
    ));
    // SAFETY: the file allocation is initialized for exactly `size` bytes.
    let kernel_bytes =
        unsafe { core::slice::from_raw_parts(kernel_file.base as *const u8, kernel_file.size) };
    let kernel = unsafe { load_elf_kernel(boot_services, kernel_bytes) }
        .unwrap_or_else(|message| fatal(message));
    unsafe {
        let _ = (boot_services.free_pages)(kernel_file.base, kernel_file.page_count);
    }
    serialln(format_args!(
        "SLOPOS-UEFI: kernel mapped {:#x}..{:#x} entry={:#x}",
        kernel.physical_start, kernel.physical_end, kernel.entry
    ));

    let initrd_file = unsafe { read_boot_file(boot_services, image_handle, INITRD_PATH) }
        .unwrap_or_else(|message| fatal(message));
    let initrd = InitrdInfo {
        base: initrd_file.base,
        size: initrd_file.size as u64,
    };
    serialln(format_args!(
        "SLOPOS-UEFI: initrd loaded bytes={} base={:#x}",
        initrd.size, initrd.base
    ));

    let boot_info_address = unsafe {
        allocate_pages(
            boot_services,
            AllocateType::ANY_PAGES,
            MemoryType::LOADER_DATA,
            1,
            0,
        )
    }
    .unwrap_or_else(|message| fatal(message));
    let boot_info_pointer = boot_info_address as *mut BootInfo;
    // SAFETY: the page is an exclusive, correctly aligned allocation.
    unsafe {
        boot_info_pointer.write(BootInfo {
            magic: BOOT_INFO_MAGIC,
            version: BOOT_INFO_VERSION,
            struct_size: size_of::<BootInfo>() as u32,
            framebuffer,
            memory_map: MemoryMapInfo {
                base: 0,
                size: 0,
                descriptor_size: 0,
                descriptor_version: 0,
                descriptor_count: 0,
                _reserved: 0,
            },
            acpi_rsdp,
            initrd,
            kernel,
        })
    };

    let memory_map = unsafe { exit_boot_services(boot_services, image_handle) };
    // SAFETY: BootInfo resides in loader-owned memory and remains writable after exit.
    unsafe {
        (*boot_info_pointer).memory_map = MemoryMapInfo {
            base: memory_map.base,
            size: memory_map.size as u64,
            descriptor_size: memory_map.descriptor_size as u32,
            descriptor_version: memory_map.descriptor_version,
            descriptor_count: (memory_map.size / memory_map.descriptor_size) as u32,
            _reserved: 0,
        };
    }
    serialln(format_args!(
        "SLOPOS-UEFI: Boot Services exited descriptors={}",
        memory_map.size / memory_map.descriptor_size
    ));
    serialln(format_args!("SLOPOS-UEFI: transferring control to kernel"));

    let entry: unsafe extern "sysv64" fn(*const BootInfo) -> ! =
        // SAFETY: the checked ELF entry belongs to the loaded executable image.
        unsafe { mem::transmute(kernel.entry as usize) };
    // SAFETY: BootInfo is initialized and valid for the non-returning kernel entry.
    unsafe { entry(boot_info_pointer.cast_const()) }
}

#[derive(Clone, Copy)]
struct PageFile {
    base: u64,
    size: usize,
    page_count: usize,
}

unsafe fn read_boot_file(
    boot_services: &BootServices,
    image_handle: Handle,
    path: &[u16],
) -> Result<PageFile, &'static str> {
    let mut loaded_image_interface: *mut c_void = ptr::null_mut();
    let status = unsafe {
        (boot_services.handle_protocol)(
            image_handle,
            &LoadedImageProtocol::GUID,
            &mut loaded_image_interface,
        )
    };
    if !status.is_success() || loaded_image_interface.is_null() {
        return Err("cannot open LoadedImage protocol");
    }
    let loaded_image = loaded_image_interface.cast::<LoadedImageProtocol>();

    let mut filesystem_interface: *mut c_void = ptr::null_mut();
    let status = unsafe {
        (boot_services.handle_protocol)(
            (*loaded_image).device_handle,
            &SimpleFileSystemProtocol::GUID,
            &mut filesystem_interface,
        )
    };
    if !status.is_success() || filesystem_interface.is_null() {
        return Err("boot device has no SimpleFileSystem protocol");
    }
    let filesystem = filesystem_interface.cast::<SimpleFileSystemProtocol>();

    let mut root: *mut FileProtocolV1 = ptr::null_mut();
    if !unsafe { ((*filesystem).open_volume)(filesystem, &mut root) }.is_success() || root.is_null()
    {
        return Err("cannot open boot volume");
    }

    let mut file: *mut FileProtocolV1 = ptr::null_mut();
    let open_status = unsafe {
        ((*root).open)(
            root,
            &mut file,
            path.as_ptr(),
            FileMode::READ,
            FileAttribute::empty(),
        )
    };
    if !open_status.is_success() || file.is_null() {
        unsafe {
            let _ = ((*root).close)(root);
        };
        return Err("required boot file cannot be opened");
    }

    let mut info_size = 0usize;
    let query_status =
        unsafe { ((*file).get_info)(file, &FileInfo::ID, &mut info_size, ptr::null_mut()) };
    if query_status != Status::BUFFER_TOO_SMALL || info_size < size_of::<FileInfo>() {
        close_files(root, file);
        return Err("cannot query boot file size");
    }
    let mut info_buffer: *mut u8 = ptr::null_mut();
    let allocation_status = unsafe {
        (boot_services.allocate_pool)(MemoryType::LOADER_DATA, info_size, &mut info_buffer)
    };
    if !allocation_status.is_success() || info_buffer.is_null() {
        close_files(root, file);
        return Err("cannot allocate file information buffer");
    }
    let info_status =
        unsafe { ((*file).get_info)(file, &FileInfo::ID, &mut info_size, info_buffer.cast()) };
    if !info_status.is_success() {
        unsafe {
            let _ = (boot_services.free_pool)(info_buffer);
        };
        close_files(root, file);
        return Err("cannot read boot file information");
    }
    let file_size = unsafe { (*info_buffer.cast::<FileInfo>()).file_size as usize };
    unsafe {
        let _ = (boot_services.free_pool)(info_buffer);
    };
    if file_size == 0 {
        close_files(root, file);
        return Err("boot file is empty");
    }

    let page_count = file_size.div_ceil(PAGE_SIZE);
    let base = unsafe {
        allocate_pages(
            boot_services,
            AllocateType::ANY_PAGES,
            MemoryType::LOADER_DATA,
            page_count,
            0,
        )
    }?;
    unsafe { ptr::write_bytes(base as *mut u8, 0, page_count * PAGE_SIZE) };
    let mut bytes_to_read = file_size;
    let read_status = unsafe { ((*file).read)(file, &mut bytes_to_read, base as *mut c_void) };
    close_files(root, file);
    if !read_status.is_success() || bytes_to_read != file_size {
        unsafe {
            let _ = (boot_services.free_pages)(base, page_count);
        };
        return Err("boot file read was incomplete");
    }
    Ok(PageFile {
        base,
        size: file_size,
        page_count,
    })
}

fn close_files(root: *mut FileProtocolV1, file: *mut FileProtocolV1) {
    // SAFETY: both handles were successfully opened and are no longer used.
    unsafe {
        let _ = ((*file).close)(file);
        let _ = ((*root).close)(root);
    }
}

unsafe fn discover_framebuffer(boot_services: &BootServices) -> FramebufferInfo {
    let mut interface: *mut c_void = ptr::null_mut();
    let status = unsafe {
        (boot_services.locate_protocol)(
            &GraphicsOutputProtocol::GUID,
            ptr::null_mut(),
            &mut interface,
        )
    };
    if !status.is_success() || interface.is_null() {
        fatal("GOP protocol is unavailable");
    }
    let gop = interface.cast::<GraphicsOutputProtocol>();
    let mode_pointer = unsafe { (*gop).mode };
    if mode_pointer.is_null() {
        fatal("GOP mode information is unavailable");
    }

    let max_mode = unsafe { (*mode_pointer).max_mode };
    for mode_number in 0..max_mode {
        let mut info_size = 0usize;
        let mut info: *const GraphicsOutputModeInformation = ptr::null();
        let query_status =
            unsafe { ((*gop).query_mode)(gop, mode_number, &mut info_size, &mut info) };
        if !query_status.is_success() || info.is_null() {
            continue;
        }
        let preferred = unsafe {
            (*info).horizontal_resolution == 1024
                && (*info).vertical_resolution == 768
                && (*info).pixel_format != GraphicsPixelFormat::PIXEL_BLT_ONLY
        };
        unsafe {
            let _ = (boot_services.free_pool)(info.cast_mut().cast());
        };
        if preferred {
            if !unsafe { ((*gop).set_mode)(gop, mode_number) }.is_success() {
                fatal("cannot set preferred GOP mode");
            }
            break;
        }
    }

    let active_mode = unsafe { (*gop).mode };
    if active_mode.is_null() {
        fatal("active GOP mode is invalid");
    }
    let active_info = unsafe { (*active_mode).info };
    if active_info.is_null() {
        fatal("active GOP mode is invalid");
    }
    let pixel_format = match unsafe { (*active_info).pixel_format } {
        GraphicsPixelFormat::PIXEL_RED_GREEN_BLUE_RESERVED_8_BIT_PER_COLOR => BootPixelFormat::Rgb,
        GraphicsPixelFormat::PIXEL_BLUE_GREEN_RED_RESERVED_8_BIT_PER_COLOR => BootPixelFormat::Bgr,
        GraphicsPixelFormat::PIXEL_BIT_MASK => BootPixelFormat::Bitmask,
        _ => BootPixelFormat::Unknown,
    };
    FramebufferInfo {
        base: unsafe { (*active_mode).frame_buffer_base },
        size: unsafe { (*active_mode).frame_buffer_size as u64 },
        width: unsafe { (*active_info).horizontal_resolution },
        height: unsafe { (*active_info).vertical_resolution },
        stride: unsafe { (*active_info).pixels_per_scan_line },
        pixel_format,
    }
}

unsafe fn find_acpi_rsdp(system_table: &SystemTable) -> u64 {
    if system_table.configuration_table.is_null() {
        return 0;
    }
    // SAFETY: firmware provides exactly the reported number of table entries.
    let entries = unsafe {
        core::slice::from_raw_parts(
            system_table.configuration_table,
            system_table.number_of_configuration_table_entries,
        )
    };
    entries
        .iter()
        .find(|entry| entry.vendor_guid == ACPI2_GUID)
        .or_else(|| entries.iter().find(|entry| entry.vendor_guid == ACPI_GUID))
        .map_or(0, |entry| entry.vendor_table as u64)
}

unsafe fn allocate_pages(
    boot_services: &BootServices,
    allocation_type: AllocateType,
    memory_type: MemoryType,
    page_count: usize,
    requested_address: u64,
) -> Result<u64, &'static str> {
    let mut address = requested_address;
    let status = unsafe {
        (boot_services.allocate_pages)(allocation_type, memory_type, page_count, &mut address)
    };
    if !status.is_success() || address == 0 {
        Err("UEFI page allocation failed")
    } else {
        Ok(address)
    }
}

unsafe fn load_elf_kernel(
    boot_services: &BootServices,
    bytes: &[u8],
) -> Result<KernelImageInfo, &'static str> {
    if bytes.len() < 64 || &bytes[0..4] != b"\x7fELF" {
        return Err("kernel ELF magic is missing");
    }
    if bytes[4] != 2 || bytes[5] != 1 || read_u16(bytes, 18)? != 0x3e {
        return Err("kernel is not little-endian x86-64 ELF64");
    }
    if read_u16(bytes, 16)? != 2 {
        return Err("kernel ELF is not ET_EXEC");
    }
    let entry = read_u64(bytes, 24)?;
    let program_offset = read_u64(bytes, 32)? as usize;
    let program_size = read_u16(bytes, 54)? as usize;
    let program_count = read_u16(bytes, 56)? as usize;
    if program_size < 56 {
        return Err("ELF program header is too small");
    }

    let mut image_start = u64::MAX;
    let mut image_end = 0u64;
    let mut load_count = 0usize;
    for index in 0..program_count {
        let header = program_offset
            .checked_add(
                index
                    .checked_mul(program_size)
                    .ok_or("ELF offset overflow")?,
            )
            .ok_or("ELF offset overflow")?;
        check_range(bytes, header, program_size)?;
        if read_u32(bytes, header)? != 1 {
            continue;
        }
        load_count += 1;
        let virtual_address = read_u64(bytes, header + 16)?;
        let physical_address = read_u64(bytes, header + 24)?;
        let memory_size = read_u64(bytes, header + 40)?;
        if virtual_address != physical_address {
            return Err("kernel requires non-identity virtual mapping");
        }
        image_start = image_start.min(physical_address & !((PAGE_SIZE as u64) - 1));
        let segment_end = physical_address
            .checked_add(memory_size)
            .ok_or("ELF segment address overflow")?;
        image_end = image_end.max(segment_end.next_multiple_of(PAGE_SIZE as u64));
    }
    if load_count == 0 || image_start == u64::MAX || image_end <= image_start {
        return Err("ELF contains no loadable segments");
    }
    if entry < image_start || entry >= image_end {
        return Err("ELF entry falls outside the kernel image");
    }

    let page_count = ((image_end - image_start) / PAGE_SIZE as u64) as usize;
    let allocation = unsafe {
        allocate_pages(
            boot_services,
            AllocateType::ADDRESS,
            MemoryType::LOADER_CODE,
            page_count,
            image_start,
        )
    }
    .map_err(|_| "UEFI cannot allocate the kernel's linked physical range")?;
    if allocation != image_start {
        return Err("UEFI returned an unexpected kernel address");
    }
    unsafe { ptr::write_bytes(allocation as *mut u8, 0, page_count * PAGE_SIZE) };

    for index in 0..program_count {
        let header = program_offset + index * program_size;
        if read_u32(bytes, header)? != 1 {
            continue;
        }
        let file_offset = read_u64(bytes, header + 8)? as usize;
        let physical_address = read_u64(bytes, header + 24)?;
        let file_size = read_u64(bytes, header + 32)? as usize;
        let memory_size = read_u64(bytes, header + 40)? as usize;
        if file_size > memory_size {
            return Err("ELF file segment exceeds memory segment");
        }
        check_range(bytes, file_offset, file_size)?;
        let destination_offset = (physical_address - image_start) as usize;
        if destination_offset
            .checked_add(memory_size)
            .filter(|end| *end <= page_count * PAGE_SIZE)
            .is_none()
        {
            return Err("ELF segment exceeds allocated kernel image");
        }
        unsafe {
            ptr::copy_nonoverlapping(
                bytes.as_ptr().add(file_offset),
                (allocation as *mut u8).add(destination_offset),
                file_size,
            )
        };
    }
    Ok(KernelImageInfo {
        physical_start: image_start,
        physical_end: image_end,
        entry,
    })
}

#[derive(Clone, Copy)]
struct RawMemoryMap {
    base: u64,
    size: usize,
    descriptor_size: usize,
    descriptor_version: u32,
}

unsafe fn exit_boot_services(boot_services: &BootServices, image_handle: Handle) -> RawMemoryMap {
    let mut required_size = 0usize;
    let mut map_key = 0usize;
    let mut descriptor_size = 0usize;
    let mut descriptor_version = 0u32;
    let query = unsafe {
        (boot_services.get_memory_map)(
            &mut required_size,
            ptr::null_mut(),
            &mut map_key,
            &mut descriptor_size,
            &mut descriptor_version,
        )
    };
    if query != Status::BUFFER_TOO_SMALL || descriptor_size < size_of::<MemoryDescriptor>() {
        fatal("cannot size UEFI memory map");
    }
    let capacity = required_size
        .checked_add(descriptor_size * 16)
        .unwrap_or_else(|| fatal("UEFI memory map size overflow"));
    let page_count = capacity.div_ceil(PAGE_SIZE);
    let map_base = unsafe {
        allocate_pages(
            boot_services,
            AllocateType::ANY_PAGES,
            MemoryType::LOADER_DATA,
            page_count,
            0,
        )
    }
    .unwrap_or_else(|message| fatal(message));

    for _ in 0..2 {
        let mut map_size = page_count * PAGE_SIZE;
        let map_status = unsafe {
            (boot_services.get_memory_map)(
                &mut map_size,
                map_base as *mut MemoryDescriptor,
                &mut map_key,
                &mut descriptor_size,
                &mut descriptor_version,
            )
        };
        if !map_status.is_success() {
            fatal("cannot retrieve final UEFI memory map");
        }
        let exit_status = unsafe { (boot_services.exit_boot_services)(image_handle, map_key) };
        if exit_status.is_success() {
            return RawMemoryMap {
                base: map_base,
                size: map_size,
                descriptor_size,
                descriptor_version,
            };
        }
    }
    fatal("ExitBootServices failed after retry")
}

fn check_range(bytes: &[u8], start: usize, size: usize) -> Result<(), &'static str> {
    if start
        .checked_add(size)
        .filter(|end| *end <= bytes.len())
        .is_some()
    {
        Ok(())
    } else {
        Err("ELF field points beyond end of file")
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, &'static str> {
    check_range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, &'static str> {
    check_range(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, &'static str> {
    check_range(bytes, offset, 8)?;
    Ok(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

fn fatal(message: &str) -> ! {
    serialln(format_args!("SLOPOS-UEFI: FATAL {message}"));
    loop {
        core::hint::spin_loop();
    }
}

struct Serial;

impl Write for Serial {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                write_byte(b'\r');
            }
            write_byte(byte);
        }
        Ok(())
    }
}

fn serialln(args: fmt::Arguments<'_>) {
    let mut serial = Serial;
    let _ = serial.write_fmt(args);
    let _ = serial.write_str("\n");
}

fn uart_init() {
    // SAFETY: COM1 is fixed on the supported QEMU x86-64 machine.
    unsafe {
        outb(0x3f9, 0x00);
        outb(0x3fb, 0x80);
        outb(0x3f8, 0x03);
        outb(0x3f9, 0x00);
        outb(0x3fb, 0x03);
        outb(0x3fa, 0xc7);
        outb(0x3fc, 0x0b);
    }
}

fn write_byte(byte: u8) {
    // Debugcon is write-only and available in the reproducible QEMU command.
    unsafe { outb(0x402, byte) };
    let mut attempts = 0usize;
    while attempts < 100_000 {
        if unsafe { inb(0x3fd) } & 0x20 != 0 {
            unsafe { outb(0x3f8, byte) };
            return;
        }
        attempts += 1;
        core::hint::spin_loop();
    }
}

unsafe fn outb(port: u16, value: u8) {
    // SAFETY: the caller owns semantic safety of the selected I/O port.
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        )
    };
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: the caller owns semantic safety of the selected I/O port.
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        )
    };
    value
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    serialln(format_args!("SLOPOS-UEFI: PANIC {info}"));
    loop {
        core::hint::spin_loop();
    }
}
