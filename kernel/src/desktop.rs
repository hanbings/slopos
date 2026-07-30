// SPDX-License-Identifier: 0BSD

use crate::framebuffer::{
    BLACK, CYAN, Framebuffer, GREEN, INDIGO, MUTED, PANEL, RED, WHITE, WINDOW, WINDOW_ALT,
};
use crate::ps2::{Controller, DesktopEvent, InputEvent, Key, KeyEvent, KeyModifiers, MouseEvent};
use crate::serial::serialln;
use slopos_shell::{
    BarFormatValue, BarPosition, BarText, BindingKey, BindingModifiers, NiriAction,
    NiriShellConfig, PpmImage, ResizeMode, ResolvedWaybarStyle, SwwwCommand, SwwwDaemonError,
    SwwwDefaults, WallpaperDaemon, WaybarConfig, WaybarStyle, WorkspaceSet, format_bar_text,
    parse_niri_layout, parse_niri_shell_config, parse_ppm, parse_swww_command,
    parse_swww_environment, parse_waybar_config, parse_waybar_style, transition_pixel,
};

const WINDOW_COUNT: usize = 3;
const WORKSPACE_CAPACITY: usize = 4;
const TITLE_HEIGHT: i32 = 30;
const NIRI_CONFIG: &str = include_str!("../../assets/niri-config.kdl");
const WAYBAR_CONFIG: &str = include_str!("../../assets/waybar-config.jsonc");
const WAYBAR_STYLE: &str = include_str!("../../assets/waybar-style.css");
const SWWW_ENVIRONMENT: &str = include_str!("../../assets/swww.env");
const AURORA_PPM: &str = include_str!("../../assets/wallpapers/aurora.ppm");
const SUNSET_PPM: &str = include_str!("../../assets/wallpapers/sunset.ppm");
const AURORA_PATH: &str = "/usr/share/backgrounds/slopos-aurora.ppm";
const SUNSET_PATH: &str = "/usr/share/backgrounds/slopos-sunset.ppm";

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
    workspaces: WorkspaceSet<WORKSPACE_CAPACITY, WINDOW_COUNT, 1>,
    niri: NiriShellConfig<'static>,
    bar: WaybarConfig<'static>,
    bar_style: WaybarStyle<'static>,
    swww_defaults: SwwwDefaults,
    wallpaper: WallpaperDaemon,
    active: usize,
    pointer_x: i32,
    pointer_y: i32,
    previous_buttons: u8,
    scrolling_view: bool,
    command: [u8; 128],
    command_length: usize,
    response: [u8; 128],
    response_length: usize,
    alternate_theme: bool,
    config_generation: u64,
}

impl Desktop {
    pub fn new(width: usize, height: usize) -> Self {
        let width = width as i32;
        let height = height as i32;
        let config = parse_niri_layout(NIRI_CONFIG)
            .unwrap_or_else(|_| crate::fatal("niri layout config failed validation"));
        let niri = parse_niri_shell_config(NIRI_CONFIG)
            .unwrap_or_else(|_| crate::fatal("niri shell config failed validation"));
        let bar = parse_waybar_config(WAYBAR_CONFIG)
            .unwrap_or_else(|_| crate::fatal("Waybar JSONC config failed validation"));
        let bar_style = parse_waybar_style(WAYBAR_STYLE)
            .unwrap_or_else(|_| crate::fatal("Waybar CSS failed validation"));
        let swww_defaults = parse_swww_environment(SWWW_ENVIRONMENT)
            .unwrap_or_else(|_| crate::fatal("swww environment defaults failed validation"));
        parse_ppm(AURORA_PPM).unwrap_or_else(|_| crate::fatal("aurora PPM failed validation"));
        parse_ppm(SUNSET_PPM).unwrap_or_else(|_| crate::fatal("sunset PPM failed validation"));
        if bar.position != BarPosition::Top {
            crate::fatal("early Waybar renderer currently requires position=top");
        }
        let workspace_count = niri.workspaces.len() + 1;
        let mut workspaces = WorkspaceSet::new(
            workspace_count,
            u16::try_from(width).unwrap_or(u16::MAX),
            u16::try_from(height).unwrap_or(u16::MAX),
            bar.height,
            config,
        )
        .unwrap_or_else(|_| crate::fatal("niri workspace capacity mismatch"));
        for window in 0..WINDOW_COUNT {
            let workspace = niri
                .window_rules
                .workspace_for(app_id(window_kind(window)))
                .and_then(|name| niri.workspaces.index_of(name))
                .unwrap_or(0);
            workspaces
                .open_window(workspace, window as u32)
                .unwrap_or_else(|_| crate::fatal("niri layout seed capacity mismatch"));
        }
        workspaces
            .focus_window(0)
            .unwrap_or_else(|_| crate::fatal("niri layout terminal seed is missing"));
        let mut wallpaper = WallpaperDaemon::new(
            "SLOPOS-1",
            u16::try_from(width).unwrap_or(u16::MAX),
            u16::try_from(height).unwrap_or(u16::MAX),
        );
        let SwwwCommand::Daemon = parse_swww_command("swww-daemon", swww_defaults)
            .unwrap_or_else(|_| crate::fatal("swww daemon boot command failed validation"))
        else {
            crate::fatal("swww daemon boot command changed kind");
        };
        wallpaper
            .start()
            .unwrap_or_else(|_| crate::fatal("swww daemon failed to start"));
        let SwwwCommand::Img(initial_wallpaper) = parse_swww_command(
            "swww img /usr/share/backgrounds/slopos-aurora.ppm",
            swww_defaults,
        )
        .unwrap_or_else(|_| crate::fatal("swww wallpaper boot command failed validation")) else {
            crate::fatal("swww wallpaper boot command changed kind");
        };
        wallpaper
            .apply(initial_wallpaper)
            .unwrap_or_else(|_| crate::fatal("swww initial wallpaper failed"));
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
            workspaces,
            niri,
            bar,
            bar_style,
            swww_defaults,
            wallpaper,
            active: 0,
            pointer_x: width / 2,
            pointer_y: height / 2,
            previous_buttons: 0,
            scrolling_view: false,
            command: [0; 128],
            command_length: 0,
            response: [0; 128],
            response_length: 0,
            alternate_theme: false,
            config_generation: 0,
        };
        serialln(format_args!(
            "SLOPOS-SHELL: config loaded niri_workspaces={} named={} binds={} rules={} active_columns=2 gaps={} default_width=50% center=never waybar_position=top height={} spacing={} modules={}/{}/{} module_configs={} css_rules={}",
            desktop.workspaces.len(),
            desktop.niri.workspaces.len(),
            desktop.niri.bindings.len(),
            desktop.niri.window_rules.len(),
            desktop.workspaces.config().gaps,
            desktop.bar.height,
            desktop.bar.spacing,
            desktop.bar.modules_left.len(),
            desktop.bar.modules_center.len(),
            desktop.bar.modules_right.len(),
            desktop.bar.module_configs.len(),
            desktop.bar_style.len()
        ));
        serialln(format_args!(
            "SLOPOS-WAYBAR: formats active workspace={{value}} window={{title}} cpu=\"CPU {{usage}}%\" memory=\"MEM {{percentage}}%\" intervals={}/{}/{}/{} css=foreground/background/padding/margin/border-bottom",
            desktop.module_interval("network"),
            desktop.module_interval("cpu"),
            desktop.module_interval("memory"),
            desktop.module_interval("clock")
        ));
        let wallpaper = desktop
            .wallpaper
            .query()
            .unwrap_or_else(|_| crate::fatal("swww initial query failed"));
        serialln(format_args!(
            "SLOPOS-SWWW: daemon=running output={} geometry={}x{} image={} transition={} step={} fps={}",
            wallpaper.output,
            wallpaper.width,
            wallpaper.height,
            wallpaper.image,
            desktop.wallpaper.transition().kind.name(),
            desktop.wallpaper.transition().step,
            desktop.wallpaper.transition().fps
        ));
        desktop
    }

    pub async fn run(&mut self, framebuffer: &mut Framebuffer, mut input: Controller) -> ! {
        loop {
            match crate::ps2::next_desktop_event(self.config_generation).await {
                DesktopEvent::ConfigUpdate(sources) => {
                    self.apply_config_update(sources);
                    self.render(framebuffer);
                }
                DesktopEvent::Input(byte) => {
                    if let Some(event) = input.consume(byte) {
                        let animate = match event {
                            InputEvent::Key(key) => self.keyboard(key),
                            InputEvent::Mouse(mouse) => {
                                self.mouse(mouse);
                                false
                            }
                        };
                        if animate {
                            self.animate_wallpaper(framebuffer);
                        } else {
                            self.render(framebuffer);
                        }
                    }
                }
            }
        }
    }

    pub fn render(&self, framebuffer: &mut Framebuffer) {
        self.render_wallpaper(framebuffer);
        self.render_bar(framebuffer);

        for index in 0..WINDOW_COUNT {
            if self.windows[index].open {
                self.render_window(framebuffer, index);
            }
        }
        framebuffer.cursor(self.pointer_x, self.pointer_y);
    }

    fn render_wallpaper(&self, framebuffer: &mut Framebuffer) {
        framebuffer.rect(
            0,
            0,
            self.screen_width,
            self.screen_height,
            self.workspaces.config().background_color,
        );
        let Some(current_path) = self.wallpaper.current_image() else {
            return;
        };
        let current = wallpaper_asset(current_path)
            .unwrap_or_else(|| crate::fatal("swww current image left embedded registry"));
        let previous = self
            .wallpaper
            .previous_image()
            .and_then(wallpaper_asset)
            .unwrap_or(current);
        if previous.width() != current.width() || previous.height() != current.height() {
            crate::fatal("swww embedded transition dimensions differ");
        }
        let (destination_x, destination_y, scale) = wallpaper_destination(
            self.wallpaper.transition().resize,
            current,
            self.screen_width,
            self.screen_height,
        );
        let mut old_pixels = previous.pixels();
        let mut new_pixels = current.pixels();
        for y in 0..current.height() {
            for x in 0..current.width() {
                let old = old_pixels
                    .next()
                    .unwrap_or_else(|| crate::fatal("swww previous PPM pixel stream truncated"));
                let new = new_pixels
                    .next()
                    .unwrap_or_else(|| crate::fatal("swww current PPM pixel stream truncated"));
                let color = transition_pixel(
                    self.wallpaper.transition().kind,
                    self.wallpaper.progress(),
                    (x, y),
                    (current.width(), current.height()),
                    old,
                    new,
                );
                framebuffer.rect(
                    destination_x + i32::from(x) * scale,
                    destination_y + i32::from(y) * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
    }

    fn animate_wallpaper(&mut self, framebuffer: &mut Framebuffer) {
        if !self.wallpaper.transition_active() {
            self.render(framebuffer);
            return;
        }
        let transition = self.wallpaper.transition();
        let sampled_step = transition.step.max(16);
        let mut frames = 0u16;
        loop {
            self.render(framebuffer);
            frames += 1;
            if self.wallpaper.progress() == u8::MAX {
                break;
            }
            self.wallpaper
                .set_progress(self.wallpaper.progress().saturating_add(sampled_step));
        }
        self.wallpaper.finish_transition();
        serialln(format_args!(
            "SLOPOS-SWWW: transition complete type={} step={} fps={} frames={}",
            transition.kind.name(),
            transition.step,
            transition.fps,
            frames
        ));
    }

    fn render_bar(&self, framebuffer: &mut Framebuffer) {
        let bar_height = i32::from(self.bar.height);
        let baseline = ((bar_height - 7) / 2).max(2);
        let bar_style = self.bar_style.resolve(
            "window#waybar",
            ResolvedWaybarStyle::new(WHITE, Some(PANEL)),
        );
        if let Some(background) = bar_style.background {
            framebuffer.rect(0, 0, self.screen_width, bar_height, background);
        }
        framebuffer.rect(10, 7, 26, 26, self.accent());
        framebuffer.text(18, 14, "S", WHITE, 2);

        let mut left_x = 47;
        for module in self.bar.modules_left.iter() {
            left_x += self.render_bar_module(framebuffer, module, left_x, baseline, bar_height)
                + i32::from(self.bar.spacing);
        }

        let mut center_width = 0;
        for module in self.bar.modules_center.iter() {
            if center_width != 0 {
                center_width += i32::from(self.bar.spacing);
            }
            center_width += self.bar_module_width(module);
        }
        let mut center_x = (self.screen_width - center_width) / 2;
        for module in self.bar.modules_center.iter() {
            center_x += self.render_bar_module(framebuffer, module, center_x, baseline, bar_height)
                + i32::from(self.bar.spacing);
        }

        let mut right_width = 0;
        for module in self.bar.modules_right.iter() {
            if right_width != 0 {
                right_width += i32::from(self.bar.spacing);
            }
            right_width += self.bar_module_width(module);
        }
        let mut right_x = self.screen_width - right_width - 12;
        for module in self.bar.modules_right.iter() {
            right_x += self.render_bar_module(framebuffer, module, right_x, baseline, bar_height)
                + i32::from(self.bar.spacing);
        }
        if bar_style.border_bottom_width != 0 {
            framebuffer.rect(
                0,
                bar_height - i32::from(bar_style.border_bottom_width),
                self.screen_width,
                i32::from(bar_style.border_bottom_width),
                bar_style.border_bottom_color,
            );
        }
    }

    fn render_bar_module(
        &self,
        framebuffer: &mut Framebuffer,
        module: &str,
        x: i32,
        baseline: i32,
        bar_height: i32,
    ) -> i32 {
        let text = self.bar_module_text(module);
        let style = self.bar_module_style(module);
        let box_x = x + i32::from(style.margin_left);
        let box_width = i32::from(style.padding_left)
            + text_width(text.as_str())
            + i32::from(style.padding_right);
        if let Some(background) = style.background {
            framebuffer.rect(box_x, 0, box_width, bar_height, background);
        }
        if style.border_bottom_width != 0 {
            framebuffer.rect(
                box_x,
                bar_height - i32::from(style.border_bottom_width),
                box_width,
                i32::from(style.border_bottom_width),
                style.border_bottom_color,
            );
        }
        framebuffer.text(
            box_x + i32::from(style.padding_left),
            baseline,
            text.as_str(),
            style.foreground,
            1,
        );
        i32::from(style.margin_left) + box_width + i32::from(style.margin_right)
    }

    fn bar_module_width(&self, module: &str) -> i32 {
        let text = self.bar_module_text(module);
        let style = self.bar_module_style(module);
        i32::from(style.margin_left)
            + i32::from(style.padding_left)
            + text_width(text.as_str())
            + i32::from(style.padding_right)
            + i32::from(style.margin_right)
    }

    fn bar_module_style(&self, module: &str) -> ResolvedWaybarStyle {
        self.bar_style.resolve(
            module_selector(module),
            ResolvedWaybarStyle::new(WHITE, None),
        )
    }

    fn module_interval(&self, module: &str) -> u16 {
        self.bar
            .module_configs
            .get(module)
            .and_then(|config| config.interval)
            .unwrap_or(0)
    }

    fn apply_config_update(&mut self, sources: crate::desktop_config::DesktopConfigSources) {
        let layout = parse_niri_layout(sources.niri)
            .unwrap_or_else(|_| crate::fatal("published niri layout became invalid"));
        let niri = parse_niri_shell_config(sources.niri)
            .unwrap_or_else(|_| crate::fatal("published niri shell config became invalid"));
        let bar = parse_waybar_config(sources.waybar)
            .unwrap_or_else(|_| crate::fatal("published Waybar config became invalid"));
        let bar_style = parse_waybar_style(sources.waybar_style)
            .unwrap_or_else(|_| crate::fatal("published Waybar style became invalid"));
        let swww_defaults = parse_swww_environment(sources.swww)
            .unwrap_or_else(|_| crate::fatal("published swww defaults became invalid"));
        if bar.position != BarPosition::Top {
            crate::fatal("published Waybar position is unsupported");
        }

        let old_workspace = self.workspaces.active();
        let preferred_window = self.workspaces.focused_window();
        let workspace_count = niri.workspaces.len() + 1;
        let mut workspaces = WorkspaceSet::new(
            workspace_count,
            u16::try_from(self.screen_width).unwrap_or(u16::MAX),
            u16::try_from(self.screen_height).unwrap_or(u16::MAX),
            bar.height,
            layout,
        )
        .unwrap_or_else(|_| crate::fatal("published niri workspace capacity mismatch"));
        for window in 0..WINDOW_COUNT {
            if !self.windows[window].open {
                continue;
            }
            let workspace = niri
                .window_rules
                .workspace_for(app_id(window_kind(window)))
                .and_then(|name| niri.workspaces.index_of(name))
                .unwrap_or(0);
            workspaces
                .open_window(workspace, window as u32)
                .unwrap_or_else(|_| crate::fatal("published niri layout seed failed"));
        }
        workspaces
            .focus_workspace(old_workspace.min(workspace_count - 1))
            .unwrap_or_else(|_| crate::fatal("published niri active workspace failed"));
        if let Some(window) = preferred_window
            && workspaces.tile_rect(window).is_ok()
        {
            workspaces
                .focus_window(window)
                .unwrap_or_else(|_| crate::fatal("published niri focus restore failed"));
        }

        self.workspaces = workspaces;
        self.niri = niri;
        self.bar = bar;
        self.bar_style = bar_style;
        self.swww_defaults = swww_defaults;
        self.config_generation = sources.generation;
        self.sync_focused_window();
        crate::desktop_config::acknowledge(sources.generation);
        serialln(format_args!(
            "SLOPOS-CONFIG: reload applied generation={} atomic=true niri={} waybar={} style={} swww={} workspaces={} module_configs={} css_rules={}",
            sources.generation,
            sources.niri_path,
            sources.waybar_path,
            sources.waybar_style_path,
            sources.swww_path,
            self.workspaces.len(),
            self.bar.module_configs.len(),
            self.bar_style.len()
        ));
    }

    fn request_config_reload(&self) {
        let accepted = crate::desktop_config::request_reload();
        serialln(format_args!(
            "SLOPOS-CONFIG: reload requested generation={} accepted={}",
            crate::desktop_config::current_generation(),
            accepted
        ));
    }

    fn request_invalid_config_reload(&self) {
        let accepted = crate::desktop_config::request_invalid_reload();
        serialln(format_args!(
            "SLOPOS-CONFIG: invalid reload requested generation={} accepted={}",
            crate::desktop_config::current_generation(),
            accepted
        ));
    }

    fn bar_module_text(&self, module: &str) -> BarText {
        let blank = BarFormatValue {
            name: "",
            value: "",
        };
        let mut values = [blank; 4];
        let (default, value_count) = match module {
            "niri/workspaces" => {
                let label = workspace_label(self.workspaces.active(), self.workspaces.len());
                values[0] = BarFormatValue {
                    name: "value",
                    value: label,
                };
                values[1] = BarFormatValue {
                    name: "name",
                    value: self.active_workspace_name(),
                };
                values[2] = BarFormatValue {
                    name: "index",
                    value: small_number(self.workspaces.active() + 1),
                };
                values[3] = BarFormatValue {
                    name: "total",
                    value: small_number(self.workspaces.len()),
                };
                (label, 4)
            }
            "niri/window" => {
                let focused = self
                    .workspaces
                    .focused_window()
                    .map(|window| title(self.windows[window as usize].kind))
                    .unwrap_or("");
                values[0] = BarFormatValue {
                    name: "title",
                    value: focused,
                };
                (focused, 1)
            }
            "custom/launcher" => ("SLOPOS", 0),
            "network" => {
                values[0] = BarFormatValue {
                    name: "ifname",
                    value: "--",
                };
                ("NET --", 1)
            }
            "cpu" => {
                values[0] = BarFormatValue {
                    name: "usage",
                    value: "0",
                };
                ("CPU OK", 1)
            }
            "memory" => {
                values[0] = BarFormatValue {
                    name: "percentage",
                    value: "36",
                };
                ("MEM 36%", 1)
            }
            "clock" => ("UTC", 0),
            _ => (module, 0),
        };
        let module_config = self.bar.module_configs.get(module);
        let template = if module == "network" {
            module_config
                .and_then(|config| config.format_disconnected.or(config.format))
                .unwrap_or(default)
        } else {
            module_config
                .and_then(|config| config.format)
                .unwrap_or(default)
        };
        let mut text = format_bar_text(template, default, &values[..value_count])
            .unwrap_or_else(|_| crate::fatal("Waybar module format failed validation"));
        if let Some(maximum) = module_config.and_then(|config| config.max_length) {
            text.truncate(usize::from(maximum));
        }
        if let Some(minimum) = module_config.and_then(|config| config.min_length) {
            text.pad_to(usize::from(minimum))
                .unwrap_or_else(|_| crate::fatal("Waybar min-length exceeds fixed text buffer"));
        }
        text
    }

    fn render_window(&self, framebuffer: &mut Framebuffer, index: usize) {
        let Some(window) = self.positioned_window(index) else {
            return;
        };
        let active = self.workspaces.focused_window() == Some(index as u32);
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
        if self.workspaces.config().focus_ring.enabled {
            framebuffer.outline(
                window.x,
                window.y,
                window.width,
                window.height,
                if active {
                    i32::from(self.workspaces.config().focus_ring.width)
                } else {
                    1
                },
                if active {
                    self.workspaces.config().focus_ring.active_color
                } else {
                    self.workspaces.config().focus_ring.inactive_color
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

    fn keyboard(&mut self, event: KeyEvent) -> bool {
        if let Some(key) = binding_key(event.key)
            && let Some(action) = self
                .niri
                .bindings
                .action(binding_modifiers(event.modifiers), key)
        {
            self.execute_niri_action(action);
            return false;
        }
        if event.modifiers.logo || event.modifiers.control || event.modifiers.alt {
            return false;
        }
        match event.key {
            Key::Tab => self.focus_next(),
            Key::Escape => {
                self.scrolling_view = false;
            }
            Key::Backspace if self.terminal_focused() && self.command_length > 0 => {
                self.command_length -= 1;
            }
            Key::Enter if self.terminal_focused() => return self.execute_command(),
            Key::Character(character)
                if self.terminal_focused() && self.command_length < self.command.len() =>
            {
                self.command[self.command_length] = character.to_ascii_uppercase();
                self.command_length += 1;
            }
            _ => {}
        }
        false
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
            self.workspaces.scroll_by(-i32::from(event.dx));
            serialln(format_args!(
                "SLOPOS-SHELL: view scrolled workspace={} offset={} gesture=titlebar-drag",
                self.workspaces.active() + 1,
                self.workspaces.view_offset()
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
                    self.close_window(index);
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
                    self.request_config_reload();
                }
            }
            return;
        }
    }

    fn execute_command(&mut self) -> bool {
        let command_bytes = self.command;
        // SAFETY: PS/2 input writes only ASCII into the command buffer.
        let command =
            unsafe { core::str::from_utf8_unchecked(&command_bytes[..self.command_length]) };
        let mut animate = false;
        if command == "HELP" {
            self.set_response("HELP STATUS ABOUT CLEAR RELOAD [BAD] FAULT / SWWW ...")
        } else if command == "STATUS" {
            self.set_response("KERNEL OK / 3 NIRI COLUMNS / PS2 READY")
        } else if command == "ABOUT" {
            self.set_response("SLOPOS SCROLLING-TILE RUST SHELL")
        } else if command == "RELOAD BAD" {
            self.request_invalid_config_reload();
            self.set_response("INVALID CONFIG RELOAD REQUESTED")
        } else if command == "RELOAD" {
            self.request_config_reload();
            self.set_response("DESKTOP CONFIG RELOAD REQUESTED")
        } else if command == "FAULT" {
            crate::interrupts::trigger_page_fault()
        } else if command == "CLEAR" || command.is_empty() {
            self.set_response("")
        } else {
            match parse_swww_command(command, self.swww_defaults) {
                Ok(SwwwCommand::Daemon) => match self.wallpaper.start() {
                    Ok(()) => {
                        self.set_response("SWWW DAEMON STARTED");
                        serialln(format_args!("SLOPOS-SWWW: daemon started"));
                    }
                    Err(error) => self.set_response(swww_error(error)),
                },
                Ok(SwwwCommand::Img(request)) => {
                    if wallpaper_asset(request.path).is_none() {
                        self.set_response("SWWW IMAGE NOT IN EMBEDDED PNM REGISTRY");
                    } else {
                        match self.wallpaper.apply(request) {
                            Ok(()) => {
                                self.set_response("SWWW IMAGE APPLIED");
                                serialln(format_args!(
                                    "SLOPOS-SWWW: image={} output={} transition={} step={} fps={}",
                                    request.path,
                                    request.output.unwrap_or("*"),
                                    self.wallpaper.transition().kind.name(),
                                    self.wallpaper.transition().step,
                                    self.wallpaper.transition().fps
                                ));
                                animate = self.wallpaper.transition_active();
                            }
                            Err(error) => self.set_response(swww_error(error)),
                        }
                    }
                }
                Ok(SwwwCommand::Query) => match self.wallpaper.query() {
                    Ok(query) => {
                        let mut output = [0u8; 32];
                        let output_length = query.output.len().min(output.len());
                        output[..output_length]
                            .copy_from_slice(&query.output.as_bytes()[..output_length]);
                        let mut image = [0u8; 96];
                        let image_length = query.image.len().min(image.len());
                        image[..image_length]
                            .copy_from_slice(&query.image.as_bytes()[..image_length]);
                        let width = query.width;
                        let height = query.height;
                        serialln(format_args!(
                            "SLOPOS-SWWW: query output={} geometry={}x{} image={}",
                            query.output, query.width, query.height, query.image
                        ));
                        // SAFETY: both arrays contain exact copies of UTF-8 string prefixes.
                        let output =
                            unsafe { core::str::from_utf8_unchecked(&output[..output_length]) };
                        // SAFETY: the image array contains an exact copy of a UTF-8 path.
                        let image =
                            unsafe { core::str::from_utf8_unchecked(&image[..image_length]) };
                        self.set_query_response(output, width, height, image);
                    }
                    Err(error) => self.set_response(swww_error(error)),
                },
                Ok(SwwwCommand::Kill) => match self.wallpaper.kill() {
                    Ok(()) => {
                        self.set_response("SWWW DAEMON STOPPED");
                        serialln(format_args!("SLOPOS-SWWW: daemon stopped"));
                    }
                    Err(error) => self.set_response(swww_error(error)),
                },
                Err(_) => self.set_response("UNKNOWN COMMAND. TYPE HELP."),
            }
        }
        serialln(format_args!("SLOPOS-TERMINAL: command={command}"));
        self.command.fill(0);
        self.command_length = 0;
        animate
    }

    fn set_response(&mut self, response: &str) {
        self.response.fill(0);
        self.response_length = response.len().min(self.response.len());
        self.response[..self.response_length]
            .copy_from_slice(&response.as_bytes()[..self.response_length]);
    }

    fn append_response(&mut self, text: &str) {
        let available = self.response.len() - self.response_length;
        let length = text.len().min(available);
        self.response[self.response_length..self.response_length + length]
            .copy_from_slice(&text.as_bytes()[..length]);
        self.response_length += length;
    }

    fn append_response_number(&mut self, mut number: u16) {
        let mut digits = [0u8; 5];
        let mut start = digits.len();
        loop {
            start -= 1;
            digits[start] = b'0' + (number % 10) as u8;
            number /= 10;
            if number == 0 {
                break;
            }
        }
        // SAFETY: the temporary buffer contains ASCII decimal digits.
        self.append_response(unsafe { core::str::from_utf8_unchecked(&digits[start..]) });
    }

    fn set_query_response(&mut self, output: &str, width: u16, height: u16, image: &str) {
        let mut image_copy = [0u8; 96];
        let image_length = image.len().min(image_copy.len());
        image_copy[..image_length].copy_from_slice(&image.as_bytes()[..image_length]);
        self.set_response(output);
        self.append_response(" ");
        self.append_response_number(width);
        self.append_response("X");
        self.append_response_number(height);
        self.append_response(" IMAGE ");
        // SAFETY: bytes were copied from a UTF-8 path without changing them.
        self.append_response(unsafe {
            core::str::from_utf8_unchecked(&image_copy[..image_length])
        });
    }

    fn focus(&mut self, index: usize) {
        if !self.windows[index].open {
            return;
        }
        self.workspaces
            .focus_window(index as u32)
            .unwrap_or_else(|_| crate::fatal("layout focus lost a tiled window"));
        self.active = index;
    }

    fn execute_niri_action(&mut self, action: NiriAction) {
        let changed = match action {
            NiriAction::FocusColumnLeft => self.workspaces.focus_column_left(),
            NiriAction::FocusColumnRight => self.workspaces.focus_column_right(),
            NiriAction::FocusWorkspaceUp => self.workspaces.focus_workspace_up(),
            NiriAction::FocusWorkspaceDown => self.workspaces.focus_workspace_down(),
            NiriAction::MoveColumnToWorkspaceUp => {
                let active = self.workspaces.active();
                active > 0
                    && self
                        .workspaces
                        .move_focused_to_workspace(active - 1)
                        .unwrap_or_else(|_| crate::fatal("niri move-to-workspace-up failed"))
            }
            NiriAction::MoveColumnToWorkspaceDown => {
                let active = self.workspaces.active();
                active + 1 < self.workspaces.len()
                    && self
                        .workspaces
                        .move_focused_to_workspace(active + 1)
                        .unwrap_or_else(|_| crate::fatal("niri move-to-workspace-down failed"))
            }
            NiriAction::CloseWindow => {
                if let Some(window) = self.workspaces.focused_window() {
                    self.close_window(window as usize);
                    true
                } else {
                    false
                }
            }
        };
        self.sync_focused_window();
        serialln(format_args!(
            "SLOPOS-NIRI: binding action={} changed={} workspace={} name={} focused={}",
            action_name(action),
            changed,
            self.workspaces.active() + 1,
            self.active_workspace_name(),
            self.workspaces
                .focused_window()
                .map(|window| window as i32)
                .unwrap_or(-1)
        ));
    }

    fn close_window(&mut self, index: usize) {
        if !self.windows[index].open {
            return;
        }
        self.workspaces
            .close_window(index as u32)
            .unwrap_or_else(|_| crate::fatal("niri close lost a tiled window"));
        self.windows[index].open = false;
        serialln(format_args!(
            "SLOPOS-DESKTOP: window closed kind={} workspace={}",
            title(self.windows[index].kind),
            self.workspaces.active() + 1
        ));
        self.sync_focused_window();
    }

    fn terminal_focused(&self) -> bool {
        self.windows[0].open && self.workspaces.focused_window() == Some(0)
    }

    fn sync_focused_window(&mut self) {
        if let Some(window) = self.workspaces.focused_window() {
            self.active = window as usize;
        }
    }

    fn active_workspace_name(&self) -> &'static str {
        self.niri
            .workspaces
            .get(self.workspaces.active())
            .map(|workspace| workspace.name)
            .unwrap_or("<empty>")
    }

    fn focus_next(&mut self) {
        if self.workspaces.focus_column_right() {
            if let Some(window) = self.workspaces.focused_window() {
                self.active = window as usize;
            }
            return;
        }
        while self.workspaces.focus_column_left() {}
        if let Some(window) = self.workspaces.focused_window() {
            self.active = window as usize;
        }
    }

    fn positioned_window(&self, index: usize) -> Option<Window> {
        if !self.windows[index].open {
            return None;
        }
        let rect = self.workspaces.tile_rect(index as u32).ok()?;
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

const fn window_kind(index: usize) -> WindowKind {
    match index {
        0 => WindowKind::Terminal,
        1 => WindowKind::System,
        _ => WindowKind::Config,
    }
}

const fn app_id(kind: WindowKind) -> &'static str {
    match kind {
        WindowKind::Terminal => "slopos-terminal",
        WindowKind::System => "slopos-system",
        WindowKind::Config => "slopos-config",
    }
}

const fn binding_modifiers(modifiers: KeyModifiers) -> BindingModifiers {
    BindingModifiers::from_bits(
        (modifiers.logo as u8)
            | ((modifiers.control as u8) << 1)
            | ((modifiers.shift as u8) << 2)
            | ((modifiers.alt as u8) << 3),
    )
}

const fn binding_key(key: Key) -> Option<BindingKey> {
    Some(match key {
        Key::Left => BindingKey::Left,
        Key::Right => BindingKey::Right,
        Key::Up => BindingKey::Up,
        Key::Down => BindingKey::Down,
        Key::PageUp => BindingKey::PageUp,
        Key::PageDown => BindingKey::PageDown,
        Key::Enter => BindingKey::Return,
        Key::Tab => BindingKey::Tab,
        Key::Escape => BindingKey::Escape,
        Key::Character(character) => BindingKey::Character(character.to_ascii_uppercase()),
        Key::Backspace => return None,
    })
}

const fn action_name(action: NiriAction) -> &'static str {
    match action {
        NiriAction::FocusColumnLeft => "focus-column-left",
        NiriAction::FocusColumnRight => "focus-column-right",
        NiriAction::FocusWorkspaceUp => "focus-workspace-up",
        NiriAction::FocusWorkspaceDown => "focus-workspace-down",
        NiriAction::MoveColumnToWorkspaceUp => "move-column-to-workspace-up",
        NiriAction::MoveColumnToWorkspaceDown => "move-column-to-workspace-down",
        NiriAction::CloseWindow => "close-window",
    }
}

const fn workspace_label(active: usize, count: usize) -> &'static str {
    match (active, count) {
        (0, 1) => "[1]",
        (0, 2) => "[1] 2",
        (1, 2) => "1 [2]",
        (0, 3) => "[1] 2 3",
        (1, 3) => "1 [2] 3",
        (2, 3) => "1 2 [3]",
        (0, _) => "[1] 2 3 4",
        (1, _) => "1 [2] 3 4",
        (2, _) => "1 2 [3] 4",
        _ => "1 2 3 [4]",
    }
}

const fn small_number(value: usize) -> &'static str {
    match value {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        _ => "?",
    }
}

fn module_selector(module: &str) -> &'static str {
    match module {
        "niri/workspaces" => "#workspaces",
        "niri/window" => "#window",
        "custom/launcher" => "#custom-launcher",
        "network" => "#network",
        "cpu" => "#cpu",
        "memory" => "#memory",
        "clock" => "#clock",
        _ => "#module",
    }
}

fn inside(x: i32, y: i32, window: Window) -> bool {
    x >= window.x && x < window.x + window.width && y >= window.y && y < window.y + window.height
}

fn text_width(text: &str) -> i32 {
    i32::try_from(text.len()).unwrap_or(i32::MAX / 6) * 6
}

fn wallpaper_asset(path: &str) -> Option<PpmImage<'static>> {
    if path.eq_ignore_ascii_case(AURORA_PATH)
        || path.eq_ignore_ascii_case("aurora.ppm")
        || path.eq_ignore_ascii_case("slopos-aurora.ppm")
    {
        parse_ppm(AURORA_PPM).ok()
    } else if path.eq_ignore_ascii_case(SUNSET_PATH)
        || path.eq_ignore_ascii_case("sunset.ppm")
        || path.eq_ignore_ascii_case("slopos-sunset.ppm")
    {
        parse_ppm(SUNSET_PPM).ok()
    } else {
        None
    }
}

fn wallpaper_destination(
    resize: ResizeMode,
    image: PpmImage<'_>,
    output_width: i32,
    output_height: i32,
) -> (i32, i32, i32) {
    let image_width = i32::from(image.width());
    let image_height = i32::from(image.height());
    let scale = match resize {
        ResizeMode::Crop => {
            ceil_div(output_width, image_width).max(ceil_div(output_height, image_height))
        }
        ResizeMode::Fit => (output_width / image_width)
            .min(output_height / image_height)
            .max(1),
        ResizeMode::No => 1,
    };
    let width = image_width * scale;
    let height = image_height * scale;
    (
        (output_width - width) / 2,
        (output_height - height) / 2,
        scale,
    )
}

fn ceil_div(value: i32, divisor: i32) -> i32 {
    (value + divisor - 1) / divisor
}

fn swww_error(error: SwwwDaemonError) -> &'static str {
    match error {
        SwwwDaemonError::AlreadyRunning => "SWWW DAEMON ALREADY RUNNING",
        SwwwDaemonError::NotRunning => "SWWW DAEMON IS NOT RUNNING",
        SwwwDaemonError::InvalidPath => "SWWW IMAGE PATH IS INVALID",
        SwwwDaemonError::InvalidTransition => "SWWW TRANSITION IS INVALID",
        SwwwDaemonError::UnknownOutput => "SWWW OUTPUT IS UNKNOWN",
        SwwwDaemonError::NoImage => "SWWW OUTPUT HAS NO IMAGE",
    }
}
