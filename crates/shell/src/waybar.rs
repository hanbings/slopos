// SPDX-License-Identifier: 0BSD

pub const MAX_BAR_MODULES: usize = 16;
pub const MAX_BAR_MODULE_CONFIGS: usize = 24;
pub const MAX_BAR_TEXT: usize = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarPosition {
    Top,
    Bottom,
    Left,
    Right,
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
    pub position: BarPosition,
    pub height: u16,
    pub spacing: u16,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
    pub fixed_center: bool,
    pub exclusive: bool,
    pub modules_left: BarModuleList<'a>,
    pub modules_center: BarModuleList<'a>,
    pub modules_right: BarModuleList<'a>,
    pub module_configs: BarModuleConfigList<'a>,
}

impl Default for WaybarConfig<'_> {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            height: 30,
            spacing: 4,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            fixed_center: true,
            exclusive: true,
            modules_left: BarModuleList::empty(),
            modules_center: BarModuleList::empty(),
            modules_right: BarModuleList::empty(),
            module_configs: BarModuleConfigList::empty(),
        }
    }
}

impl WaybarConfig<'_> {
    pub fn reserved_top(self) -> u16 {
        if !self.exclusive || self.position != BarPosition::Top {
            return 0;
        }
        i32::from(self.height)
            .saturating_add(self.margin_top)
            .clamp(0, i32::from(u16::MAX)) as u16
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
    const POSITION: u16 = 1 << 0;
    const HEIGHT: u16 = 1 << 1;
    const SPACING: u16 = 1 << 2;
    const LEFT: u16 = 1 << 3;
    const CENTER: u16 = 1 << 4;
    const RIGHT: u16 = 1 << 5;
    const MARGIN: u16 = 1 << 6;
    const MARGIN_TOP: u16 = 1 << 7;
    const MARGIN_RIGHT: u16 = 1 << 8;
    const MARGIN_BOTTOM: u16 = 1 << 9;
    const MARGIN_LEFT: u16 = 1 << 10;
    const FIXED_CENTER: u16 = 1 << 11;
    const EXCLUSIVE: u16 = 1 << 12;

    const fn new(input: &'a str) -> Self {
        Self {
            lexer: JsonLexer::new(input),
            pushed: None,
        }
    }

    fn parse(mut self) -> Result<WaybarConfig<'a>, BarConfigError> {
        self.expect(Token::LeftBrace)?;
        let mut config = WaybarConfig::default();
        let mut fields = 0u16;
        let mut margin = None;
        let mut margin_top = None;
        let mut margin_right = None;
        let mut margin_bottom = None;
        let mut margin_left = None;
        loop {
            match self.next() {
                Token::RightBrace => break,
                Token::String(name) => {
                    self.expect(Token::Colon)?;
                    match name {
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
                        "exclusive" => {
                            mark_once(&mut fields, Self::EXCLUSIVE)?;
                            config.exclusive = match self.next() {
                                Token::Bool(value) => value,
                                _ => return Err(BarConfigError::UnexpectedToken),
                            };
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
        Ok(config)
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
        let mut fields = 0u16;
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

fn mark_once(fields: &mut u16, field: u16) -> Result<(), BarConfigError> {
    if *fields & field != 0 {
        return Err(BarConfigError::DuplicateField);
    }
    *fields |= field;
    Ok(())
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
                "position": "top",
                "height": 40,
                "spacing": 8,
                "margin": "1 2 3 4",
                "fixed-center": false,
                "exclusive": false,
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
        assert_eq!(config.position, BarPosition::Top);
        assert_eq!(config.height, 40);
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
        assert!(!config.exclusive);
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
        assert_eq!(config.position, BarPosition::Top);
        assert_eq!(config.height, 30);
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
        assert!(config.exclusive);
        assert_eq!(config.reserved_top(), 30);
        assert!(config.modules_left.is_empty());
        assert_eq!(
            config.module_configs.get("clock").unwrap().format_alt_click,
            BarButton::Left
        );
    }

    #[test]
    fn rejects_duplicate_invalid_and_overfull_fields() {
        assert_eq!(
            parse_waybar_config(r#"{ "height": 30, "height": 31 }"#),
            Err(BarConfigError::DuplicateField)
        );
        assert_eq!(
            parse_waybar_config(r#"{ "position": "middle" }"#),
            Err(BarConfigError::InvalidPosition)
        );
        for input in [
            r#"{ "margin": "" }"#,
            r#"{ "margin": "1 2 3 4 5" }"#,
            r#"{ "margin": "1px" }"#,
            r#"{ "margin": 1.5 }"#,
            r#"{ "margin-left": 2147483648 }"#,
            r#"{ "fixed-center": "false" }"#,
            r#"{ "exclusive": 1 }"#,
        ] {
            assert!(parse_waybar_config(input).is_err());
        }
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
