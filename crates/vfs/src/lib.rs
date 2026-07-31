// SPDX-License-Identifier: 0BSD

#![no_std]

pub const MAX_PATH_COMPONENTS: usize = 16;
pub const MAX_MOUNT_PATH_BYTES: usize = 256;
pub const FIRST_FILE_DESCRIPTOR: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsError {
    InvalidPath,
    TooManyComponents,
    MountPathTooLong,
    MountTableFull,
    DuplicateMount,
    MountNotFound,
    FileTableFull,
    BadFileDescriptor,
    InvalidOffset,
    NotReadable,
    NotWritable,
}

#[derive(Debug, Eq, PartialEq)]
pub struct AbsolutePath<'a> {
    components: [&'a [u8]; MAX_PATH_COMPONENTS],
    component_count: usize,
}

impl<'a> AbsolutePath<'a> {
    pub fn parse(path: &'a [u8]) -> Result<Self, VfsError> {
        if path.first() != Some(&b'/') || path.contains(&0) {
            return Err(VfsError::InvalidPath);
        }
        let mut components = [&[][..]; MAX_PATH_COMPONENTS];
        let mut component_count = 0usize;
        for component in path.split(|byte| *byte == b'/') {
            if component.is_empty() || component == b"." {
                continue;
            }
            if component == b".." {
                component_count = component_count.saturating_sub(1);
                continue;
            }
            if component.len() > 255 {
                return Err(VfsError::InvalidPath);
            }
            if component_count == MAX_PATH_COMPONENTS {
                return Err(VfsError::TooManyComponents);
            }
            components[component_count] = component;
            component_count += 1;
        }
        Ok(Self {
            components,
            component_count,
        })
    }

    pub fn components(&self) -> &[&'a [u8]] {
        &self.components[..self.component_count]
    }

    pub const fn component_count(&self) -> usize {
        self.component_count
    }
}

#[derive(Clone, Copy)]
struct MountEntry {
    path: [u8; MAX_MOUNT_PATH_BYTES],
    path_length: usize,
    component_count: usize,
    filesystem_id: u16,
}

pub struct MountTable<const N: usize> {
    entries: [Option<MountEntry>; N],
    count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MountResolution {
    pub filesystem_id: u16,
    pub matched_components: usize,
}

impl<const N: usize> MountTable<N> {
    pub const fn new() -> Self {
        Self {
            entries: [None; N],
            count: 0,
        }
    }

    pub fn mount(&mut self, path: &AbsolutePath<'_>, filesystem_id: u16) -> Result<(), VfsError> {
        let (canonical, path_length) = canonical_path(path)?;
        if self
            .entries
            .iter()
            .flatten()
            .any(|entry| entry.path[..entry.path_length] == canonical[..path_length])
        {
            return Err(VfsError::DuplicateMount);
        }
        let slot = self
            .entries
            .iter_mut()
            .find(|entry| entry.is_none())
            .ok_or(VfsError::MountTableFull)?;
        *slot = Some(MountEntry {
            path: canonical,
            path_length,
            component_count: path.component_count(),
            filesystem_id,
        });
        self.count += 1;
        Ok(())
    }

    pub fn resolve(&self, path: &AbsolutePath<'_>) -> Result<MountResolution, VfsError> {
        self.entries
            .iter()
            .flatten()
            .filter(|entry| entry.component_count <= path.component_count())
            .filter(|entry| mount_matches(entry, path))
            .max_by_key(|entry| entry.component_count)
            .map(|entry| MountResolution {
                filesystem_id: entry.filesystem_id,
                matched_components: entry.component_count,
            })
            .ok_or(VfsError::MountNotFound)
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }
}

impl<const N: usize> Default for MountTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

fn canonical_path(
    path: &AbsolutePath<'_>,
) -> Result<([u8; MAX_MOUNT_PATH_BYTES], usize), VfsError> {
    let mut output = [0u8; MAX_MOUNT_PATH_BYTES];
    output[0] = b'/';
    let mut length = 1;
    for (index, component) in path.components().iter().enumerate() {
        if index != 0 {
            if length == output.len() {
                return Err(VfsError::MountPathTooLong);
            }
            output[length] = b'/';
            length += 1;
        }
        let end = length
            .checked_add(component.len())
            .ok_or(VfsError::MountPathTooLong)?;
        if end > output.len() {
            return Err(VfsError::MountPathTooLong);
        }
        output[length..end].copy_from_slice(component);
        length = end;
    }
    Ok((output, length))
}

fn mount_matches(entry: &MountEntry, path: &AbsolutePath<'_>) -> bool {
    if entry.component_count == 0 {
        return entry.path_length == 1 && entry.path[0] == b'/';
    }
    let mut cursor = 1;
    for (index, component) in path.components()[..entry.component_count]
        .iter()
        .enumerate()
    {
        if index != 0 {
            if entry.path.get(cursor) != Some(&b'/') {
                return false;
            }
            cursor += 1;
        }
        let end = cursor + component.len();
        if end > entry.path_length || entry.path[cursor..end] != **component {
            return false;
        }
        cursor = end;
    }
    cursor == entry.path_length
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileNode {
    pub filesystem_id: u16,
    pub node_id: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorObject {
    File(FileNode),
    LocalSocket { index: u16, generation: u16 },
    SharedMemory { index: u16, generation: u16 },
    DesktopEvents,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessMode {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl AccessMode {
    const fn readable(self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    const fn writable(self) -> bool {
        matches!(self, Self::WriteOnly | Self::ReadWrite)
    }
}

#[derive(Clone, Copy)]
struct Descriptor {
    object: DescriptorObject,
    offset: u64,
    access_mode: AccessMode,
    open: bool,
}

impl Descriptor {
    const EMPTY: Self = Self {
        object: DescriptorObject::File(FileNode {
            filesystem_id: 0,
            node_id: 0,
            size: 0,
        }),
        offset: 0,
        access_mode: AccessMode::ReadOnly,
        open: false,
    };
}

pub struct FileDescriptorTable<const N: usize> {
    descriptors: [Descriptor; N],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadWindow {
    pub node: FileNode,
    pub offset: u64,
    pub length: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteWindow {
    pub node: FileNode,
    pub offset: u64,
    pub length: usize,
}

impl<const N: usize> FileDescriptorTable<N> {
    pub const fn new() -> Self {
        Self {
            descriptors: [Descriptor::EMPTY; N],
        }
    }

    pub fn open(&mut self, node: FileNode) -> Result<u32, VfsError> {
        self.open_with_mode(node, AccessMode::ReadOnly)
    }

    pub fn open_with_mode(
        &mut self,
        node: FileNode,
        access_mode: AccessMode,
    ) -> Result<u32, VfsError> {
        self.open_object(DescriptorObject::File(node), access_mode)
    }

    pub fn open_local_socket(&mut self, index: u16, generation: u16) -> Result<u32, VfsError> {
        self.open_object(
            DescriptorObject::LocalSocket { index, generation },
            AccessMode::ReadWrite,
        )
    }

    pub fn open_shared_memory(&mut self, index: u16, generation: u16) -> Result<u32, VfsError> {
        self.open_object(
            DescriptorObject::SharedMemory { index, generation },
            AccessMode::ReadWrite,
        )
    }

    pub fn open_shared_memory_read_only(
        &mut self,
        index: u16,
        generation: u16,
    ) -> Result<u32, VfsError> {
        self.open_object(
            DescriptorObject::SharedMemory { index, generation },
            AccessMode::ReadOnly,
        )
    }

    pub fn open_desktop_events(&mut self, generation: u64) -> Result<u32, VfsError> {
        let fd = self.open_object(DescriptorObject::DesktopEvents, AccessMode::ReadOnly)?;
        self.descriptor_mut(fd)?.offset = generation;
        Ok(fd)
    }

    fn open_object(
        &mut self,
        object: DescriptorObject,
        access_mode: AccessMode,
    ) -> Result<u32, VfsError> {
        let index = self
            .descriptors
            .iter()
            .position(|descriptor| !descriptor.open)
            .ok_or(VfsError::FileTableFull)?;
        let descriptor_number = u32::try_from(index).map_err(|_| VfsError::FileTableFull)?;
        let fd = FIRST_FILE_DESCRIPTOR
            .checked_add(descriptor_number)
            .ok_or(VfsError::FileTableFull)?;
        self.descriptors[index] = Descriptor {
            object,
            offset: 0,
            access_mode,
            open: true,
        };
        Ok(fd)
    }

    pub fn read_window(&self, fd: u32, requested: usize) -> Result<ReadWindow, VfsError> {
        let descriptor = self.descriptor(fd)?;
        if !descriptor.access_mode.readable() {
            return Err(VfsError::NotReadable);
        }
        let DescriptorObject::File(node) = descriptor.object else {
            return Err(VfsError::BadFileDescriptor);
        };
        let remaining = node.size.saturating_sub(descriptor.offset);
        Ok(ReadWindow {
            node,
            offset: descriptor.offset,
            length: requested.min(usize::try_from(remaining).unwrap_or(usize::MAX)),
        })
    }

    pub fn write_window(&self, fd: u32, requested: usize) -> Result<WriteWindow, VfsError> {
        let descriptor = self.descriptor(fd)?;
        if !descriptor.access_mode.writable() {
            return Err(VfsError::NotWritable);
        }
        let DescriptorObject::File(node) = descriptor.object else {
            return Err(VfsError::BadFileDescriptor);
        };
        let remaining = node.size.saturating_sub(descriptor.offset);
        Ok(WriteWindow {
            node,
            offset: descriptor.offset,
            length: requested.min(usize::try_from(remaining).unwrap_or(usize::MAX)),
        })
    }

    pub fn append_window(&self, fd: u32, requested: usize) -> Result<WriteWindow, VfsError> {
        let descriptor = self.descriptor(fd)?;
        if !descriptor.access_mode.writable() {
            return Err(VfsError::NotWritable);
        }
        let DescriptorObject::File(node) = descriptor.object else {
            return Err(VfsError::BadFileDescriptor);
        };
        if descriptor.offset != node.size
            || descriptor
                .offset
                .checked_add(u64::try_from(requested).map_err(|_| VfsError::InvalidOffset)?)
                .is_none()
        {
            return Err(VfsError::InvalidOffset);
        }
        Ok(WriteWindow {
            node,
            offset: descriptor.offset,
            length: requested,
        })
    }

    pub fn set_size(&mut self, fd: u32, size: u64) -> Result<(), VfsError> {
        let descriptor = self.descriptor_mut(fd)?;
        if !descriptor.access_mode.writable() {
            return Err(VfsError::NotWritable);
        }
        if descriptor.offset > size {
            return Err(VfsError::InvalidOffset);
        }
        let DescriptorObject::File(ref mut node) = descriptor.object else {
            return Err(VfsError::BadFileDescriptor);
        };
        node.size = size;
        Ok(())
    }

    pub fn advance(&mut self, fd: u32, length: usize) -> Result<(), VfsError> {
        let descriptor = self.descriptor_mut(fd)?;
        let length = u64::try_from(length).map_err(|_| VfsError::InvalidOffset)?;
        let new_offset = descriptor
            .offset
            .checked_add(length)
            .ok_or(VfsError::InvalidOffset)?;
        let DescriptorObject::File(node) = descriptor.object else {
            return Err(VfsError::BadFileDescriptor);
        };
        if new_offset > node.size {
            return Err(VfsError::InvalidOffset);
        }
        descriptor.offset = new_offset;
        Ok(())
    }

    pub fn seek(&mut self, fd: u32, offset: u64) -> Result<(), VfsError> {
        let descriptor = self.descriptor_mut(fd)?;
        let DescriptorObject::File(node) = descriptor.object else {
            return Err(VfsError::BadFileDescriptor);
        };
        if offset > node.size {
            return Err(VfsError::InvalidOffset);
        }
        descriptor.offset = offset;
        Ok(())
    }

    pub fn close(&mut self, fd: u32) -> Result<(), VfsError> {
        *self.descriptor_mut(fd)? = Descriptor::EMPTY;
        Ok(())
    }

    pub fn object(&self, fd: u32) -> Result<DescriptorObject, VfsError> {
        Ok(self.descriptor(fd)?.object)
    }

    pub fn readable_object(&self, fd: u32) -> Result<(DescriptorObject, u64), VfsError> {
        let descriptor = self.descriptor(fd)?;
        if !descriptor.access_mode.readable() {
            return Err(VfsError::NotReadable);
        }
        Ok((descriptor.object, descriptor.offset))
    }

    pub fn advance_object(
        &mut self,
        fd: u32,
        length: usize,
        object_size: u64,
    ) -> Result<(), VfsError> {
        let descriptor = self.descriptor_mut(fd)?;
        let length = u64::try_from(length).map_err(|_| VfsError::InvalidOffset)?;
        let new_offset = descriptor
            .offset
            .checked_add(length)
            .ok_or(VfsError::InvalidOffset)?;
        if new_offset > object_size {
            return Err(VfsError::InvalidOffset);
        }
        descriptor.offset = new_offset;
        Ok(())
    }

    pub fn set_object_offset(&mut self, fd: u32, offset: u64) -> Result<(), VfsError> {
        let descriptor = self.descriptor_mut(fd)?;
        if !descriptor.access_mode.readable()
            || matches!(descriptor.object, DescriptorObject::File(_))
        {
            return Err(VfsError::BadFileDescriptor);
        }
        descriptor.offset = offset;
        Ok(())
    }

    pub fn snapshot_objects(&self, output: &mut [Option<DescriptorObject>]) -> usize {
        output.fill(None);
        let mut copied = 0;
        for descriptor in self.descriptors.iter().filter(|descriptor| descriptor.open) {
            let Some(slot) = output.get_mut(copied) else {
                break;
            };
            *slot = Some(descriptor.object);
            copied += 1;
        }
        copied
    }

    pub fn close_all(&mut self) -> usize {
        let mut closed = 0;
        for descriptor in &mut self.descriptors {
            if descriptor.open {
                *descriptor = Descriptor::EMPTY;
                closed += 1;
            }
        }
        closed
    }

    fn descriptor(&self, fd: u32) -> Result<&Descriptor, VfsError> {
        let index = descriptor_index(fd)?;
        self.descriptors
            .get(index)
            .filter(|descriptor| descriptor.open)
            .ok_or(VfsError::BadFileDescriptor)
    }

    fn descriptor_mut(&mut self, fd: u32) -> Result<&mut Descriptor, VfsError> {
        let index = descriptor_index(fd)?;
        self.descriptors
            .get_mut(index)
            .filter(|descriptor| descriptor.open)
            .ok_or(VfsError::BadFileDescriptor)
    }
}

impl<const N: usize> Default for FileDescriptorTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

fn descriptor_index(fd: u32) -> Result<usize, VfsError> {
    let index = fd
        .checked_sub(FIRST_FILE_DESCRIPTOR)
        .ok_or(VfsError::BadFileDescriptor)?;
    usize::try_from(index).map_err(|_| VfsError::BadFileDescriptor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_absolute_paths_without_allocation() {
        let path = AbsolutePath::parse(b"/usr//share/../etc/./system.conf").unwrap();
        let expected: &[&[u8]] = &[b"usr", b"etc", b"system.conf"];
        assert_eq!(path.components(), expected);
        assert_eq!(AbsolutePath::parse(b"relative"), Err(VfsError::InvalidPath));
        assert_eq!(
            AbsolutePath::parse(b"/nul\0path"),
            Err(VfsError::InvalidPath)
        );
    }

    #[test]
    fn resolves_the_longest_mount_prefix() {
        let mut mounts = MountTable::<3>::new();
        mounts
            .mount(&AbsolutePath::parse(b"/").unwrap(), 1)
            .unwrap();
        mounts
            .mount(&AbsolutePath::parse(b"/mnt").unwrap(), 2)
            .unwrap();
        mounts
            .mount(&AbsolutePath::parse(b"/mnt/data").unwrap(), 3)
            .unwrap();
        assert_eq!(
            mounts.resolve(&AbsolutePath::parse(b"/etc/config").unwrap()),
            Ok(MountResolution {
                filesystem_id: 1,
                matched_components: 0,
            })
        );
        assert_eq!(
            mounts.resolve(&AbsolutePath::parse(b"/mnt/data/file").unwrap()),
            Ok(MountResolution {
                filesystem_id: 3,
                matched_components: 2,
            })
        );
        assert_eq!(
            mounts.mount(&AbsolutePath::parse(b"/mnt").unwrap(), 4),
            Err(VfsError::DuplicateMount)
        );
    }

    #[test]
    fn tracks_descriptor_offsets_seek_and_close() {
        let mut descriptors = FileDescriptorTable::<2>::new();
        let node = FileNode {
            filesystem_id: 1,
            node_id: 42,
            size: 10,
        };
        let fd = descriptors.open(node).unwrap();
        assert_eq!(fd, 3);
        assert_eq!(
            descriptors.read_window(fd, 6),
            Ok(ReadWindow {
                node,
                offset: 0,
                length: 6,
            })
        );
        descriptors.advance(fd, 6).unwrap();
        assert_eq!(descriptors.read_window(fd, 8).unwrap().length, 4);
        descriptors.seek(fd, 2).unwrap();
        assert_eq!(descriptors.read_window(fd, 3).unwrap().offset, 2);
        assert_eq!(descriptors.seek(fd, 11), Err(VfsError::InvalidOffset));
        descriptors.close(fd).unwrap();
        assert_eq!(
            descriptors.read_window(fd, 1),
            Err(VfsError::BadFileDescriptor)
        );
        let first = descriptors.open(node).unwrap();
        let second = descriptors.open(node).unwrap();
        assert_eq!((first, second), (3, 4));
        assert_eq!(descriptors.close_all(), 2);
        assert_eq!(descriptors.close_all(), 0);
        assert_eq!(
            descriptors.read_window(first, 1),
            Err(VfsError::BadFileDescriptor)
        );
    }

    #[test]
    fn stores_local_sockets_without_treating_them_as_files() {
        let mut descriptors = FileDescriptorTable::<2>::new();
        let fd = descriptors.open_local_socket(7, 3).unwrap();
        assert_eq!(
            descriptors.object(fd),
            Ok(DescriptorObject::LocalSocket {
                index: 7,
                generation: 3,
            })
        );
        assert_eq!(
            descriptors.read_window(fd, 1),
            Err(VfsError::BadFileDescriptor)
        );
        assert_eq!(
            descriptors.write_window(fd, 1),
            Err(VfsError::BadFileDescriptor)
        );
        descriptors.close(fd).unwrap();
        assert_eq!(descriptors.object(fd), Err(VfsError::BadFileDescriptor));
    }

    #[test]
    fn stores_shared_memory_without_exposing_file_windows() {
        let mut descriptors = FileDescriptorTable::<2>::new();
        let fd = descriptors.open_shared_memory(2, 7).unwrap();
        assert_eq!(
            descriptors.object(fd),
            Ok(DescriptorObject::SharedMemory {
                index: 2,
                generation: 7,
            })
        );
        assert_eq!(
            descriptors.read_window(fd, 1),
            Err(VfsError::BadFileDescriptor)
        );
        assert_eq!(descriptors.seek(fd, 0), Err(VfsError::BadFileDescriptor));
        assert_eq!(
            descriptors.readable_object(fd),
            Ok((
                DescriptorObject::SharedMemory {
                    index: 2,
                    generation: 7,
                },
                0,
            ))
        );
        descriptors.advance_object(fd, 6, 10).unwrap();
        assert_eq!(descriptors.readable_object(fd).unwrap().1, 6);
        assert_eq!(
            descriptors.advance_object(fd, 5, 10),
            Err(VfsError::InvalidOffset)
        );
        descriptors.close(fd).unwrap();
        let fd = descriptors.open_shared_memory_read_only(3, 8).unwrap();
        assert_eq!(descriptors.readable_object(fd).unwrap().1, 0);
    }

    #[test]
    fn tracks_a_read_only_desktop_event_generation_cursor() {
        let mut descriptors = FileDescriptorTable::<1>::new();
        let fd = descriptors.open_desktop_events(7).unwrap();
        assert_eq!(
            descriptors.readable_object(fd),
            Ok((DescriptorObject::DesktopEvents, 7))
        );
        descriptors.set_object_offset(fd, 11).unwrap();
        assert_eq!(descriptors.readable_object(fd).unwrap().1, 11);
        assert_eq!(descriptors.write_window(fd, 1), Err(VfsError::NotWritable));
    }

    #[test]
    fn enforces_descriptor_access_modes() {
        let mut descriptors = FileDescriptorTable::<1>::new();
        let node = FileNode {
            filesystem_id: 1,
            node_id: 42,
            size: 10,
        };
        let fd = descriptors.open(node).unwrap();
        assert_eq!(descriptors.write_window(fd, 1), Err(VfsError::NotWritable));
        descriptors.close(fd).unwrap();

        let fd = descriptors
            .open_with_mode(node, AccessMode::WriteOnly)
            .unwrap();
        assert_eq!(descriptors.read_window(fd, 1), Err(VfsError::NotReadable));
        assert_eq!(
            descriptors.write_window(fd, 6),
            Ok(WriteWindow {
                node,
                offset: 0,
                length: 6,
            })
        );
        descriptors.close(fd).unwrap();

        let fd = descriptors
            .open_with_mode(node, AccessMode::ReadWrite)
            .unwrap();
        assert_eq!(descriptors.read_window(fd, 1).unwrap().length, 1);
        assert_eq!(descriptors.write_window(fd, 1).unwrap().length, 1);
    }

    #[test]
    fn extends_a_writable_descriptor_at_eof() {
        let mut descriptors = FileDescriptorTable::<1>::new();
        let node = FileNode {
            filesystem_id: 1,
            node_id: 42,
            size: 10,
        };
        let fd = descriptors
            .open_with_mode(node, AccessMode::ReadWrite)
            .unwrap();
        assert_eq!(
            descriptors.append_window(fd, 1),
            Err(VfsError::InvalidOffset)
        );
        descriptors.seek(fd, 10).unwrap();
        assert_eq!(
            descriptors.append_window(fd, 6),
            Ok(WriteWindow {
                node,
                offset: 10,
                length: 6,
            })
        );
        descriptors.set_size(fd, 16).unwrap();
        descriptors.advance(fd, 6).unwrap();
        assert_eq!(descriptors.read_window(fd, 1).unwrap().length, 0);
        assert_eq!(descriptors.set_size(fd, 10), Err(VfsError::InvalidOffset));
        descriptors.seek(fd, 10).unwrap();
        descriptors.set_size(fd, 10).unwrap();
        assert_eq!(descriptors.read_window(fd, 1).unwrap().node.size, 10);
    }
}
