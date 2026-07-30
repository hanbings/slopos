// SPDX-License-Identifier: 0BSD

use core::arch::asm;

struct PortConfig;

impl slopos_pci::ConfigAccess for PortConfig {
    fn read_u32(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
        let address = configuration_address(
            slopos_pci::Bdf {
                bus,
                device,
                function,
            },
            offset,
        );
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

pub fn enable_memory_bus_master(device: slopos_pci::Device) {
    let command = device.command | (1 << 1) | (1 << 2) | (1 << 10);
    let address = configuration_address(device.address, 0x04);
    // SAFETY: boot still runs with interrupts disabled, so the mechanism-1
    // address/data pair is exclusive. A 16-bit write changes only command,
    // leaving write-one-to-clear status bits untouched.
    unsafe {
        outl(0x0cf8, address);
        outw(0x0cfc, command);
        outl(0x0cf8, address);
        if inl(0x0cfc) as u16 & ((1 << 1) | (1 << 2)) != (1 << 1) | (1 << 2) {
            crate::fatal("PCI memory or bus-master enable did not persist");
        }
    }
}

fn configuration_address(address: slopos_pci::Bdf, offset: u8) -> u32 {
    1u32 << 31
        | u32::from(address.bus) << 16
        | u32::from(address.device) << 11
        | u32::from(address.function) << 8
        | u32::from(offset & 0xfc)
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

unsafe fn outw(port: u16, value: u16) {
    // SAFETY: caller selects the PCI configuration data port.
    unsafe {
        asm!(
            "out dx, ax",
            in("dx") port,
            in("ax") value,
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
