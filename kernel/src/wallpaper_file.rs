// SPDX-License-Identifier: 0BSD

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use slopos_shell::{
    ImgRequest, MAX_WALLPAPER_PATH, ResizeMode, TransitionOptions, TransitionPosition,
    TransitionType, parse_ppm,
};

pub const WALLPAPER_FILE_CAPACITY: usize = 8 * 1024;
const RESULT_BANKS: usize = 2;
const OUTPUT_CAPACITY: usize = 32;
const NO_BANK: usize = usize::MAX;
const RELATIVE_PREFIX: &str = "/usr/share/slopos/";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallpaperFileRequestError {
    Busy,
    InvalidPath,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WallpaperFileError {
    InvalidPath,
    NotFound,
    FileTooLarge,
    InvalidUtf8,
    InvalidPpm,
}

#[derive(Clone, Copy)]
struct StoredText<const N: usize> {
    bytes: [u8; N],
    length: usize,
}

impl<const N: usize> StoredText<N> {
    const fn empty() -> Self {
        Self {
            bytes: [0; N],
            length: 0,
        }
    }

    fn set(&mut self, value: &str) -> Result<(), WallpaperFileRequestError> {
        if value.is_empty()
            || value.len() > self.bytes.len()
            || value
                .bytes()
                .any(|byte| !byte.is_ascii() || byte.is_ascii_control())
        {
            return Err(WallpaperFileRequestError::InvalidPath);
        }
        self.bytes.fill(0);
        self.bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.length = value.len();
        Ok(())
    }

    fn set_resolved_path(&mut self, value: &str) -> Result<(), WallpaperFileRequestError> {
        self.bytes.fill(0);
        let mut length = 0usize;
        if !value.starts_with('/') {
            if RELATIVE_PREFIX.len() > self.bytes.len() {
                return Err(WallpaperFileRequestError::InvalidPath);
            }
            self.bytes[..RELATIVE_PREFIX.len()].copy_from_slice(RELATIVE_PREFIX.as_bytes());
            length = RELATIVE_PREFIX.len();
        }
        let end = length
            .checked_add(value.len())
            .ok_or(WallpaperFileRequestError::InvalidPath)?;
        if value.is_empty() || end > self.bytes.len() {
            return Err(WallpaperFileRequestError::InvalidPath);
        }
        for (destination, source) in self.bytes[length..end].iter_mut().zip(value.bytes()) {
            if !source.is_ascii() || source.is_ascii_control() {
                return Err(WallpaperFileRequestError::InvalidPath);
            }
            *destination = source.to_ascii_lowercase();
        }
        self.length = end;
        Ok(())
    }

    fn as_str(&self) -> &str {
        // SAFETY: setters accept ASCII only and preserve that invariant.
        unsafe { core::str::from_utf8_unchecked(&self.bytes[..self.length]) }
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

#[derive(Clone, Copy)]
pub struct WallpaperFileRequest {
    generation: u64,
    request_path: StoredText<MAX_WALLPAPER_PATH>,
    resolved_path: StoredText<MAX_WALLPAPER_PATH>,
    output: StoredText<OUTPUT_CAPACITY>,
    has_output: bool,
    transition: TransitionOptions,
}

impl WallpaperFileRequest {
    const fn empty() -> Self {
        Self {
            generation: 0,
            request_path: StoredText::empty(),
            resolved_path: StoredText::empty(),
            output: StoredText::empty(),
            has_output: false,
            transition: TransitionOptions {
                kind: TransitionType::Simple,
                step: 2,
                fps: 30,
                duration_seconds: 3,
                angle_degrees: 45,
                position: TransitionPosition::center(),
                invert_y: false,
                resize: ResizeMode::Crop,
            },
        }
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub fn request_path(&self) -> &str {
        self.request_path.as_str()
    }

    pub fn resolved_path(&self) -> &str {
        self.resolved_path.as_str()
    }

    pub fn resolved_path_bytes(&self) -> &[u8] {
        self.resolved_path.as_bytes()
    }
}

struct SharedRequest(UnsafeCell<WallpaperFileRequest>);

// SAFETY: the desktop task writes a complete request before release-publishing
// REQUEST_READY. The block task is the only reader and copies it after acquire.
unsafe impl Sync for SharedRequest {}

static REQUEST: SharedRequest = SharedRequest(UnsafeCell::new(WallpaperFileRequest::empty()));
static REQUEST_BUSY: AtomicBool = AtomicBool::new(false);
static REQUEST_READY: AtomicBool = AtomicBool::new(false);
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn request(request: ImgRequest<'_>) -> Result<u64, WallpaperFileRequestError> {
    let mut stored = WallpaperFileRequest::empty();
    stored.request_path.set(request.path)?;
    stored.resolved_path.set_resolved_path(request.path)?;
    if let Some(output) = request.output {
        stored.output.set(output)?;
        stored.has_output = true;
    }
    stored.transition = request.transition;

    if REQUEST_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(WallpaperFileRequestError::Busy);
    }
    stored.generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    // SAFETY: REQUEST_BUSY grants the desktop task exclusive write access.
    unsafe { *REQUEST.0.get() = stored };
    REQUEST_READY.store(true, Ordering::Release);
    crate::executor::wake_task(crate::executor::BLOCK_TASK);
    Ok(stored.generation)
}

pub fn take_request() -> Option<WallpaperFileRequest> {
    if !REQUEST_READY.swap(false, Ordering::AcqRel) {
        return None;
    }
    // SAFETY: acquire observes the complete request and it remains immutable
    // until the desktop acknowledges the corresponding published result.
    Some(unsafe { *REQUEST.0.get() })
}

pub fn request_ready() -> bool {
    REQUEST_READY.load(Ordering::Acquire)
}

struct WallpaperFileBank {
    request: WallpaperFileRequest,
    image: [u8; WALLPAPER_FILE_CAPACITY],
    image_length: usize,
    result: Result<(), WallpaperFileError>,
}

impl WallpaperFileBank {
    const fn empty() -> Self {
        Self {
            request: WallpaperFileRequest::empty(),
            image: [0; WALLPAPER_FILE_CAPACITY],
            image_length: 0,
            result: Ok(()),
        }
    }
}

struct SharedBanks(UnsafeCell<[WallpaperFileBank; RESULT_BANKS]>);

// SAFETY: the block task is the sole writer. Release/acquire publication and
// desktop acknowledgement prevent a bank from being rewritten while the
// renderer still holds static string slices into it.
unsafe impl Sync for SharedBanks {}

static BANKS: SharedBanks = SharedBanks(UnsafeCell::new([
    WallpaperFileBank::empty(),
    WallpaperFileBank::empty(),
]));
static PUBLISHED_BANK: AtomicUsize = AtomicUsize::new(NO_BANK);
static ACTIVE_IMAGE_BANK: AtomicUsize = AtomicUsize::new(NO_BANK);
static RESULT_GENERATION: AtomicU64 = AtomicU64::new(0);
static CONSUMED_GENERATION: AtomicU64 = AtomicU64::new(0);

pub struct WallpaperFileWriter {
    bank: usize,
    length: usize,
}

pub fn begin_result(request: WallpaperFileRequest) -> WallpaperFileWriter {
    let active = ACTIVE_IMAGE_BANK.load(Ordering::Acquire);
    let bank = if active == 0 { 1 } else { 0 };
    // SAFETY: REQUEST_BUSY permits only one in-flight request, the block task
    // is the sole writer, and the selected bank is not the active image bank.
    let output = unsafe { &mut (*BANKS.0.get())[bank] };
    output.request = request;
    output.image.fill(0);
    output.image_length = 0;
    output.result = Ok(());
    WallpaperFileWriter { bank, length: 0 }
}

impl WallpaperFileWriter {
    pub fn write(&mut self, bytes: &[u8]) -> bool {
        let Some(end) = self.length.checked_add(bytes.len()) else {
            return false;
        };
        if end > WALLPAPER_FILE_CAPACITY {
            return false;
        }
        // SAFETY: this writer has exclusive access to its unpublished bank.
        let bank = unsafe { &mut (*BANKS.0.get())[self.bank] };
        bank.image[self.length..end].copy_from_slice(bytes);
        self.length = end;
        true
    }

    pub fn publish(self) -> Result<u64, WallpaperFileError> {
        let validation = {
            // SAFETY: this writer has exclusive access to its unpublished bank.
            let bank = unsafe { &mut (*BANKS.0.get())[self.bank] };
            bank.image_length = self.length;
            core::str::from_utf8(&bank.image[..self.length])
                .map_err(|_| WallpaperFileError::InvalidUtf8)
                .and_then(|image| {
                    parse_ppm(image)
                        .map(|_| ())
                        .map_err(|_| WallpaperFileError::InvalidPpm)
                })
        };
        if let Err(error) = validation {
            self.publish_error(error);
            return Err(error);
        }
        // SAFETY: validation above ended its borrow of this unpublished bank.
        let bank = unsafe { &mut (*BANKS.0.get())[self.bank] };
        bank.result = Ok(());
        let generation = bank.request.generation;
        publish_bank(self.bank, generation);
        Ok(generation)
    }

    pub fn publish_error(self, error: WallpaperFileError) {
        // SAFETY: this writer has exclusive access to its unpublished bank.
        let bank = unsafe { &mut (*BANKS.0.get())[self.bank] };
        bank.image_length = 0;
        bank.result = Err(error);
        publish_bank(self.bank, bank.request.generation);
    }
}

fn publish_bank(bank: usize, generation: u64) {
    PUBLISHED_BANK.store(bank, Ordering::Release);
    RESULT_GENERATION.store(generation, Ordering::Release);
    crate::executor::wake_task(crate::executor::INPUT_TASK);
}

pub fn publish_error(request: WallpaperFileRequest, error: WallpaperFileError) {
    begin_result(request).publish_error(error);
}

#[derive(Clone, Copy)]
pub struct WallpaperFileSource {
    pub generation: u64,
    pub request_path: &'static str,
    pub resolved_path: &'static str,
    pub output: Option<&'static str>,
    pub transition: TransitionOptions,
    pub image: &'static str,
}

#[derive(Clone, Copy)]
pub enum WallpaperFileUpdate {
    Ready(WallpaperFileSource),
    Failed {
        generation: u64,
        request_path: &'static str,
        resolved_path: &'static str,
        error: WallpaperFileError,
    },
}

impl WallpaperFileUpdate {
    pub const fn generation(self) -> u64 {
        match self {
            Self::Ready(source) => source.generation,
            Self::Failed { generation, .. } => generation,
        }
    }
}

pub fn latest_after(generation: u64) -> Option<WallpaperFileUpdate> {
    let current = RESULT_GENERATION.load(Ordering::Acquire);
    if current == 0 || current == generation {
        return None;
    }
    let index = PUBLISHED_BANK.load(Ordering::Acquire);
    if index >= RESULT_BANKS {
        return None;
    }
    // SAFETY: acquire observes the complete published bank. It cannot be
    // rewritten until acknowledge advances CONSUMED_GENERATION.
    let bank = unsafe { &(*BANKS.0.get())[index] };
    let request_path = bank.request.request_path.as_str();
    let resolved_path = bank.request.resolved_path.as_str();
    Some(match bank.result {
        Ok(()) => {
            // SAFETY: publish validates UTF-8 for the exact stored range.
            let image = unsafe { core::str::from_utf8_unchecked(&bank.image[..bank.image_length]) };
            WallpaperFileUpdate::Ready(WallpaperFileSource {
                generation: current,
                request_path,
                resolved_path,
                output: bank
                    .request
                    .has_output
                    .then(|| bank.request.output.as_str()),
                transition: bank.request.transition,
                image,
            })
        }
        Err(error) => WallpaperFileUpdate::Failed {
            generation: current,
            request_path,
            resolved_path,
            error,
        },
    })
}

pub fn acknowledge(generation: u64, pin_image: bool) {
    let published = RESULT_GENERATION.load(Ordering::Acquire);
    let bank = PUBLISHED_BANK.load(Ordering::Acquire);
    if generation == 0
        || generation != published
        || bank >= RESULT_BANKS
        || CONSUMED_GENERATION
            .compare_exchange(
                generation - 1,
                generation,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
    {
        crate::fatal("swww VFS image result acknowledgement is invalid");
    }
    // SAFETY: the desktop has completed rendering the transition before this
    // acknowledgement, so a successful bank becomes the only pinned VFS image.
    let succeeded = unsafe { (*BANKS.0.get())[bank].result.is_ok() };
    if pin_image && !succeeded {
        crate::fatal("swww VFS failed result cannot become the active image");
    }
    if pin_image {
        ACTIVE_IMAGE_BANK.store(bank, Ordering::Release);
    }
    REQUEST_BUSY.store(false, Ordering::Release);
}
