// SPDX-License-Identifier: 0BSD

use core::arch::asm;

struct PortConfig;

impl slopos_pci::ConfigAccess for PortConfig {
    fn read_u32(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        let address = 1u32 << 31
            | u32::from(bus) << 16
            | u32::from(device) << 11
            | u32::from(function) << 8
            | u32::from(offset & 0xfc);
        // SAFETY: PCI configuration mechanism 1 uses a serialized address/data
        // port pair. Boot runs this scan once with interrupts disabled.
        unsafe {
            outl(0x0cf8, address);
            inl(0x0cfc)
        }
    }
}

pub fn discover() -> slopos_pci::Inventory {
    slopos_pci::scan(&mut PortConfig)
}

unsafe fn outl(port: u16, value: u32) {
    // SAFETY: caller selects the PCI configuration address port.
    unsafe {
        asm!(
            "out dx, eax",
            in("dx") port,
            in("eax") value,
            options(nomem, nostack, preserves_flags)
        )
    };
}

unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    // SAFETY: caller selects the PCI configuration data port.
    unsafe {
        asm!(
            "in eax, dx",
            in("dx") port,
            out("eax") value,
            options(nomem, nostack, preserves_flags)
        )
    };
    value
}
