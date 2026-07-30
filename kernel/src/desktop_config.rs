// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use slopos_shell::{
    BarPosition, parse_niri_layout, parse_niri_shell_config, parse_swww_environment,
    parse_waybar_config, parse_waybar_style,
};

const CONFIG_BANKS: usize = 2;
const CONFIG_FILE_CAPACITY: usize = 4096;
const ENV_FILE_CAPACITY: usize = 512;
const NO_BANK: usize = usize::MAX;

#[derive(Clone, Copy)]
struct ConfigFile<const N: usize> {
    bytes: [u8; N],
    length: usize,
    path: &'static str,
}

impl<const N: usize> ConfigFile<N> {
    const fn empty() -> Self {
        Self {
            bytes: [0; N],
            length: 0,
            path: "",
        }
    }

    fn write(&mut self, bytes: &[u8], path: &'static str) -> Result<(), ConfigPublishError> {
        if bytes.is_empty()
            || bytes.len() > self.bytes.len()
            || core::str::from_utf8(bytes).is_err()
        {
            return Err(ConfigPublishError::InvalidFile);
        }
        self.bytes.fill(0);
        self.bytes[..bytes.len()].copy_from_slice(bytes);
        self.length = bytes.len();
        self.path = path;
        Ok(())
    }

    fn text(&self) -> &'static str {
        // SAFETY: ConfigFile belongs to a static bank, write validates UTF-8,
        // and the publication protocol prevents rewriting the referenced bank.
        unsafe {
            let bytes = core::slice::from_raw_parts(self.bytes.as_ptr(), self.length);
            core::str::from_utf8_unchecked(bytes)
        }
    }
}

#[derive(Clone, Copy)]
struct ConfigBank {
    niri: ConfigFile<CONFIG_FILE_CAPACITY>,
    waybar: ConfigFile<CONFIG_FILE_CAPACITY>,
    waybar_style: ConfigFile<CONFIG_FILE_CAPACITY>,
    swww: ConfigFile<ENV_FILE_CAPACITY>,
}

impl ConfigBank {
    const fn empty() -> Self {
        Self {
            niri: ConfigFile::empty(),
            waybar: ConfigFile::empty(),
            waybar_style: ConfigFile::empty(),
            swww: ConfigFile::empty(),
        }
    }
}

struct SharedBanks(UnsafeCell<[ConfigBank; CONFIG_BANKS]>);

// SAFETY: the block task is the sole writer. Release/acquire publication,
// generation acknowledgement, and alternating banks prevent concurrent access
// to a bank that is referenced by the desktop task.
unsafe impl Sync for SharedBanks {}

static BANKS: SharedBanks = SharedBanks(UnsafeCell::new([ConfigBank::empty(); CONFIG_BANKS]));
static PUBLISHED_BANK: AtomicUsize = AtomicUsize::new(NO_BANK);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CONSUMED_GENERATION: AtomicU64 = AtomicU64::new(0);
static WRITING: AtomicBool = AtomicBool::new(false);
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static INVALID_RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
pub struct DesktopConfigSources {
    pub generation: u64,
    pub niri: &'static str,
    pub niri_path: &'static str,
    pub waybar: &'static str,
    pub waybar_path: &'static str,
    pub waybar_style: &'static str,
    pub waybar_style_path: &'static str,
    pub swww: &'static str,
    pub swww_path: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigPublishError {
    Busy,
    InvalidFile,
    InvalidNiri,
    InvalidWaybar,
    UnsupportedBarPosition,
    InvalidWaybarStyle,
    InvalidSwww,
}

pub struct ConfigWriter {
    bank: usize,
}

impl ConfigWriter {
    pub fn write_niri(
        &mut self,
        bytes: &[u8],
        path: &'static str,
    ) -> Result<(), ConfigPublishError> {
        self.bank_mut().niri.write(bytes, path)
    }

    pub fn write_waybar(
        &mut self,
        bytes: &[u8],
        path: &'static str,
    ) -> Result<(), ConfigPublishError> {
        self.bank_mut().waybar.write(bytes, path)
    }

    pub fn write_waybar_style(
        &mut self,
        bytes: &[u8],
        path: &'static str,
    ) -> Result<(), ConfigPublishError> {
        self.bank_mut().waybar_style.write(bytes, path)
    }

    pub fn write_swww(
        &mut self,
        bytes: &[u8],
        path: &'static str,
    ) -> Result<(), ConfigPublishError> {
        self.bank_mut().swww.write(bytes, path)
    }

    pub fn publish(self) -> Result<u64, ConfigPublishError> {
        let result = self.validate_and_publish();
        WRITING.store(false, Ordering::Release);
        result
    }

    pub fn cancel(self) {
        WRITING.store(false, Ordering::Release);
    }

    fn validate_and_publish(&self) -> Result<u64, ConfigPublishError> {
        let sources = sources_for_bank(self.bank, 0);
        parse_niri_layout(sources.niri).map_err(|_| ConfigPublishError::InvalidNiri)?;
        parse_niri_shell_config(sources.niri).map_err(|_| ConfigPublishError::InvalidNiri)?;
        let waybar =
            parse_waybar_config(sources.waybar).map_err(|_| ConfigPublishError::InvalidWaybar)?;
        if waybar.position != BarPosition::Top {
            return Err(ConfigPublishError::UnsupportedBarPosition);
        }
        parse_waybar_style(sources.waybar_style)
            .map_err(|_| ConfigPublishError::InvalidWaybarStyle)?;
        parse_swww_environment(sources.swww).map_err(|_| ConfigPublishError::InvalidSwww)?;

        let generation = GENERATION.load(Ordering::Relaxed).saturating_add(1);
        PUBLISHED_BANK.store(self.bank, Ordering::Release);
        GENERATION.store(generation, Ordering::Release);
        crate::executor::wake_task(crate::executor::INPUT_TASK);
        Ok(generation)
    }

    fn bank_mut(&mut self) -> &'static mut ConfigBank {
        // SAFETY: begin_write holds WRITING and chose a bank that has been
        // acknowledged as inactive by the only consumer.
        unsafe { &mut (*BANKS.0.get())[self.bank] }
    }
}

pub fn begin_write() -> Result<ConfigWriter, ConfigPublishError> {
    if WRITING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(ConfigPublishError::Busy);
    }
    let generation = GENERATION.load(Ordering::Acquire);
    if generation != CONSUMED_GENERATION.load(Ordering::Acquire) {
        WRITING.store(false, Ordering::Release);
        return Err(ConfigPublishError::Busy);
    }
    let current = PUBLISHED_BANK.load(Ordering::Acquire);
    let bank = if current == 0 { 1 } else { 0 };
    Ok(ConfigWriter { bank })
}

pub fn latest_after(generation: u64) -> Option<DesktopConfigSources> {
    let current = GENERATION.load(Ordering::Acquire);
    if current == 0 || current == generation {
        return None;
    }
    let bank = PUBLISHED_BANK.load(Ordering::Acquire);
    if bank >= CONFIG_BANKS {
        return None;
    }
    Some(sources_for_bank(bank, current))
}

pub fn acknowledge(generation: u64) {
    CONSUMED_GENERATION.store(generation, Ordering::Release);
    crate::desktop_service::acknowledge_config_applied(generation);
}

pub fn current_generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

pub fn request_reload() -> bool {
    let first = !RELOAD_REQUESTED.swap(true, Ordering::AcqRel);
    crate::executor::wake_task(crate::executor::BLOCK_TASK);
    first
}

pub fn request_invalid_reload() -> bool {
    INVALID_RELOAD_REQUESTED.store(true, Ordering::Release);
    request_reload()
}

pub fn take_invalid_reload_request() -> bool {
    INVALID_RELOAD_REQUESTED.swap(false, Ordering::AcqRel)
}

pub fn take_reload_request() -> bool {
    RELOAD_REQUESTED.swap(false, Ordering::AcqRel)
}

fn sources_for_bank(bank: usize, generation: u64) -> DesktopConfigSources {
    // SAFETY: callers either hold the exclusive writer token or acquired a
    // published generation. In both cases this bank remains stable.
    let bank = unsafe { &(*BANKS.0.get())[bank] };
    DesktopConfigSources {
        generation,
        niri: bank.niri.text(),
        niri_path: bank.niri.path,
        waybar: bank.waybar.text(),
        waybar_path: bank.waybar.path,
        waybar_style: bank.waybar_style.text(),
        waybar_style_path: bank.waybar_style.path,
        swww: bank.swww.text(),
        swww_path: bank.swww.path,
    }
}
