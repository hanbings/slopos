// SPDX-License-Identifier: 0BSD

#![no_std]

pub const LOCAL_PATH_MAX: usize = 108;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketHandle {
    index: u16,
    generation: u16,
}

impl SocketHandle {
    pub const fn from_parts(index: u16, generation: u16) -> Self {
        Self { index, generation }
    }

    pub const fn index(self) -> u16 {
        self.index
    }

    pub const fn generation(self) -> u16 {
        self.generation
    }
}

/// Opaque generation-checked object transferred beside stream bytes.
///
/// The transport deliberately does not interpret the object. The kernel VFS
/// validates it as a shareable descriptor before enqueueing it, mirroring the
/// separation between Unix `SCM_RIGHTS` and the file type being transferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AncillaryRights {
    object: u16,
    generation: u16,
}

impl AncillaryRights {
    pub const fn new(object: u16, generation: u16) -> Self {
        Self { object, generation }
    }

    pub const fn object(self) -> u16 {
        self.object
    }

    pub const fn generation(self) -> u16 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SocketError {
    TableFull,
    InvalidHandle,
    InvalidPath,
    AddressInUse,
    AddressNotFound,
    InvalidState,
    BacklogFull,
    WouldBlock,
    BrokenPipe,
    CounterOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Readiness {
    pub readable: bool,
    pub writable: bool,
    pub peer_closed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SocketState {
    Free,
    Created,
    Bound,
    Listening,
    Connected,
}

#[derive(Clone, Copy)]
struct ByteRing<const BYTES: usize> {
    bytes: [u8; BYTES],
    head: usize,
    length: usize,
}

impl<const BYTES: usize> ByteRing<BYTES> {
    const EMPTY: Self = Self {
        bytes: [0; BYTES],
        head: 0,
        length: 0,
    };

    const fn available(&self) -> usize {
        BYTES - self.length
    }

    fn push(&mut self, input: &[u8]) -> usize {
        let count = input.len().min(self.available());
        for (offset, byte) in input[..count].iter().copied().enumerate() {
            let index = (self.head + self.length + offset) % BYTES;
            self.bytes[index] = byte;
        }
        self.length += count;
        count
    }

    fn pop(&mut self, output: &mut [u8]) -> usize {
        let count = output.len().min(self.length);
        for (offset, byte) in output[..count].iter_mut().enumerate() {
            *byte = self.bytes[(self.head + offset) % BYTES];
        }
        if BYTES != 0 {
            self.head = (self.head + count) % BYTES;
        }
        self.length -= count;
        count
    }
}

#[derive(Clone, Copy)]
struct SocketSlot<const BACKLOG: usize, const BYTES: usize> {
    generation: u16,
    state: SocketState,
    path: [u8; LOCAL_PATH_MAX],
    path_length: usize,
    peer: Option<usize>,
    peer_closed: bool,
    receive: ByteRing<BYTES>,
    rights: Option<AncillaryRights>,
    pending: [Option<usize>; BACKLOG],
    pending_head: usize,
    pending_length: usize,
}

impl<const BACKLOG: usize, const BYTES: usize> SocketSlot<BACKLOG, BYTES> {
    const EMPTY: Self = Self {
        generation: 0,
        state: SocketState::Free,
        path: [0; LOCAL_PATH_MAX],
        path_length: 0,
        peer: None,
        peer_closed: false,
        receive: ByteRing::EMPTY,
        rights: None,
        pending: [None; BACKLOG],
        pending_head: 0,
        pending_length: 0,
    };

    fn reset_for_allocation(&mut self, generation: u16) {
        *self = Self {
            generation,
            state: SocketState::Created,
            ..Self::EMPTY
        };
    }

    fn handle(&self, index: usize) -> Result<SocketHandle, SocketError> {
        Ok(SocketHandle {
            index: u16::try_from(index).map_err(|_| SocketError::TableFull)?,
            generation: self.generation,
        })
    }
}

/// Fixed-capacity local byte-stream namespace.
///
/// Connections are full duplex. Each endpoint owns its receive ring, so a
/// successful `send` copies bytes into the peer and naturally applies bounded
/// backpressure. Handles carry a generation and cannot alias a reused slot.
pub struct LocalSocketTable<const SOCKETS: usize, const BACKLOG: usize, const BYTES: usize> {
    slots: [SocketSlot<BACKLOG, BYTES>; SOCKETS],
}

impl<const SOCKETS: usize, const BACKLOG: usize, const BYTES: usize>
    LocalSocketTable<SOCKETS, BACKLOG, BYTES>
{
    pub const fn new() -> Self {
        Self {
            slots: [SocketSlot::EMPTY; SOCKETS],
        }
    }

    pub fn socket(&mut self) -> Result<SocketHandle, SocketError> {
        if SOCKETS > usize::from(u16::MAX) || BACKLOG == 0 || BYTES == 0 {
            return Err(SocketError::TableFull);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| slot.state == SocketState::Free)
            .ok_or(SocketError::TableFull)?;
        let generation = self.slots[index]
            .generation
            .checked_add(1)
            .ok_or(SocketError::CounterOverflow)?;
        self.slots[index].reset_for_allocation(generation);
        self.slots[index].handle(index)
    }

    pub fn bind(&mut self, socket: SocketHandle, path: &[u8]) -> Result<(), SocketError> {
        validate_path(path)?;
        if self.slots.iter().any(|slot| {
            matches!(slot.state, SocketState::Bound | SocketState::Listening)
                && slot.path[..slot.path_length] == *path
        }) {
            return Err(SocketError::AddressInUse);
        }
        let slot = self.slot_mut(socket)?;
        if slot.state != SocketState::Created {
            return Err(SocketError::InvalidState);
        }
        slot.path[..path.len()].copy_from_slice(path);
        slot.path_length = path.len();
        slot.state = SocketState::Bound;
        Ok(())
    }

    pub fn listen(&mut self, socket: SocketHandle) -> Result<(), SocketError> {
        let slot = self.slot_mut(socket)?;
        if slot.state != SocketState::Bound {
            return Err(SocketError::InvalidState);
        }
        slot.state = SocketState::Listening;
        Ok(())
    }

    pub fn connect(&mut self, socket: SocketHandle, path: &[u8]) -> Result<(), SocketError> {
        validate_path(path)?;
        let client_index = self.index(socket)?;
        if self.slots[client_index].state != SocketState::Created {
            return Err(SocketError::InvalidState);
        }
        let listener_index = self
            .slots
            .iter()
            .position(|slot| {
                slot.state == SocketState::Listening && slot.path[..slot.path_length] == *path
            })
            .ok_or(SocketError::AddressNotFound)?;
        if self.slots[listener_index].pending_length == BACKLOG {
            return Err(SocketError::BacklogFull);
        }
        let server_index = self
            .slots
            .iter()
            .enumerate()
            .find(|(index, slot)| {
                *index != client_index
                    && *index != listener_index
                    && slot.state == SocketState::Free
            })
            .map(|(index, _)| index)
            .ok_or(SocketError::TableFull)?;
        let generation = self.slots[server_index]
            .generation
            .checked_add(1)
            .ok_or(SocketError::CounterOverflow)?;
        self.slots[server_index].reset_for_allocation(generation);
        self.slots[server_index].state = SocketState::Connected;
        self.slots[server_index].peer = Some(client_index);
        self.slots[client_index].state = SocketState::Connected;
        self.slots[client_index].peer = Some(server_index);

        let listener = &mut self.slots[listener_index];
        let tail = (listener.pending_head + listener.pending_length) % BACKLOG;
        listener.pending[tail] = Some(server_index);
        listener.pending_length += 1;
        Ok(())
    }

    pub fn accept(&mut self, listener: SocketHandle) -> Result<SocketHandle, SocketError> {
        let listener_index = self.index(listener)?;
        let slot = &mut self.slots[listener_index];
        if slot.state != SocketState::Listening {
            return Err(SocketError::InvalidState);
        }
        if slot.pending_length == 0 {
            return Err(SocketError::WouldBlock);
        }
        let server_index = slot.pending[slot.pending_head]
            .take()
            .ok_or(SocketError::InvalidState)?;
        slot.pending_head = (slot.pending_head + 1) % BACKLOG;
        slot.pending_length -= 1;
        self.slots[server_index].handle(server_index)
    }

    pub fn send(&mut self, socket: SocketHandle, input: &[u8]) -> Result<usize, SocketError> {
        let index = self.index(socket)?;
        if self.slots[index].state != SocketState::Connected {
            return Err(SocketError::InvalidState);
        }
        let peer = self.slots[index].peer.ok_or(SocketError::BrokenPipe)?;
        if self.slots[index].peer_closed || self.slots[peer].state != SocketState::Connected {
            return Err(SocketError::BrokenPipe);
        }
        let count = self.slots[peer].receive.push(input);
        if count == 0 && !input.is_empty() {
            return Err(SocketError::WouldBlock);
        }
        Ok(count)
    }

    /// Atomically enqueues one rights object with a complete byte batch.
    ///
    /// A single receive endpoint can hold one not-yet-delivered ancillary
    /// object. Unlike ordinary stream writes, this operation never performs a
    /// partial write because the rights object must remain attached to the
    /// first byte of this batch.
    pub fn send_with_rights(
        &mut self,
        socket: SocketHandle,
        input: &[u8],
        rights: AncillaryRights,
    ) -> Result<usize, SocketError> {
        let index = self.index(socket)?;
        if self.slots[index].state != SocketState::Connected || input.is_empty() {
            return Err(SocketError::InvalidState);
        }
        let peer = self.slots[index].peer.ok_or(SocketError::BrokenPipe)?;
        if self.slots[index].peer_closed || self.slots[peer].state != SocketState::Connected {
            return Err(SocketError::BrokenPipe);
        }
        if self.slots[peer].rights.is_some()
            || self.slots[peer].receive.length != 0
            || input.len() > self.slots[peer].receive.available()
        {
            return Err(SocketError::WouldBlock);
        }
        let count = self.slots[peer].receive.push(input);
        if count != input.len() {
            return Err(SocketError::WouldBlock);
        }
        self.slots[peer].rights = Some(rights);
        Ok(count)
    }

    pub fn recv(&mut self, socket: SocketHandle, output: &mut [u8]) -> Result<usize, SocketError> {
        let slot = self.slot_mut(socket)?;
        if slot.state != SocketState::Connected {
            return Err(SocketError::InvalidState);
        }
        // The transport cannot silently discard an opaque object because its
        // owner must release the corresponding reference. Require the caller
        // to use recv_with_rights for the batch that carries it.
        if slot.rights.is_some() {
            return Err(SocketError::InvalidState);
        }
        let count = slot.receive.pop(output);
        if count == 0 && !output.is_empty() && !slot.peer_closed {
            return Err(SocketError::WouldBlock);
        }
        Ok(count)
    }

    pub fn recv_with_rights(
        &mut self,
        socket: SocketHandle,
        output: &mut [u8],
    ) -> Result<(usize, Option<AncillaryRights>), SocketError> {
        let slot = self.slot_mut(socket)?;
        if slot.state != SocketState::Connected {
            return Err(SocketError::InvalidState);
        }
        let count = slot.receive.pop(output);
        if count == 0 && !output.is_empty() && !slot.peer_closed {
            return Err(SocketError::WouldBlock);
        }
        let rights = (count != 0).then(|| slot.rights.take()).flatten();
        Ok((count, rights))
    }

    pub fn take_rights(
        &mut self,
        socket: SocketHandle,
    ) -> Result<Option<AncillaryRights>, SocketError> {
        Ok(self.slot_mut(socket)?.rights.take())
    }

    pub fn readiness(&self, socket: SocketHandle) -> Result<Readiness, SocketError> {
        let index = self.index(socket)?;
        let slot = &self.slots[index];
        if slot.state != SocketState::Connected {
            return Err(SocketError::InvalidState);
        }
        let writable = slot
            .peer
            .filter(|peer| self.slots[*peer].state == SocketState::Connected)
            .is_some_and(|peer| self.slots[peer].receive.available() != 0)
            && !slot.peer_closed;
        Ok(Readiness {
            readable: slot.receive.length != 0 || slot.peer_closed,
            writable,
            peer_closed: slot.peer_closed,
        })
    }

    pub fn close(&mut self, socket: SocketHandle) -> Result<(), SocketError> {
        let index = self.index(socket)?;
        let peer = self.slots[index].peer;
        if let Some(peer) = peer
            && self.slots[peer].state == SocketState::Connected
        {
            self.slots[peer].peer = None;
            self.slots[peer].peer_closed = true;
        }
        let generation = self.slots[index].generation;
        self.slots[index] = SocketSlot {
            generation,
            ..SocketSlot::EMPTY
        };
        Ok(())
    }

    fn index(&self, handle: SocketHandle) -> Result<usize, SocketError> {
        let index = usize::from(handle.index);
        self.slots
            .get(index)
            .filter(|slot| slot.state != SocketState::Free && slot.generation == handle.generation)
            .map(|_| index)
            .ok_or(SocketError::InvalidHandle)
    }

    fn slot_mut(
        &mut self,
        handle: SocketHandle,
    ) -> Result<&mut SocketSlot<BACKLOG, BYTES>, SocketError> {
        let index = self.index(handle)?;
        Ok(&mut self.slots[index])
    }
}

impl<const SOCKETS: usize, const BACKLOG: usize, const BYTES: usize> Default
    for LocalSocketTable<SOCKETS, BACKLOG, BYTES>
{
    fn default() -> Self {
        Self::new()
    }
}

fn validate_path(path: &[u8]) -> Result<(), SocketError> {
    if path.is_empty() || path.len() > LOCAL_PATH_MAX || path[0] != b'/' || path.contains(&0) {
        return Err(SocketError::InvalidPath);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    type Sockets = LocalSocketTable<8, 2, 8>;

    fn connection(table: &mut Sockets) -> (SocketHandle, SocketHandle, SocketHandle) {
        let listener = table.socket().unwrap();
        table.bind(listener, b"/run/slopos/test.sock").unwrap();
        table.listen(listener).unwrap();
        let client = table.socket().unwrap();
        table.connect(client, b"/run/slopos/test.sock").unwrap();
        let server = table.accept(listener).unwrap();
        (listener, client, server)
    }

    #[test]
    fn connects_accepts_and_transfers_duplex_stream_bytes() {
        let mut table = Sockets::new();
        let (_, client, server) = connection(&mut table);
        assert_eq!(table.send(client, b"request").unwrap(), 7);
        assert!(table.readiness(server).unwrap().readable);
        let mut bytes = [0; 8];
        assert_eq!(table.recv(server, &mut bytes[..3]).unwrap(), 3);
        assert_eq!(&bytes[..3], b"req");
        assert_eq!(table.recv(server, &mut bytes).unwrap(), 4);
        assert_eq!(&bytes[..4], b"uest");
        assert_eq!(table.send(server, b"event").unwrap(), 5);
        assert_eq!(table.recv(client, &mut bytes).unwrap(), 5);
        assert_eq!(&bytes[..5], b"event");
    }

    #[test]
    fn transfers_one_generation_checked_rights_object_with_stream_bytes() {
        let mut table = Sockets::new();
        let (_, client, server) = connection(&mut table);
        let rights = AncillaryRights::new(4, 9);
        assert_eq!(table.send(client, b"prefix"), Ok(6));
        assert_eq!(
            table.send_with_rights(client, b"pool", rights),
            Err(SocketError::WouldBlock)
        );
        let mut bytes = [0; 8];
        assert_eq!(table.recv(server, &mut bytes), Ok(6));
        assert_eq!(&bytes[..6], b"prefix");
        assert_eq!(table.send_with_rights(client, b"pool", rights), Ok(4));
        assert_eq!(
            table.recv(server, &mut bytes),
            Err(SocketError::InvalidState)
        );
        assert_eq!(
            table.send_with_rights(client, b"next", rights),
            Err(SocketError::WouldBlock)
        );
        assert_eq!(
            table.recv_with_rights(server, &mut bytes),
            Ok((4, Some(rights)))
        );
        assert_eq!(&bytes[..4], b"pool");
        assert_eq!(table.take_rights(server), Ok(None));
        assert_eq!(table.send_with_rights(client, b"next", rights), Ok(4));
        assert_eq!(
            table.recv_with_rights(server, &mut bytes),
            Ok((4, Some(rights)))
        );
        let keymap = AncillaryRights::new(7, 11);
        assert_eq!(table.send_with_rights(server, b"keymap", keymap), Ok(6));
        assert_eq!(
            table.recv_with_rights(client, &mut bytes),
            Ok((6, Some(keymap)))
        );
        assert_eq!(&bytes[..6], b"keymap");
    }

    #[test]
    fn enforces_backlog_and_stream_backpressure() {
        let mut table = Sockets::new();
        let listener = table.socket().unwrap();
        table.bind(listener, b"/run/slopos/wayland-0").unwrap();
        table.listen(listener).unwrap();
        let first = table.socket().unwrap();
        let second = table.socket().unwrap();
        let third = table.socket().unwrap();
        table.connect(first, b"/run/slopos/wayland-0").unwrap();
        table.connect(second, b"/run/slopos/wayland-0").unwrap();
        assert_eq!(
            table.connect(third, b"/run/slopos/wayland-0"),
            Err(SocketError::BacklogFull)
        );
        let server = table.accept(listener).unwrap();
        assert_eq!(table.send(first, b"123456789").unwrap(), 8);
        assert_eq!(table.send(first, b"x"), Err(SocketError::WouldBlock));
        assert!(!table.readiness(first).unwrap().writable);
        let mut byte = [0];
        table.recv(server, &mut byte).unwrap();
        assert!(table.readiness(first).unwrap().writable);
    }

    #[test]
    fn reports_eof_and_rejects_stale_handles_after_close() {
        let mut table = Sockets::new();
        let (_, client, server) = connection(&mut table);
        table.send(client, b"tail").unwrap();
        table.close(client).unwrap();
        let mut bytes = [0; 8];
        assert_eq!(table.recv(server, &mut bytes).unwrap(), 4);
        assert_eq!(table.recv(server, &mut bytes).unwrap(), 0);
        assert_eq!(table.send(server, b"late"), Err(SocketError::BrokenPipe));
        assert_eq!(table.readiness(client), Err(SocketError::InvalidHandle));
        let replacement = table.socket().unwrap();
        assert_eq!(replacement.index(), client.index());
        assert_ne!(replacement.generation(), client.generation());
    }

    #[test]
    fn rejects_invalid_paths_and_duplicate_bindings() {
        let mut table = Sockets::new();
        let first = table.socket().unwrap();
        let second = table.socket().unwrap();
        assert_eq!(
            table.bind(first, b"relative"),
            Err(SocketError::InvalidPath)
        );
        table.bind(first, b"/run/slopos/control").unwrap();
        assert_eq!(
            table.bind(second, b"/run/slopos/control"),
            Err(SocketError::AddressInUse)
        );
        assert_eq!(
            table.connect(second, b"/run/slopos/missing"),
            Err(SocketError::AddressNotFound)
        );
    }
}
