// SPDX-License-Identifier: 0BSD

use crate::{
    ColumnDisplay, ColumnWidth, ColumnWidthChange, FocusRing, LayoutConfig, LayoutError, Rect,
    ScrollLayout, TabbedColumnInfo,
};

pub const MAX_NIRI_WORKSPACES: usize = 8;
pub const MAX_NIRI_BINDINGS: usize = 80;
pub const MAX_NIRI_WINDOW_RULES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingModifiers(u8);

impl BindingModifiers {
    pub const NONE: Self = Self(0);
    pub const MOD: Self = Self(1 << 0);
    pub const CTRL: Self = Self(1 << 1);
    pub const SHIFT: Self = Self(1 << 2);
    pub const ALT: Self = Self(1 << 3);

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0x0f)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    const fn with(self, modifier: Self) -> Self {
        Self(self.0 | modifier.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKey {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    WheelScrollUp,
    WheelScrollDown,
    Home,
    End,
    Return,
    Tab,
    Escape,
    Minus,
    Equal,
    Character(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceReference<'a> {
    Index(u8),
    Name(&'a str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NiriAction<'a> {
    FocusColumnLeft,
    FocusColumnRight,
    FocusColumnFirst,
    FocusColumnLast,
    FocusWindowUp,
    FocusWindowDown,
    MoveColumnLeft,
    MoveColumnRight,
    MoveColumnToFirst,
    MoveColumnToLast,
    MoveWindowUp,
    MoveWindowDown,
    FocusWorkspaceUp,
    FocusWorkspaceDown,
    FocusWorkspacePrevious,
    FocusWorkspace(WorkspaceReference<'a>),
    MoveWorkspaceUp,
    MoveWorkspaceDown,
    MoveColumnToWorkspaceUp,
    MoveColumnToWorkspaceDown,
    MoveColumnToWorkspace(WorkspaceReference<'a>),
    MoveWindowToWorkspaceUp,
    MoveWindowToWorkspaceDown,
    MoveWindowToWorkspace(WorkspaceReference<'a>),
    ConsumeWindowIntoColumn,
    ExpelWindowFromColumn,
    ConsumeOrExpelWindowLeft,
    ConsumeOrExpelWindowRight,
    ToggleColumnTabbedDisplay,
    ToggleWindowFloating,
    SwitchFocusBetweenFloatingAndTiling,
    MoveWindowToFloating,
    MoveWindowToTiling,
    FocusFloating,
    FocusTiling,
    SwitchPresetColumnWidth,
    SwitchPresetColumnWidthBack,
    SwitchPresetWindowHeight,
    SwitchPresetWindowHeightBack,
    MaximizeColumn,
    FullscreenWindow,
    MaximizeWindowToEdges,
    CenterColumn,
    CenterVisibleColumns,
    ExpandColumnToAvailableWidth,
    SetColumnWidth(ColumnWidthChange),
    SetWindowHeight(ColumnWidthChange),
    ResetWindowHeight,
    CloseWindow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriBinding<'a> {
    pub modifiers: BindingModifiers,
    pub key: BindingKey,
    pub cooldown_ms: Option<u16>,
    pub action: NiriAction<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriBindingList<'a> {
    entries: [Option<NiriBinding<'a>>; MAX_NIRI_BINDINGS],
    length: usize,
}

impl<'a> NiriBindingList<'a> {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_NIRI_BINDINGS],
            length: 0,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn action(self, modifiers: BindingModifiers, key: BindingKey) -> Option<NiriAction<'a>> {
        self.binding(modifiers, key)
            .map(|(_, binding)| binding.action)
    }

    pub fn binding(
        self,
        modifiers: BindingModifiers,
        key: BindingKey,
    ) -> Option<(usize, NiriBinding<'a>)> {
        self.entries[..self.length]
            .iter()
            .flatten()
            .copied()
            .enumerate()
            .find(|(_, binding)| binding.modifiers == modifiers && binding.key == key)
    }

    fn push(&mut self, binding: NiriBinding<'a>) -> Result<(), NiriConfigError> {
        if self.length == self.entries.len() {
            return Err(NiriConfigError::TooManyBindings);
        }
        if self.entries[..self.length]
            .iter()
            .flatten()
            .any(|current| current.modifiers == binding.modifiers && current.key == binding.key)
        {
            return Err(NiriConfigError::DuplicateBinding);
        }
        self.entries[self.length] = Some(binding);
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedWorkspace<'a> {
    pub name: &'a str,
    pub open_on_output: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamedWorkspaceList<'a> {
    entries: [Option<NamedWorkspace<'a>>; MAX_NIRI_WORKSPACES],
    length: usize,
}

impl<'a> NamedWorkspaceList<'a> {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_NIRI_WORKSPACES],
            length: 0,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn get(self, index: usize) -> Option<NamedWorkspace<'a>> {
        self.entries.get(index).copied().flatten()
    }

    pub fn index_of(self, name: &str) -> Option<usize> {
        self.entries[..self.length]
            .iter()
            .flatten()
            .position(|workspace| workspace.name == name)
    }

    fn push(&mut self, workspace: NamedWorkspace<'a>) -> Result<(), NiriConfigError> {
        if workspace.name.is_empty() || workspace.name.len() > 64 {
            return Err(NiriConfigError::InvalidWorkspace);
        }
        if self.index_of(workspace.name).is_some() {
            return Err(NiriConfigError::DuplicateWorkspace);
        }
        if self.length == self.entries.len() {
            return Err(NiriConfigError::TooManyWorkspaces);
        }
        self.entries[self.length] = Some(workspace);
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatingPositionRelativeTo {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Top,
    Bottom,
    Left,
    Right,
}

impl FloatingPositionRelativeTo {
    pub const fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FloatingPosition {
    pub x: i32,
    pub y: i32,
    pub relative_to: FloatingPositionRelativeTo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriWindowRule<'a> {
    pub app_id: Option<&'a str>,
    pub open_on_workspace: Option<&'a str>,
    pub open_floating: Option<bool>,
    pub open_maximized: Option<bool>,
    pub open_maximized_to_edges: Option<bool>,
    pub open_fullscreen: Option<bool>,
    pub open_focused: Option<bool>,
    pub focus_ring_off: bool,
    pub focus_ring_on: bool,
    pub focus_ring_width: Option<u16>,
    pub focus_ring_active_color: Option<u32>,
    pub focus_ring_inactive_color: Option<u32>,
    pub default_floating_position: Option<FloatingPosition>,
    pub default_column_width: Option<ColumnWidth>,
    pub default_window_height: Option<ColumnWidth>,
    pub default_column_display: Option<ColumnDisplay>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriWindowRuleList<'a> {
    entries: [Option<NiriWindowRule<'a>>; MAX_NIRI_WINDOW_RULES],
    length: usize,
}

impl<'a> NiriWindowRuleList<'a> {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_NIRI_WINDOW_RULES],
            length: 0,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn workspace_for(self, app_id: &str) -> Option<&'a str> {
        let mut workspace = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(name) = rule.open_on_workspace {
                    workspace = Some(name);
                }
            }
        }
        workspace
    }

    pub fn floating_for(self, app_id: &str) -> Option<bool> {
        let mut floating = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.open_floating {
                    floating = Some(value);
                }
            }
        }
        floating
    }

    pub fn maximized_for(self, app_id: &str) -> Option<bool> {
        let mut maximized = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.open_maximized {
                    maximized = Some(value);
                }
            }
        }
        maximized
    }

    pub fn maximized_to_edges_for(self, app_id: &str) -> Option<bool> {
        let mut maximized = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.open_maximized_to_edges {
                    maximized = Some(value);
                }
            }
        }
        maximized
    }

    pub fn fullscreen_for(self, app_id: &str) -> Option<bool> {
        let mut fullscreen = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.open_fullscreen {
                    fullscreen = Some(value);
                }
            }
        }
        fullscreen
    }

    pub fn focused_for(self, app_id: &str) -> Option<bool> {
        let mut focused = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.open_focused {
                    focused = Some(value);
                }
            }
        }
        focused
    }

    pub fn floating_position_for(self, app_id: &str) -> Option<FloatingPosition> {
        let mut position = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.default_floating_position {
                    position = Some(value);
                }
            }
        }
        position
    }

    pub fn focus_ring_for(self, app_id: &str, base: FocusRing) -> Option<FocusRing> {
        let mut ring = base;
        let mut overridden = false;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_some() && rule.app_id != Some(app_id) {
                continue;
            }
            if rule.focus_ring_off {
                ring.enabled = false;
                overridden = true;
            }
            if rule.focus_ring_on {
                ring.enabled = true;
                overridden = true;
            }
            if let Some(width) = rule.focus_ring_width {
                ring.width = width;
                overridden = true;
            }
            if let Some(color) = rule.focus_ring_active_color {
                ring.active_color = color;
                overridden = true;
            }
            if let Some(color) = rule.focus_ring_inactive_color {
                ring.inactive_color = color;
                overridden = true;
            }
        }
        overridden.then_some(ring)
    }

    pub fn column_width_for(self, app_id: &str) -> Option<ColumnWidth> {
        let mut width = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.default_column_width {
                    width = Some(value);
                }
            }
        }
        width
    }

    pub fn window_height_for(self, app_id: &str) -> Option<ColumnWidth> {
        let mut height = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.default_window_height {
                    height = Some(value);
                }
            }
        }
        height
    }

    pub fn column_display_for(self, app_id: &str) -> Option<ColumnDisplay> {
        let mut display = None;
        for rule in self.entries[..self.length].iter().flatten() {
            if rule.app_id.is_none() || rule.app_id == Some(app_id) {
                if let Some(value) = rule.default_column_display {
                    display = Some(value);
                }
            }
        }
        display
    }

    fn push(&mut self, rule: NiriWindowRule<'a>) -> Result<(), NiriConfigError> {
        if self.length == self.entries.len() {
            return Err(NiriConfigError::TooManyWindowRules);
        }
        self.entries[self.length] = Some(rule);
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriShellConfig<'a> {
    pub workspaces: NamedWorkspaceList<'a>,
    pub bindings: NiriBindingList<'a>,
    pub window_rules: NiriWindowRuleList<'a>,
}

impl Default for NiriShellConfig<'_> {
    fn default() -> Self {
        Self {
            workspaces: NamedWorkspaceList::empty(),
            bindings: NiriBindingList::empty(),
            window_rules: NiriWindowRuleList::empty(),
        }
    }
}

impl NiriShellConfig<'_> {
    fn validate_workspace_references(self) -> Result<(), NiriConfigError> {
        for binding in self.bindings.entries[..self.bindings.length]
            .iter()
            .flatten()
        {
            let reference = match binding.action {
                NiriAction::FocusWorkspace(reference)
                | NiriAction::MoveColumnToWorkspace(reference)
                | NiriAction::MoveWindowToWorkspace(reference) => reference,
                _ => continue,
            };
            if let WorkspaceReference::Name(name) = reference
                && self.workspaces.index_of(name).is_none()
            {
                return Err(NiriConfigError::InvalidBinding);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NiriConfigError {
    UnexpectedEnd,
    UnexpectedToken,
    InvalidWorkspace,
    DuplicateWorkspace,
    TooManyWorkspaces,
    InvalidBinding,
    DuplicateBinding,
    TooManyBindings,
    InvalidWindowRule,
    TooManyWindowRules,
}

pub fn parse_niri_shell_config(input: &str) -> Result<NiriShellConfig<'_>, NiriConfigError> {
    ShellConfigParser::new(input).parse()
}

struct ShellConfigParser<'a> {
    lexer: KdlLexer<'a>,
    pushed: Option<KdlToken<'a>>,
}

impl<'a> ShellConfigParser<'a> {
    const fn new(input: &'a str) -> Self {
        Self {
            lexer: KdlLexer::new(input),
            pushed: None,
        }
    }

    fn parse(mut self) -> Result<NiriShellConfig<'a>, NiriConfigError> {
        let mut config = NiriShellConfig::default();
        loop {
            match self.next_non_end() {
                KdlToken::Word("workspace") => {
                    config.workspaces.push(self.parse_workspace()?)?;
                }
                KdlToken::Word("binds") => {
                    self.expect_left_brace()?;
                    self.parse_bindings(&mut config.bindings)?;
                }
                KdlToken::Word("window-rule") => {
                    self.expect_left_brace()?;
                    config.window_rules.push(self.parse_window_rule()?)?;
                }
                KdlToken::Word(_) | KdlToken::String(_) | KdlToken::Equal => {
                    self.skip_node()?;
                }
                KdlToken::LeftBrace => self.skip_block()?,
                KdlToken::RightBrace => return Err(NiriConfigError::UnexpectedToken),
                KdlToken::End => {
                    config.validate_workspace_references()?;
                    return Ok(config);
                }
                KdlToken::EndNode => {}
            }
        }
    }

    fn parse_workspace(&mut self) -> Result<NamedWorkspace<'a>, NiriConfigError> {
        let KdlToken::String(name) = self.next() else {
            return Err(NiriConfigError::InvalidWorkspace);
        };
        let mut workspace = NamedWorkspace {
            name,
            open_on_output: None,
        };
        loop {
            match self.next() {
                KdlToken::LeftBrace => loop {
                    match self.next_non_end() {
                        KdlToken::Word("open-on-output") => {
                            let KdlToken::String(output) = self.next() else {
                                return Err(NiriConfigError::InvalidWorkspace);
                            };
                            workspace.open_on_output = Some(output);
                            self.finish_node()?;
                        }
                        KdlToken::Word(_) | KdlToken::String(_) | KdlToken::Equal => {
                            self.skip_node()?;
                        }
                        KdlToken::LeftBrace => self.skip_block()?,
                        KdlToken::RightBrace => return Ok(workspace),
                        KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                        KdlToken::EndNode => {}
                    }
                },
                KdlToken::EndNode => return Ok(workspace),
                KdlToken::End => return Ok(workspace),
                KdlToken::Word(_) | KdlToken::String(_) | KdlToken::Equal => {}
                KdlToken::RightBrace => {
                    self.push(KdlToken::RightBrace);
                    return Ok(workspace);
                }
            }
        }
    }

    fn parse_bindings(
        &mut self,
        bindings: &mut NiriBindingList<'a>,
    ) -> Result<(), NiriConfigError> {
        loop {
            match self.next_non_end() {
                KdlToken::RightBrace => return Ok(()),
                KdlToken::Word(hotkey) => {
                    let (modifiers, key) = parse_hotkey(hotkey)?;
                    let mut cooldown_ms = None;
                    loop {
                        match self.next() {
                            KdlToken::LeftBrace => break,
                            KdlToken::Word("cooldown-ms") if cooldown_ms.is_none() => {
                                if self.next() != KdlToken::Equal {
                                    return Err(NiriConfigError::InvalidBinding);
                                }
                                let KdlToken::Word(value) = self.next() else {
                                    return Err(NiriConfigError::InvalidBinding);
                                };
                                cooldown_ms = Some(parse_decimal_u16(value)?);
                            }
                            KdlToken::Word("cooldown-ms") => {
                                return Err(NiriConfigError::InvalidBinding);
                            }
                            KdlToken::End | KdlToken::RightBrace => {
                                return Err(NiriConfigError::InvalidBinding);
                            }
                            _ => {}
                        }
                    }
                    let action_name = loop {
                        match self.next_non_end() {
                            KdlToken::Word(action) => break action,
                            KdlToken::RightBrace | KdlToken::End => {
                                return Err(NiriConfigError::InvalidBinding);
                            }
                            _ => {}
                        }
                    };
                    let action = self.parse_action(action_name)?;
                    self.skip_to_block_end()?;
                    bindings.push(NiriBinding {
                        modifiers,
                        key,
                        cooldown_ms,
                        action,
                    })?;
                }
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                _ => return Err(NiriConfigError::InvalidBinding),
            }
        }
    }

    fn parse_window_rule(&mut self) -> Result<NiriWindowRule<'a>, NiriConfigError> {
        let mut rule = NiriWindowRule {
            app_id: None,
            open_on_workspace: None,
            open_floating: None,
            open_maximized: None,
            open_maximized_to_edges: None,
            open_fullscreen: None,
            open_focused: None,
            focus_ring_off: false,
            focus_ring_on: false,
            focus_ring_width: None,
            focus_ring_active_color: None,
            focus_ring_inactive_color: None,
            default_floating_position: None,
            default_column_width: None,
            default_window_height: None,
            default_column_display: None,
        };
        loop {
            match self.next_non_end() {
                KdlToken::Word("match") => loop {
                    match self.next() {
                        KdlToken::Word("app-id") => {
                            if self.next() != KdlToken::Equal {
                                return Err(NiriConfigError::InvalidWindowRule);
                            }
                            let KdlToken::String(app_id) = self.next() else {
                                return Err(NiriConfigError::InvalidWindowRule);
                            };
                            rule.app_id = Some(app_id);
                        }
                        KdlToken::EndNode => break,
                        KdlToken::RightBrace => {
                            self.push(KdlToken::RightBrace);
                            break;
                        }
                        KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                        _ => {}
                    }
                },
                KdlToken::Word("open-on-workspace") => {
                    let KdlToken::String(workspace) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    rule.open_on_workspace = Some(workspace);
                    self.finish_node()?;
                }
                KdlToken::Word("open-floating") => {
                    rule.open_floating = Some(match self.next() {
                        KdlToken::Word("true") => true,
                        KdlToken::Word("false") => false,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    });
                    self.finish_node()?;
                }
                KdlToken::Word("open-maximized") => {
                    rule.open_maximized = Some(match self.next() {
                        KdlToken::Word("true") => true,
                        KdlToken::Word("false") => false,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    });
                    self.finish_node()?;
                }
                KdlToken::Word("open-maximized-to-edges") => {
                    rule.open_maximized_to_edges = Some(match self.next() {
                        KdlToken::Word("true") => true,
                        KdlToken::Word("false") => false,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    });
                    self.finish_node()?;
                }
                KdlToken::Word("open-fullscreen") => {
                    rule.open_fullscreen = Some(match self.next() {
                        KdlToken::Word("true") => true,
                        KdlToken::Word("false") => false,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    });
                    self.finish_node()?;
                }
                KdlToken::Word("open-focused") => {
                    rule.open_focused = Some(match self.next() {
                        KdlToken::Word("true") => true,
                        KdlToken::Word("false") => false,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    });
                    self.finish_node()?;
                }
                KdlToken::Word("focus-ring") => self.parse_rule_focus_ring(&mut rule)?,
                KdlToken::Word("default-floating-position") => {
                    rule.default_floating_position = Some(self.parse_floating_position()?);
                }
                KdlToken::Word("default-column-width") => {
                    rule.default_column_width = Some(self.parse_rule_size()?);
                }
                KdlToken::Word("default-window-height") => {
                    rule.default_window_height = Some(self.parse_rule_size()?);
                }
                KdlToken::Word("default-column-display") => {
                    rule.default_column_display = Some(match self.next() {
                        KdlToken::String("normal") => ColumnDisplay::Normal,
                        KdlToken::String("tabbed") => ColumnDisplay::Tabbed,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    });
                    self.finish_node()?;
                }
                KdlToken::Word(_) | KdlToken::String(_) | KdlToken::Equal => {
                    self.skip_node()?;
                }
                KdlToken::LeftBrace => self.skip_block()?,
                KdlToken::RightBrace => return Ok(rule),
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                KdlToken::EndNode => {}
            }
        }
    }

    fn parse_rule_focus_ring(
        &mut self,
        rule: &mut NiriWindowRule<'a>,
    ) -> Result<(), NiriConfigError> {
        self.expect_left_brace()?;
        loop {
            match self.next_non_end() {
                KdlToken::Word("off") => {
                    rule.focus_ring_off = true;
                    self.finish_node()?;
                }
                KdlToken::Word("on") => {
                    rule.focus_ring_on = true;
                    self.finish_node()?;
                }
                KdlToken::Word("width") => {
                    let KdlToken::Word(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    rule.focus_ring_width = Some(
                        parse_decimal_u16(value).map_err(|_| NiriConfigError::InvalidWindowRule)?,
                    );
                    self.finish_node()?;
                }
                KdlToken::Word("active-color") => {
                    let KdlToken::String(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    rule.focus_ring_active_color = Some(
                        super::parse_color(value)
                            .map_err(|_| NiriConfigError::InvalidWindowRule)?,
                    );
                    self.finish_node()?;
                }
                KdlToken::Word("inactive-color") => {
                    let KdlToken::String(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    rule.focus_ring_inactive_color = Some(
                        super::parse_color(value)
                            .map_err(|_| NiriConfigError::InvalidWindowRule)?,
                    );
                    self.finish_node()?;
                }
                KdlToken::Word(_) | KdlToken::String(_) | KdlToken::Equal => self.skip_node()?,
                KdlToken::LeftBrace => self.skip_block()?,
                KdlToken::RightBrace => return Ok(()),
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                KdlToken::EndNode => {}
            }
        }
    }

    fn parse_floating_position(&mut self) -> Result<FloatingPosition, NiriConfigError> {
        let mut x = None;
        let mut y = None;
        let mut relative_to = FloatingPositionRelativeTo::TopLeft;
        let mut relative_to_seen = false;
        loop {
            match self.next() {
                KdlToken::Word("x") if x.is_none() => {
                    if self.next() != KdlToken::Equal {
                        return Err(NiriConfigError::InvalidWindowRule);
                    }
                    let KdlToken::Word(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    x = Some(parse_floating_coordinate(value)?);
                }
                KdlToken::Word("y") if y.is_none() => {
                    if self.next() != KdlToken::Equal {
                        return Err(NiriConfigError::InvalidWindowRule);
                    }
                    let KdlToken::Word(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    y = Some(parse_floating_coordinate(value)?);
                }
                KdlToken::Word("relative-to") if !relative_to_seen => {
                    if self.next() != KdlToken::Equal {
                        return Err(NiriConfigError::InvalidWindowRule);
                    }
                    let KdlToken::String(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    relative_to = match value {
                        "top-left" => FloatingPositionRelativeTo::TopLeft,
                        "top-right" => FloatingPositionRelativeTo::TopRight,
                        "bottom-left" => FloatingPositionRelativeTo::BottomLeft,
                        "bottom-right" => FloatingPositionRelativeTo::BottomRight,
                        "top" => FloatingPositionRelativeTo::Top,
                        "bottom" => FloatingPositionRelativeTo::Bottom,
                        "left" => FloatingPositionRelativeTo::Left,
                        "right" => FloatingPositionRelativeTo::Right,
                        _ => return Err(NiriConfigError::InvalidWindowRule),
                    };
                    relative_to_seen = true;
                }
                KdlToken::EndNode => {
                    return Ok(FloatingPosition {
                        x: x.ok_or(NiriConfigError::InvalidWindowRule)?,
                        y: y.ok_or(NiriConfigError::InvalidWindowRule)?,
                        relative_to,
                    });
                }
                KdlToken::RightBrace => {
                    self.push(KdlToken::RightBrace);
                    return Ok(FloatingPosition {
                        x: x.ok_or(NiriConfigError::InvalidWindowRule)?,
                        y: y.ok_or(NiriConfigError::InvalidWindowRule)?,
                        relative_to,
                    });
                }
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                _ => return Err(NiriConfigError::InvalidWindowRule),
            }
        }
    }

    fn parse_rule_size(&mut self) -> Result<ColumnWidth, NiriConfigError> {
        self.expect_left_brace()?;
        let mut width = ColumnWidth::Client;
        let mut seen = false;
        loop {
            match self.next_non_end() {
                KdlToken::RightBrace => return Ok(width),
                KdlToken::Word("proportion") if !seen => {
                    let KdlToken::Word(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    width = ColumnWidth::Proportion(
                        super::parse_thousandths(value)
                            .map_err(|_| NiriConfigError::InvalidWindowRule)?,
                    );
                    seen = true;
                    self.finish_node()?;
                }
                KdlToken::Word("fixed") if !seen => {
                    let KdlToken::Word(value) = self.next() else {
                        return Err(NiriConfigError::InvalidWindowRule);
                    };
                    width = ColumnWidth::Fixed(
                        super::parse_rounded_u16(value)
                            .map_err(|_| NiriConfigError::InvalidWindowRule)?,
                    );
                    seen = true;
                    self.finish_node()?;
                }
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                _ => return Err(NiriConfigError::InvalidWindowRule),
            }
        }
    }

    fn parse_action(&mut self, value: &str) -> Result<NiriAction<'a>, NiriConfigError> {
        Ok(match value {
            "focus-column-left" => NiriAction::FocusColumnLeft,
            "focus-column-right" => NiriAction::FocusColumnRight,
            "focus-column-first" => NiriAction::FocusColumnFirst,
            "focus-column-last" => NiriAction::FocusColumnLast,
            "focus-window-up" => NiriAction::FocusWindowUp,
            "focus-window-down" => NiriAction::FocusWindowDown,
            "move-column-left" => NiriAction::MoveColumnLeft,
            "move-column-right" => NiriAction::MoveColumnRight,
            "move-column-to-first" => NiriAction::MoveColumnToFirst,
            "move-column-to-last" => NiriAction::MoveColumnToLast,
            "move-window-up" => NiriAction::MoveWindowUp,
            "move-window-down" => NiriAction::MoveWindowDown,
            "focus-workspace-up" => NiriAction::FocusWorkspaceUp,
            "focus-workspace-down" => NiriAction::FocusWorkspaceDown,
            "focus-workspace-previous" => NiriAction::FocusWorkspacePrevious,
            "focus-workspace" => NiriAction::FocusWorkspace(self.parse_workspace_reference()?),
            "move-workspace-up" => NiriAction::MoveWorkspaceUp,
            "move-workspace-down" => NiriAction::MoveWorkspaceDown,
            "move-column-to-workspace-up" => NiriAction::MoveColumnToWorkspaceUp,
            "move-column-to-workspace-down" => NiriAction::MoveColumnToWorkspaceDown,
            "move-column-to-workspace" => {
                NiriAction::MoveColumnToWorkspace(self.parse_workspace_reference()?)
            }
            "move-window-to-workspace-up" => NiriAction::MoveWindowToWorkspaceUp,
            "move-window-to-workspace-down" => NiriAction::MoveWindowToWorkspaceDown,
            "move-window-to-workspace" => {
                NiriAction::MoveWindowToWorkspace(self.parse_workspace_reference()?)
            }
            "consume-window-into-column" => NiriAction::ConsumeWindowIntoColumn,
            "expel-window-from-column" => NiriAction::ExpelWindowFromColumn,
            "consume-or-expel-window-left" => NiriAction::ConsumeOrExpelWindowLeft,
            "consume-or-expel-window-right" => NiriAction::ConsumeOrExpelWindowRight,
            "toggle-column-tabbed-display" => NiriAction::ToggleColumnTabbedDisplay,
            "toggle-window-floating" => NiriAction::ToggleWindowFloating,
            "switch-focus-between-floating-and-tiling" => {
                NiriAction::SwitchFocusBetweenFloatingAndTiling
            }
            "move-window-to-floating" => NiriAction::MoveWindowToFloating,
            "move-window-to-tiling" => NiriAction::MoveWindowToTiling,
            "focus-floating" => NiriAction::FocusFloating,
            "focus-tiling" => NiriAction::FocusTiling,
            "switch-preset-column-width" => NiriAction::SwitchPresetColumnWidth,
            "switch-preset-column-width-back" => NiriAction::SwitchPresetColumnWidthBack,
            "switch-preset-window-height" => NiriAction::SwitchPresetWindowHeight,
            "switch-preset-window-height-back" => NiriAction::SwitchPresetWindowHeightBack,
            "maximize-column" => NiriAction::MaximizeColumn,
            "fullscreen-window" => NiriAction::FullscreenWindow,
            "maximize-window-to-edges" => NiriAction::MaximizeWindowToEdges,
            "center-column" => NiriAction::CenterColumn,
            "center-visible-columns" => NiriAction::CenterVisibleColumns,
            "expand-column-to-available-width" => NiriAction::ExpandColumnToAvailableWidth,
            "set-column-width" => {
                let KdlToken::String(width) = self.next() else {
                    return Err(NiriConfigError::InvalidBinding);
                };
                NiriAction::SetColumnWidth(parse_column_width_change(width)?)
            }
            "set-window-height" => {
                let KdlToken::String(height) = self.next() else {
                    return Err(NiriConfigError::InvalidBinding);
                };
                NiriAction::SetWindowHeight(parse_column_width_change(height)?)
            }
            "reset-window-height" => NiriAction::ResetWindowHeight,
            "close-window" => NiriAction::CloseWindow,
            _ => return Err(NiriConfigError::InvalidBinding),
        })
    }

    fn parse_workspace_reference(&mut self) -> Result<WorkspaceReference<'a>, NiriConfigError> {
        match self.next() {
            KdlToken::Word(index) => u8::try_from(parse_decimal_u16(index)?)
                .ok()
                .filter(|index| *index != 0)
                .map(WorkspaceReference::Index)
                .ok_or(NiriConfigError::InvalidBinding),
            KdlToken::String(name) if !name.is_empty() && name.len() <= 64 => {
                Ok(WorkspaceReference::Name(name))
            }
            _ => Err(NiriConfigError::InvalidBinding),
        }
    }

    fn expect_left_brace(&mut self) -> Result<(), NiriConfigError> {
        loop {
            match self.next() {
                KdlToken::LeftBrace => return Ok(()),
                KdlToken::EndNode => {}
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                _ => return Err(NiriConfigError::UnexpectedToken),
            }
        }
    }

    fn finish_node(&mut self) -> Result<(), NiriConfigError> {
        loop {
            match self.next() {
                KdlToken::EndNode | KdlToken::End => return Ok(()),
                KdlToken::LeftBrace => return self.skip_block(),
                KdlToken::RightBrace => {
                    self.push(KdlToken::RightBrace);
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    fn skip_node(&mut self) -> Result<(), NiriConfigError> {
        loop {
            match self.next() {
                KdlToken::EndNode | KdlToken::End => return Ok(()),
                KdlToken::LeftBrace => return self.skip_block(),
                KdlToken::RightBrace => {
                    self.push(KdlToken::RightBrace);
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    fn skip_block(&mut self) -> Result<(), NiriConfigError> {
        self.skip_to_block_end()
    }

    fn skip_to_block_end(&mut self) -> Result<(), NiriConfigError> {
        let mut depth = 1usize;
        while depth != 0 {
            match self.next() {
                KdlToken::LeftBrace => depth += 1,
                KdlToken::RightBrace => depth -= 1,
                KdlToken::End => return Err(NiriConfigError::UnexpectedEnd),
                _ => {}
            }
        }
        Ok(())
    }

    fn next_non_end(&mut self) -> KdlToken<'a> {
        loop {
            let token = self.next();
            if token != KdlToken::EndNode {
                return token;
            }
        }
    }

    fn next(&mut self) -> KdlToken<'a> {
        self.pushed.take().unwrap_or_else(|| self.lexer.next())
    }

    fn push(&mut self, token: KdlToken<'a>) {
        self.pushed = Some(token);
    }
}

fn parse_hotkey(hotkey: &str) -> Result<(BindingModifiers, BindingKey), NiriConfigError> {
    let mut modifiers = BindingModifiers::NONE;
    let mut key = None;
    let mut parts = hotkey.split('+').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            key = Some(parse_binding_key(part)?);
            break;
        }
        modifiers = modifiers.with(if matches!(part, "Mod" | "Super" | "Win") {
            BindingModifiers::MOD
        } else if matches!(part, "Ctrl" | "Control") {
            BindingModifiers::CTRL
        } else if part == "Shift" {
            BindingModifiers::SHIFT
        } else if part == "Alt" {
            BindingModifiers::ALT
        } else {
            return Err(NiriConfigError::InvalidBinding);
        });
    }
    Ok((modifiers, key.ok_or(NiriConfigError::InvalidBinding)?))
}

fn parse_binding_key(value: &str) -> Result<BindingKey, NiriConfigError> {
    match value {
        "Left" => Ok(BindingKey::Left),
        "Right" => Ok(BindingKey::Right),
        "Up" => Ok(BindingKey::Up),
        "Down" => Ok(BindingKey::Down),
        "PageUp" | "Page_Up" => Ok(BindingKey::PageUp),
        "PageDown" | "Page_Down" => Ok(BindingKey::PageDown),
        "WheelScrollUp" => Ok(BindingKey::WheelScrollUp),
        "WheelScrollDown" => Ok(BindingKey::WheelScrollDown),
        "Home" => Ok(BindingKey::Home),
        "End" => Ok(BindingKey::End),
        "Return" => Ok(BindingKey::Return),
        "Tab" => Ok(BindingKey::Tab),
        "Escape" => Ok(BindingKey::Escape),
        "Minus" => Ok(BindingKey::Minus),
        "Equal" => Ok(BindingKey::Equal),
        "Comma" => Ok(BindingKey::Character(b',')),
        "Period" | "Dot" => Ok(BindingKey::Character(b'.')),
        "BracketLeft" => Ok(BindingKey::Character(b'[')),
        "BracketRight" => Ok(BindingKey::Character(b']')),
        _ if value.len() == 1 => Ok(BindingKey::Character(
            value.as_bytes()[0].to_ascii_uppercase(),
        )),
        _ => Err(NiriConfigError::InvalidBinding),
    }
}

fn parse_column_width_change(value: &str) -> Result<ColumnWidthChange, NiriConfigError> {
    let (relative, negative, magnitude) = if let Some(value) = value.strip_prefix('+') {
        (true, false, value)
    } else if let Some(value) = value.strip_prefix('-') {
        (true, true, value)
    } else {
        (false, false, value)
    };
    if magnitude.is_empty() {
        return Err(NiriConfigError::InvalidBinding);
    }
    if let Some(percent) = magnitude.strip_suffix('%') {
        let percent = parse_decimal_u16(percent)?;
        if percent == 0 || percent > 100 {
            return Err(NiriConfigError::InvalidBinding);
        }
        let thousandths =
            i16::try_from(percent * 10).map_err(|_| NiriConfigError::InvalidBinding)?;
        if relative {
            Ok(ColumnWidthChange::AdjustProportion(if negative {
                -thousandths
            } else {
                thousandths
            }))
        } else {
            Ok(ColumnWidthChange::Set(ColumnWidth::Proportion(
                thousandths as u16,
            )))
        }
    } else {
        let pixels = parse_decimal_u16(magnitude)?;
        if pixels == 0 {
            return Err(NiriConfigError::InvalidBinding);
        }
        if relative {
            Ok(ColumnWidthChange::AdjustFixed(if negative {
                -i32::from(pixels)
            } else {
                i32::from(pixels)
            }))
        } else {
            Ok(ColumnWidthChange::Set(ColumnWidth::Fixed(pixels)))
        }
    }
}

fn parse_decimal_u16(value: &str) -> Result<u16, NiriConfigError> {
    let mut parsed = 0u16;
    for byte in value.bytes() {
        if !byte.is_ascii_digit() {
            return Err(NiriConfigError::InvalidBinding);
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|value| value.checked_add(u16::from(byte - b'0')))
            .ok_or(NiriConfigError::InvalidBinding)?;
    }
    Ok(parsed)
}

fn parse_floating_coordinate(value: &str) -> Result<i32, NiriConfigError> {
    let (negative, magnitude) = if let Some(value) = value.strip_prefix('-') {
        (true, value)
    } else if let Some(value) = value.strip_prefix('+') {
        (false, value)
    } else {
        (false, value)
    };
    if magnitude.is_empty() {
        return Err(NiriConfigError::InvalidWindowRule);
    }
    let thousandths = super::parse_decimal_thousandths(magnitude)
        .map_err(|_| NiriConfigError::InvalidWindowRule)?;
    if thousandths > 65_535_000 {
        return Err(NiriConfigError::InvalidWindowRule);
    }
    let coordinate = i32::try_from((thousandths + 500) / 1000)
        .map_err(|_| NiriConfigError::InvalidWindowRule)?;
    Ok(if negative { -coordinate } else { coordinate })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum KdlToken<'a> {
    Word(&'a str),
    String(&'a str),
    LeftBrace,
    RightBrace,
    Equal,
    EndNode,
    End,
}

struct KdlLexer<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> KdlLexer<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn next(&mut self) -> KdlToken<'a> {
        let bytes = self.input.as_bytes();
        loop {
            if self.offset == bytes.len() {
                return KdlToken::End;
            }
            match bytes[self.offset] {
                b' ' | b'\t' | b'\r' => self.offset += 1,
                b'\n' | b';' => {
                    self.offset += 1;
                    return KdlToken::EndNode;
                }
                b'/' if bytes.get(self.offset + 1) == Some(&b'/') => {
                    while self.offset < bytes.len() && bytes[self.offset] != b'\n' {
                        self.offset += 1;
                    }
                }
                b'{' => {
                    self.offset += 1;
                    return KdlToken::LeftBrace;
                }
                b'}' => {
                    self.offset += 1;
                    return KdlToken::RightBrace;
                }
                b'=' => {
                    self.offset += 1;
                    return KdlToken::Equal;
                }
                b'"' => return self.string(),
                _ => return self.word(),
            }
        }
    }

    fn string(&mut self) -> KdlToken<'a> {
        self.offset += 1;
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len() {
            match bytes[self.offset] {
                b'"' => {
                    let value = &self.input[start..self.offset];
                    self.offset += 1;
                    return KdlToken::String(value);
                }
                b'\\' => self.offset = (self.offset + 2).min(bytes.len()),
                _ => self.offset += 1,
            }
        }
        KdlToken::End
    }

    fn word(&mut self) -> KdlToken<'a> {
        let start = self.offset;
        let bytes = self.input.as_bytes();
        while self.offset < bytes.len()
            && !bytes[self.offset].is_ascii_whitespace()
            && !matches!(bytes[self.offset], b'{' | b'}' | b';' | b'=' | b'"')
        {
            self.offset += 1;
        }
        KdlToken::Word(&self.input[start..self.offset])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceError {
    InvalidCount,
    InvalidWorkspace,
    Layout(LayoutError),
}

const FLOATING_MOVE_STEP: i32 = 50;
const MIN_FLOATING_WIDTH: i32 = 160;
const MIN_FLOATING_HEIGHT: i32 = 120;

#[derive(Clone, Copy)]
struct FloatingEntry {
    window: u32,
    rect: Rect,
    default_rect: Rect,
}

impl FloatingEntry {
    const fn empty() -> Self {
        Self {
            window: 0,
            rect: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            default_rect: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum FloatingDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy)]
struct FloatingLayout<const WINDOWS: usize> {
    entries: [FloatingEntry; WINDOWS],
    count: usize,
    output_width: u16,
    output_height: u16,
    reserved_top: u16,
    gap: u16,
}

impl<const WINDOWS: usize> FloatingLayout<WINDOWS> {
    const fn new(output_width: u16, output_height: u16, reserved_top: u16, gap: u16) -> Self {
        Self {
            entries: [FloatingEntry::empty(); WINDOWS],
            count: 0,
            output_width,
            output_height,
            reserved_top,
            gap,
        }
    }

    const fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn contains(&self, window: u32) -> bool {
        self.index_of(window).is_some()
    }

    fn focused_window(&self) -> Option<u32> {
        self.count
            .checked_sub(1)
            .map(|index| self.entries[index].window)
    }

    fn rect(&self, window: u32) -> Option<Rect> {
        self.index_of(window).map(|index| self.entries[index].rect)
    }

    fn window_at_z(&self, index: usize) -> Option<u32> {
        (index < self.count).then_some(self.entries[index].window)
    }

    fn add_with_dimensions_and_position(
        &mut self,
        window: u32,
        source: Option<Rect>,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
        position: Option<FloatingPosition>,
    ) -> Result<(), LayoutError> {
        if self.contains(window) {
            return Err(LayoutError::DuplicateWindow);
        }
        if self.count == WINDOWS {
            return Err(LayoutError::WindowCapacity);
        }
        let mut rect = source
            .map(|rect| self.rect_near_tiled(rect))
            .unwrap_or_else(|| self.default_rect_with_dimensions(width, height));
        if let Some(position) = position {
            rect = self.rect_at_default_position(rect, position);
        }
        self.entries[self.count] = FloatingEntry {
            window,
            rect,
            default_rect: rect,
        };
        self.count += 1;
        Ok(())
    }

    fn add_entry(&mut self, entry: FloatingEntry) -> Result<(), LayoutError> {
        if self.contains(entry.window) {
            return Err(LayoutError::DuplicateWindow);
        }
        if self.count == WINDOWS {
            return Err(LayoutError::WindowCapacity);
        }
        self.entries[self.count] = entry;
        self.count += 1;
        Ok(())
    }

    fn remove(&mut self, window: u32) -> Result<FloatingEntry, LayoutError> {
        let index = self.index_of(window).ok_or(LayoutError::UnknownWindow)?;
        let entry = self.entries[index];
        for current in index..self.count - 1 {
            self.entries[current] = self.entries[current + 1];
        }
        self.count -= 1;
        self.entries[self.count] = FloatingEntry::empty();
        Ok(entry)
    }

    fn focus(&mut self, window: u32) -> Result<bool, LayoutError> {
        let index = self.index_of(window).ok_or(LayoutError::UnknownWindow)?;
        if index + 1 == self.count {
            return Ok(false);
        }
        let entry = self.entries[index];
        for current in index..self.count - 1 {
            self.entries[current] = self.entries[current + 1];
        }
        self.entries[self.count - 1] = entry;
        Ok(true)
    }

    fn focus_direction(&mut self, direction: FloatingDirection) -> bool {
        if self.count < 2 {
            return false;
        }
        let focused = self.entries[self.count - 1].rect;
        let focused_x = focused.x + i32::from(focused.width) / 2;
        let focused_y = focused.y + i32::from(focused.height) / 2;
        let mut best = None;
        for index in 0..self.count - 1 {
            let rect = self.entries[index].rect;
            let x = rect.x + i32::from(rect.width) / 2;
            let y = rect.y + i32::from(rect.height) / 2;
            let (primary, secondary) = match direction {
                FloatingDirection::Left if x < focused_x => (focused_x - x, (focused_y - y).abs()),
                FloatingDirection::Right if x > focused_x => (x - focused_x, (focused_y - y).abs()),
                FloatingDirection::Up if y < focused_y => (focused_y - y, (focused_x - x).abs()),
                FloatingDirection::Down if y > focused_y => (y - focused_y, (focused_x - x).abs()),
                _ => continue,
            };
            let score = i64::from(primary) * 4 + i64::from(secondary);
            if best.is_none_or(|(_, current)| score < current) {
                best = Some((index, score));
            }
        }
        let Some((index, _)) = best else {
            return false;
        };
        let window = self.entries[index].window;
        self.focus(window).unwrap_or(false);
        true
    }

    fn move_focused(&mut self, dx: i32, dy: i32) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let mut rect = self.entries[index].rect;
        let previous = rect;
        rect.x = rect.x.saturating_add(dx);
        rect.y = rect.y.saturating_add(dy);
        self.clamp_rect_position(&mut rect);
        self.entries[index].rect = rect;
        rect != previous
    }

    fn resize_focused(&mut self, dx: i32, dy: i32) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let previous = self.entries[index].rect;
        let maximum_width = i32::from(self.output_width).max(1);
        let maximum_height = i32::from(self.output_height.saturating_sub(self.reserved_top)).max(1);
        let width = (i32::from(previous.width) + dx)
            .clamp(MIN_FLOATING_WIDTH.min(maximum_width), maximum_width);
        let height = (i32::from(previous.height) + dy)
            .clamp(MIN_FLOATING_HEIGHT.min(maximum_height), maximum_height);
        let mut rect = Rect {
            width: width as u16,
            height: height as u16,
            ..previous
        };
        self.clamp_rect_position(&mut rect);
        self.entries[index].rect = rect;
        rect != previous
    }

    fn change_focused_width(&mut self, change: ColumnWidthChange) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let current = self.entries[index].rect.width;
        let requested = resolve_size_change(change, current, self.output_width, self.gap);
        self.resize_focused(requested - i32::from(current), 0)
    }

    fn change_focused_height(&mut self, change: ColumnWidthChange) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let current = self.entries[index].rect.height;
        let working_height = self.output_height.saturating_sub(self.reserved_top);
        let requested = resolve_size_change(change, current, working_height, self.gap);
        self.resize_focused(0, requested - i32::from(current))
    }

    fn reset_focused_height(&mut self) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let current = self.entries[index].rect.height;
        let target = self.entries[index].default_rect.height;
        self.resize_focused(0, i32::from(target) - i32::from(current))
    }

    fn center_focused(&mut self) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let previous = self.entries[index].rect;
        let mut rect = previous;
        rect.x = (i32::from(self.output_width) - i32::from(rect.width)) / 2;
        rect.y = i32::from(self.reserved_top)
            + (i32::from(self.output_height.saturating_sub(self.reserved_top))
                - i32::from(rect.height))
                / 2;
        self.clamp_rect_position(&mut rect);
        self.entries[index].rect = rect;
        rect != previous
    }

    fn expand_focused(&mut self) -> bool {
        let Some(index) = self.count.checked_sub(1) else {
            return false;
        };
        let previous = self.entries[index].rect;
        let gap = self.gap.min(self.output_width / 2);
        let width = self
            .output_width
            .saturating_sub(gap.saturating_mul(2))
            .max(1);
        let mut rect = previous;
        rect.x = i32::from(gap);
        rect.width = width;
        self.clamp_rect_position(&mut rect);
        self.entries[index].rect = rect;
        rect != previous
    }

    fn default_rect_with_dimensions(
        &self,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
    ) -> Rect {
        let width_gap = self.gap.min(self.output_width / 2);
        let default_width = ColumnWidth::Proportion(500)
            .resolve(self.output_width, width_gap)
            .max(1);
        let working_height = self.output_height.saturating_sub(self.reserved_top);
        let height_gap = self.gap.min(working_height / 2);
        let default_height =
            ((u32::from(working_height) * 2) / 3).clamp(1, u32::from(u16::MAX)) as u16;
        let maximum_width = self.output_width.max(1);
        let maximum_height = working_height.max(1);
        let width = width
            .filter(|width| *width != ColumnWidth::Client)
            .map(|width| width.resolve(self.output_width, width_gap))
            .unwrap_or(default_width)
            .clamp(
                u16::try_from(MIN_FLOATING_WIDTH)
                    .unwrap_or(u16::MAX)
                    .min(maximum_width),
                maximum_width,
            );
        let height = height
            .filter(|height| *height != ColumnWidth::Client)
            .map(|height| height.resolve(working_height, height_gap))
            .unwrap_or(default_height)
            .clamp(
                u16::try_from(MIN_FLOATING_HEIGHT)
                    .unwrap_or(u16::MAX)
                    .min(maximum_height),
                maximum_height,
            );
        Rect {
            x: (i32::from(self.output_width) - i32::from(width)) / 2,
            y: i32::from(self.reserved_top) + (i32::from(working_height) - i32::from(height)) / 2,
            width,
            height,
        }
    }

    fn rect_at_default_position(&self, mut rect: Rect, position: FloatingPosition) -> Rect {
        let working_width = i32::from(self.output_width);
        let working_height = i32::from(self.output_height.saturating_sub(self.reserved_top));
        let width = i32::from(rect.width);
        let height = i32::from(rect.height);
        rect.x = position.x;
        rect.y = i32::from(self.reserved_top) + position.y;
        if matches!(
            position.relative_to,
            FloatingPositionRelativeTo::TopRight
                | FloatingPositionRelativeTo::BottomRight
                | FloatingPositionRelativeTo::Right
        ) {
            rect.x = working_width - width - position.x;
        }
        if matches!(
            position.relative_to,
            FloatingPositionRelativeTo::BottomLeft
                | FloatingPositionRelativeTo::BottomRight
                | FloatingPositionRelativeTo::Bottom
        ) {
            rect.y = i32::from(self.reserved_top) + working_height - height - position.y;
        }
        if matches!(
            position.relative_to,
            FloatingPositionRelativeTo::Top | FloatingPositionRelativeTo::Bottom
        ) {
            rect.x += working_width / 2 - width / 2;
        }
        if matches!(
            position.relative_to,
            FloatingPositionRelativeTo::Left | FloatingPositionRelativeTo::Right
        ) {
            rect.y += working_height / 2 - height / 2;
        }
        self.clamp_rect_position(&mut rect);
        rect
    }

    fn rect_near_tiled(&self, source: Rect) -> Rect {
        let working_height = self.output_height.saturating_sub(self.reserved_top);
        let preferred_height =
            ((u32::from(working_height) * 2) / 3).clamp(1, u32::from(u16::MAX)) as u16;
        let width = source.width.min(self.output_width).max(1);
        let height = source.height.min(preferred_height).max(1);
        let mut rect = Rect {
            x: source.x + (i32::from(source.width) - i32::from(width)) / 2,
            y: source.y + (i32::from(source.height) - i32::from(height)) / 2,
            width,
            height,
        };
        self.clamp_rect_position(&mut rect);
        rect
    }

    fn clamp_rect_position(&self, rect: &mut Rect) {
        let maximum_x = i32::from(self.output_width.saturating_sub(rect.width));
        let maximum_y = i32::from(self.output_height.saturating_sub(rect.height))
            .max(i32::from(self.reserved_top));
        rect.x = rect.x.clamp(0, maximum_x.max(0));
        rect.y = rect.y.clamp(i32::from(self.reserved_top), maximum_y);
    }

    fn index_of(&self, window: u32) -> Option<usize> {
        self.entries[..self.count]
            .iter()
            .position(|entry| entry.window == window)
    }
}

fn resolve_size_change(change: ColumnWidthChange, current: u16, available: u16, gap: u16) -> i32 {
    match change {
        ColumnWidthChange::Set(size) => i32::from(size.resolve(available, gap)),
        ColumnWidthChange::AdjustProportion(thousandths) => i32::from(current).saturating_add(
            i32::from(available.saturating_sub(gap)).saturating_mul(i32::from(thousandths)) / 1000,
        ),
        ColumnWidthChange::AdjustFixed(pixels) => i32::from(current).saturating_add(pixels),
    }
}

fn distinct_pair_mut<T, const N: usize>(
    values: &mut [T; N],
    first: usize,
    second: usize,
) -> (&mut T, &mut T) {
    debug_assert!(first != second);
    if first < second {
        let (left, right) = values.split_at_mut(second);
        (&mut left[first], &mut right[0])
    } else {
        let (left, right) = values.split_at_mut(first);
        (&mut right[0], &mut left[second])
    }
}

pub struct WorkspaceSet<const WORKSPACES: usize, const COLUMNS: usize, const WINDOWS: usize> {
    layouts: [ScrollLayout<COLUMNS, WINDOWS>; WORKSPACES],
    floating: [FloatingLayout<WINDOWS>; WORKSPACES],
    floating_active: [bool; WORKSPACES],
    fullscreen: [Option<u32>; WORKSPACES],
    identities: [Option<u8>; WORKSPACES],
    remembered_floating: [Option<FloatingEntry>; WINDOWS],
    count: usize,
    active: usize,
    previous: usize,
}

impl<const WORKSPACES: usize, const COLUMNS: usize, const WINDOWS: usize>
    WorkspaceSet<WORKSPACES, COLUMNS, WINDOWS>
{
    pub fn new(
        count: usize,
        persistent: usize,
        output_width: u16,
        output_height: u16,
        reserved_top: u16,
        config: LayoutConfig,
    ) -> Result<Self, WorkspaceError> {
        if count == 0
            || count > WORKSPACES
            || persistent > count
            || persistent > usize::from(u8::MAX)
        {
            return Err(WorkspaceError::InvalidCount);
        }
        let mut identities = [None; WORKSPACES];
        for (index, identity) in identities[..persistent].iter_mut().enumerate() {
            *identity = Some(u8::try_from(index).map_err(|_| WorkspaceError::InvalidCount)?);
        }
        Ok(Self {
            layouts: core::array::from_fn(|_| {
                ScrollLayout::new(output_width, output_height, reserved_top, config)
            }),
            floating: core::array::from_fn(|_| {
                FloatingLayout::new(output_width, output_height, reserved_top, config.gaps)
            }),
            floating_active: [false; WORKSPACES],
            fullscreen: [None; WORKSPACES],
            identities,
            remembered_floating: [None; WINDOWS],
            count,
            active: 0,
            previous: 0,
        })
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub const fn active(&self) -> usize {
        self.active
    }

    pub const fn previous(&self) -> usize {
        self.previous
    }

    pub fn workspace_identity(&self, workspace: usize) -> Result<Option<u8>, WorkspaceError> {
        self.identities
            .get(workspace)
            .filter(|_| workspace < self.count)
            .copied()
            .ok_or(WorkspaceError::InvalidWorkspace)
    }

    pub fn workspace_for_identity(&self, identity: u8) -> Option<usize> {
        self.identities[..self.count]
            .iter()
            .position(|current| *current == Some(identity))
    }

    pub fn workspace_is_empty(&self, workspace: usize) -> Result<bool, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        Ok(self.layouts[workspace].is_empty() && self.floating[workspace].is_empty())
    }

    pub fn config(&self) -> LayoutConfig {
        self.layouts[self.active].config()
    }

    pub fn open_window(&mut self, workspace: usize, window: u32) -> Result<(), WorkspaceError> {
        self.open_window_with_dimensions(workspace, window, None, None)
    }

    pub fn open_window_with_width(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
    ) -> Result<(), WorkspaceError> {
        self.open_window_with_dimensions(workspace, window, width, None)
    }

    pub fn open_window_with_dimensions(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
    ) -> Result<(), WorkspaceError> {
        self.open_window_with_properties(workspace, window, width, height, None)
    }

    pub fn open_window_with_properties(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
        display: Option<ColumnDisplay>,
    ) -> Result<(), WorkspaceError> {
        self.open_window_with_properties_and_focus(workspace, window, width, height, display, true)
    }

    pub fn open_window_with_properties_and_focus(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
        display: Option<ColumnDisplay>,
        focus: bool,
    ) -> Result<(), WorkspaceError> {
        let previous = self.focused_window_in_workspace(workspace)?;
        self.layouts
            .get_mut(workspace)
            .filter(|_| workspace < self.count)
            .ok_or(WorkspaceError::InvalidWorkspace)?
            .open_window_with_properties(window, width, height, display)
            .map_err(WorkspaceError::Layout)?;
        if focus {
            self.focus_window_in_workspace(workspace, window)
        } else if let Some(previous) = previous {
            self.focus_window_in_workspace(workspace, previous)
        } else {
            Ok(())
        }
    }

    pub fn set_window_maximized(
        &mut self,
        workspace: usize,
        window: u32,
        maximized: bool,
    ) -> Result<bool, WorkspaceError> {
        self.layouts
            .get_mut(workspace)
            .filter(|_| workspace < self.count)
            .ok_or(WorkspaceError::InvalidWorkspace)?
            .set_window_maximized(window, maximized)
            .map_err(WorkspaceError::Layout)
    }

    pub fn set_window_maximized_to_edges(
        &mut self,
        workspace: usize,
        window: u32,
        maximized: bool,
    ) -> Result<bool, WorkspaceError> {
        self.layouts
            .get_mut(workspace)
            .filter(|_| workspace < self.count)
            .ok_or(WorkspaceError::InvalidWorkspace)?
            .set_window_maximized_to_edges(window, maximized)
            .map_err(WorkspaceError::Layout)
    }

    pub fn open_floating_window(
        &mut self,
        workspace: usize,
        window: u32,
    ) -> Result<(), WorkspaceError> {
        self.open_floating_window_with_dimensions(workspace, window, None, None)
    }

    pub fn open_floating_window_with_dimensions(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
    ) -> Result<(), WorkspaceError> {
        self.open_floating_window_with_dimensions_and_focus(workspace, window, width, height, true)
    }

    pub fn open_floating_window_with_dimensions_and_focus(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
        focus: bool,
    ) -> Result<(), WorkspaceError> {
        self.open_floating_window_with_properties(workspace, window, width, height, None, focus)
    }

    pub fn open_floating_window_with_properties(
        &mut self,
        workspace: usize,
        window: u32,
        width: Option<ColumnWidth>,
        height: Option<ColumnWidth>,
        position: Option<FloatingPosition>,
        focus: bool,
    ) -> Result<(), WorkspaceError> {
        let previous = self.focused_window_in_workspace(workspace)?;
        self.floating
            .get_mut(workspace)
            .filter(|_| workspace < self.count)
            .ok_or(WorkspaceError::InvalidWorkspace)?
            .add_with_dimensions_and_position(window, None, width, height, position)
            .map_err(WorkspaceError::Layout)?;
        if focus {
            self.focus_window_in_workspace(workspace, window)
        } else if let Some(previous) = previous {
            self.focus_window_in_workspace(workspace, previous)
        } else {
            self.floating_active[workspace] = true;
            Ok(())
        }
    }

    pub fn focus_window(&mut self, window: u32) -> Result<(), WorkspaceError> {
        self.focus_window_in_workspace(self.active, window)
    }

    pub fn focus_window_without_workspace_switch(&mut self, window: u32) -> bool {
        let Some(workspace) = (0..self.count).find(|workspace| {
            self.floating[*workspace].contains(window)
                || self.layouts[*workspace].tile_rect(window).is_ok()
        }) else {
            return false;
        };
        self.focus_window_in_workspace(workspace, window).is_ok()
    }

    pub fn floating_position_is_remembered(&self, window: u32) -> bool {
        self.remembered_floating_position(window).is_some()
    }

    fn remembered_floating_position(&self, window: u32) -> Option<FloatingEntry> {
        self.remembered_floating
            .iter()
            .flatten()
            .find(|entry| entry.window == window)
            .copied()
    }

    fn remember_floating_position(&mut self, entry: FloatingEntry) {
        let index = self
            .remembered_floating
            .iter()
            .position(|slot| slot.is_some_and(|current| current.window == entry.window))
            .or_else(|| {
                self.remembered_floating
                    .iter()
                    .position(|slot| slot.is_none())
            });
        if let Some(index) = index {
            self.remembered_floating[index] = Some(entry);
        }
    }

    fn forget_floating_position(&mut self, window: u32) {
        if let Some(slot) = self
            .remembered_floating
            .iter_mut()
            .find(|slot| slot.is_some_and(|entry| entry.window == window))
        {
            *slot = None;
        }
    }

    fn focus_window_in_workspace(
        &mut self,
        workspace: usize,
        window: u32,
    ) -> Result<(), WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        if self.fullscreen[workspace].is_some_and(|fullscreen| fullscreen != window) {
            return Err(WorkspaceError::Layout(LayoutError::UnknownWindow));
        }
        if self.floating[workspace].contains(window) {
            self.floating[workspace]
                .focus(window)
                .map_err(WorkspaceError::Layout)?;
            self.floating_active[workspace] = true;
            return Ok(());
        }
        self.layouts[workspace]
            .focus_window(window)
            .map_err(WorkspaceError::Layout)?;
        self.floating_active[workspace] = false;
        Ok(())
    }

    fn focused_window_in_workspace(&self, workspace: usize) -> Result<Option<u32>, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        if let Some(fullscreen) = self.fullscreen[workspace] {
            return Ok(Some(fullscreen));
        }
        Ok(if self.floating_active[workspace] {
            self.floating[workspace]
                .focused_window()
                .or_else(|| self.layouts[workspace].focused_window())
        } else {
            self.layouts[workspace]
                .focused_window()
                .or_else(|| self.floating[workspace].focused_window())
        })
    }

    pub fn close_window(&mut self, window: u32) -> Result<(), WorkspaceError> {
        if self.fullscreen[self.active] == Some(window) {
            self.fullscreen[self.active] = None;
        }
        if self.floating[self.active].contains(window) {
            self.floating[self.active]
                .remove(window)
                .map_err(WorkspaceError::Layout)?;
            if self.floating[self.active].is_empty() {
                self.floating_active[self.active] = false;
            }
            self.forget_floating_position(window);
            return Ok(());
        }
        self.layouts[self.active]
            .close_window(window)
            .map_err(WorkspaceError::Layout)?;
        self.forget_floating_position(window);
        Ok(())
    }

    pub fn tile_rect(&self, window: u32) -> Result<Rect, WorkspaceError> {
        self.window_rect_in_workspace(self.active, window)
    }

    pub fn window_rect_in_workspace(
        &self,
        workspace: usize,
        window: u32,
    ) -> Result<Rect, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        if self.fullscreen[workspace] == Some(window) {
            return Ok(Rect {
                x: 0,
                y: 0,
                width: self.floating[workspace].output_width,
                height: self.floating[workspace].output_height,
            });
        }
        if let Some(rect) = self.floating[workspace].rect(window) {
            return Ok(rect);
        }
        self.layouts[workspace]
            .tile_rect(window)
            .map_err(WorkspaceError::Layout)
    }

    pub fn window_is_visible(&self, window: u32) -> bool {
        if let Some(fullscreen) = self.fullscreen[self.active] {
            return window == fullscreen;
        }
        self.floating[self.active].contains(window)
            || self.layouts[self.active].window_is_visible(window)
    }

    pub fn tabbed_column_info(&self, window: u32) -> Option<TabbedColumnInfo> {
        if self.fullscreen[self.active] == Some(window) {
            return None;
        }
        if self.floating[self.active].contains(window) {
            return None;
        }
        self.layouts[self.active].tabbed_column_info(window)
    }

    pub fn focused_window(&self) -> Option<u32> {
        if let Some(fullscreen) = self.fullscreen[self.active] {
            return Some(fullscreen);
        }
        if self.floating_layer_is_active() {
            self.floating[self.active]
                .focused_window()
                .or_else(|| self.layouts[self.active].focused_window())
        } else {
            self.layouts[self.active]
                .focused_window()
                .or_else(|| self.floating[self.active].focused_window())
        }
    }

    pub fn focused_window_is_floating(&self) -> bool {
        self.floating_layer_is_active()
    }

    pub fn window_is_floating(&self, window: u32) -> bool {
        self.floating[self.active].contains(window)
    }

    pub fn fullscreen_window(&self) -> Option<u32> {
        self.fullscreen[self.active]
    }

    pub fn window_is_fullscreen(&self, window: u32) -> bool {
        self.fullscreen[self.active] == Some(window)
    }

    pub fn set_window_fullscreen(
        &mut self,
        workspace: usize,
        window: u32,
        fullscreen: bool,
    ) -> Result<bool, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        if !self.floating[workspace].contains(window)
            && self.layouts[workspace].tile_rect(window).is_err()
        {
            return Err(WorkspaceError::Layout(LayoutError::UnknownWindow));
        }
        let current = self.fullscreen[workspace];
        let next = if fullscreen {
            Some(window)
        } else if current == Some(window) {
            None
        } else {
            current
        };
        self.fullscreen[workspace] = next;
        Ok(current != next)
    }

    pub fn toggle_focused_window_fullscreen(&mut self) -> bool {
        if self.fullscreen[self.active].is_some() {
            self.fullscreen[self.active] = None;
            return true;
        }
        let Some(window) = self.focused_window() else {
            return false;
        };
        self.fullscreen[self.active] = Some(window);
        true
    }

    pub fn window_is_floating_anywhere(&self, window: u32) -> bool {
        self.floating[..self.count]
            .iter()
            .any(|layout| layout.contains(window))
    }

    pub fn floating_window_at_z(&self, index: usize) -> Option<u32> {
        self.floating[self.active].window_at_z(index)
    }

    fn floating_layer_is_active(&self) -> bool {
        !self.floating[self.active].is_empty()
            && (self.floating_active[self.active] || self.layouts[self.active].is_empty())
    }

    pub fn focus_column_left(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].focus_direction(FloatingDirection::Left);
        }
        self.layouts[self.active].focus_column_left()
    }

    pub fn focus_column_right(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].focus_direction(FloatingDirection::Right);
        }
        self.layouts[self.active].focus_column_right()
    }

    pub fn focus_column_first(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].focus_column_first()
    }

    pub fn focus_column_last(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].focus_column_last()
    }

    pub fn focus_window_up(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].focus_direction(FloatingDirection::Up);
        }
        self.layouts[self.active].focus_window_up()
    }

    pub fn focus_window_down(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].focus_direction(FloatingDirection::Down);
        }
        self.layouts[self.active].focus_window_down()
    }

    pub fn move_column_left(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].move_focused(-FLOATING_MOVE_STEP, 0);
        }
        self.layouts[self.active].move_column_left()
    }

    pub fn move_column_right(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].move_focused(FLOATING_MOVE_STEP, 0);
        }
        self.layouts[self.active].move_column_right()
    }

    pub fn move_column_to_first(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].move_column_to_first()
    }

    pub fn move_column_to_last(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].move_column_to_last()
    }

    pub fn move_window_up(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].move_focused(0, -FLOATING_MOVE_STEP);
        }
        self.layouts[self.active].move_window_up()
    }

    pub fn move_window_down(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].move_focused(0, FLOATING_MOVE_STEP);
        }
        self.layouts[self.active].move_window_down()
    }

    pub fn consume_window_into_column(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].consume_window_into_column()
    }

    pub fn expel_window_from_column(&mut self) -> bool {
        self.expel_window_from_column_with_display(None)
    }

    pub fn expel_window_from_column_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].expel_window_from_column_with_display(display)
    }

    pub fn consume_or_expel_focused_window_left(&mut self) -> bool {
        self.consume_or_expel_focused_window_left_with_display(None)
    }

    pub fn consume_or_expel_focused_window_left_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].consume_or_expel_focused_window_left_with_display(display)
    }

    pub fn consume_or_expel_focused_window_right(&mut self) -> bool {
        self.consume_or_expel_focused_window_right_with_display(None)
    }

    pub fn consume_or_expel_focused_window_right_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].consume_or_expel_focused_window_right_with_display(display)
    }

    pub fn toggle_focused_column_tabbed_display(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].toggle_focused_column_tabbed_display()
    }

    pub fn toggle_focused_window_floating(&mut self) -> bool {
        self.toggle_focused_window_floating_with_display(None)
    }

    pub fn toggle_focused_window_floating_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        self.toggle_focused_window_floating_with_properties(display, None)
    }

    pub fn toggle_focused_window_floating_with_properties(
        &mut self,
        display: Option<ColumnDisplay>,
        position: Option<FloatingPosition>,
    ) -> bool {
        if self.focused_window_is_floating() {
            self.move_focused_window_to_tiling_with_display(display)
        } else {
            self.move_focused_window_to_floating_with_position(position)
        }
    }

    pub fn move_focused_window_to_floating(&mut self) -> bool {
        self.move_focused_window_to_floating_with_position(None)
    }

    pub fn move_focused_window_to_floating_with_position(
        &mut self,
        position: Option<FloatingPosition>,
    ) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        let Some(window) = self.layouts[self.active].focused_window() else {
            return false;
        };
        let Ok(rect) = self.layouts[self.active].tile_rect(window) else {
            return false;
        };
        if self.layouts[self.active].close_window(window).is_err() {
            return false;
        }
        let remembered = self.remembered_floating_position(window);
        let added = if let Some(entry) = remembered {
            self.floating[self.active].add_entry(entry)
        } else {
            self.floating[self.active].add_with_dimensions_and_position(
                window,
                Some(rect),
                None,
                None,
                position,
            )
        };
        if added.is_err() {
            self.layouts[self.active]
                .open_window(window)
                .expect("detached tiled window has capacity to roll back");
            return false;
        }
        self.forget_floating_position(window);
        self.floating_active[self.active] = true;
        true
    }

    pub fn move_focused_window_to_tiling(&mut self) -> bool {
        self.move_focused_window_to_tiling_with_display(None)
    }

    pub fn move_focused_window_to_tiling_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if !self.focused_window_is_floating() {
            return false;
        }
        let Some(window) = self.floating[self.active].focused_window() else {
            return false;
        };
        let entry = self.floating[self.active]
            .remove(window)
            .expect("focused floating window is present");
        if self.layouts[self.active]
            .open_window_with_properties(window, None, None, display)
            .is_err()
        {
            self.floating[self.active]
                .add_entry(entry)
                .expect("removed floating window has capacity to roll back");
            return false;
        }
        self.remember_floating_position(entry);
        self.floating_active[self.active] = false;
        true
    }

    pub fn switch_focus_between_floating_and_tiling(&mut self) -> bool {
        if self.layouts[self.active].is_empty() || self.floating[self.active].is_empty() {
            return false;
        }
        self.floating_active[self.active] = !self.floating_active[self.active];
        true
    }

    pub fn focus_floating(&mut self) -> bool {
        if self.floating[self.active].is_empty() || self.floating_layer_is_active() {
            return false;
        }
        self.floating_active[self.active] = true;
        true
    }

    pub fn focus_tiling(&mut self) -> bool {
        if self.layouts[self.active].is_empty() || !self.floating_layer_is_active() {
            return false;
        }
        self.floating_active[self.active] = false;
        true
    }

    pub fn move_focused_floating(&mut self, dx: i32, dy: i32) -> bool {
        self.focused_window_is_floating() && self.floating[self.active].move_focused(dx, dy)
    }

    pub fn resize_focused_floating(&mut self, dx: i32, dy: i32) -> bool {
        self.focused_window_is_floating() && self.floating[self.active].resize_focused(dx, dy)
    }

    pub fn scroll_by(&mut self, delta: i32) {
        self.layouts[self.active].scroll_by(delta);
    }

    pub fn change_focused_column_width(
        &mut self,
        change: ColumnWidthChange,
    ) -> Result<bool, WorkspaceError> {
        if self.focused_window_is_floating() {
            return Ok(self.floating[self.active].change_focused_width(change));
        }
        self.layouts[self.active]
            .change_focused_column_width(change)
            .map_err(WorkspaceError::Layout)
    }

    pub fn change_focused_window_height(
        &mut self,
        change: ColumnWidthChange,
    ) -> Result<bool, WorkspaceError> {
        if self.focused_window_is_floating() {
            return Ok(self.floating[self.active].change_focused_height(change));
        }
        self.layouts[self.active]
            .change_focused_window_height(change)
            .map_err(WorkspaceError::Layout)
    }

    pub fn reset_focused_window_height(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].reset_focused_height();
        }
        self.layouts[self.active].reset_focused_window_height()
    }

    pub fn switch_preset_column_width(&mut self) -> bool {
        if self.focused_window_is_floating() {
            let config = self.config();
            let Some(window) = self.focused_window() else {
                return false;
            };
            let Ok(rect) = self.tile_rect(window) else {
                return false;
            };
            if config.preset_column_widths.is_empty() {
                return false;
            }
            let index = super::next_preset_index(
                config.preset_column_widths,
                rect.width,
                self.floating[self.active].output_width,
                config.gaps,
                false,
            );
            return self.floating[self.active].change_focused_width(ColumnWidthChange::Set(
                config
                    .preset_column_widths
                    .get(index)
                    .expect("floating preset index is bounded"),
            ));
        }
        self.layouts[self.active].switch_preset_column_width()
    }

    pub fn switch_preset_column_width_back(&mut self) -> bool {
        if self.focused_window_is_floating() {
            let config = self.config();
            let Some(window) = self.focused_window() else {
                return false;
            };
            let Ok(rect) = self.tile_rect(window) else {
                return false;
            };
            if config.preset_column_widths.is_empty() {
                return false;
            }
            let index = super::next_preset_index(
                config.preset_column_widths,
                rect.width,
                self.floating[self.active].output_width,
                config.gaps,
                true,
            );
            return self.floating[self.active].change_focused_width(ColumnWidthChange::Set(
                config
                    .preset_column_widths
                    .get(index)
                    .expect("floating preset index is bounded"),
            ));
        }
        self.layouts[self.active].switch_preset_column_width_back()
    }

    pub fn switch_preset_window_height(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.switch_floating_preset_height(false);
        }
        self.layouts[self.active].switch_preset_window_height()
    }

    pub fn switch_preset_window_height_back(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.switch_floating_preset_height(true);
        }
        self.layouts[self.active].switch_preset_window_height_back()
    }

    pub fn maximize_focused_column(&mut self) -> bool {
        self.maximize_focused_column_with_display(None)
    }

    pub fn maximize_focused_column_with_display(&mut self, display: Option<ColumnDisplay>) -> bool {
        if self.focused_window_is_floating()
            && !self.toggle_focused_window_floating_with_display(display)
        {
            return false;
        }
        self.layouts[self.active].toggle_maximize_focused_column()
    }

    pub fn maximize_focused_window_to_edges(&mut self) -> bool {
        self.maximize_focused_window_to_edges_with_display(None)
    }

    pub fn maximize_focused_window_to_edges_with_display(
        &mut self,
        display: Option<ColumnDisplay>,
    ) -> bool {
        if self.focused_window_is_floating()
            && !self.toggle_focused_window_floating_with_display(display)
        {
            return false;
        }
        self.layouts[self.active].toggle_maximize_focused_window_to_edges_with_display(display)
    }

    pub fn center_focused_column(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].center_focused();
        }
        self.layouts[self.active].center_focused_column()
    }

    pub fn center_visible_columns(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return false;
        }
        self.layouts[self.active].center_visible_columns()
    }

    pub fn expand_focused_column_to_available_width(&mut self) -> bool {
        if self.focused_window_is_floating() {
            return self.floating[self.active].expand_focused();
        }
        self.layouts[self.active].expand_focused_column_to_available_width()
    }

    pub fn view_offset(&self) -> i32 {
        self.layouts[self.active].view_offset()
    }

    fn switch_floating_preset_height(&mut self, backwards: bool) -> bool {
        let config = self.config();
        if config.preset_window_heights.is_empty() {
            return false;
        }
        let Some(window) = self.focused_window() else {
            return false;
        };
        let Ok(rect) = self.tile_rect(window) else {
            return false;
        };
        let working_height = self.floating[self.active]
            .output_height
            .saturating_sub(self.floating[self.active].reserved_top);
        let index = super::next_preset_index(
            config.preset_window_heights,
            rect.height,
            working_height,
            config.gaps,
            backwards,
        );
        self.floating[self.active].change_focused_height(ColumnWidthChange::Set(
            config
                .preset_window_heights
                .get(index)
                .expect("floating height preset index is bounded"),
        ))
    }

    pub fn focus_workspace_up(&mut self) -> bool {
        if self.active == 0 {
            return false;
        }
        self.focus_workspace(self.active - 1).unwrap_or(false)
    }

    pub fn focus_workspace_down(&mut self) -> bool {
        if self.active + 1 >= self.count {
            return false;
        }
        self.focus_workspace(self.active + 1).unwrap_or(false)
    }

    pub fn focus_workspace_previous(&mut self) -> bool {
        if self.previous == self.active || self.previous >= self.count {
            return false;
        }
        core::mem::swap(&mut self.active, &mut self.previous);
        true
    }

    pub fn focus_workspace(&mut self, workspace: usize) -> Result<bool, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        let changed = workspace != self.active;
        if changed {
            self.previous = self.active;
            self.active = workspace;
        }
        Ok(changed)
    }

    pub fn move_workspace_up(&mut self) -> bool {
        if self.active == 0 {
            return false;
        }
        self.move_active_workspace_to(self.active - 1)
    }

    pub fn move_workspace_down(&mut self) -> bool {
        if self.active + 1 >= self.count {
            return false;
        }
        self.move_active_workspace_to(self.active + 1)
    }

    pub fn move_focused_column_to_workspace(
        &mut self,
        workspace: usize,
    ) -> Result<bool, WorkspaceError> {
        self.move_focused_to_workspace(workspace, true, None)
    }

    pub fn move_focused_window_to_workspace(
        &mut self,
        workspace: usize,
    ) -> Result<bool, WorkspaceError> {
        self.move_focused_window_to_workspace_with_display(workspace, None)
    }

    pub fn move_focused_window_to_workspace_with_display(
        &mut self,
        workspace: usize,
        display: Option<ColumnDisplay>,
    ) -> Result<bool, WorkspaceError> {
        self.move_focused_to_workspace(workspace, false, display)
    }

    fn move_focused_to_workspace(
        &mut self,
        workspace: usize,
        whole_column: bool,
        display: Option<ColumnDisplay>,
    ) -> Result<bool, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        if workspace == self.active {
            return Ok(false);
        }
        if self.focused_window_is_floating() {
            let Some(window) = self.floating[self.active].focused_window() else {
                return Ok(false);
            };
            let entry = self.floating[self.active]
                .remove(window)
                .map_err(WorkspaceError::Layout)?;
            if let Err(error) = self.floating[workspace].add_entry(entry) {
                self.floating[self.active]
                    .add_entry(entry)
                    .expect("floating workspace move rollback has capacity");
                return Err(WorkspaceError::Layout(error));
            }
            if self.floating[self.active].is_empty() {
                self.floating_active[self.active] = false;
            }
            self.floating_active[workspace] = true;
        } else {
            let (source, destination) =
                distinct_pair_mut(&mut self.layouts, self.active, workspace);
            let moved = if whole_column {
                source.move_focused_column_to(destination)
            } else {
                source.move_focused_window_to_with_display(destination, display)
            }
            .map_err(WorkspaceError::Layout)?;
            if !moved {
                return Ok(false);
            }
            self.floating_active[workspace] = false;
        }
        self.previous = self.active;
        self.active = workspace;
        Ok(true)
    }

    pub fn normalize_dynamic(&mut self, persistent: usize) -> Result<bool, WorkspaceError> {
        if persistent >= self.count
            || self.identities[..self.count]
                .iter()
                .filter(|identity| identity.is_some())
                .count()
                != persistent
        {
            return Err(WorkspaceError::InvalidCount);
        }
        let mut changed = false;
        let mut workspace = 0;
        while workspace + 1 < self.count {
            if self.identities[workspace].is_none()
                && self.layouts[workspace].is_empty()
                && self.floating[workspace].is_empty()
                && self.active != workspace
            {
                for index in workspace..self.count - 1 {
                    self.layouts.swap(index, index + 1);
                    self.floating.swap(index, index + 1);
                    self.floating_active.swap(index, index + 1);
                    self.fullscreen.swap(index, index + 1);
                    self.identities.swap(index, index + 1);
                }
                self.count -= 1;
                if self.active > workspace {
                    self.active -= 1;
                }
                if self.previous > workspace {
                    self.previous -= 1;
                } else if self.previous == workspace {
                    self.previous = self.active;
                }
                changed = true;
            } else {
                workspace += 1;
            }
        }
        if (!self.layouts[self.count - 1].is_empty() || !self.floating[self.count - 1].is_empty())
            && self.count < WORKSPACES
        {
            if !self.layouts[self.count].is_empty()
                || !self.floating[self.count].is_empty()
                || self.fullscreen[self.count].is_some()
                || self.identities[self.count].is_some()
            {
                return Err(WorkspaceError::InvalidCount);
            }
            self.identities[self.count] = None;
            self.count += 1;
            changed = true;
        }
        Ok(changed)
    }

    fn move_active_workspace_to(&mut self, target: usize) -> bool {
        let active_is_empty_dynamic = self.identities[self.active].is_none()
            && self.layouts[self.active].is_empty()
            && self.floating[self.active].is_empty();
        let target_is_empty_dynamic = self.identities[target].is_none()
            && self.layouts[target].is_empty()
            && self.floating[target].is_empty();
        if active_is_empty_dynamic || target_is_empty_dynamic {
            return false;
        }

        let source = self.active;
        self.layouts.swap(source, target);
        self.floating.swap(source, target);
        self.floating_active.swap(source, target);
        self.fullscreen.swap(source, target);
        self.identities.swap(source, target);
        self.active = target;
        if self.previous == source {
            self.previous = target;
        } else if self.previous == target {
            self.previous = source;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_workspaces_bindings_and_ordered_window_rules() {
        let config = parse_niri_shell_config(
            r##"
            workspace "main"
            workspace "config" { open-on-output "SLOPOS-1"; }
            binds {
                Mod+Left { focus-column-left; }
                Mod+Home { focus-column-first; }
                Mod+End { focus-column-last; }
                Mod+Shift+Left { move-column-left; }
                Mod+Shift+Right { move-column-right; }
                Mod+Ctrl+Home { move-column-to-first; }
                Mod+Ctrl+End { move-column-to-last; }
                Mod+Shift+Down repeat=false { move-column-to-workspace-down; }
                Mod+Shift+Page_Up { move-workspace-up; }
                Mod+Shift+Page_Down { move-workspace-down; }
                Mod+WheelScrollDown cooldown-ms=150 { focus-workspace-down; }
                Mod+Ctrl+WheelScrollUp cooldown-ms=150 { move-column-to-workspace-up; }
                Mod+Shift+WheelScrollDown { focus-column-right; }
                Mod+Ctrl+Shift+WheelScrollUp { move-column-left; }
                Mod+1 { focus-workspace 1; }
                Mod+Ctrl+2 { move-column-to-workspace 2; }
                Mod+Alt+C { focus-workspace "config"; }
                Mod+Ctrl+Alt+M { move-column-to-workspace "main"; }
                Mod+Alt+Up { move-window-to-workspace-up; }
                Mod+Alt+Down { move-window-to-workspace-down; }
                Mod+Ctrl+Shift+2 { move-window-to-workspace 2; }
                Mod+Shift+Alt+M { move-window-to-workspace "main"; }
                Mod+Tab { focus-workspace-previous; }
                Mod+K { focus-window-up; }
                Mod+J { focus-window-down; }
                Mod+Ctrl+K { move-window-up; }
                Mod+Ctrl+J { move-window-down; }
                Mod+Comma { consume-window-into-column; }
                Mod+Period { expel-window-from-column; }
                Mod+BracketLeft { consume-or-expel-window-left; }
                Mod+BracketRight { consume-or-expel-window-right; }
                Mod+W { toggle-column-tabbed-display; }
                Mod+V { toggle-window-floating; }
                Mod+Shift+V { switch-focus-between-floating-and-tiling; }
                Mod+Alt+V { move-window-to-floating; }
                Mod+Ctrl+V { move-window-to-tiling; }
                Mod+Alt+G { focus-floating; }
                Mod+Alt+T { focus-tiling; }
                Mod+Minus { set-column-width "-10%"; }
                Mod+Equal { set-column-width "640"; }
                Mod+Shift+Minus { set-window-height "-10%"; }
                Mod+Shift+Equal { set-window-height "+10%"; }
                Mod+R { switch-preset-column-width; }
                Mod+Shift+R { switch-preset-column-width-back; }
                Mod+Ctrl+Shift+R { switch-preset-window-height; }
                Mod+Ctrl+Alt+R { switch-preset-window-height-back; }
                Mod+Ctrl+R { reset-window-height; }
                Mod+F { maximize-column; }
                Mod+Shift+F { fullscreen-window; }
                Mod+M { maximize-window-to-edges; }
                Mod+C { center-column; }
                Mod+Ctrl+C { center-visible-columns; }
                Mod+Ctrl+F { expand-column-to-available-width; }
                Mod+Q { close-window; }
            }
            window-rule {
                open-maximized true
                open-maximized-to-edges true
                open-fullscreen true
                open-focused true
                focus-ring {
                    off
                    width 5
                    active-color "#112233"
                }
                default-floating-position x=10 y=20
                default-column-width { fixed 600; }
                default-window-height { fixed 400; }
                default-column-display "normal"
            }
            window-rule {
                match app-id="slopos-config"
                open-on-workspace "main"
                open-floating true
                open-maximized true
                open-maximized-to-edges true
                open-fullscreen true
                open-focused true
                focus-ring {
                    on
                    width 7
                    inactive-color "#445566"
                }
                default-floating-position x=-10.4 y=200 relative-to="bottom-left"
                default-column-width { proportion 0.333; }
                default-window-height { proportion 0.333; }
                default-column-display "normal"
            }
            window-rule {
                match app-id="slopos-config"
                open-on-workspace "config"
                open-floating false
                open-maximized false
                open-maximized-to-edges false
                open-fullscreen false
                open-focused false
                focus-ring {
                    off
                    active-color "#778899"
                }
                default-floating-position x=32.6 y=48.4 relative-to="bottom-right"
                default-column-width { proportion 0.667; }
                default-window-height { proportion 0.5; }
                default-column-display "tabbed"
            }
            "##,
        )
        .unwrap();
        assert_eq!(config.workspaces.len(), 2);
        assert_eq!(config.workspaces.get(1).unwrap().name, "config");
        assert_eq!(
            config.workspaces.get(1).unwrap().open_on_output,
            Some("SLOPOS-1")
        );
        assert_eq!(config.bindings.len(), 54);
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Left),
            Some(NiriAction::FocusColumnLeft)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Home),
            Some(NiriAction::FocusColumnFirst)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::End),
            Some(NiriAction::FocusColumnLast)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Left
            ),
            Some(NiriAction::MoveColumnLeft)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Right
            ),
            Some(NiriAction::MoveColumnRight)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Home
            ),
            Some(NiriAction::MoveColumnToFirst)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::End
            ),
            Some(NiriAction::MoveColumnToLast)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Down
            ),
            Some(NiriAction::MoveColumnToWorkspaceDown)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::PageUp
            ),
            Some(NiriAction::MoveWorkspaceUp)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::PageDown
            ),
            Some(NiriAction::MoveWorkspaceDown)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::WheelScrollDown),
            Some(NiriAction::FocusWorkspaceDown)
        );
        assert_eq!(
            config
                .bindings
                .binding(BindingModifiers::MOD, BindingKey::WheelScrollDown)
                .unwrap()
                .1
                .cooldown_ms,
            Some(150)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::WheelScrollUp
            ),
            Some(NiriAction::MoveColumnToWorkspaceUp)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::WheelScrollDown
            ),
            Some(NiriAction::FocusColumnRight)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD
                    .with(BindingModifiers::CTRL)
                    .with(BindingModifiers::SHIFT),
                BindingKey::WheelScrollUp
            ),
            Some(NiriAction::MoveColumnLeft)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::ALT),
                BindingKey::Up
            ),
            Some(NiriAction::MoveWindowToWorkspaceUp)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::ALT),
                BindingKey::Down
            ),
            Some(NiriAction::MoveWindowToWorkspaceDown)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD
                    .with(BindingModifiers::CTRL)
                    .with(BindingModifiers::SHIFT),
                BindingKey::Character(b'2')
            ),
            Some(NiriAction::MoveWindowToWorkspace(
                WorkspaceReference::Index(2)
            ))
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD
                    .with(BindingModifiers::SHIFT)
                    .with(BindingModifiers::ALT),
                BindingKey::Character(b'M')
            ),
            Some(NiriAction::MoveWindowToWorkspace(WorkspaceReference::Name(
                "main"
            )))
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'1')),
            Some(NiriAction::FocusWorkspace(WorkspaceReference::Index(1)))
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'2')
            ),
            Some(NiriAction::MoveColumnToWorkspace(
                WorkspaceReference::Index(2)
            ))
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'R')),
            Some(NiriAction::SwitchPresetColumnWidth)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Character(b'R')
            ),
            Some(NiriAction::SwitchPresetColumnWidthBack)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'R')
            ),
            Some(NiriAction::ResetWindowHeight)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD
                    .with(BindingModifiers::CTRL)
                    .with(BindingModifiers::SHIFT),
                BindingKey::Character(b'R')
            ),
            Some(NiriAction::SwitchPresetWindowHeight)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD
                    .with(BindingModifiers::CTRL)
                    .with(BindingModifiers::ALT),
                BindingKey::Character(b'R')
            ),
            Some(NiriAction::SwitchPresetWindowHeightBack)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'F')),
            Some(NiriAction::MaximizeColumn)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Character(b'F')
            ),
            Some(NiriAction::FullscreenWindow)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'C')),
            Some(NiriAction::CenterColumn)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'M')),
            Some(NiriAction::MaximizeWindowToEdges)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'C')
            ),
            Some(NiriAction::CenterVisibleColumns)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'F')
            ),
            Some(NiriAction::ExpandColumnToAvailableWidth)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::ALT),
                BindingKey::Character(b'C')
            ),
            Some(NiriAction::FocusWorkspace(WorkspaceReference::Name(
                "config"
            )))
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD
                    .with(BindingModifiers::CTRL)
                    .with(BindingModifiers::ALT),
                BindingKey::Character(b'M')
            ),
            Some(NiriAction::MoveColumnToWorkspace(WorkspaceReference::Name(
                "main"
            )))
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Tab),
            Some(NiriAction::FocusWorkspacePrevious)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'K')),
            Some(NiriAction::FocusWindowUp)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'J')),
            Some(NiriAction::FocusWindowDown)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'K')
            ),
            Some(NiriAction::MoveWindowUp)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'J')
            ),
            Some(NiriAction::MoveWindowDown)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b',')),
            Some(NiriAction::ConsumeWindowIntoColumn)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'.')),
            Some(NiriAction::ExpelWindowFromColumn)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'[')),
            Some(NiriAction::ConsumeOrExpelWindowLeft)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b']')),
            Some(NiriAction::ConsumeOrExpelWindowRight)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'W')),
            Some(NiriAction::ToggleColumnTabbedDisplay)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'V')),
            Some(NiriAction::ToggleWindowFloating)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Character(b'V')
            ),
            Some(NiriAction::SwitchFocusBetweenFloatingAndTiling)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::ALT),
                BindingKey::Character(b'V')
            ),
            Some(NiriAction::MoveWindowToFloating)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'V')
            ),
            Some(NiriAction::MoveWindowToTiling)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::ALT),
                BindingKey::Character(b'G')
            ),
            Some(NiriAction::FocusFloating)
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::ALT),
                BindingKey::Character(b'T')
            ),
            Some(NiriAction::FocusTiling)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Minus),
            Some(NiriAction::SetColumnWidth(
                ColumnWidthChange::AdjustProportion(-100)
            ))
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Equal),
            Some(NiriAction::SetColumnWidth(ColumnWidthChange::Set(
                ColumnWidth::Fixed(640)
            )))
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Minus
            ),
            Some(NiriAction::SetWindowHeight(
                ColumnWidthChange::AdjustProportion(-100)
            ))
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Equal
            ),
            Some(NiriAction::SetWindowHeight(
                ColumnWidthChange::AdjustProportion(100)
            ))
        );
        assert_eq!(
            parse_column_width_change("50%"),
            Ok(ColumnWidthChange::Set(ColumnWidth::Proportion(500)))
        );
        assert_eq!(
            parse_column_width_change("+32"),
            Ok(ColumnWidthChange::AdjustFixed(32))
        );
        assert_eq!(
            parse_column_width_change("-32"),
            Ok(ColumnWidthChange::AdjustFixed(-32))
        );
        assert_eq!(
            config.window_rules.workspace_for("slopos-config"),
            Some("config")
        );
        assert_eq!(
            config.window_rules.floating_for("slopos-config"),
            Some(false)
        );
        assert_eq!(
            config.window_rules.maximized_for("slopos-terminal"),
            Some(true)
        );
        assert_eq!(
            config.window_rules.maximized_for("slopos-config"),
            Some(false)
        );
        assert_eq!(
            config.window_rules.column_width_for("slopos-terminal"),
            Some(ColumnWidth::Fixed(600))
        );
        assert_eq!(
            config.window_rules.column_width_for("slopos-config"),
            Some(ColumnWidth::Proportion(667))
        );
        assert_eq!(
            config.window_rules.window_height_for("slopos-terminal"),
            Some(ColumnWidth::Fixed(400))
        );
        assert_eq!(
            config.window_rules.window_height_for("slopos-config"),
            Some(ColumnWidth::Proportion(500))
        );
        assert_eq!(
            config.window_rules.column_display_for("slopos-terminal"),
            Some(ColumnDisplay::Normal)
        );
        assert_eq!(
            config.window_rules.column_display_for("slopos-config"),
            Some(ColumnDisplay::Tabbed)
        );
        assert_eq!(
            config
                .window_rules
                .maximized_to_edges_for("slopos-terminal"),
            Some(true)
        );
        assert_eq!(
            config.window_rules.maximized_to_edges_for("slopos-config"),
            Some(false)
        );
        assert_eq!(
            config.window_rules.fullscreen_for("slopos-terminal"),
            Some(true)
        );
        assert_eq!(
            config.window_rules.fullscreen_for("slopos-config"),
            Some(false)
        );
        assert_eq!(
            config.window_rules.focused_for("slopos-terminal"),
            Some(true)
        );
        assert_eq!(
            config.window_rules.focused_for("slopos-config"),
            Some(false)
        );
        assert_eq!(
            config.window_rules.focus_ring_for(
                "slopos-terminal",
                FocusRing {
                    enabled: true,
                    width: 3,
                    active_color: 0,
                    inactive_color: 0,
                }
            ),
            Some(FocusRing {
                enabled: false,
                width: 5,
                active_color: 0x11_22_33,
                inactive_color: 0,
            })
        );
        assert_eq!(
            config.window_rules.focus_ring_for(
                "slopos-config",
                FocusRing {
                    enabled: true,
                    width: 3,
                    active_color: 0,
                    inactive_color: 0,
                }
            ),
            Some(FocusRing {
                enabled: false,
                width: 7,
                active_color: 0x77_88_99,
                inactive_color: 0x44_55_66,
            })
        );
        assert_eq!(
            config.window_rules.floating_position_for("slopos-terminal"),
            Some(FloatingPosition {
                x: 10,
                y: 20,
                relative_to: FloatingPositionRelativeTo::TopLeft,
            })
        );
        assert_eq!(
            config.window_rules.floating_position_for("slopos-config"),
            Some(FloatingPosition {
                x: 33,
                y: 48,
                relative_to: FloatingPositionRelativeTo::BottomRight,
            })
        );
    }

    #[test]
    fn rejects_duplicate_names_chords_and_unsupported_actions() {
        assert_eq!(
            parse_niri_shell_config("workspace \"one\"\nworkspace \"one\"\n"),
            Err(NiriConfigError::DuplicateWorkspace)
        );
        assert_eq!(
            parse_niri_shell_config("binds { Mod+Q { close-window; } Mod+Q { close-window; } }"),
            Err(NiriConfigError::DuplicateBinding)
        );
        assert_eq!(
            parse_niri_shell_config("binds { Mod+Q { explode-window; } }"),
            Err(NiriConfigError::InvalidBinding)
        );
        assert_eq!(
            parse_niri_shell_config("window-rule { open-floating maybe; }"),
            Err(NiriConfigError::InvalidWindowRule)
        );
        assert_eq!(
            parse_niri_shell_config("window-rule { open-maximized maybe; }"),
            Err(NiriConfigError::InvalidWindowRule)
        );
        assert_eq!(
            parse_niri_shell_config("window-rule { open-maximized-to-edges maybe; }"),
            Err(NiriConfigError::InvalidWindowRule)
        );
        assert_eq!(
            parse_niri_shell_config("window-rule { open-fullscreen maybe; }"),
            Err(NiriConfigError::InvalidWindowRule)
        );
        assert_eq!(
            parse_niri_shell_config("window-rule { open-focused maybe; }"),
            Err(NiriConfigError::InvalidWindowRule)
        );
        for input in [
            r#"window-rule { default-column-display "stacked"; }"#,
            "window-rule { default-column-display tabbed; }",
            "window-rule { default-floating-position x=1; }",
            "window-rule { default-floating-position y=1; }",
            "window-rule { default-floating-position x=1 x=2 y=3; }",
            "window-rule { default-floating-position x=1 y=2 relative-to=top; }",
            r#"window-rule { default-floating-position x=1 y=2 relative-to="center"; }"#,
            "window-rule { default-floating-position x=65536 y=0; }",
            "window-rule { default-floating-position x=65535.1 y=0; }",
            "window-rule { default-floating-position x=1.2345 y=0; }",
            "window-rule { focus-ring { width -1; } }",
            "window-rule { focus-ring { width 65536; } }",
            "window-rule { focus-ring { active-color #ffffff; } }",
            r##"window-rule { focus-ring { inactive-color "#zzzzzz"; } }"##,
        ] {
            assert_eq!(
                parse_niri_shell_config(input),
                Err(NiriConfigError::InvalidWindowRule)
            );
        }
        for input in [
            "window-rule { default-column-width { proportion 1.5; } }",
            "window-rule { default-column-width { fixed nope; } }",
            "window-rule { default-column-width { fixed 200; fixed 300; } }",
            "window-rule { default-window-height { proportion 1.5; } }",
            "window-rule { default-window-height { fixed nope; } }",
            "window-rule { default-window-height { proportion 0.5; fixed 300; } }",
        ] {
            assert_eq!(
                parse_niri_shell_config(input),
                Err(NiriConfigError::InvalidWindowRule)
            );
        }
        assert_eq!(
            parse_niri_shell_config("window-rule { default-column-width {} }")
                .unwrap()
                .window_rules
                .column_width_for("anything"),
            Some(ColumnWidth::Client)
        );
        assert_eq!(
            parse_niri_shell_config("window-rule { default-window-height {} }")
                .unwrap()
                .window_rules
                .window_height_for("anything"),
            Some(ColumnWidth::Client)
        );
        assert!(
            parse_niri_shell_config(
                r#"binds { Mod+L { focus-workspace "later"; } } workspace "later""#
            )
            .is_ok()
        );
        for input in [
            r#"binds { Mod+Equal { set-column-width "0%"; } }"#,
            r#"binds { Mod+Equal { set-column-width "101%"; } }"#,
            r#"binds { Mod+Equal { set-column-width "+"; } }"#,
            r#"binds { Mod+Equal { set-column-width "-0"; } }"#,
            r#"binds { Mod+Equal { set-column-width "12.5%"; } }"#,
            r#"binds { Mod+Equal { set-column-width "70000"; } }"#,
            r#"binds { Mod+1 { focus-workspace 0; } }"#,
            r#"binds { Mod+1 { focus-workspace 256; } }"#,
            r#"binds { Mod+1 { focus-workspace ""; } }"#,
            r#"binds { Mod+1 { focus-workspace "missing"; } } workspace "main""#,
            r#"binds { Mod+1 { move-column-to-workspace; } }"#,
            r#"binds { Mod+1 { move-window-to-workspace "missing"; } } workspace "main""#,
            r#"binds { Mod+Q cooldown-ms=-1 { close-window; } }"#,
            r#"binds { Mod+Q cooldown-ms=1.5 { close-window; } }"#,
            r#"binds { Mod+Q cooldown-ms=65536 { close-window; } }"#,
            r#"binds { Mod+Q cooldown-ms 150 { close-window; } }"#,
            r#"binds { Mod+Q cooldown-ms=10 cooldown-ms=20 { close-window; } }"#,
        ] {
            assert_eq!(
                parse_niri_shell_config(input),
                Err(NiriConfigError::InvalidBinding)
            );
        }
    }

    #[test]
    fn moves_windows_between_tiling_and_floating_layers() {
        let mut workspaces =
            WorkspaceSet::<3, 3, 3>::new(2, 2, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 1).unwrap();
        workspaces.open_window(0, 2).unwrap();
        workspaces.focus_window(1).unwrap();

        assert!(workspaces.toggle_focused_window_floating());
        assert!(workspaces.focused_window_is_floating());
        assert!(workspaces.window_is_floating(1));
        assert_eq!(
            workspaces.tile_rect(1).unwrap(),
            Rect {
                x: 16,
                y: 150,
                width: 476,
                height: 440,
            }
        );
        assert_eq!(workspaces.tabbed_column_info(1), None);
        assert_eq!(workspaces.tile_rect(2).unwrap().x, 16);
        assert!(workspaces.switch_focus_between_floating_and_tiling());
        assert_eq!(workspaces.focused_window(), Some(2));
        assert!(!workspaces.focused_window_is_floating());
        assert!(!workspaces.focus_tiling());
        assert!(workspaces.focus_floating());
        assert!(!workspaces.focus_floating());
        assert_eq!(workspaces.focused_window(), Some(1));
        assert!(workspaces.focus_tiling());
        assert!(workspaces.switch_focus_between_floating_and_tiling());
        assert_eq!(workspaces.focused_window(), Some(1));

        assert!(workspaces.move_focused_floating(100, 20));
        assert_eq!(workspaces.tile_rect(1).unwrap().x, 116);
        assert_eq!(workspaces.tile_rect(1).unwrap().y, 170);
        assert!(
            workspaces
                .change_focused_column_width(ColumnWidthChange::AdjustFixed(50))
                .unwrap()
        );
        assert_eq!(workspaces.tile_rect(1).unwrap().width, 526);
        assert!(
            workspaces
                .change_focused_window_height(ColumnWidthChange::AdjustFixed(-40))
                .unwrap()
        );
        assert_eq!(workspaces.tile_rect(1).unwrap().height, 400);
        assert!(workspaces.reset_focused_window_height());
        assert_eq!(workspaces.tile_rect(1).unwrap().height, 440);

        assert!(workspaces.move_focused_column_to_workspace(1).unwrap());
        assert_eq!(workspaces.active(), 1);
        assert!(workspaces.focused_window_is_floating());
        assert!(workspaces.window_is_floating(1));
        assert!(workspaces.move_focused_window_to_tiling());
        assert!(!workspaces.move_focused_window_to_tiling());
        assert!(!workspaces.focused_window_is_floating());
        assert!(!workspaces.window_is_floating(1));
        assert_eq!(workspaces.tile_rect(1).unwrap().y, 56);

        assert!(workspaces.move_focused_window_to_floating());
        assert!(!workspaces.move_focused_window_to_floating());
        assert!(workspaces.toggle_focused_window_floating());
        assert!(!workspaces.window_is_floating(1));

        workspaces.open_floating_window(1, 3).unwrap();
        workspaces.focus_window(3).unwrap();
        assert_eq!(workspaces.floating_window_at_z(0), Some(3));
        assert!(workspaces.switch_focus_between_floating_and_tiling());
        assert_eq!(workspaces.focused_window(), Some(1));
        assert!(workspaces.switch_focus_between_floating_and_tiling());
        assert_eq!(workspaces.focused_window(), Some(3));

        let mut floating_only =
            WorkspaceSet::<2, 2, 2>::new(1, 1, 1000, 700, 40, LayoutConfig::default()).unwrap();
        floating_only.open_floating_window(0, 9).unwrap();
        assert!(floating_only.focused_window_is_floating());
        assert_eq!(floating_only.focused_window(), Some(9));
    }

    #[test]
    fn preserves_or_replaces_focus_when_opening_windows() {
        let mut workspaces =
            WorkspaceSet::<3, 6, 6>::new(2, 2, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 1).unwrap();
        workspaces
            .open_window_with_properties_and_focus(0, 2, None, None, None, false)
            .unwrap();
        assert_eq!(workspaces.focused_window(), Some(1));
        assert!(!workspaces.focused_window_is_floating());

        workspaces
            .open_floating_window_with_dimensions_and_focus(0, 3, None, None, false)
            .unwrap();
        assert_eq!(workspaces.focused_window(), Some(1));
        assert!(!workspaces.focused_window_is_floating());
        workspaces
            .open_floating_window_with_dimensions_and_focus(0, 4, None, None, true)
            .unwrap();
        assert_eq!(workspaces.focused_window(), Some(4));
        assert!(workspaces.focused_window_is_floating());

        workspaces
            .open_window_with_properties_and_focus(0, 5, None, None, None, false)
            .unwrap();
        assert_eq!(workspaces.focused_window(), Some(4));
        assert!(workspaces.focused_window_is_floating());
        workspaces
            .open_window_with_properties_and_focus(0, 6, None, None, None, true)
            .unwrap();
        assert_eq!(workspaces.focused_window(), Some(6));
        assert!(!workspaces.focused_window_is_floating());

        workspaces
            .open_window_with_properties_and_focus(1, 7, None, None, None, true)
            .unwrap();
        assert_eq!(workspaces.active(), 0);
        assert_eq!(workspaces.focused_window(), Some(6));
        assert!(workspaces.focus_window_without_workspace_switch(7));
        assert_eq!(workspaces.active(), 0);
        assert_eq!(workspaces.focused_window(), Some(6));
        assert!(!workspaces.focus_window_without_workspace_switch(99));
        assert!(workspaces.focus_workspace(1).unwrap());
        assert_eq!(workspaces.focused_window(), Some(7));
    }

    #[test]
    fn anchors_and_remembers_default_floating_positions() {
        let cases = [
            (FloatingPositionRelativeTo::TopLeft, 10, 60),
            (FloatingPositionRelativeTo::TopRight, 790, 60),
            (FloatingPositionRelativeTo::BottomLeft, 10, 560),
            (FloatingPositionRelativeTo::BottomRight, 790, 560),
            (FloatingPositionRelativeTo::Top, 410, 60),
            (FloatingPositionRelativeTo::Bottom, 410, 560),
            (FloatingPositionRelativeTo::Left, 10, 330),
            (FloatingPositionRelativeTo::Right, 790, 330),
        ];
        for (relative_to, expected_x, expected_y) in cases {
            let mut workspaces =
                WorkspaceSet::<1, 1, 1>::new(1, 1, 1000, 700, 40, LayoutConfig::default()).unwrap();
            workspaces
                .open_floating_window_with_properties(
                    0,
                    1,
                    Some(ColumnWidth::Fixed(200)),
                    Some(ColumnWidth::Fixed(120)),
                    Some(FloatingPosition {
                        x: 10,
                        y: 20,
                        relative_to,
                    }),
                    true,
                )
                .unwrap();
            assert_eq!(
                workspaces.tile_rect(1).unwrap(),
                Rect {
                    x: expected_x,
                    y: expected_y,
                    width: 200,
                    height: 120,
                }
            );
        }

        let mut workspaces =
            WorkspaceSet::<1, 2, 2>::new(1, 1, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 7).unwrap();
        let position = FloatingPosition {
            x: 30,
            y: 20,
            relative_to: FloatingPositionRelativeTo::BottomRight,
        };
        assert!(workspaces.move_focused_window_to_floating_with_position(Some(position)));
        let initial = workspaces.tile_rect(7).unwrap();
        assert_eq!(initial.x, 1000 - i32::from(initial.width) - 30);
        assert_eq!(initial.y, 700 - i32::from(initial.height) - 20);
        assert!(workspaces.move_focused_floating(0, 50));
        let moved = workspaces.tile_rect(7).unwrap();
        assert_ne!(moved.y, initial.y);
        assert!(workspaces.move_focused_window_to_tiling());
        assert!(workspaces.floating_position_is_remembered(7));
        assert!(
            workspaces.move_focused_window_to_floating_with_position(Some(FloatingPosition {
                x: 0,
                y: 0,
                relative_to: FloatingPositionRelativeTo::TopLeft,
            }))
        );
        assert_eq!(workspaces.tile_rect(7).unwrap(), moved);
        assert!(!workspaces.floating_position_is_remembered(7));
    }

    #[test]
    fn applies_rule_dimensions_to_tiled_and_floating_windows() {
        let mut workspaces =
            WorkspaceSet::<2, 2, 2>::new(1, 1, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces
            .open_window_with_properties(
                0,
                1,
                Some(ColumnWidth::Proportion(667)),
                Some(ColumnWidth::Proportion(500)),
                Some(ColumnDisplay::Tabbed),
            )
            .unwrap();
        assert_eq!(
            workspaces.tile_rect(1).unwrap(),
            Rect {
                x: 16,
                y: 56,
                width: 640,
                height: 306,
            }
        );
        assert_eq!(
            workspaces.tabbed_column_info(1),
            Some(TabbedColumnInfo {
                active_tab: 0,
                tab_count: 1,
            })
        );

        workspaces
            .open_floating_window_with_dimensions(
                0,
                2,
                Some(ColumnWidth::Fixed(420)),
                Some(ColumnWidth::Fixed(270)),
            )
            .unwrap();
        workspaces.focus_window(2).unwrap();
        assert_eq!(
            workspaces.tile_rect(2).unwrap(),
            Rect {
                x: 290,
                y: 235,
                width: 420,
                height: 270,
            }
        );
        assert!(
            workspaces
                .change_focused_window_height(ColumnWidthChange::AdjustFixed(-40))
                .unwrap()
        );
        assert_eq!(workspaces.tile_rect(2).unwrap().height, 230);
        assert!(workspaces.reset_focused_window_height());
        assert_eq!(workspaces.tile_rect(2).unwrap().height, 270);
        assert!(workspaces.move_focused_window_to_tiling_with_display(Some(ColumnDisplay::Tabbed)));
        assert_eq!(
            workspaces.tabbed_column_info(2),
            Some(TabbedColumnInfo {
                active_tab: 0,
                tab_count: 1,
            })
        );
    }

    #[test]
    fn switches_workspaces_and_moves_the_focused_column() {
        let mut workspaces =
            WorkspaceSet::<4, 3, 1>::new(3, 2, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 10).unwrap();
        workspaces.open_window(0, 20).unwrap();
        workspaces.focus_window(10).unwrap();
        assert!(
            workspaces
                .change_focused_column_width(ColumnWidthChange::AdjustProportion(100))
                .unwrap()
        );
        assert_eq!(workspaces.tile_rect(10).unwrap().width, 574);
        assert_eq!(workspaces.focused_window(), Some(10));
        assert!(workspaces.move_focused_column_to_workspace(1).unwrap());
        assert_eq!(workspaces.active(), 1);
        assert_eq!(workspaces.previous(), 0);
        assert_eq!(workspaces.focused_window(), Some(10));
        assert!(workspaces.tile_rect(10).is_ok());
        assert!(workspaces.focus_workspace_previous());
        assert_eq!(workspaces.active(), 0);
        assert_eq!(workspaces.previous(), 1);
        assert!(workspaces.focus_workspace_previous());
        assert_eq!(workspaces.active(), 1);
        assert!(workspaces.focus_workspace_up());
        assert_eq!(workspaces.active(), 0);
        assert_eq!(workspaces.focused_window(), Some(20));
        assert!(workspaces.tile_rect(10).is_err());
        assert!(workspaces.focus_workspace_down());
        assert!(workspaces.focus_workspace_down());
        assert!(!workspaces.focus_workspace_down());
        assert_eq!(workspaces.focused_window(), None);

        workspaces.focus_workspace(1).unwrap();
        assert!(workspaces.move_focused_column_to_workspace(2).unwrap());
        assert!(workspaces.normalize_dynamic(2).unwrap());
        assert_eq!(workspaces.len(), 4);
        assert_eq!(workspaces.active(), 2);
        assert!(workspaces.workspace_is_empty(3).unwrap());
        assert!(workspaces.move_focused_column_to_workspace(1).unwrap());
        assert!(workspaces.normalize_dynamic(2).unwrap());
        assert_eq!(workspaces.len(), 3);
        assert_eq!(workspaces.active(), 1);
        assert!(workspaces.workspace_is_empty(2).unwrap());
    }

    #[test]
    fn reorders_complete_named_and_dynamic_workspaces() {
        let mut workspaces =
            WorkspaceSet::<4, 3, 3>::new(3, 2, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 10).unwrap();
        workspaces.open_floating_window(0, 11).unwrap();
        workspaces.open_window(1, 20).unwrap();
        workspaces.focus_window(10).unwrap();
        assert!(workspaces.toggle_focused_window_fullscreen());

        assert!(workspaces.move_workspace_down());
        assert_eq!(workspaces.active(), 1);
        assert_eq!(workspaces.workspace_identity(1).unwrap(), Some(0));
        assert_eq!(workspaces.workspace_for_identity(0), Some(1));
        assert_eq!(workspaces.workspace_for_identity(1), Some(0));
        assert_eq!(workspaces.focused_window(), Some(10));
        assert_eq!(workspaces.fullscreen_window(), Some(10));
        assert!(workspaces.window_is_floating(11));
        assert!(workspaces.tile_rect(20).is_err());
        assert!(!workspaces.focus_workspace_previous());

        assert!(workspaces.move_workspace_up());
        assert_eq!(workspaces.active(), 0);
        assert_eq!(workspaces.workspace_identity(0).unwrap(), Some(0));
        assert!(workspaces.toggle_focused_window_fullscreen());
        workspaces.focus_window(11).unwrap();
        assert!(workspaces.focused_window_is_floating());

        workspaces.open_window(2, 30).unwrap();
        assert!(workspaces.focus_workspace(2).unwrap());
        assert!(workspaces.normalize_dynamic(2).unwrap());
        assert_eq!(workspaces.len(), 4);
        assert_eq!(workspaces.workspace_identity(2).unwrap(), None);
        assert!(workspaces.move_workspace_up());
        assert_eq!(workspaces.active(), 1);
        assert_eq!(workspaces.workspace_identity(1).unwrap(), None);
        assert_eq!(workspaces.workspace_for_identity(1), Some(2));
        assert_eq!(workspaces.focused_window(), Some(30));
        assert!(workspaces.move_workspace_down());
        assert_eq!(workspaces.active(), 2);
        assert!(!workspaces.move_workspace_down());
        assert!(workspaces.workspace_is_empty(3).unwrap());
    }

    #[test]
    fn distinguishes_window_and_column_workspace_transfers() {
        let mut workspaces =
            WorkspaceSet::<3, 3, 3>::new(2, 2, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 10).unwrap();
        workspaces.open_window(0, 20).unwrap();
        workspaces.focus_window(10).unwrap();
        assert!(workspaces.consume_window_into_column());
        assert_eq!(workspaces.focused_window(), Some(20));

        assert!(
            workspaces
                .move_focused_window_to_workspace_with_display(1, Some(ColumnDisplay::Tabbed))
                .unwrap()
        );
        assert_eq!(workspaces.active(), 1);
        assert_eq!(workspaces.focused_window(), Some(20));
        assert_eq!(workspaces.tabbed_column_info(20).unwrap().tab_count, 1);
        assert!(workspaces.focus_workspace_previous());
        assert_eq!(workspaces.focused_window(), Some(10));
        assert!(workspaces.tile_rect(20).is_err());

        assert!(workspaces.focus_workspace_previous());
        assert!(workspaces.move_focused_window_to_workspace(0).unwrap());
        assert_eq!(workspaces.active(), 0);
        assert!(workspaces.focus_column_left());
        assert!(workspaces.consume_window_into_column());
        assert_eq!(workspaces.focused_window(), Some(20));

        assert!(workspaces.move_focused_column_to_workspace(1).unwrap());
        assert_eq!(workspaces.active(), 1);
        assert_eq!(workspaces.focused_window(), Some(20));
        assert_eq!(
            workspaces.tile_rect(10).unwrap().x,
            workspaces.tile_rect(20).unwrap().x
        );
        assert!(workspaces.focus_workspace_previous());
        assert!(workspaces.workspace_is_empty(0).unwrap());
    }

    #[test]
    fn fullscreen_hides_siblings_and_restores_tiled_or_floating_geometry() {
        let mut workspaces =
            WorkspaceSet::<3, 3, 3>::new(2, 2, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 10).unwrap();
        workspaces.open_window(0, 20).unwrap();
        workspaces.focus_window(10).unwrap();
        let tiled = workspaces.tile_rect(10).unwrap();

        assert!(workspaces.set_window_fullscreen(0, 10, true).unwrap());
        assert!(!workspaces.set_window_fullscreen(0, 10, true).unwrap());
        assert_eq!(workspaces.fullscreen_window(), Some(10));
        assert_eq!(
            workspaces.tile_rect(10).unwrap(),
            Rect {
                x: 0,
                y: 0,
                width: 1000,
                height: 700,
            }
        );
        assert!(!workspaces.window_is_visible(20));
        assert!(workspaces.tabbed_column_info(10).is_none());
        assert!(workspaces.focus_workspace(1).unwrap());
        assert_eq!(workspaces.fullscreen_window(), None);
        assert!(workspaces.focus_workspace_previous());
        assert_eq!(workspaces.fullscreen_window(), Some(10));
        assert!(workspaces.set_window_fullscreen(0, 10, false).unwrap());
        assert!(!workspaces.set_window_fullscreen(0, 10, false).unwrap());
        assert_eq!(workspaces.fullscreen_window(), None);
        assert_eq!(workspaces.tile_rect(10).unwrap(), tiled);
        assert!(workspaces.window_is_visible(20));

        assert!(workspaces.move_focused_window_to_floating());
        let floating = workspaces.tile_rect(10).unwrap();
        assert!(workspaces.window_is_floating(10));
        assert!(workspaces.set_window_fullscreen(0, 10, true).unwrap());
        assert_eq!(workspaces.tile_rect(10).unwrap().x, 0);
        assert_eq!(workspaces.tile_rect(10).unwrap().height, 700);
        assert!(workspaces.set_window_fullscreen(0, 10, false).unwrap());
        assert!(workspaces.window_is_floating(10));
        assert_eq!(workspaces.tile_rect(10).unwrap(), floating);
        assert_eq!(
            workspaces.set_window_fullscreen(0, 99, true),
            Err(WorkspaceError::Layout(LayoutError::UnknownWindow))
        );
        assert_eq!(
            workspaces.set_window_fullscreen(2, 10, true),
            Err(WorkspaceError::InvalidWorkspace)
        );
    }
}
