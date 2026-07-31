// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use core::task::{Context, Poll};
use slopos_desktop_protocol::{
    WAYLAND_EVENT_CONFIGURE, WAYLAND_EVENT_MAX_WIRE_SIZE, WAYLAND_EVENT_PRESENTED,
    WAYLAND_EVENT_REGISTRY, WAYLAND_SURFACE_MAX_PIXEL_SIZE, WAYLAND_SURFACE_MAX_SIZE,
    WaylandServerEvent, WaylandSurfaceCommit,
};
use slopos_wayland::{
    CORE_GLOBALS, CommittedSurface, SingleSurfaceSession, SurfaceSessionEvent,
    encode_buffer_release, encode_callback_done, encode_display_delete_id,
    encode_output_description, encode_output_done, encode_output_geometry, encode_output_mode,
    encode_output_name, encode_output_scale, encode_pointer_enter, encode_registry_global,
    encode_seat_capabilities, encode_seat_name, encode_shm_format, encode_xdg_surface_configure,
    encode_xdg_toplevel_configure,
};

const DESKTOP_SERVICE_PID: u32 = 2;
const SURFACE_BANKS: usize = 2;
const NO_BANK: usize = usize::MAX;
const CONFIGURE_SERIAL: u32 = 1;
const POINTER_ENTER_SERIAL: u32 = 2;
const SERVER_SHM_FD: i32 = 1;

struct SurfaceBank {
    metadata: Option<CommittedSurface>,
    pixels: [u8; WAYLAND_SURFACE_MAX_PIXEL_SIZE],
    pixel_length: usize,
}

impl SurfaceBank {
    const fn empty() -> Self {
        Self {
            metadata: None,
            pixels: [0; WAYLAND_SURFACE_MAX_PIXEL_SIZE],
            pixel_length: 0,
        }
    }
}

struct SharedBanks(UnsafeCell<[SurfaceBank; SURFACE_BANKS]>);

// SAFETY: PID 2 is the sole producer. Release/acquire publication and
// acknowledgement keep the selected bank immutable while the desktop renderer
// holds its static pixel slice.
unsafe impl Sync for SharedBanks {}

static BANKS: SharedBanks = SharedBanks(UnsafeCell::new([
    SurfaceBank::empty(),
    SurfaceBank::empty(),
]));
static PUBLISHED_BANK: AtomicUsize = AtomicUsize::new(NO_BANK);
static ACTIVE_BANK: AtomicUsize = AtomicUsize::new(NO_BANK);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static ACKNOWLEDGED_GENERATION: AtomicU64 = AtomicU64::new(0);

struct SharedStaging(UnsafeCell<[u8; WAYLAND_SURFACE_MAX_SIZE]>);

// SAFETY: syscall entry is IF-masked on SlopOS's single bootstrap processor,
// and only PID 2 is authorized to use the staging area.
unsafe impl Sync for SharedStaging {}

static STAGING: SharedStaging = SharedStaging(UnsafeCell::new([0; WAYLAND_SURFACE_MAX_SIZE]));

struct SharedSession(UnsafeCell<Option<SingleSurfaceSession<16>>>);

// SAFETY: the IF-masked PID 2 syscall path and desktop task run serially on the
// bootstrap processor. Each request is applied to a clone before atomic swap.
unsafe impl Sync for SharedSession {}

static SESSION: SharedSession = SharedSession(UnsafeCell::new(None));

struct EventBank {
    kind: u16,
    wire: [u8; WAYLAND_EVENT_MAX_WIRE_SIZE],
    wire_length: usize,
}

struct SharedEvent(UnsafeCell<EventBank>);

// SAFETY: the producer fully writes this single-consumer bank before publishing
// EVENT_SEQUENCE with Release; it cannot be reused until EVENT_ACK catches up.
unsafe impl Sync for SharedEvent {}

static EVENT: SharedEvent = SharedEvent(UnsafeCell::new(EventBank {
    kind: 0,
    wire: [0; WAYLAND_EVENT_MAX_WIRE_SIZE],
    wire_length: 0,
}));
static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static EVENT_ACK: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
pub struct WaylandSurfaceSnapshot {
    pub generation: u64,
    pub owner_pid: u32,
    pub metadata: CommittedSurface,
    pub pixels: &'static [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaylandSubmission {
    Registry {
        event_sequence: u64,
    },
    Configure {
        event_sequence: u64,
        serial: u32,
    },
    Surface {
        generation: u64,
        metadata: CommittedSurface,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaylandServiceError {
    PermissionDenied,
    InvalidProtocol,
    CommitPending,
}

/// Returns the private syscall staging buffer.
///
/// # Safety
///
/// The caller must be the IF-masked syscall path for PID 2 and must finish
/// decoding/submitting before returning to user mode.
pub unsafe fn staging_buffer() -> &'static mut [u8; WAYLAND_SURFACE_MAX_SIZE] {
    // SAFETY: guaranteed by the caller contract and single-core execution.
    unsafe { &mut *STAGING.0.get() }
}

pub fn submit(
    pid: u32,
    commit: WaylandSurfaceCommit<'_>,
) -> Result<WaylandSubmission, WaylandServiceError> {
    commit
        .validate()
        .map_err(|_| WaylandServiceError::InvalidProtocol)?;
    submit_parts(pid, commit.wire, !commit.pixels.is_empty(), commit.pixels)
}

pub fn submit_wire(
    pid: u32,
    wire: &[u8],
    descriptor_received: bool,
    pixels: &[u8],
) -> Result<WaylandSubmission, WaylandServiceError> {
    submit_parts(pid, wire, descriptor_received, pixels)
}

fn submit_parts(
    pid: u32,
    wire: &[u8],
    descriptor_received: bool,
    pixels: &[u8],
) -> Result<WaylandSubmission, WaylandServiceError> {
    if pid != DESKTOP_SERVICE_PID {
        return Err(WaylandServiceError::PermissionDenied);
    }
    if wire.is_empty()
        || wire.len() > slopos_desktop_protocol::WAYLAND_SURFACE_MAX_WIRE_SIZE
        || wire.len() % 4 != 0
        || pixels.len() > WAYLAND_SURFACE_MAX_PIXEL_SIZE
        || pixels.len() % 4 != 0
    {
        return Err(WaylandServiceError::InvalidProtocol);
    }
    if EVENT_SEQUENCE.load(Ordering::Acquire) != EVENT_ACK.load(Ordering::Acquire) {
        return Err(WaylandServiceError::CommitPending);
    }

    // SAFETY: serialized by the IF-masked PID 2 syscall path.
    let current = unsafe { &mut *SESSION.0.get() };
    let mut candidate = match current.as_ref() {
        Some(session) => session.clone(),
        None => SingleSurfaceSession::new(CONFIGURE_SERIAL)
            .map_err(|_| WaylandServiceError::InvalidProtocol)?,
    };
    let descriptor = descriptor_received.then_some(SERVER_SHM_FD);
    let progress = candidate
        .accept_batch(wire, descriptor, pixels.len())
        .map_err(|_| WaylandServiceError::InvalidProtocol)?;

    let submission = match progress {
        SurfaceSessionEvent::Registry { registry } => {
            let mut wire = [0; WAYLAND_EVENT_MAX_WIRE_SIZE];
            let mut cursor = 0;
            for global in CORE_GLOBALS {
                let length = encode_registry_global(&mut wire[cursor..], registry, global)
                    .map_err(|_| WaylandServiceError::InvalidProtocol)?
                    .len();
                cursor += length;
            }
            let sequence = publish_event(WAYLAND_EVENT_REGISTRY, &wire[..cursor])?;
            crate::serial::serialln(format_args!(
                "SLOPOS-WAYLAND-SERVER: registry advertised pid={pid} sequence={sequence} registry={registry} globals=wl_compositor/wl_shm/wl_seat/wl_output/xdg_wm_base wire_bytes={cursor}"
            ));
            WaylandSubmission::Registry {
                event_sequence: sequence,
            }
        }
        SurfaceSessionEvent::Configure {
            shm,
            seat,
            pointer,
            output,
            xdg_surface,
            toplevel,
            serial,
        } => {
            let mut wire = [0; WAYLAND_EVENT_MAX_WIRE_SIZE];
            let mut cursor = 0;
            cursor += encode_seat_capabilities(&mut wire[cursor..], seat, 1)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_seat_name(&mut wire[cursor..], seat, "seat0")
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_output_geometry(
                &mut wire[cursor..],
                output,
                0,
                0,
                270,
                203,
                0,
                "SlopOS",
                "Virtual Display",
                0,
            )
            .map_err(|_| WaylandServiceError::InvalidProtocol)?
            .len();
            cursor += encode_output_mode(&mut wire[cursor..], output, 3, 1024, 768, 60_000)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_output_scale(&mut wire[cursor..], output, 1)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_output_name(&mut wire[cursor..], output, "SLOPOS-1")
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor +=
                encode_output_description(&mut wire[cursor..], output, "SlopOS Virtual Output")
                    .map_err(|_| WaylandServiceError::InvalidProtocol)?
                    .len();
            cursor += encode_output_done(&mut wire[cursor..], output)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_shm_format(&mut wire[cursor..], shm, 0)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_shm_format(&mut wire[cursor..], shm, 1)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_xdg_toplevel_configure(&mut wire[cursor..], toplevel, 32, 24, &[])
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            cursor += encode_xdg_surface_configure(&mut wire[cursor..], xdg_surface, serial)
                .map_err(|_| WaylandServiceError::InvalidProtocol)?
                .len();
            let sequence = publish_event(WAYLAND_EVENT_CONFIGURE, &wire[..cursor])?;
            crate::serial::serialln(format_args!(
                "SLOPOS-WAYLAND-SERVER: configure emitted pid={pid} sequence={sequence} serial={serial} seat={seat} capabilities=pointer pointer={pointer} output={output} output_name=SLOPOS-1 mode=1024x768@60000 scale=1 shm={shm} formats=argb8888/xrgb8888 xdg_surface={xdg_surface} toplevel={toplevel} geometry=32x24 states=empty wire_bytes={cursor}"
            ));
            WaylandSubmission::Configure {
                event_sequence: sequence,
                serial,
            }
        }
        SurfaceSessionEvent::Committed(metadata) => {
            let generation = publish_surface(pid, wire, pixels, metadata)?;
            WaylandSubmission::Surface {
                generation,
                metadata,
            }
        }
    };
    *current = Some(candidate);
    Ok(submission)
}

fn publish_surface(
    pid: u32,
    wire: &[u8],
    pixels: &[u8],
    metadata: CommittedSurface,
) -> Result<u64, WaylandServiceError> {
    let generation = GENERATION.load(Ordering::Acquire);
    if generation != ACKNOWLEDGED_GENERATION.load(Ordering::Acquire) {
        return Err(WaylandServiceError::CommitPending);
    }
    let active = ACTIVE_BANK.load(Ordering::Acquire);
    let bank_index = if active == 0 { 1 } else { 0 };
    // SAFETY: no commit is pending, PID 2 is the only writer, and the selected
    // bank is not the renderer-pinned active bank.
    let bank = unsafe { &mut (*BANKS.0.get())[bank_index] };
    bank.metadata = Some(metadata);
    bank.pixel_length = pixels.len();
    bank.pixels[..bank.pixel_length].copy_from_slice(pixels);
    bank.pixels[bank.pixel_length..].fill(0);

    let next_generation = generation
        .checked_add(1)
        .ok_or(WaylandServiceError::CommitPending)?;
    let lifecycle = if generation == 0 {
        "registry/configure/ack-configure"
    } else {
        "configured-buffer-reuse"
    };
    PUBLISHED_BANK.store(bank_index, Ordering::Relaxed);
    GENERATION.store(next_generation, Ordering::Release);
    crate::executor::wake_task(crate::executor::INPUT_TASK);
    crate::serial::serialln(format_args!(
        "SLOPOS-WAYLAND-SERVER: commit accepted pid={pid} generation={next_generation} transport=AF_UNIX/SOCK_STREAM backing=SCM_RIGHTS/mmap-shared-v1 lifecycle={lifecycle} objects=registry/compositor/shm/seat/pointer/output/xdg_toplevel surface={} buffer={} callback={} seat={} pointer={} output={} geometry={}x{} stride={} format={} title=\"{}\" app_id={} wire_bytes={} pixel_bytes={}",
        metadata.surface,
        metadata.buffer,
        metadata.frame_callback,
        metadata.seat,
        metadata.pointer,
        metadata.output,
        metadata.width,
        metadata.height,
        metadata.stride,
        metadata.format,
        metadata.title.as_str(),
        metadata.app_id.as_str(),
        wire.len(),
        pixels.len()
    ));
    Ok(next_generation)
}

fn publish_event(kind: u16, wire: &[u8]) -> Result<u64, WaylandServiceError> {
    let sequence = EVENT_SEQUENCE.load(Ordering::Acquire);
    if sequence != EVENT_ACK.load(Ordering::Acquire)
        || wire.is_empty()
        || wire.len() > WAYLAND_EVENT_MAX_WIRE_SIZE
    {
        return Err(WaylandServiceError::CommitPending);
    }
    let next = sequence
        .checked_add(1)
        .ok_or(WaylandServiceError::CommitPending)?;
    // SAFETY: the event bank is idle and this producer is serialized.
    let event = unsafe { &mut *EVENT.0.get() };
    event.kind = kind;
    event.wire_length = wire.len();
    event.wire[..wire.len()].copy_from_slice(wire);
    event.wire[wire.len()..].fill(0);
    EVENT_SEQUENCE.store(next, Ordering::Release);
    crate::executor::wake_task(crate::executor::BLOCK_TASK);
    Ok(next)
}

pub fn event_after(after_sequence: u64) -> Option<WaylandServerEvent<'static>> {
    let sequence = EVENT_SEQUENCE.load(Ordering::Acquire);
    if sequence == 0 || sequence <= after_sequence {
        return None;
    }
    // SAFETY: acquiring EVENT_SEQUENCE observes the complete event bank, which
    // remains immutable until the event is copied and acknowledged.
    let event = unsafe { &*EVENT.0.get() };
    WaylandServerEvent::new(event.kind, sequence, &event.wire[..event.wire_length]).ok()
}

pub fn acknowledge_event(sequence: u64) {
    if sequence == 0
        || sequence != EVENT_SEQUENCE.load(Ordering::Acquire)
        || EVENT_ACK
            .compare_exchange(sequence - 1, sequence, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        crate::fatal("Wayland server event acknowledgement is invalid");
    }
}

pub async fn next_event(after_sequence: u64) -> WaylandServerEvent<'static> {
    ServerEvent { after_sequence }.await
}

struct ServerEvent {
    after_sequence: u64,
}

impl Future for ServerEvent {
    type Output = WaylandServerEvent<'static>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        event_after(self.after_sequence).map_or(Poll::Pending, Poll::Ready)
    }
}

pub fn latest_after(generation: u64) -> Option<WaylandSurfaceSnapshot> {
    let current = GENERATION.load(Ordering::Acquire);
    if current == 0 || current == generation {
        return None;
    }
    let bank_index = PUBLISHED_BANK.load(Ordering::Relaxed);
    if bank_index >= SURFACE_BANKS {
        return None;
    }
    // SAFETY: acquire of GENERATION observes the completely initialized bank;
    // it remains immutable until the renderer acknowledges this generation.
    let bank = unsafe { &(*BANKS.0.get())[bank_index] };
    Some(WaylandSurfaceSnapshot {
        generation: current,
        owner_pid: DESKTOP_SERVICE_PID,
        metadata: bank.metadata?,
        pixels: &bank.pixels[..bank.pixel_length],
    })
}

pub fn acknowledge(generation: u64) {
    let published = GENERATION.load(Ordering::Acquire);
    let bank = PUBLISHED_BANK.load(Ordering::Relaxed);
    if generation == 0
        || generation != published
        || bank >= SURFACE_BANKS
        || ACKNOWLEDGED_GENERATION
            .compare_exchange(
                generation - 1,
                generation,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
    {
        crate::fatal("Wayland surface acknowledgement is invalid");
    }
    ACTIVE_BANK.store(bank, Ordering::Release);

    // SAFETY: the desktop task is serialized with PID 2 and the session is not
    // accessed by another producer while the client waits for presentation.
    let current = unsafe { &mut *SESSION.0.get() };
    let mut candidate = current
        .as_ref()
        .unwrap_or_else(|| crate::fatal("Wayland presentation has no session"))
        .clone();
    let presented = candidate
        .present()
        .unwrap_or_else(|_| crate::fatal("Wayland presentation lifecycle is invalid"));
    let mut wire = [0; WAYLAND_EVENT_MAX_WIRE_SIZE];
    let mut cursor = 0;
    if generation == 1 {
        cursor += encode_pointer_enter(
            &mut wire[cursor..],
            presented.pointer,
            POINTER_ENTER_SERIAL,
            presented.surface,
            16 << 8,
            12 << 8,
        )
        .unwrap_or_else(|_| crate::fatal("Wayland pointer enter encoding failed"))
        .len();
    }
    cursor += encode_buffer_release(&mut wire[cursor..], presented.buffer)
        .unwrap_or_else(|_| crate::fatal("Wayland buffer release encoding failed"))
        .len();
    cursor += encode_callback_done(
        &mut wire[cursor..],
        presented.frame_callback,
        generation as u32,
    )
    .unwrap_or_else(|_| crate::fatal("Wayland frame callback encoding failed"))
    .len();
    cursor += encode_display_delete_id(&mut wire[cursor..], presented.frame_callback)
        .unwrap_or_else(|_| crate::fatal("Wayland delete-id encoding failed"))
        .len();
    let event_sequence = publish_event(WAYLAND_EVENT_PRESENTED, &wire[..cursor])
        .unwrap_or_else(|_| crate::fatal("Wayland presentation event remained pending"));
    *current = Some(candidate);
    let events = if generation == 1 {
        "wl_pointer.enter/wl_buffer.release/wl_callback.done/wl_display.delete_id"
    } else {
        "wl_buffer.release/wl_callback.done/wl_display.delete_id"
    };
    crate::serial::serialln(format_args!(
        "SLOPOS-WAYLAND-SERVER: commit acknowledged generation={generation} renderer=desktop active_bank={bank} event_sequence={event_sequence} events={events} callback_data={generation}"
    ));
}
