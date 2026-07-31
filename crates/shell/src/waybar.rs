// SPDX-License-Identifier: 0BSD

pub const MAX_BAR_MODULES: usize = 16;
pub const MAX_BAR_MODULE_CONFIGS: usize = 24;
pub const MAX_BAR_TEXT: usize = 96;
pub const MAX_BAR_MODES: usize = 8;
pub const MAX_BAR_MODE_NAME: usize = 32;
pub const MAX_BAR_NAME: usize = 32;
pub const MAX_BAR_OUTPUTS: usize = 8;
pub const MAX_BAR_OUTPUT_NAME: usize = 96;
pub const MAX_BAR_OUTPUT_DIMENSIONS: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarPosition {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarLayer {
    Bottom,
    Top,
    Overlay,
}

impl BarLayer {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Overlay => "overlay",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarMode {
    Default,
    Dock,
    Hide,
    Invisible,
    Overlay,
    Custom,
}

impl BarMode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Dock => "dock",
            Self::Hide => "hide",
            Self::Invisible => "invisible",
            Self::Overlay => "overlay",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarSignal {
    User1,
    User2,
}

impl BarSignal {
    pub const fn name(self) -> &'static str {
        match self {
            Self::User1 => "SIGUSR1",
            Self::User2 => "SIGUSR2",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarSignalAction {
    Show,
    Hide,
    Toggle,
    Reload,
    Noop,
}

impl BarSignalAction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Show => "show",
            Self::Hide => "hide",
            Self::Toggle => "toggle",
            Self::Reload => "reload",
            Self::Noop => "noop",
        }
    }

    const fn from_name(name: &str) -> Option<Self> {
        match name.as_bytes() {
            b"show" => Some(Self::Show),
            b"hide" => Some(Self::Hide),
            b"toggle" => Some(Self::Toggle),
            b"reload" => Some(Self::Reload),
            b"noop" => Some(Self::Noop),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarOutputList<'a> {
    outputs: [&'a str; MAX_BAR_OUTPUTS],
    length: usize,
    configured: bool,
    array: bool,
}

impl<'a> BarOutputList<'a> {
    const fn any() -> Self {
        Self {
            outputs: [""; MAX_BAR_OUTPUTS],
            length: 0,
            configured: false,
            array: false,
        }
    }

    const fn string(output: &'a str) -> Self {
        let mut outputs = [""; MAX_BAR_OUTPUTS];
        outputs[0] = output;
        Self {
            outputs,
            length: 1,
            configured: true,
            array: false,
        }
    }

    const fn array() -> Self {
        Self {
            outputs: [""; MAX_BAR_OUTPUTS],
            length: 0,
            configured: true,
            array: true,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub const fn is_configured(self) -> bool {
        self.configured
    }

    pub const fn is_array(self) -> bool {
        self.array
    }

    pub const fn form_name(self) -> &'static str {
        if !self.configured {
            "any"
        } else if self.array {
            "array"
        } else {
            "string"
        }
    }

    pub fn iter(self) -> impl Iterator<Item = &'a str> {
        self.outputs.into_iter().take(self.length)
    }

    pub fn matches(self, name: &str, identifier: &str) -> bool {
        self.matches_with_environment(name, identifier, |_| None::<&str>)
    }

    pub fn matches_with_environment<'environment>(
        self,
        name: &str,
        identifier: &str,
        mut environment: impl FnMut(&str) -> Option<&'environment str>,
    ) -> bool {
        if !self.configured {
            return true;
        }
        if !self.array {
            let output = self.outputs[0];
            if output.is_empty() {
                return true;
            }
            if let Some(excluded) = output.strip_prefix('!') {
                return !output_matches_with_environment(
                    excluded,
                    name,
                    identifier,
                    &mut environment,
                );
            }
            return output_matches_with_environment(output, name, identifier, &mut environment);
        }
        for output in self.iter() {
            if let Some(excluded) = output.strip_prefix('!') {
                if output_matches_with_environment(excluded, name, identifier, &mut environment) {
                    return false;
                }
                continue;
            }
            if output_matches_with_environment(output, name, identifier, &mut environment) {
                return true;
            }
            if output.starts_with('*') {
                return true;
            }
        }
        false
    }

    const fn takes_precedence(self) -> bool {
        self.configured && (self.array || !self.outputs[0].is_empty())
    }

    fn push(&mut self, output: &'a str) -> Result<(), BarConfigError> {
        validate_output_name(output)?;
        if self.length == self.outputs.len() {
            return Err(BarConfigError::TooManyOutputs);
        }
        self.outputs[self.length] = output;
        self.length += 1;
        Ok(())
    }
}

fn output_matches(output: &str, name: &str, identifier: &str) -> bool {
    output == name || output == identifier
}

fn output_matches_with_environment<'environment>(
    output: &str,
    name: &str,
    identifier: &str,
    environment: &mut impl FnMut(&str) -> Option<&'environment str>,
) -> bool {
    if let Some(variable) = output.strip_prefix('$')
        && environment(variable).is_some_and(|value| output_matches(value, name, identifier))
    {
        return true;
    }
    output_matches(output, name, identifier)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarOutputDimension {
    WidthLess(i32),
    WidthGreater(i32),
    HeightLess(i32),
    HeightGreater(i32),
}

impl BarOutputDimension {
    const fn matches(self, width: i32, height: i32) -> bool {
        match self {
            Self::WidthLess(value) => width < value,
            Self::WidthGreater(value) => width > value,
            Self::HeightLess(value) => height < value,
            Self::HeightGreater(value) => height > value,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarOutputDimensionList {
    dimensions: [Option<BarOutputDimension>; MAX_BAR_OUTPUT_DIMENSIONS],
    length: usize,
    configured: bool,
    array: bool,
}

impl BarOutputDimensionList {
    const fn any() -> Self {
        Self {
            dimensions: [None; MAX_BAR_OUTPUT_DIMENSIONS],
            length: 0,
            configured: false,
            array: false,
        }
    }

    const fn string() -> Self {
        Self {
            dimensions: [None; MAX_BAR_OUTPUT_DIMENSIONS],
            length: 0,
            configured: true,
            array: false,
        }
    }

    const fn array() -> Self {
        Self {
            dimensions: [None; MAX_BAR_OUTPUT_DIMENSIONS],
            length: 0,
            configured: true,
            array: true,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub const fn is_configured(self) -> bool {
        self.configured
    }

    pub const fn is_array(self) -> bool {
        self.array
    }

    pub const fn form_name(self) -> &'static str {
        if !self.configured {
            "any"
        } else if self.array {
            "array"
        } else {
            "string"
        }
    }

    pub fn iter(self) -> impl Iterator<Item = BarOutputDimension> {
        self.dimensions.into_iter().take(self.length).flatten()
    }

    pub fn matches(self, width: i32, height: i32) -> bool {
        self.iter()
            .all(|dimension| dimension.matches(width, height))
    }

    fn push_text(&mut self, text: &str) -> Result<(), BarConfigError> {
        validate_output_name(text)?;
        let Some(dimension) = parse_output_dimension(text) else {
            return Ok(());
        };
        if self.length == self.dimensions.len() {
            return Err(BarConfigError::TooManyOutputDimensions);
        }
        self.dimensions[self.length] = Some(dimension);
        self.length += 1;
        Ok(())
    }
}

fn parse_output_dimension(text: &str) -> Option<BarOutputDimension> {
    let mut fields = text.split_ascii_whitespace();
    let dimension = fields.next()?;
    let comparator = fields.next()?;
    let value = parse_i32(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }
    match (dimension.as_bytes(), comparator.as_bytes()) {
        (b"width", b"<") => Some(BarOutputDimension::WidthLess(value)),
        (b"width", b">") => Some(BarOutputDimension::WidthGreater(value)),
        (b"height", b"<") => Some(BarOutputDimension::HeightLess(value)),
        (b"height", b">") => Some(BarOutputDimension::HeightGreater(value)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BarModeOptions {
    layer: BarLayer,
    exclusive: bool,
    passthrough: bool,
    visible: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BarModeState<'a> {
    mode: BarMode,
    name: &'a str,
    options: BarModeOptions,
}

impl BarModeState<'_> {
    const fn preset(name: &'static str) -> Self {
        let Some((mode, options)) = BarModeOptions::preset(name) else {
            panic!("invalid built-in Waybar mode")
        };
        Self {
            mode,
            name,
            options,
        }
    }
}

impl BarModeOptions {
    const fn empty_custom() -> Self {
        Self {
            layer: BarLayer::Bottom,
            exclusive: false,
            passthrough: false,
            visible: false,
        }
    }

    const fn preset(name: &str) -> Option<(BarMode, Self)> {
        match name.as_bytes() {
            b"default" => Some((
                BarMode::Default,
                Self {
                    layer: BarLayer::Bottom,
                    exclusive: true,
                    passthrough: false,
                    visible: true,
                },
            )),
            b"dock" => Some((
                BarMode::Dock,
                Self {
                    layer: BarLayer::Bottom,
                    exclusive: true,
                    passthrough: false,
                    visible: true,
                },
            )),
            b"hide" => Some((
                BarMode::Hide,
                Self {
                    layer: BarLayer::Overlay,
                    exclusive: false,
                    passthrough: false,
                    visible: true,
                },
            )),
            b"invisible" => Some((
                BarMode::Invisible,
                Self {
                    layer: BarLayer::Bottom,
                    exclusive: false,
                    passthrough: true,
                    visible: false,
                },
            )),
            b"overlay" => Some((
                BarMode::Overlay,
                Self {
                    layer: BarLayer::Overlay,
                    exclusive: false,
                    passthrough: true,
                    visible: true,
                },
            )),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BarModeDefinition<'a> {
    name: &'a str,
    options: BarModeOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BarModeList<'a> {
    entries: [Option<BarModeDefinition<'a>>; MAX_BAR_MODES],
    length: usize,
}

impl<'a> BarModeList<'a> {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_BAR_MODES],
            length: 0,
        }
    }

    fn get(&self, name: &str) -> Option<BarModeOptions> {
        self.entries[..self.length]
            .iter()
            .flatten()
            .find(|mode| mode.name == name)
            .map(|mode| mode.options)
    }

    fn push(&mut self, mode: BarModeDefinition<'a>) -> Result<(), BarConfigError> {
        if self.get(mode.name).is_some() {
            return Err(BarConfigError::DuplicateField);
        }
        if self.length == self.entries.len() {
            return Err(BarConfigError::TooManyModes);
        }
        self.entries[self.length] = Some(mode);
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarButton {
    Left,
    Middle,
    Right,
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarModuleList<'a> {
    modules: [&'a str; MAX_BAR_MODULES],
    length: usize,
}

impl<'a> BarModuleList<'a> {
    const fn empty() -> Self {
        Self {
            modules: [""; MAX_BAR_MODULES],
            length: 0,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn iter(self) -> impl Iterator<Item = &'a str> {
        self.modules.into_iter().take(self.length)
    }

    fn push(&mut self, module: &'a str) -> Result<(), BarConfigError> {
        if self.length == MAX_BAR_MODULES {
            return Err(BarConfigError::TooManyModules);
        }
        if module.is_empty() || module.len() > 64 {
            return Err(BarConfigError::InvalidModule);
        }
        self.modules[self.length] = module;
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarModuleConfig<'a> {
    pub name: &'a str,
    pub format: Option<&'a str>,
    pub format_alt: Option<&'a str>,
    pub format_alt_click: BarButton,
    pub format_disconnected: Option<&'a str>,
    pub interval: Option<u16>,
    pub tooltip: Option<bool>,
    pub min_length: Option<u16>,
    pub max_length: Option<u16>,
    pub on_click: Option<&'a str>,
    pub on_click_right: Option<&'a str>,
    pub on_click_middle: Option<&'a str>,
    pub on_scroll_up: Option<&'a str>,
    pub on_scroll_down: Option<&'a str>,
}

impl<'a> BarModuleConfig<'a> {
    const fn empty(name: &'a str) -> Self {
        Self {
            name,
            format: None,
            format_alt: None,
            format_alt_click: BarButton::Left,
            format_disconnected: None,
            interval: None,
            tooltip: None,
            min_length: None,
            max_length: None,
            on_click: None,
            on_click_right: None,
            on_click_middle: None,
            on_scroll_up: None,
            on_scroll_down: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarModuleConfigList<'a> {
    entries: [Option<BarModuleConfig<'a>>; MAX_BAR_MODULE_CONFIGS],
    length: usize,
}

impl<'a> BarModuleConfigList<'a> {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_BAR_MODULE_CONFIGS],
            length: 0,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn get(self, name: &str) -> Option<BarModuleConfig<'a>> {
        self.entries[..self.length]
            .iter()
            .flatten()
            .find(|module| module.name == name)
            .copied()
    }

    pub fn index_of(self, name: &str) -> Option<usize> {
        self.entries[..self.length]
            .iter()
            .position(|module| module.is_some_and(|module| module.name == name))
    }

    fn push(&mut self, module: BarModuleConfig<'a>) -> Result<(), BarConfigError> {
        if self.get(module.name).is_some() {
            return Err(BarConfigError::DuplicateField);
        }
        if self.length == self.entries.len() {
            return Err(BarConfigError::TooManyModuleConfigs);
        }
        self.entries[self.length] = Some(module);
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaybarConfig<'a> {
    pub name: Option<&'a str>,
    pub output: BarOutputList<'a>,
    pub output_dimensions: BarOutputDimensionList,
    pub position: BarPosition,
    pub height: u16,
    pub width: u16,
    pub spacing: u16,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub fixed_center: bool,
    pub expand_left: bool,
    pub expand_center: bool,
    pub expand_right: bool,
    pub no_center: bool,
    pub layer: BarLayer,
    pub mode: BarMode,
    pub mode_name: &'a str,
    pub exclusive: bool,
    pub passthrough: bool,
    pub visible: bool,
    pub on_sigusr1: BarSignalAction,
    pub on_sigusr2: BarSignalAction,
    pub modules_left: BarModuleList<'a>,
    pub modules_center: BarModuleList<'a>,
    pub modules_right: BarModuleList<'a>,
    pub module_configs: BarModuleConfigList<'a>,
    shown_mode: BarModeState<'a>,
    hidden_mode: BarModeState<'a>,
    visibility_state: bool,
    output_selected: bool,
}

impl Default for WaybarConfig<'_> {
    fn default() -> Self {
        let shown_mode = BarModeState::preset("default");
        let hidden_mode = BarModeState::preset("invisible");
        Self {
            name: None,
            output: BarOutputList::any(),
            output_dimensions: BarOutputDimensionList::any(),
            position: BarPosition::Top,
            height: 30,
            width: 0,
            spacing: 4,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            fixed_center: true,
            expand_left: false,
            expand_center: false,
            expand_right: false,
            no_center: false,
            layer: BarLayer::Bottom,
            mode: BarMode::Default,
            mode_name: "default",
            exclusive: true,
            passthrough: false,
            visible: true,
            on_sigusr1: BarSignalAction::Toggle,
            on_sigusr2: BarSignalAction::Reload,
            modules_left: BarModuleList::empty(),
            modules_center: BarModuleList::empty(),
            modules_right: BarModuleList::empty(),
            module_configs: BarModuleConfigList::empty(),
            shown_mode,
            hidden_mode,
            visibility_state: true,
            output_selected: true,
        }
    }
}

impl<'a> WaybarConfig<'a> {
    pub fn namespace(self) -> &'a str {
        self.name.unwrap_or("waybar")
    }

    pub const fn output_selected(self) -> bool {
        self.output_selected
    }

    pub fn select_output(&mut self, name: &str, identifier: &str, width: i32, height: i32) -> bool {
        self.select_output_with_environment(name, identifier, width, height, |_| None::<&str>)
    }

    pub fn select_output_with_environment<'environment>(
        &mut self,
        name: &str,
        identifier: &str,
        width: i32,
        height: i32,
        environment: impl FnMut(&str) -> Option<&'environment str>,
    ) -> bool {
        self.output_selected = if self.output.takes_precedence() {
            self.output
                .matches_with_environment(name, identifier, environment)
        } else {
            self.output_dimensions.matches(width, height)
        };
        self.apply_mode_state(if self.visibility_state {
            self.shown_mode
        } else {
            self.hidden_mode
        });
        self.output_selected
    }

    pub fn reserved_top(self) -> u16 {
        if !self.visible || !self.exclusive || self.position != BarPosition::Top {
            return 0;
        }
        i32::from(self.height)
            .saturating_add(self.margin_top)
            .clamp(0, i32::from(u16::MAX)) as u16
    }

    pub const fn layer_is_above_windows(self) -> bool {
        matches!(self.layer, BarLayer::Top | BarLayer::Overlay)
    }

    pub fn horizontal_geometry(self, output_width: i32) -> (i32, i32) {
        let available = output_width
            .saturating_sub(self.margin_left)
            .saturating_sub(self.margin_right)
            .max(0);
        if self.width <= 1 {
            return (self.margin_left, available);
        }
        let width = i32::from(self.width).min(available);
        let x = if width
            .saturating_add(self.margin_left)
            .saturating_add(self.margin_right)
            < output_width
        {
            output_width.saturating_sub(width) / 2
        } else {
            self.margin_left
        };
        (x, width)
    }

    pub fn dynamic_center_origin(
        self,
        available_start: i32,
        available_end: i32,
        center_width: i32,
    ) -> i32 {
        let latest_center = available_end.saturating_sub(center_width);
        if latest_center <= available_start {
            return available_start;
        }
        let extra = latest_center.saturating_sub(available_start);
        let expand_count = 1 + usize::from(self.expand_left) + usize::from(self.expand_right);
        let share = extra / i32::try_from(expand_count).unwrap_or(3);
        available_start
            .saturating_add(if self.expand_left { share } else { 0 })
            .saturating_add(if self.expand_center { 0 } else { share / 2 })
    }

    pub const fn visibility_state(self) -> bool {
        self.visibility_state
    }

    pub const fn signal_action(self, signal: BarSignal) -> BarSignalAction {
        match signal {
            BarSignal::User1 => self.on_sigusr1,
            BarSignal::User2 => self.on_sigusr2,
        }
    }

    pub fn set_visibility(&mut self, visible: bool) -> bool {
        if self.visibility_state == visible {
            return false;
        }
        self.visibility_state = visible;
        self.apply_mode_state(if visible {
            self.shown_mode
        } else {
            self.hidden_mode
        });
        true
    }

    pub fn toggle_visibility(&mut self) {
        self.set_visibility(!self.visibility_state);
    }

    fn apply_mode_state(&mut self, state: BarModeState<'a>) {
        self.mode = state.mode;
        self.mode_name = state.name;
        self.layer = state.options.layer;
        self.exclusive = state.options.exclusive;
        self.passthrough = state.options.passthrough;
        self.visible = state.options.visible && self.output_selected;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarConfigError {
    UnexpectedEnd,
    UnexpectedToken,
    DuplicateField,
    InvalidPosition,
    InvalidNumber,
    InvalidModule,
    InvalidModuleOption,
    TooManyModules,
    TooManyModuleConfigs,
    InvalidName,
    InvalidOutput,
    TooManyOutputs,
    TooManyOutputDimensions,
    InvalidMode,
    TooManyModes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarFormatError {
    TooLong,
    InvalidPlaceholder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarFormatValue<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BarText {
    bytes: [u8; MAX_BAR_TEXT],
    length: usize,
}

impl BarText {
    const fn empty() -> Self {
        Self {
            bytes: [0; MAX_BAR_TEXT],
            length: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: format_bar_text only copies UTF-8 template/value byte slices.
        unsafe { core::str::from_utf8_unchecked(&self.bytes[..self.length]) }
    }

    pub const fn len(&self) -> usize {
        self.length
    }

    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn truncate(&mut self, length: usize) {
        self.length = self.length.min(length);
        while self.length > 0 && core::str::from_utf8(&self.bytes[..self.length]).is_err() {
            self.length -= 1;
        }
    }

    pub fn pad_to(&mut self, length: usize) -> Result<(), BarFormatError> {
        while self.length < length {
            self.push_bytes(b" ")?;
        }
        Ok(())
    }

    fn push_bytes(&mut self, bytes: &[u8]) -> Result<(), BarFormatError> {
        let end = self
            .length
            .checked_add(bytes.len())
            .filter(|end| *end <= self.bytes.len())
            .ok_or(BarFormatError::TooLong)?;
        self.bytes[self.length..end].copy_from_slice(bytes);
        self.length = end;
        Ok(())
    }
}

pub fn format_bar_text(
    template: &str,
    default: &str,
    values: &[BarFormatValue<'_>],
) -> Result<BarText, BarFormatError> {
    let mut output = BarText::empty();
    let bytes = template.as_bytes();
    let mut offset = 0usize;
    while offset < bytes.len() {
        if bytes[offset] == b'{' {
            let close = bytes[offset + 1..]
                .iter()
                .position(|byte| *byte == b'}')
                .map(|relative| offset + 1 + relative)
                .ok_or(BarFormatError::InvalidPlaceholder)?;
            let placeholder = &template[offset + 1..close];
            let (name, alignment) = placeholder
                .split_once(':')
                .map_or((placeholder, None), |(name, alignment)| {
                    (name, Some(alignment))
                });
            let value = if name.is_empty() {
                default
            } else {
                values
                    .iter()
                    .find(|value| value.name == name)
                    .map(|value| value.value)
                    .ok_or(BarFormatError::InvalidPlaceholder)?
            };
            if let Some(width) = alignment.and_then(|alignment| alignment.strip_prefix('>')) {
                let width = parse_usize(width).ok_or(BarFormatError::InvalidPlaceholder)?;
                for _ in value.len()..width {
                    output.push_bytes(b" ")?;
                }
            } else if alignment.is_some() {
                return Err(BarFormatError::InvalidPlaceholder);
            }
            output.push_bytes(value.as_bytes())?;
            offset = close + 1;
        } else if bytes[offset] == b'}' {
            return Err(BarFormatError::InvalidPlaceholder);
        } else {
            let start = offset;
            while offset < bytes.len() && !matches!(bytes[offset], b'{' | b'}') {
                offset += 1;
            }
            output.push_bytes(&bytes[start..offset])?;
        }
    }
    Ok(output)
}

pub fn parse_waybar_config(input: &str) -> Result<WaybarConfig<'_>, BarConfigError> {
    JsonParser::new(input).parse()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Token<'a> {
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Colon,
    Comma,
    String(&'a str),
    Number(&'a str),
    Bool(bool),
    Null,
    Invalid,
    End,
}

struct JsonLexer<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> JsonLexer<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn next(&mut self) -> Token<'a> {
        if !self.skip_trivia() {
            return Token::Invalid;
        }
        let bytes = self.input.as_bytes();
        if self.offset == bytes.len() {
            return Token::End;
        }
        let token = match bytes[self.offset] {
            b'{' => Token::LeftBrace,
            b'}' => Token::RightBrace,
            b'[' => Token::LeftBracket,
            b']' => Token::RightBracket,
            b':' => Token::Colon,
            b',' => Token::Comma,
            b'"' => return self.string(),
            b'-' | b'0'..=b'9' => return self.number(),
            b't' if self.consume_literal(b"true") => return Token::Bool(true),
            b'f' if self.consume_literal(b"false") => return Token::Bool(false),
            b'n' if self.consume_literal(b"null") => return Token::Null,
            _ => Token::Invalid,
        };
        self.offset += 1;
        token
    }

    fn skip_trivia(&mut self) -> bool {
        let bytes = self.input.as_bytes();
        loop {
            while self.offset < bytes.len() && bytes[self.offset].is_ascii_whitespace() {
                self.offset += 1;
            }
            if bytes.get(self.offset..self.offset + 2) == Some(b"//") {
                while self.offset < bytes.len() && bytes[self.offset] != b'\n' {
                    self.offset += 1;
                }
                continue;
            }
            if bytes.get(self.offset..self.offset + 2) == Some(b"/*") {
                self.offset += 2;
                let mut closed = false;
                while self.offset + 1 < bytes.len() {
                    if &bytes[self.offset..self.offset + 2] == b"*/" {
                        self.offset += 2;
                        closed = true;
                        break;
                    }
                    self.offset += 1;
                }
                if !closed {
                    return false;
                }
                continue;
            }
            return true;
        }
    }

    fn string(&mut self) -> Token<'a> {
        self.offset += 1;
        let start = self.offset;
        let bytes = self.input.as_bytes();
        let mut escaped = false;
        while self.offset < bytes.len() {
            match bytes[self.offset] {
                b'"' if !escaped => {
                    let value = &self.input[start..self.offset];
                    self.offset += 1;
                    return Token::String(value);
                }
                b'\\' if !escaped => {
                    escaped = true;
                    self.offset += 1;
                }
                _ => {
                    escaped = false;
                    self.offset += 1;
                }
            }
        }
        Token::Invalid
    }

    fn number(&mut self) -> Token<'a> {
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len()
            && matches!(
                bytes[self.offset],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9'
            )
        {
            self.offset += 1;
        }
        Token::Number(&self.input[start..self.offset])
    }

    fn consume_literal(&mut self, literal: &[u8]) -> bool {
        if self
            .input
            .as_bytes()
            .get(self.offset..self.offset + literal.len())
            == Some(literal)
        {
            self.offset += literal.len();
            true
        } else {
            false
        }
    }
}

struct JsonParser<'a> {
    lexer: JsonLexer<'a>,
    pushed: Option<Token<'a>>,
}

impl<'a> JsonParser<'a> {
    const POSITION: u32 = 1 << 0;
    const HEIGHT: u32 = 1 << 1;
    const SPACING: u32 = 1 << 2;
    const LEFT: u32 = 1 << 3;
    const CENTER: u32 = 1 << 4;
    const RIGHT: u32 = 1 << 5;
    const MARGIN: u32 = 1 << 6;
    const MARGIN_TOP: u32 = 1 << 7;
    const MARGIN_RIGHT: u32 = 1 << 8;
    const MARGIN_BOTTOM: u32 = 1 << 9;
    const MARGIN_LEFT: u32 = 1 << 10;
    const FIXED_CENTER: u32 = 1 << 11;
    const EXCLUSIVE: u32 = 1 << 12;
    const LAYER: u32 = 1 << 13;
    const MODE: u32 = 1 << 14;
    const PASSTHROUGH: u32 = 1 << 15;
    const START_HIDDEN: u32 = 1 << 16;
    const MODES: u32 = 1 << 17;
    const VISIBLE: u32 = 1 << 18;
    const ON_SIGUSR1: u32 = 1 << 19;
    const ON_SIGUSR2: u32 = 1 << 20;
    const WIDTH: u32 = 1 << 21;
    const NO_CENTER: u32 = 1 << 22;
    const EXPAND_LEFT: u32 = 1 << 23;
    const EXPAND_CENTER: u32 = 1 << 24;
    const EXPAND_RIGHT: u32 = 1 << 25;
    const NAME: u32 = 1 << 26;
    const OUTPUT: u32 = 1 << 27;
    const OUTPUT_DIMENSIONS: u32 = 1 << 28;

    const fn new(input: &'a str) -> Self {
        Self {
            lexer: JsonLexer::new(input),
            pushed: None,
        }
    }

    fn parse(mut self) -> Result<WaybarConfig<'a>, BarConfigError> {
        self.expect(Token::LeftBrace)?;
        let mut config = WaybarConfig::default();
        let mut fields = 0u32;
        let mut margin = None;
        let mut margin_top = None;
        let mut margin_right = None;
        let mut margin_bottom = None;
        let mut margin_left = None;
        let mut start_hidden = false;
        let mut selected_mode = "default";
        let mut modes = BarModeList::empty();
        loop {
            match self.next() {
                Token::RightBrace => break,
                Token::String(name) => {
                    self.expect(Token::Colon)?;
                    match name {
                        "name" => {
                            mark_once(&mut fields, Self::NAME)?;
                            config.name = Some(self.bar_name_value()?);
                        }
                        "output" => {
                            mark_once(&mut fields, Self::OUTPUT)?;
                            config.output = self.output_value()?;
                            config.output_selected = false;
                        }
                        "output-dimensions" => {
                            mark_once(&mut fields, Self::OUTPUT_DIMENSIONS)?;
                            config.output_dimensions = self.output_dimensions_value()?;
                            config.output_selected = false;
                        }
                        "position" => {
                            mark_once(&mut fields, Self::POSITION)?;
                            config.position = match self.string_value()? {
                                "top" => BarPosition::Top,
                                "bottom" => BarPosition::Bottom,
                                "left" => BarPosition::Left,
                                "right" => BarPosition::Right,
                                _ => return Err(BarConfigError::InvalidPosition),
                            };
                        }
                        "height" => {
                            mark_once(&mut fields, Self::HEIGHT)?;
                            config.height = self.u16_value()?;
                            if config.height == 0 {
                                return Err(BarConfigError::InvalidNumber);
                            }
                        }
                        "width" => {
                            mark_once(&mut fields, Self::WIDTH)?;
                            config.width = self.u16_value()?;
                        }
                        "spacing" => {
                            mark_once(&mut fields, Self::SPACING)?;
                            config.spacing = self.u16_value()?;
                        }
                        "margin" => {
                            mark_once(&mut fields, Self::MARGIN)?;
                            margin = Some(self.margin_value()?);
                        }
                        "margin-top" => {
                            mark_once(&mut fields, Self::MARGIN_TOP)?;
                            margin_top = Some(self.i32_value()?);
                        }
                        "margin-right" => {
                            mark_once(&mut fields, Self::MARGIN_RIGHT)?;
                            margin_right = Some(self.i32_value()?);
                        }
                        "margin-bottom" => {
                            mark_once(&mut fields, Self::MARGIN_BOTTOM)?;
                            margin_bottom = Some(self.i32_value()?);
                        }
                        "margin-left" => {
                            mark_once(&mut fields, Self::MARGIN_LEFT)?;
                            margin_left = Some(self.i32_value()?);
                        }
                        "fixed-center" => {
                            mark_once(&mut fields, Self::FIXED_CENTER)?;
                            config.fixed_center = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "expand-left" => {
                            mark_once(&mut fields, Self::EXPAND_LEFT)?;
                            config.expand_left = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "expand-center" => {
                            mark_once(&mut fields, Self::EXPAND_CENTER)?;
                            config.expand_center = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "expand-right" => {
                            mark_once(&mut fields, Self::EXPAND_RIGHT)?;
                            config.expand_right = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "no-center" => {
                            mark_once(&mut fields, Self::NO_CENTER)?;
                            config.no_center = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "exclusive" => {
                            mark_once(&mut fields, Self::EXCLUSIVE)?;
                            config.exclusive = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "layer" => {
                            mark_once(&mut fields, Self::LAYER)?;
                            config.layer = match self.string_value()? {
                                "bottom" => BarLayer::Bottom,
                                "top" => BarLayer::Top,
                                "overlay" => BarLayer::Overlay,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "mode" => {
                            mark_once(&mut fields, Self::MODE)?;
                            selected_mode = self.mode_name_value()?;
                        }
                        "modes" => {
                            mark_once(&mut fields, Self::MODES)?;
                            modes = self.mode_definitions()?;
                        }
                        "passthrough" => {
                            mark_once(&mut fields, Self::PASSTHROUGH)?;
                            config.passthrough = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "start_hidden" => {
                            mark_once(&mut fields, Self::START_HIDDEN)?;
                            start_hidden = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "visible" => {
                            mark_once(&mut fields, Self::VISIBLE)?;
                            config.visible = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
                        }
                        "on-sigusr1" => {
                            mark_once(&mut fields, Self::ON_SIGUSR1)?;
                            config.on_sigusr1 = BarSignalAction::from_name(self.string_value()?)
                                .unwrap_or(BarSignalAction::Toggle);
                        }
                        "on-sigusr2" => {
                            mark_once(&mut fields, Self::ON_SIGUSR2)?;
                            config.on_sigusr2 = BarSignalAction::from_name(self.string_value()?)
                                .unwrap_or(BarSignalAction::Reload);
                        }
                        "modules-left" => {
                            mark_once(&mut fields, Self::LEFT)?;
                            config.modules_left = self.module_list()?;
                        }
                        "modules-center" => {
                            mark_once(&mut fields, Self::CENTER)?;
                            config.modules_center = self.module_list()?;
                        }
                        "modules-right" => {
                            mark_once(&mut fields, Self::RIGHT)?;
                            config.modules_right = self.module_list()?;
                        }
                        _ => {
                            if let Some(module) = self.module_config_value(name)? {
                                config.module_configs.push(module)?;
                            }
                        }
                    }
                    match self.next() {
                        Token::Comma => {}
                        Token::RightBrace => break,
                        token => {
                            self.push(token);
                            return Err(BarConfigError::UnexpectedToken);
                        }
                    }
                }
                Token::End => return Err(BarConfigError::UnexpectedEnd),
                _ => return Err(BarConfigError::UnexpectedToken),
            }
        }
        if self.next() != Token::End {
            return Err(BarConfigError::UnexpectedToken);
        }
        if margin_top.is_some()
            || margin_right.is_some()
            || margin_bottom.is_some()
            || margin_left.is_some()
        {
            config.margin_top = margin_top.unwrap_or(0);
            config.margin_right = margin_right.unwrap_or(0);
            config.margin_bottom = margin_bottom.unwrap_or(0);
            config.margin_left = margin_left.unwrap_or(0);
        } else if let Some([top, right, bottom, left]) = margin {
            config.margin_top = top;
            config.margin_right = right;
            config.margin_bottom = bottom;
            config.margin_left = left;
        }
        let mut default_options = modes
            .get("default")
            .unwrap_or_else(|| BarModeOptions::preset("default").unwrap().1);
        if fields & Self::LAYER != 0 {
            default_options.layer = config.layer;
        }
        if fields & Self::EXCLUSIVE != 0 {
            default_options.exclusive = config.exclusive;
        }
        if fields & Self::PASSTHROUGH != 0 {
            default_options.passthrough = config.passthrough;
        }
        if fields & Self::VISIBLE != 0 {
            default_options.visible = config.visible;
        }
        let resolve_mode = |requested_mode| {
            if requested_mode == "default" {
                BarModeState {
                    mode: BarMode::Default,
                    name: "default",
                    options: default_options,
                }
            } else if let Some(options) = modes.get(requested_mode) {
                BarModeState {
                    mode: BarModeOptions::preset(requested_mode)
                        .map(|(mode, _)| mode)
                        .unwrap_or(BarMode::Custom),
                    name: requested_mode,
                    options,
                }
            } else if let Some((mode, options)) = BarModeOptions::preset(requested_mode) {
                BarModeState {
                    mode,
                    name: requested_mode,
                    options,
                }
            } else {
                BarModeState {
                    mode: BarMode::Default,
                    name: "default",
                    options: default_options,
                }
            }
        };
        config.shown_mode = resolve_mode(selected_mode);
        config.hidden_mode = resolve_mode("invisible");
        config.visibility_state = !start_hidden;
        config.apply_mode_state(if start_hidden {
            config.hidden_mode
        } else {
            config.shown_mode
        });
        Ok(config)
    }

    fn mode_definitions(&mut self) -> Result<BarModeList<'a>, BarConfigError> {
        self.expect(Token::LeftBrace)?;
        let mut modes = BarModeList::empty();
        loop {
            match self.next() {
                Token::RightBrace => return Ok(modes),
                Token::String(name) => {
                    if !valid_mode_name(name) {
                        return Err(BarConfigError::InvalidMode);
                    }
                    self.expect(Token::Colon)?;
                    self.expect(Token::LeftBrace)?;
                    let mut options = BarModeOptions::preset(name)
                        .map(|(_, options)| options)
                        .unwrap_or_else(BarModeOptions::empty_custom);
                    let mut fields = 0u32;
                    loop {
                        match self.next() {
                            Token::RightBrace => break,
                            Token::String(option) => {
                                self.expect(Token::Colon)?;
                                match option {
                                    "layer" => {
                                        mark_once(&mut fields, 1 << 0)?;
                                        options.layer = match self.string_value()? {
                                            "bottom" => BarLayer::Bottom,
                                            "top" => BarLayer::Top,
                                            "overlay" => BarLayer::Overlay,
                                            _ => return Err(BarConfigError::InvalidMode),
                                        };
                                    }
                                    "exclusive" => {
                                        mark_once(&mut fields, 1 << 1)?;
                                        options.exclusive = self.mode_bool_value()?;
                                    }
                                    "passthrough" => {
                                        mark_once(&mut fields, 1 << 2)?;
                                        options.passthrough = self.mode_bool_value()?;
                                    }
                                    "visible" => {
                                        mark_once(&mut fields, 1 << 3)?;
                                        options.visible = self.mode_bool_value()?;
                                    }
                                    _ => self.skip_value()?,
                                }
                                match self.next() {
                                    Token::Comma => {}
                                    Token::RightBrace => break,
                                    token => {
                                        self.push(token);
                                        return Err(BarConfigError::UnexpectedToken);
                                    }
                                }
                            }
                            Token::End => return Err(BarConfigError::UnexpectedEnd),
                            _ => return Err(BarConfigError::UnexpectedToken),
                        }
                    }
                    modes.push(BarModeDefinition { name, options })?;
                    match self.next() {
                        Token::Comma => {}
                        Token::RightBrace => return Ok(modes),
                        _ => return Err(BarConfigError::UnexpectedToken),
                    }
                }
                Token::End => return Err(BarConfigError::UnexpectedEnd),
                _ => return Err(BarConfigError::UnexpectedToken),
            }
        }
    }

    fn mode_name_value(&mut self) -> Result<&'a str, BarConfigError> {
        let value = self.string_value()?;
        valid_mode_name(value)
            .then_some(value)
            .ok_or(BarConfigError::InvalidMode)
    }

    fn bar_name_value(&mut self) -> Result<&'a str, BarConfigError> {
        let value = self.string_value()?;
        valid_bar_name(value)
            .then_some(value)
            .ok_or(BarConfigError::InvalidName)
    }

    fn output_value(&mut self) -> Result<BarOutputList<'a>, BarConfigError> {
        match self.next() {
            Token::String(output) => {
                validate_output_name(output)?;
                Ok(BarOutputList::string(output))
            }
            Token::LeftBracket => {
                let mut outputs = BarOutputList::array();
                loop {
                    match self.next() {
                        Token::RightBracket => return Ok(outputs),
                        Token::String(output) => {
                            outputs.push(output)?;
                            match self.next() {
                                Token::Comma => {}
                                Token::RightBracket => return Ok(outputs),
                                Token::End => return Err(BarConfigError::UnexpectedEnd),
                                _ => return Err(BarConfigError::InvalidOutput),
                            }
                        }
                        Token::End => return Err(BarConfigError::UnexpectedEnd),
                        _ => return Err(BarConfigError::InvalidOutput),
                    }
                }
            }
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::InvalidOutput),
        }
    }

    fn output_dimensions_value(&mut self) -> Result<BarOutputDimensionList, BarConfigError> {
        match self.next() {
            Token::String(text) => {
                let mut dimensions = BarOutputDimensionList::string();
                dimensions.push_text(text)?;
                Ok(dimensions)
            }
            Token::LeftBracket => {
                let mut dimensions = BarOutputDimensionList::array();
                loop {
                    let token = self.next();
                    match token {
                        Token::RightBracket => return Ok(dimensions),
                        Token::String(text) => dimensions.push_text(text)?,
                        Token::End => return Err(BarConfigError::UnexpectedEnd),
                        token => {
                            self.push(token);
                            self.skip_value()?;
                        }
                    }
                    match self.next() {
                        Token::Comma => {}
                        Token::RightBracket => return Ok(dimensions),
                        Token::End => return Err(BarConfigError::UnexpectedEnd),
                        _ => return Err(BarConfigError::InvalidOutput),
                    }
                }
            }
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::InvalidOutput),
        }
    }

    fn mode_bool_value(&mut self) -> Result<bool, BarConfigError> {
        match self.next() {
            Token::Bool(value) => Ok(value),
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::InvalidMode),
        }
    }

    fn module_list(&mut self) -> Result<BarModuleList<'a>, BarConfigError> {
        self.expect(Token::LeftBracket)?;
        let mut modules = BarModuleList::empty();
        loop {
            match self.next() {
                Token::RightBracket => return Ok(modules),
                Token::String(module) => {
                    modules.push(module)?;
                    match self.next() {
                        Token::Comma => {}
                        Token::RightBracket => return Ok(modules),
                        _ => return Err(BarConfigError::UnexpectedToken),
                    }
                }
                Token::End => return Err(BarConfigError::UnexpectedEnd),
                _ => return Err(BarConfigError::InvalidModule),
            }
        }
    }

    fn module_config_value(
        &mut self,
        name: &'a str,
    ) -> Result<Option<BarModuleConfig<'a>>, BarConfigError> {
        let token = self.next();
        if token != Token::LeftBrace {
            self.push(token);
            self.skip_value()?;
            return Ok(None);
        }
        let mut module = BarModuleConfig::empty(name);
        let mut fields = 0u32;
        let mut supported = false;
        loop {
            match self.next() {
                Token::RightBrace => break,
                Token::String(option) => {
                    self.expect(Token::Colon)?;
                    match option {
                        "format" => {
                            mark_once(&mut fields, 1 << 0)?;
                            module.format = Some(self.module_format_value()?);
                            supported = true;
                        }
                        "format-alt" => {
                            mark_once(&mut fields, 1 << 1)?;
                            module.format_alt = Some(self.module_format_value()?);
                            supported = true;
                        }
                        "format-alt-click" => {
                            mark_once(&mut fields, 1 << 12)?;
                            module.format_alt_click = self.module_button_value()?;
                            supported = true;
                        }
                        "format-disconnected" => {
                            mark_once(&mut fields, 1 << 2)?;
                            module.format_disconnected = Some(self.module_format_value()?);
                            supported = true;
                        }
                        "interval" => {
                            mark_once(&mut fields, 1 << 3)?;
                            let interval = self.u16_value()?;
                            if interval == 0 {
                                return Err(BarConfigError::InvalidModuleOption);
                            }
                            module.interval = Some(interval);
                            supported = true;
                        }
                        "tooltip" => {
                            mark_once(&mut fields, 1 << 4)?;
                            module.tooltip = Some(self.bool_value()?);
                            supported = true;
                        }
                        "min-length" => {
                            mark_once(&mut fields, 1 << 5)?;
                            module.min_length = Some(self.u16_value()?);
                            supported = true;
                        }
                        "max-length" => {
                            mark_once(&mut fields, 1 << 6)?;
                            module.max_length = Some(self.u16_value()?);
                            supported = true;
                        }
                        "on-click" => {
                            mark_once(&mut fields, 1 << 7)?;
                            module.on_click = Some(self.module_action_value()?);
                            supported = true;
                        }
                        "on-click-right" => {
                            mark_once(&mut fields, 1 << 8)?;
                            module.on_click_right = Some(self.module_action_value()?);
                            supported = true;
                        }
                        "on-click-middle" => {
                            mark_once(&mut fields, 1 << 9)?;
                            module.on_click_middle = Some(self.module_action_value()?);
                            supported = true;
                        }
                        "on-scroll-up" => {
                            mark_once(&mut fields, 1 << 10)?;
                            module.on_scroll_up = Some(self.module_action_value()?);
                            supported = true;
                        }
                        "on-scroll-down" => {
                            mark_once(&mut fields, 1 << 11)?;
                            module.on_scroll_down = Some(self.module_action_value()?);
                            supported = true;
                        }
                        _ => self.skip_value()?,
                    }
                    match self.next() {
                        Token::Comma => {}
                        Token::RightBrace => break,
                        token => {
                            self.push(token);
                            return Err(BarConfigError::UnexpectedToken);
                        }
                    }
                }
                Token::End => return Err(BarConfigError::UnexpectedEnd),
                _ => return Err(BarConfigError::UnexpectedToken),
            }
        }
        if module
            .min_length
            .zip(module.max_length)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            return Err(BarConfigError::InvalidModuleOption);
        }
        Ok(supported.then_some(module))
    }

    fn module_format_value(&mut self) -> Result<&'a str, BarConfigError> {
        let value = self.string_value()?;
        if value.len() > MAX_BAR_TEXT {
            return Err(BarConfigError::InvalidModuleOption);
        }
        Ok(value)
    }

    fn module_action_value(&mut self) -> Result<&'a str, BarConfigError> {
        let value = self.string_value()?;
        if value.is_empty()
            || value.len() > MAX_BAR_TEXT
            || !value
                .bytes()
                .all(|byte| byte == b' ' || byte.is_ascii_graphic())
        {
            return Err(BarConfigError::InvalidModuleOption);
        }
        Ok(value)
    }

    fn module_button_value(&mut self) -> Result<BarButton, BarConfigError> {
        match self.next() {
            Token::String("click" | "click-left") | Token::Number("1") => Ok(BarButton::Left),
            Token::String("click-middle") | Token::Number("2") => Ok(BarButton::Middle),
            Token::String("click-right") | Token::Number("3") => Ok(BarButton::Right),
            Token::String("click-backward") | Token::Number("8") => Ok(BarButton::Backward),
            Token::String("click-forward") | Token::Number("9") => Ok(BarButton::Forward),
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::InvalidModuleOption),
        }
    }

    fn skip_value(&mut self) -> Result<(), BarConfigError> {
        match self.next() {
            Token::LeftBrace => self.skip_container(Token::RightBrace),
            Token::LeftBracket => self.skip_container(Token::RightBracket),
            Token::String(_) | Token::Number(_) | Token::Bool(_) | Token::Null => Ok(()),
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::UnexpectedToken),
        }
    }

    fn skip_container(&mut self, closing: Token<'a>) -> Result<(), BarConfigError> {
        let mut braces = 0usize;
        let mut brackets = 0usize;
        loop {
            let token = self.next();
            if token == closing && braces == 0 && brackets == 0 {
                return Ok(());
            }
            match token {
                Token::LeftBrace => braces += 1,
                Token::RightBrace => {
                    braces = braces
                        .checked_sub(1)
                        .ok_or(BarConfigError::UnexpectedToken)?;
                }
                Token::LeftBracket => brackets += 1,
                Token::RightBracket => {
                    brackets = brackets
                        .checked_sub(1)
                        .ok_or(BarConfigError::UnexpectedToken)?;
                }
                Token::End => return Err(BarConfigError::UnexpectedEnd),
                Token::Invalid => return Err(BarConfigError::UnexpectedToken),
                _ => {}
            }
        }
    }

    fn string_value(&mut self) -> Result<&'a str, BarConfigError> {
        match self.next() {
            Token::String(value) => Ok(value),
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::UnexpectedToken),
        }
    }

    fn bool_value(&mut self) -> Result<bool, BarConfigError> {
        match self.next() {
            Token::Bool(value) => Ok(value),
            Token::End => Err(BarConfigError::UnexpectedEnd),
            _ => Err(BarConfigError::InvalidModuleOption),
        }
    }

    fn u16_value(&mut self) -> Result<u16, BarConfigError> {
        let Token::Number(value) = self.next() else {
            return Err(BarConfigError::InvalidNumber);
        };
        let mut parsed = 0u16;
        if value.is_empty() {
            return Err(BarConfigError::InvalidNumber);
        }
        for byte in value.bytes() {
            if !byte.is_ascii_digit() {
                return Err(BarConfigError::InvalidNumber);
            }
            parsed = parsed
                .checked_mul(10)
                .and_then(|current| current.checked_add(u16::from(byte - b'0')))
                .ok_or(BarConfigError::InvalidNumber)?;
        }
        Ok(parsed)
    }

    fn i32_value(&mut self) -> Result<i32, BarConfigError> {
        let Token::Number(value) = self.next() else {
            return Err(BarConfigError::InvalidNumber);
        };
        parse_i32(value).ok_or(BarConfigError::InvalidNumber)
    }

    fn margin_value(&mut self) -> Result<[i32; 4], BarConfigError> {
        match self.next() {
            Token::Number(value) => {
                let value = parse_i32(value).ok_or(BarConfigError::InvalidNumber)?;
                Ok([value; 4])
            }
            Token::String(value) => {
                let mut values = [0; 4];
                let mut count = 0usize;
                for component in value.split_ascii_whitespace() {
                    if count == values.len() {
                        return Err(BarConfigError::InvalidNumber);
                    }
                    values[count] = parse_i32(component).ok_or(BarConfigError::InvalidNumber)?;
                    count += 1;
                }
                match count {
                    1 => Ok([values[0]; 4]),
                    2 => Ok([values[0], values[1], values[0], values[1]]),
                    3 => Ok([values[0], values[1], values[2], values[1]]),
                    4 => Ok(values),
                    _ => Err(BarConfigError::InvalidNumber),
                }
            }
            _ => Err(BarConfigError::InvalidNumber),
        }
    }

    fn expect(&mut self, expected: Token<'a>) -> Result<(), BarConfigError> {
        let actual = self.next();
        if actual == expected {
            Ok(())
        } else if actual == Token::End {
            Err(BarConfigError::UnexpectedEnd)
        } else {
            Err(BarConfigError::UnexpectedToken)
        }
    }

    fn next(&mut self) -> Token<'a> {
        self.pushed.take().unwrap_or_else(|| self.lexer.next())
    }

    fn push(&mut self, token: Token<'a>) {
        self.pushed = Some(token);
    }
}

fn mark_once(fields: &mut u32, field: u32) -> Result<(), BarConfigError> {
    if *fields & field != 0 {
        return Err(BarConfigError::DuplicateField);
    }
    *fields |= field;
    Ok(())
}

fn valid_mode_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_BAR_MODE_NAME
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_bar_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_BAR_NAME
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_output_name(value: &str) -> Result<(), BarConfigError> {
    if value.len() <= MAX_BAR_OUTPUT_NAME {
        Ok(())
    } else {
        Err(BarConfigError::InvalidOutput)
    }
}

fn parse_usize(value: &str) -> Option<usize> {
    if value.is_empty() {
        return None;
    }
    let mut parsed = 0usize;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        parsed = parsed
            .checked_mul(10)?
            .checked_add(usize::from(byte - b'0'))?;
    }
    Some(parsed)
}

fn parse_i32(value: &str) -> Option<i32> {
    let bytes = value.as_bytes();
    let (&first, rest) = bytes.split_first()?;
    let (negative, digits) = match first {
        b'-' => (true, rest),
        b'+' => (false, rest),
        _ => (false, bytes),
    };
    if digits.is_empty() {
        return None;
    }
    let mut magnitude = 0u32;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        magnitude = magnitude
            .checked_mul(10)?
            .checked_add(u32::from(*byte - b'0'))?;
    }
    if negative {
        if magnitude == i32::MAX as u32 + 1 {
            Some(i32::MIN)
        } else {
            i32::try_from(magnitude).ok()?.checked_neg()
        }
    } else {
        i32::try_from(magnitude).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_waybar_jsonc_modules_and_common_module_options() {
        let config = parse_waybar_config(
            r#"
            {
                // Waybar-compatible top-level fields.
                "name": "slop-test",
                "position": "top",
                "height": 40,
                "width": 800,
                "spacing": 8,
                "margin": "1 2 3 4",
                "fixed-center": false,
                "expand-left": true,
                "expand-center": true,
                "expand-right": true,
                "no-center": true,
                "layer": "top",
                "exclusive": false,
                "passthrough": true,
                "modules-left": ["niri/workspaces", "custom/launcher"],
                "modules-center": ["niri/window"],
                "modules-right": ["network", "cpu", "memory", "clock",],
                "clock": {
                    "format": "{:%H:%M}",
                    "format-alt": "UTC",
                    "format-alt-click": "click-right",
                    "interval": 60,
                    "tooltip": false,
                    "min-length": 3,
                    "max-length": 12,
                    "on-click": "status",
                    "on-click-right": "about",
                    "on-click-middle": "swww query",
                    "on-scroll-up": "clear",
                    "on-scroll-down": "reload",
                    "calendar": { "mode": "month" }
                },
            }
            "#,
        )
        .unwrap();
        assert_eq!(config.name, Some("slop-test"));
        assert_eq!(config.namespace(), "slop-test");
        assert_eq!(config.position, BarPosition::Top);
        assert_eq!(config.height, 40);
        assert_eq!(config.width, 800);
        assert_eq!(config.spacing, 8);
        assert_eq!(
            (
                config.margin_top,
                config.margin_right,
                config.margin_bottom,
                config.margin_left
            ),
            (1, 2, 3, 4)
        );
        assert!(!config.fixed_center);
        assert!(config.expand_left);
        assert!(config.expand_center);
        assert!(config.expand_right);
        assert!(config.no_center);
        assert_eq!(config.horizontal_geometry(1024), (112, 800));
        assert_eq!(config.layer, BarLayer::Top);
        assert_eq!(config.mode, BarMode::Default);
        assert!(!config.exclusive);
        assert!(config.passthrough);
        assert!(config.visible);
        assert_eq!(config.reserved_top(), 0);
        let mut left = config.modules_left.iter();
        assert_eq!(left.next(), Some("niri/workspaces"));
        assert_eq!(left.next(), Some("custom/launcher"));
        assert_eq!(left.next(), None);
        let mut center = config.modules_center.iter();
        assert_eq!(center.next(), Some("niri/window"));
        assert_eq!(center.next(), None);
        let mut right = config.modules_right.iter();
        assert_eq!(right.next(), Some("network"));
        assert_eq!(right.next(), Some("cpu"));
        assert_eq!(right.next(), Some("memory"));
        assert_eq!(right.next(), Some("clock"));
        assert_eq!(right.next(), None);
        let clock = config.module_configs.get("clock").unwrap();
        assert_eq!(clock.format, Some("{:%H:%M}"));
        assert_eq!(clock.format_alt, Some("UTC"));
        assert_eq!(clock.format_alt_click, BarButton::Right);
        assert_eq!(clock.interval, Some(60));
        assert_eq!(clock.tooltip, Some(false));
        assert_eq!(clock.min_length, Some(3));
        assert_eq!(clock.max_length, Some(12));
        assert_eq!(clock.on_click, Some("status"));
        assert_eq!(clock.on_click_right, Some("about"));
        assert_eq!(clock.on_click_middle, Some("swww query"));
        assert_eq!(clock.on_scroll_up, Some("clear"));
        assert_eq!(clock.on_scroll_down, Some("reload"));
    }

    #[test]
    fn supports_block_comments_and_defaults() {
        let config = parse_waybar_config(
            r#"/* comment */ {
                "modules-left": [],
                "clock": { "format-alt": "UTC" }
            }"#,
        )
        .unwrap();
        assert_eq!(config.name, None);
        assert_eq!(config.namespace(), "waybar");
        assert!(!config.output.is_configured());
        assert_eq!(config.output.form_name(), "any");
        assert!(config.output.matches("SLOPOS-1", "SlopOS Display"));
        assert!(config.output_selected());
        assert_eq!(config.position, BarPosition::Top);
        assert_eq!(config.height, 30);
        assert_eq!(config.width, 0);
        assert_eq!(config.spacing, 4);
        assert_eq!(
            (
                config.margin_top,
                config.margin_right,
                config.margin_bottom,
                config.margin_left
            ),
            (0, 0, 0, 0)
        );
        assert!(config.fixed_center);
        assert!(!config.expand_left);
        assert!(!config.expand_center);
        assert!(!config.expand_right);
        assert!(!config.no_center);
        assert_eq!(config.horizontal_geometry(1024), (0, 1024));
        assert_eq!(config.layer, BarLayer::Bottom);
        assert_eq!(config.mode, BarMode::Default);
        assert!(config.exclusive);
        assert!(!config.passthrough);
        assert!(config.visible);
        assert!(config.visibility_state());
        assert_eq!(
            config.signal_action(BarSignal::User1),
            BarSignalAction::Toggle
        );
        assert_eq!(
            config.signal_action(BarSignal::User2),
            BarSignalAction::Reload
        );
        assert_eq!(config.reserved_top(), 30);
        assert!(config.modules_left.is_empty());
        assert_eq!(
            config.module_configs.get("clock").unwrap().format_alt_click,
            BarButton::Left
        );
    }

    #[test]
    fn selects_waybar_output_strings_and_ordered_arrays() {
        let mut array = parse_waybar_config(
            r#"{
                "output": ["!HDMI-A-1", "SLOPOS-1", "*"],
                "layer": "top"
            }"#,
        )
        .unwrap();
        assert!(array.output.is_configured());
        assert!(array.output.is_array());
        assert_eq!(array.output.form_name(), "array");
        assert_eq!(array.output.len(), 3);
        let mut outputs = array.output.iter();
        assert_eq!(outputs.next(), Some("!HDMI-A-1"));
        assert_eq!(outputs.next(), Some("SLOPOS-1"));
        assert_eq!(outputs.next(), Some("*"));
        assert_eq!(outputs.next(), None);
        assert!(!array.output.matches("HDMI-A-1", "Other Display"));
        assert!(array.output.matches("SLOPOS-1", "Other Display"));
        assert!(array.output.matches("DP-2", "Other Display"));
        assert!(!array.output_selected());
        assert!(!array.visible);
        assert!(array.select_output("SLOPOS-1", "SlopOS Virtual Display 0x00000001", 1024, 768));
        assert!(array.output_selected());
        assert!(array.visible);
        assert_eq!(array.reserved_top(), 30);

        let identifier =
            parse_waybar_config(r#"{ "output": "SlopOS Virtual Display 0x00000001" }"#).unwrap();
        assert!(
            identifier
                .output
                .matches("OTHER-1", "SlopOS Virtual Display 0x00000001")
        );
        assert!(!identifier.output.matches("OTHER-1", "Other Display"));

        let excluded = parse_waybar_config(r#"{ "output": "!SLOPOS-1" }"#).unwrap();
        assert!(!excluded.output.matches("SLOPOS-1", "Other Display"));
        assert!(excluded.output.matches("DP-2", "Other Display"));

        let wildcard_string = parse_waybar_config(r#"{ "output": "*" }"#).unwrap();
        assert!(!wildcard_string.output.matches("SLOPOS-1", "Other Display"));
        let empty_string = parse_waybar_config(r#"{ "output": "" }"#).unwrap();
        assert!(empty_string.output.matches("SLOPOS-1", "Other Display"));
        let empty_array = parse_waybar_config(r#"{ "output": [] }"#).unwrap();
        assert!(!empty_array.output.matches("SLOPOS-1", "Other Display"));

        let ordered_positive =
            parse_waybar_config(r#"{ "output": ["SLOPOS-1", "!SLOPOS-1"] }"#).unwrap();
        assert!(ordered_positive.output.matches("SLOPOS-1", "Other Display"));
        let ordered_exclusion = parse_waybar_config(r#"{ "output": ["!SLOPOS-1", "*"] }"#).unwrap();
        assert!(
            !ordered_exclusion
                .output
                .matches("SLOPOS-1", "Other Display")
        );
        assert!(ordered_exclusion.output.matches("DP-2", "Other Display"));

        let mut gated = parse_waybar_config(r#"{ "output": "SLOPOS-1" }"#).unwrap();
        assert!(!gated.select_output("DP-2", "Other Display", 1024, 768));
        gated.toggle_visibility();
        assert!(gated.set_visibility(true));
        assert!(!gated.visible);
        assert!(gated.select_output("SLOPOS-1", "Other Display", 1024, 768));
        assert!(gated.visible);
    }

    #[test]
    fn expands_waybar_output_environment_references() {
        let mut string = parse_waybar_config(r#"{ "output": "$SLOPOS_WAYBAR_OUTPUT" }"#).unwrap();
        assert!(!string.output.matches("SLOPOS-1", "Other Display"));
        assert!(
            string
                .output
                .matches_with_environment("SLOPOS-1", "Other Display", |variable| match variable {
                    "SLOPOS_WAYBAR_OUTPUT" => Some("SLOPOS-1"),
                    _ => None,
                })
        );
        assert!(string.select_output_with_environment(
            "SLOPOS-1",
            "Other Display",
            1024,
            768,
            |variable| match variable {
                "SLOPOS_WAYBAR_OUTPUT" => Some("SLOPOS-1"),
                _ => None,
            }
        ));
        assert!(string.visible);

        let excluded = parse_waybar_config(
            r#"{ "output": ["!$EXCLUDED_OUTPUT", "$SLOPOS_WAYBAR_OUTPUT", "*"] }"#,
        )
        .unwrap();
        assert!(!excluded.output.matches_with_environment(
            "SLOPOS-1",
            "Other Display",
            |variable| match variable {
                "EXCLUDED_OUTPUT" | "SLOPOS_WAYBAR_OUTPUT" => Some("SLOPOS-1"),
                _ => None,
            }
        ));
        assert!(excluded.output.matches_with_environment(
            "SLOPOS-1",
            "Other Display",
            |variable| match variable {
                "SLOPOS_WAYBAR_OUTPUT" => Some("SLOPOS-1"),
                _ => None,
            }
        ));

        let wrong_value = parse_waybar_config(r#"{ "output": "$OTHER_OUTPUT" }"#).unwrap();
        assert!(
            !wrong_value
                .output
                .matches_with_environment("SLOPOS-1", "Other Display", |_| Some("DP-2"))
        );
        assert!(wrong_value.output.matches("$OTHER_OUTPUT", "Other Display"));
    }

    #[test]
    fn selects_waybar_output_dimensions_when_output_is_absent() {
        let mut dimensions = parse_waybar_config(
            r#"{
                "output-dimensions": [
                    "width > 800",
                    "height > 700",
                    "malformed",
                    17
                ]
            }"#,
        )
        .unwrap();
        assert!(dimensions.output_dimensions.is_configured());
        assert!(dimensions.output_dimensions.is_array());
        assert_eq!(dimensions.output_dimensions.form_name(), "array");
        assert_eq!(dimensions.output_dimensions.len(), 2);
        let mut conditions = dimensions.output_dimensions.iter();
        assert_eq!(
            conditions.next(),
            Some(BarOutputDimension::WidthGreater(800))
        );
        assert_eq!(
            conditions.next(),
            Some(BarOutputDimension::HeightGreater(700))
        );
        assert_eq!(conditions.next(), None);
        assert!(dimensions.output_dimensions.matches(1024, 768));
        assert!(!dimensions.output_dimensions.matches(800, 768));
        assert!(!dimensions.output_dimensions.matches(1024, 700));
        assert!(!dimensions.output_selected());
        assert!(dimensions.select_output("SLOPOS-1", "Other Display", 1024, 768));
        assert!(dimensions.visible);
        assert!(!dimensions.select_output("SLOPOS-1", "Other Display", 800, 768));
        assert!(!dimensions.visible);
        dimensions.toggle_visibility();
        assert!(!dimensions.visible);
        assert!(dimensions.set_visibility(true));
        assert!(!dimensions.visible);

        let mut string = parse_waybar_config(r#"{ "output-dimensions": "width < 1200" }"#).unwrap();
        assert!(string.output_dimensions.is_configured());
        assert!(!string.output_dimensions.is_array());
        assert_eq!(string.output_dimensions.form_name(), "string");
        assert_eq!(string.output_dimensions.len(), 1);
        assert!(string.select_output("SLOPOS-1", "Other Display", 1024, 768));
        assert!(!string.select_output("SLOPOS-1", "Other Display", 1200, 768));

        let mut any = parse_waybar_config("{}").unwrap();
        assert!(any.select_output("SLOPOS-1", "Other Display", 1024, 768));
        for input in [
            r#"{ "output-dimensions": [] }"#,
            r#"{ "output-dimensions": "" }"#,
            r#"{ "output-dimensions": ["unknown = 3", null, false] }"#,
        ] {
            let mut config = parse_waybar_config(input).unwrap();
            assert!(config.select_output("SLOPOS-1", "Other Display", 1024, 768));
        }

        let mut output_wins = parse_waybar_config(
            r#"{
                "output": "SLOPOS-1",
                "output-dimensions": "width > 2000"
            }"#,
        )
        .unwrap();
        assert!(output_wins.select_output("SLOPOS-1", "Other Display", 1024, 768));
        let mut empty_string_falls_through = parse_waybar_config(
            r#"{
                "output": "",
                "output-dimensions": "width > 2000"
            }"#,
        )
        .unwrap();
        assert!(!empty_string_falls_through.select_output("SLOPOS-1", "Other Display", 1024, 768));
        let mut empty_array_wins = parse_waybar_config(
            r#"{
                "output": [],
                "output-dimensions": "width < 2000"
            }"#,
        )
        .unwrap();
        assert!(!empty_array_wins.select_output("SLOPOS-1", "Other Display", 1024, 768));

        assert_eq!(
            parse_waybar_config(
                r#"{
                    "output-dimensions": [
                        "width > 0", "width > 1", "width > 2",
                        "width > 3", "width > 4", "width > 5",
                        "width > 6", "width > 7", "width > 8"
                    ]
                }"#,
            ),
            Err(BarConfigError::TooManyOutputDimensions)
        );
    }

    #[test]
    fn rejects_duplicate_invalid_and_overfull_fields() {
        assert_eq!(
            parse_waybar_config(r#"{ "height": 30, "height": 31 }"#),
            Err(BarConfigError::DuplicateField)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "output": "a", "output": "b" }"#),
            Err(BarConfigError::DuplicateField)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "position": "middle" }"#),
            Err(BarConfigError::InvalidPosition)
        );
        for input in [
            r#"{ "name": "" }"#,
            r#"{ "name": "bad name" }"#,
            r#"{ "name": "waybar.main" }"#,
            r#"{ "name": 1 }"#,
            r#"{ "output": 1 }"#,
            r#"{ "output": ["SLOPOS-1", 1] }"#,
            r#"{ "output-dimensions": 1 }"#,
            r#"{ "output": [
                "a", "b", "c", "d", "e", "f", "g", "h", "i"
            ] }"#,
            r#"{ "margin": "" }"#,
            r#"{ "margin": "1 2 3 4 5" }"#,
            r#"{ "margin": "1px" }"#,
            r#"{ "margin": 1.5 }"#,
            r#"{ "margin-left": 2147483648 }"#,
            r#"{ "width": -1 }"#,
            r#"{ "width": 65536 }"#,
            r#"{ "fixed-center": "false" }"#,
            r#"{ "expand-left": 1 }"#,
            r#"{ "expand-center": "true" }"#,
            r#"{ "expand-right": null }"#,
            r#"{ "no-center": 1 }"#,
            r#"{ "exclusive": 1 }"#,
            r#"{ "layer": "background" }"#,
            r#"{ "mode": "bad mode" }"#,
            r#"{ "passthrough": 1 }"#,
            r#"{ "start_hidden": "true" }"#,
            r#"{ "visible": 1 }"#,
            r#"{ "on-sigusr1": 1 }"#,
            r#"{ "on-sigusr2": false }"#,
            r#"{ "modes": [] }"#,
            r#"{ "modes": { "reading": { "visible": 1 } } }"#,
            r#"{ "modes": { "reading": {}, "reading": {} } }"#,
            r#"{ "modes": {
                "a": {}, "b": {}, "c": {}, "d": {}, "e": {},
                "f": {}, "g": {}, "h": {}, "i": {}
            } }"#,
        ] {
            assert!(parse_waybar_config(input).is_err());
        }
        let overlong_output = [b'a'; MAX_BAR_OUTPUT_NAME + 1];
        assert_eq!(
            validate_output_name(core::str::from_utf8(&overlong_output).unwrap()),
            Err(BarConfigError::InvalidOutput)
        );
        let individual_margin = parse_waybar_config(
            r#"{
                "margin": "10 20 30 40",
                "margin-top": -2,
                "margin-right": 5
            }"#,
        )
        .unwrap();
        assert_eq!(
            (
                individual_margin.margin_top,
                individual_margin.margin_right,
                individual_margin.margin_bottom,
                individual_margin.margin_left
            ),
            (-2, 5, 0, 0)
        );
        assert_eq!(individual_margin.reserved_top(), 28);
        assert_eq!(individual_margin.horizontal_geometry(1000), (0, 995));
        let fixed_width = parse_waybar_config(r#"{ "width": 800, "margin": "4 12" }"#).unwrap();
        assert_eq!(fixed_width.horizontal_geometry(1024), (112, 800));
        let clamped_width = parse_waybar_config(r#"{ "width": 1200, "margin": "4 12" }"#).unwrap();
        assert_eq!(clamped_width.horizontal_geometry(1024), (12, 1000));
        for (input, expected) in [
            (r#"{ "margin": 7 }"#, (7, 7, 7, 7)),
            (r#"{ "margin": "1 2" }"#, (1, 2, 1, 2)),
            (r#"{ "margin": "1 2 3" }"#, (1, 2, 3, 2)),
        ] {
            let config = parse_waybar_config(input).unwrap();
            assert_eq!(
                (
                    config.margin_top,
                    config.margin_right,
                    config.margin_bottom,
                    config.margin_left
                ),
                expected
            );
        }
        for (input, mode, layer, exclusive, passthrough, visible) in [
            (
                r#"{"layer":"top","exclusive":true,"passthrough":false,"mode":"dock"}"#,
                "dock",
                BarLayer::Bottom,
                true,
                false,
                true,
            ),
            (
                r#"{"layer":"top","exclusive":true,"passthrough":false,"mode":"hide"}"#,
                "hide",
                BarLayer::Overlay,
                false,
                false,
                true,
            ),
            (
                r#"{"layer":"top","exclusive":true,"passthrough":false,"mode":"invisible"}"#,
                "invisible",
                BarLayer::Bottom,
                false,
                true,
                false,
            ),
            (
                r#"{"layer":"top","exclusive":true,"passthrough":false,"mode":"overlay"}"#,
                "overlay",
                BarLayer::Overlay,
                false,
                true,
                true,
            ),
        ] {
            let config = parse_waybar_config(input).unwrap();
            assert_eq!(config.mode.name(), mode);
            assert_eq!(config.mode_name, mode);
            assert_eq!(config.layer, layer);
            assert_eq!(config.exclusive, exclusive);
            assert_eq!(config.passthrough, passthrough);
            assert_eq!(config.visible, visible);
            assert_eq!(config.reserved_top(), if exclusive { 30 } else { 0 });
        }
        let hidden = parse_waybar_config(
            r#"{
                "layer": "top",
                "exclusive": true,
                "passthrough": false,
                "mode": "dock",
                "start_hidden": true
            }"#,
        )
        .unwrap();
        assert_eq!(hidden.mode, BarMode::Invisible);
        assert_eq!(hidden.mode_name, "invisible");
        assert_eq!(hidden.layer, BarLayer::Bottom);
        assert!(!hidden.exclusive);
        assert!(hidden.passthrough);
        assert!(!hidden.visible);
        assert_eq!(hidden.reserved_top(), 0);

        let custom = parse_waybar_config(
            r#"{
                "mode": "reading",
                "modes": {
                    "reading": {
                        "layer": "overlay",
                        "exclusive": false,
                        "passthrough": false,
                        "visible": true,
                        "animation": "ignored"
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(custom.mode, BarMode::Custom);
        assert_eq!(custom.mode_name, "reading");
        assert_eq!(custom.layer, BarLayer::Overlay);
        assert!(!custom.exclusive);
        assert!(!custom.passthrough);
        assert!(custom.visible);
        assert_eq!(custom.reserved_top(), 0);

        let overridden_dock = parse_waybar_config(
            r#"{
                "mode": "dock",
                "modes": {
                    "dock": { "passthrough": true, "visible": false }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(overridden_dock.mode, BarMode::Dock);
        assert_eq!(overridden_dock.mode_name, "dock");
        assert_eq!(overridden_dock.layer, BarLayer::Bottom);
        assert!(overridden_dock.exclusive);
        assert!(overridden_dock.passthrough);
        assert!(!overridden_dock.visible);

        let custom_default = parse_waybar_config(
            r#"{
                "modes": {
                    "default": {
                        "layer": "overlay",
                        "exclusive": false,
                        "visible": false
                    }
                },
                "layer": "top"
            }"#,
        )
        .unwrap();
        assert_eq!(custom_default.mode, BarMode::Default);
        assert_eq!(custom_default.mode_name, "default");
        assert_eq!(custom_default.layer, BarLayer::Top);
        assert!(!custom_default.exclusive);
        assert!(!custom_default.visible);

        let unknown = parse_waybar_config(
            r#"{
                "mode": "not-configured",
                "layer": "top",
                "exclusive": false
            }"#,
        )
        .unwrap();
        assert_eq!(unknown.mode, BarMode::Default);
        assert_eq!(unknown.mode_name, "default");
        assert_eq!(unknown.layer, BarLayer::Top);
        assert!(!unknown.exclusive);

        let mut configured_hidden = parse_waybar_config(
            r#"{
                "mode": "reading",
                "start_hidden": true,
                "on-sigusr1": "show",
                "on-sigusr2": "noop",
                "modes": {
                    "reading": {
                        "layer": "top",
                        "exclusive": true,
                        "passthrough": false,
                        "visible": true
                    },
                    "invisible": {
                        "layer": "overlay",
                        "passthrough": false,
                        "visible": true
                    }
                }
            }"#,
        )
        .unwrap();
        assert_eq!(configured_hidden.mode, BarMode::Invisible);
        assert_eq!(configured_hidden.mode_name, "invisible");
        assert_eq!(configured_hidden.layer, BarLayer::Overlay);
        assert!(!configured_hidden.exclusive);
        assert!(!configured_hidden.passthrough);
        assert!(configured_hidden.visible);
        assert!(!configured_hidden.visibility_state());
        assert_eq!(
            configured_hidden.signal_action(BarSignal::User1),
            BarSignalAction::Show
        );
        assert_eq!(
            configured_hidden.signal_action(BarSignal::User2),
            BarSignalAction::Noop
        );
        assert!(configured_hidden.set_visibility(true));
        assert_eq!(configured_hidden.mode, BarMode::Custom);
        assert_eq!(configured_hidden.mode_name, "reading");
        assert_eq!(configured_hidden.layer, BarLayer::Top);
        assert!(configured_hidden.exclusive);
        assert!(!configured_hidden.passthrough);
        assert!(configured_hidden.visible);
        assert!(configured_hidden.visibility_state());
        assert!(!configured_hidden.set_visibility(true));
        configured_hidden.toggle_visibility();
        assert_eq!(configured_hidden.mode, BarMode::Invisible);
        assert!(!configured_hidden.visibility_state());

        let invalid_actions = parse_waybar_config(
            r#"{
                "on-sigusr1": "invalid",
                "on-sigusr2": "also-invalid"
            }"#,
        )
        .unwrap();
        assert_eq!(invalid_actions.on_sigusr1, BarSignalAction::Toggle);
        assert_eq!(invalid_actions.on_sigusr2, BarSignalAction::Reload);
        assert_eq!(
            parse_waybar_config(
                r#"{
                    "modules-left": [
                        "0","1","2","3","4","5","6","7","8",
                        "9","10","11","12","13","14","15","16"
                    ]
                }"#
            ),
            Err(BarConfigError::TooManyModules)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "clock": { "interval": 0 } }"#),
            Err(BarConfigError::InvalidModuleOption)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "clock": { "min-length": 8, "max-length": 4 } }"#),
            Err(BarConfigError::InvalidModuleOption)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "clock": { "on-click": "" } }"#),
            Err(BarConfigError::InvalidModuleOption)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "clock": { "on-click": "状态" } }"#),
            Err(BarConfigError::InvalidModuleOption)
        );
        assert_eq!(
            parse_waybar_config("{ \"clock\": { \"on-click\": \"status\nabout\" } }"),
            Err(BarConfigError::InvalidModuleOption)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "clock": { "format-alt-click": "double-click" } }"#),
            Err(BarConfigError::InvalidModuleOption)
        );
    }

    #[test]
    fn distributes_dynamic_center_space_like_gtk_box_packing() {
        let default = parse_waybar_config(r#"{ "fixed-center": false }"#).unwrap();
        assert_eq!(default.dynamic_center_origin(100, 900, 100), 450);

        let center =
            parse_waybar_config(r#"{ "fixed-center": false, "expand-center": true }"#).unwrap();
        assert_eq!(center.dynamic_center_origin(100, 900, 100), 100);

        let left =
            parse_waybar_config(r#"{ "fixed-center": false, "expand-left": true }"#).unwrap();
        assert_eq!(left.dynamic_center_origin(100, 900, 100), 625);

        let right =
            parse_waybar_config(r#"{ "fixed-center": false, "expand-right": true }"#).unwrap();
        assert_eq!(right.dynamic_center_origin(100, 900, 100), 275);

        let all = parse_waybar_config(
            r#"{
                "fixed-center": false,
                "expand-left": true,
                "expand-center": true,
                "expand-right": true
            }"#,
        )
        .unwrap();
        assert_eq!(all.dynamic_center_origin(100, 900, 100), 333);
        assert_eq!(all.dynamic_center_origin(100, 150, 100), 100);
    }

    #[test]
    fn formats_named_default_and_right_aligned_replacements() {
        let text = format_bar_text(
            "CPU {usage:>2}% {}",
            "OK",
            &[BarFormatValue {
                name: "usage",
                value: "7",
            }],
        )
        .unwrap();
        assert_eq!(text.as_str(), "CPU  7% OK");
        assert_eq!(
            format_bar_text("{missing}", "", &[]),
            Err(BarFormatError::InvalidPlaceholder)
        );
    }
}
