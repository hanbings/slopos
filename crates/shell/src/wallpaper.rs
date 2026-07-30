// SPDX-License-Identifier: 0BSD

pub const MAX_WALLPAPER_PATH: usize = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionType {
    None,
    Simple,
    Fade,
    Left,
    Right,
    Top,
    Bottom,
    Center,
    Outer,
    Any,
    Random,
}

impl TransitionType {
    pub const fn default_step(self) -> u8 {
        match self {
            Self::None => u8::MAX,
            Self::Simple | Self::Fade => 2,
            _ => 90,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Simple => "simple",
            Self::Fade => "fade",
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Center => "center",
            Self::Outer => "outer",
            Self::Any => "any",
            Self::Random => "random",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeMode {
    Crop,
    Fit,
    No,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionOptions {
    pub kind: TransitionType,
    pub step: u8,
    pub fps: u8,
    pub duration_seconds: u16,
    pub angle_degrees: u16,
    pub resize: ResizeMode,
}

impl Default for TransitionOptions {
    fn default() -> Self {
        Self {
            kind: TransitionType::Simple,
            step: TransitionType::Simple.default_step(),
            fps: 30,
            duration_seconds: 3,
            angle_degrees: 45,
            resize: ResizeMode::Crop,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SwwwDefaults {
    pub transition: TransitionOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImgRequest<'a> {
    pub path: &'a str,
    pub output: Option<&'a str>,
    pub transition: TransitionOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwwwCommand<'a> {
    Daemon,
    Img(ImgRequest<'a>),
    Query,
    Kill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwwwParseError {
    Empty,
    UnknownCommand,
    MissingValue,
    MissingImage,
    DuplicateImage,
    InvalidNumber,
    InvalidTransition,
    InvalidResize,
    UnexpectedArgument,
}

pub fn parse_swww_command(
    input: &str,
    defaults: SwwwDefaults,
) -> Result<SwwwCommand<'_>, SwwwParseError> {
    let mut args = Arguments::new(input);
    let first = args.next().ok_or(SwwwParseError::Empty)?;
    if equal(first, "swww-daemon") {
        if args.next().is_some() {
            return Err(SwwwParseError::UnexpectedArgument);
        }
        return Ok(SwwwCommand::Daemon);
    }

    let subcommand = if equal(first, "swww") {
        args.next().ok_or(SwwwParseError::UnknownCommand)?
    } else {
        first
    };
    if equal(subcommand, "query") {
        return no_arguments(args, SwwwCommand::Query);
    }
    if equal(subcommand, "kill") {
        return no_arguments(args, SwwwCommand::Kill);
    }
    if equal(subcommand, "img") {
        return parse_img(args, defaults.transition).map(SwwwCommand::Img);
    }
    Err(SwwwParseError::UnknownCommand)
}

pub fn parse_swww_environment(input: &str) -> Result<SwwwDefaults, SwwwParseError> {
    let mut defaults = SwwwDefaults::default();
    let mut offset = 0usize;
    while offset < input.len() {
        let remainder = &input[offset..];
        let line_length = remainder.find('\n').unwrap_or(remainder.len());
        let line = remainder[..line_length].trim();
        offset += line_length + usize::from(line_length < remainder.len());
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            return Err(SwwwParseError::MissingValue);
        };
        let name = name.trim();
        let value = value.trim();
        if name == "SWWW_TRANSITION" {
            defaults.transition.kind = parse_transition(value)?;
            defaults.transition.step = defaults.transition.kind.default_step();
        } else if name == "SWWW_TRANSITION_STEP" {
            defaults.transition.step = parse_nonzero_u8(value)?;
        } else if name == "SWWW_TRANSITION_FPS" {
            defaults.transition.fps = parse_nonzero_u8(value)?;
        } else if name == "SWWW_TRANSITION_DURATION" {
            defaults.transition.duration_seconds = parse_u16(value)?;
        } else if name == "SWWW_TRANSITION_ANGLE" {
            defaults.transition.angle_degrees = parse_angle(value)?;
        }
    }
    Ok(defaults)
}

fn parse_img<'a>(
    mut args: Arguments<'a>,
    mut transition: TransitionOptions,
) -> Result<ImgRequest<'a>, SwwwParseError> {
    let mut path = None;
    let mut output = None;
    while let Some(argument) = args.next() {
        if equal(argument, "-o") || equal(argument, "--outputs") {
            output = Some(args.next().ok_or(SwwwParseError::MissingValue)?);
        } else if equal(argument, "--transition-type") {
            transition.kind = parse_transition(args.next().ok_or(SwwwParseError::MissingValue)?)?;
            transition.step = transition.kind.default_step();
        } else if equal(argument, "--transition-step") {
            transition.step = parse_nonzero_u8(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-fps") {
            transition.fps = parse_nonzero_u8(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-duration") {
            transition.duration_seconds =
                parse_u16(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-angle") {
            transition.angle_degrees =
                parse_angle(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--resize") {
            transition.resize = match args.next().ok_or(SwwwParseError::MissingValue)? {
                value if equal(value, "crop") => ResizeMode::Crop,
                value if equal(value, "fit") => ResizeMode::Fit,
                value if equal(value, "no") => ResizeMode::No,
                _ => return Err(SwwwParseError::InvalidResize),
            };
        } else if argument.starts_with('-') {
            return Err(SwwwParseError::UnexpectedArgument);
        } else if path.replace(argument).is_some() {
            return Err(SwwwParseError::DuplicateImage);
        }
    }
    Ok(ImgRequest {
        path: path.ok_or(SwwwParseError::MissingImage)?,
        output,
        transition,
    })
}

fn no_arguments<'a>(
    mut args: Arguments<'a>,
    command: SwwwCommand<'a>,
) -> Result<SwwwCommand<'a>, SwwwParseError> {
    if args.next().is_some() {
        Err(SwwwParseError::UnexpectedArgument)
    } else {
        Ok(command)
    }
}

fn parse_transition(value: &str) -> Result<TransitionType, SwwwParseError> {
    if equal(value, "none") {
        Ok(TransitionType::None)
    } else if equal(value, "simple") {
        Ok(TransitionType::Simple)
    } else if equal(value, "fade") {
        Ok(TransitionType::Fade)
    } else if equal(value, "left") {
        Ok(TransitionType::Left)
    } else if equal(value, "right") {
        Ok(TransitionType::Right)
    } else if equal(value, "top") {
        Ok(TransitionType::Top)
    } else if equal(value, "bottom") {
        Ok(TransitionType::Bottom)
    } else if equal(value, "center") {
        Ok(TransitionType::Center)
    } else if equal(value, "outer") {
        Ok(TransitionType::Outer)
    } else if equal(value, "any") {
        Ok(TransitionType::Any)
    } else if equal(value, "random") {
        Ok(TransitionType::Random)
    } else {
        Err(SwwwParseError::InvalidTransition)
    }
}

fn parse_nonzero_u8(value: &str) -> Result<u8, SwwwParseError> {
    let value = parse_u16(value)?;
    u8::try_from(value)
        .ok()
        .filter(|value| *value != 0)
        .ok_or(SwwwParseError::InvalidNumber)
}

fn parse_angle(value: &str) -> Result<u16, SwwwParseError> {
    parse_u16(value).and_then(|angle| {
        if angle < 360 {
            Ok(angle)
        } else {
            Err(SwwwParseError::InvalidNumber)
        }
    })
}

fn parse_u16(value: &str) -> Result<u16, SwwwParseError> {
    if value.is_empty() {
        return Err(SwwwParseError::InvalidNumber);
    }
    let mut parsed = 0u16;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(SwwwParseError::InvalidNumber);
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(u16::from(byte - b'0')))
            .ok_or(SwwwParseError::InvalidNumber)?;
    }
    Ok(parsed)
}

fn equal(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

struct Arguments<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Arguments<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() && bytes[self.offset].is_ascii_whitespace() {
            self.offset += 1;
        }
        if self.offset == bytes.len() {
            return None;
        }
        let start = self.offset;
        while self.offset < bytes.len() && !bytes[self.offset].is_ascii_whitespace() {
            self.offset += 1;
        }
        Some(&self.input[start..self.offset])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StoredPath {
    bytes: [u8; MAX_WALLPAPER_PATH],
    length: usize,
}

impl StoredPath {
    const fn empty() -> Self {
        Self {
            bytes: [0; MAX_WALLPAPER_PATH],
            length: 0,
        }
    }

    fn set(&mut self, value: &str) -> Result<(), SwwwDaemonError> {
        if value.is_empty() || value.len() > self.bytes.len() {
            return Err(SwwwDaemonError::InvalidPath);
        }
        self.bytes.fill(0);
        self.bytes[..value.len()].copy_from_slice(value.as_bytes());
        self.length = value.len();
        Ok(())
    }

    fn clear(&mut self) {
        self.bytes.fill(0);
        self.length = 0;
    }

    fn get(&self) -> Option<&str> {
        if self.length == 0 {
            return None;
        }
        // SAFETY: `set` copies bytes from an already validated UTF-8 `str`.
        Some(unsafe { core::str::from_utf8_unchecked(&self.bytes[..self.length]) })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwwwDaemonError {
    AlreadyRunning,
    NotRunning,
    InvalidPath,
    InvalidTransition,
    UnknownOutput,
    NoImage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WallpaperQuery<'a> {
    pub output: &'a str,
    pub width: u16,
    pub height: u16,
    pub image: &'a str,
}

pub struct WallpaperDaemon {
    output: &'static str,
    width: u16,
    height: u16,
    running: bool,
    current: StoredPath,
    previous: StoredPath,
    transition: TransitionOptions,
    progress: u8,
    transition_active: bool,
    generation: u32,
}

impl WallpaperDaemon {
    pub const fn new(output: &'static str, width: u16, height: u16) -> Self {
        Self {
            output,
            width,
            height,
            running: false,
            current: StoredPath::empty(),
            previous: StoredPath::empty(),
            transition: TransitionOptions {
                kind: TransitionType::Simple,
                step: 2,
                fps: 30,
                duration_seconds: 3,
                angle_degrees: 45,
                resize: ResizeMode::Crop,
            },
            progress: 0,
            transition_active: false,
            generation: 0,
        }
    }

    pub const fn is_running(&self) -> bool {
        self.running
    }

    pub fn start(&mut self) -> Result<(), SwwwDaemonError> {
        if self.running {
            return Err(SwwwDaemonError::AlreadyRunning);
        }
        self.running = true;
        self.current.clear();
        self.previous.clear();
        self.transition_active = false;
        self.progress = 0;
        Ok(())
    }

    pub fn kill(&mut self) -> Result<(), SwwwDaemonError> {
        if !self.running {
            return Err(SwwwDaemonError::NotRunning);
        }
        self.running = false;
        self.current.clear();
        self.previous.clear();
        self.transition_active = false;
        self.progress = 0;
        Ok(())
    }

    pub fn apply(&mut self, request: ImgRequest<'_>) -> Result<(), SwwwDaemonError> {
        if !self.running {
            return Err(SwwwDaemonError::NotRunning);
        }
        if request.transition.step == 0 || request.transition.fps == 0 {
            return Err(SwwwDaemonError::InvalidTransition);
        }
        if let Some(output) = request.output
            && output != "*"
            && !output.eq_ignore_ascii_case(self.output)
        {
            return Err(SwwwDaemonError::UnknownOutput);
        }
        let mut current = StoredPath::empty();
        current.set(request.path)?;
        self.previous = self.current;
        self.current = current;
        self.transition = request.transition;
        self.generation = self.generation.wrapping_add(1);
        self.transition.kind = resolve_transition(self.transition.kind, self.generation);
        self.transition_active =
            self.previous.get().is_some() && self.transition.kind != TransitionType::None;
        self.progress = if self.transition_active { 0 } else { u8::MAX };
        Ok(())
    }

    pub fn query(&self) -> Result<WallpaperQuery<'_>, SwwwDaemonError> {
        if !self.running {
            return Err(SwwwDaemonError::NotRunning);
        }
        Ok(WallpaperQuery {
            output: self.output,
            width: self.width,
            height: self.height,
            image: self.current.get().ok_or(SwwwDaemonError::NoImage)?,
        })
    }

    pub fn current_image(&self) -> Option<&str> {
        if self.running {
            self.current.get()
        } else {
            None
        }
    }

    pub fn previous_image(&self) -> Option<&str> {
        if self.running {
            self.previous.get()
        } else {
            None
        }
    }

    pub const fn transition(&self) -> TransitionOptions {
        self.transition
    }

    pub const fn progress(&self) -> u8 {
        self.progress
    }

    pub const fn transition_active(&self) -> bool {
        self.transition_active
    }

    pub fn tick(&mut self) -> bool {
        if !self.transition_active {
            return false;
        }
        self.progress = self.progress.saturating_add(self.transition.step);
        if self.progress == u8::MAX {
            self.transition_active = false;
            self.previous.clear();
        }
        true
    }

    pub fn set_progress(&mut self, progress: u8) {
        if self.transition_active {
            self.progress = progress;
        }
    }

    pub fn finish_transition(&mut self) {
        self.progress = u8::MAX;
        self.transition_active = false;
        self.previous.clear();
    }
}

fn resolve_transition(kind: TransitionType, generation: u32) -> TransitionType {
    const RANDOM_TYPES: [TransitionType; 8] = [
        TransitionType::Simple,
        TransitionType::Left,
        TransitionType::Right,
        TransitionType::Top,
        TransitionType::Bottom,
        TransitionType::Center,
        TransitionType::Outer,
        TransitionType::Fade,
    ];
    match kind {
        TransitionType::Any => {
            if generation & 1 == 0 {
                TransitionType::Center
            } else {
                TransitionType::Outer
            }
        }
        TransitionType::Random => RANDOM_TYPES[generation as usize % RANDOM_TYPES.len()],
        value => value,
    }
}

pub fn transition_pixel(
    kind: TransitionType,
    progress: u8,
    position: (u16, u16),
    dimensions: (u16, u16),
    old: u32,
    new: u32,
) -> u32 {
    if kind == TransitionType::None {
        return new;
    }
    if progress == 0 {
        return old;
    }
    if progress == u8::MAX {
        return new;
    }
    let (x, y) = position;
    let (width, height) = dimensions;
    match kind {
        TransitionType::Simple | TransitionType::Fade => blend(old, new, progress),
        TransitionType::Left => choose(scale_position(x, width) <= progress, old, new),
        TransitionType::Right => choose(
            scale_position(width.saturating_sub(1).saturating_sub(x), width) <= progress,
            old,
            new,
        ),
        TransitionType::Top => choose(scale_position(y, height) <= progress, old, new),
        TransitionType::Bottom => choose(
            scale_position(height.saturating_sub(1).saturating_sub(y), height) <= progress,
            old,
            new,
        ),
        TransitionType::Center => {
            choose(center_distance(x, y, width, height) <= progress, old, new)
        }
        TransitionType::Outer => choose(
            center_distance(x, y, width, height) >= u8::MAX - progress,
            old,
            new,
        ),
        TransitionType::None | TransitionType::Any | TransitionType::Random => new,
    }
}

fn choose(reveal: bool, old: u32, new: u32) -> u32 {
    if reveal { new } else { old }
}

fn scale_position(position: u16, length: u16) -> u8 {
    if length <= 1 {
        return 0;
    }
    (u32::from(position) * 255 / u32::from(length - 1)) as u8
}

fn center_distance(x: u16, y: u16, width: u16, height: u16) -> u8 {
    let horizontal = axis_center_distance(x, width);
    let vertical = axis_center_distance(y, height);
    horizontal.max(vertical).min(255) as u8
}

fn axis_center_distance(position: u16, length: u16) -> u32 {
    if length <= 1 {
        return 0;
    }
    let raw = (i32::from(position) * 2 + 1 - i32::from(length)).unsigned_abs();
    let minimum = u32::from(length % 2 == 0);
    let maximum = u32::from(length - 1).saturating_sub(minimum).max(1);
    raw.saturating_sub(minimum) * 255 / maximum
}

fn blend(old: u32, new: u32, progress: u8) -> u32 {
    let inverse = 255 - u32::from(progress);
    let progress = u32::from(progress);
    let red = (((old >> 16) & 0xff) * inverse + ((new >> 16) & 0xff) * progress) / 255;
    let green = (((old >> 8) & 0xff) * inverse + ((new >> 8) & 0xff) * progress) / 255;
    let blue = ((old & 0xff) * inverse + (new & 0xff) * progress) / 255;
    red << 16 | green << 8 | blue
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PpmError {
    InvalidMagic,
    MissingHeader,
    InvalidNumber,
    InvalidDimensions,
    InvalidColor,
    TruncatedPixels,
    ExtraPixels,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PpmImage<'a> {
    width: u16,
    height: u16,
    maximum: u16,
    pixels: &'a str,
}

impl<'a> PpmImage<'a> {
    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub fn pixels(self) -> PpmPixels<'a> {
        PpmPixels {
            tokens: PpmTokens::new(self.pixels),
            maximum: self.maximum,
        }
    }
}

pub fn parse_ppm(input: &str) -> Result<PpmImage<'_>, PpmError> {
    let mut tokens = PpmTokens::new(input);
    if tokens.next() != Some("P3") {
        return Err(PpmError::InvalidMagic);
    }
    let width = parse_ppm_number(tokens.next().ok_or(PpmError::MissingHeader)?)?;
    let height = parse_ppm_number(tokens.next().ok_or(PpmError::MissingHeader)?)?;
    let maximum = parse_ppm_number(tokens.next().ok_or(PpmError::MissingHeader)?)?;
    if width == 0 || height == 0 || maximum == 0 || maximum > 255 {
        return Err(PpmError::InvalidDimensions);
    }
    let pixel_start = tokens.offset;
    let expected_components = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or(PpmError::InvalidDimensions)?;
    for _ in 0..expected_components {
        let value = parse_ppm_number(tokens.next().ok_or(PpmError::TruncatedPixels)?)?;
        if value > maximum {
            return Err(PpmError::InvalidColor);
        }
    }
    if tokens.next().is_some() {
        return Err(PpmError::ExtraPixels);
    }
    Ok(PpmImage {
        width,
        height,
        maximum,
        pixels: &input[pixel_start..],
    })
}

pub struct PpmPixels<'a> {
    tokens: PpmTokens<'a>,
    maximum: u16,
}

impl Iterator for PpmPixels<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let red = parse_ppm_number(self.tokens.next()?).ok()?;
        let green = parse_ppm_number(self.tokens.next()?).ok()?;
        let blue = parse_ppm_number(self.tokens.next()?).ok()?;
        let scale = |value: u16| u32::from(value) * 255 / u32::from(self.maximum);
        Some(scale(red) << 16 | scale(green) << 8 | scale(blue))
    }
}

struct PpmTokens<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> PpmTokens<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let bytes = self.input.as_bytes();
        loop {
            while self.offset < bytes.len() && bytes[self.offset].is_ascii_whitespace() {
                self.offset += 1;
            }
            if self.offset == bytes.len() {
                return None;
            }
            if bytes[self.offset] == b'#' {
                while self.offset < bytes.len() && bytes[self.offset] != b'\n' {
                    self.offset += 1;
                }
                continue;
            }
            break;
        }
        let start = self.offset;
        while self.offset < bytes.len()
            && !bytes[self.offset].is_ascii_whitespace()
            && bytes[self.offset] != b'#'
        {
            self.offset += 1;
        }
        Some(&self.input[start..self.offset])
    }
}

fn parse_ppm_number(value: &str) -> Result<u16, PpmError> {
    if value.is_empty() {
        return Err(PpmError::InvalidNumber);
    }
    let mut parsed = 0u16;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(PpmError::InvalidNumber);
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(u16::from(byte - b'0')))
            .ok_or(PpmError::InvalidNumber)?;
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_daemon_img_query_and_kill_commands() {
        assert_eq!(
            parse_swww_command("swww-daemon", SwwwDefaults::default()),
            Ok(SwwwCommand::Daemon)
        );
        let SwwwCommand::Img(request) = parse_swww_command(
            "swww img -o SLOPOS-1 sunset.ppm --transition-type center --transition-step 64 --transition-fps 60 --resize fit",
            SwwwDefaults::default(),
        )
        .unwrap()
        else {
            panic!("expected image request");
        };
        assert_eq!(request.path, "sunset.ppm");
        assert_eq!(request.output, Some("SLOPOS-1"));
        assert_eq!(request.transition.kind, TransitionType::Center);
        assert_eq!(request.transition.step, 64);
        assert_eq!(request.transition.fps, 60);
        assert_eq!(request.transition.resize, ResizeMode::Fit);
        assert_eq!(
            parse_swww_command("SWWW QUERY", SwwwDefaults::default()),
            Ok(SwwwCommand::Query)
        );
        assert_eq!(
            parse_swww_command("kill", SwwwDefaults::default()),
            Ok(SwwwCommand::Kill)
        );
    }

    #[test]
    fn applies_environment_defaults_and_transition_specific_step() {
        let defaults = parse_swww_environment(
            "SWWW_TRANSITION=center\nSWWW_TRANSITION_STEP=33\nSWWW_TRANSITION_FPS=60\nSWWW_TRANSITION_DURATION=2\n",
        )
        .unwrap();
        assert_eq!(defaults.transition.kind, TransitionType::Center);
        assert_eq!(defaults.transition.step, 33);
        assert_eq!(defaults.transition.fps, 60);
        assert_eq!(defaults.transition.duration_seconds, 2);

        let SwwwCommand::Img(request) =
            parse_swww_command("img image.ppm --transition-type left", defaults).unwrap()
        else {
            panic!("expected image request");
        };
        assert_eq!(request.transition.kind, TransitionType::Left);
        assert_eq!(request.transition.step, TransitionType::Left.default_step());
    }

    #[test]
    fn rejects_invalid_and_ambiguous_image_arguments() {
        assert_eq!(
            parse_swww_command("swww img", SwwwDefaults::default()),
            Err(SwwwParseError::MissingImage)
        );
        assert_eq!(
            parse_swww_command("swww img one.ppm two.ppm", SwwwDefaults::default()),
            Err(SwwwParseError::DuplicateImage)
        );
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --transition-step 0",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidNumber)
        );
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --transition-type explode",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidTransition)
        );
    }

    #[test]
    fn daemon_tracks_image_lifecycle_and_output() {
        let mut daemon = WallpaperDaemon::new("SLOPOS-1", 800, 600);
        assert_eq!(daemon.query(), Err(SwwwDaemonError::NotRunning));
        daemon.start().unwrap();
        let first = ImgRequest {
            path: "aurora.ppm",
            output: None,
            transition: TransitionOptions::default(),
        };
        daemon.apply(first).unwrap();
        assert!(!daemon.transition_active());
        assert_eq!(daemon.query().unwrap().image, "aurora.ppm");
        let invalid = ImgRequest {
            path: "sunset.ppm",
            output: None,
            transition: TransitionOptions {
                step: 0,
                ..TransitionOptions::default()
            },
        };
        assert_eq!(
            daemon.apply(invalid),
            Err(SwwwDaemonError::InvalidTransition)
        );
        assert_eq!(daemon.query().unwrap().image, "aurora.ppm");
        assert_eq!(daemon.previous_image(), None);

        let second = ImgRequest {
            path: "sunset.ppm",
            output: Some("SLOPOS-1"),
            transition: TransitionOptions {
                kind: TransitionType::Center,
                step: 128,
                ..TransitionOptions::default()
            },
        };
        daemon.apply(second).unwrap();
        assert!(daemon.transition_active());
        assert_eq!(daemon.previous_image(), Some("aurora.ppm"));
        assert!(daemon.tick());
        assert_eq!(daemon.progress(), 128);
        assert!(daemon.tick());
        assert!(!daemon.transition_active());
        assert_eq!(daemon.previous_image(), None);
        daemon.kill().unwrap();
        assert_eq!(daemon.current_image(), None);
    }

    #[test]
    fn transition_masks_and_blending_reach_expected_pixels() {
        let old = 0x00_00_00;
        let new = 0xff_80_40;
        assert_eq!(
            transition_pixel(TransitionType::Simple, 0, (0, 0), (4, 4), old, new),
            old
        );
        assert_eq!(
            transition_pixel(TransitionType::Simple, u8::MAX, (0, 0), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel(TransitionType::Simple, 128, (0, 0), (4, 4), old, new),
            0x80_40_20
        );
        assert_eq!(
            transition_pixel(TransitionType::Left, 90, (0, 0), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel(TransitionType::Left, 90, (3, 0), (4, 4), old, new),
            old
        );
        assert_eq!(
            transition_pixel(TransitionType::Center, 1, (1, 1), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel(TransitionType::Center, 254, (0, 0), (4, 4), old, new),
            old
        );
        assert_eq!(
            transition_pixel(TransitionType::Center, 255, (0, 0), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel(TransitionType::None, 0, (0, 0), (4, 4), old, new),
            new
        );
    }

    #[test]
    fn parses_bounded_ascii_ppm_pixels() {
        let image = parse_ppm("P3\n# tiny\n2 1\n15\n15 0 0  0 8 15\n").unwrap();
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 1);
        let mut pixels = image.pixels();
        assert_eq!(pixels.next(), Some(0xff_00_00));
        assert_eq!(pixels.next(), Some(0x00_88_ff));
        assert_eq!(pixels.next(), None);
        assert_eq!(parse_ppm("P3 1 1 15 16 0 0"), Err(PpmError::InvalidColor));
        assert_eq!(parse_ppm("P3 1 1 15 1 2"), Err(PpmError::TruncatedPixels));
        assert_eq!(parse_ppm("P3 1 1 15 1 2 3 4"), Err(PpmError::ExtraPixels));
    }

    #[test]
    fn embedded_wallpapers_have_matching_valid_geometry() {
        let aurora = parse_ppm(include_str!("../../../assets/wallpapers/aurora.ppm")).unwrap();
        let sunset = parse_ppm(include_str!("../../../assets/wallpapers/sunset.ppm")).unwrap();
        assert_eq!((aurora.width(), aurora.height()), (12, 8));
        assert_eq!(
            (sunset.width(), sunset.height()),
            (aurora.width(), aurora.height())
        );
        assert_eq!(aurora.pixels().count(), 96);
        assert_eq!(sunset.pixels().count(), 96);
    }
}
