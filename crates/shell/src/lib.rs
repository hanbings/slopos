// SPDX-License-Identifier: 0BSD

#![no_std]

mod niri;
mod wallpaper;
mod waybar;
mod waybar_style;

pub use niri::{
    BindingKey, BindingModifiers, MAX_NIRI_BINDINGS, MAX_NIRI_WINDOW_RULES, MAX_NIRI_WORKSPACES,
    NamedWorkspace, NamedWorkspaceList, NiriAction, NiriBinding, NiriBindingList, NiriConfigError,
    NiriShellConfig, NiriWindowRule, NiriWindowRuleList, WorkspaceError, WorkspaceSet,
    parse_niri_shell_config,
};
pub use wallpaper::{
    ImgRequest, MAX_WALLPAPER_PATH, PpmError, PpmImage, ResizeMode, SwwwCommand, SwwwDaemonError,
    SwwwDefaults, SwwwParseError, TransitionOptions, TransitionType, WallpaperDaemon,
    WallpaperQuery, parse_ppm, parse_swww_command, parse_swww_environment, transition_pixel,
};
pub use waybar::{
    BarConfigError, BarFormatError, BarFormatValue, BarModuleConfig, BarModuleConfigList,
    BarModuleList, BarPosition, BarText, MAX_BAR_MODULE_CONFIGS, MAX_BAR_MODULES, MAX_BAR_TEXT,
    WaybarConfig, format_bar_text, parse_waybar_config,
};
pub use waybar_style::{
    MAX_WAYBAR_STYLE_RULES, ResolvedWaybarStyle, WaybarStyle, WaybarStyleError, parse_waybar_style,
};

pub const DEFAULT_GAPS: u16 = 16;
pub const DEFAULT_FOCUS_RING_WIDTH: u16 = 4;
pub const DEFAULT_ACTIVE_COLOR: u32 = 0x7f_c8_ff;
pub const DEFAULT_INACTIVE_COLOR: u32 = 0x50_50_50;
pub const DEFAULT_BACKGROUND_COLOR: u32 = 0x10_14_26;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CenterFocusedColumn {
    Never,
    Always,
    OnOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnWidth {
    Proportion(u16),
    Fixed(u16),
    Client,
}

impl ColumnWidth {
    fn resolve(self, output_width: u16) -> u16 {
        match self {
            Self::Proportion(thousandths) => ((u32::from(output_width) * u32::from(thousandths))
                / 1000)
                .clamp(1, u32::from(u16::MAX)) as u16,
            Self::Fixed(width) => width,
            Self::Client => output_width / 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocusRing {
    pub enabled: bool,
    pub width: u16,
    pub active_color: u32,
    pub inactive_color: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutConfig {
    pub gaps: u16,
    pub center_focused_column: CenterFocusedColumn,
    pub always_center_single_column: bool,
    pub default_column_width: ColumnWidth,
    pub focus_ring: FocusRing,
    pub background_color: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            gaps: DEFAULT_GAPS,
            center_focused_column: CenterFocusedColumn::Never,
            always_center_single_column: false,
            default_column_width: ColumnWidth::Proportion(500),
            focus_ring: FocusRing {
                enabled: true,
                width: DEFAULT_FOCUS_RING_WIDTH,
                active_color: DEFAULT_ACTIVE_COLOR,
                inactive_color: DEFAULT_INACTIVE_COLOR,
            },
            background_color: DEFAULT_BACKGROUND_COLOR,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    MissingLayout,
    UnexpectedEnd,
    InvalidNumber,
    InvalidColor,
    InvalidCenterPolicy,
    InvalidColumnWidth,
    InvalidFocusRing,
}

pub fn parse_niri_layout(input: &str) -> Result<LayoutConfig, ConfigError> {
    ConfigParser::new(input).parse()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutError {
    DuplicateWindow,
    ColumnCapacity,
    WindowCapacity,
    UnknownWindow,
    NoFocusedWindow,
}

#[derive(Clone, Copy)]
struct Column<const WINDOWS: usize> {
    windows: [u32; WINDOWS],
    window_count: usize,
    focused_window: usize,
    width: u16,
}

impl<const WINDOWS: usize> Column<WINDOWS> {
    const fn empty() -> Self {
        Self {
            windows: [0; WINDOWS],
            window_count: 0,
            focused_window: 0,
            width: 0,
        }
    }
}

pub struct ScrollLayout<const COLUMNS: usize, const WINDOWS: usize> {
    config: LayoutConfig,
    output_width: u16,
    output_height: u16,
    reserved_top: u16,
    columns: [Column<WINDOWS>; COLUMNS],
    column_count: usize,
    focused_column: usize,
    view_offset: i32,
}

impl<const COLUMNS: usize, const WINDOWS: usize> ScrollLayout<COLUMNS, WINDOWS> {
    pub fn new(
        output_width: u16,
        output_height: u16,
        reserved_top: u16,
        config: LayoutConfig,
    ) -> Self {
        Self {
            config,
            output_width,
            output_height,
            reserved_top: reserved_top.min(output_height),
            columns: [Column::empty(); COLUMNS],
            column_count: 0,
            focused_column: 0,
            view_offset: 0,
        }
    }

    pub const fn config(&self) -> LayoutConfig {
        self.config
    }

    pub const fn len(&self) -> usize {
        self.column_count
    }

    pub const fn is_empty(&self) -> bool {
        self.column_count == 0
    }

    pub const fn focused_column(&self) -> Option<usize> {
        if self.column_count == 0 {
            None
        } else {
            Some(self.focused_column)
        }
    }

    pub fn focused_window(&self) -> Option<u32> {
        let column = self.columns.get(self.focused_column)?;
        if column.window_count == 0 {
            None
        } else {
            Some(column.windows[column.focused_window])
        }
    }

    pub fn focus_window(&mut self, window: u32) -> Result<(), LayoutError> {
        let (column, row) = self.find_window(window).ok_or(LayoutError::UnknownWindow)?;
        self.focused_column = column;
        self.columns[column].focused_window = row;
        self.ensure_focused_visible();
        Ok(())
    }

    pub const fn view_offset(&self) -> i32 {
        self.view_offset
    }

    pub fn open_window(&mut self, window: u32) -> Result<(), LayoutError> {
        self.reject_duplicate(window)?;
        if self.column_count == COLUMNS {
            return Err(LayoutError::ColumnCapacity);
        }
        let insert_at = if self.column_count == 0 {
            0
        } else {
            self.focused_column + 1
        };
        for index in (insert_at..self.column_count).rev() {
            self.columns[index + 1] = self.columns[index];
        }
        let mut column = Column::empty();
        column.windows[0] = window;
        column.window_count = 1;
        column.width = self.config.default_column_width.resolve(self.output_width);
        self.columns[insert_at] = column;
        self.column_count += 1;
        self.focused_column = insert_at;
        self.ensure_focused_visible();
        Ok(())
    }

    pub fn consume_window(&mut self, window: u32) -> Result<(), LayoutError> {
        self.reject_duplicate(window)?;
        let column = self
            .columns
            .get_mut(self.focused_column)
            .filter(|_| self.column_count != 0)
            .ok_or(LayoutError::NoFocusedWindow)?;
        if column.window_count == WINDOWS {
            return Err(LayoutError::WindowCapacity);
        }
        column.windows[column.window_count] = window;
        column.focused_window = column.window_count;
        column.window_count += 1;
        Ok(())
    }

    pub fn close_window(&mut self, window: u32) -> Result<(), LayoutError> {
        let (column_index, window_index) =
            self.find_window(window).ok_or(LayoutError::UnknownWindow)?;
        let column = &mut self.columns[column_index];
        for index in window_index..column.window_count - 1 {
            column.windows[index] = column.windows[index + 1];
        }
        column.window_count -= 1;
        column.focused_window = column
            .focused_window
            .min(column.window_count.saturating_sub(1));
        if column.window_count == 0 {
            for index in column_index..self.column_count - 1 {
                self.columns[index] = self.columns[index + 1];
            }
            self.column_count -= 1;
            self.columns[self.column_count] = Column::empty();
            self.focused_column = self.focused_column.min(self.column_count.saturating_sub(1));
        }
        self.ensure_focused_visible();
        Ok(())
    }

    pub fn focus_column_left(&mut self) -> bool {
        if self.focused_column == 0 || self.column_count == 0 {
            return false;
        }
        self.focused_column -= 1;
        self.ensure_focused_visible();
        true
    }

    pub fn focus_column_right(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column + 1 >= self.column_count {
            return false;
        }
        self.focused_column += 1;
        self.ensure_focused_visible();
        true
    }

    pub fn focus_window_up(&mut self) -> bool {
        let column = &mut self.columns[self.focused_column];
        if column.window_count == 0 || column.focused_window == 0 {
            return false;
        }
        column.focused_window -= 1;
        true
    }

    pub fn focus_window_down(&mut self) -> bool {
        let column = &mut self.columns[self.focused_column];
        if column.window_count == 0 || column.focused_window + 1 >= column.window_count {
            return false;
        }
        column.focused_window += 1;
        true
    }

    pub fn set_focused_column_width(&mut self, width: ColumnWidth) -> Result<(), LayoutError> {
        let column = self
            .columns
            .get_mut(self.focused_column)
            .filter(|_| self.column_count != 0)
            .ok_or(LayoutError::NoFocusedWindow)?;
        column.width = width.resolve(self.output_width);
        self.ensure_focused_visible();
        Ok(())
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let maximum = self.maximum_view_offset();
        self.view_offset = self.view_offset.saturating_add(delta).clamp(0, maximum);
    }

    pub fn tile_rect(&self, window: u32) -> Result<Rect, LayoutError> {
        let (column_index, window_index) =
            self.find_window(window).ok_or(LayoutError::UnknownWindow)?;
        let column = self.columns[column_index];
        let gaps = i32::from(self.config.gaps);
        let x = self.column_start(column_index) - self.view_offset;
        let available_height = i32::from(self.output_height.saturating_sub(self.reserved_top));
        let total_gaps = gaps * (column.window_count as i32 + 1);
        let tile_height = ((available_height - total_gaps) / column.window_count as i32).max(1);
        Ok(Rect {
            x,
            y: i32::from(self.reserved_top) + gaps + window_index as i32 * (tile_height + gaps),
            width: column.width,
            height: tile_height.min(i32::from(u16::MAX)) as u16,
        })
    }

    fn reject_duplicate(&self, window: u32) -> Result<(), LayoutError> {
        if self.find_window(window).is_some() {
            Err(LayoutError::DuplicateWindow)
        } else {
            Ok(())
        }
    }

    fn find_window(&self, window: u32) -> Option<(usize, usize)> {
        for column_index in 0..self.column_count {
            let column = self.columns[column_index];
            for window_index in 0..column.window_count {
                if column.windows[window_index] == window {
                    return Some((column_index, window_index));
                }
            }
        }
        None
    }

    fn column_start(&self, column_index: usize) -> i32 {
        let mut x = i32::from(self.config.gaps);
        for index in 0..column_index {
            x += i32::from(self.columns[index].width) + i32::from(self.config.gaps);
        }
        x
    }

    fn maximum_view_offset(&self) -> i32 {
        if self.column_count == 0 {
            return 0;
        }
        let last = self.column_count - 1;
        (self.column_start(last)
            + i32::from(self.columns[last].width)
            + i32::from(self.config.gaps)
            - i32::from(self.output_width))
        .max(0)
    }

    fn ensure_focused_visible(&mut self) {
        if self.column_count == 0 {
            self.view_offset = 0;
            return;
        }
        let start = self.column_start(self.focused_column);
        let end = start + i32::from(self.columns[self.focused_column].width);
        let output_width = i32::from(self.output_width);
        let gap = i32::from(self.config.gaps);
        let centered = (start + end) / 2 - output_width / 2;
        let should_center = self.config.center_focused_column == CenterFocusedColumn::Always
            || (self.config.always_center_single_column && self.column_count == 1)
            || (self.config.center_focused_column == CenterFocusedColumn::OnOverflow
                && (start < self.view_offset + gap || end > self.view_offset + output_width - gap));
        if should_center {
            self.view_offset = centered.clamp(0, self.maximum_view_offset());
        } else if start < self.view_offset + gap {
            self.view_offset = (start - gap).max(0);
        } else if end > self.view_offset + output_width - gap {
            self.view_offset = (end - output_width + gap).min(self.maximum_view_offset());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Token<'a> {
    Identifier(&'a str),
    Number(&'a str),
    String(&'a str),
    LeftBrace,
    RightBrace,
    EndNode,
    Other,
    End,
}

struct Lexer<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn next(&mut self) -> Token<'a> {
        let bytes = self.input.as_bytes();
        loop {
            if self.offset >= bytes.len() {
                return Token::End;
            }
            match bytes[self.offset] {
                b' ' | b'\t' | b'\r' => self.offset += 1,
                b'\n' | b';' => {
                    self.offset += 1;
                    return Token::EndNode;
                }
                b'/' if bytes.get(self.offset + 1) == Some(&b'/') => {
                    while self.offset < bytes.len() && bytes[self.offset] != b'\n' {
                        self.offset += 1;
                    }
                }
                b'{' => {
                    self.offset += 1;
                    return Token::LeftBrace;
                }
                b'}' => {
                    self.offset += 1;
                    return Token::RightBrace;
                }
                b'"' => return self.string(),
                byte if is_identifier_start(byte) => return self.identifier(),
                byte if byte.is_ascii_digit() || byte == b'-' || byte == b'+' => {
                    return self.number();
                }
                _ => {
                    self.offset += 1;
                    return Token::Other;
                }
            }
        }
    }

    fn identifier(&mut self) -> Token<'a> {
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() && is_identifier_continue(bytes[self.offset]) {
            self.offset += 1;
        }
        Token::Identifier(&self.input[start..self.offset])
    }

    fn number(&mut self) -> Token<'a> {
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len()
            && (bytes[self.offset].is_ascii_digit()
                || matches!(bytes[self.offset], b'.' | b'-' | b'+'))
        {
            self.offset += 1;
        }
        Token::Number(&self.input[start..self.offset])
    }

    fn string(&mut self) -> Token<'a> {
        self.offset += 1;
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() {
            match bytes[self.offset] {
                b'"' => {
                    let value = &self.input[start..self.offset];
                    self.offset += 1;
                    return Token::String(value);
                }
                b'\\' => self.offset = (self.offset + 2).min(bytes.len()),
                _ => self.offset += 1,
            }
        }
        Token::End
    }
}

struct ConfigParser<'a> {
    lexer: Lexer<'a>,
    pushed: Option<Token<'a>>,
}

impl<'a> ConfigParser<'a> {
    const fn new(input: &'a str) -> Self {
        Self {
            lexer: Lexer::new(input),
            pushed: None,
        }
    }

    fn parse(mut self) -> Result<LayoutConfig, ConfigError> {
        loop {
            match self.next_non_end_node() {
                Token::Identifier("layout") => {
                    self.expect_block()?;
                    return self.parse_layout();
                }
                Token::Identifier(_) | Token::Other | Token::Number(_) | Token::String(_) => {
                    self.skip_node()?
                }
                Token::End => return Err(ConfigError::MissingLayout),
                Token::RightBrace | Token::LeftBrace | Token::EndNode => {}
            }
        }
    }

    fn parse_layout(&mut self) -> Result<LayoutConfig, ConfigError> {
        let mut config = LayoutConfig::default();
        loop {
            match self.next_non_end_node() {
                Token::RightBrace => return Ok(config),
                Token::Identifier("gaps") => {
                    config.gaps = parse_rounded_u16(self.value_number()?)?;
                    self.finish_node()?;
                }
                Token::Identifier("center-focused-column") => {
                    config.center_focused_column = match self.value_string()? {
                        "never" => CenterFocusedColumn::Never,
                        "always" => CenterFocusedColumn::Always,
                        "on-overflow" => CenterFocusedColumn::OnOverflow,
                        _ => return Err(ConfigError::InvalidCenterPolicy),
                    };
                    self.finish_node()?;
                }
                Token::Identifier("always-center-single-column") => {
                    config.always_center_single_column = true;
                    self.finish_node()?;
                }
                Token::Identifier("default-column-width") => {
                    self.expect_block()?;
                    config.default_column_width = self.parse_column_width()?;
                }
                Token::Identifier("focus-ring") => {
                    self.expect_block()?;
                    config.focus_ring = self.parse_focus_ring(config.focus_ring)?;
                }
                Token::Identifier("background-color") => {
                    config.background_color = parse_color(self.value_string()?)?;
                    self.finish_node()?;
                }
                Token::Identifier(_) | Token::Other | Token::Number(_) | Token::String(_) => {
                    self.skip_node()?
                }
                Token::End => return Err(ConfigError::UnexpectedEnd),
                Token::LeftBrace | Token::EndNode => {}
            }
        }
    }

    fn parse_column_width(&mut self) -> Result<ColumnWidth, ConfigError> {
        let mut width = ColumnWidth::Client;
        let mut seen = false;
        loop {
            match self.next_non_end_node() {
                Token::RightBrace if !seen => return Ok(width),
                Token::RightBrace => return Ok(width),
                Token::Identifier("proportion") if !seen => {
                    width = ColumnWidth::Proportion(parse_thousandths(self.value_number()?)?);
                    seen = true;
                    self.finish_node()?;
                }
                Token::Identifier("fixed") if !seen => {
                    width = ColumnWidth::Fixed(parse_rounded_u16(self.value_number()?)?);
                    seen = true;
                    self.finish_node()?;
                }
                Token::Identifier("proportion" | "fixed") => {
                    return Err(ConfigError::InvalidColumnWidth);
                }
                Token::Identifier(_) | Token::Other | Token::Number(_) | Token::String(_) => {
                    return Err(ConfigError::InvalidColumnWidth);
                }
                Token::End => return Err(ConfigError::UnexpectedEnd),
                Token::LeftBrace | Token::EndNode => {}
            }
        }
    }

    fn parse_focus_ring(&mut self, mut ring: FocusRing) -> Result<FocusRing, ConfigError> {
        loop {
            match self.next_non_end_node() {
                Token::RightBrace => return Ok(ring),
                Token::Identifier("on") => {
                    ring.enabled = true;
                    self.finish_node()?;
                }
                Token::Identifier("off") => {
                    ring.enabled = false;
                    self.finish_node()?;
                }
                Token::Identifier("width") => {
                    ring.width = parse_rounded_u16(self.value_number()?)?;
                    self.finish_node()?;
                }
                Token::Identifier("active-color") => {
                    ring.active_color = parse_color(self.value_string()?)?;
                    self.finish_node()?;
                }
                Token::Identifier("inactive-color") => {
                    ring.inactive_color = parse_color(self.value_string()?)?;
                    self.finish_node()?;
                }
                Token::Identifier(_) | Token::Other | Token::Number(_) | Token::String(_) => {
                    self.skip_node()?
                }
                Token::End => return Err(ConfigError::UnexpectedEnd),
                Token::LeftBrace | Token::EndNode => {}
            }
        }
    }

    fn expect_block(&mut self) -> Result<(), ConfigError> {
        loop {
            match self.next() {
                Token::LeftBrace => return Ok(()),
                Token::EndNode => {}
                Token::End => return Err(ConfigError::UnexpectedEnd),
                _ => return Err(ConfigError::UnexpectedEnd),
            }
        }
    }

    fn value_number(&mut self) -> Result<&'a str, ConfigError> {
        match self.next() {
            Token::Number(value) => Ok(value),
            _ => Err(ConfigError::InvalidNumber),
        }
    }

    fn value_string(&mut self) -> Result<&'a str, ConfigError> {
        match self.next() {
            Token::String(value) => Ok(value),
            _ => Err(ConfigError::UnexpectedEnd),
        }
    }

    fn finish_node(&mut self) -> Result<(), ConfigError> {
        loop {
            match self.next() {
                Token::EndNode => return Ok(()),
                Token::RightBrace => {
                    self.push(Token::RightBrace);
                    return Ok(());
                }
                Token::LeftBrace => {
                    self.skip_block()?;
                    return Ok(());
                }
                Token::End => return Ok(()),
                _ => {}
            }
        }
    }

    fn skip_node(&mut self) -> Result<(), ConfigError> {
        loop {
            match self.next() {
                Token::EndNode => return Ok(()),
                Token::RightBrace => {
                    self.push(Token::RightBrace);
                    return Ok(());
                }
                Token::LeftBrace => return self.skip_block(),
                Token::End => return Ok(()),
                _ => {}
            }
        }
    }

    fn skip_block(&mut self) -> Result<(), ConfigError> {
        let mut depth = 1usize;
        while depth != 0 {
            match self.next() {
                Token::LeftBrace => depth += 1,
                Token::RightBrace => depth -= 1,
                Token::End => return Err(ConfigError::UnexpectedEnd),
                _ => {}
            }
        }
        Ok(())
    }

    fn next_non_end_node(&mut self) -> Token<'a> {
        loop {
            let token = self.next();
            if token != Token::EndNode {
                return token;
            }
        }
    }

    fn next(&mut self) -> Token<'a> {
        self.pushed.take().unwrap_or_else(|| self.lexer.next())
    }

    fn push(&mut self, token: Token<'a>) {
        self.pushed = Some(token);
    }
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

const fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'/')
}

fn parse_rounded_u16(value: &str) -> Result<u16, ConfigError> {
    let thousandths = parse_decimal_thousandths(value)?;
    let rounded = (thousandths + 500) / 1000;
    u16::try_from(rounded).map_err(|_| ConfigError::InvalidNumber)
}

fn parse_thousandths(value: &str) -> Result<u16, ConfigError> {
    let thousandths = parse_decimal_thousandths(value)?;
    if thousandths > 1000 {
        return Err(ConfigError::InvalidColumnWidth);
    }
    u16::try_from(thousandths).map_err(|_| ConfigError::InvalidColumnWidth)
}

fn parse_decimal_thousandths(value: &str) -> Result<u32, ConfigError> {
    if value.is_empty() || value.starts_with('-') || value.starts_with('+') {
        return Err(ConfigError::InvalidNumber);
    }
    let (integer, fraction) = value.split_once('.').unwrap_or((value, ""));
    let integer = parse_digits(integer)?;
    let mut fraction_value = 0u32;
    let mut digits = 0usize;
    for byte in fraction.bytes() {
        if !byte.is_ascii_digit() || digits == 3 {
            return Err(ConfigError::InvalidNumber);
        }
        fraction_value = fraction_value * 10 + u32::from(byte - b'0');
        digits += 1;
    }
    while digits < 3 {
        fraction_value *= 10;
        digits += 1;
    }
    integer
        .checked_mul(1000)
        .and_then(|value| value.checked_add(fraction_value))
        .ok_or(ConfigError::InvalidNumber)
}

fn parse_digits(value: &str) -> Result<u32, ConfigError> {
    if value.is_empty() {
        return Ok(0);
    }
    let mut result = 0u32;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(ConfigError::InvalidNumber);
        }
        result = result
            .checked_mul(10)
            .and_then(|current| current.checked_add(u32::from(byte - b'0')))
            .ok_or(ConfigError::InvalidNumber)?;
    }
    Ok(result)
}

fn parse_color(value: &str) -> Result<u32, ConfigError> {
    let hex = value.strip_prefix('#').ok_or(ConfigError::InvalidColor)?;
    if hex.len() != 6 && hex.len() != 8 {
        return Err(ConfigError::InvalidColor);
    }
    let mut color = 0u32;
    for byte in hex.bytes().take(6) {
        color = color
            .checked_mul(16)
            .and_then(|current| hex_digit(byte).map(|digit| current + u32::from(digit)))
            .ok_or(ConfigError::InvalidColor)?;
    }
    Ok(color)
}

const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_niri_layout_subset_inside_a_full_document() {
        let config = parse_niri_layout(
            r##"
            input {
                keyboard { numlock; }
            }
            layout {
                gaps 12.4
                center-focused-column "on-overflow"
                always-center-single-column
                default-column-width { proportion 0.625; }
                focus-ring {
                    width 3
                    active-color "#89b4fa"
                    inactive-color "#45475a80"
                }
                background-color "#1e1e2e"
                border { off; }
            }
            window-rule {
                default-column-width { fixed 900; }
            }
            "##,
        )
        .unwrap();
        assert_eq!(config.gaps, 12);
        assert_eq!(
            config.center_focused_column,
            CenterFocusedColumn::OnOverflow
        );
        assert!(config.always_center_single_column);
        assert_eq!(config.default_column_width, ColumnWidth::Proportion(625));
        assert_eq!(config.focus_ring.width, 3);
        assert_eq!(config.focus_ring.active_color, 0x89_b4_fa);
        assert_eq!(config.focus_ring.inactive_color, 0x45_47_5a);
        assert_eq!(config.background_color, 0x1e_1e_2e);
    }

    #[test]
    fn parses_fixed_and_client_selected_widths() {
        assert_eq!(
            parse_niri_layout("layout { default-column-width { fixed 720; } }")
                .unwrap()
                .default_column_width,
            ColumnWidth::Fixed(720)
        );
        assert_eq!(
            parse_niri_layout("layout { default-column-width {} }")
                .unwrap()
                .default_column_width,
            ColumnWidth::Client
        );
    }

    #[test]
    fn rejects_invalid_supported_values() {
        assert_eq!(
            parse_niri_layout(r#"layout { center-focused-column "sometimes"; }"#),
            Err(ConfigError::InvalidCenterPolicy)
        );
        assert_eq!(
            parse_niri_layout("layout { default-column-width { proportion 1.5; } }"),
            Err(ConfigError::InvalidColumnWidth)
        );
        assert_eq!(
            parse_niri_layout(r##"layout { background-color "#xyzxyz"; }"##),
            Err(ConfigError::InvalidColor)
        );
    }

    #[test]
    fn opening_windows_extends_strip_without_resizing_existing_columns() {
        let mut layout = ScrollLayout::<4, 3>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(10).unwrap();
        let first = layout.tile_rect(10).unwrap();
        let first_strip_x = first.x + layout.view_offset();
        layout.open_window(20).unwrap();
        let first_after = layout.tile_rect(10).unwrap();
        let second = layout.tile_rect(20).unwrap();
        assert_eq!(first.width, 500);
        assert_eq!(first.width, first_after.width);
        assert_eq!(first_strip_x, first_after.x + layout.view_offset());
        assert!(second.x > first.x);
        assert_eq!(layout.focused_window(), Some(20));
    }

    #[test]
    fn focus_scrolls_columns_at_output_edges() {
        let config = LayoutConfig {
            default_column_width: ColumnWidth::Fixed(600),
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<4, 2>::new(1000, 700, 30, config);
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        assert!(layout.view_offset() > 0);
        assert!(layout.focus_column_left());
        assert_eq!(layout.view_offset(), 0);
        assert!(layout.focus_column_right());
        assert!(layout.tile_rect(2).unwrap().x + 600 <= 1000);
    }

    #[test]
    fn stacks_windows_vertically_with_stable_column_width() {
        let mut layout = ScrollLayout::<2, 3>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        layout.consume_window(2).unwrap();
        let top = layout.tile_rect(1).unwrap();
        let bottom = layout.tile_rect(2).unwrap();
        assert_eq!(top.x, bottom.x);
        assert_eq!(top.width, bottom.width);
        assert!(bottom.y > top.y);
        assert_eq!(layout.focused_window(), Some(2));
        assert!(layout.focus_window_up());
        assert_eq!(layout.focused_window(), Some(1));
    }

    #[test]
    fn closing_the_last_window_removes_its_column() {
        let mut layout = ScrollLayout::<3, 2>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        layout.close_window(2).unwrap();
        assert_eq!(layout.len(), 1);
        assert_eq!(layout.focused_window(), Some(1));
        assert_eq!(layout.close_window(2), Err(LayoutError::UnknownWindow));
    }

    #[test]
    fn manual_scroll_is_bounded_by_strip_geometry() {
        let config = LayoutConfig {
            default_column_width: ColumnWidth::Fixed(600),
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<3, 1>::new(1000, 700, 30, config);
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        layout.scroll_by(i32::MAX);
        let maximum = layout.view_offset();
        layout.scroll_by(100);
        assert_eq!(layout.view_offset(), maximum);
        layout.scroll_by(i32::MIN);
        assert_eq!(layout.view_offset(), 0);
    }
}
