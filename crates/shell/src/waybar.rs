// SPDX-License-Identifier: 0BSD

pub const MAX_BAR_MODULES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BarPosition {
    Top,
    Bottom,
    Left,
    Right,
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
pub struct WaybarConfig<'a> {
    pub position: BarPosition,
    pub height: u16,
    pub spacing: u16,
    pub modules_left: BarModuleList<'a>,
    pub modules_center: BarModuleList<'a>,
    pub modules_right: BarModuleList<'a>,
}

impl Default for WaybarConfig<'_> {
    fn default() -> Self {
        Self {
            position: BarPosition::Top,
            height: 30,
            spacing: 4,
            modules_left: BarModuleList::empty(),
            modules_center: BarModuleList::empty(),
            modules_right: BarModuleList::empty(),
        }
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
    TooManyModules,
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
    Literal,
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
            b't' if self.consume_literal(b"true") => return Token::Literal,
            b'f' if self.consume_literal(b"false") => return Token::Literal,
            b'n' if self.consume_literal(b"null") => return Token::Literal,
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
    const POSITION: u8 = 1 << 0;
    const HEIGHT: u8 = 1 << 1;
    const SPACING: u8 = 1 << 2;
    const LEFT: u8 = 1 << 3;
    const CENTER: u8 = 1 << 4;
    const RIGHT: u8 = 1 << 5;

    const fn new(input: &'a str) -> Self {
        Self {
            lexer: JsonLexer::new(input),
            pushed: None,
        }
    }

    fn parse(mut self) -> Result<WaybarConfig<'a>, BarConfigError> {
        self.expect(Token::LeftBrace)?;
        let mut config = WaybarConfig::default();
        let mut fields = 0u8;
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
        if self.next() != Token::End {
            return Err(BarConfigError::UnexpectedToken);
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

    fn skip_value(&mut self) -> Result<(), BarConfigError> {
        match self.next() {
            Token::LeftBrace => self.skip_container(Token::RightBrace),
            Token::LeftBracket => self.skip_container(Token::RightBracket),
            Token::String(_) | Token::Number(_) | Token::Literal => Ok(()),
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

fn mark_once(fields: &mut u8, field: u8) -> Result<(), BarConfigError> {
    if *fields & field != 0 {
        return Err(BarConfigError::DuplicateField);
    }
    *fields |= field;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_waybar_jsonc_modules_and_ignores_module_options() {
        let config = parse_waybar_config(
            r#"
            {
                // Waybar-compatible top-level fields.
                "position": "top",
                "height": 40,
                "spacing": 8,
                "modules-left": ["niri/workspaces", "custom/launcher"],
                "modules-center": ["niri/window"],
                "modules-right": ["network", "cpu", "memory", "clock",],
                "clock": {
                    "format": "{:%H:%M}",
                    "tooltip": false
                },
            }
            "#,
        )
        .unwrap();
        assert_eq!(config.position, BarPosition::Top);
        assert_eq!(config.height, 40);
        assert_eq!(config.spacing, 8);
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
    }

    #[test]
    fn supports_block_comments_and_defaults() {
        let config = parse_waybar_config(r#"/* comment */ { "modules-left": [] }"#).unwrap();
        assert_eq!(config.position, BarPosition::Top);
        assert_eq!(config.height, 30);
        assert_eq!(config.spacing, 4);
        assert!(config.modules_left.is_empty());
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
    }
}
