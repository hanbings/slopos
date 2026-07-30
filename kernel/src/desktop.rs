// SPDX-License-Identifier: 0BSD

use crate::framebuffer::{
    AMBER, BLACK, CYAN, DESKTOP, Framebuffer, GREEN, INDIGO, MUTED, PANEL, RED, WHITE, WINDOW,
    WINDOW_ALT,
};
use crate::ps2::{Controller, InputEvent, Key, MouseEvent};
use crate::serial::serialln;

const WINDOW_COUNT: usize = 3;
const TITLE_HEIGHT: i32 = 30;
const TASKBAR_HEIGHT: i32 = 48;

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
    z_order: [usize; WINDOW_COUNT],
    active: usize,
    pointer_x: i32,
    pointer_y: i32,
    previous_buttons: u8,
    dragging: Option<(usize, i32, i32)>,
    resizing: Option<usize>,
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
        let terminal_width = (width * 58 / 100).max(360);
        let terminal_height = (height * 52 / 100).max(250);
        Self {
            screen_width: width,
            screen_height: height,
            windows: [
                Window {
                    x: 42,
                    y: 88,
                    width: terminal_width,
                    height: terminal_height,
                    open: true,
                    kind: WindowKind::Terminal,
                },
                Window {
                    x: (width - 370).max(260),
                    y: 104,
                    width: 330,
                    height: 270,
                    open: true,
                    kind: WindowKind::System,
                },
                Window {
                    x: (width - 470).max(180),
                    y: (height - 335).max(250),
                    width: 420,
                    height: 260,
                    open: true,
                    kind: WindowKind::Config,
                },
            ],
            z_order: [2, 1, 0],
            active: 0,
            pointer_x: width / 2,
            pointer_y: height / 2,
            previous_buttons: 0,
            dragging: None,
            resizing: None,
            command: [0; 48],
            command_length: 0,
            response: [0; 64],
            response_length: 0,
            alternate_theme: false,
        }
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
        framebuffer.rect(0, 0, self.screen_width, self.screen_height, DESKTOP);
        for band in 0..8 {
            framebuffer.rect(
                0,
                48 + band * ((self.screen_height - 96) / 8),
                self.screen_width,
                (self.screen_height - 96) / 8,
                if band % 2 == 0 { 0x11172d } else { 0x131a32 },
            );
        }

        framebuffer.rect(0, 0, self.screen_width, 48, PANEL);
        framebuffer.rect(16, 10, 28, 28, self.accent());
        framebuffer.text(24, 17, "S", WHITE, 2);
        framebuffer.text(56, 13, "SLOPOS", WHITE, 2);
        framebuffer.text(151, 18, "NATIVE RUST SYSTEM", MUTED, 1);
        framebuffer.text(self.screen_width - 170, 18, "UEFI  ASYNC PREVIEW", GREEN, 1);

        framebuffer.rect(
            0,
            self.screen_height - TASKBAR_HEIGHT,
            self.screen_width,
            TASKBAR_HEIGHT,
            PANEL,
        );
        for index in 0..WINDOW_COUNT {
            let x = 18 + index as i32 * 132;
            let selected = self.windows[index].open && self.active == index;
            framebuffer.rect(
                x,
                self.screen_height - 39,
                120,
                30,
                if selected { self.accent() } else { WINDOW },
            );
            framebuffer.text(
                x + 10,
                self.screen_height - 29,
                title(self.windows[index].kind),
                if selected { WHITE } else { MUTED },
                1,
            );
        }

        for index in self.z_order {
            if self.windows[index].open {
                self.render_window(framebuffer, index);
            }
        }
        framebuffer.cursor(self.pointer_x, self.pointer_y);
    }

    fn render_window(&self, framebuffer: &mut Framebuffer, index: usize) {
        let window = self.windows[index];
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
        framebuffer.outline(
            window.x,
            window.y,
            window.width,
            window.height,
            if active { 2 } else { 1 },
            if active { self.accent() } else { MUTED },
        );
        framebuffer.text(window.x + 12, window.y + 9, title(window.kind), WHITE, 1);
        framebuffer.rect(window.x + window.width - 26, window.y + 5, 20, 20, RED);
        framebuffer.text(window.x + window.width - 20, window.y + 11, "X", WHITE, 1);

        match window.kind {
            WindowKind::Terminal => self.render_terminal(framebuffer, window),
            WindowKind::System => self.render_system(framebuffer, window),
            WindowKind::Config => self.render_config(framebuffer, window),
        }
        framebuffer.rect(
            window.x + window.width - 12,
            window.y + window.height - 4,
            10,
            2,
            MUTED,
        );
        framebuffer.rect(
            window.x + window.width - 4,
            window.y + window.height - 12,
            2,
            10,
            MUTED,
        );
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
        framebuffer.text(x + 114, y + 164, "1 KERNEL / 0 USER", AMBER, 1);
    }

    fn render_config(&self, framebuffer: &mut Framebuffer, window: Window) {
        let x = window.x + 16;
        let y = window.y + 47;
        framebuffer.text(x, y, "DECLARATIVE CONFIG PREVIEW", WHITE, 1);
        framebuffer.rect(x, y + 22, window.width - 32, 82, BLACK);
        framebuffer.text(x + 10, y + 34, "DESKTOP = {", CYAN, 1);
        framebuffer.text(
            x + 22,
            y + 52,
            if self.alternate_theme {
                "THEME = CYAN;"
            } else {
                "THEME = INDIGO;"
            },
            WHITE,
            1,
        );
        framebuffer.text(x + 22, y + 70, "POINTER = PS2;", WHITE, 1);
        framebuffer.text(x + 10, y + 88, "};", CYAN, 1);
        framebuffer.rect(x, y + 120, 122, 28, self.accent());
        framebuffer.text(x + 18, y + 130, "APPLY THEME", WHITE, 1);
        framebuffer.text(x, y + 166, "CLICK APPLY TO ATOMICALLY SWITCH", MUTED, 1);
        framebuffer.text(x, y + 182, "THE IN-MEMORY DEMO THEME.", MUTED, 1);
    }

    fn keyboard(&mut self, key: Key) {
        match key {
            Key::Tab => self.focus_next(),
            Key::Escape => {
                self.dragging = None;
                self.resizing = None;
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
            if let Some(index) = self.resizing {
                let window = self.windows[index];
                serialln(format_args!(
                    "SLOPOS-DESKTOP: window resized kind={} width={} height={}",
                    title(window.kind),
                    window.width,
                    window.height
                ));
            }
            self.dragging = None;
            self.resizing = None;
        }

        if left {
            if let Some((index, offset_x, offset_y)) = self.dragging {
                let window = &mut self.windows[index];
                window.x =
                    (self.pointer_x - offset_x).clamp(0, (self.screen_width - window.width).max(0));
                window.y = (self.pointer_y - offset_y).clamp(
                    48,
                    (self.screen_height - TASKBAR_HEIGHT - TITLE_HEIGHT).max(48),
                );
                if event.dx != 0 || event.dy != 0 {
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: window moved kind={} x={} y={}",
                        title(window.kind),
                        window.x,
                        window.y
                    ));
                }
            }
            if let Some(index) = self.resizing {
                let window = &mut self.windows[index];
                window.width = (self.pointer_x - window.x).clamp(240, self.screen_width - window.x);
                window.height = (self.pointer_y - window.y)
                    .clamp(150, self.screen_height - TASKBAR_HEIGHT - window.y);
            }
        }
        self.previous_buttons = event.buttons;
    }

    fn pointer_pressed(&mut self) {
        if self.pointer_y >= self.screen_height - TASKBAR_HEIGHT {
            let launcher = (self.pointer_x - 18) / 132;
            if (0..WINDOW_COUNT as i32).contains(&launcher)
                && self.pointer_x <= 18 + launcher * 132 + 120
            {
                let index = launcher as usize;
                self.windows[index].open = true;
                self.focus(index);
                return;
            }
        }

        for z_index in (0..WINDOW_COUNT).rev() {
            let index = self.z_order[z_index];
            let window = self.windows[index];
            if !window.open || !inside(self.pointer_x, self.pointer_y, window) {
                continue;
            }
            self.focus(index);
            if self.pointer_y < window.y + TITLE_HEIGHT {
                if self.pointer_x >= window.x + window.width - 30 {
                    self.windows[index].open = false;
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: window closed kind={}",
                        title(window.kind)
                    ));
                    self.focus_top_open();
                } else {
                    self.dragging =
                        Some((index, self.pointer_x - window.x, self.pointer_y - window.y));
                }
            } else if self.pointer_x >= window.x + window.width - 16
                && self.pointer_y >= window.y + window.height - 16
            {
                self.resizing = Some(index);
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
            "KERNEL OK / 3 WINDOWS / PS2 READY"
        } else if command == "ABOUT" {
            "SLOPOS 0.1 EARLY NATIVE RUST DESKTOP"
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
        self.active = index;
        if let Some(position) = self
            .z_order
            .iter()
            .position(|candidate| *candidate == index)
        {
            for current in position..WINDOW_COUNT - 1 {
                self.z_order[current] = self.z_order[current + 1];
            }
            self.z_order[WINDOW_COUNT - 1] = index;
        }
    }

    fn focus_top_open(&mut self) {
        for position in (0..WINDOW_COUNT).rev() {
            let index = self.z_order[position];
            if self.windows[index].open {
                self.active = index;
                return;
            }
        }
    }

    fn focus_next(&mut self) {
        for step in 1..=WINDOW_COUNT {
            let candidate = (self.active + step) % WINDOW_COUNT;
            if self.windows[candidate].open {
                self.focus(candidate);
                return;
            }
        }
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
