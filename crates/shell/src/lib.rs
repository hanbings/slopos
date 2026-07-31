// SPDX-License-Identifier: 0BSD

#![no_std]

mod niri;
mod png;
mod wallpaper;
mod waybar;
mod waybar_style;

pub use niri::{
    BindingKey, BindingModifiers, MAX_NIRI_BINDINGS, MAX_NIRI_WINDOW_RULES, MAX_NIRI_WORKSPACES,
    NamedWorkspace, NamedWorkspaceList, NiriAction, NiriBinding, NiriBindingList, NiriConfigError,
    NiriShellConfig, NiriWindowRule, NiriWindowRuleList, WorkspaceError, WorkspaceReference,
    WorkspaceSet, parse_niri_shell_config,
};
pub use png::{DecodedPng, PngError, decode_png_rgb};
pub use wallpaper::{
    ClearRequest, CropGravity, ImgRequest, MAX_WALLPAPER_PATH, PpmError, PpmFormat, PpmImage,
    RasterImage, RasterPixels, ResizeFilter, ResizeMode, SwwwCommand, SwwwDaemonError,
    SwwwDefaults, SwwwParseError, TransitionBezier, TransitionCoordinate, TransitionOptions,
    TransitionPosition, TransitionType, TransitionWave, WallpaperDaemon, WallpaperQuery, parse_ppm,
    parse_ppm_bytes, parse_swww_command, parse_swww_environment, resize_filter_sample,
    transition_eased_progress, transition_pixel, transition_pixel_with_options,
};
pub use waybar::{
    BarButton, BarConfigError, BarFormatError, BarFormatValue, BarLayer, BarMode, BarModuleConfig,
    BarModuleConfigList, BarModuleList, BarOutputDimension, BarOutputDimensionList, BarOutputList,
    BarPosition, BarSignal, BarSignalAction, BarText, MAX_BAR_MODE_NAME, MAX_BAR_MODES,
    MAX_BAR_MODULE_CONFIGS, MAX_BAR_MODULES, MAX_BAR_NAME, MAX_BAR_OUTPUT_DIMENSIONS,
    MAX_BAR_OUTPUT_NAME, MAX_BAR_OUTPUTS, MAX_BAR_TEXT, WaybarConfig, format_bar_text,
    parse_waybar_config,
};
pub use waybar_style::{
    MAX_WAYBAR_STYLE_RULES, ResolvedWaybarStyle, WaybarStyle, WaybarStyleError, parse_waybar_style,
};

pub const DEFAULT_GAPS: u16 = 16;
pub const DEFAULT_FOCUS_RING_WIDTH: u16 = 4;
pub const DEFAULT_ACTIVE_COLOR: u32 = 0x7f_c8_ff;
pub const DEFAULT_INACTIVE_COLOR: u32 = 0x50_50_50;
pub const DEFAULT_BORDER_COLOR: u32 = 0xff_c8_7f;
pub const DEFAULT_BACKGROUND_COLOR: u32 = 0x10_14_26;
pub const DEFAULT_SHADOW_SOFTNESS: u16 = 30;
pub const DEFAULT_SHADOW_SPREAD: i16 = 5;
pub const MAX_PRESET_SIZES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CenterFocusedColumn {
    Never,
    Always,
    OnOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnDisplay {
    Normal,
    Tabbed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnWidth {
    Proportion(u16),
    Fixed(u16),
    Client,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnWidthChange {
    Set(ColumnWidth),
    AdjustProportion(i16),
    AdjustFixed(i32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetSizes {
    entries: [ColumnWidth; MAX_PRESET_SIZES],
    length: usize,
}

impl PresetSizes {
    const fn defaults() -> Self {
        let mut entries = [ColumnWidth::Client; MAX_PRESET_SIZES];
        entries[0] = ColumnWidth::Proportion(333);
        entries[1] = ColumnWidth::Proportion(500);
        entries[2] = ColumnWidth::Proportion(667);
        Self { entries, length: 3 }
    }

    const fn empty() -> Self {
        Self {
            entries: [ColumnWidth::Client; MAX_PRESET_SIZES],
            length: 0,
        }
    }

    fn push(&mut self, width: ColumnWidth) -> Result<(), ConfigError> {
        if self.length == MAX_PRESET_SIZES {
            return Err(ConfigError::InvalidColumnWidth);
        }
        self.entries[self.length] = width;
        self.length += 1;
        Ok(())
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn get(self, index: usize) -> Option<ColumnWidth> {
        self.entries
            .get(index)
            .copied()
            .filter(|_| index < self.length)
    }
}

impl ColumnWidth {
    fn resolve(self, view_size: u16, gap: u16) -> u16 {
        match self {
            Self::Proportion(thousandths) => {
                let full = u32::from(view_size.saturating_sub(gap));
                ((full * u32::from(thousandths)) / 1000)
                    .saturating_sub(u32::from(gap))
                    .clamp(1, u32::from(u16::MAX)) as u16
            }
            Self::Fixed(width) => width,
            Self::Client => view_size / 2,
        }
    }
}

fn next_preset_index(
    presets: PresetSizes,
    current: u16,
    view_size: u16,
    gap: u16,
    backwards: bool,
) -> usize {
    let length = presets.len();
    let exact = (0..length).find(|index| {
        presets
            .get(*index)
            .is_some_and(|size| size.resolve(view_size, gap) == current)
    });
    if let Some(index) = exact {
        if backwards {
            (index + length - 1) % length
        } else {
            (index + 1) % length
        }
    } else if backwards {
        (0..length)
            .rev()
            .find(|index| {
                presets
                    .get(*index)
                    .is_some_and(|size| size.resolve(view_size, gap) < current)
            })
            .unwrap_or(length - 1)
    } else {
        (0..length)
            .find(|index| {
                presets
                    .get(*index)
                    .is_some_and(|size| size.resolve(view_size, gap) > current)
            })
            .unwrap_or(0)
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
pub struct Border {
    pub enabled: bool,
    pub width: u16,
    pub active_color: u32,
    pub inactive_color: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShadowColor {
    pub rgb: u32,
    pub opacity: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Shadow {
    pub enabled: bool,
    pub offset_x: i32,
    pub offset_y: i32,
    pub softness: u16,
    pub spread: i16,
    pub draw_behind_window: bool,
    pub color: ShadowColor,
    pub inactive_color: Option<ShadowColor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutConfig {
    pub gaps: u16,
    pub center_focused_column: CenterFocusedColumn,
    pub always_center_single_column: bool,
    pub default_column_display: ColumnDisplay,
    pub default_column_width: ColumnWidth,
    pub preset_column_widths: PresetSizes,
    pub preset_window_heights: PresetSizes,
    pub focus_ring: FocusRing,
    pub border: Border,
    pub shadow: Shadow,
    pub background_color: u32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            gaps: DEFAULT_GAPS,
            center_focused_column: CenterFocusedColumn::Never,
            always_center_single_column: false,
            default_column_display: ColumnDisplay::Normal,
            default_column_width: ColumnWidth::Proportion(500),
            preset_column_widths: PresetSizes::defaults(),
            preset_window_heights: PresetSizes::defaults(),
            focus_ring: FocusRing {
                enabled: true,
                width: DEFAULT_FOCUS_RING_WIDTH,
                active_color: DEFAULT_ACTIVE_COLOR,
                inactive_color: DEFAULT_INACTIVE_COLOR,
            },
            border: Border {
                enabled: false,
                width: DEFAULT_FOCUS_RING_WIDTH,
                active_color: DEFAULT_BORDER_COLOR,
                inactive_color: DEFAULT_INACTIVE_COLOR,
            },
            shadow: Shadow {
                enabled: false,
                offset_x: 0,
                offset_y: 5,
                softness: DEFAULT_SHADOW_SOFTNESS,
                spread: DEFAULT_SHADOW_SPREAD,
                draw_behind_window: false,
                color: ShadowColor {
                    rgb: 0,
                    opacity: 467,
                },
                inactive_color: None,
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
    InvalidColumnDisplay,
    InvalidColumnWidth,
    InvalidFocusRing,
    InvalidShadow,
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
pub struct TabbedColumnInfo {
    pub active_tab: usize,
    pub tab_count: usize,
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
    window_heights: [u16; WINDOWS],
    window_count: usize,
    focused_window: usize,
    width: u16,
    maximized: bool,
    maximized_to_edges: bool,
    display: ColumnDisplay,
    tabbed_height: u16,
}

impl<const WINDOWS: usize> Column<WINDOWS> {
    const fn empty() -> Self {
        Self {
            windows: [0; WINDOWS],
            window_heights: [0; WINDOWS],
            window_count: 0,
            focused_window: 0,
            width: 0,
            maximized: false,
            maximized_to_edges: false,
            display: ColumnDisplay::Normal,
            tabbed_height: 0,
        }
    }

    fn reset_window_heights(&mut self) {
        self.window_heights = [0; WINDOWS];
        self.tabbed_height = 0;
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

    pub const fn reserved_top(&self) -> u16 {
        self.reserved_top
    }

    pub fn set_reserved_top(&mut self, reserved_top: u16) -> bool {
        let reserved_top = reserved_top.min(self.output_height);
        if self.reserved_top == reserved_top {
            return false;
        }
        self.reserved_top = reserved_top;
        let available_height = self.output_height.saturating_sub(self.reserved_top);
        let maximum_single = available_height
            .saturating_sub(self.config.gaps.saturating_mul(2))
            .max(1);
        for column in &mut self.columns[..self.column_count] {
            if column.tabbed_height != 0 {
                column.tabbed_height = column.tabbed_height.min(maximum_single);
            }
            if column.window_count == 0 || column.window_heights[0] == 0 {
                continue;
            }
            let maximum_total = available_height
                .saturating_sub(
                    self.config
                        .gaps
                        .saturating_mul(column.window_count as u16 + 1),
                )
                .max(column.window_count as u16);
            let total = column.window_heights[..column.window_count]
                .iter()
                .fold(0u32, |sum, height| sum.saturating_add(u32::from(*height)));
            if total <= u32::from(maximum_total) {
                continue;
            }
            let mut remaining = maximum_total;
            for index in 0..column.window_count {
                let windows_left = (column.window_count - index - 1) as u16;
                let scaled = if index + 1 == column.window_count {
                    remaining
                } else {
                    ((u32::from(column.window_heights[index]) * u32::from(maximum_total)) / total)
                        .clamp(1, u32::from(remaining.saturating_sub(windows_left)))
                        as u16
                };
                column.window_heights[index] = scaled;
                remaining = remaining.saturating_sub(scaled);
            }
        }
        true
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

    pub fn window_is_visible(&self, window: u32) -> bool {
        self.find_window(window).is_some_and(|(column, row)| {
            self.columns[column].display == ColumnDisplay::Normal
                || self.columns[column].focused_window == row
        })
    }

    pub fn tabbed_column_info(&self, window: u32) -> Option<TabbedColumnInfo> {
        let (column, _) = self.find_window(window)?;
        let column = self.columns[column];
        (column.display == ColumnDisplay::Tabbed).then_some(TabbedColumnInfo {
            active_tab: column.focused_window,
            tab_count: column.window_count,
        })
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
        self.open_window_with_dimensions(window, None, None)
    }

    pub fn open_window_with_width(
        &mut self,
        window: u32,
        width: Option<ColumnWidth>,
    ) -> Result<(), LayoutError> {
        self.open_window_with_dimensions(window, width, None)
    }

    pub fn open_window_with_dimensions(
        &mut self,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
    ) -> Result<(), LayoutError> {
        self.open_window_with_properties(window, width, height, None)
    }

    pub fn open_window_with_properties(
        &mut self,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
        display: Option<ColumnDisplay>,
    ) -> Result<(), LayoutError> {
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
        column.display = display.unwrap_or(self.config.default_column_display);
        column.width = width
            .unwrap_or(self.config.default_column_width)
            .resolve(self.output_width, self.config.gaps);
        if let Some(height) = height.filter(|height| *height != ColumnWidth::Client) {
            let available_height = self.output_height.saturating_sub(self.reserved_top);
            let maximum = available_height
                .saturating_sub(self.config.gaps.saturating_mul(2))
                .max(1);
            let height = height
                .resolve(available_height, self.config.gaps)
                .clamp(1, maximum);
            if column.display == ColumnDisplay::Tabbed {
                column.tabbed_height = height;
            } else {
                column.window_heights[0] = height;
            }
        }
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
        column.maximized_to_edges = false;
        column.reset_window_heights();
        Ok(())
    }

    pub fn consume_window_into_column(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column + 1 >= self.column_count {
            return false;
        }
        let destination = self.focused_column;
        let source = destination + 1;
        if self.columns[destination].window_count == WINDOWS {
            return false;
        }
        let window = self.columns[source].windows[0];
        for index in 0..self.columns[source].window_count - 1 {
            self.columns[source].windows[index] = self.columns[source].windows[index + 1];
        }
        self.columns[source].window_count -= 1;
        self.columns[source].focused_window = self.columns[source]
            .focused_window
            .saturating_sub(1)
            .min(self.columns[source].window_count.saturating_sub(1));

        let destination_row = self.columns[destination].window_count;
        self.columns[destination].windows[destination_row] = window;
        self.columns[destination].window_count += 1;
        self.columns[destination].focused_window = destination_row;
        self.columns[destination].maximized_to_edges = false;
        self.columns[source].maximized_to_edges = false;
        self.columns[destination].reset_window_heights();
        self.columns[source].reset_window_heights();

        if self.columns[source].window_count == 0 {
            for index in source..self.column_count - 1 {
                self.columns[index] = self.columns[index + 1];
            }
            self.column_count -= 1;
            self.columns[self.column_count] = Column::empty();
        }
        self.ensure_focused_visible();
        true
    }

    pub fn expel_window_from_column(&mut self) -> bool {
        self.expel_window_from_column_with_display(None)
    }

    pub fn expel_window_from_column_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.column_count == 0
            || self.column_count == COLUMNS
            || self.columns[self.focused_column].window_count <= 1
        {
            return false;
        }
        let source = self.focused_column;
        let source_row = self.columns[source].window_count - 1;
        let window = self.columns[source].windows[source_row];
        let width = self.columns[source].width;
        self.columns[source].window_count -= 1;
        self.columns[source].focused_window = self.columns[source]
            .focused_window
            .min(self.columns[source].window_count - 1);
        self.columns[source].reset_window_heights();

        let destination = source + 1;
        for index in (destination..self.column_count).rev() {
            self.columns[index + 1] = self.columns[index];
        }
        let mut column = Column::empty();
        column.windows[0] = window;
        column.window_count = 1;
        column.display = display.unwrap_or(self.config.default_column_display);
        column.width = width;
        self.columns[destination] = column;
        self.column_count += 1;
        self.ensure_focused_visible();
        true
    }

    pub fn consume_or_expel_focused_window_left(&mut self) -> bool {
        self.consume_or_expel_focused_window_left_with_display(None)
    }

    pub fn consume_or_expel_focused_window_left_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.column_count == 0 {
            return false;
        }
        if self.columns[self.focused_column].window_count > 1 {
            return self.extract_focused_window_to_side(true, display);
        }
        if self.focused_column == 0 {
            return false;
        }
        self.consume_focused_singleton_into_adjacent(true)
    }

    pub fn consume_or_expel_focused_window_right(&mut self) -> bool {
        self.consume_or_expel_focused_window_right_with_display(None)
    }

    pub fn consume_or_expel_focused_window_right_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.column_count == 0 {
            return false;
        }
        if self.columns[self.focused_column].window_count > 1 {
            return self.extract_focused_window_to_side(false, display);
        }
        if self.focused_column + 1 >= self.column_count {
            return false;
        }
        self.consume_focused_singleton_into_adjacent(false)
    }

    pub fn toggle_focused_column_tabbed_display(&mut self) -> bool {
        if self.column_count == 0 {
            return false;
        }
        let column = &mut self.columns[self.focused_column];
        column.display = match column.display {
            ColumnDisplay::Normal => ColumnDisplay::Tabbed,
            ColumnDisplay::Tabbed => ColumnDisplay::Normal,
        };
        true
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
        column.reset_window_heights();
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

    pub fn focus_column_first(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column == 0 {
            return false;
        }
        self.focused_column = 0;
        self.ensure_focused_visible();
        true
    }

    pub fn focus_column_last(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column + 1 == self.column_count {
            return false;
        }
        self.focused_column = self.column_count - 1;
        self.ensure_focused_visible();
        true
    }

    pub fn move_column_left(&mut self) -> bool {
        if self.focused_column == 0 || self.column_count == 0 {
            return false;
        }
        self.columns
            .swap(self.focused_column, self.focused_column - 1);
        self.focused_column -= 1;
        self.ensure_focused_visible();
        true
    }

    pub fn move_column_right(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column + 1 >= self.column_count {
            return false;
        }
        self.columns
            .swap(self.focused_column, self.focused_column + 1);
        self.focused_column += 1;
        self.ensure_focused_visible();
        true
    }

    pub fn move_column_to_first(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column == 0 {
            return false;
        }
        let source = self.focused_column;
        let column = self.columns[source];
        for index in (1..=source).rev() {
            self.columns[index] = self.columns[index - 1];
        }
        self.columns[0] = column;
        self.focused_column = 0;
        self.ensure_focused_visible();
        true
    }

    pub fn move_column_to_last(&mut self) -> bool {
        if self.column_count == 0 || self.focused_column + 1 == self.column_count {
            return false;
        }
        let source = self.focused_column;
        let column = self.columns[source];
        for index in source..self.column_count - 1 {
            self.columns[index] = self.columns[index + 1];
        }
        self.columns[self.column_count - 1] = column;
        self.focused_column = self.column_count - 1;
        self.ensure_focused_visible();
        true
    }

    pub fn move_focused_column_to(&mut self, destination: &mut Self) -> Result<bool, LayoutError> {
        if self.column_count == 0 {
            return Ok(false);
        }
        if destination.column_count == COLUMNS {
            return Err(LayoutError::ColumnCapacity);
        }
        let source_index = self.focused_column;
        let column = self.columns[source_index];
        for window in &column.windows[..column.window_count] {
            destination.reject_duplicate(*window)?;
        }

        let destination_index = if destination.column_count == 0 {
            0
        } else {
            destination.focused_column + 1
        };
        for index in (destination_index..destination.column_count).rev() {
            destination.columns[index + 1] = destination.columns[index];
        }
        destination.columns[destination_index] = column;
        destination.column_count += 1;
        destination.focused_column = destination_index;
        destination.ensure_focused_visible();

        for index in source_index..self.column_count - 1 {
            self.columns[index] = self.columns[index + 1];
        }
        self.column_count -= 1;
        self.columns[self.column_count] = Column::empty();
        self.focused_column = source_index.min(self.column_count.saturating_sub(1));
        self.ensure_focused_visible();
        Ok(true)
    }

    pub fn move_focused_window_to(&mut self, destination: &mut Self) -> Result<bool, LayoutError> {
        self.move_focused_window_to_with_display(destination, None)
    }

    pub fn move_focused_window_to_with_display(
        &mut self,
        destination: &mut Self,
        display: Option<ColumnDisplay>,
    ) -> Result<bool, LayoutError> {
        if self.column_count == 0 {
            return Ok(false);
        }
        if destination.column_count == COLUMNS {
            return Err(LayoutError::ColumnCapacity);
        }
        let source = self.columns[self.focused_column];
        let window = source.windows[source.focused_window];
        destination.reject_duplicate(window)?;

        let mut column = Column::empty();
        column.windows[0] = window;
        column.window_count = 1;
        column.width = source.width;
        column.maximized = source.maximized;
        column.maximized_to_edges = source.maximized_to_edges;
        column.display = display.unwrap_or(destination.config.default_column_display);
        let destination_index = if destination.column_count == 0 {
            0
        } else {
            destination.focused_column + 1
        };
        for index in (destination_index..destination.column_count).rev() {
            destination.columns[index + 1] = destination.columns[index];
        }
        destination.columns[destination_index] = column;
        destination.column_count += 1;
        destination.focused_column = destination_index;
        destination.ensure_focused_visible();

        self.close_window(window)
            .expect("focused source window remains present until transfer");
        Ok(true)
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

    pub fn move_window_up(&mut self) -> bool {
        let column = &mut self.columns[self.focused_column];
        if column.window_count == 0 || column.focused_window == 0 {
            return false;
        }
        column
            .windows
            .swap(column.focused_window, column.focused_window - 1);
        column
            .window_heights
            .swap(column.focused_window, column.focused_window - 1);
        column.focused_window -= 1;
        true
    }

    pub fn move_window_down(&mut self) -> bool {
        let column = &mut self.columns[self.focused_column];
        if column.window_count == 0 || column.focused_window + 1 >= column.window_count {
            return false;
        }
        column
            .windows
            .swap(column.focused_window, column.focused_window + 1);
        column
            .window_heights
            .swap(column.focused_window, column.focused_window + 1);
        column.focused_window += 1;
        true
    }

    pub fn change_focused_window_height(
        &mut self,
        change: ColumnWidthChange,
    ) -> Result<bool, LayoutError> {
        if self.column_count == 0 {
            return Err(LayoutError::NoFocusedWindow);
        }
        let column_index = self.focused_column;
        let was_maximized_to_edges = self.columns[column_index].maximized_to_edges;
        self.columns[column_index].maximized_to_edges = false;
        let window_count = self.columns[column_index].window_count;
        if window_count == 0 {
            return Err(LayoutError::NoFocusedWindow);
        }
        let focused_window = self.columns[column_index].focused_window;
        let mut heights = self.resolved_window_heights(column_index);
        let previous = i32::from(heights[focused_window]);
        let available_height = self.output_height.saturating_sub(self.reserved_top);
        let requested = match change {
            ColumnWidthChange::Set(height) => {
                i32::from(height.resolve(available_height, self.config.gaps))
            }
            ColumnWidthChange::AdjustProportion(thousandths) => previous.saturating_add(
                i32::from(available_height.saturating_sub(self.config.gaps))
                    .saturating_mul(i32::from(thousandths))
                    / 1000,
            ),
            ColumnWidthChange::AdjustFixed(pixels) => previous.saturating_add(pixels),
        };
        let maximum = available_height.saturating_sub(self.config.gaps.saturating_mul(2));
        if self.columns[column_index].display == ColumnDisplay::Tabbed {
            let target = requested.clamp(1, i32::from(maximum.max(1))) as u16;
            self.columns[column_index].tabbed_height = target;
            return Ok(was_maximized_to_edges || target != previous as u16);
        }
        if window_count == 1 {
            let target = requested.clamp(1, i32::from(maximum.max(1))) as u16;
            self.columns[column_index].window_heights[0] = target;
            return Ok(was_maximized_to_edges || target != previous as u16);
        }
        let total_height: i32 = heights[..window_count]
            .iter()
            .map(|height| i32::from(*height))
            .sum();
        let target = requested.clamp(1, total_height - (window_count as i32 - 1));
        let delta = target - previous;
        if delta == 0 {
            return Ok(false);
        }

        if delta > 0 {
            let mut remaining = delta;
            for offset in 1..window_count {
                let index = (focused_window + offset) % window_count;
                let available = i32::from(heights[index]).saturating_sub(1);
                let taken = available.min(remaining);
                heights[index] -= taken as u16;
                remaining -= taken;
                if remaining == 0 {
                    break;
                }
            }
            heights[focused_window] = (previous + delta - remaining) as u16;
        } else {
            let released = -delta;
            heights[focused_window] = target as u16;
            let recipient = (focused_window + 1) % window_count;
            heights[recipient] = i32::from(heights[recipient])
                .saturating_add(released)
                .min(i32::from(u16::MAX)) as u16;
        }
        let changed = heights[focused_window] != previous as u16;
        self.columns[column_index].window_heights = heights;
        Ok(was_maximized_to_edges || changed)
    }

    pub fn reset_focused_window_height(&mut self) -> bool {
        if self.column_count == 0 {
            return false;
        }
        let column = &mut self.columns[self.focused_column];
        let was_maximized_to_edges = column.maximized_to_edges;
        column.maximized_to_edges = false;
        let changed = column.tabbed_height != 0
            || column.window_heights[..column.window_count]
                .iter()
                .any(|height| *height != 0);
        column.reset_window_heights();
        was_maximized_to_edges || changed
    }

    pub fn switch_preset_column_width(&mut self) -> bool {
        self.switch_preset_column_width_in_direction(false)
    }

    pub fn switch_preset_column_width_back(&mut self) -> bool {
        self.switch_preset_column_width_in_direction(true)
    }

    pub fn toggle_maximize_focused_column(&mut self) -> bool {
        if self.column_count == 0 {
            return false;
        }
        let column = &mut self.columns[self.focused_column];
        if column.maximized_to_edges {
            column.maximized_to_edges = false;
            column.maximized = false;
        } else {
            column.maximized = !column.maximized;
        }
        self.ensure_focused_visible();
        true
    }

    pub fn set_window_maximized(
        &mut self,
        window: u32,
        maximized: bool,
    ) -> Result<bool, LayoutError> {
        let (column_index, _) = self.find_window(window).ok_or(LayoutError::UnknownWindow)?;
        let changed = self.columns[column_index].maximized != maximized;
        self.columns[column_index].maximized = maximized;
        self.ensure_focused_visible();
        Ok(changed)
    }

    pub fn set_window_maximized_to_edges(
        &mut self,
        window: u32,
        maximized: bool,
    ) -> Result<bool, LayoutError> {
        let (column_index, _) = self.find_window(window).ok_or(LayoutError::UnknownWindow)?;
        let changed = self.columns[column_index].maximized_to_edges != maximized;
        self.columns[column_index].maximized_to_edges = maximized;
        self.ensure_focused_visible();
        Ok(changed)
    }

    pub fn toggle_maximize_focused_window_to_edges(&mut self) -> bool {
        self.toggle_maximize_focused_window_to_edges_with_display(None)
    }

    pub fn toggle_maximize_focused_window_to_edges_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.column_count == 0 {
            return false;
        }
        if self.columns[self.focused_column].maximized_to_edges {
            self.columns[self.focused_column].maximized_to_edges = false;
            self.ensure_focused_visible();
            return true;
        }
        if self.columns[self.focused_column].window_count > 1
            && !self.extract_focused_window_to_side(false, display)
        {
            return false;
        }
        self.columns[self.focused_column].maximized_to_edges = true;
        self.ensure_focused_visible();
        true
    }

    pub fn center_focused_column(&mut self) -> bool {
        if self.column_count == 0 {
            return false;
        }
        let start = self.column_start(self.focused_column);
        let end = start + i32::from(self.effective_column_width(self.focused_column));
        let centered = (start + end) / 2 - i32::from(self.output_width) / 2;
        let changed = centered != self.view_offset;
        self.view_offset = centered;
        changed
    }

    pub fn center_visible_columns(&mut self) -> bool {
        if self.column_count == 0
            || self.config.center_focused_column == CenterFocusedColumn::Always
            || (self.config.always_center_single_column && self.column_count == 1)
        {
            return false;
        }

        let view_start = self.view_offset;
        let view_end = view_start + i32::from(self.output_width);
        let gap = i32::from(self.config.gaps);
        let mut width_taken = 0i32;
        let mut leftmost_start = None;
        let mut active_visible = false;

        for index in 0..self.column_count {
            let start = self.column_start(index);
            let width = i32::from(self.effective_column_width(index));
            if start < view_start + gap {
                continue;
            }
            if start + width + gap > view_end {
                break;
            }

            if leftmost_start.is_none() {
                leftmost_start = Some(start);
            }
            if index == self.focused_column {
                active_visible = true;
            }
            width_taken += width + gap;
        }

        if !active_visible {
            return false;
        }
        let free_space = i32::from(self.output_width) - width_taken + gap;
        let centered =
            leftmost_start.expect("a visible active column has a left edge") - free_space / 2;
        let changed = centered != self.view_offset;
        self.view_offset = centered;
        changed
    }

    pub fn expand_focused_column_to_available_width(&mut self) -> bool {
        if self.column_count == 0 || self.columns[self.focused_column].maximized {
            return false;
        }

        let view_start = self.view_offset;
        let view_end = view_start + i32::from(self.output_width);
        let gap = i32::from(self.config.gaps);
        let mut width_taken = 0i32;
        let mut leftmost_start = None;
        let mut active_visible = false;
        let mut non_active_visible = false;

        for index in 0..self.column_count {
            let start = self.column_start(index);
            let width = i32::from(self.effective_column_width(index));
            if start < view_start + gap {
                continue;
            }
            if start + width + gap > view_end {
                break;
            }

            if leftmost_start.is_none() {
                leftmost_start = Some(start);
            }
            if index == self.focused_column {
                active_visible = true;
            } else {
                non_active_visible = true;
            }
            width_taken += width + gap;
        }

        if !active_visible {
            return false;
        }
        let available = i32::from(self.output_width) - gap - width_taken;
        if available <= 0 {
            return false;
        }
        if !non_active_visible {
            self.columns[self.focused_column].maximized = true;
            self.ensure_focused_visible();
            return true;
        }

        let current = i32::from(self.columns[self.focused_column].width);
        self.columns[self.focused_column].width = current.saturating_add(available) as u16;
        self.view_offset = leftmost_start.expect("a visible active column has a left edge") - gap;
        self.ensure_focused_visible();
        true
    }

    pub fn switch_preset_window_height(&mut self) -> bool {
        self.switch_preset_window_height_in_direction(false)
    }

    pub fn switch_preset_window_height_back(&mut self) -> bool {
        self.switch_preset_window_height_in_direction(true)
    }

    fn switch_preset_window_height_in_direction(&mut self, backwards: bool) -> bool {
        if self.column_count == 0 || self.config.preset_window_heights.is_empty() {
            return false;
        }
        let column_index = self.focused_column;
        let focused_window = self.columns[column_index].focused_window;
        if focused_window >= self.columns[column_index].window_count {
            return false;
        }
        let available_height = self.output_height.saturating_sub(self.reserved_top);
        let current = self.resolved_window_heights(column_index)[focused_window];
        let presets = self.config.preset_window_heights;
        let target_index = next_preset_index(
            presets,
            current,
            available_height,
            self.config.gaps,
            backwards,
        );
        self.change_focused_window_height(ColumnWidthChange::Set(
            presets
                .get(target_index)
                .expect("preset index stays within the fixed list"),
        ))
        .unwrap_or(false)
    }

    fn switch_preset_column_width_in_direction(&mut self, backwards: bool) -> bool {
        if self.column_count == 0 || self.config.preset_column_widths.is_empty() {
            return false;
        }
        let presets = self.config.preset_column_widths;
        let current = self.effective_column_width(self.focused_column);
        let was_maximized = self.columns[self.focused_column].maximized;
        let was_maximized_to_edges = self.columns[self.focused_column].maximized_to_edges;
        let target_index = next_preset_index(
            presets,
            current,
            self.output_width,
            self.config.gaps,
            backwards,
        );
        let width = presets
            .get(target_index)
            .expect("preset index stays within the fixed list")
            .resolve(self.output_width, self.config.gaps);
        if width == current && !was_maximized && !was_maximized_to_edges {
            return false;
        }
        self.columns[self.focused_column].width = width;
        self.columns[self.focused_column].maximized = false;
        self.columns[self.focused_column].maximized_to_edges = false;
        self.ensure_focused_visible();
        true
    }

    pub fn set_focused_column_width(&mut self, width: ColumnWidth) -> Result<(), LayoutError> {
        self.change_focused_column_width(ColumnWidthChange::Set(width))
            .map(|_| ())
    }

    pub fn change_focused_column_width(
        &mut self,
        change: ColumnWidthChange,
    ) -> Result<bool, LayoutError> {
        let maximized_width = self.maximized_column_width();
        let column = self
            .columns
            .get_mut(self.focused_column)
            .filter(|_| self.column_count != 0)
            .ok_or(LayoutError::NoFocusedWindow)?;
        let was_maximized = column.maximized;
        let was_maximized_to_edges = column.maximized_to_edges;
        let previous = if was_maximized_to_edges {
            self.output_width
        } else if was_maximized {
            maximized_width
        } else {
            column.width
        };
        let adjusted = match change {
            ColumnWidthChange::Set(width) => {
                i32::from(width.resolve(self.output_width, self.config.gaps))
            }
            ColumnWidthChange::AdjustProportion(thousandths) => {
                let delta = i32::from(self.output_width.saturating_sub(self.config.gaps))
                    * i32::from(thousandths)
                    / 1000;
                i32::from(previous).saturating_add(delta)
            }
            ColumnWidthChange::AdjustFixed(pixels) => i32::from(previous).saturating_add(pixels),
        };
        column.width = adjusted.clamp(1, i32::from(u16::MAX)) as u16;
        column.maximized = false;
        column.maximized_to_edges = false;
        let changed = was_maximized || was_maximized_to_edges || column.width != previous;
        self.ensure_focused_visible();
        Ok(changed)
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let maximum = self.maximum_view_offset();
        self.view_offset = self.view_offset.saturating_add(delta).clamp(0, maximum);
    }

    pub fn tile_rect(&self, window: u32) -> Result<Rect, LayoutError> {
        let (column_index, window_index) =
            self.find_window(window).ok_or(LayoutError::UnknownWindow)?;
        if self.columns[column_index].maximized_to_edges {
            return Ok(Rect {
                x: 0,
                y: i32::from(self.reserved_top),
                width: self.output_width,
                height: self.output_height.saturating_sub(self.reserved_top),
            });
        }
        let gaps = i32::from(self.config.gaps);
        let x = self.column_start(column_index) - self.view_offset;
        let heights = self.resolved_window_heights(column_index);
        if self.columns[column_index].display == ColumnDisplay::Tabbed {
            return Ok(Rect {
                x,
                y: i32::from(self.reserved_top) + gaps,
                width: self.effective_column_width(column_index),
                height: heights[window_index],
            });
        }
        let y = i32::from(self.reserved_top)
            + gaps
            + heights[..window_index]
                .iter()
                .map(|height| i32::from(*height) + gaps)
                .sum::<i32>();
        Ok(Rect {
            x,
            y,
            width: self.effective_column_width(column_index),
            height: heights[window_index],
        })
    }

    fn resolved_window_heights(&self, column_index: usize) -> [u16; WINDOWS] {
        let column = self.columns[column_index];
        if column.display == ColumnDisplay::Tabbed {
            let maximum = self
                .output_height
                .saturating_sub(self.reserved_top)
                .saturating_sub(self.config.gaps.saturating_mul(2))
                .max(1);
            let height = if column.tabbed_height == 0 {
                maximum
            } else {
                column.tabbed_height.min(maximum)
            };
            let mut heights = [0; WINDOWS];
            heights[..column.window_count].fill(height);
            return heights;
        }
        if column.window_count == 0 || column.window_heights[0] != 0 {
            return column.window_heights;
        }
        let available_height = i32::from(self.output_height.saturating_sub(self.reserved_top));
        let total_gaps = i32::from(self.config.gaps) * (column.window_count as i32 + 1);
        let equal_height = ((available_height - total_gaps) / column.window_count as i32)
            .clamp(1, i32::from(u16::MAX)) as u16;
        let mut heights = [0; WINDOWS];
        heights[..column.window_count].fill(equal_height);
        heights
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

    fn consume_focused_singleton_into_adjacent(&mut self, left: bool) -> bool {
        let source = self.focused_column;
        let target = if left { source - 1 } else { source + 1 };
        if self.columns[target].window_count == WINDOWS {
            return false;
        }
        let window = self.columns[source].windows[0];
        for index in source..self.column_count - 1 {
            self.columns[index] = self.columns[index + 1];
        }
        self.column_count -= 1;
        self.columns[self.column_count] = Column::empty();

        let destination = if left { source - 1 } else { source };
        let destination_row = self.columns[destination].window_count;
        self.columns[destination].windows[destination_row] = window;
        self.columns[destination].window_count += 1;
        self.columns[destination].focused_window = destination_row;
        self.columns[destination].maximized_to_edges = false;
        self.columns[destination].reset_window_heights();
        self.focused_column = destination;
        self.ensure_focused_visible();
        true
    }

    fn extract_focused_window_to_side(
        &mut self,
        left: bool,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.column_count == 0
            || self.column_count == COLUMNS
            || self.columns[self.focused_column].window_count <= 1
        {
            return false;
        }

        let source = self.focused_column;
        let source_row = self.columns[source].focused_window;
        let window = self.columns[source].windows[source_row];
        let width = self.columns[source].width;
        for index in source_row..self.columns[source].window_count - 1 {
            self.columns[source].windows[index] = self.columns[source].windows[index + 1];
            self.columns[source].window_heights[index] =
                self.columns[source].window_heights[index + 1];
        }
        self.columns[source].window_count -= 1;
        self.columns[source].focused_window = source_row.min(self.columns[source].window_count - 1);
        self.columns[source].reset_window_heights();

        let destination = if left { source } else { source + 1 };
        for index in (destination..self.column_count).rev() {
            self.columns[index + 1] = self.columns[index];
        }
        let mut column = Column::empty();
        column.windows[0] = window;
        column.window_count = 1;
        column.display = display.unwrap_or(self.config.default_column_display);
        column.width = width;
        self.columns[destination] = column;
        self.column_count += 1;
        self.focused_column = destination;
        self.ensure_focused_visible();
        true
    }

    fn column_start(&self, column_index: usize) -> i32 {
        let mut x = i32::from(self.config.gaps);
        for index in 0..column_index {
            x += i32::from(self.effective_column_width(index)) + i32::from(self.config.gaps);
        }
        x
    }

    fn maximized_column_width(&self) -> u16 {
        self.output_width
            .saturating_sub(self.config.gaps.saturating_mul(2))
            .max(1)
    }

    fn effective_column_width(&self, column_index: usize) -> u16 {
        if self.columns[column_index].maximized_to_edges {
            self.output_width.max(1)
        } else if self.columns[column_index].maximized {
            self.maximized_column_width()
        } else {
            self.columns[column_index].width
        }
    }

    fn maximum_view_offset(&self) -> i32 {
        if self.column_count == 0 {
            return 0;
        }
        let last = self.column_count - 1;
        (self.column_start(last)
            + i32::from(self.effective_column_width(last))
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
        if self.columns[self.focused_column].maximized_to_edges {
            self.view_offset = start;
            return;
        }
        let end = start + i32::from(self.effective_column_width(self.focused_column));
        let output_width = i32::from(self.output_width);
        let gap = i32::from(self.config.gaps);
        let centered = (start + end) / 2 - output_width / 2;
        let should_center = self.config.center_focused_column == CenterFocusedColumn::Always
            || (self.config.always_center_single_column && self.column_count == 1)
            || (self.config.center_focused_column == CenterFocusedColumn::OnOverflow
                && (start < self.view_offset + gap || end > self.view_offset + output_width - gap));
        if should_center {
            self.view_offset = centered;
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
                Token::Identifier("default-column-display") => {
                    config.default_column_display = match self.value_string()? {
                        "normal" => ColumnDisplay::Normal,
                        "tabbed" => ColumnDisplay::Tabbed,
                        _ => return Err(ConfigError::InvalidColumnDisplay),
                    };
                    self.finish_node()?;
                }
                Token::Identifier("default-column-width") => {
                    self.expect_block()?;
                    config.default_column_width = self.parse_column_width()?;
                }
                Token::Identifier("preset-column-widths") => {
                    self.expect_block()?;
                    config.preset_column_widths = self.parse_preset_sizes()?;
                }
                Token::Identifier("preset-window-heights") => {
                    self.expect_block()?;
                    config.preset_window_heights = self.parse_preset_sizes()?;
                }
                Token::Identifier("focus-ring") => {
                    self.expect_block()?;
                    config.focus_ring = self.parse_focus_ring(config.focus_ring)?;
                }
                Token::Identifier("border") => {
                    self.expect_block()?;
                    config.border = self.parse_border(config.border)?;
                }
                Token::Identifier("shadow") => {
                    self.expect_block()?;
                    config.shadow = self.parse_shadow(config.shadow)?;
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

    fn parse_preset_sizes(&mut self) -> Result<PresetSizes, ConfigError> {
        let mut sizes = PresetSizes::empty();
        loop {
            match self.next_non_end_node() {
                Token::RightBrace if sizes.is_empty() => return Ok(PresetSizes::defaults()),
                Token::RightBrace => return Ok(sizes),
                Token::Identifier("proportion") => {
                    sizes.push(ColumnWidth::Proportion(parse_thousandths(
                        self.value_number()?,
                    )?))?;
                    self.finish_node()?;
                }
                Token::Identifier("fixed") => {
                    sizes.push(ColumnWidth::Fixed(parse_rounded_u16(self.value_number()?)?))?;
                    self.finish_node()?;
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

    fn parse_border(&mut self, mut border: Border) -> Result<Border, ConfigError> {
        loop {
            match self.next_non_end_node() {
                Token::RightBrace => return Ok(border),
                Token::Identifier("on") => {
                    border.enabled = true;
                    self.finish_node()?;
                }
                Token::Identifier("off") => {
                    border.enabled = false;
                    self.finish_node()?;
                }
                Token::Identifier("width") => {
                    border.width = parse_rounded_u16(self.value_number()?)?;
                    self.finish_node()?;
                }
                Token::Identifier("active-color") => {
                    border.active_color = parse_color(self.value_string()?)?;
                    self.finish_node()?;
                }
                Token::Identifier("inactive-color") => {
                    border.inactive_color = parse_color(self.value_string()?)?;
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

    fn parse_shadow(&mut self, mut shadow: Shadow) -> Result<Shadow, ConfigError> {
        loop {
            match self.next_non_end_node() {
                Token::RightBrace => return Ok(shadow),
                Token::Identifier("on") => {
                    shadow.enabled = true;
                    self.finish_node()?;
                }
                Token::Identifier("off") => {
                    shadow.enabled = false;
                    self.finish_node()?;
                }
                Token::Identifier("softness") => {
                    shadow.softness = parse_rounded_i32(self.value_number()?, 0, 1024)? as u16;
                    self.finish_node()?;
                }
                Token::Identifier("spread") => {
                    shadow.spread = parse_rounded_i32(self.value_number()?, -1024, 1024)? as i16;
                    self.finish_node()?;
                }
                Token::Identifier("offset") => {
                    let (x, y) = self.parse_shadow_offset()?;
                    shadow.offset_x = x;
                    shadow.offset_y = y;
                }
                Token::Identifier("draw-behind-window") => {
                    shadow.draw_behind_window = match self.next() {
                        Token::Identifier("true") => true,
                        Token::Identifier("false") => false,
                        _ => return Err(ConfigError::InvalidShadow),
                    };
                    self.finish_node()?;
                }
                Token::Identifier("color") => {
                    shadow.color = parse_shadow_color(self.value_string()?)?;
                    self.finish_node()?;
                }
                Token::Identifier("inactive-color") => {
                    shadow.inactive_color = Some(parse_shadow_color(self.value_string()?)?);
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

    fn parse_shadow_offset(&mut self) -> Result<(i32, i32), ConfigError> {
        let mut x = 0;
        let mut y = 0;
        loop {
            match self.next() {
                Token::Identifier("x") => {
                    if self.next() != Token::Other {
                        return Err(ConfigError::InvalidShadow);
                    }
                    x = parse_rounded_i32(self.value_number()?, -65_535, 65_535)?;
                }
                Token::Identifier("y") => {
                    if self.next() != Token::Other {
                        return Err(ConfigError::InvalidShadow);
                    }
                    y = parse_rounded_i32(self.value_number()?, -65_535, 65_535)?;
                }
                Token::EndNode | Token::End => return Ok((x, y)),
                Token::RightBrace => {
                    self.push(Token::RightBrace);
                    return Ok((x, y));
                }
                _ => return Err(ConfigError::InvalidShadow),
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

fn parse_rounded_i32(value: &str, minimum: i32, maximum: i32) -> Result<i32, ConfigError> {
    let (negative, magnitude) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else if let Some(value) = value.strip_prefix('+') {
        (false, value)
    } else {
        (false, value)
    };
    if magnitude.is_empty() {
        return Err(ConfigError::InvalidNumber);
    }
    let rounded = i32::try_from(
        parse_decimal_thousandths(magnitude)?
            .checked_add(500)
            .ok_or(ConfigError::InvalidNumber)?
            / 1000,
    )
    .map_err(|_| ConfigError::InvalidNumber)?;
    let value = if negative { -rounded } else { rounded };
    if !(minimum..=maximum).contains(&value) {
        return Err(ConfigError::InvalidNumber);
    }
    Ok(value)
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
    Ok(parse_shadow_color(value)?.rgb)
}

fn parse_shadow_color(value: &str) -> Result<ShadowColor, ConfigError> {
    let hex = value.strip_prefix('#').ok_or(ConfigError::InvalidColor)?;
    let (red, green, blue, alpha) = match hex.len() {
        3 | 4 => {
            let mut digits = [0u8; 4];
            for (index, byte) in hex.bytes().enumerate() {
                digits[index] = hex_digit(byte).ok_or(ConfigError::InvalidColor)? * 17;
            }
            (
                digits[0],
                digits[1],
                digits[2],
                if hex.len() == 4 { digits[3] } else { 255 },
            )
        }
        6 | 8 => {
            let mut bytes = [0u8; 4];
            for (index, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
                let high = hex_digit(pair[0]).ok_or(ConfigError::InvalidColor)?;
                let low = hex_digit(pair[1]).ok_or(ConfigError::InvalidColor)?;
                bytes[index] = high * 16 + low;
            }
            (
                bytes[0],
                bytes[1],
                bytes[2],
                if hex.len() == 8 { bytes[3] } else { 255 },
            )
        }
        _ => return Err(ConfigError::InvalidColor),
    };
    Ok(ShadowColor {
        rgb: u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue),
        opacity: ((u32::from(alpha) * 1000 + 127) / 255) as u16,
    })
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
                default-column-display "tabbed"
                default-column-width { proportion 0.625; }
                preset-column-widths {
                    proportion 0.333
                    proportion 0.5
                    fixed 900
                }
                preset-window-heights {
                    fixed 240
                    proportion 0.5
                }
                focus-ring {
                    width 3
                    active-color "#89b4fa"
                    inactive-color "#45475a80"
                }
                shadow {
                    on
                    softness 18.4
                    spread -3.6
                    offset x=-7.4 y=9.6
                    draw-behind-window true
                    color "#123b"
                    inactive-color "#10203080"
                }
                background-color "#1e1e2e"
                border {
                    on
                    width 2
                    active-color "#fab387"
                    inactive-color "#585b70"
                }
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
        assert_eq!(config.default_column_display, ColumnDisplay::Tabbed);
        assert_eq!(config.default_column_width, ColumnWidth::Proportion(625));
        assert_eq!(config.preset_column_widths.len(), 3);
        assert_eq!(
            config.preset_column_widths.get(0),
            Some(ColumnWidth::Proportion(333))
        );
        assert_eq!(
            config.preset_column_widths.get(2),
            Some(ColumnWidth::Fixed(900))
        );
        assert_eq!(config.preset_window_heights.len(), 2);
        assert_eq!(
            config.preset_window_heights.get(0),
            Some(ColumnWidth::Fixed(240))
        );
        assert_eq!(config.focus_ring.width, 3);
        assert_eq!(config.focus_ring.active_color, 0x89_b4_fa);
        assert_eq!(config.focus_ring.inactive_color, 0x45_47_5a);
        assert!(config.border.enabled);
        assert_eq!(config.border.width, 2);
        assert_eq!(config.border.active_color, 0xfa_b3_87);
        assert_eq!(config.border.inactive_color, 0x58_5b_70);
        assert!(config.shadow.enabled);
        assert_eq!(config.shadow.softness, 18);
        assert_eq!(config.shadow.spread, -4);
        assert_eq!(config.shadow.offset_x, -7);
        assert_eq!(config.shadow.offset_y, 10);
        assert!(config.shadow.draw_behind_window);
        assert_eq!(
            config.shadow.color,
            ShadowColor {
                rgb: 0x11_22_33,
                opacity: 733,
            }
        );
        assert_eq!(
            config.shadow.inactive_color,
            Some(ShadowColor {
                rgb: 0x10_20_30,
                opacity: 502,
            })
        );
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
        assert_eq!(
            parse_niri_layout(
                "layout {
                    preset-column-widths {}
                    preset-window-heights {}
                }"
            )
            .unwrap()
            .preset_window_heights,
            LayoutConfig::default().preset_window_heights
        );
    }

    #[test]
    fn rejects_invalid_supported_values() {
        assert_eq!(
            parse_niri_layout(r#"layout { center-focused-column "sometimes"; }"#),
            Err(ConfigError::InvalidCenterPolicy)
        );
        assert_eq!(
            parse_niri_layout(r#"layout { default-column-display "stacked"; }"#),
            Err(ConfigError::InvalidColumnDisplay)
        );
        assert_eq!(
            parse_niri_layout("layout { default-column-width { proportion 1.5; } }"),
            Err(ConfigError::InvalidColumnWidth)
        );
        assert_eq!(
            parse_niri_layout(r##"layout { background-color "#xyzxyz"; }"##),
            Err(ConfigError::InvalidColor)
        );
        for input in [
            "layout { shadow { softness -1; } }",
            "layout { shadow { softness 1025; } }",
            "layout { shadow { spread -1025; } }",
            "layout { shadow { offset x=65536; } }",
            "layout { shadow { draw-behind-window maybe; } }",
            r##"layout { shadow { color "#12"; } }"##,
        ] {
            assert!(parse_niri_layout(input).is_err(), "{input}");
        }
        assert_eq!(
            parse_niri_layout(
                "layout { preset-window-heights {
                    fixed 1; fixed 2; fixed 3; fixed 4; fixed 5;
                    fixed 6; fixed 7; fixed 8; fixed 9;
                } }"
            ),
            Err(ConfigError::InvalidColumnWidth)
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
        assert_eq!(first.width, 476);
        assert_eq!(first.width, first_after.width);
        assert_eq!(first_strip_x, first_after.x + layout.view_offset());
        assert!(second.x > first.x);
        assert_eq!(layout.focused_window(), Some(20));
    }

    #[test]
    fn focuses_and_moves_columns_to_strip_boundaries() {
        let mut layout = ScrollLayout::<4, 2>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(10).unwrap();
        layout.open_window(20).unwrap();
        layout.open_window(30).unwrap();
        layout.consume_window(40).unwrap();

        assert!(layout.focus_column_first());
        assert_eq!(layout.focused_window(), Some(10));
        assert!(!layout.focus_column_first());
        assert!(layout.focus_column_last());
        assert_eq!(layout.focused_window(), Some(40));
        assert!(!layout.focus_column_last());

        assert!(layout.move_column_to_first());
        assert_eq!(layout.focused_window(), Some(40));
        assert_eq!(layout.tile_rect(30).unwrap().x + layout.view_offset(), 16);
        assert_eq!(
            layout.tile_rect(40).unwrap().x,
            layout.tile_rect(30).unwrap().x
        );
        assert_eq!(layout.tile_rect(10).unwrap().x + layout.view_offset(), 508);
        assert!(layout.move_column_to_last());
        assert_eq!(layout.focused_window(), Some(40));
        assert!(layout.tile_rect(30).unwrap().x > layout.tile_rect(20).unwrap().x);
        assert_eq!(
            layout.tile_rect(40).unwrap().x,
            layout.tile_rect(30).unwrap().x
        );
        assert!(!layout.move_column_to_last());
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
        assert!(layout.center_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().x, 200);
        assert!(layout.focus_column_right());
        assert!(layout.tile_rect(2).unwrap().x + 600 <= 1000);
    }

    #[test]
    fn stacks_windows_vertically_with_stable_column_width() {
        let mut layout = ScrollLayout::<2, 3>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        assert!(layout.focus_column_left());
        assert!(layout.consume_window_into_column());
        let top = layout.tile_rect(1).unwrap();
        let bottom = layout.tile_rect(2).unwrap();
        assert_eq!(top.x, bottom.x);
        assert_eq!(top.width, bottom.width);
        assert!(bottom.y > top.y);
        assert_eq!(layout.focused_window(), Some(2));
        assert!(
            layout
                .change_focused_window_height(ColumnWidthChange::AdjustProportion(100))
                .unwrap()
        );
        let taller_bottom = layout.tile_rect(2).unwrap();
        let shorter_top = layout.tile_rect(1).unwrap();
        assert_eq!(taller_bottom.height, bottom.height + 65);
        assert_eq!(shorter_top.height, top.height - 65);
        assert!(layout.reset_focused_window_height());
        assert_eq!(layout.tile_rect(1).unwrap(), top);
        assert_eq!(layout.tile_rect(2).unwrap(), bottom);
        assert!(!layout.reset_focused_window_height());
        assert!(layout.switch_preset_window_height());
        assert_eq!(layout.tile_rect(2).unwrap().height, 420);
        assert!(layout.switch_preset_window_height_back());
        assert_eq!(layout.tile_rect(2).unwrap().height, 311);
        assert!(layout.reset_focused_window_height());
        assert!(
            layout
                .change_focused_window_height(ColumnWidthChange::AdjustProportion(100))
                .unwrap()
        );
        assert!(
            layout
                .change_focused_window_height(ColumnWidthChange::AdjustProportion(-100))
                .unwrap()
        );
        assert_eq!(layout.tile_rect(1).unwrap(), top);
        assert_eq!(layout.tile_rect(2).unwrap(), bottom);
        assert!(layout.move_window_up());
        assert_eq!(layout.focused_window(), Some(2));
        assert!(layout.tile_rect(2).unwrap().y < layout.tile_rect(1).unwrap().y);
        assert!(!layout.move_window_up());
        assert!(layout.move_window_down());
        assert_eq!(layout.focused_window(), Some(2));
        assert!(layout.tile_rect(2).unwrap().y > layout.tile_rect(1).unwrap().y);
        assert!(!layout.move_window_down());
        assert!(layout.focus_window_up());
        assert_eq!(layout.focused_window(), Some(1));
        assert!(layout.focus_window_down());
        assert!(layout.expel_window_from_column());
        assert_eq!(layout.focused_window(), Some(1));
        assert!(layout.tile_rect(2).unwrap().x > layout.tile_rect(1).unwrap().x);
    }

    #[test]
    fn toggles_a_column_between_vertical_tiles_and_tabs() {
        let config = LayoutConfig {
            always_center_single_column: true,
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<2, 3>::new(1000, 700, 30, config);
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        assert!(layout.focus_column_left());
        assert!(layout.consume_window_into_column());

        assert!(layout.toggle_focused_column_tabbed_display());
        assert!(!layout.window_is_visible(1));
        assert!(layout.window_is_visible(2));
        assert_eq!(
            layout.tabbed_column_info(2),
            Some(TabbedColumnInfo {
                active_tab: 1,
                tab_count: 2,
            })
        );
        assert_eq!(
            layout.tile_rect(1).unwrap(),
            Rect {
                x: 262,
                y: 46,
                width: 476,
                height: 638,
            }
        );
        assert_eq!(layout.tile_rect(1).unwrap(), layout.tile_rect(2).unwrap());

        assert!(layout.focus_window_up());
        assert_eq!(layout.focused_window(), Some(1));
        assert!(layout.window_is_visible(1));
        assert!(!layout.window_is_visible(2));
        assert_eq!(layout.tabbed_column_info(1).unwrap().active_tab, 0);
        assert!(
            layout
                .change_focused_window_height(ColumnWidthChange::Set(ColumnWidth::Fixed(240)))
                .unwrap()
        );
        assert_eq!(layout.tile_rect(1).unwrap().height, 240);
        assert_eq!(layout.tile_rect(2).unwrap().height, 240);
        assert!(layout.switch_preset_window_height());
        assert_eq!(layout.tile_rect(1).unwrap().height, 311);
        assert!(layout.reset_focused_window_height());
        assert_eq!(layout.tile_rect(1).unwrap().height, 638);

        assert!(layout.toggle_focused_column_tabbed_display());
        assert!(layout.window_is_visible(1));
        assert!(layout.window_is_visible(2));
        assert_eq!(layout.tabbed_column_info(1), None);
        assert_eq!(layout.tile_rect(1).unwrap().y, 46);
        assert_eq!(layout.tile_rect(2).unwrap().y, 373);

        let default_tabbed = LayoutConfig {
            default_column_display: ColumnDisplay::Tabbed,
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<1, 1>::new(1000, 700, 30, default_tabbed);
        layout.open_window(3).unwrap();
        assert_eq!(layout.tabbed_column_info(3).unwrap().tab_count, 1);
    }

    #[test]
    fn consumes_or_expels_the_focused_window_toward_either_side() {
        let config = LayoutConfig {
            always_center_single_column: true,
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<3, 2>::new(1000, 700, 30, config);
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();

        assert!(layout.consume_or_expel_focused_window_left());
        assert_eq!(layout.len(), 1);
        assert_eq!(layout.focused_window(), Some(2));
        assert_eq!(layout.tile_rect(1).unwrap().x, 262);
        assert_eq!(layout.tile_rect(1).unwrap().y, 46);
        assert_eq!(layout.tile_rect(2).unwrap().x, 262);
        assert_eq!(layout.tile_rect(2).unwrap().y, 373);

        assert!(
            layout.consume_or_expel_focused_window_left_with_display(Some(ColumnDisplay::Tabbed))
        );
        assert_eq!(layout.len(), 2);
        assert_eq!(layout.focused_window(), Some(2));
        assert_eq!(layout.tabbed_column_info(2).unwrap().tab_count, 1);
        assert_eq!(layout.tile_rect(2).unwrap().x, 262);
        assert_eq!(layout.tile_rect(1).unwrap().x, 754);

        assert!(layout.consume_or_expel_focused_window_right());
        assert_eq!(layout.len(), 1);
        assert_eq!(layout.focused_window(), Some(2));
        assert_eq!(layout.tile_rect(1).unwrap().x, 262);
        assert_eq!(layout.tile_rect(2).unwrap().y, 373);

        assert!(layout.consume_or_expel_focused_window_right());
        assert_eq!(layout.len(), 2);
        assert_eq!(layout.focused_window(), Some(2));
        assert_eq!(layout.tile_rect(1).unwrap().x, 16);
        assert_eq!(layout.tile_rect(2).unwrap().x, 508);
        assert!(!layout.consume_or_expel_focused_window_right());
        assert!(layout.focus_column_left());
        assert!(!layout.consume_or_expel_focused_window_left());

        let mut full = ScrollLayout::<1, 2>::new(1000, 700, 30, LayoutConfig::default());
        full.open_window(1).unwrap();
        full.consume_window(2).unwrap();
        assert!(!full.consume_or_expel_focused_window_left());
        assert!(!full.consume_or_expel_focused_window_right());
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

    #[test]
    fn changes_the_focused_column_width_with_niri_units() {
        let mut layout = ScrollLayout::<3, 1>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        assert!(layout.switch_preset_column_width());
        assert_eq!(layout.tile_rect(1).unwrap().width, 640);
        assert!(layout.switch_preset_column_width_back());
        assert_eq!(layout.tile_rect(1).unwrap().width, 476);
        assert!(
            layout
                .change_focused_column_width(ColumnWidthChange::AdjustProportion(100))
                .unwrap()
        );
        assert_eq!(layout.tile_rect(1).unwrap().width, 574);
        assert!(
            layout
                .change_focused_column_width(ColumnWidthChange::AdjustFixed(-50))
                .unwrap()
        );
        assert_eq!(layout.tile_rect(1).unwrap().width, 524);
        assert!(
            layout
                .change_focused_column_width(ColumnWidthChange::Set(ColumnWidth::Fixed(720)))
                .unwrap()
        );
        assert_eq!(layout.tile_rect(1).unwrap().width, 720);
        assert!(
            !layout
                .change_focused_column_width(ColumnWidthChange::Set(ColumnWidth::Fixed(720)))
                .unwrap()
        );
        layout
            .change_focused_column_width(ColumnWidthChange::AdjustFixed(i32::MIN))
            .unwrap();
        assert_eq!(layout.tile_rect(1).unwrap().width, 1);
    }

    #[test]
    fn cycles_a_single_window_through_gap_aware_height_presets() {
        let mut layout = ScrollLayout::<1, 1>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        assert_eq!(layout.tile_rect(1).unwrap().height, 638);
        assert!(layout.switch_preset_window_height());
        assert_eq!(layout.tile_rect(1).unwrap().height, 201);
        assert!(layout.switch_preset_window_height_back());
        assert_eq!(layout.tile_rect(1).unwrap().height, 420);
        assert!(layout.reset_focused_window_height());
        assert_eq!(layout.tile_rect(1).unwrap().height, 638);
    }

    #[test]
    fn maximizes_a_column_and_restores_its_previous_width() {
        let mut layout = ScrollLayout::<2, 1>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        assert_eq!(layout.tile_rect(1).unwrap().width, 476);
        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().width, 968);
        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().width, 476);
        assert!(layout.toggle_maximize_focused_column());
        assert!(
            layout
                .change_focused_column_width(ColumnWidthChange::Set(ColumnWidth::Fixed(720)))
                .unwrap()
        );
        assert_eq!(layout.tile_rect(1).unwrap().width, 720);
        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().width, 968);
        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().width, 720);
        assert!(layout.toggle_maximize_focused_column());
        assert!(layout.switch_preset_column_width());
        assert_eq!(layout.tile_rect(1).unwrap().width, 311);
        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().width, 968);
        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(1).unwrap().width, 311);
    }

    #[test]
    fn applies_initial_maximize_to_a_specific_column_without_stealing_focus() {
        let mut layout = ScrollLayout::<2, 1>::new(1000, 700, 30, LayoutConfig::default());
        layout
            .open_window_with_dimensions(
                1,
                Some(ColumnWidth::Proportion(667)),
                Some(ColumnWidth::Proportion(500)),
            )
            .unwrap();
        layout.open_window(2).unwrap();
        assert_eq!(layout.focused_window(), Some(2));

        assert!(layout.set_window_maximized(1, true).unwrap());
        assert_eq!(layout.focused_window(), Some(2));
        assert_eq!(layout.tile_rect(1).unwrap().width, 968);
        assert_eq!(layout.tile_rect(1).unwrap().height, 311);
        assert!(!layout.set_window_maximized(1, true).unwrap());
        assert!(layout.set_window_maximized_to_edges(1, true).unwrap());
        assert_eq!(
            layout.tile_rect(1).unwrap(),
            Rect {
                x: 0,
                y: 30,
                width: 1000,
                height: 670,
            }
        );
        assert_eq!(layout.focused_window(), Some(2));
        assert!(!layout.set_window_maximized_to_edges(1, true).unwrap());
        assert!(layout.set_window_maximized_to_edges(1, false).unwrap());
        assert_eq!(layout.tile_rect(1).unwrap().width, 968);
        assert!(layout.set_window_maximized(1, false).unwrap());
        assert_eq!(layout.tile_rect(1).unwrap().width, 640);
        assert_eq!(layout.tile_rect(1).unwrap().height, 311);
        assert_eq!(
            layout.set_window_maximized(99, true),
            Err(LayoutError::UnknownWindow)
        );
        assert_eq!(
            layout.set_window_maximized_to_edges(99, true),
            Err(LayoutError::UnknownWindow)
        );
    }

    #[test]
    fn maximizes_a_stacked_window_to_working_area_edges() {
        let mut layout = ScrollLayout::<3, 2>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        assert!(layout.focus_column_left());
        assert!(layout.consume_window_into_column());
        assert_eq!(layout.len(), 1);
        assert_eq!(layout.focused_window(), Some(2));

        assert!(layout.toggle_maximize_focused_window_to_edges());
        assert_eq!(layout.len(), 2);
        assert_eq!(layout.focused_window(), Some(2));
        assert_eq!(
            layout.tile_rect(2).unwrap(),
            Rect {
                x: 0,
                y: 30,
                width: 1000,
                height: 670,
            }
        );
        assert!(layout.toggle_maximize_focused_window_to_edges());
        assert_eq!(
            layout.tile_rect(2).unwrap(),
            Rect {
                x: 16,
                y: 46,
                width: 476,
                height: 638,
            }
        );
        assert_eq!(layout.len(), 2);

        assert!(layout.toggle_maximize_focused_column());
        assert_eq!(layout.tile_rect(2).unwrap().width, 968);
        assert!(layout.toggle_maximize_focused_window_to_edges());
        assert_eq!(layout.tile_rect(2).unwrap().width, 1000);
        assert!(layout.toggle_maximize_focused_window_to_edges());
        assert_eq!(layout.tile_rect(2).unwrap().width, 968);
    }

    #[test]
    fn expands_a_column_into_space_not_taken_by_visible_columns() {
        let config = LayoutConfig {
            default_column_width: ColumnWidth::Fixed(300),
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<2, 1>::new(1000, 700, 30, config);
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        assert!(layout.focus_column_left());
        assert!(layout.expand_focused_column_to_available_width());
        assert_eq!(layout.tile_rect(1).unwrap().width, 652);
        assert_eq!(layout.tile_rect(2).unwrap().x, 684);
        assert!(!layout.expand_focused_column_to_available_width());

        let mut single = ScrollLayout::<1, 1>::new(1000, 700, 30, config);
        single.open_window(1).unwrap();
        assert!(single.expand_focused_column_to_available_width());
        assert_eq!(single.tile_rect(1).unwrap().width, 968);
        assert!(!single.expand_focused_column_to_available_width());
        assert!(single.toggle_maximize_focused_column());
        assert_eq!(single.tile_rect(1).unwrap().width, 300);
    }

    #[test]
    fn centers_fully_visible_columns_as_a_group() {
        let config = LayoutConfig {
            default_column_width: ColumnWidth::Fixed(300),
            ..LayoutConfig::default()
        };
        let mut layout = ScrollLayout::<2, 1>::new(1000, 700, 30, config);
        layout.open_window(1).unwrap();
        layout.open_window(2).unwrap();
        assert!(layout.focus_column_left());
        assert!(layout.center_visible_columns());
        assert_eq!(layout.tile_rect(1).unwrap().x, 192);
        assert_eq!(layout.tile_rect(2).unwrap().x, 508);
        assert!(!layout.center_visible_columns());

        let always = LayoutConfig {
            center_focused_column: CenterFocusedColumn::Always,
            ..config
        };
        let mut always_centered = ScrollLayout::<1, 1>::new(1000, 700, 30, always);
        always_centered.open_window(1).unwrap();
        assert!(!always_centered.center_visible_columns());
    }

    #[test]
    fn moves_the_focused_column_without_changing_its_contents() {
        let mut layout = ScrollLayout::<3, 2>::new(1000, 700, 30, LayoutConfig::default());
        layout.open_window(10).unwrap();
        layout.consume_window(11).unwrap();
        layout.open_window(20).unwrap();
        layout.focus_window(10).unwrap();

        assert!(!layout.move_column_left());
        assert!(layout.move_column_right());
        assert_eq!(layout.focused_column(), Some(1));
        assert_eq!(layout.focused_window(), Some(10));
        assert_eq!(layout.tile_rect(10).unwrap().x, 508);
        assert_eq!(layout.tile_rect(11).unwrap().x, 508);
        assert_eq!(layout.tile_rect(20).unwrap().x, 16);

        assert!(!layout.move_column_right());
        assert!(layout.move_column_left());
        assert_eq!(layout.focused_column(), Some(0));
        assert_eq!(layout.focused_window(), Some(10));
        assert_eq!(layout.tile_rect(10).unwrap().x, 16);
        assert_eq!(layout.tile_rect(20).unwrap().x, 508);
    }

    #[test]
    fn transfers_a_whole_column_or_only_its_focused_window() {
        let mut source = ScrollLayout::<3, 3>::new(1000, 700, 30, LayoutConfig::default());
        let mut destination = ScrollLayout::<3, 3>::new(1000, 700, 30, LayoutConfig::default());
        source.open_window(10).unwrap();
        source.consume_window(11).unwrap();
        assert!(
            source
                .change_focused_column_width(ColumnWidthChange::AdjustFixed(80))
                .unwrap()
        );
        destination.open_window(20).unwrap();

        assert!(source.move_focused_column_to(&mut destination).unwrap());
        assert!(source.is_empty());
        assert_eq!(destination.focused_window(), Some(11));
        assert_eq!(
            destination.tile_rect(10).unwrap().x + destination.view_offset(),
            508
        );
        assert_eq!(
            destination.tile_rect(11).unwrap().x + destination.view_offset(),
            508
        );
        assert_eq!(destination.tile_rect(10).unwrap().width, 556);

        assert!(destination.move_focused_window_to(&mut source).unwrap());
        assert_eq!(source.focused_window(), Some(11));
        assert_eq!(source.tile_rect(11).unwrap().width, 556);
        assert_eq!(destination.focused_window(), Some(10));
        assert!(destination.tile_rect(11).is_err());
        assert_eq!(
            destination.tile_rect(10).unwrap().x + destination.view_offset(),
            508
        );

        let mut full = ScrollLayout::<3, 3>::new(1000, 700, 30, LayoutConfig::default());
        full.open_window(30).unwrap();
        full.open_window(31).unwrap();
        full.open_window(32).unwrap();
        assert_eq!(
            destination.move_focused_column_to(&mut full),
            Err(LayoutError::ColumnCapacity)
        );
        assert_eq!(destination.focused_window(), Some(10));
        assert_eq!(full.focused_window(), Some(32));
    }
}
