// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use slopos_desktop_protocol::WAYLAND_EVENT_CONFIGURE;
use slopos_ipc::{AncillaryRights, LocalSocketTable, SocketError, SocketHandle};
use slopos_wayland::SLOPOS_XKB_KEYMAP_TEXT;

const SOCKET_CAPACITY: usize = 8;
const SOCKET_BACKLOG: usize = 2;
const SOCKET_BYTES: usize = 512;
const CONNECTION_CAPACITY: usize = 3;
pub const WAYLAND_SOCKET_PATH: &[u8] = b"/run/slopos/wayland-0";

type SocketTable = LocalSocketTable<SOCKET_CAPACITY, SOCKET_BACKLOG, SOCKET_BYTES>;

#[derive(Clone, Copy)]
struct Connection {
    pid: u32,
    client: SocketHandle,
    server: SocketHandle,
    event_sequence: u64,
    backing: Option<crate::shared_memory_service::SharedMemoryHandle>,
    keymap: Option<crate::shared_memory_service::SharedMemoryHandle>,
}

struct ServiceState {
    table: SocketTable,
    listener: Option<SocketHandle>,
    connections: [Option<Connection>; CONNECTION_CAPACITY],
}

impl ServiceState {
    const fn new() -> Self {
        Self {
            table: SocketTable::new(),
            listener: None,
            connections: [None; CONNECTION_CAPACITY],
        }
    }

    fn connection_index(&self, pid: u32, client: SocketHandle) -> Option<usize> {
        self.connections.iter().position(|connection| {
            connection
                .is_some_and(|connection| connection.pid == pid && connection.client == client)
        })
    }
}

struct SharedState(UnsafeCell<ServiceState>);

// SAFETY: SlopOS currently runs the syscall and block-task completion paths on
// one bootstrap processor. Syscall entry masks interrupts, and an async socket
// receive never keeps a mutable borrow across its await point.
unsafe impl Sync for SharedState {}

static STATE: SharedState = SharedState(UnsafeCell::new(ServiceState::new()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalSocketServiceError {
    Socket(SocketError),
    PermissionDenied,
    WaylandProtocol,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SocketReceive {
    pub length: usize,
    pub rights: Option<crate::shared_memory_service::SharedMemoryHandle>,
}

impl From<SocketError> for LocalSocketServiceError {
    fn from(error: SocketError) -> Self {
        Self::Socket(error)
    }
}

fn state_mut() -> &'static mut ServiceState {
    // SAFETY: justified by SharedState's single-processor ownership contract.
    unsafe { &mut *STATE.0.get() }
}

pub fn initialize() {
    let state = state_mut();
    if state.listener.is_some() {
        return;
    }
    let listener = state
        .table
        .socket()
        .unwrap_or_else(|_| crate::fatal("Wayland local socket allocation failed"));
    state
        .table
        .bind(listener, WAYLAND_SOCKET_PATH)
        .unwrap_or_else(|_| crate::fatal("Wayland local socket bind failed"));
    state
        .table
        .listen(listener)
        .unwrap_or_else(|_| crate::fatal("Wayland local socket listen failed"));
    state.listener = Some(listener);
    crate::serial::serialln(format_args!(
        "SLOPOS-IPC: AF_UNIX SOCK_STREAM listener bound path=/run/slopos/wayland-0 backlog={SOCKET_BACKLOG} bytes_per_direction={SOCKET_BYTES}"
    ));
}

pub fn socket() -> Result<SocketHandle, LocalSocketServiceError> {
    initialize();
    Ok(state_mut().table.socket()?)
}

pub fn connect(pid: u32, client: SocketHandle, path: &[u8]) -> Result<(), LocalSocketServiceError> {
    if pid != 2 || path != WAYLAND_SOCKET_PATH {
        return Err(LocalSocketServiceError::PermissionDenied);
    }
    initialize();
    let state = state_mut();
    if state.connection_index(pid, client).is_some() {
        return Err(SocketError::InvalidState.into());
    }
    let slot = state
        .connections
        .iter()
        .position(Option::is_none)
        .ok_or(SocketError::TableFull)?;
    state.table.connect(client, path)?;
    let listener = state.listener.ok_or(SocketError::InvalidState)?;
    let server = state.table.accept(listener)?;
    state.connections[slot] = Some(Connection {
        pid,
        client,
        server,
        event_sequence: 0,
        backing: None,
        keymap: None,
    });
    crate::serial::serialln(format_args!(
        "SLOPOS-IPC: AF_UNIX connected pid={pid} path=/run/slopos/wayland-0 client={}:{} server={}:{} transport=bounded-stream",
        client.index(),
        client.generation(),
        server.index(),
        server.generation()
    ));
    Ok(())
}

pub fn send(
    pid: u32,
    client: SocketHandle,
    input: &[u8],
) -> Result<usize, LocalSocketServiceError> {
    let state = state_mut();
    if state.connection_index(pid, client).is_none() {
        return Err(LocalSocketServiceError::PermissionDenied);
    }
    Ok(state.table.send(client, input)?)
}

pub fn send_with_rights(
    pid: u32,
    client: SocketHandle,
    input: &[u8],
    shared: crate::shared_memory_service::SharedMemoryHandle,
) -> Result<usize, LocalSocketServiceError> {
    let state = state_mut();
    if state.connection_index(pid, client).is_none() {
        return Err(LocalSocketServiceError::PermissionDenied);
    }
    crate::shared_memory_service::retain(shared)
        .map_err(|_| LocalSocketServiceError::PermissionDenied)?;
    let rights = AncillaryRights::new(shared.index(), shared.generation());
    match state.table.send_with_rights(client, input, rights) {
        Ok(bytes) => Ok(bytes),
        Err(error) => {
            let _ = crate::shared_memory_service::release(shared);
            Err(error.into())
        }
    }
}

pub async fn recv(
    pid: u32,
    client: SocketHandle,
    output: &mut [u8],
) -> Result<usize, LocalSocketServiceError> {
    let received = receive(pid, client, output, false).await?;
    if received.rights.is_some() {
        return Err(LocalSocketServiceError::WaylandProtocol);
    }
    Ok(received.length)
}

pub async fn recv_with_rights(
    pid: u32,
    client: SocketHandle,
    output: &mut [u8],
) -> Result<SocketReceive, LocalSocketServiceError> {
    receive(pid, client, output, true).await
}

async fn receive(
    pid: u32,
    client: SocketHandle,
    output: &mut [u8],
    receive_rights: bool,
) -> Result<SocketReceive, LocalSocketServiceError> {
    let after_sequence = {
        let state = state_mut();
        let index = state
            .connection_index(pid, client)
            .ok_or(LocalSocketServiceError::PermissionDenied)?;
        let connection = state.connections[index].ok_or(SocketError::InvalidState)?;
        let readiness = state.table.readiness(client)?;
        if readiness.readable {
            return pop_client(state, client, output, receive_rights);
        }

        let mut request = [0u8; SOCKET_BYTES];
        let (request_length, rights) = match state
            .table
            .recv_with_rights(connection.server, &mut request)
        {
            Ok(received) => received,
            Err(SocketError::WouldBlock) => (0, None),
            Err(error) => return Err(error.into()),
        };
        if request_length != 0 {
            let mut descriptor_received = false;
            if let Some(rights) = rights {
                if connection.backing.is_some() {
                    let handle = crate::shared_memory_service::SharedMemoryHandle::from_parts(
                        rights.object(),
                        rights.generation(),
                    );
                    let _ = crate::shared_memory_service::release(handle);
                    return Err(LocalSocketServiceError::WaylandProtocol);
                }
                let handle = crate::shared_memory_service::SharedMemoryHandle::from_parts(
                    rights.object(),
                    rights.generation(),
                );
                crate::shared_memory_service::frame_and_length(handle)
                    .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
                state.connections[index]
                    .as_mut()
                    .ok_or(SocketError::InvalidState)?
                    .backing = Some(handle);
                descriptor_received = true;
            }
            let backing = state.connections[index]
                .ok_or(SocketError::InvalidState)?
                .backing;
            let pixels = match backing {
                Some(handle) => crate::shared_memory_service::bytes(handle)
                    .map_err(|_| LocalSocketServiceError::WaylandProtocol)?,
                None => &[],
            };
            crate::wayland_service::submit_wire(
                pid,
                &request[..request_length],
                descriptor_received,
                pixels,
            )
            .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
            crate::serial::serialln(format_args!(
                "SLOPOS-WAYLAND-SERVER: request received pid={pid} transport=AF_UNIX/SOCK_STREAM path=/run/slopos/wayland-0 wire_bytes={request_length} scm_rights={descriptor_received}"
            ));
        }
        connection.event_sequence
    };

    let event = crate::wayland_service::next_event(after_sequence).await;
    let received = {
        let state = state_mut();
        let index = state
            .connection_index(pid, client)
            .ok_or(LocalSocketServiceError::PermissionDenied)?;
        let connection = state.connections[index].ok_or(SocketError::InvalidState)?;
        let carries_keymap = event.header.kind == WAYLAND_EVENT_CONFIGURE;
        let sent = if carries_keymap {
            let keymap = if let Some(keymap) = connection.keymap {
                keymap
            } else {
                let keymap = crate::shared_memory_service::create_initialized(
                    0,
                    SLOPOS_XKB_KEYMAP_TEXT,
                    true,
                )
                .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
                state.connections[index]
                    .as_mut()
                    .ok_or(SocketError::InvalidState)?
                    .keymap = Some(keymap);
                keymap
            };
            crate::shared_memory_service::retain(keymap)
                .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
            let rights = AncillaryRights::new(keymap.index(), keymap.generation());
            match state
                .table
                .send_with_rights(connection.server, event.wire, rights)
            {
                Ok(sent) => sent,
                Err(error) => {
                    let _ = crate::shared_memory_service::release(keymap);
                    return Err(error.into());
                }
            }
        } else {
            state.table.send(connection.server, event.wire)?
        };
        if sent != event.wire.len() {
            return Err(SocketError::WouldBlock.into());
        }
        state.connections[index]
            .as_mut()
            .ok_or(SocketError::InvalidState)?
            .event_sequence = event.header.sequence;
        let received = pop_client(state, client, output, receive_rights)?;
        crate::serial::serialln(format_args!(
            "SLOPOS-WAYLAND-SERVER: event sent pid={pid} transport=AF_UNIX/SOCK_STREAM kind={} sequence={} wire_bytes={} read_bytes={} scm_rights={carries_keymap}",
            event.header.kind,
            event.header.sequence,
            event.wire.len(),
            received.length
        ));
        received
    };
    crate::wayland_service::acknowledge_event(event.header.sequence);
    Ok(received)
}

fn pop_client(
    state: &mut ServiceState,
    client: SocketHandle,
    output: &mut [u8],
    receive_rights: bool,
) -> Result<SocketReceive, LocalSocketServiceError> {
    if !receive_rights {
        return Ok(SocketReceive {
            length: state.table.recv(client, output)?,
            rights: None,
        });
    }
    let (length, rights) = state.table.recv_with_rights(client, output)?;
    let rights = rights.map(|rights| {
        crate::shared_memory_service::SharedMemoryHandle::from_parts(
            rights.object(),
            rights.generation(),
        )
    });
    if let Some(handle) = rights {
        crate::shared_memory_service::frame_and_length(handle)
            .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
    }
    Ok(SocketReceive { length, rights })
}

pub fn close(pid: u32, client: SocketHandle) -> Result<(), LocalSocketServiceError> {
    let state = state_mut();
    if let Some(index) = state.connection_index(pid, client) {
        let connection = state.connections[index]
            .take()
            .ok_or(SocketError::InvalidState)?;
        for endpoint in [connection.server, connection.client] {
            if let Some(rights) = state.table.take_rights(endpoint)? {
                let handle = crate::shared_memory_service::SharedMemoryHandle::from_parts(
                    rights.object(),
                    rights.generation(),
                );
                crate::shared_memory_service::release(handle)
                    .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
            }
        }
        if let Some(handle) = connection.backing {
            crate::shared_memory_service::release(handle)
                .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
        }
        if let Some(handle) = connection.keymap {
            crate::shared_memory_service::release(handle)
                .map_err(|_| LocalSocketServiceError::WaylandProtocol)?;
        }
        state.table.close(connection.client)?;
        state.table.close(connection.server)?;
        return Ok(());
    }
    state.table.close(client)?;
    Ok(())
}
