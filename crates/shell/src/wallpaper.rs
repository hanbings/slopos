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
    Wipe,
    Wave,
    Grow,
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
            Self::Wipe => "wipe",
            Self::Wave => "wave",
            Self::Grow => "grow",
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
    Stretch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResizeFilter {
    Nearest,
    Bilinear,
    CatmullRom,
    Mitchell,
    Lanczos3,
}

impl ResizeFilter {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest",
            Self::Bilinear => "Bilinear",
            Self::CatmullRom => "CatmullRom",
            Self::Mitchell => "Mitchell",
            Self::Lanczos3 => "Lanczos3",
        }
    }
}

impl Default for ResizeFilter {
    fn default() -> Self {
        Self::Nearest
    }
}

impl ResizeMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Crop => "crop",
            Self::Fit => "fit",
            Self::No => "no",
            Self::Stretch => "stretch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CropGravity {
    Center,
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl CropGravity {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Center => "center",
            Self::TopLeft => "top-left",
            Self::Top => "top",
            Self::TopRight => "top-right",
            Self::Left => "left",
            Self::Right => "right",
            Self::BottomLeft => "bottom-left",
            Self::Bottom => "bottom",
            Self::BottomRight => "bottom-right",
        }
    }
}

const POSITION_SCALE: u32 = 10_000;
const TRANSITION_FIXED_SCALE: i32 = 10_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionBezier {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionWave {
    pub width: u32,
    pub height: i32,
}

impl TransitionWave {
    pub const fn swww_default() -> Self {
        Self {
            width: 200_000,
            height: 200_000,
        }
    }
}

impl Default for TransitionWave {
    fn default() -> Self {
        Self::swww_default()
    }
}

impl TransitionBezier {
    pub const fn swww_default() -> Self {
        Self {
            x1: 5_400,
            y1: 0,
            x2: 3_400,
            y2: 9_900,
        }
    }

    pub const fn linear() -> Self {
        Self {
            x1: 0,
            y1: 0,
            x2: TRANSITION_FIXED_SCALE,
            y2: TRANSITION_FIXED_SCALE,
        }
    }
}

impl Default for TransitionBezier {
    fn default() -> Self {
        Self::swww_default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionCoordinate {
    Pixel(u32),
    Percent(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionPosition {
    pub x: TransitionCoordinate,
    pub y: TransitionCoordinate,
}

impl TransitionPosition {
    pub const fn center() -> Self {
        Self {
            x: TransitionCoordinate::Percent(POSITION_SCALE / 2),
            y: TransitionCoordinate::Percent(POSITION_SCALE / 2),
        }
    }

    pub fn to_pixel(self, dimensions: (u16, u16), invert_y: bool) -> (i32, i32) {
        let width = u32::from(dimensions.0);
        let height = u32::from(dimensions.1);
        let x = coordinate_to_pixel(self.x, width);
        let y_from_bottom = coordinate_to_pixel(self.y, height);
        let y = if invert_y {
            y_from_bottom
        } else {
            i64::from(height) - y_from_bottom
        };
        (clamp_i64_to_i32(x), clamp_i64_to_i32(y))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionOptions {
    pub kind: TransitionType,
    pub step: u8,
    pub fps: u8,
    pub duration_milliseconds: u32,
    pub angle_degrees: u16,
    pub position: TransitionPosition,
    pub invert_y: bool,
    pub bezier: TransitionBezier,
    pub wave: TransitionWave,
    pub resize: ResizeMode,
    pub filter: ResizeFilter,
    pub crop_gravity: CropGravity,
    pub fill_color: u32,
}

impl Default for TransitionOptions {
    fn default() -> Self {
        Self {
            kind: TransitionType::Simple,
            step: TransitionType::Simple.default_step(),
            fps: 30,
            duration_milliseconds: 3_000,
            angle_degrees: 45,
            position: TransitionPosition::center(),
            invert_y: false,
            bezier: TransitionBezier::default(),
            wave: TransitionWave::default(),
            resize: ResizeMode::Crop,
            filter: ResizeFilter::default(),
            crop_gravity: CropGravity::Center,
            fill_color: 0,
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
pub struct ClearRequest<'a> {
    pub color: u32,
    pub output: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwwwCommand<'a> {
    Daemon,
    Img(ImgRequest<'a>),
    Clear(ClearRequest<'a>),
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
    InvalidFilter,
    InvalidCropGravity,
    InvalidPosition,
    InvalidBezier,
    InvalidWave,
    InvalidBoolean,
    InvalidColor,
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
    if equal(subcommand, "clear") {
        return parse_clear(args).map(SwwwCommand::Clear);
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
            defaults.transition.duration_milliseconds = parse_duration_milliseconds(value)?;
        } else if name == "SWWW_TRANSITION_ANGLE" {
            defaults.transition.angle_degrees = parse_angle(value)?;
        } else if name == "SWWW_TRANSITION_POS" {
            defaults.transition.position = parse_position(value)?;
        } else if name == "SWWW_INVERT_Y" {
            defaults.transition.invert_y = parse_bool(value)?;
        } else if name == "SWWW_TRANSITION_BEZIER" {
            defaults.transition.bezier = parse_bezier(value)?;
        } else if name == "SWWW_TRANSITION_WAVE" {
            defaults.transition.wave = parse_wave(value)?;
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
        } else if equal(argument, "-t") || equal(argument, "--transition-type") {
            transition.kind = parse_transition(args.next().ok_or(SwwwParseError::MissingValue)?)?;
            transition.step = transition.kind.default_step();
        } else if equal(argument, "--transition-step") {
            transition.step = parse_nonzero_u8(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-fps") {
            transition.fps = parse_nonzero_u8(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-duration") {
            transition.duration_milliseconds =
                parse_duration_milliseconds(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-angle") {
            transition.angle_degrees =
                parse_angle(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-pos") {
            transition.position = parse_position(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--invert-y") {
            transition.invert_y = parse_bool(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-bezier") {
            transition.bezier = parse_bezier(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--transition-wave") {
            transition.wave = parse_wave(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--no-resize") {
            transition.resize = ResizeMode::No;
        } else if equal(argument, "--resize") {
            transition.resize = match args.next().ok_or(SwwwParseError::MissingValue)? {
                value if equal(value, "crop") => ResizeMode::Crop,
                value if equal(value, "fit") => ResizeMode::Fit,
                value if equal(value, "no") => ResizeMode::No,
                value if equal(value, "stretch") => ResizeMode::Stretch,
                _ => return Err(SwwwParseError::InvalidResize),
            };
        } else if equal(argument, "-f") || equal(argument, "--filter") {
            transition.filter = parse_filter(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--crop-gravity") {
            transition.crop_gravity =
                parse_crop_gravity(args.next().ok_or(SwwwParseError::MissingValue)?)?;
        } else if equal(argument, "--fill-color") {
            transition.fill_color =
                parse_hex_color(args.next().ok_or(SwwwParseError::MissingValue)?)?;
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

fn parse_clear<'a>(mut args: Arguments<'a>) -> Result<ClearRequest<'a>, SwwwParseError> {
    let mut color = None;
    let mut output = None;
    while let Some(argument) = args.next() {
        if equal(argument, "-o") || equal(argument, "--outputs") {
            output = Some(args.next().ok_or(SwwwParseError::MissingValue)?);
        } else if argument.starts_with('-') {
            return Err(SwwwParseError::UnexpectedArgument);
        } else {
            if color.is_some() {
                return Err(SwwwParseError::UnexpectedArgument);
            }
            color = Some(parse_hex_color(argument)?);
        }
    }
    Ok(ClearRequest {
        color: color.unwrap_or(0),
        output,
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
    } else if equal(value, "wipe") {
        Ok(TransitionType::Wipe)
    } else if equal(value, "wave") {
        Ok(TransitionType::Wave)
    } else if equal(value, "grow") {
        Ok(TransitionType::Grow)
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

fn parse_filter(value: &str) -> Result<ResizeFilter, SwwwParseError> {
    if equal(value, "Nearest") {
        Ok(ResizeFilter::Nearest)
    } else if equal(value, "Bilinear") {
        Ok(ResizeFilter::Bilinear)
    } else if equal(value, "CatmullRom") {
        Ok(ResizeFilter::CatmullRom)
    } else if equal(value, "Mitchell") {
        Ok(ResizeFilter::Mitchell)
    } else if equal(value, "Lanczos3") {
        Ok(ResizeFilter::Lanczos3)
    } else {
        Err(SwwwParseError::InvalidFilter)
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

fn parse_position(value: &str) -> Result<TransitionPosition, SwwwParseError> {
    let alias = if equal(value, "center") {
        Some((5_000, 5_000))
    } else if equal(value, "top") {
        Some((5_000, 10_000))
    } else if equal(value, "bottom") {
        Some((5_000, 0))
    } else if equal(value, "left") {
        Some((0, 5_000))
    } else if equal(value, "right") {
        Some((10_000, 5_000))
    } else if equal(value, "top-left") {
        Some((0, 10_000))
    } else if equal(value, "top-right") {
        Some((10_000, 10_000))
    } else if equal(value, "bottom-left") {
        Some((0, 0))
    } else if equal(value, "bottom-right") {
        Some((10_000, 0))
    } else {
        None
    };
    if let Some((x, y)) = alias {
        return Ok(TransitionPosition {
            x: TransitionCoordinate::Percent(x),
            y: TransitionCoordinate::Percent(y),
        });
    }

    let Some((x, y)) = value.split_once(',') else {
        return Err(SwwwParseError::InvalidPosition);
    };
    if y.contains(',') {
        return Err(SwwwParseError::InvalidPosition);
    }
    Ok(TransitionPosition {
        x: parse_coordinate(x.trim())?,
        y: parse_coordinate(y.trim())?,
    })
}

fn parse_coordinate(value: &str) -> Result<TransitionCoordinate, SwwwParseError> {
    if value.contains('.') {
        parse_decimal_fixed(value)
            .map(TransitionCoordinate::Percent)
            .map_err(|_| SwwwParseError::InvalidPosition)
    } else {
        parse_u32(value)
            .map(TransitionCoordinate::Pixel)
            .map_err(|_| SwwwParseError::InvalidPosition)
    }
}

fn parse_decimal_fixed(value: &str) -> Result<u32, SwwwParseError> {
    let Some((whole, fraction)) = value.split_once('.') else {
        return Err(SwwwParseError::InvalidNumber);
    };
    if fraction.contains('.') || (whole.is_empty() && fraction.is_empty()) {
        return Err(SwwwParseError::InvalidNumber);
    }
    let whole = if whole.is_empty() {
        0
    } else {
        parse_u32(whole)?
    };
    let mut fractional = 0u32;
    let mut digits = 0u32;
    for byte in fraction.bytes() {
        if !byte.is_ascii_digit() {
            return Err(SwwwParseError::InvalidNumber);
        }
        if digits < 4 {
            fractional = fractional * 10 + u32::from(byte - b'0');
            digits += 1;
        }
    }
    while digits < 4 {
        fractional *= 10;
        digits += 1;
    }
    whole
        .checked_mul(POSITION_SCALE)
        .and_then(|value| value.checked_add(fractional))
        .ok_or(SwwwParseError::InvalidNumber)
}

fn parse_duration_milliseconds(value: &str) -> Result<u32, SwwwParseError> {
    let (seconds, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.contains('.') || (seconds.is_empty() && fraction.is_empty()) {
        return Err(SwwwParseError::InvalidNumber);
    }
    let seconds = if seconds.is_empty() {
        0
    } else {
        parse_u32(seconds)?
    };
    let mut milliseconds = 0u32;
    let mut digits = 0u8;
    for byte in fraction.bytes() {
        if !byte.is_ascii_digit() {
            return Err(SwwwParseError::InvalidNumber);
        }
        if digits < 3 {
            milliseconds = milliseconds * 10 + u32::from(byte - b'0');
            digits += 1;
        }
    }
    while digits < 3 {
        milliseconds *= 10;
        digits += 1;
    }
    seconds
        .checked_mul(1_000)
        .and_then(|value| value.checked_add(milliseconds))
        .ok_or(SwwwParseError::InvalidNumber)
}

fn parse_bezier(value: &str) -> Result<TransitionBezier, SwwwParseError> {
    let mut components = value.split(',');
    let bezier = TransitionBezier {
        x1: parse_signed_fixed(
            components.next().ok_or(SwwwParseError::InvalidBezier)?,
            TRANSITION_FIXED_SCALE,
        )?,
        y1: parse_signed_fixed(
            components.next().ok_or(SwwwParseError::InvalidBezier)?,
            TRANSITION_FIXED_SCALE,
        )?,
        x2: parse_signed_fixed(
            components.next().ok_or(SwwwParseError::InvalidBezier)?,
            TRANSITION_FIXED_SCALE,
        )?,
        y2: parse_signed_fixed(
            components.next().ok_or(SwwwParseError::InvalidBezier)?,
            TRANSITION_FIXED_SCALE,
        )?,
    };
    if components.next().is_some()
        || !(0..=TRANSITION_FIXED_SCALE).contains(&bezier.x1)
        || !(0..=TRANSITION_FIXED_SCALE).contains(&bezier.x2)
        || bezier
            == (TransitionBezier {
                x1: 0,
                y1: 0,
                x2: 0,
                y2: 0,
            })
    {
        return Err(SwwwParseError::InvalidBezier);
    }
    Ok(bezier)
}

fn parse_wave(value: &str) -> Result<TransitionWave, SwwwParseError> {
    let mut components = value.split(',');
    let width = parse_signed_fixed(
        components.next().ok_or(SwwwParseError::InvalidWave)?,
        TRANSITION_FIXED_SCALE,
    )
    .map_err(|_| SwwwParseError::InvalidWave)?;
    let height = parse_signed_fixed(
        components.next().ok_or(SwwwParseError::InvalidWave)?,
        TRANSITION_FIXED_SCALE,
    )
    .map_err(|_| SwwwParseError::InvalidWave)?;
    if components.next().is_some() || width <= 0 {
        return Err(SwwwParseError::InvalidWave);
    }
    Ok(TransitionWave {
        width: width as u32,
        height,
    })
}

fn parse_signed_fixed(value: &str, scale: i32) -> Result<i32, SwwwParseError> {
    let value = value.trim();
    let (negative, magnitude) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else if let Some(value) = value.strip_prefix('+') {
        (false, value)
    } else {
        (false, value)
    };
    let (whole, fraction) = magnitude.split_once('.').unwrap_or((magnitude, ""));
    if fraction.contains('.') || (whole.is_empty() && fraction.is_empty()) {
        return Err(SwwwParseError::InvalidBezier);
    }
    let whole = if whole.is_empty() {
        0
    } else {
        i64::from(parse_u32(whole).map_err(|_| SwwwParseError::InvalidBezier)?)
    };
    let digits_in_scale = 4u8;
    let mut fractional = 0i64;
    let mut digits = 0u8;
    for byte in fraction.bytes() {
        if !byte.is_ascii_digit() {
            return Err(SwwwParseError::InvalidBezier);
        }
        if digits < digits_in_scale {
            fractional = fractional * 10 + i64::from(byte - b'0');
            digits += 1;
        }
    }
    while digits < digits_in_scale {
        fractional *= 10;
        digits += 1;
    }
    let magnitude = whole
        .checked_mul(i64::from(scale))
        .and_then(|value| value.checked_add(fractional))
        .ok_or(SwwwParseError::InvalidBezier)?;
    let signed = if negative { -magnitude } else { magnitude };
    i32::try_from(signed).map_err(|_| SwwwParseError::InvalidBezier)
}

fn parse_bool(value: &str) -> Result<bool, SwwwParseError> {
    if equal(value, "true") {
        Ok(true)
    } else if equal(value, "false") {
        Ok(false)
    } else {
        Err(SwwwParseError::InvalidBoolean)
    }
}

fn parse_crop_gravity(value: &str) -> Result<CropGravity, SwwwParseError> {
    if equal(value, "center") {
        Ok(CropGravity::Center)
    } else if equal(value, "top-left") {
        Ok(CropGravity::TopLeft)
    } else if equal(value, "top") {
        Ok(CropGravity::Top)
    } else if equal(value, "top-right") {
        Ok(CropGravity::TopRight)
    } else if equal(value, "left") {
        Ok(CropGravity::Left)
    } else if equal(value, "right") {
        Ok(CropGravity::Right)
    } else if equal(value, "bottom-left") {
        Ok(CropGravity::BottomLeft)
    } else if equal(value, "bottom") {
        Ok(CropGravity::Bottom)
    } else if equal(value, "bottom-right") {
        Ok(CropGravity::BottomRight)
    } else {
        Err(SwwwParseError::InvalidCropGravity)
    }
}

fn parse_u16(value: &str) -> Result<u16, SwwwParseError> {
    u16::try_from(parse_u32(value)?).map_err(|_| SwwwParseError::InvalidNumber)
}

fn parse_u32(value: &str) -> Result<u32, SwwwParseError> {
    if value.is_empty() {
        return Err(SwwwParseError::InvalidNumber);
    }
    let mut parsed = 0u32;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(SwwwParseError::InvalidNumber);
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(u32::from(byte - b'0')))
            .ok_or(SwwwParseError::InvalidNumber)?;
    }
    Ok(parsed)
}

fn coordinate_to_pixel(coordinate: TransitionCoordinate, length: u32) -> i64 {
    match coordinate {
        TransitionCoordinate::Pixel(value) => i64::from(value),
        TransitionCoordinate::Percent(value) => {
            i64::from(value) * i64::from(length) / i64::from(POSITION_SCALE)
        }
    }
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn parse_hex_color(value: &str) -> Result<u32, SwwwParseError> {
    if value.len() != 6 {
        return Err(SwwwParseError::InvalidColor);
    }
    let mut color = 0u32;
    for byte in value.bytes() {
        let component = match byte {
            b'0'..=b'9' => u32::from(byte - b'0'),
            b'a'..=b'f' => u32::from(byte - b'a' + 10),
            b'A'..=b'F' => u32::from(byte - b'A' + 10),
            _ => return Err(SwwwParseError::InvalidColor),
        };
        color = color << 4 | component;
    }
    Ok(color)
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

    fn set_color(&mut self, color: u32) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        self.bytes.fill(0);
        self.bytes[0] = b'0';
        self.bytes[1] = b'x';
        for index in 0..6 {
            let shift = (5 - index) * 4;
            self.bytes[index + 2] = HEX[((color >> shift) & 0x0f) as usize];
        }
        self.length = 8;
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
    clear_color: Option<u32>,
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
                duration_milliseconds: 3_000,
                angle_degrees: 45,
                position: TransitionPosition::center(),
                invert_y: false,
                bezier: TransitionBezier::swww_default(),
                wave: TransitionWave::swww_default(),
                resize: ResizeMode::Crop,
                filter: ResizeFilter::Nearest,
                crop_gravity: CropGravity::Center,
                fill_color: 0,
            },
            progress: 0,
            transition_active: false,
            generation: 0,
            clear_color: None,
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
        self.clear_color = None;
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
        self.clear_color = None;
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
        self.clear_color = None;
        self.transition = request.transition;
        self.generation = self.generation.wrapping_add(1);
        resolve_transition(&mut self.transition, self.generation);
        self.transition_active =
            self.previous.get().is_some() && self.transition.kind != TransitionType::None;
        self.progress = if self.transition_active { 0 } else { u8::MAX };
        Ok(())
    }

    pub fn clear(&mut self, request: ClearRequest<'_>) -> Result<(), SwwwDaemonError> {
        if !self.running {
            return Err(SwwwDaemonError::NotRunning);
        }
        if let Some(output) = request.output
            && output != "*"
            && !output.eq_ignore_ascii_case(self.output)
        {
            return Err(SwwwDaemonError::UnknownOutput);
        }
        self.current.set_color(request.color);
        self.previous.clear();
        self.transition_active = false;
        self.progress = u8::MAX;
        self.clear_color = Some(request.color);
        self.generation = self.generation.wrapping_add(1);
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
        if self.running && self.clear_color.is_none() {
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

    pub const fn clear_color(&self) -> Option<u32> {
        if self.running { self.clear_color } else { None }
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

fn resolve_transition(transition: &mut TransitionOptions, generation: u32) {
    const RANDOM_TYPES: [TransitionType; 8] = [
        TransitionType::Simple,
        TransitionType::Wipe,
        TransitionType::Wave,
        TransitionType::Grow,
        TransitionType::Outer,
        TransitionType::Fade,
        TransitionType::Left,
        TransitionType::Top,
    ];
    match transition.kind {
        TransitionType::Any => {
            transition.position = pseudo_random_position(generation);
            if generation & 1 == 0 {
                transition.kind = TransitionType::Grow;
            } else {
                transition.kind = TransitionType::Outer;
            }
        }
        TransitionType::Random => {
            transition.position = pseudo_random_position(generation);
            transition.angle_degrees = ((generation.wrapping_mul(137)) % 360) as u16;
            transition.kind = RANDOM_TYPES[generation as usize % RANDOM_TYPES.len()];
        }
        _ => {}
    }
}

fn pseudo_random_position(generation: u32) -> TransitionPosition {
    TransitionPosition {
        x: TransitionCoordinate::Percent(generation.wrapping_mul(7_919) % 10_001),
        y: TransitionCoordinate::Percent(generation.wrapping_mul(4_279) % 10_001),
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
    transition_pixel_with_options(
        TransitionOptions {
            kind,
            ..TransitionOptions::default()
        },
        progress,
        (i32::from(position.0), i32::from(position.1)),
        dimensions,
        old,
        new,
    )
}

pub fn transition_pixel_with_options(
    options: TransitionOptions,
    progress: u8,
    position: (i32, i32),
    dimensions: (u16, u16),
    old: u32,
    new: u32,
) -> u32 {
    if options.kind == TransitionType::None {
        return new;
    }
    if progress == 0 {
        return old;
    }
    if progress == u8::MAX {
        return new;
    }
    let progress = transition_eased_progress(options, progress);
    match options.kind {
        TransitionType::Simple | TransitionType::Fade => blend(old, new, progress),
        TransitionType::Left => {
            choose(wipe_revealed(180, progress, position, dimensions), old, new)
        }
        TransitionType::Right => choose(wipe_revealed(0, progress, position, dimensions), old, new),
        TransitionType::Top => choose(wipe_revealed(90, progress, position, dimensions), old, new),
        TransitionType::Bottom => {
            choose(wipe_revealed(270, progress, position, dimensions), old, new)
        }
        TransitionType::Wipe => choose(
            wipe_revealed(options.angle_degrees, progress, position, dimensions),
            old,
            new,
        ),
        TransitionType::Wave => choose(
            wave_revealed(
                options.angle_degrees,
                options.wave,
                progress,
                position,
                dimensions,
            ),
            old,
            new,
        ),
        TransitionType::Grow => choose(
            radial_revealed(
                options.position.to_pixel(dimensions, options.invert_y),
                progress,
                position,
                dimensions,
                false,
            ),
            old,
            new,
        ),
        TransitionType::Center => choose(
            radial_revealed(
                TransitionPosition::center().to_pixel(dimensions, false),
                progress,
                position,
                dimensions,
                false,
            ),
            old,
            new,
        ),
        TransitionType::Outer => choose(
            radial_revealed(
                options.position.to_pixel(dimensions, options.invert_y),
                progress,
                position,
                dimensions,
                true,
            ),
            old,
            new,
        ),
        TransitionType::None | TransitionType::Any | TransitionType::Random => new,
    }
}

pub fn transition_eased_progress(options: TransitionOptions, progress: u8) -> u8 {
    if progress == 0
        || progress == u8::MAX
        || matches!(options.kind, TransitionType::None | TransitionType::Simple)
    {
        return progress;
    }
    let scale = i64::from(TRANSITION_FIXED_SCALE);
    let target_x = (i64::from(progress) * scale + i64::from(u8::MAX) / 2) / i64::from(u8::MAX);
    let mut low = 0i64;
    let mut high = scale;
    while low < high {
        let middle = (low + high) / 2;
        if cubic_bezier_coordinate(options.bezier.x1, options.bezier.x2, middle) < target_x {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let eased = cubic_bezier_coordinate(options.bezier.y1, options.bezier.y2, low).clamp(0, scale);
    ((eased * i64::from(u8::MAX) + scale / 2) / scale) as u8
}

fn cubic_bezier_coordinate(first: i32, second: i32, time: i64) -> i64 {
    let scale = i128::from(TRANSITION_FIXED_SCALE);
    let time = i128::from(time);
    let inverse = scale - time;
    let numerator = 3 * inverse * inverse * time * i128::from(first)
        + 3 * inverse * time * time * i128::from(second)
        + time * time * time * scale;
    (numerator / (scale * scale * scale)) as i64
}

fn choose(reveal: bool, old: u32, new: u32) -> u32 {
    if reveal { new } else { old }
}

fn wipe_revealed(angle: u16, progress: u8, position: (i32, i32), dimensions: (u16, u16)) -> bool {
    let (dx, dy) = wipe_direction(angle);
    let width = i64::from(dimensions.0.saturating_sub(1));
    let height = i64::from(dimensions.1.saturating_sub(1));
    let corners = [
        0,
        i64::from(dx) * width,
        i64::from(dy) * height,
        i64::from(dx) * width + i64::from(dy) * height,
    ];
    let mut minimum = corners[0];
    let mut maximum = corners[0];
    for value in &corners[1..] {
        minimum = minimum.min(*value);
        maximum = maximum.max(*value);
    }
    let raw = i64::from(dx) * i64::from(position.0) + i64::from(dy) * i64::from(position.1);
    if maximum == minimum {
        return true;
    }
    (raw - minimum).clamp(0, maximum - minimum) * 255 <= (maximum - minimum) * i64::from(progress)
}

fn wipe_direction(angle: u16) -> (i16, i16) {
    let angle = angle % 360;
    match angle {
        0..=89 => (-(90 - angle as i16), angle as i16),
        90..=179 => ((angle - 90) as i16, (180 - angle) as i16),
        180..=269 => ((270 - angle) as i16, -((angle - 180) as i16)),
        _ => (-((angle - 270) as i16), -((360 - angle) as i16)),
    }
}

fn wave_revealed(
    angle: u16,
    wave: TransitionWave,
    progress: u8,
    position: (i32, i32),
    dimensions: (u16, u16),
) -> bool {
    let (dx, dy) = wipe_direction(angle);
    let width = i64::from(dimensions.0.saturating_sub(1));
    let height = i64::from(dimensions.1.saturating_sub(1));
    let corners = [
        0,
        i64::from(dx) * width,
        i64::from(dy) * height,
        i64::from(dx) * width + i64::from(dy) * height,
    ];
    let mut minimum = corners[0];
    let mut maximum = corners[0];
    for value in &corners[1..] {
        minimum = minimum.min(*value);
        maximum = maximum.max(*value);
    }
    if maximum == minimum {
        return true;
    }
    let raw = i64::from(dx) * i64::from(position.0) + i64::from(dy) * i64::from(position.1);
    let magnitude = i64::from(dx.unsigned_abs().max(dy.unsigned_abs()).max(1));
    let tangent = -i64::from(dy) * i64::from(position.0) + i64::from(dx) * i64::from(position.1);
    let phase = (i128::from(tangent) * i128::from(TRANSITION_FIXED_SCALE) / i128::from(magnitude))
        .rem_euclid(i128::from(wave.width)) as u32;
    let sine = wave_sine(phase, wave.width);
    let wave_offset = i128::from(sine) * i128::from(wave.height) * i128::from(magnitude)
        / (i128::from(TRANSITION_FIXED_SCALE) * i128::from(TRANSITION_FIXED_SCALE));
    let boundary = i128::from(minimum)
        + i128::from(maximum - minimum) * i128::from(progress) / i128::from(u8::MAX)
        + wave_offset;
    i128::from(raw) <= boundary
}

fn wave_sine(phase: u32, period: u32) -> i32 {
    const SAMPLES: [i32; 17] = [
        0, 3_827, 7_071, 9_239, 10_000, 9_239, 7_071, 3_827, 0, -3_827, -7_071, -9_239, -10_000,
        -9_239, -7_071, -3_827, 0,
    ];
    let scaled = u64::from(phase) * 16;
    let index = (scaled / u64::from(period)) as usize;
    let remainder = scaled % u64::from(period);
    let start = i64::from(SAMPLES[index.min(15)]);
    let end = i64::from(SAMPLES[(index + 1).min(16)]);
    (start + (end - start) * remainder as i64 / i64::from(period)) as i32
}

fn radial_revealed(
    center: (i32, i32),
    progress: u8,
    position: (i32, i32),
    dimensions: (u16, u16),
    outer: bool,
) -> bool {
    let distance = squared_distance(position, center);
    let right = i32::from(dimensions.0.saturating_sub(1));
    let bottom = i32::from(dimensions.1.saturating_sub(1));
    let maximum = squared_distance((0, 0), center)
        .max(squared_distance((right, 0), center))
        .max(squared_distance((0, bottom), center))
        .max(squared_distance((right, bottom), center))
        .max(1);
    let threshold = if outer {
        u64::from(u8::MAX - progress)
    } else {
        u64::from(progress)
    };
    if outer {
        distance.saturating_mul(u64::from(u8::MAX).pow(2))
            >= maximum.saturating_mul(threshold.pow(2))
    } else {
        distance.saturating_mul(u64::from(u8::MAX).pow(2))
            <= maximum.saturating_mul(threshold.pow(2))
    }
}

fn squared_distance(left: (i32, i32), right: (i32, i32)) -> u64 {
    let x = i64::from(left.0) - i64::from(right.0);
    let y = i64::from(left.1) - i64::from(right.1);
    x.unsigned_abs()
        .saturating_mul(x.unsigned_abs())
        .saturating_add(y.unsigned_abs().saturating_mul(y.unsigned_abs()))
}

fn blend(old: u32, new: u32, progress: u8) -> u32 {
    let inverse = 255 - u32::from(progress);
    let progress = u32::from(progress);
    let red = (((old >> 16) & 0xff) * inverse + ((new >> 16) & 0xff) * progress) / 255;
    let green = (((old >> 8) & 0xff) * inverse + ((new >> 8) & 0xff) * progress) / 255;
    let blue = ((old & 0xff) * inverse + (new & 0xff) * progress) / 255;
    red << 16 | green << 8 | blue
}

pub fn resize_filter_sample(
    filter: ResizeFilter,
    pixels: &[u32],
    source_dimensions: (u16, u16),
    destination_position: (u32, u32),
    destination_dimensions: (u32, u32),
) -> Option<u32> {
    let source_width = usize::from(source_dimensions.0);
    let source_height = usize::from(source_dimensions.1);
    let destination_width = destination_dimensions.0;
    let destination_height = destination_dimensions.1;
    if source_width == 0
        || source_height == 0
        || destination_width == 0
        || destination_height == 0
        || destination_position.0 >= destination_width
        || destination_position.1 >= destination_height
        || pixels.len() < source_width.checked_mul(source_height)?
    {
        return None;
    }
    if filter == ResizeFilter::Nearest {
        let x = ((u64::from(destination_position.0) * 2 + 1) * source_width as u64
            / (u64::from(destination_width) * 2))
            .min(source_width as u64 - 1) as usize;
        let y = ((u64::from(destination_position.1) * 2 + 1) * source_height as u64
            / (u64::from(destination_height) * 2))
            .min(source_height as u64 - 1) as usize;
        return pixels.get(y * source_width + x).copied();
    }

    const SAMPLE_SCALE: i64 = 1 << 16;
    let coordinate = |position: u32, source: usize, destination: u32| {
        (((i64::from(position) * 2 + 1) * source as i64 * SAMPLE_SCALE)
            / (i64::from(destination) * 2))
            - SAMPLE_SCALE / 2
    };
    let source_x = coordinate(destination_position.0, source_width, destination_width);
    let source_y = coordinate(destination_position.1, source_height, destination_height);
    match filter {
        ResizeFilter::CatmullRom | ResizeFilter::Mitchell => Some(cubic_filter_color(
            filter,
            pixels,
            source_width,
            source_height,
            source_x,
            source_y,
            SAMPLE_SCALE,
        )),
        ResizeFilter::Lanczos3 => Some(lanczos3_filter_color(
            pixels,
            source_width,
            source_height,
            source_x,
            source_y,
            SAMPLE_SCALE,
        )),
        ResizeFilter::Bilinear => {
            let x0 = source_x.div_euclid(SAMPLE_SCALE);
            let y0 = source_y.div_euclid(SAMPLE_SCALE);
            let x1 = x0 + 1;
            let y1 = y0 + 1;
            let clamp_x = |x: i64| x.clamp(0, source_width as i64 - 1) as usize;
            let clamp_y = |y: i64| y.clamp(0, source_height as i64 - 1) as usize;
            let x_weight = source_x.rem_euclid(SAMPLE_SCALE);
            let y_weight = source_y.rem_euclid(SAMPLE_SCALE);
            Some(bilinear_color(
                pixels[clamp_y(y0) * source_width + clamp_x(x0)],
                pixels[clamp_y(y0) * source_width + clamp_x(x1)],
                pixels[clamp_y(y1) * source_width + clamp_x(x0)],
                pixels[clamp_y(y1) * source_width + clamp_x(x1)],
                x_weight,
                y_weight,
                SAMPLE_SCALE,
            ))
        }
        ResizeFilter::Nearest => unreachable!(),
    }
}

fn bilinear_color(
    top_left: u32,
    top_right: u32,
    bottom_left: u32,
    bottom_right: u32,
    x_weight: i64,
    y_weight: i64,
    scale: i64,
) -> u32 {
    let channel = |shift: u32| {
        let value = |color: u32| i64::from((color >> shift) & 0xffu32);
        let top = value(top_left) * (scale - x_weight) + value(top_right) * x_weight;
        let bottom = value(bottom_left) * (scale - x_weight) + value(bottom_right) * x_weight;
        ((top * (scale - y_weight) + bottom * y_weight + scale * scale / 2) / (scale * scale))
            as u32
    };
    channel(16) << 16 | channel(8) << 8 | channel(0)
}

fn cubic_filter_color(
    filter: ResizeFilter,
    pixels: &[u32],
    source_width: usize,
    source_height: usize,
    source_x: i64,
    source_y: i64,
    sample_scale: i64,
) -> u32 {
    const WEIGHT_SCALE: i64 = 1 << 16;

    let base_x = source_x.div_euclid(sample_scale);
    let base_y = source_y.div_euclid(sample_scale);
    let mut x_indices = [0usize; 4];
    let mut y_indices = [0usize; 4];
    let mut x_weights = [0i64; 4];
    let mut y_weights = [0i64; 4];
    for (index, offset) in (-1..=2).enumerate() {
        let tap_x = base_x + offset;
        x_indices[index] = tap_x.clamp(0, source_width as i64 - 1) as usize;
        x_weights[index] = cubic_filter_weight(
            filter,
            (tap_x * sample_scale - source_x).unsigned_abs(),
            sample_scale,
            WEIGHT_SCALE,
        );

        let tap_y = base_y + offset;
        y_indices[index] = tap_y.clamp(0, source_height as i64 - 1) as usize;
        y_weights[index] = cubic_filter_weight(
            filter,
            (tap_y * sample_scale - source_y).unsigned_abs(),
            sample_scale,
            WEIGHT_SCALE,
        );
    }
    let fallback = pixels[base_y.clamp(0, source_height as i64 - 1) as usize * source_width
        + base_x.clamp(0, source_width as i64 - 1) as usize];
    separable_filter_color(
        pixels,
        source_width,
        &x_indices,
        &y_indices,
        &x_weights,
        &y_weights,
        fallback,
    )
}

fn cubic_filter_weight(
    filter: ResizeFilter,
    distance: u64,
    sample_scale: i64,
    weight_scale: i64,
) -> i64 {
    let x = i128::from(distance);
    let scale = i128::from(sample_scale);
    if x >= scale * 2 {
        return 0;
    }
    let x_squared = x * x;
    let x_cubed = x_squared * x;
    let scale_squared = scale * scale;
    let scale_cubed = scale_squared * scale;
    let (numerator, denominator) = match (filter, x < scale) {
        (ResizeFilter::CatmullRom, true) => (
            3 * x_cubed - 5 * x_squared * scale + 2 * scale_cubed,
            2 * scale_cubed,
        ),
        (ResizeFilter::CatmullRom, false) => (
            -x_cubed + 5 * x_squared * scale - 8 * x * scale_squared + 4 * scale_cubed,
            2 * scale_cubed,
        ),
        (ResizeFilter::Mitchell, true) => (
            21 * x_cubed - 36 * x_squared * scale + 16 * scale_cubed,
            18 * scale_cubed,
        ),
        (ResizeFilter::Mitchell, false) => (
            -7 * x_cubed + 36 * x_squared * scale - 60 * x * scale_squared + 32 * scale_cubed,
            18 * scale_cubed,
        ),
        _ => unreachable!(),
    };
    rounded_divide(numerator * i128::from(weight_scale), denominator) as i64
}

fn lanczos3_filter_color(
    pixels: &[u32],
    source_width: usize,
    source_height: usize,
    source_x: i64,
    source_y: i64,
    sample_scale: i64,
) -> u32 {
    const WEIGHT_SCALE: i64 = 1 << 16;

    let base_x = source_x.div_euclid(sample_scale);
    let base_y = source_y.div_euclid(sample_scale);
    let mut x_indices = [0usize; 6];
    let mut y_indices = [0usize; 6];
    let mut x_weights = [0i64; 6];
    let mut y_weights = [0i64; 6];
    for (index, offset) in (-2..=3).enumerate() {
        let tap_x = base_x + offset;
        x_indices[index] = tap_x.clamp(0, source_width as i64 - 1) as usize;
        x_weights[index] = lanczos3_filter_weight(
            (tap_x * sample_scale - source_x).unsigned_abs(),
            sample_scale,
            WEIGHT_SCALE,
        );

        let tap_y = base_y + offset;
        y_indices[index] = tap_y.clamp(0, source_height as i64 - 1) as usize;
        y_weights[index] = lanczos3_filter_weight(
            (tap_y * sample_scale - source_y).unsigned_abs(),
            sample_scale,
            WEIGHT_SCALE,
        );
    }
    let fallback = pixels[base_y.clamp(0, source_height as i64 - 1) as usize * source_width
        + base_x.clamp(0, source_width as i64 - 1) as usize];
    separable_filter_color(
        pixels,
        source_width,
        &x_indices,
        &y_indices,
        &x_weights,
        &y_weights,
        fallback,
    )
}

fn lanczos3_filter_weight(distance: u64, sample_scale: i64, weight_scale: i64) -> i64 {
    const PI_Q16: i128 = 205_887;

    if distance == 0 {
        return weight_scale;
    }
    if distance >= (sample_scale * 3) as u64 {
        return 0;
    }
    let sine = i128::from(sin_pi_fixed(distance, sample_scale));
    let window_sine = i128::from(sin_pi_fixed(distance / 3, sample_scale));
    let scale = i128::from(sample_scale);
    let distance = i128::from(distance);
    let numerator = 3 * sine * window_sine * scale * scale * i128::from(weight_scale);
    let denominator = PI_Q16 * PI_Q16 * distance * distance;
    rounded_divide(numerator, denominator) as i64
}

fn sin_pi_fixed(argument: u64, sample_scale: i64) -> i64 {
    const TABLE: [i64; 33] = [
        0, 6_424, 12_785, 19_024, 25_080, 30_893, 36_410, 41_576, 46_341, 50_660, 54_491, 57_798,
        60_547, 62_714, 64_277, 65_220, 65_536, 65_220, 64_277, 62_714, 60_547, 57_798, 54_491,
        50_660, 46_341, 41_576, 36_410, 30_893, 25_080, 19_024, 12_785, 6_424, 0,
    ];
    const STEPS: u64 = 32;

    let sample_scale = sample_scale as u64;
    let whole = argument / sample_scale;
    let scaled_fraction = argument % sample_scale * STEPS;
    let index = (scaled_fraction / sample_scale) as usize;
    let remainder = (scaled_fraction % sample_scale) as i64;
    let value = (TABLE[index] * (sample_scale as i64 - remainder)
        + TABLE[index + 1] * remainder
        + sample_scale as i64 / 2)
        / sample_scale as i64;
    if whole % 2 == 0 { value } else { -value }
}

fn separable_filter_color<const TAPS: usize>(
    pixels: &[u32],
    source_width: usize,
    x_indices: &[usize; TAPS],
    y_indices: &[usize; TAPS],
    x_weights: &[i64; TAPS],
    y_weights: &[i64; TAPS],
    fallback: u32,
) -> u32 {
    let mut total_weight = 0i128;
    let mut channels = [0i128; 3];
    for (y, &y_weight) in y_indices.iter().zip(y_weights) {
        for (x, &x_weight) in x_indices.iter().zip(x_weights) {
            let weight = i128::from(x_weight) * i128::from(y_weight);
            let color = pixels[*y * source_width + *x];
            total_weight += weight;
            channels[0] += i128::from((color >> 16) & 0xff) * weight;
            channels[1] += i128::from((color >> 8) & 0xff) * weight;
            channels[2] += i128::from(color & 0xff) * weight;
        }
    }
    if total_weight <= 0 {
        return fallback;
    }
    let channel =
        |value: i128| rounded_divide(value, total_weight).clamp(0, i128::from(u8::MAX)) as u32;
    channel(channels[0]) << 16 | channel(channels[1]) << 8 | channel(channels[2])
}

fn rounded_divide(numerator: i128, denominator: i128) -> i128 {
    if numerator < 0 {
        (numerator - denominator / 2) / denominator
    } else {
        (numerator + denominator / 2) / denominator
    }
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
pub enum PpmFormat {
    Plain,
    Binary,
}

impl PpmFormat {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Plain => "P3",
            Self::Binary => "P6",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PpmPixelData<'a> {
    Plain(&'a [u8]),
    Binary(&'a [u8]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PpmImage<'a> {
    width: u16,
    height: u16,
    maximum: u16,
    data: PpmPixelData<'a>,
}

impl<'a> PpmImage<'a> {
    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub const fn format(self) -> PpmFormat {
        match self.data {
            PpmPixelData::Plain(_) => PpmFormat::Plain,
            PpmPixelData::Binary(_) => PpmFormat::Binary,
        }
    }

    pub fn pixels(self) -> PpmPixels<'a> {
        PpmPixels {
            source: match self.data {
                PpmPixelData::Plain(bytes) => PpmPixelSource::Plain(PpmTokens::new(bytes)),
                PpmPixelData::Binary(bytes) => PpmPixelSource::Binary { bytes, offset: 0 },
            },
            maximum: self.maximum,
        }
    }
}

pub fn parse_ppm(input: &str) -> Result<PpmImage<'_>, PpmError> {
    parse_ppm_bytes(input.as_bytes())
}

pub fn parse_ppm_bytes(input: &[u8]) -> Result<PpmImage<'_>, PpmError> {
    let mut tokens = PpmTokens::new(input);
    let format = match tokens.next() {
        Some(b"P3") => PpmFormat::Plain,
        Some(b"P6") => PpmFormat::Binary,
        _ => return Err(PpmError::InvalidMagic),
    };
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
    let data = match format {
        PpmFormat::Plain => {
            for _ in 0..expected_components {
                let value = parse_ppm_number(tokens.next().ok_or(PpmError::TruncatedPixels)?)?;
                if value > maximum {
                    return Err(PpmError::InvalidColor);
                }
            }
            if tokens.next().is_some() {
                return Err(PpmError::ExtraPixels);
            }
            PpmPixelData::Plain(&input[pixel_start..])
        }
        PpmFormat::Binary => {
            let pixel_start = binary_pixel_start(input, tokens.offset)?;
            let pixel_end = pixel_start
                .checked_add(expected_components)
                .ok_or(PpmError::InvalidDimensions)?;
            if pixel_end > input.len() {
                return Err(PpmError::TruncatedPixels);
            }
            if pixel_end < input.len() {
                return Err(PpmError::ExtraPixels);
            }
            let pixels = &input[pixel_start..pixel_end];
            if pixels.iter().any(|value| u16::from(*value) > maximum) {
                return Err(PpmError::InvalidColor);
            }
            PpmPixelData::Binary(pixels)
        }
    };
    Ok(PpmImage {
        width,
        height,
        maximum,
        data,
    })
}

fn binary_pixel_start(input: &[u8], delimiter: usize) -> Result<usize, PpmError> {
    let Some(byte) = input.get(delimiter) else {
        return Err(PpmError::TruncatedPixels);
    };
    if !byte.is_ascii_whitespace() {
        return Err(PpmError::MissingHeader);
    }
    if *byte == b'\r' && input.get(delimiter + 1) == Some(&b'\n') {
        Ok(delimiter + 2)
    } else {
        Ok(delimiter + 1)
    }
}

enum PpmPixelSource<'a> {
    Plain(PpmTokens<'a>),
    Binary { bytes: &'a [u8], offset: usize },
}

pub struct PpmPixels<'a> {
    source: PpmPixelSource<'a>,
    maximum: u16,
}

impl Iterator for PpmPixels<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let (red, green, blue) = match &mut self.source {
            PpmPixelSource::Plain(tokens) => (
                parse_ppm_number(tokens.next()?).ok()?,
                parse_ppm_number(tokens.next()?).ok()?,
                parse_ppm_number(tokens.next()?).ok()?,
            ),
            PpmPixelSource::Binary { bytes, offset } => {
                let end = offset.checked_add(3)?;
                let pixel = bytes.get(*offset..end)?;
                *offset = end;
                (
                    u16::from(pixel[0]),
                    u16::from(pixel[1]),
                    u16::from(pixel[2]),
                )
            }
        };
        let scale = |value: u16| u32::from(value) * 255 / u32::from(self.maximum);
        Some(scale(red) << 16 | scale(green) << 8 | scale(blue))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RasterPixelData<'a> {
    Pnm(PpmImage<'a>),
    Rgb(&'a [u8]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterImage<'a> {
    width: u16,
    height: u16,
    data: RasterPixelData<'a>,
}

impl<'a> RasterImage<'a> {
    pub const fn from_pnm(image: PpmImage<'a>) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            data: RasterPixelData::Pnm(image),
        }
    }

    pub fn from_rgb(width: u16, height: u16, bytes: &'a [u8]) -> Option<Self> {
        let expected = usize::from(width)
            .checked_mul(usize::from(height))?
            .checked_mul(3)?;
        if width == 0 || height == 0 || bytes.len() != expected {
            return None;
        }
        Some(Self {
            width,
            height,
            data: RasterPixelData::Rgb(bytes),
        })
    }

    pub const fn width(self) -> u16 {
        self.width
    }

    pub const fn height(self) -> u16 {
        self.height
    }

    pub fn pixels(self) -> RasterPixels<'a> {
        RasterPixels {
            source: match self.data {
                RasterPixelData::Pnm(image) => RasterPixelSource::Pnm(image.pixels()),
                RasterPixelData::Rgb(bytes) => RasterPixelSource::Rgb { bytes, offset: 0 },
            },
        }
    }
}

enum RasterPixelSource<'a> {
    Pnm(PpmPixels<'a>),
    Rgb { bytes: &'a [u8], offset: usize },
}

pub struct RasterPixels<'a> {
    source: RasterPixelSource<'a>,
}

impl Iterator for RasterPixels<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.source {
            RasterPixelSource::Pnm(pixels) => pixels.next(),
            RasterPixelSource::Rgb { bytes, offset } => {
                let end = offset.checked_add(3)?;
                let pixel = bytes.get(*offset..end)?;
                *offset = end;
                Some(u32::from(pixel[0]) << 16 | u32::from(pixel[1]) << 8 | u32::from(pixel[2]))
            }
        }
    }
}

struct PpmTokens<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> PpmTokens<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn next(&mut self) -> Option<&'a [u8]> {
        let bytes = self.input;
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

fn parse_ppm_number(value: &[u8]) -> Result<u16, PpmError> {
    if value.is_empty() {
        return Err(PpmError::InvalidNumber);
    }
    let mut parsed = 0u16;
    for &byte in value {
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
            "swww img -o SLOPOS-1 sunset.ppm --transition-type center --transition-step 64 --transition-fps 60 --resize fit --filter Bilinear",
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
        assert_eq!(request.transition.filter, ResizeFilter::Bilinear);
        let SwwwCommand::Img(request) = parse_swww_command(
            "img aurora.ppm -t none --resize stretch --crop-gravity bottom-right --fill-color 1a2b3c -f Lanczos3",
            SwwwDefaults::default(),
        )
        .unwrap() else {
            panic!("expected image request");
        };
        assert_eq!(request.transition.resize, ResizeMode::Stretch);
        assert_eq!(request.transition.filter, ResizeFilter::Lanczos3);
        assert_eq!(request.transition.kind, TransitionType::None);
        assert_eq!(request.transition.crop_gravity, CropGravity::BottomRight);
        assert_eq!(request.transition.fill_color, 0x1a2b3c);
        let SwwwCommand::Img(request) = parse_swww_command(
            "img aurora.ppm --resize fit --no-resize",
            SwwwDefaults::default(),
        )
        .unwrap() else {
            panic!("expected image request");
        };
        assert_eq!(request.transition.resize, ResizeMode::No);
        assert_eq!(
            parse_swww_command("swww clear", SwwwDefaults::default()),
            Ok(SwwwCommand::Clear(ClearRequest {
                color: 0,
                output: None,
            }))
        );
        assert_eq!(
            parse_swww_command("clear -o SLOPOS-1 1a804a", SwwwDefaults::default()),
            Ok(SwwwCommand::Clear(ClearRequest {
                color: 0x1a804a,
                output: Some("SLOPOS-1"),
            }))
        );
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
            "SWWW_TRANSITION=grow\nSWWW_TRANSITION_STEP=33\nSWWW_TRANSITION_FPS=60\nSWWW_TRANSITION_DURATION=2.125\nSWWW_TRANSITION_ANGLE=120\nSWWW_TRANSITION_POS=top-right\nSWWW_INVERT_Y=true\nSWWW_TRANSITION_BEZIER=.25,-.1,.75,1.2\nSWWW_TRANSITION_WAVE=40.5,-2.25\n",
        )
        .unwrap();
        assert_eq!(defaults.transition.kind, TransitionType::Grow);
        assert_eq!(defaults.transition.step, 33);
        assert_eq!(defaults.transition.fps, 60);
        assert_eq!(defaults.transition.duration_milliseconds, 2_125);
        assert_eq!(defaults.transition.angle_degrees, 120);
        assert_eq!(
            defaults.transition.position,
            TransitionPosition {
                x: TransitionCoordinate::Percent(10_000),
                y: TransitionCoordinate::Percent(10_000),
            }
        );
        assert!(defaults.transition.invert_y);
        assert_eq!(
            defaults.transition.bezier,
            TransitionBezier {
                x1: 2_500,
                y1: -1_000,
                x2: 7_500,
                y2: 12_000,
            }
        );
        assert_eq!(
            defaults.transition.wave,
            TransitionWave {
                width: 405_000,
                height: -22_500,
            }
        );

        let SwwwCommand::Img(request) = parse_swww_command(
            "img image.ppm --transition-type wave --transition-angle 30 --transition-pos 0.25,0.75 --invert-y false --transition-bezier 0,0,1,0 --transition-wave 40,24",
            defaults,
        )
        .unwrap()
        else {
            panic!("expected image request");
        };
        assert_eq!(request.transition.kind, TransitionType::Wave);
        assert_eq!(request.transition.step, TransitionType::Wave.default_step());
        assert_eq!(request.transition.angle_degrees, 30);
        assert_eq!(
            request.transition.position,
            TransitionPosition {
                x: TransitionCoordinate::Percent(2_500),
                y: TransitionCoordinate::Percent(7_500),
            }
        );
        assert!(!request.transition.invert_y);
        assert_eq!(
            request.transition.bezier,
            TransitionBezier {
                x1: 0,
                y1: 0,
                x2: 10_000,
                y2: 0,
            }
        );
        assert_eq!(
            request.transition.wave,
            TransitionWave {
                width: 400_000,
                height: 240_000,
            }
        );
        let SwwwCommand::Img(request) = parse_swww_command(
            "img image.ppm --transition-duration .25",
            SwwwDefaults::default(),
        )
        .unwrap() else {
            panic!("expected image request");
        };
        assert_eq!(request.transition.duration_milliseconds, 250);
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
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --transition-pos middle",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidPosition)
        );
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --transition-pos 0.2,wat",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidPosition)
        );
        assert_eq!(
            parse_swww_command("swww img one.ppm --invert-y yes", SwwwDefaults::default()),
            Err(SwwwParseError::InvalidBoolean)
        );
        assert_eq!(
            parse_swww_command("swww img one.ppm --resize squish", SwwwDefaults::default()),
            Err(SwwwParseError::InvalidResize)
        );
        assert_eq!(
            parse_swww_command("swww img one.ppm --filter blurry", SwwwDefaults::default()),
            Err(SwwwParseError::InvalidFilter)
        );
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --crop-gravity diagonal",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidCropGravity)
        );
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --fill-color 12345z",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidColor)
        );
        assert_eq!(
            parse_swww_command(
                "swww img one.ppm --transition-duration 1.two",
                SwwwDefaults::default()
            ),
            Err(SwwwParseError::InvalidNumber)
        );
        for bezier in ["0,0,0,0", "0,0,1", "-.1,0,1,1", "0,0,1.1,1", "0,a,1,1"] {
            assert_eq!(parse_bezier(bezier), Err(SwwwParseError::InvalidBezier));
        }
        for wave in ["0,20", "-1,20", "20", "20,20,20", "wide,20"] {
            assert_eq!(parse_wave(wave), Err(SwwwParseError::InvalidWave));
        }
        assert_eq!(
            parse_swww_command("swww clear #1a804a", SwwwDefaults::default()),
            Err(SwwwParseError::InvalidColor)
        );
        assert_eq!(
            parse_swww_command("swww clear fff", SwwwDefaults::default()),
            Err(SwwwParseError::InvalidColor)
        );
        assert_eq!(
            parse_swww_command("swww clear 1a80xz", SwwwDefaults::default()),
            Err(SwwwParseError::InvalidColor)
        );
        assert_eq!(
            parse_swww_command("swww clear 1a804a 000000", SwwwDefaults::default()),
            Err(SwwwParseError::UnexpectedArgument)
        );
        assert_eq!(
            parse_swww_command("swww clear -o", SwwwDefaults::default()),
            Err(SwwwParseError::MissingValue)
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

        daemon
            .clear(ClearRequest {
                color: 0x1a804a,
                output: Some("SLOPOS-1"),
            })
            .unwrap();
        assert_eq!(daemon.clear_color(), Some(0x1a804a));
        assert_eq!(daemon.current_image(), None);
        assert_eq!(daemon.previous_image(), None);
        assert_eq!(daemon.query().unwrap().image, "0x1A804A");
        assert!(!daemon.transition_active());
        assert_eq!(daemon.progress(), u8::MAX);
        assert_eq!(
            daemon.clear(ClearRequest {
                color: 0,
                output: Some("other"),
            }),
            Err(SwwwDaemonError::UnknownOutput)
        );

        daemon.apply(first).unwrap();
        assert_eq!(daemon.clear_color(), None);
        assert_eq!(daemon.current_image(), Some("aurora.ppm"));
        daemon.kill().unwrap();
        assert_eq!(daemon.current_image(), None);
        assert_eq!(daemon.clear_color(), None);
    }

    #[test]
    fn transition_masks_and_blending_reach_expected_pixels() {
        let old = 0x00_00_00;
        let new = 0xff_80_40;
        let linear = TransitionOptions {
            kind: TransitionType::Fade,
            bezier: TransitionBezier::linear(),
            ..TransitionOptions::default()
        };
        assert_eq!(transition_eased_progress(linear, 128), 128);
        let curved = TransitionOptions {
            bezier: TransitionBezier {
                x1: 0,
                y1: 0,
                x2: 10_000,
                y2: 0,
            },
            ..linear
        };
        assert_eq!(transition_eased_progress(curved, 128), 32);
        assert_eq!(
            transition_pixel_with_options(curved, 128, (0, 0), (4, 4), old, new),
            0x20_10_08
        );
        assert_eq!(
            transition_eased_progress(
                TransitionOptions {
                    kind: TransitionType::Simple,
                    ..curved
                },
                128
            ),
            128
        );
        let wave = TransitionOptions {
            kind: TransitionType::Wave,
            angle_degrees: 0,
            bezier: TransitionBezier::linear(),
            wave: TransitionWave {
                width: 200_000,
                height: 100_000,
            },
            ..TransitionOptions::default()
        };
        assert_eq!(
            transition_pixel_with_options(wave, 128, (50, 5), (100, 80), old, new),
            old
        );
        assert_eq!(
            transition_pixel_with_options(wave, 128, (50, 15), (100, 80), old, new),
            new
        );
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
        let center = TransitionOptions {
            kind: TransitionType::Center,
            bezier: TransitionBezier::linear(),
            ..TransitionOptions::default()
        };
        assert_eq!(
            transition_pixel_with_options(center, 1, (2, 2), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel_with_options(center, 254, (0, 0), (4, 4), old, new),
            old
        );
        assert_eq!(
            transition_pixel_with_options(center, 255, (0, 0), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel(TransitionType::None, 0, (0, 0), (4, 4), old, new),
            new
        );

        let mut options = TransitionOptions {
            kind: TransitionType::Wipe,
            angle_degrees: 0,
            bezier: TransitionBezier::linear(),
            ..TransitionOptions::default()
        };
        assert_eq!(
            transition_pixel_with_options(options, 90, (3, 0), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel_with_options(options, 90, (0, 0), (4, 4), old, new),
            old
        );
        options.angle_degrees = 180;
        assert_eq!(
            transition_pixel_with_options(options, 90, (0, 0), (4, 4), old, new),
            new
        );
        assert_eq!(
            transition_pixel_with_options(options, 90, (3, 0), (4, 4), old, new),
            old
        );

        options.kind = TransitionType::Grow;
        options.position = parse_position("top-left").unwrap();
        assert_eq!(options.position.to_pixel((100, 80), false), (0, 0));
        assert_eq!(
            transition_pixel_with_options(options, 1, (0, 0), (100, 80), old, new),
            new
        );
        assert_eq!(
            transition_pixel_with_options(options, 1, (99, 79), (100, 80), old, new),
            old
        );

        options.position = parse_position("20,10").unwrap();
        assert_eq!(options.position.to_pixel((100, 80), false), (20, 70));
        options.invert_y = true;
        assert_eq!(options.position.to_pixel((100, 80), true), (20, 10));
    }

    #[test]
    fn samples_distinct_resize_filters() {
        assert_eq!(lanczos3_filter_weight(0, 1 << 16, 1 << 16), 1 << 16);
        assert_eq!(lanczos3_filter_weight(1 << 16, 1 << 16, 1 << 16), 0);
        assert_eq!(lanczos3_filter_weight(2 << 16, 1 << 16, 1 << 16), 0);
        assert_eq!(lanczos3_filter_weight(3 << 16, 1 << 16, 1 << 16), 0);

        let pixels = [0xff_00_00, 0x00_ff_00, 0x00_00_ff, 0xff_ff_ff];
        assert_eq!(
            resize_filter_sample(ResizeFilter::Nearest, &pixels, (2, 2), (1, 1), (3, 3)),
            Some(0xff_ff_ff)
        );
        for filter in [
            ResizeFilter::Bilinear,
            ResizeFilter::CatmullRom,
            ResizeFilter::Mitchell,
            ResizeFilter::Lanczos3,
        ] {
            assert_eq!(
                resize_filter_sample(filter, &pixels, (2, 2), (1, 1), (3, 3)),
                Some(0x80_80_80)
            );
            assert_eq!(
                resize_filter_sample(filter, &pixels, (2, 2), (0, 0), (3, 3)),
                Some(0xff_00_00)
            );
        }
        assert_eq!(
            resize_filter_sample(ResizeFilter::Bilinear, &pixels[..3], (2, 2), (1, 1), (3, 3)),
            None
        );

        let detail = [
            0x00_00_00, 0xff_00_00, 0x00_ff_00, 0x00_00_ff, 0xff_ff_ff, 0x20_20_20, 0xe0_e0_e0,
            0x80_80_80, 0x11_22_33, 0x44_55_66, 0x77_88_99, 0xaa_bb_cc, 0xff_00_ff, 0x00_ff_ff,
            0xff_ff_00, 0x12_34_56,
        ];
        assert_eq!(
            resize_filter_sample(ResizeFilter::Bilinear, &detail, (4, 4), (2, 3), (7, 9)),
            Some(0x31_32_33)
        );
        assert_eq!(
            resize_filter_sample(ResizeFilter::CatmullRom, &detail, (4, 4), (2, 3), (7, 9)),
            Some(0x1f_26_26)
        );
        assert_eq!(
            resize_filter_sample(ResizeFilter::Mitchell, &detail, (4, 4), (2, 3), (7, 9)),
            Some(0x40_3a_3c)
        );
        assert_eq!(
            resize_filter_sample(ResizeFilter::Lanczos3, &detail, (4, 4), (2, 3), (7, 9)),
            Some(0x1d_23_23)
        );

        let aurora = parse_ppm(include_str!("../../../assets/wallpapers/aurora.ppm")).unwrap();
        let mut aurora_pixels = [0u32; 96];
        for (index, color) in aurora.pixels().enumerate() {
            aurora_pixels[index] = color;
        }
        assert_eq!(
            resize_filter_sample(
                ResizeFilter::Bilinear,
                &aurora_pixels,
                (aurora.width(), aurora.height()),
                (514, 302),
                (1024, 768)
            ),
            Some(0x2b_c5_ce)
        );
        assert_eq!(
            resize_filter_sample(
                ResizeFilter::CatmullRom,
                &aurora_pixels,
                (aurora.width(), aurora.height()),
                (514, 302),
                (1024, 768)
            ),
            Some(0x27_d2_d4)
        );
        assert_eq!(
            resize_filter_sample(
                ResizeFilter::Lanczos3,
                &aurora_pixels,
                (aurora.width(), aurora.height()),
                (514, 302),
                (1024, 768)
            ),
            Some(0x25_d5_d6)
        );
    }

    #[test]
    fn parses_bounded_plain_and_binary_ppm_pixels() {
        let image = parse_ppm("P3\n# tiny\n2 1\n15\n15 0 0  0 8 15\n").unwrap();
        assert_eq!(image.format(), PpmFormat::Plain);
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 1);
        let mut pixels = image.pixels();
        assert_eq!(pixels.next(), Some(0xff_00_00));
        assert_eq!(pixels.next(), Some(0x00_88_ff));
        assert_eq!(pixels.next(), None);
        assert_eq!(parse_ppm("P3 1 1 15 16 0 0"), Err(PpmError::InvalidColor));
        assert_eq!(parse_ppm("P3 1 1 15 1 2"), Err(PpmError::TruncatedPixels));
        assert_eq!(parse_ppm("P3 1 1 15 1 2 3 4"), Err(PpmError::ExtraPixels));

        let image = parse_ppm_bytes(b"P6\n# tiny\n2 1\n15\n\x0f\x00\x00\x00\x08\x0f").unwrap();
        assert_eq!(image.format(), PpmFormat::Binary);
        assert_eq!((image.width(), image.height()), (2, 1));
        let mut pixels = image.pixels();
        assert_eq!(pixels.next(), Some(0xff_00_00));
        assert_eq!(pixels.next(), Some(0x00_88_ff));
        assert_eq!(pixels.next(), None);
        assert_eq!(
            parse_ppm_bytes(b"P6\r\n1 1\r\n255\r\n\x11\x22\x33")
                .unwrap()
                .pixels()
                .next(),
            Some(0x11_22_33)
        );
        assert_eq!(
            parse_ppm_bytes(b"P6\n1 1\n255\n#\n ")
                .unwrap()
                .pixels()
                .next(),
            Some(0x23_0a_20)
        );
        assert_eq!(
            parse_ppm_bytes(b"P6\n1 1\n15\n\x10\x00\x00"),
            Err(PpmError::InvalidColor)
        );
        assert_eq!(
            parse_ppm_bytes(b"P6\n1 1\n255\n\x11\x22"),
            Err(PpmError::TruncatedPixels)
        );
        assert_eq!(
            parse_ppm_bytes(b"P6\n1 1\n255\n\x11\x22\x33\x44"),
            Err(PpmError::ExtraPixels)
        );
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
