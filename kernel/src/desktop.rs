// SPDX-License-Identifier: 0BSD

use crate::framebuffer::{
    BLACK, CYAN, Framebuffer, GREEN, INDIGO, MUTED, PANEL, RED, WHITE, WINDOW, WINDOW_ALT,
};
use crate::ps2::{Controller, InputEvent, Key, MouseEvent};
use crate::serial::serialln;
use slopos_shell::{
    BarPosition, ScrollLayout, WaybarConfig, parse_niri_layout, parse_waybar_config,
};

const WINDOW_COUNT: usize = 3;
const TITLE_HEIGHT: i32 = 30;
const NIRI_CONFIG: &str = include_str!("../../assets/niri-config.kdl");
const WAYBAR_CONFIG: &str = include_str!("../../assets/waybar-config.jsonc");

#[derive(Clone, Copy, Eq, PartialEq)]
enum WindowKind {
    Terminal,
    System,
    Config,
}

#[derive(Clone, Copy)]
struct Window {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    open: bool,
    kind: WindowKind,
}

pub struct Desktop {
    screen_width: i32,
    screen_height: i32,
    windows: [Window; WINDOW_COUNT],
    layout: ScrollLayout<WINDOW_COUNT, 1>,
    bar: WaybarConfig<'static>,
    active: usize,
    pointer_x: i32,
    pointer_y: i32,
    previous_buttons: u8,
    scrolling_view: bool,
    command: [u8; 48],
    command_length: usize,
    response: [u8; 64],
    response_length: usize,
    alternate_theme: bool,
}

impl Desktop {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width as i32;
        let height = height as i32;
        let config = parse_niri_layout(NIRI_CONFIG)
            .unwrap_or_else(|_| crate::fatal("niri layout config failed validation"));
        let bar = parse_waybar_config(WAYBAR_CONFIG)
            .unwrap_or_else(|_| crate::fatal("Waybar JSONC config failed validation"));
        if bar.position != BarPosition::Top {
            crate::fatal("early Waybar renderer currently requires position=top");
        }
        let mut layout = ScrollLayout::new(
            u16::try_from(width).unwrap_or(u16::MAX),
            u16::try_from(height).unwrap_or(u16::MAX),
            bar.height,
            config,
        );
        for window in 0..WINDOW_COUNT {
            layout
                .open_window(window as u32)
                .unwrap_or_else(|_| crate::fatal("niri layout seed capacity mismatch"));
        }
        layout
            .focus_window(0)
            .unwrap_or_else(|_| crate::fatal("niri layout terminal seed is missing"));
        let desktop = Self {
            screen_width: width,
            screen_height: height,
            windows: [
                Window {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                    open: true,
                    kind: WindowKind::Terminal,
                },
                Window {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                    open: true,
                    kind: WindowKind::System,
                },
                Window {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                    open: true,
                    kind: WindowKind::Config,
                },
            ],
            layout,
            bar,
            active: 0,
            pointer_x: width / 2,
            pointer_y: height / 2,
            previous_buttons: 0,
            scrolling_view: false,
            command: [0; 48],
            command_length: 0,
            response: [0; 64],
            response_length: 0,
            alternate_theme: false,
        };
        serialln(format_args!(
            "SLOPOS-SHELL: config loaded niri_columns=3 gaps={} default_width=50% center=never waybar_position=top height={} spacing={} modules={}/{}/{}",
            desktop.layout.config().gaps,
            desktop.bar.height,
            desktop.bar.spacing,
            desktop.bar.modules_left.len(),
            desktop.bar.modules_center.len(),
            desktop.bar.modules_right.len()
        ));
        desktop
    }

    pub async fn run(&mut self, framebuffer: &mut Framebuffer, mut input: Controller) -> ! {
        loop {
            let byte = crate::ps2::next_byte().await;
            if let Some(event) = input.consume(byte) {
                match event {
                    InputEvent::Key(key) => self.keyboard(key),
                    InputEvent::Mouse(mouse) => self.mouse(mouse),
                }
                self.render(framebuffer);
            }
        }
    }

    pub fn render(&self, framebuffer: &mut Framebuffer) {
        let bar_height = i32::from(self.bar.height);
        framebuffer.rect(
            0,
            0,
            self.screen_width,
            self.screen_height,
            self.layout.config().background_color,
        );
        for band in 0..8 {
            framebuffer.rect(
                0,
                bar_height + band * ((self.screen_height - bar_height) / 8),
                self.screen_width,
                (self.screen_height - bar_height) / 8,
                if band % 2 == 0 { 0x11172d } else { 0x131a32 },
            );
        }

        self.render_bar(framebuffer);

        for index in 0..WINDOW_COUNT {
            if self.windows[index].open {
                self.render_window(framebuffer, index);
            }
        }
        framebuffer.cursor(self.pointer_x, self.pointer_y);
    }

    fn render_bar(&self, framebuffer: &mut Framebuffer) {
        let bar_height = i32::from(self.bar.height);
        let baseline = ((bar_height - 7) / 2).max(2);
        framebuffer.rect(0, 0, self.screen_width, bar_height, PANEL);
        framebuffer.rect(10, 7, 26, 26, self.accent());
        framebuffer.text(18, 14, "S", WHITE, 2);

        let mut left_x = 47;
        for module in self.bar.modules_left.iter() {
            let text = self.bar_module_text(module);
            framebuffer.text(left_x, baseline, text, WHITE, 1);
            left_x += text_width(text) + i32::from(self.bar.spacing);
        }

        let mut center_width = 0;
        for module in self.bar.modules_center.iter() {
            if center_width != 0 {
                center_width += i32::from(self.bar.spacing);
            }
            center_width += text_width(self.bar_module_text(module));
        }
        let mut center_x = (self.screen_width - center_width) / 2;
        for module in self.bar.modules_center.iter() {
            let text = self.bar_module_text(module);
            framebuffer.text(center_x, baseline, text, WHITE, 1);
            center_x += text_width(text) + i32::from(self.bar.spacing);
        }

        let mut right_width = 0;
        for module in self.bar.modules_right.iter() {
            if right_width != 0 {
                right_width += i32::from(self.bar.spacing);
            }
            right_width += text_width(self.bar_module_text(module));
        }
        let mut right_x = self.screen_width - right_width - 12;
        for module in self.bar.modules_right.iter() {
            let text = self.bar_module_text(module);
            framebuffer.text(right_x, baseline, text, GREEN, 1);
            right_x += text_width(text) + i32::from(self.bar.spacing);
        }
    }

    fn bar_module_text<'a>(&self, module: &'a str) -> &'a str {
        match module {
            "niri/workspaces" => "1  2  3",
            "niri/window" => title(self.windows[self.active].kind),
            "custom/launcher" => "SLOPOS",
            "network" => "NET --",
            "cpu" => "CPU OK",
            "memory" => "MEM 36%",
            "clock" => "UTC",
            _ => module,
        }
    }

    fn render_window(&self, framebuffer: &mut Framebuffer, index: usize) {
        let Some(window) = self.positioned_window(index) else {
            return;
        };
        let active = index == self.active;
        framebuffer.rect(
            window.x + 7,
            window.y + 8,
            window.width,
            window.height,
            0x080a12,
        );
        framebuffer.rect(window.x, window.y, window.width, window.height, WINDOW_ALT);
        framebuffer.rect(
            window.x,
            window.y,
            window.width,
            TITLE_HEIGHT,
            if active { self.accent() } else { WINDOW },
        );
        if self.layout.config().focus_ring.enabled {
            framebuffer.outline(
                window.x,
                window.y,
                window.width,
                window.height,
                if active {
                    i32::from(self.layout.config().focus_ring.width)
                } else {
                    1
                },
                if active {
                    self.layout.config().focus_ring.active_color
                } else {
                    self.layout.config().focus_ring.inactive_color
                },
            );
        }
        framebuffer.text(window.x + 12, window.y + 9, title(window.kind), WHITE, 1);
        framebuffer.rect(window.x + window.width - 26, window.y + 5, 20, 20, RED);
        framebuffer.text(window.x + window.width - 20, window.y + 11, "X", WHITE, 1);

        match window.kind {
            WindowKind::Terminal => self.render_terminal(framebuffer, window),
            WindowKind::System => self.render_system(framebuffer, window),
            WindowKind::Config => self.render_config(framebuffer, window),
        }
    }

    fn render_terminal(&self, framebuffer: &mut Framebuffer, window: Window) {
        let x = window.x + 16;
        let mut y = window.y + 48;
        framebuffer.text(x, y, "SLOPOS KERNEL MONITOR 0.1", CYAN, 1);
        y += 19;
        framebuffer.text(x, y, "TYPE HELP FOR BUILT-IN COMMANDS.", MUTED, 1);
        y += 28;
        if self.response_length != 0 {
            framebuffer.text(x, y, self.response_text(), GREEN, 1);
            y += 24;
        }
        framebuffer.text(x, y, "SLOP> ", self.accent(), 1);
        framebuffer.text(x + 36, y, self.command_text(), WHITE, 1);
        framebuffer.rect(x + 36 + self.command_length as i32 * 6, y + 9, 5, 2, WHITE);
    }

    fn render_system(&self, framebuffer: &mut Framebuffer, window: Window) {
        let x = window.x + 16;
        let y = window.y + 47;
        framebuffer.text(x, y, "SYSTEM HEALTH", WHITE, 1);
        framebuffer.text(x, y + 22, "KERNEL", MUTED, 1);
        framebuffer.text(x + 114, y + 22, "RUNNING", GREEN, 1);
        framebuffer.text(x, y + 42, "MEMORY MAP", MUTED, 1);
        framebuffer.text(x + 114, y + 42, "UEFI OWNED", GREEN, 1);
        framebuffer.text(x, y + 62, "INPUT", MUTED, 1);
        framebuffer.text(x + 114, y + 62, "PS2 ACTIVE", GREEN, 1);
        framebuffer.text(x, y + 92, "BOOT MEMORY", WHITE, 1);
        framebuffer.rect(x, y + 110, window.width - 32, 10, BLACK);
        framebuffer.rect(x, y + 110, (window.width - 32) * 36 / 100, 10, CYAN);
        framebuffer.text(x, y + 132, "36% RESERVED DURING BOOT", MUTED, 1);
        framebuffer.text(x, y + 164, "TASKS", WHITE, 1);
        framebuffer.text(x + 114, y + 164, "PID 1 EXITED OK", GREEN, 1);
    }

    fn render_config(&self, framebuffer: &mut Framebuffer, window: Window) {
        let x = window.x + 16;
        let y = window.y + 47;
        framebuffer.text(x, y, "NIRI LAYOUT CONFIG", WHITE, 1);
        framebuffer.rect(x, y + 22, window.width - 32, 82, BLACK);
        framebuffer.text(x + 10, y + 34, "LAYOUT {", CYAN, 1);
        framebuffer.text(x + 22, y + 52, "GAPS 16", WHITE, 1);
        framebuffer.text(x + 22, y + 70, "DEFAULT-COLUMN-WIDTH 50%", WHITE, 1);
        framebuffer.text(x + 10, y + 88, "}", CYAN, 1);
        framebuffer.rect(x, y + 120, 122, 28, self.accent());
        framebuffer.text(x + 18, y + 130, "RELOAD STYLE", WHITE, 1);
        framebuffer.text(x, y + 166, "SOURCE: ASSETS/NIRI-CONFIG.KDL", MUTED, 1);
        framebuffer.text(x, y + 182, "KDL SUBSET VALIDATED AT STARTUP.", MUTED, 1);
    }

    fn keyboard(&mut self, key: Key) {
        match key {
            Key::Tab => self.focus_next(),
            Key::Escape => {
                self.scrolling_view = false;
            }
            Key::Backspace if self.active == 0 && self.command_length > 0 => {
                self.command_length -= 1;
            }
            Key::Enter if self.active == 0 => self.execute_command(),
            Key::Character(character)
                if self.active == 0
                    && self.windows[0].open
                    && self.command_length < self.command.len() =>
            {
                self.command[self.command_length] = character.to_ascii_uppercase();
                self.command_length += 1;
            }
            _ => {}
        }
    }

    fn mouse(&mut self, event: MouseEvent) {
        self.pointer_x = (self.pointer_x + event.dx as i32).clamp(0, self.screen_width - 1);
        self.pointer_y = (self.pointer_y + event.dy as i32).clamp(0, self.screen_height - 1);
        let left = event.buttons & 1 != 0;
        let left_was_down = self.previous_buttons & 1 != 0;

        if left && !left_was_down {
            self.pointer_pressed();
        } else if !left {
            self.scrolling_view = false;
        }

        if left && self.scrolling_view && event.dx != 0 {
            self.layout.scroll_by(-i32::from(event.dx));
            serialln(format_args!(
                "SLOPOS-SHELL: view scrolled offset={} gesture=titlebar-drag",
                self.layout.view_offset()
            ));
            if let Some(window) = self.positioned_window(self.active) {
                serialln(format_args!(
                    "SLOPOS-DESKTOP: window moved kind={} x={} y={} layout=scrolling",
                    title(window.kind),
                    window.x,
                    window.y
                ));
            }
        }
        self.previous_buttons = event.buttons;
    }

    fn pointer_pressed(&mut self) {
        for index in 0..WINDOW_COUNT {
            let Some(window) = self.positioned_window(index) else {
                continue;
            };
            if !window.open || !inside(self.pointer_x, self.pointer_y, window) {
                continue;
            }
            self.focus(index);
            if self.pointer_y < window.y + TITLE_HEIGHT {
                if self.pointer_x >= window.x + window.width - 30 {
                    self.windows[index].open = false;
                    self.layout
                        .close_window(index as u32)
                        .unwrap_or_else(|_| crate::fatal("layout close lost a tiled window"));
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: window closed kind={}",
                        title(window.kind)
                    ));
                    self.focus_top_open();
                } else {
                    self.scrolling_view = true;
                }
            } else if window.kind == WindowKind::Config {
                let apply_x = window.x + 16;
                let apply_y = window.y + 167;
                if self.pointer_x >= apply_x
                    && self.pointer_x < apply_x + 122
                    && self.pointer_y >= apply_y
                    && self.pointer_y < apply_y + 28
                {
                    self.alternate_theme = !self.alternate_theme;
                    serialln(format_args!(
                        "SLOPOS-CONFIG: in-memory desktop theme applied atomically"
                    ));
                }
            }
            return;
        }
    }

    fn execute_command(&mut self) {
        let command = self.command_text();
        let response = if command == "HELP" {
            "COMMANDS: HELP STATUS ABOUT CLEAR FAULT"
        } else if command == "STATUS" {
            "KERNEL OK / 3 NIRI COLUMNS / PS2 READY"
        } else if command == "ABOUT" {
            "SLOPOS SCROLLING-TILE RUST SHELL"
        } else if command == "FAULT" {
            crate::interrupts::trigger_page_fault()
        } else if command == "CLEAR" || command.is_empty() {
            ""
        } else {
            "UNKNOWN COMMAND. TYPE HELP."
        };
        serialln(format_args!("SLOPOS-TERMINAL: command={command}"));
        self.response.fill(0);
        self.response_length = response.len().min(self.response.len());
        self.response[..self.response_length]
            .copy_from_slice(&response.as_bytes()[..self.response_length]);
        self.command.fill(0);
        self.command_length = 0;
    }

    fn focus(&mut self, index: usize) {
        if !self.windows[index].open {
            return;
        }
        self.layout
            .focus_window(index as u32)
            .unwrap_or_else(|_| crate::fatal("layout focus lost a tiled window"));
        self.active = index;
    }

    fn focus_top_open(&mut self) {
        if let Some(window) = self.layout.focused_window() {
            self.active = window as usize;
        }
    }

    fn focus_next(&mut self) {
        if self.layout.focus_column_right() {
            if let Some(window) = self.layout.focused_window() {
                self.active = window as usize;
            }
            return;
        }
        while self.layout.focus_column_left() {}
        if let Some(window) = self.layout.focused_window() {
            self.active = window as usize;
        }
    }

    fn positioned_window(&self, index: usize) -> Option<Window> {
        if !self.windows[index].open {
            return None;
        }
        let rect = self.layout.tile_rect(index as u32).ok()?;
        let mut window = self.windows[index];
        window.x = rect.x;
        window.y = rect.y;
        window.width = i32::from(rect.width);
        window.height = i32::from(rect.height);
        Some(window)
    }

    fn command_text(&self) -> &str {
        // SAFETY: PS/2 input and programmatic writes are always ASCII.
        unsafe { core::str::from_utf8_unchecked(&self.command[..self.command_length]) }
    }

    fn response_text(&self) -> &str {
        // SAFETY: every response literal is ASCII.
        unsafe { core::str::from_utf8_unchecked(&self.response[..self.response_length]) }
    }

    fn accent(&self) -> u32 {
        if self.alternate_theme { CYAN } else { INDIGO }
    }
}

fn title(kind: WindowKind) -> &'static str {
    match kind {
        WindowKind::Terminal => "TERMINAL",
        WindowKind::System => "SYSTEM",
        WindowKind::Config => "CONFIG",
    }
}

fn inside(x: i32, y: i32, window: Window) -> bool {
    x >= window.x && x < window.x + window.width && y >= window.y && y < window.y + window.height
}

fn text_width(text: &str) -> i32 {
    i32::try_from(text.len()).unwrap_or(i32::MAX / 6) * 6
}
