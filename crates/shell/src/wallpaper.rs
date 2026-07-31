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
        let SwwwCommand::Img(request) = parse_swww_command(
            "img aurora.ppm -t none --resize stretch --crop-gravity bottom-right --fill-color 1a2b3c",
            SwwwDefaults::default(),
        )
        .unwrap() else {
            panic!("expected image request");
        };
        assert_eq!(request.transition.resize, ResizeMode::Stretch);
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
