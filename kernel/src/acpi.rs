// SPDX-License-Identifier: 0BSD

use core::slice;
use slopos_acpi::{MadtInfo, RootKind, RootTable, Rsdp, SDT_HEADER_SIZE, parse_sdt, sdt_length};

pub struct PlatformInfo {
    pub root_kind: RootKind,
    pub madt: MadtInfo,
}

pub fn discover(rsdp_address: u64) -> Option<PlatformInfo> {
    if rsdp_address == 0 {
        return None;
    }

    // SAFETY: the loader obtained this physical pointer from the UEFI ACPI
    // configuration table and firmware keeps ACPI reclaim data valid here.
    let first = unsafe { slice::from_raw_parts(rsdp_address as *const u8, 20) };
    if first.get(15).copied()? >= 2 {
        // SAFETY: ACPI revision 2+ RSDP structures contain at least 36 bytes.
        let rsdp_bytes = unsafe { slice::from_raw_parts(rsdp_address as *const u8, 36) };
        let declared_length = u32::from_le_bytes(rsdp_bytes.get(20..24)?.try_into().ok()?) as usize;
        if !(36..=4096).contains(&declared_length) {
            return None;
        }
        // SAFETY: the validated RSDP length is bounded and supplied by firmware.
        let rsdp_bytes =
            unsafe { slice::from_raw_parts(rsdp_address as *const u8, declared_length) };
        discover_from_rsdp(Rsdp::parse(rsdp_bytes).ok()?)
    } else {
        discover_from_rsdp(Rsdp::parse(first).ok()?)
    }
}

fn discover_from_rsdp(rsdp: Rsdp) -> Option<PlatformInfo> {
    let root_bytes = physical_table(rsdp.root_address())?;
    let root = RootTable::parse(root_bytes, rsdp.root_kind()).ok()?;

    for index in 0..root.len() {
        let address = root.entry(index)?;
        let bytes = physical_table(address)?;
        let header = parse_sdt(bytes).ok()?;
        if header.signature == *b"APIC" {
            return Some(PlatformInfo {
                root_kind: rsdp.root_kind(),
                madt: MadtInfo::parse(bytes).ok()?,
            });
        }
    }
    None
}

fn physical_table(address: u64) -> Option<&'static [u8]> {
    if address == 0 {
        return None;
    }
    // SAFETY: ACPI root entries point to at least a generic 36-byte header.
    let header = unsafe { slice::from_raw_parts(address as *const u8, SDT_HEADER_SIZE) };
    let length = sdt_length(header).ok()?;
    // SAFETY: sdt_length bounds the firmware-declared table to one MiB.
    Some(unsafe { slice::from_raw_parts(address as *const u8, length) })
}
