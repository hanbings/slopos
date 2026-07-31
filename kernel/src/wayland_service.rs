// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use slopos_desktop_protocol::{
    WAYLAND_SURFACE_MAX_PIXEL_SIZE, WAYLAND_SURFACE_MAX_SIZE, WaylandSurfaceCommit,
};
use slopos_wayland::{CommittedSurface, SingleSurfaceSession};

const DESKTOP_SERVICE_PID: u32 = 2;
const SURFACE_BANKS: usize = 2;
const NO_BANK: usize = usize::MAX;

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

#[derive(Clone, Copy)]
pub struct WaylandSurfaceSnapshot {
    pub generation: u64,
    pub owner_pid: u32,
    pub metadata: CommittedSurface,
    pub pixels: &'static [u8],
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
) -> Result<(u64, CommittedSurface), WaylandServiceError> {
    if pid != DESKTOP_SERVICE_PID {
        return Err(WaylandServiceError::PermissionDenied);
    }
    commit
        .validate()
        .map_err(|_| WaylandServiceError::InvalidProtocol)?;
    let generation = GENERATION.load(Ordering::Acquire);
    if generation != ACKNOWLEDGED_GENERATION.load(Ordering::Acquire) {
        return Err(WaylandServiceError::CommitPending);
    }
    let mut session =
        SingleSurfaceSession::<16>::new().map_err(|_| WaylandServiceError::InvalidProtocol)?;
    let metadata = session
        .accept(
            commit.wire,
            commit.header.file_descriptor,
            commit.pixels.len(),
        )
        .map_err(|_| WaylandServiceError::InvalidProtocol)?;

    let active = ACTIVE_BANK.load(Ordering::Acquire);
    let bank_index = if active == 0 { 1 } else { 0 };
    // SAFETY: no commit is pending, PID 2 is the only writer, and the selected
    // bank is not the renderer-pinned active bank.
    let bank = unsafe { &mut (*BANKS.0.get())[bank_index] };
    bank.metadata = Some(metadata);
    bank.pixel_length = commit.pixels.len();
    bank.pixels[..bank.pixel_length].copy_from_slice(commit.pixels);
    bank.pixels[bank.pixel_length..].fill(0);

    let next_generation = generation
        .checked_add(1)
        .ok_or(WaylandServiceError::CommitPending)?;
    PUBLISHED_BANK.store(bank_index, Ordering::Relaxed);
    GENERATION.store(next_generation, Ordering::Release);
    crate::executor::wake_task(crate::executor::INPUT_TASK);
    crate::serial::serialln(format_args!(
        "SLOPOS-WAYLAND-SERVER: commit accepted pid={pid} generation={next_generation} transport=syscall-bootstrap-v1 objects=registry/compositor/shm/xdg_toplevel surface={} buffer={} callback={} geometry={}x{} stride={} format={} title=\"{}\" app_id={} wire_bytes={} pixel_bytes={}",
        metadata.surface,
        metadata.buffer,
        metadata.frame_callback,
        metadata.width,
        metadata.height,
        metadata.stride,
        metadata.format,
        metadata.title.as_str(),
        metadata.app_id.as_str(),
        commit.wire.len(),
        commit.pixels.len()
    ));
    Ok((next_generation, metadata))
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
    crate::serial::serialln(format_args!(
        "SLOPOS-WAYLAND-SERVER: commit acknowledged generation={generation} renderer=desktop active_bank={bank}"
    ));
}
