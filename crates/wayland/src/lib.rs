// SPDX-License-Identifier: 0BSD

#![no_std]

use core::str;

pub const HEADER_SIZE: usize = 8;
pub const DISPLAY_OBJECT_ID: u32 = 1;
pub const MAX_MESSAGE_SIZE: usize = u16::MAX as usize & !3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireError {
    TruncatedHeader,
    NullObject,
    InvalidSize,
    UnalignedSize,
    TruncatedMessage,
    BufferTooSmall,
    MessageTooLarge,
    TruncatedArgument,
    TrailingArguments,
    InvalidString,
    InvalidArray,
    MissingFileDescriptor,
    UnexpectedFileDescriptor,
    UnknownObject,
    WrongInterface,
    DuplicateObject,
    ObjectCapacity,
    ObjectNotRetired,
    UnknownOpcode,
    UnsupportedVersion,
    UnknownGlobal,
    InterfaceMismatch,
    InvalidVersion,
    InvalidArgument,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header {
    pub object_id: u32,
    pub opcode: u16,
    pub size: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame<'a> {
    pub header: Header,
    pub payload: &'a [u8],
}

impl<'a> Frame<'a> {
    pub fn decode(bytes: &'a [u8]) -> Result<(Self, &'a [u8]), WireError> {
        if bytes.len() < HEADER_SIZE {
            return Err(WireError::TruncatedHeader);
        }
        let object_id = u32::from_le_bytes(
            bytes[0..4]
                .try_into()
                .map_err(|_| WireError::TruncatedHeader)?,
        );
        if object_id == 0 {
            return Err(WireError::NullObject);
        }
        let word = u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| WireError::TruncatedHeader)?,
        );
        let opcode = word as u16;
        let size = (word >> 16) as u16;
        let size_usize = usize::from(size);
        if size_usize < HEADER_SIZE {
            return Err(WireError::InvalidSize);
        }
        if size_usize % 4 != 0 {
            return Err(WireError::UnalignedSize);
        }
        if bytes.len() < size_usize {
            return Err(WireError::TruncatedMessage);
        }
        Ok((
            Self {
                header: Header {
                    object_id,
                    opcode,
                    size,
                },
                payload: &bytes[HEADER_SIZE..size_usize],
            },
            &bytes[size_usize..],
        ))
    }
}

pub struct ArgumentReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> ArgumentReader<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub fn uint(&mut self) -> Result<u32, WireError> {
        let bytes = self.take(4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().map_err(|_| WireError::TruncatedArgument)?,
        ))
    }

    pub fn int(&mut self) -> Result<i32, WireError> {
        Ok(self.uint()? as i32)
    }

    pub fn fixed(&mut self) -> Result<i32, WireError> {
        self.int()
    }

    pub fn object(&mut self) -> Result<u32, WireError> {
        let id = self.uint()?;
        if id == 0 {
            return Err(WireError::NullObject);
        }
        Ok(id)
    }

    pub fn nullable_object(&mut self) -> Result<Option<u32>, WireError> {
        match self.uint()? {
            0 => Ok(None),
            id => Ok(Some(id)),
        }
    }

    pub fn string(&mut self) -> Result<&'a str, WireError> {
        self.nullable_string()?.ok_or(WireError::InvalidString)
    }

    pub fn nullable_string(&mut self) -> Result<Option<&'a str>, WireError> {
        let length = usize::try_from(self.uint()?).map_err(|_| WireError::InvalidString)?;
        if length == 0 {
            return Ok(None);
        }
        let padded = align4(length).ok_or(WireError::InvalidString)?;
        let bytes = self.take(padded)?;
        let value = bytes.get(..length).ok_or(WireError::InvalidString)?;
        if value.last() != Some(&0) || value[..length - 1].contains(&0) {
            return Err(WireError::InvalidString);
        }
        str::from_utf8(&value[..length - 1])
            .map(Some)
            .map_err(|_| WireError::InvalidString)
    }

    pub fn array(&mut self) -> Result<&'a [u8], WireError> {
        let length = usize::try_from(self.uint()?).map_err(|_| WireError::InvalidArray)?;
        let padded = align4(length).ok_or(WireError::InvalidArray)?;
        let bytes = self.take(padded)?;
        bytes.get(..length).ok_or(WireError::InvalidArray)
    }

    pub fn finish(self) -> Result<(), WireError> {
        if self.cursor == self.bytes.len() {
            Ok(())
        } else {
            Err(WireError::TrailingArguments)
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], WireError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(WireError::TruncatedArgument)?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or(WireError::TruncatedArgument)?;
        self.cursor = end;
        Ok(bytes)
    }
}

pub struct MessageBuilder<'a> {
    bytes: &'a mut [u8],
    cursor: usize,
    object_id: u32,
    opcode: u16,
}

impl<'a> MessageBuilder<'a> {
    pub fn new(bytes: &'a mut [u8], object_id: u32, opcode: u16) -> Result<Self, WireError> {
        if object_id == 0 {
            return Err(WireError::NullObject);
        }
        if bytes.len() < HEADER_SIZE {
            return Err(WireError::BufferTooSmall);
        }
        Ok(Self {
            bytes,
            cursor: HEADER_SIZE,
            object_id,
            opcode,
        })
    }

    pub fn uint(&mut self, value: u32) -> Result<(), WireError> {
        self.write(&value.to_le_bytes())
    }

    pub fn int(&mut self, value: i32) -> Result<(), WireError> {
        self.uint(value as u32)
    }

    pub fn fixed(&mut self, value: i32) -> Result<(), WireError> {
        self.int(value)
    }

    pub fn object(&mut self, value: u32) -> Result<(), WireError> {
        if value == 0 {
            return Err(WireError::NullObject);
        }
        self.uint(value)
    }

    pub fn nullable_object(&mut self, value: Option<u32>) -> Result<(), WireError> {
        self.uint(value.unwrap_or(0))
    }

    pub fn string(&mut self, value: &str) -> Result<(), WireError> {
        let length = value
            .len()
            .checked_add(1)
            .ok_or(WireError::MessageTooLarge)?;
        let length_u32 = u32::try_from(length).map_err(|_| WireError::MessageTooLarge)?;
        self.uint(length_u32)?;
        self.write(value.as_bytes())?;
        self.write(&[0])?;
        self.pad(length)
    }

    pub fn nullable_string(&mut self, value: Option<&str>) -> Result<(), WireError> {
        match value {
            Some(value) => self.string(value),
            None => self.uint(0),
        }
    }

    pub fn array(&mut self, value: &[u8]) -> Result<(), WireError> {
        let length = u32::try_from(value.len()).map_err(|_| WireError::MessageTooLarge)?;
        self.uint(length)?;
        self.write(value)?;
        self.pad(value.len())
    }

    pub fn finish(self) -> Result<&'a [u8], WireError> {
        if self.cursor > MAX_MESSAGE_SIZE {
            return Err(WireError::MessageTooLarge);
        }
        let size = u16::try_from(self.cursor).map_err(|_| WireError::MessageTooLarge)?;
        self.bytes[0..4].copy_from_slice(&self.object_id.to_le_bytes());
        let word = (u32::from(size) << 16) | u32::from(self.opcode);
        self.bytes[4..8].copy_from_slice(&word.to_le_bytes());
        Ok(&self.bytes[..self.cursor])
    }

    fn write(&mut self, value: &[u8]) -> Result<(), WireError> {
        let end = self
            .cursor
            .checked_add(value.len())
            .ok_or(WireError::MessageTooLarge)?;
        let destination = self
            .bytes
            .get_mut(self.cursor..end)
            .ok_or(WireError::BufferTooSmall)?;
        destination.copy_from_slice(value);
        self.cursor = end;
        Ok(())
    }

    fn pad(&mut self, unpadded_length: usize) -> Result<(), WireError> {
        let padded = align4(unpadded_length).ok_or(WireError::MessageTooLarge)?;
        for _ in unpadded_length..padded {
            self.write(&[0])?;
        }
        Ok(())
    }
}

const fn align4(value: usize) -> Option<usize> {
    match value.checked_add(3) {
        Some(value) => Some(value & !3),
        None => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Interface {
    Display,
    Registry,
    Callback,
    Compositor,
    Surface,
    Region,
    Shm,
    ShmPool,
    Buffer,
    Seat,
    Pointer,
    Keyboard,
    Output,
    XdgWmBase,
    XdgSurface,
    XdgToplevel,
}

impl Interface {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Display => "wl_display",
            Self::Registry => "wl_registry",
            Self::Callback => "wl_callback",
            Self::Compositor => "wl_compositor",
            Self::Surface => "wl_surface",
            Self::Region => "wl_region",
            Self::Shm => "wl_shm",
            Self::ShmPool => "wl_shm_pool",
            Self::Buffer => "wl_buffer",
            Self::Seat => "wl_seat",
            Self::Pointer => "wl_pointer",
            Self::Keyboard => "wl_keyboard",
            Self::Output => "wl_output",
            Self::XdgWmBase => "xdg_wm_base",
            Self::XdgSurface => "xdg_surface",
            Self::XdgToplevel => "xdg_toplevel",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "wl_display" => Self::Display,
            "wl_registry" => Self::Registry,
            "wl_callback" => Self::Callback,
            "wl_compositor" => Self::Compositor,
            "wl_surface" => Self::Surface,
            "wl_region" => Self::Region,
            "wl_shm" => Self::Shm,
            "wl_shm_pool" => Self::ShmPool,
            "wl_buffer" => Self::Buffer,
            "wl_seat" => Self::Seat,
            "wl_pointer" => Self::Pointer,
            "wl_keyboard" => Self::Keyboard,
            "wl_output" => Self::Output,
            "xdg_wm_base" => Self::XdgWmBase,
            "xdg_surface" => Self::XdgSurface,
            "xdg_toplevel" => Self::XdgToplevel,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Global {
    pub name: u32,
    pub interface: Interface,
    pub version: u32,
}

pub const CORE_GLOBALS: [Global; 5] = [
    Global {
        name: 1,
        interface: Interface::Compositor,
        version: 6,
    },
    Global {
        name: 2,
        interface: Interface::Shm,
        version: 1,
    },
    Global {
        name: 3,
        interface: Interface::Seat,
        version: 9,
    },
    Global {
        name: 4,
        interface: Interface::Output,
        version: 4,
    },
    Global {
        name: 5,
        interface: Interface::XdgWmBase,
        version: 6,
    },
];

pub fn encode_registry_global(
    bytes: &mut [u8],
    registry: u32,
    global: Global,
) -> Result<&[u8], WireError> {
    let mut message = MessageBuilder::new(bytes, registry, 0)?;
    message.uint(global.name)?;
    message.string(global.interface.name())?;
    message.uint(global.version)?;
    message.finish()
}

pub fn encode_registry_global_remove(
    bytes: &mut [u8],
    registry: u32,
    global_name: u32,
) -> Result<&[u8], WireError> {
    let mut message = MessageBuilder::new(bytes, registry, 1)?;
    message.uint(global_name)?;
    message.finish()
}

pub fn encode_display_error<'a>(
    bytes: &'a mut [u8],
    object: u32,
    code: u32,
    description: &str,
) -> Result<&'a [u8], WireError> {
    let mut message = MessageBuilder::new(bytes, DISPLAY_OBJECT_ID, 0)?;
    message.object(object)?;
    message.uint(code)?;
    message.string(description)?;
    message.finish()
}

pub fn encode_display_delete_id(bytes: &mut [u8], object: u32) -> Result<&[u8], WireError> {
    let mut message = MessageBuilder::new(bytes, DISPLAY_OBJECT_ID, 1)?;
    message.object(object)?;
    message.finish()
}

pub fn encode_callback_done(
    bytes: &mut [u8],
    callback: u32,
    callback_data: u32,
) -> Result<&[u8], WireError> {
    let mut message = MessageBuilder::new(bytes, callback, 0)?;
    message.uint(callback_data)?;
    message.finish()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObjectState {
    Empty,
    Active,
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Object {
    pub id: u32,
    pub interface: Interface,
    pub version: u32,
    state: ObjectState,
}

impl Object {
    const EMPTY: Self = Self {
        id: 0,
        interface: Interface::Display,
        version: 0,
        state: ObjectState::Empty,
    };

    pub const fn is_active(self) -> bool {
        matches!(self.state, ObjectState::Active)
    }

    pub const fn is_retired(self) -> bool {
        matches!(self.state, ObjectState::Retired)
    }
}

pub struct ObjectMap<const CAPACITY: usize> {
    slots: [Object; CAPACITY],
}

impl<const CAPACITY: usize> ObjectMap<CAPACITY> {
    pub fn new() -> Result<Self, WireError> {
        if CAPACITY == 0 {
            return Err(WireError::ObjectCapacity);
        }
        let mut slots = [Object::EMPTY; CAPACITY];
        slots[0] = Object {
            id: DISPLAY_OBJECT_ID,
            interface: Interface::Display,
            version: 1,
            state: ObjectState::Active,
        };
        Ok(Self { slots })
    }

    pub fn get(&self, id: u32) -> Result<Object, WireError> {
        self.slots
            .iter()
            .copied()
            .find(|slot| slot.id == id && slot.is_active())
            .ok_or(WireError::UnknownObject)
    }

    pub fn insert(&mut self, id: u32, interface: Interface, version: u32) -> Result<(), WireError> {
        if id == 0 {
            return Err(WireError::NullObject);
        }
        if version == 0 {
            return Err(WireError::InvalidVersion);
        }
        if self.slots.iter().any(|slot| slot.id == id) {
            return Err(WireError::DuplicateObject);
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| matches!(slot.state, ObjectState::Empty))
            .ok_or(WireError::ObjectCapacity)?;
        *slot = Object {
            id,
            interface,
            version,
            state: ObjectState::Active,
        };
        Ok(())
    }

    pub fn retire(&mut self, id: u32) -> Result<(), WireError> {
        if id == DISPLAY_OBJECT_ID {
            return Err(WireError::InvalidArgument);
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.id == id && slot.is_active())
            .ok_or(WireError::UnknownObject)?;
        slot.state = ObjectState::Retired;
        Ok(())
    }

    pub fn delete_id_sent(&mut self, id: u32) -> Result<(), WireError> {
        let slot = self
            .slots
            .iter_mut()
            .find(|slot| slot.id == id && slot.is_retired())
            .ok_or(WireError::ObjectNotRetired)?;
        *slot = Object::EMPTY;
        Ok(())
    }

    pub fn active_count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_active()).count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Request<'a> {
    Sync {
        callback: u32,
    },
    GetRegistry {
        registry: u32,
    },
    Bind {
        registry: u32,
        global: u32,
        interface: Interface,
        version: u32,
        new_id: u32,
    },
    CreateSurface {
        compositor: u32,
        surface: u32,
    },
    CreateRegion {
        compositor: u32,
        region: u32,
    },
    Destroy {
        object: u32,
        interface: Interface,
    },
    RegionAdd {
        region: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    RegionSubtract {
        region: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    SurfaceAttach {
        surface: u32,
        buffer: Option<u32>,
        x: i32,
        y: i32,
    },
    SurfaceDamage {
        surface: u32,
        buffer_coordinates: bool,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    SurfaceFrame {
        surface: u32,
        callback: u32,
    },
    SurfaceSetRegion {
        surface: u32,
        input: bool,
        region: Option<u32>,
    },
    SurfaceCommit {
        surface: u32,
    },
    SurfaceSetBufferTransform {
        surface: u32,
        transform: i32,
    },
    SurfaceSetBufferScale {
        surface: u32,
        scale: i32,
    },
    SurfaceOffset {
        surface: u32,
        x: i32,
        y: i32,
    },
    ShmCreatePool {
        shm: u32,
        pool: u32,
        fd: i32,
        size: i32,
    },
    ShmPoolCreateBuffer {
        pool: u32,
        buffer: u32,
        offset: i32,
        width: i32,
        height: i32,
        stride: i32,
        format: u32,
    },
    ShmPoolResize {
        pool: u32,
        size: i32,
    },
    SeatGetPointer {
        seat: u32,
        pointer: u32,
    },
    SeatGetKeyboard {
        seat: u32,
        keyboard: u32,
    },
    PointerSetCursor {
        pointer: u32,
        serial: u32,
        surface: Option<u32>,
        hotspot_x: i32,
        hotspot_y: i32,
    },
    XdgGetSurface {
        wm_base: u32,
        xdg_surface: u32,
        surface: u32,
    },
    XdgPong {
        wm_base: u32,
        serial: u32,
    },
    XdgGetToplevel {
        xdg_surface: u32,
        toplevel: u32,
    },
    XdgAckConfigure {
        xdg_surface: u32,
        serial: u32,
    },
    XdgSetWindowGeometry {
        xdg_surface: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    ToplevelSetTitle {
        toplevel: u32,
        title: &'a str,
    },
    ToplevelSetAppId {
        toplevel: u32,
        app_id: &'a str,
    },
    ToplevelMove {
        toplevel: u32,
        seat: u32,
        serial: u32,
    },
    ToplevelResize {
        toplevel: u32,
        seat: u32,
        serial: u32,
        edges: u32,
    },
    ToplevelSetMaximized {
        toplevel: u32,
        maximized: bool,
    },
    ToplevelSetFullscreen {
        toplevel: u32,
        output: Option<u32>,
    },
    ToplevelSetMinimized {
        toplevel: u32,
    },
}

pub struct Connection<const OBJECTS: usize> {
    objects: ObjectMap<OBJECTS>,
}

impl<const OBJECTS: usize> Connection<OBJECTS> {
    pub fn new() -> Result<Self, WireError> {
        Ok(Self {
            objects: ObjectMap::new()?,
        })
    }

    pub const fn objects(&self) -> &ObjectMap<OBJECTS> {
        &self.objects
    }

    pub fn delete_id_sent(&mut self, id: u32) -> Result<(), WireError> {
        self.objects.delete_id_sent(id)
    }

    pub fn dispatch<'a>(
        &mut self,
        bytes: &'a [u8],
        file_descriptors: &[i32],
    ) -> Result<(Request<'a>, &'a [u8]), WireError> {
        let (frame, remaining) = Frame::decode(bytes)?;
        let object = self.objects.get(frame.header.object_id)?;
        let mut arguments = ArgumentReader::new(frame.payload);
        let request = match object.interface {
            Interface::Display => {
                self.dispatch_display(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Registry => {
                self.dispatch_registry(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Compositor => {
                self.dispatch_compositor(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Surface => {
                self.dispatch_surface(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Region => {
                self.dispatch_region(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Shm => self.dispatch_shm(
                object,
                frame.header.opcode,
                &mut arguments,
                file_descriptors,
            )?,
            Interface::ShmPool => {
                self.dispatch_shm_pool(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Buffer => {
                self.dispatch_destroy_only(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Seat => self.dispatch_seat(object, frame.header.opcode, &mut arguments)?,
            Interface::Pointer => {
                self.dispatch_pointer(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Keyboard | Interface::Output => {
                self.dispatch_release(object, frame.header.opcode, &mut arguments)?
            }
            Interface::XdgWmBase => {
                self.dispatch_xdg_wm_base(object, frame.header.opcode, &mut arguments)?
            }
            Interface::XdgSurface => {
                self.dispatch_xdg_surface(object, frame.header.opcode, &mut arguments)?
            }
            Interface::XdgToplevel => {
                self.dispatch_xdg_toplevel(object, frame.header.opcode, &mut arguments)?
            }
            Interface::Callback => return Err(WireError::UnknownOpcode),
        };
        if !matches!(object.interface, Interface::Shm) && !file_descriptors.is_empty() {
            return Err(WireError::UnexpectedFileDescriptor);
        }
        arguments.finish()?;
        Ok((request, remaining))
    }

    fn dispatch_display(
        &mut self,
        _object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => {
                let callback = arguments.object()?;
                self.objects.insert(callback, Interface::Callback, 1)?;
                Ok(Request::Sync { callback })
            }
            1 => {
                let registry = arguments.object()?;
                self.objects.insert(registry, Interface::Registry, 1)?;
                Ok(Request::GetRegistry { registry })
            }
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_registry(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        if opcode != 0 {
            return Err(WireError::UnknownOpcode);
        }
        let global_name = arguments.uint()?;
        let interface_name = arguments.string()?;
        let version = arguments.uint()?;
        let new_id = arguments.object()?;
        let global = CORE_GLOBALS
            .iter()
            .copied()
            .find(|global| global.name == global_name)
            .ok_or(WireError::UnknownGlobal)?;
        let interface = Interface::from_name(interface_name).ok_or(WireError::InterfaceMismatch)?;
        if interface != global.interface {
            return Err(WireError::InterfaceMismatch);
        }
        if version == 0 || version > global.version {
            return Err(WireError::InvalidVersion);
        }
        self.objects.insert(new_id, interface, version)?;
        Ok(Request::Bind {
            registry: object.id,
            global: global_name,
            interface,
            version,
            new_id,
        })
    }

    fn dispatch_compositor(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        let new_id = arguments.object()?;
        match opcode {
            0 => {
                self.objects
                    .insert(new_id, Interface::Surface, object.version)?;
                Ok(Request::CreateSurface {
                    compositor: object.id,
                    surface: new_id,
                })
            }
            1 => {
                self.objects.insert(new_id, Interface::Region, 1)?;
                Ok(Request::CreateRegion {
                    compositor: object.id,
                    region: new_id,
                })
            }
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_surface(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => self.destroy(object),
            1 => {
                let buffer = arguments.nullable_object()?;
                if let Some(buffer) = buffer {
                    self.expect(buffer, Interface::Buffer)?;
                }
                Ok(Request::SurfaceAttach {
                    surface: object.id,
                    buffer,
                    x: arguments.int()?,
                    y: arguments.int()?,
                })
            }
            2 | 9 => {
                if opcode == 9 && object.version < 4 {
                    return Err(WireError::UnsupportedVersion);
                }
                Ok(Request::SurfaceDamage {
                    surface: object.id,
                    buffer_coordinates: opcode == 9,
                    x: arguments.int()?,
                    y: arguments.int()?,
                    width: arguments.int()?,
                    height: arguments.int()?,
                })
            }
            3 => {
                let callback = arguments.object()?;
                self.objects.insert(callback, Interface::Callback, 1)?;
                Ok(Request::SurfaceFrame {
                    surface: object.id,
                    callback,
                })
            }
            4 | 5 => {
                let region = arguments.nullable_object()?;
                if let Some(region) = region {
                    self.expect(region, Interface::Region)?;
                }
                Ok(Request::SurfaceSetRegion {
                    surface: object.id,
                    input: opcode == 5,
                    region,
                })
            }
            6 => Ok(Request::SurfaceCommit { surface: object.id }),
            7 => Ok(Request::SurfaceSetBufferTransform {
                surface: object.id,
                transform: arguments.int()?,
            }),
            8 => {
                let scale = arguments.int()?;
                if scale <= 0 {
                    return Err(WireError::InvalidArgument);
                }
                Ok(Request::SurfaceSetBufferScale {
                    surface: object.id,
                    scale,
                })
            }
            10 => {
                if object.version < 5 {
                    return Err(WireError::UnsupportedVersion);
                }
                Ok(Request::SurfaceOffset {
                    surface: object.id,
                    x: arguments.int()?,
                    y: arguments.int()?,
                })
            }
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_region(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => self.destroy(object),
            1 | 2 => {
                let x = arguments.int()?;
                let y = arguments.int()?;
                let width = arguments.int()?;
                let height = arguments.int()?;
                if width < 0 || height < 0 {
                    return Err(WireError::InvalidArgument);
                }
                if opcode == 1 {
                    Ok(Request::RegionAdd {
                        region: object.id,
                        x,
                        y,
                        width,
                        height,
                    })
                } else {
                    Ok(Request::RegionSubtract {
                        region: object.id,
                        x,
                        y,
                        width,
                        height,
                    })
                }
            }
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_shm(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
        file_descriptors: &[i32],
    ) -> Result<Request<'static>, WireError> {
        if opcode != 0 {
            return Err(WireError::UnknownOpcode);
        }
        let [fd] = file_descriptors else {
            return if file_descriptors.is_empty() {
                Err(WireError::MissingFileDescriptor)
            } else {
                Err(WireError::UnexpectedFileDescriptor)
            };
        };
        let pool = arguments.object()?;
        let size = arguments.int()?;
        if size <= 0 {
            return Err(WireError::InvalidArgument);
        }
        self.objects.insert(pool, Interface::ShmPool, 1)?;
        Ok(Request::ShmCreatePool {
            shm: object.id,
            pool,
            fd: *fd,
            size,
        })
    }

    fn dispatch_shm_pool(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => {
                let buffer = arguments.object()?;
                let offset = arguments.int()?;
                let width = arguments.int()?;
                let height = arguments.int()?;
                let stride = arguments.int()?;
                let format = arguments.uint()?;
                if offset < 0 || width <= 0 || height <= 0 || stride <= 0 {
                    return Err(WireError::InvalidArgument);
                }
                self.objects.insert(buffer, Interface::Buffer, 1)?;
                Ok(Request::ShmPoolCreateBuffer {
                    pool: object.id,
                    buffer,
                    offset,
                    width,
                    height,
                    stride,
                    format,
                })
            }
            1 => self.destroy(object),
            2 => {
                let size = arguments.int()?;
                if size <= 0 {
                    return Err(WireError::InvalidArgument);
                }
                Ok(Request::ShmPoolResize {
                    pool: object.id,
                    size,
                })
            }
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_destroy_only(
        &mut self,
        object: Object,
        opcode: u16,
        _arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        if opcode == 0 {
            self.destroy(object)
        } else {
            Err(WireError::UnknownOpcode)
        }
    }

    fn dispatch_seat(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => {
                let pointer = arguments.object()?;
                self.objects
                    .insert(pointer, Interface::Pointer, object.version)?;
                Ok(Request::SeatGetPointer {
                    seat: object.id,
                    pointer,
                })
            }
            1 => {
                let keyboard = arguments.object()?;
                self.objects
                    .insert(keyboard, Interface::Keyboard, object.version)?;
                Ok(Request::SeatGetKeyboard {
                    seat: object.id,
                    keyboard,
                })
            }
            3 if object.version >= 5 => self.destroy(object),
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_pointer(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => {
                let serial = arguments.uint()?;
                let surface = arguments.nullable_object()?;
                if let Some(surface) = surface {
                    self.expect(surface, Interface::Surface)?;
                }
                Ok(Request::PointerSetCursor {
                    pointer: object.id,
                    serial,
                    surface,
                    hotspot_x: arguments.int()?,
                    hotspot_y: arguments.int()?,
                })
            }
            1 if object.version >= 3 => self.destroy(object),
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_release(
        &mut self,
        object: Object,
        opcode: u16,
        _arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        if opcode != 0 {
            return Err(WireError::UnknownOpcode);
        }
        let minimum = match object.interface {
            Interface::Keyboard => 3,
            Interface::Output => 3,
            _ => return Err(WireError::WrongInterface),
        };
        if object.version < minimum {
            return Err(WireError::UnsupportedVersion);
        }
        self.destroy(object)
    }

    fn dispatch_xdg_wm_base(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => self.destroy(object),
            2 => {
                let xdg_surface = arguments.object()?;
                let surface = arguments.object()?;
                self.expect(surface, Interface::Surface)?;
                self.objects
                    .insert(xdg_surface, Interface::XdgSurface, object.version)?;
                Ok(Request::XdgGetSurface {
                    wm_base: object.id,
                    xdg_surface,
                    surface,
                })
            }
            3 => Ok(Request::XdgPong {
                wm_base: object.id,
                serial: arguments.uint()?,
            }),
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_xdg_surface(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'_>,
    ) -> Result<Request<'static>, WireError> {
        match opcode {
            0 => self.destroy(object),
            1 => {
                let toplevel = arguments.object()?;
                self.objects
                    .insert(toplevel, Interface::XdgToplevel, object.version)?;
                Ok(Request::XdgGetToplevel {
                    xdg_surface: object.id,
                    toplevel,
                })
            }
            3 => {
                let x = arguments.int()?;
                let y = arguments.int()?;
                let width = arguments.int()?;
                let height = arguments.int()?;
                if width <= 0 || height <= 0 {
                    return Err(WireError::InvalidArgument);
                }
                Ok(Request::XdgSetWindowGeometry {
                    xdg_surface: object.id,
                    x,
                    y,
                    width,
                    height,
                })
            }
            4 => Ok(Request::XdgAckConfigure {
                xdg_surface: object.id,
                serial: arguments.uint()?,
            }),
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn dispatch_xdg_toplevel<'a>(
        &mut self,
        object: Object,
        opcode: u16,
        arguments: &mut ArgumentReader<'a>,
    ) -> Result<Request<'a>, WireError> {
        match opcode {
            0 => self.destroy(object),
            2 => Ok(Request::ToplevelSetTitle {
                toplevel: object.id,
                title: arguments.string()?,
            }),
            3 => Ok(Request::ToplevelSetAppId {
                toplevel: object.id,
                app_id: arguments.string()?,
            }),
            5 => {
                let seat = arguments.object()?;
                self.expect(seat, Interface::Seat)?;
                Ok(Request::ToplevelMove {
                    toplevel: object.id,
                    seat,
                    serial: arguments.uint()?,
                })
            }
            6 => {
                let seat = arguments.object()?;
                self.expect(seat, Interface::Seat)?;
                Ok(Request::ToplevelResize {
                    toplevel: object.id,
                    seat,
                    serial: arguments.uint()?,
                    edges: arguments.uint()?,
                })
            }
            9 => Ok(Request::ToplevelSetMaximized {
                toplevel: object.id,
                maximized: true,
            }),
            10 => Ok(Request::ToplevelSetMaximized {
                toplevel: object.id,
                maximized: false,
            }),
            11 => {
                let output = arguments.nullable_object()?;
                if let Some(output) = output {
                    self.expect(output, Interface::Output)?;
                }
                Ok(Request::ToplevelSetFullscreen {
                    toplevel: object.id,
                    output,
                })
            }
            12 => Ok(Request::ToplevelSetFullscreen {
                toplevel: object.id,
                output: None,
            }),
            13 => Ok(Request::ToplevelSetMinimized {
                toplevel: object.id,
            }),
            _ => Err(WireError::UnknownOpcode),
        }
    }

    fn expect(&self, id: u32, interface: Interface) -> Result<Object, WireError> {
        let object = self.objects.get(id)?;
        if object.interface != interface {
            return Err(WireError::WrongInterface);
        }
        Ok(object)
    }

    fn destroy(&mut self, object: Object) -> Result<Request<'static>, WireError> {
        self.objects.retire(object.id)?;
        Ok(Request::Destroy {
            object: object.id,
            interface: object.interface,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(
        object_id: u32,
        opcode: u16,
        build: impl FnOnce(&mut MessageBuilder<'_>),
    ) -> [u8; 128] {
        let mut bytes = [0; 128];
        let mut builder = MessageBuilder::new(&mut bytes, object_id, opcode).unwrap();
        build(&mut builder);
        let length = builder.finish().unwrap().len();
        bytes[length..].fill(0xCC);
        bytes
    }

    fn frame_length(bytes: &[u8]) -> usize {
        usize::from(u16::from_le_bytes([bytes[6], bytes[7]]))
    }

    fn dispatch<'a, const N: usize>(
        connection: &mut Connection<N>,
        bytes: &'a [u8],
        fds: &[i32],
    ) -> Result<Request<'a>, WireError> {
        let length = frame_length(bytes);
        let (request, remaining) = connection.dispatch(&bytes[..length], fds)?;
        assert!(remaining.is_empty());
        Ok(request)
    }

    fn bind<const N: usize>(
        connection: &mut Connection<N>,
        registry: u32,
        global: Global,
        id: u32,
    ) {
        let bytes = message(registry, 0, |builder| {
            builder.uint(global.name).unwrap();
            builder.string(global.interface.name()).unwrap();
            builder.uint(global.version).unwrap();
            builder.object(id).unwrap();
        });
        assert_eq!(
            dispatch(connection, &bytes, &[]).unwrap(),
            Request::Bind {
                registry,
                global: global.name,
                interface: global.interface,
                version: global.version,
                new_id: id,
            }
        );
    }

    #[test]
    fn decodes_framed_messages_and_keeps_the_remainder() {
        let mut bytes = [0; 64];
        let first_len = {
            let mut builder = MessageBuilder::new(&mut bytes, 7, 3).unwrap();
            builder.uint(0x1122_3344).unwrap();
            builder.string("niri").unwrap();
            builder.finish().unwrap().len()
        };
        let second_len = {
            let mut builder = MessageBuilder::new(&mut bytes[first_len..], 9, 2).unwrap();
            builder.int(-17).unwrap();
            builder.finish().unwrap().len()
        };
        let (first, remaining) = Frame::decode(&bytes[..first_len + second_len]).unwrap();
        assert_eq!(
            first.header,
            Header {
                object_id: 7,
                opcode: 3,
                size: first_len as u16,
            }
        );
        let mut arguments = ArgumentReader::new(first.payload);
        assert_eq!(arguments.uint().unwrap(), 0x1122_3344);
        assert_eq!(arguments.string().unwrap(), "niri");
        arguments.finish().unwrap();
        let (second, remaining) = Frame::decode(remaining).unwrap();
        assert_eq!(second.header.object_id, 9);
        assert_eq!(ArgumentReader::new(second.payload).int().unwrap(), -17);
        assert!(remaining.is_empty());
    }

    #[test]
    fn encodes_registry_lifecycle_and_display_events() {
        let mut bytes = [0; 96];
        let event = encode_registry_global(&mut bytes, 2, CORE_GLOBALS[0]).unwrap();
        let (frame, remaining) = Frame::decode(event).unwrap();
        assert!(remaining.is_empty());
        assert_eq!(frame.header.object_id, 2);
        assert_eq!(frame.header.opcode, 0);
        let mut arguments = ArgumentReader::new(frame.payload);
        assert_eq!(arguments.uint().unwrap(), CORE_GLOBALS[0].name);
        assert_eq!(arguments.string().unwrap(), Interface::Compositor.name());
        assert_eq!(arguments.uint().unwrap(), CORE_GLOBALS[0].version);
        arguments.finish().unwrap();

        let event = encode_display_delete_id(&mut bytes, 19).unwrap();
        let (frame, _) = Frame::decode(event).unwrap();
        assert_eq!(frame.header.object_id, DISPLAY_OBJECT_ID);
        assert_eq!(frame.header.opcode, 1);
        assert_eq!(ArgumentReader::new(frame.payload).object().unwrap(), 19);
    }

    #[test]
    fn rejects_malformed_headers_strings_and_trailing_arguments() {
        assert_eq!(Frame::decode(&[0; 7]), Err(WireError::TruncatedHeader));
        let mut header = [0; 12];
        header[0..4].copy_from_slice(&1u32.to_le_bytes());
        header[4..8].copy_from_slice(&((10u32 << 16) | 1).to_le_bytes());
        assert_eq!(Frame::decode(&header), Err(WireError::UnalignedSize));
        header[4..8].copy_from_slice(&((16u32 << 16) | 1).to_le_bytes());
        assert_eq!(Frame::decode(&header), Err(WireError::TruncatedMessage));

        let mut invalid = [0; 8];
        invalid[0..4].copy_from_slice(&3u32.to_le_bytes());
        invalid[4..8].copy_from_slice(b"abcx");
        let mut reader = ArgumentReader::new(&invalid);
        assert_eq!(reader.string(), Err(WireError::InvalidString));

        let reader = ArgumentReader::new(&[1, 0, 0, 0]);
        assert_eq!(reader.finish(), Err(WireError::TrailingArguments));
    }

    #[test]
    fn creates_registry_and_binds_only_advertised_versions() {
        let mut connection = Connection::<16>::new().unwrap();
        let get_registry = message(DISPLAY_OBJECT_ID, 1, |builder| {
            builder.object(2).unwrap();
        });
        assert_eq!(
            dispatch(&mut connection, &get_registry, &[]).unwrap(),
            Request::GetRegistry { registry: 2 }
        );
        bind(&mut connection, 2, CORE_GLOBALS[0], 3);
        assert_eq!(
            connection.objects().get(3).unwrap().interface,
            Interface::Compositor
        );

        let too_new = message(2, 0, |builder| {
            builder.uint(CORE_GLOBALS[0].name).unwrap();
            builder.string(Interface::Compositor.name()).unwrap();
            builder.uint(CORE_GLOBALS[0].version + 1).unwrap();
            builder.object(4).unwrap();
        });
        assert_eq!(
            dispatch(&mut connection, &too_new, &[]),
            Err(WireError::InvalidVersion)
        );
    }

    #[test]
    fn preserves_object_ids_until_delete_id_is_sent() {
        let mut objects = ObjectMap::<2>::new().unwrap();
        objects.insert(7, Interface::Surface, 1).unwrap();
        objects.retire(7).unwrap();
        assert_eq!(
            objects.insert(7, Interface::Surface, 1),
            Err(WireError::DuplicateObject)
        );
        objects.delete_id_sent(7).unwrap();
        objects.insert(7, Interface::Surface, 1).unwrap();
        assert_eq!(objects.active_count(), 2);
    }

    #[test]
    fn dispatches_compositor_surface_and_frame_lifecycle() {
        let mut connection = Connection::<16>::new().unwrap();
        let get_registry = message(1, 1, |builder| builder.object(2).unwrap());
        dispatch(&mut connection, &get_registry, &[]).unwrap();
        bind(&mut connection, 2, CORE_GLOBALS[0], 3);
        let create = message(3, 0, |builder| builder.object(4).unwrap());
        assert_eq!(
            dispatch(&mut connection, &create, &[]).unwrap(),
            Request::CreateSurface {
                compositor: 3,
                surface: 4,
            }
        );
        let frame = message(4, 3, |builder| builder.object(5).unwrap());
        assert_eq!(
            dispatch(&mut connection, &frame, &[]).unwrap(),
            Request::SurfaceFrame {
                surface: 4,
                callback: 5,
            }
        );
        let commit = message(4, 6, |_| {});
        assert_eq!(
            dispatch(&mut connection, &commit, &[]).unwrap(),
            Request::SurfaceCommit { surface: 4 }
        );
        let destroy = message(4, 0, |_| {});
        assert_eq!(
            dispatch(&mut connection, &destroy, &[]).unwrap(),
            Request::Destroy {
                object: 4,
                interface: Interface::Surface,
            }
        );
        assert_eq!(connection.objects().get(4), Err(WireError::UnknownObject));
    }

    #[test]
    fn imports_shm_buffers_with_out_of_band_file_descriptors() {
        let mut connection = Connection::<24>::new().unwrap();
        let get_registry = message(1, 1, |builder| builder.object(2).unwrap());
        dispatch(&mut connection, &get_registry, &[]).unwrap();
        bind(&mut connection, 2, CORE_GLOBALS[1], 3);
        let pool = message(3, 0, |builder| {
            builder.object(4).unwrap();
            builder.int(4096).unwrap();
        });
        assert_eq!(
            dispatch(&mut connection, &pool, &[]),
            Err(WireError::MissingFileDescriptor)
        );
        assert_eq!(
            dispatch(&mut connection, &pool, &[11]).unwrap(),
            Request::ShmCreatePool {
                shm: 3,
                pool: 4,
                fd: 11,
                size: 4096,
            }
        );
        let buffer = message(4, 0, |builder| {
            builder.object(5).unwrap();
            builder.int(0).unwrap();
            builder.int(32).unwrap();
            builder.int(16).unwrap();
            builder.int(128).unwrap();
            builder.uint(0).unwrap();
        });
        assert_eq!(
            dispatch(&mut connection, &buffer, &[]).unwrap(),
            Request::ShmPoolCreateBuffer {
                pool: 4,
                buffer: 5,
                offset: 0,
                width: 32,
                height: 16,
                stride: 128,
                format: 0,
            }
        );
    }

    #[test]
    fn constructs_xdg_toplevel_metadata_requests() {
        let mut connection = Connection::<24>::new().unwrap();
        let registry = message(1, 1, |builder| builder.object(2).unwrap());
        dispatch(&mut connection, &registry, &[]).unwrap();
        bind(&mut connection, 2, CORE_GLOBALS[0], 3);
        bind(&mut connection, 2, CORE_GLOBALS[4], 4);
        let surface = message(3, 0, |builder| builder.object(5).unwrap());
        dispatch(&mut connection, &surface, &[]).unwrap();
        let xdg_surface = message(4, 2, |builder| {
            builder.object(6).unwrap();
            builder.object(5).unwrap();
        });
        dispatch(&mut connection, &xdg_surface, &[]).unwrap();
        let toplevel = message(6, 1, |builder| builder.object(7).unwrap());
        dispatch(&mut connection, &toplevel, &[]).unwrap();
        let title = message(7, 2, |builder| builder.string("Terminal").unwrap());
        assert_eq!(
            dispatch(&mut connection, &title, &[]).unwrap(),
            Request::ToplevelSetTitle {
                toplevel: 7,
                title: "Terminal",
            }
        );
    }

    #[test]
    fn rejects_wrong_interfaces_duplicate_ids_and_unexpected_fds() {
        let mut connection = Connection::<8>::new().unwrap();
        let registry = message(1, 1, |builder| builder.object(2).unwrap());
        dispatch(&mut connection, &registry, &[]).unwrap();
        let duplicate = message(1, 0, |builder| builder.object(2).unwrap());
        assert_eq!(
            dispatch(&mut connection, &duplicate, &[]),
            Err(WireError::DuplicateObject)
        );
        let unexpected_fd = message(1, 0, |builder| builder.object(3).unwrap());
        assert_eq!(
            dispatch(&mut connection, &unexpected_fd, &[4]),
            Err(WireError::UnexpectedFileDescriptor)
        );
        let wrong_opcode = message(2, 9, |_| {});
        assert_eq!(
            dispatch(&mut connection, &wrong_opcode, &[]),
            Err(WireError::UnknownOpcode)
        );
    }
}
