// SPDX-License-Identifier: 0BSD

use crate::{ColumnWidth, ColumnWidthChange, LayoutConfig, LayoutError, Rect, ScrollLayout};

pub const MAX_NIRI_WORKSPACES: usize = 8;
pub const MAX_NIRI_BINDINGS: usize = 24;
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
    Return,
    Tab,
    Escape,
    Minus,
    Equal,
    Character(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NiriAction {
    FocusColumnLeft,
    FocusColumnRight,
    MoveColumnLeft,
    MoveColumnRight,
    FocusWorkspaceUp,
    FocusWorkspaceDown,
    FocusWorkspace(u8),
    MoveColumnToWorkspaceUp,
    MoveColumnToWorkspaceDown,
    MoveColumnToWorkspace(u8),
    SetColumnWidth(ColumnWidthChange),
    CloseWindow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriBinding {
    pub modifiers: BindingModifiers,
    pub key: BindingKey,
    pub action: NiriAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NiriBindingList {
    entries: [Option<NiriBinding>; MAX_NIRI_BINDINGS],
    length: usize,
}

impl NiriBindingList {
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

    pub fn action(self, modifiers: BindingModifiers, key: BindingKey) -> Option<NiriAction> {
        self.entries[..self.length]
            .iter()
            .flatten()
            .find(|binding| binding.modifiers == modifiers && binding.key == key)
            .map(|binding| binding.action)
    }

    fn push(&mut self, binding: NiriBinding) -> Result<(), NiriConfigError> {
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
pub struct NiriWindowRule<'a> {
    pub app_id: Option<&'a str>,
    pub open_on_workspace: Option<&'a str>,
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
    pub bindings: NiriBindingList,
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
                KdlToken::End => return Ok(config),
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

    fn parse_bindings(&mut self, bindings: &mut NiriBindingList) -> Result<(), NiriConfigError> {
        loop {
            match self.next_non_end() {
                KdlToken::RightBrace => return Ok(()),
                KdlToken::Word(hotkey) => {
                    let (modifiers, key) = parse_hotkey(hotkey)?;
                    loop {
                        match self.next() {
                            KdlToken::LeftBrace => break,
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

    fn parse_action(&mut self, value: &str) -> Result<NiriAction, NiriConfigError> {
        Ok(match value {
            "focus-column-left" => NiriAction::FocusColumnLeft,
            "focus-column-right" => NiriAction::FocusColumnRight,
            "move-column-left" => NiriAction::MoveColumnLeft,
            "move-column-right" => NiriAction::MoveColumnRight,
            "focus-workspace-up" => NiriAction::FocusWorkspaceUp,
            "focus-workspace-down" => NiriAction::FocusWorkspaceDown,
            "focus-workspace" => NiriAction::FocusWorkspace(self.parse_workspace_index()?),
            "move-column-to-workspace-up" => NiriAction::MoveColumnToWorkspaceUp,
            "move-column-to-workspace-down" => NiriAction::MoveColumnToWorkspaceDown,
            "move-column-to-workspace" => {
                NiriAction::MoveColumnToWorkspace(self.parse_workspace_index()?)
            }
            "set-column-width" => {
                let KdlToken::String(width) = self.next() else {
                    return Err(NiriConfigError::InvalidBinding);
                };
                NiriAction::SetColumnWidth(parse_column_width_change(width)?)
            }
            "close-window" => NiriAction::CloseWindow,
            _ => return Err(NiriConfigError::InvalidBinding),
        })
    }

    fn parse_workspace_index(&mut self) -> Result<u8, NiriConfigError> {
        let KdlToken::Word(index) = self.next() else {
            return Err(NiriConfigError::InvalidBinding);
        };
        u8::try_from(parse_decimal_u16(index)?)
            .ok()
            .filter(|index| *index != 0)
            .ok_or(NiriConfigError::InvalidBinding)
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
        "Return" => Ok(BindingKey::Return),
        "Tab" => Ok(BindingKey::Tab),
        "Escape" => Ok(BindingKey::Escape),
        "Minus" => Ok(BindingKey::Minus),
        "Equal" => Ok(BindingKey::Equal),
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

pub struct WorkspaceSet<const WORKSPACES: usize, const COLUMNS: usize, const WINDOWS: usize> {
    layouts: [ScrollLayout<COLUMNS, WINDOWS>; WORKSPACES],
    count: usize,
    active: usize,
}

impl<const WORKSPACES: usize, const COLUMNS: usize, const WINDOWS: usize>
    WorkspaceSet<WORKSPACES, COLUMNS, WINDOWS>
{
    pub fn new(
        count: usize,
        output_width: u16,
        output_height: u16,
        reserved_top: u16,
        config: LayoutConfig,
    ) -> Result<Self, WorkspaceError> {
        if count == 0 || count > WORKSPACES {
            return Err(WorkspaceError::InvalidCount);
        }
        Ok(Self {
            layouts: core::array::from_fn(|_| {
                ScrollLayout::new(output_width, output_height, reserved_top, config)
            }),
            count,
            active: 0,
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

    pub fn config(&self) -> LayoutConfig {
        self.layouts[self.active].config()
    }

    pub fn open_window(&mut self, workspace: usize, window: u32) -> Result<(), WorkspaceError> {
        self.layouts
            .get_mut(workspace)
            .filter(|_| workspace < self.count)
            .ok_or(WorkspaceError::InvalidWorkspace)?
            .open_window(window)
            .map_err(WorkspaceError::Layout)
    }

    pub fn focus_window(&mut self, window: u32) -> Result<(), WorkspaceError> {
        self.layouts[self.active]
            .focus_window(window)
            .map_err(WorkspaceError::Layout)
    }

    pub fn close_window(&mut self, window: u32) -> Result<(), WorkspaceError> {
        self.layouts[self.active]
            .close_window(window)
            .map_err(WorkspaceError::Layout)
    }

    pub fn tile_rect(&self, window: u32) -> Result<Rect, WorkspaceError> {
        self.layouts[self.active]
            .tile_rect(window)
            .map_err(WorkspaceError::Layout)
    }

    pub fn focused_window(&self) -> Option<u32> {
        self.layouts[self.active].focused_window()
    }

    pub fn focus_column_left(&mut self) -> bool {
        self.layouts[self.active].focus_column_left()
    }

    pub fn focus_column_right(&mut self) -> bool {
        self.layouts[self.active].focus_column_right()
    }

    pub fn move_column_left(&mut self) -> bool {
        self.layouts[self.active].move_column_left()
    }

    pub fn move_column_right(&mut self) -> bool {
        self.layouts[self.active].move_column_right()
    }

    pub fn scroll_by(&mut self, delta: i32) {
        self.layouts[self.active].scroll_by(delta);
    }

    pub fn change_focused_column_width(
        &mut self,
        change: ColumnWidthChange,
    ) -> Result<bool, WorkspaceError> {
        self.layouts[self.active]
            .change_focused_column_width(change)
            .map_err(WorkspaceError::Layout)
    }

    pub fn view_offset(&self) -> i32 {
        self.layouts[self.active].view_offset()
    }

    pub fn focus_workspace_up(&mut self) -> bool {
        if self.active == 0 {
            return false;
        }
        self.active -= 1;
        true
    }

    pub fn focus_workspace_down(&mut self) -> bool {
        if self.active + 1 >= self.count {
            return false;
        }
        self.active += 1;
        true
    }

    pub fn focus_workspace(&mut self, workspace: usize) -> Result<bool, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        let changed = workspace != self.active;
        self.active = workspace;
        Ok(changed)
    }

    pub fn move_focused_to_workspace(&mut self, workspace: usize) -> Result<bool, WorkspaceError> {
        if workspace >= self.count {
            return Err(WorkspaceError::InvalidWorkspace);
        }
        if workspace == self.active {
            return Ok(false);
        }
        let Some(window) = self.layouts[self.active].focused_window() else {
            return Ok(false);
        };
        self.layouts[workspace]
            .open_window(window)
            .map_err(WorkspaceError::Layout)?;
        self.layouts[self.active]
            .close_window(window)
            .map_err(WorkspaceError::Layout)?;
        self.active = workspace;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_workspaces_bindings_and_ordered_window_rules() {
        let config = parse_niri_shell_config(
            r#"
            workspace "main"
            workspace "config" { open-on-output "SLOPOS-1"; }
            binds {
                Mod+Left { focus-column-left; }
                Mod+Shift+Left { move-column-left; }
                Mod+Shift+Right { move-column-right; }
                Mod+Shift+Down repeat=false { move-column-to-workspace-down; }
                Mod+1 { focus-workspace 1; }
                Mod+Ctrl+2 { move-column-to-workspace 2; }
                Mod+Minus { set-column-width "-10%"; }
                Mod+Equal { set-column-width "640"; }
                Mod+Q { close-window; }
            }
            window-rule {
                match app-id="slopos-config"
                open-on-workspace "main"
            }
            window-rule {
                match app-id="slopos-config"
                open-on-workspace "config"
            }
            "#,
        )
        .unwrap();
        assert_eq!(config.workspaces.len(), 2);
        assert_eq!(config.workspaces.get(1).unwrap().name, "config");
        assert_eq!(
            config.workspaces.get(1).unwrap().open_on_output,
            Some("SLOPOS-1")
        );
        assert_eq!(config.bindings.len(), 9);
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Left),
            Some(NiriAction::FocusColumnLeft)
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
                BindingModifiers::MOD.with(BindingModifiers::SHIFT),
                BindingKey::Down
            ),
            Some(NiriAction::MoveColumnToWorkspaceDown)
        );
        assert_eq!(
            config
                .bindings
                .action(BindingModifiers::MOD, BindingKey::Character(b'1')),
            Some(NiriAction::FocusWorkspace(1))
        );
        assert_eq!(
            config.bindings.action(
                BindingModifiers::MOD.with(BindingModifiers::CTRL),
                BindingKey::Character(b'2')
            ),
            Some(NiriAction::MoveColumnToWorkspace(2))
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
        for input in [
            r#"binds { Mod+Equal { set-column-width "0%"; } }"#,
            r#"binds { Mod+Equal { set-column-width "101%"; } }"#,
            r#"binds { Mod+Equal { set-column-width "+"; } }"#,
            r#"binds { Mod+Equal { set-column-width "-0"; } }"#,
            r#"binds { Mod+Equal { set-column-width "12.5%"; } }"#,
            r#"binds { Mod+Equal { set-column-width "70000"; } }"#,
            r#"binds { Mod+1 { focus-workspace 0; } }"#,
            r#"binds { Mod+1 { focus-workspace 256; } }"#,
            r#"binds { Mod+1 { focus-workspace "main"; } }"#,
            r#"binds { Mod+1 { move-column-to-workspace; } }"#,
        ] {
            assert_eq!(
                parse_niri_shell_config(input),
                Err(NiriConfigError::InvalidBinding)
            );
        }
    }

    #[test]
    fn switches_workspaces_and_moves_the_focused_column() {
        let mut workspaces =
            WorkspaceSet::<3, 3, 1>::new(3, 1000, 700, 40, LayoutConfig::default()).unwrap();
        workspaces.open_window(0, 10).unwrap();
        workspaces.open_window(0, 20).unwrap();
        workspaces.focus_window(10).unwrap();
        assert!(
            workspaces
                .change_focused_column_width(ColumnWidthChange::AdjustProportion(100))
                .unwrap()
        );
        assert_eq!(workspaces.tile_rect(10).unwrap().width, 600);
        assert_eq!(workspaces.focused_window(), Some(10));
        assert!(workspaces.move_focused_to_workspace(1).unwrap());
        assert_eq!(workspaces.active(), 1);
        assert_eq!(workspaces.focused_window(), Some(10));
        assert!(workspaces.tile_rect(10).is_ok());
        assert!(workspaces.focus_workspace_up());
        assert_eq!(workspaces.active(), 0);
        assert_eq!(workspaces.focused_window(), Some(20));
        assert!(workspaces.tile_rect(10).is_err());
        assert!(workspaces.focus_workspace_down());
        assert!(workspaces.focus_workspace_down());
        assert!(!workspaces.focus_workspace_down());
        assert_eq!(workspaces.focused_window(), None);
    }
}
