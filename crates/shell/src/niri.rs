// SPDX-License-Identifier: 0BSD

use crate::{ColumnWidth, ColumnWidthChange, LayoutConfig, LayoutError, Rect, ScrollLayout};

pub const MAX_NIRI_WORKSPACES: usize = 8;
pub const MAX_NIRI_BINDINGS: usize = 64;
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
pub enum WorkspaceReference<'a> {
    Index(u8),
    Name(&'a str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NiriAction<'a> {
    FocusColumnLeft,
    FocusColumnRight,
    FocusWindowUp,
    FocusWindowDown,
    MoveColumnLeft,
    MoveColumnRight,
    MoveWindowUp,
    MoveWindowDown,
    FocusWorkspaceUp,
    FocusWorkspaceDown,
    FocusWorkspacePrevious,
    FocusWorkspace(WorkspaceReference<'a>),
    MoveColumnToWorkspaceUp,
    MoveColumnToWorkspaceDown,
    MoveColumnToWorkspace(WorkspaceReference<'a>),
    ConsumeWindowIntoColumn,
    ExpelWindowFromColumn,
    ConsumeOrExpelWindowLeft,
    ConsumeOrExpelWindowRight,
    SwitchPresetColumnWidth,
    SwitchPresetColumnWidthBack,
    SwitchPresetWindowHeight,
    SwitchPresetWindowHeightBack,
    MaximizeColumn,
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
        self.entries[..self.length]
            .iter()
            .flatten()
            .find(|binding| binding.modifiers == modifiers && binding.key == key)
            .map(|binding| binding.action)
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
                | NiriAction::MoveColumnToWorkspace(reference) => reference,
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

    fn parse_action(&mut self, value: &str) -> Result<NiriAction<'a>, NiriConfigError> {
        Ok(match value {
            "focus-column-left" => NiriAction::FocusColumnLeft,
            "focus-column-right" => NiriAction::FocusColumnRight,
            "focus-window-up" => NiriAction::FocusWindowUp,
            "focus-window-down" => NiriAction::FocusWindowDown,
            "move-column-left" => NiriAction::MoveColumnLeft,
            "move-column-right" => NiriAction::MoveColumnRight,
            "move-window-up" => NiriAction::MoveWindowUp,
            "move-window-down" => NiriAction::MoveWindowDown,
            "focus-workspace-up" => NiriAction::FocusWorkspaceUp,
            "focus-workspace-down" => NiriAction::FocusWorkspaceDown,
            "focus-workspace-previous" => NiriAction::FocusWorkspacePrevious,
            "focus-workspace" => NiriAction::FocusWorkspace(self.parse_workspace_reference()?),
            "move-column-to-workspace-up" => NiriAction::MoveColumnToWorkspaceUp,
            "move-column-to-workspace-down" => NiriAction::MoveColumnToWorkspaceDown,
            "move-column-to-workspace" => {
                NiriAction::MoveColumnToWorkspace(self.parse_workspace_reference()?)
            }
            "consume-window-into-column" => NiriAction::ConsumeWindowIntoColumn,
            "expel-window-from-column" => NiriAction::ExpelWindowFromColumn,
            "consume-or-expel-window-left" => NiriAction::ConsumeOrExpelWindowLeft,
            "consume-or-expel-window-right" => NiriAction::ConsumeOrExpelWindowRight,
            "switch-preset-column-width" => NiriAction::SwitchPresetColumnWidth,
            "switch-preset-column-width-back" => NiriAction::SwitchPresetColumnWidthBack,
            "switch-preset-window-height" => NiriAction::SwitchPresetWindowHeight,
            "switch-preset-window-height-back" => NiriAction::SwitchPresetWindowHeightBack,
            "maximize-column" => NiriAction::MaximizeColumn,
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
    previous: usize,
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

    pub fn workspace_is_empty(&self, workspace: usize) -> Result<bool, WorkspaceError> {
        self.layouts
            .get(workspace)
            .filter(|_| workspace < self.count)
            .map(ScrollLayout::is_empty)
            .ok_or(WorkspaceError::InvalidWorkspace)
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

    pub fn focus_window_up(&mut self) -> bool {
        self.layouts[self.active].focus_window_up()
    }

    pub fn focus_window_down(&mut self) -> bool {
        self.layouts[self.active].focus_window_down()
    }

    pub fn move_column_left(&mut self) -> bool {
        self.layouts[self.active].move_column_left()
    }

    pub fn move_column_right(&mut self) -> bool {
        self.layouts[self.active].move_column_right()
    }

    pub fn move_window_up(&mut self) -> bool {
        self.layouts[self.active].move_window_up()
    }

    pub fn move_window_down(&mut self) -> bool {
        self.layouts[self.active].move_window_down()
    }

    pub fn consume_window_into_column(&mut self) -> bool {
        self.layouts[self.active].consume_window_into_column()
    }

    pub fn expel_window_from_column(&mut self) -> bool {
        self.layouts[self.active].expel_window_from_column()
    }

    pub fn consume_or_expel_focused_window_left(&mut self) -> bool {
        self.layouts[self.active].consume_or_expel_focused_window_left()
    }

    pub fn consume_or_expel_focused_window_right(&mut self) -> bool {
        self.layouts[self.active].consume_or_expel_focused_window_right()
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

    pub fn change_focused_window_height(
        &mut self,
        change: ColumnWidthChange,
    ) -> Result<bool, WorkspaceError> {
        self.layouts[self.active]
            .change_focused_window_height(change)
            .map_err(WorkspaceError::Layout)
    }

    pub fn reset_focused_window_height(&mut self) -> bool {
        self.layouts[self.active].reset_focused_window_height()
    }

    pub fn switch_preset_column_width(&mut self) -> bool {
        self.layouts[self.active].switch_preset_column_width()
    }

    pub fn switch_preset_column_width_back(&mut self) -> bool {
        self.layouts[self.active].switch_preset_column_width_back()
    }

    pub fn switch_preset_window_height(&mut self) -> bool {
        self.layouts[self.active].switch_preset_window_height()
    }

    pub fn switch_preset_window_height_back(&mut self) -> bool {
        self.layouts[self.active].switch_preset_window_height_back()
    }

    pub fn maximize_focused_column(&mut self) -> bool {
        self.layouts[self.active].toggle_maximize_focused_column()
    }

    pub fn maximize_focused_window_to_edges(&mut self) -> bool {
        self.layouts[self.active].toggle_maximize_focused_window_to_edges()
    }

    pub fn center_focused_column(&mut self) -> bool {
        self.layouts[self.active].center_focused_column()
    }

    pub fn center_visible_columns(&mut self) -> bool {
        self.layouts[self.active].center_visible_columns()
    }

    pub fn expand_focused_column_to_available_width(&mut self) -> bool {
        self.layouts[self.active].expand_focused_column_to_available_width()
    }

    pub fn view_offset(&self) -> i32 {
        self.layouts[self.active].view_offset()
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
        self.previous = self.active;
        self.active = workspace;
        Ok(true)
    }

    pub fn normalize_dynamic(&mut self, persistent: usize) -> Result<bool, WorkspaceError> {
        if persistent >= self.count {
            return Err(WorkspaceError::InvalidCount);
        }
        let mut changed = false;
        let mut workspace = persistent;
        while workspace + 1 < self.count {
            if self.layouts[workspace].is_empty() && self.active != workspace {
                for index in workspace..self.count - 1 {
                    self.layouts.swap(index, index + 1);
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
        if !self.layouts[self.count - 1].is_empty() && self.count < WORKSPACES {
            if !self.layouts[self.count].is_empty() {
                return Err(WorkspaceError::InvalidCount);
            }
            self.count += 1;
            changed = true;
        }
        Ok(changed)
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
                Mod+Alt+C { focus-workspace "config"; }
                Mod+Ctrl+Alt+M { move-column-to-workspace "main"; }
                Mod+Tab { focus-workspace-previous; }
                Mod+K { focus-window-up; }
                Mod+J { focus-window-down; }
                Mod+Ctrl+K { move-window-up; }
                Mod+Ctrl+J { move-window-down; }
                Mod+Comma { consume-window-into-column; }
                Mod+Period { expel-window-from-column; }
                Mod+BracketLeft { consume-or-expel-window-left; }
                Mod+BracketRight { consume-or-expel-window-right; }
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
                Mod+M { maximize-window-to-edges; }
                Mod+C { center-column; }
                Mod+Ctrl+C { center-visible-columns; }
                Mod+Ctrl+F { expand-column-to-available-width; }
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
        assert_eq!(config.bindings.len(), 32);
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
            WorkspaceSet::<4, 3, 1>::new(3, 1000, 700, 40, LayoutConfig::default()).unwrap();
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
        assert!(workspaces.move_focused_to_workspace(1).unwrap());
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
        assert!(workspaces.move_focused_to_workspace(2).unwrap());
        assert!(workspaces.normalize_dynamic(2).unwrap());
        assert_eq!(workspaces.len(), 4);
        assert_eq!(workspaces.active(), 2);
        assert!(workspaces.workspace_is_empty(3).unwrap());
        assert!(workspaces.move_focused_to_workspace(1).unwrap());
        assert!(workspaces.normalize_dynamic(2).unwrap());
        assert_eq!(workspaces.len(), 3);
        assert_eq!(workspaces.active(), 1);
        assert!(workspaces.workspace_is_empty(2).unwrap());
    }
}
