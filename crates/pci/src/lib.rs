// SPDX-License-Identifier: 0BSD

#![no_std]

pub const MAX_DEVICES: usize = 64;
const STATUS_CAPABILITIES_LIST: u16 = 1 << 4;
const CAPABILITY_VENDOR_SPECIFIC: u8 = 0x09;

pub trait ConfigAccess {
    fn read_u32(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> u32;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bdf {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Device {
    pub address: Bdf,
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision: u8,
    pub programming_interface: u8,
    pub subclass: u8,
    pub class: u8,
    pub header_type: u8,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub bars: [u32; 6],
    pub virtio_capability_mask: u32,
    virtio_capabilities: [VirtioCapability; 6],
}

impl Device {
    const EMPTY: Self = Self {
        address: Bdf {
            bus: 0,
            device: 0,
            function: 0,
        },
        vendor_id: 0,
        device_id: 0,
        command: 0,
        status: 0,
        revision: 0,
        programming_interface: 0,
        subclass: 0,
        class: 0,
        header_type: 0,
        subsystem_vendor_id: 0,
        subsystem_id: 0,
        interrupt_line: 0,
        interrupt_pin: 0,
        bars: [0; 6],
        virtio_capability_mask: 0,
        virtio_capabilities: [VirtioCapability::EMPTY; 6],
    };

    pub const fn is_multifunction(self) -> bool {
        self.header_type & 0x80 != 0
    }

    pub const fn virtio_device_type(self) -> Option<u16> {
        if self.vendor_id != 0x1af4 {
            return None;
        }
        match self.device_id {
            0x1000..=0x103f => Some(self.device_id - 0x0fff),
            0x1040..=0x107f => Some(self.device_id - 0x1040),
            _ => None,
        }
    }

    pub const fn is_virtio_block(self) -> bool {
        matches!(self.virtio_device_type(), Some(2))
    }

    pub fn bar_base(&self, index: usize) -> Option<u64> {
        let low = *self.bars.get(index)?;
        if low == 0 || low == u32::MAX || low & 1 != 0 {
            return None;
        }
        let memory_type = (low >> 1) & 0b11;
        let low_address = u64::from(low & 0xffff_fff0);
        let address = if memory_type == 0b10 {
            let high = u64::from(*self.bars.get(index + 1)?);
            low_address | (high << 32)
        } else if memory_type <= 0b01 {
            low_address
        } else {
            return None;
        };
        (address != 0).then_some(address)
    }

    pub fn virtio_region(&self, configuration_type: u8) -> Option<VirtioRegion> {
        let capability = *self
            .virtio_capabilities
            .get(usize::from(configuration_type))?;
        if !capability.valid {
            return None;
        }
        let bar_base = self.bar_base(usize::from(capability.bar))?;
        Some(VirtioRegion {
            base: bar_base.checked_add(u64::from(capability.offset))?,
            length: capability.length,
            notify_multiplier: capability.notify_multiplier,
            bar: capability.bar,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VirtioCapability {
    valid: bool,
    bar: u8,
    offset: u32,
    length: u32,
    notify_multiplier: u32,
}

impl VirtioCapability {
    const EMPTY: Self = Self {
        valid: false,
        bar: 0,
        offset: 0,
        length: 0,
        notify_multiplier: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioRegion {
    pub base: u64,
    pub length: u32,
    pub notify_multiplier: u32,
    pub bar: u8,
}

pub struct Inventory {
    devices: [Device; MAX_DEVICES],
    count: usize,
    pub overflowed: bool,
}

impl Inventory {
    const fn empty() -> Self {
        Self {
            devices: [Device::EMPTY; MAX_DEVICES],
            count: 0,
            overflowed: false,
        }
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices[..self.count]
    }

    pub fn virtio_devices(&self) -> impl Iterator<Item = &Device> {
        self.devices()
            .iter()
            .filter(|device| device.virtio_device_type().is_some())
    }

    pub fn find_virtio_block(&self) -> Option<&Device> {
        self.devices()
            .iter()
            .find(|device| device.is_virtio_block())
    }

    fn push(&mut self, device: Device) {
        if self.count == MAX_DEVICES {
            self.overflowed = true;
            return;
        }
        self.devices[self.count] = device;
        self.count += 1;
    }
}

pub fn scan(config: &mut impl ConfigAccess) -> Inventory {
    let mut inventory = Inventory::empty();
    for bus in 0..=u8::MAX {
        for device in 0..32 {
            let Some(first) = read_device(config, bus, device, 0) else {
                continue;
            };
            let multifunction = first.is_multifunction();
            inventory.push(first);
            if multifunction {
                for function in 1..8 {
                    if let Some(device) = read_device(config, bus, device, function) {
                        inventory.push(device);
                    }
                }
            }
        }
    }
    inventory
}

fn read_device(
    config: &mut impl ConfigAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> Option<Device> {
    let identity = config.read_u32(bus, device, function, 0x00);
    let vendor_id = identity as u16;
    if vendor_id == 0xffff {
        return None;
    }
    let command_status = config.read_u32(bus, device, function, 0x04);
    let class_revision = config.read_u32(bus, device, function, 0x08);
    let header = config.read_u32(bus, device, function, 0x0c);
    let header_type = (header >> 16) as u8;
    let mut bars = [0u32; 6];
    let subsystem = if header_type & 0x7f == 0 {
        for (index, bar) in bars.iter_mut().enumerate() {
            *bar = config.read_u32(bus, device, function, 0x10 + index as u8 * 4);
        }
        config.read_u32(bus, device, function, 0x2c)
    } else {
        0
    };
    let interrupt = config.read_u32(bus, device, function, 0x3c);
    let status = (command_status >> 16) as u16;
    let (virtio_capability_mask, virtio_capabilities) = if status & STATUS_CAPABILITIES_LIST != 0 {
        scan_virtio_capabilities(config, bus, device, function)
    } else {
        (0, [VirtioCapability::EMPTY; 6])
    };

    Some(Device {
        address: Bdf {
            bus,
            device,
            function,
        },
        vendor_id,
        device_id: (identity >> 16) as u16,
        command: command_status as u16,
        status,
        revision: class_revision as u8,
        programming_interface: (class_revision >> 8) as u8,
        subclass: (class_revision >> 16) as u8,
        class: (class_revision >> 24) as u8,
        header_type,
        subsystem_vendor_id: subsystem as u16,
        subsystem_id: (subsystem >> 16) as u16,
        interrupt_line: interrupt as u8,
        interrupt_pin: (interrupt >> 8) as u8,
        bars,
        virtio_capability_mask,
        virtio_capabilities,
    })
}

fn scan_virtio_capabilities(
    config: &mut impl ConfigAccess,
    bus: u8,
    device: u8,
    function: u8,
) -> (u32, [VirtioCapability; 6]) {
    let mut pointer = config.read_u32(bus, device, function, 0x34) as u8 & 0xfc;
    let mut visited = 0u64;
    let mut mask = 0u32;
    let mut capabilities = [VirtioCapability::EMPTY; 6];
    for _ in 0..48 {
        if pointer < 0x40 {
            break;
        }
        let slot = pointer / 4;
        let bit = 1u64 << slot;
        if visited & bit != 0 {
            break;
        }
        visited |= bit;
        let capability = config.read_u32(bus, device, function, pointer);
        let identifier = capability as u8;
        let next = (capability >> 8) as u8 & 0xfc;
        let length = (capability >> 16) as u8;
        let configuration_type = (capability >> 24) as u8;
        if identifier == CAPABILITY_VENDOR_SPECIFIC
            && (1..=5).contains(&configuration_type)
            && length >= 16
            && u16::from(pointer) + 15 <= u16::from(u8::MAX)
        {
            let bar = config.read_u32(bus, device, function, pointer + 4) as u8;
            let offset = config.read_u32(bus, device, function, pointer + 8);
            let region_length = config.read_u32(bus, device, function, pointer + 12);
            if bar < 6 && region_length != 0 {
                let notify_multiplier = if configuration_type == 2
                    && length >= 20
                    && u16::from(pointer) + 19 <= u16::from(u8::MAX)
                {
                    config.read_u32(bus, device, function, pointer + 16)
                } else {
                    0
                };
                mask |= 1 << configuration_type;
                capabilities[usize::from(configuration_type)] = VirtioCapability {
                    valid: true,
                    bar,
                    offset,
                    length: region_length,
                    notify_multiplier,
                };
            }
        }
        pointer = next;
    }
    (mask, capabilities)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    struct FakeConfig {
        registers: Vec<(Bdf, u8, u32)>,
    }

    impl FakeConfig {
        fn new(registers: &[(Bdf, u8, u32)]) -> Self {
            Self {
                registers: registers.into(),
            }
        }
    }

    impl ConfigAccess for FakeConfig {
        fn read_u32(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> u32 {
            let address = Bdf {
                bus,
                device,
                function,
            };
            self.registers
                .iter()
                .find(|(candidate, candidate_offset, _)| {
                    *candidate == address && *candidate_offset == offset
                })
                .map_or(u32::MAX, |(_, _, value)| *value)
        }
    }

    const ROOT: Bdf = Bdf {
        bus: 0,
        device: 0,
        function: 0,
    };
    const BLOCK: Bdf = Bdf {
        bus: 0,
        device: 1,
        function: 0,
    };
    const NET_FUNCTION: Bdf = Bdf {
        bus: 0,
        device: 1,
        function: 1,
    };

    #[test]
    fn scans_multifunction_and_virtio_capabilities() {
        let mut config = FakeConfig::new(&[
            (ROOT, 0x00, 0x29c0_8086),
            (ROOT, 0x04, 0),
            (ROOT, 0x08, 0x0600_0002),
            (ROOT, 0x0c, 0),
            (ROOT, 0x2c, 0),
            (ROOT, 0x3c, 0),
            (BLOCK, 0x00, 0x1042_1af4),
            (BLOCK, 0x04, u32::from(STATUS_CAPABILITIES_LIST) << 16),
            (BLOCK, 0x08, 0x0100_0001),
            (BLOCK, 0x0c, 0x0080_0000),
            (BLOCK, 0x10, 1),
            (BLOCK, 0x14, 0),
            (BLOCK, 0x18, 0),
            (BLOCK, 0x1c, 0),
            (BLOCK, 0x20, 0x8000_0004),
            (BLOCK, 0x24, 0),
            (BLOCK, 0x2c, 0x0002_1af4),
            (BLOCK, 0x34, 0x40),
            (BLOCK, 0x40, 0x0110_5009),
            (BLOCK, 0x44, 4),
            (BLOCK, 0x48, 0),
            (BLOCK, 0x4c, 56),
            (BLOCK, 0x50, 0x0410_0009),
            (BLOCK, 0x54, 4),
            (BLOCK, 0x58, 0x2000),
            (BLOCK, 0x5c, 8),
            (BLOCK, 0x3c, 0x0001_000b),
            (NET_FUNCTION, 0x00, 0x1041_1af4),
            (NET_FUNCTION, 0x04, 0),
            (NET_FUNCTION, 0x08, 0x0200_0001),
            (NET_FUNCTION, 0x0c, 0),
            (NET_FUNCTION, 0x2c, 0),
            (NET_FUNCTION, 0x3c, 0),
        ]);
        let inventory = scan(&mut config);
        assert_eq!(inventory.len(), 3);
        let block = inventory.find_virtio_block().unwrap();
        assert_eq!(block.address, BLOCK);
        assert_eq!(block.virtio_device_type(), Some(2));
        assert_eq!(block.virtio_capability_mask, (1 << 1) | (1 << 4));
        assert_eq!(block.bar_base(4), Some(0x8000_0000));
        assert_eq!(
            block.virtio_region(1),
            Some(VirtioRegion {
                base: 0x8000_0000,
                length: 56,
                notify_multiplier: 0,
                bar: 4
            })
        );
        assert_eq!(
            block.virtio_region(4),
            Some(VirtioRegion {
                base: 0x8000_2000,
                length: 8,
                notify_multiplier: 0,
                bar: 4
            })
        );
        assert_eq!(inventory.virtio_devices().count(), 2);
    }

    #[test]
    fn classifies_transitional_virtio_ids() {
        let device = Device {
            vendor_id: 0x1af4,
            device_id: 0x1001,
            ..Device::EMPTY
        };
        assert!(device.is_virtio_block());
        assert_eq!(device.virtio_device_type(), Some(2));
    }

    #[test]
    fn breaks_cyclic_capability_lists() {
        let mut config = FakeConfig::new(&[
            (BLOCK, 0x00, 0x1042_1af4),
            (BLOCK, 0x04, u32::from(STATUS_CAPABILITIES_LIST) << 16),
            (BLOCK, 0x08, 0),
            (BLOCK, 0x0c, 0),
            (BLOCK, 0x10, 0),
            (BLOCK, 0x14, 0),
            (BLOCK, 0x18, 0),
            (BLOCK, 0x1c, 0),
            (BLOCK, 0x20, 0x8000_0000),
            (BLOCK, 0x24, 0),
            (BLOCK, 0x2c, 0),
            (BLOCK, 0x34, 0x40),
            (BLOCK, 0x40, 0x0110_4009),
            (BLOCK, 0x44, 4),
            (BLOCK, 0x48, 0),
            (BLOCK, 0x4c, 56),
            (BLOCK, 0x3c, 0),
        ]);
        let inventory = scan(&mut config);
        assert_eq!(
            inventory
                .find_virtio_block()
                .unwrap()
                .virtio_capability_mask,
            1 << 1
        );
    }
}
