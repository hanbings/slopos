// SPDX-License-Identifier: 0BSD

pub const MAX_WAYBAR_STYLE_RULES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Background {
    Transparent,
    Color(u32),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct StylePatch {
    foreground: Option<u32>,
    background: Option<Background>,
    padding_left: Option<u16>,
    padding_right: Option<u16>,
    margin_left: Option<u16>,
    margin_right: Option<u16>,
    border_bottom_width: Option<u16>,
    border_bottom_color: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StyleRule<'a> {
    selector: &'a str,
    patch: StylePatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedWaybarStyle {
    pub foreground: u32,
    pub background: Option<u32>,
    pub padding_left: u16,
    pub padding_right: u16,
    pub margin_left: u16,
    pub margin_right: u16,
    pub border_bottom_width: u16,
    pub border_bottom_color: u32,
}

impl ResolvedWaybarStyle {
    pub const fn new(foreground: u32, background: Option<u32>) -> Self {
        Self {
            foreground,
            background,
            padding_left: 0,
            padding_right: 0,
            margin_left: 0,
            margin_right: 0,
            border_bottom_width: 0,
            border_bottom_color: foreground,
        }
    }

    fn apply(&mut self, patch: StylePatch) {
        if let Some(color) = patch.foreground {
            self.foreground = color;
        }
        if let Some(background) = patch.background {
            self.background = match background {
                Background::Transparent => None,
                Background::Color(color) => Some(color),
            };
        }
        if let Some(value) = patch.padding_left {
            self.padding_left = value;
        }
        if let Some(value) = patch.padding_right {
            self.padding_right = value;
        }
        if let Some(value) = patch.margin_left {
            self.margin_left = value;
        }
        if let Some(value) = patch.margin_right {
            self.margin_right = value;
        }
        if let Some(value) = patch.border_bottom_width {
            self.border_bottom_width = value;
        }
        if let Some(value) = patch.border_bottom_color {
            self.border_bottom_color = value;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaybarStyle<'a> {
    rules: [Option<StyleRule<'a>>; MAX_WAYBAR_STYLE_RULES],
    length: usize,
}

impl<'a> WaybarStyle<'a> {
    const fn empty() -> Self {
        Self {
            rules: [None; MAX_WAYBAR_STYLE_RULES],
            length: 0,
        }
    }

    pub const fn len(self) -> usize {
        self.length
    }

    pub const fn is_empty(self) -> bool {
        self.length == 0
    }

    pub fn resolve(self, selector: &str, mut defaults: ResolvedWaybarStyle) -> ResolvedWaybarStyle {
        for rule in self.rules[..self.length].iter().flatten() {
            if rule.selector == "*" || rule.selector == selector {
                defaults.apply(rule.patch);
            }
        }
        defaults
    }

    fn push(&mut self, selector: &'a str, patch: StylePatch) -> Result<(), WaybarStyleError> {
        let selector = selector.trim();
        if selector.is_empty() || selector.len() > 64 {
            return Err(WaybarStyleError::InvalidSelector);
        }
        if self.length == self.rules.len() {
            return Err(WaybarStyleError::TooManyRules);
        }
        self.rules[self.length] = Some(StyleRule { selector, patch });
        self.length += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaybarStyleError {
    UnexpectedEnd,
    UnexpectedToken,
    InvalidSelector,
    InvalidColor,
    InvalidLength,
    InvalidBorder,
    TooManyRules,
}

pub fn parse_waybar_style(input: &str) -> Result<WaybarStyle<'_>, WaybarStyleError> {
    CssParser::new(input).parse()
}

struct CssParser<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> CssParser<'a> {
    const fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    fn parse(mut self) -> Result<WaybarStyle<'a>, WaybarStyleError> {
        let mut style = WaybarStyle::empty();
        loop {
            self.skip_trivia()?;
            if self.offset == self.input.len() {
                return Ok(style);
            }
            let selectors = self.read_until(b'{')?.trim();
            if selectors.is_empty() || selectors.starts_with('@') {
                return Err(WaybarStyleError::InvalidSelector);
            }
            self.offset += 1;
            let patch = self.declarations()?;
            for selector in selectors.split(',') {
                style.push(selector, patch)?;
            }
        }
    }

    fn declarations(&mut self) -> Result<StylePatch, WaybarStyleError> {
        let mut patch = StylePatch::default();
        loop {
            self.skip_trivia()?;
            if self.peek() == Some(b'}') {
                self.offset += 1;
                return Ok(patch);
            }
            let property = self.read_until(b':')?.trim();
            if property.is_empty() || property.contains(['{', '}']) {
                return Err(WaybarStyleError::UnexpectedToken);
            }
            self.offset += 1;
            let (value, closed) = self.read_declaration_value()?;
            apply_declaration(&mut patch, property, value.trim())?;
            if closed {
                return Ok(patch);
            }
        }
    }

    fn read_declaration_value(&mut self) -> Result<(&'a str, bool), WaybarStyleError> {
        let start = self.offset;
        while let Some(byte) = self.peek() {
            match byte {
                b';' => {
                    let value = &self.input[start..self.offset];
                    self.offset += 1;
                    return Ok((value, false));
                }
                b'}' => {
                    let value = &self.input[start..self.offset];
                    self.offset += 1;
                    return Ok((value, true));
                }
                b'{' => return Err(WaybarStyleError::UnexpectedToken),
                _ => self.offset += 1,
            }
        }
        Err(WaybarStyleError::UnexpectedEnd)
    }

    fn read_until(&mut self, delimiter: u8) -> Result<&'a str, WaybarStyleError> {
        let start = self.offset;
        while let Some(byte) = self.peek() {
            if byte == delimiter {
                return Ok(&self.input[start..self.offset]);
            }
            self.offset += 1;
        }
        Err(WaybarStyleError::UnexpectedEnd)
    }

    fn skip_trivia(&mut self) -> Result<(), WaybarStyleError> {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.offset += 1;
            }
            if self
                .input
                .as_bytes()
                .get(self.offset..self.offset.saturating_add(2))
                == Some(b"/*")
            {
                self.offset += 2;
                while self
                    .input
                    .as_bytes()
                    .get(self.offset..self.offset.saturating_add(2))
                    != Some(b"*/")
                {
                    if self.offset == self.input.len() {
                        return Err(WaybarStyleError::UnexpectedEnd);
                    }
                    self.offset += 1;
                }
                self.offset += 2;
                continue;
            }
            return Ok(());
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.offset).copied()
    }
}

fn apply_declaration(
    patch: &mut StylePatch,
    property: &str,
    value: &str,
) -> Result<(), WaybarStyleError> {
    match property {
        "color" => patch.foreground = Some(parse_color(value)?),
        "background" | "background-color" => {
            patch.background = Some(if value == "transparent" {
                Background::Transparent
            } else {
                Background::Color(parse_color(value)?)
            });
        }
        "padding" => {
            let (left, right) = parse_horizontal_box(value)?;
            patch.padding_left = Some(left);
            patch.padding_right = Some(right);
        }
        "margin" => {
            let (left, right) = parse_horizontal_box(value)?;
            patch.margin_left = Some(left);
            patch.margin_right = Some(right);
        }
        "border-bottom" => {
            let mut fields = value.split_ascii_whitespace();
            let width = parse_px(fields.next().ok_or(WaybarStyleError::InvalidBorder)?)?;
            if fields.next() != Some("solid") {
                return Err(WaybarStyleError::InvalidBorder);
            }
            let color = parse_color(fields.next().ok_or(WaybarStyleError::InvalidBorder)?)?;
            if fields.next().is_some() {
                return Err(WaybarStyleError::InvalidBorder);
            }
            patch.border_bottom_width = Some(width);
            patch.border_bottom_color = Some(color);
        }
        _ => {}
    }
    Ok(())
}

fn parse_horizontal_box(value: &str) -> Result<(u16, u16), WaybarStyleError> {
    let mut values = [0u16; 4];
    let mut count = 0usize;
    for field in value.split_ascii_whitespace() {
        if count == values.len() {
            return Err(WaybarStyleError::InvalidLength);
        }
        values[count] = parse_px(field)?;
        count += 1;
    }
    match count {
        1 => Ok((values[0], values[0])),
        2 | 3 => Ok((values[1], values[1])),
        4 => Ok((values[3], values[1])),
        _ => Err(WaybarStyleError::InvalidLength),
    }
}

fn parse_px(value: &str) -> Result<u16, WaybarStyleError> {
    let digits = value
        .strip_suffix("px")
        .or_else(|| (value == "0").then_some("0"))
        .ok_or(WaybarStyleError::InvalidLength)?;
    if digits.is_empty() {
        return Err(WaybarStyleError::InvalidLength);
    }
    let mut result = 0u16;
    for byte in digits.bytes() {
        if !byte.is_ascii_digit() {
            return Err(WaybarStyleError::InvalidLength);
        }
        result = result
            .checked_mul(10)
            .and_then(|current| current.checked_add(u16::from(byte - b'0')))
            .ok_or(WaybarStyleError::InvalidLength)?;
    }
    Ok(result)
}

fn parse_color(value: &str) -> Result<u32, WaybarStyleError> {
    let digits = value
        .strip_prefix('#')
        .ok_or(WaybarStyleError::InvalidColor)?;
    if digits.len() != 6 && digits.len() != 8 {
        return Err(WaybarStyleError::InvalidColor);
    }
    let mut color = 0u32;
    for byte in digits.bytes().take(6) {
        color = color
            .checked_mul(16)
            .and_then(|current| hex_digit(byte).map(|digit| current + u32::from(digit)))
            .ok_or(WaybarStyleError::InvalidColor)?;
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
    fn parses_bar_and_module_colors_spacing_and_border() {
        let style = parse_waybar_style(
            r#"
            * { color: #eeeeee; }
            window#waybar {
                background-color: #121827;
                border-bottom: 2px solid #6558f5;
            }
            #cpu { color: #001122; background: #62e6a8; padding: 0 7px; }
            "#,
        )
        .unwrap();
        assert_eq!(style.len(), 3);
        let bar = style.resolve("window#waybar", ResolvedWaybarStyle::new(0, Some(0)));
        assert_eq!(bar.foreground, 0xeeeeee);
        assert_eq!(bar.background, Some(0x121827));
        assert_eq!(bar.border_bottom_width, 2);
        assert_eq!(bar.border_bottom_color, 0x6558f5);
        let cpu = style.resolve("#cpu", ResolvedWaybarStyle::new(0, None));
        assert_eq!(cpu.foreground, 0x001122);
        assert_eq!(cpu.background, Some(0x62e6a8));
        assert_eq!(cpu.padding_left, 7);
        assert_eq!(cpu.padding_right, 7);
    }

    #[test]
    fn supports_comments_selector_lists_transparency_and_cascade() {
        let style = parse_waybar_style(
            r#"
            /* GTK CSS-compatible selector seed. */
            #network, #memory { margin: 1px 3px 2px 4px; color: #ffffff; }
            #network { color: #00ff00; background-color: transparent; }
            "#,
        )
        .unwrap();
        assert_eq!(style.len(), 3);
        let network = style.resolve("#network", ResolvedWaybarStyle::new(0, Some(1)));
        assert_eq!(network.foreground, 0x00ff00);
        assert_eq!(network.background, None);
        assert_eq!(network.margin_left, 4);
        assert_eq!(network.margin_right, 3);
    }

    #[test]
    fn rejects_invalid_colors_lengths_borders_and_unclosed_input() {
        assert_eq!(
            parse_waybar_style("#cpu { color: red; }"),
            Err(WaybarStyleError::InvalidColor)
        );
        assert_eq!(
            parse_waybar_style("#cpu { padding: 1em; }"),
            Err(WaybarStyleError::InvalidLength)
        );
        assert_eq!(
            parse_waybar_style("#cpu { border-bottom: 1px dotted #ffffff; }"),
            Err(WaybarStyleError::InvalidBorder)
        );
        assert_eq!(
            parse_waybar_style("#cpu { color: #ffffff;"),
            Err(WaybarStyleError::UnexpectedEnd)
        );
    }
}
