// SPDX-License-Identifier: 0BSD

use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};
use slopos_desktop_protocol::{
    CAPABILITY_SWWW_POLICY, CAPABILITY_WAYBAR_PROVIDER, DesktopCommit, DesktopServiceEvent,
    EVENT_CONFIG_APPLIED, EVENT_POLICY_APPLIED, WALLPAPER_AURORA, config_hash,
};

const DESKTOP_SERVICE_PID: u32 = 2;
const EXPECTED_WAYBAR_HASH: u64 = config_hash(include_bytes!("../../assets/waybar-config.jsonc"));
const EXPECTED_SWWW_HASH: u64 = config_hash(include_bytes!("../../assets/swww.env"));

static STATE: AtomicU64 = AtomicU64::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static POLICY_APPLIED_GENERATION: AtomicU64 = AtomicU64::new(0);
static CONFIG_APPLIED_GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DesktopServiceSnapshot {
    pub generation: u64,
    pub owner_pid: u32,
    pub capabilities: u32,
    pub cpu_usage: u8,
    pub memory_percentage: u8,
    pub wallpaper: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DesktopServiceError {
    PermissionDenied,
    InvalidProtocol,
    ConfigMismatch,
    CommitPending,
}

pub fn submit(pid: u32, commit: DesktopCommit) -> Result<u64, DesktopServiceError> {
    if pid != DESKTOP_SERVICE_PID {
        return Err(DesktopServiceError::PermissionDenied);
    }
    commit
        .validate()
        .map_err(|_| DesktopServiceError::InvalidProtocol)?;
    if commit.waybar_hash != EXPECTED_WAYBAR_HASH || commit.swww_hash != EXPECTED_SWWW_HASH {
        return Err(DesktopServiceError::ConfigMismatch);
    }
    let generation = GENERATION.load(Ordering::Acquire);
    if generation != POLICY_APPLIED_GENERATION.load(Ordering::Acquire) {
        return Err(DesktopServiceError::CommitPending);
    }
    let next_generation = generation
        .checked_add(1)
        .ok_or(DesktopServiceError::CommitPending)?;
    let state = u64::from(pid)
        | (u64::from(commit.capabilities) << 32)
        | (u64::from(commit.cpu_usage) << 40)
        | (u64::from(commit.memory_percentage) << 48)
        | (u64::from(commit.wallpaper) << 56);
    STATE.store(state, Ordering::Relaxed);
    GENERATION.store(next_generation, Ordering::Release);
    crate::executor::wake_task(crate::executor::INPUT_TASK);
    crate::serial::serialln(format_args!(
        "SLOPOS-DESKTOP-SERVICE: policy submitted pid={pid} generation={next_generation} protocol={} capabilities=waybar-provider/swww-policy cpu={} memory={} wallpaper=aurora config_hashes={:#x}/{:#x}",
        commit.version,
        commit.cpu_usage,
        commit.memory_percentage,
        commit.waybar_hash,
        commit.swww_hash
    ));
    Ok(next_generation)
}

pub fn latest_after(generation: u64) -> Option<DesktopServiceSnapshot> {
    let current = GENERATION.load(Ordering::Acquire);
    if current == 0 || current == generation {
        return None;
    }
    let state = STATE.load(Ordering::Relaxed);
    Some(DesktopServiceSnapshot {
        generation: current,
        owner_pid: state as u32,
        capabilities: ((state >> 32) & 0xff) as u32,
        cpu_usage: (state >> 40) as u8,
        memory_percentage: (state >> 48) as u8,
        wallpaper: (state >> 56) as u8,
    })
}

pub fn snapshot_is_valid(snapshot: DesktopServiceSnapshot) -> bool {
    snapshot.owner_pid == DESKTOP_SERVICE_PID
        && snapshot.capabilities == CAPABILITY_WAYBAR_PROVIDER | CAPABILITY_SWWW_POLICY
        && snapshot.cpu_usage <= 100
        && snapshot.memory_percentage <= 100
        && snapshot.wallpaper == WALLPAPER_AURORA
}

pub fn acknowledge_applied(generation: u64) {
    let submitted = GENERATION.load(Ordering::Acquire);
    if generation == 0 || generation != submitted {
        crate::fatal("desktop service acknowledged an invalid policy generation");
    }
    if POLICY_APPLIED_GENERATION
        .compare_exchange(
            generation - 1,
            generation,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        crate::fatal("desktop service policy was acknowledged more than once");
    }
    crate::executor::wake_task(crate::executor::BLOCK_TASK);
    crate::serial::serialln(format_args!(
        "SLOPOS-DESKTOP-SERVICE: policy acknowledged generation={generation} owner_pid={DESKTOP_SERVICE_PID} event=policy-applied wake=block-task"
    ));
}

pub fn acknowledge_config_applied(generation: u64) {
    if generation == 0
        || CONFIG_APPLIED_GENERATION
            .compare_exchange(
                generation - 1,
                generation,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
    {
        crate::fatal("desktop service observed an invalid config generation");
    }
    crate::executor::wake_task(crate::executor::BLOCK_TASK);
    crate::serial::serialln(format_args!(
        "SLOPOS-DESKTOP-SERVICE: config acknowledged generation={generation} event=config-applied wake=block-task"
    ));
}

pub fn event_after(kind: u16, after_generation: u64) -> Option<DesktopServiceEvent> {
    let generation = match kind {
        EVENT_POLICY_APPLIED => POLICY_APPLIED_GENERATION.load(Ordering::Acquire),
        EVENT_CONFIG_APPLIED => CONFIG_APPLIED_GENERATION.load(Ordering::Acquire),
        _ => return None,
    };
    if generation == 0 || generation <= after_generation {
        None
    } else if kind == EVENT_POLICY_APPLIED {
        Some(DesktopServiceEvent::policy_applied(generation))
    } else {
        Some(DesktopServiceEvent::config_applied(generation))
    }
}

pub async fn next_event(kind: u16, after_generation: u64) -> DesktopServiceEvent {
    ServiceEvent {
        kind,
        after_generation,
    }
    .await
}

struct ServiceEvent {
    kind: u16,
    after_generation: u64,
}

impl Future for ServiceEvent {
    type Output = DesktopServiceEvent;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        event_after(self.kind, self.after_generation).map_or(Poll::Pending, Poll::Ready)
    }
}
