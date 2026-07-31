// SPDX-License-Identifier: 0BSD

use crate::framebuffer::{
    BLACK, CYAN, Framebuffer, GREEN, INDIGO, MUTED, PANEL, RED, WHITE, WINDOW, WINDOW_ALT,
};
use crate::ps2::{Controller, DesktopEvent, InputEvent, Key, KeyEvent, KeyModifiers, MouseEvent};
use crate::serial::serialln;
use slopos_desktop_protocol::WALLPAPER_AURORA;
use slopos_shell::{
    BarButton, BarFormatValue, BarModuleList, BarPosition, BarText, BindingKey, BindingModifiers,
    ColumnDisplay, ImgRequest, MAX_NIRI_BINDINGS, NiriAction, NiriBinding, NiriShellConfig,
    PpmImage, ResizeMode, ResolvedWaybarStyle, SwwwCommand, SwwwDaemonError, SwwwDefaults,
    TransitionType, WallpaperDaemon, WaybarConfig, WaybarStyle, WorkspaceReference, WorkspaceSet,
    format_bar_text, parse_niri_layout, parse_niri_shell_config, parse_ppm, parse_swww_command,
    parse_swww_environment, parse_waybar_config, parse_waybar_style, transition_pixel,
};

const WINDOW_COUNT: usize = 3;
const WORKSPACE_CAPACITY: usize = 4;
const TITLE_HEIGHT: i32 = 30;
const BAR_LEFT_START_X: i32 = 47;
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
    workspaces: WorkspaceSet<WORKSPACE_CAPACITY, WINDOW_COUNT, WINDOW_COUNT>,
    niri: NiriShellConfig<'static>,
    bind_cooldown_until: [u64; MAX_NIRI_BINDINGS],
    bar: WaybarConfig<'static>,
    bar_style: WaybarStyle<'static>,
    bar_alternate_formats: u32,
    swww_defaults: SwwwDefaults,
    wallpaper: WallpaperDaemon,
    wallpaper_current_image: Option<PpmImage<'static>>,
    wallpaper_previous_image: Option<PpmImage<'static>>,
    wallpaper_generation: u64,
    active: usize,
    pointer_x: i32,
    pointer_y: i32,
    previous_buttons: u8,
    scrolling_view: bool,
    resizing_column: bool,
    command: [u8; 128],
    command_length: usize,
    response: [u8; 128],
    response_length: usize,
    alternate_theme: bool,
    config_generation: u64,
    service_generation: u64,
    provider_cpu_usage: u8,
    provider_memory_percentage: u8,
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
            niri.workspaces.len(),
            u16::try_from(width).unwrap_or(u16::MAX),
            u16::try_from(height).unwrap_or(u16::MAX),
            bar.height,
            config,
        )
        .unwrap_or_else(|_| crate::fatal("niri workspace capacity mismatch"));
        let mut opening_focus = None;
        for window in 0..WINDOW_COUNT {
            let app_id = app_id(window_kind(window));
            let workspace = niri
                .window_rules
                .workspace_for(app_id)
                .and_then(|name| niri.workspaces.index_of(name))
                .unwrap_or(0);
            let maximized_to_edges = niri
                .window_rules
                .maximized_to_edges_for(app_id)
                .unwrap_or(false);
            let fullscreen = niri.window_rules.fullscreen_for(app_id).unwrap_or(false);
            let open_focused = niri.window_rules.focused_for(app_id);
            let focus_ring = niri.window_rules.focus_ring_for(app_id, config.focus_ring);
            let opacity = niri.window_rules.opacity_for(app_id);
            let floating_position = niri.window_rules.floating_position_for(app_id);
            let floating =
                niri.window_rules.floating_for(app_id).unwrap_or(false) && !maximized_to_edges;
            let maximized = niri.window_rules.maximized_for(app_id).unwrap_or(false);
            let column_width = niri.window_rules.column_width_for(app_id);
            let window_height = niri.window_rules.window_height_for(app_id);
            let column_display = niri.window_rules.column_display_for(app_id);
            if floating {
                workspaces
                    .open_floating_window_with_properties(
                        workspace,
                        window as u32,
                        column_width,
                        window_height,
                        floating_position,
                        open_focused != Some(false),
                    )
                    .unwrap_or_else(|_| crate::fatal("niri floating seed capacity mismatch"));
                if let Some(position) = floating_position {
                    let rect = workspaces
                        .window_rect_in_workspace(workspace, window as u32)
                        .unwrap_or_else(|_| crate::fatal("niri floating position geometry failed"));
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=default-floating-position x={} y={} relative-to={} applied=true workspace={} window_x={} window_y={} width={} height={} source=config",
                        app_id,
                        position.x,
                        position.y,
                        position.relative_to.name(),
                        workspace + 1,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height
                    ));
                }
            } else {
                workspaces
                    .open_window_with_properties_and_focus(
                        workspace,
                        window as u32,
                        column_width,
                        window_height,
                        column_display,
                        open_focused != Some(false),
                    )
                    .unwrap_or_else(|_| crate::fatal("niri layout seed capacity mismatch"));
                if column_display == Some(ColumnDisplay::Tabbed) {
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=default-column-display value=tabbed applied=true workspace={} source=config",
                        app_id,
                        workspace + 1
                    ));
                }
                if maximized {
                    workspaces
                        .set_window_maximized(workspace, window as u32, true)
                        .unwrap_or_else(|_| crate::fatal("niri maximize seed failed"));
                    let rect = workspaces
                        .window_rect_in_workspace(workspace, window as u32)
                        .unwrap_or_else(|_| crate::fatal("niri maximize geometry failed"));
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=open-maximized value=true applied=true workspace={} x={} y={} width={} height={} mode=maximized-column source=config",
                        app_id,
                        workspace + 1,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height
                    ));
                }
                if maximized_to_edges {
                    workspaces
                        .set_window_maximized_to_edges(workspace, window as u32, true)
                        .unwrap_or_else(|_| crate::fatal("niri edge maximize seed failed"));
                    let rect = workspaces
                        .window_rect_in_workspace(workspace, window as u32)
                        .unwrap_or_else(|_| crate::fatal("niri edge maximize geometry failed"));
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=open-maximized-to-edges value=true applied=true workspace={} x={} y={} width={} height={} mode=maximized-to-edges source=config",
                        app_id,
                        workspace + 1,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height
                    ));
                }
            }
            if fullscreen {
                workspaces
                    .set_window_fullscreen(workspace, window as u32, true)
                    .unwrap_or_else(|_| crate::fatal("niri fullscreen seed failed"));
                let rect = workspaces
                    .window_rect_in_workspace(workspace, window as u32)
                    .unwrap_or_else(|_| crate::fatal("niri fullscreen geometry failed"));
                serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={} property=open-fullscreen value=true applied=true workspace={} x={} y={} width={} height={} mode=fullscreen source=config",
                    app_id,
                    workspace + 1,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                ));
            }
            if let Some(focus_ring) = focus_ring {
                serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={} property=focus-ring enabled={} width={} active={:#08x} inactive={:#08x} applied=true source=config",
                    app_id,
                    focus_ring.enabled,
                    focus_ring.width,
                    focus_ring.active_color,
                    focus_ring.inactive_color
                ));
            }
            if let Some(opacity) = opacity {
                serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={app_id} property=opacity value={opacity}/1000 applied=true fullscreen_ignored={fullscreen} source=config"
                ));
            }
            match open_focused {
                Some(true) => {
                    opening_focus = Some((workspace, window, app_id));
                }
                Some(false) => serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={} property=open-focused value=false applied=true workspace={} activated=false source=config",
                    app_id,
                    workspace + 1
                )),
                None => {}
            }
        }
        if let Some((workspace, window, app_id)) = opening_focus {
            workspaces
                .focus_workspace(workspace)
                .unwrap_or_else(|_| crate::fatal("niri opening workspace focus failed"));
            workspaces
                .focus_window(window as u32)
                .unwrap_or_else(|_| crate::fatal("niri opening window focus failed"));
            serialln(format_args!(
                "SLOPOS-NIRI: window rule app_id={} property=open-focused value=true applied=true workspace={} focused={} activated=true source=config",
                app_id,
                workspace + 1,
                window
            ));
        } else {
            workspaces
                .focus_window(0)
                .unwrap_or_else(|_| crate::fatal("niri layout terminal seed is missing"));
        }
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
            bind_cooldown_until: [0; MAX_NIRI_BINDINGS],
            bar,
            bar_style,
            bar_alternate_formats: 0,
            swww_defaults,
            wallpaper,
            wallpaper_current_image: None,
            wallpaper_previous_image: None,
            wallpaper_generation: 0,
            active: 0,
            pointer_x: width / 2,
            pointer_y: height / 2,
            previous_buttons: 0,
            scrolling_view: false,
            resizing_column: false,
            command: [0; 128],
            command_length: 0,
            response: [0; 128],
            response_length: 0,
            alternate_theme: false,
            config_generation: 0,
            service_generation: 0,
            provider_cpu_usage: 0,
            provider_memory_percentage: 0,
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
        serialln(format_args!(
            "SLOPOS-SWWW: daemon=running output=SLOPOS-1 geometry={}x{} image=awaiting-user-policy transition={} step={} fps={} policy_owner=user-service",
            width,
            height,
            desktop.wallpaper.transition().kind.name(),
            desktop.wallpaper.transition().step,
            desktop.wallpaper.transition().fps
        ));
        desktop
    }

    pub async fn run(&mut self, framebuffer: &mut Framebuffer, mut input: Controller) -> ! {
        loop {
            match crate::ps2::next_desktop_event(
                self.config_generation,
                self.service_generation,
                self.wallpaper_generation,
            )
            .await
            {
                DesktopEvent::ConfigUpdate(sources) => {
                    self.apply_config_update(sources);
                    self.render(framebuffer);
                }
                DesktopEvent::ServiceUpdate(snapshot) => {
                    self.apply_service_update(snapshot);
                    self.render(framebuffer);
                }
                DesktopEvent::WallpaperUpdate(update) => {
                    let generation = update.generation();
                    let (animate, applied) = self.apply_wallpaper_file_update(update);
                    if animate {
                        self.animate_wallpaper(framebuffer);
                    } else {
                        self.render(framebuffer);
                    }
                    self.wallpaper_generation = generation;
                    crate::wallpaper_file::acknowledge(generation, applied);
                    serialln(format_args!(
                        "SLOPOS-SWWW-VFS: result acknowledged generation={generation} renderer=desktop active_image={applied}"
                    ));
                }
                DesktopEvent::Input(byte) => {
                    if let Some(event) = input.consume(byte) {
                        let animate = match event {
                            InputEvent::Key(key) => self.keyboard(key),
                            InputEvent::Mouse(mouse) => self.mouse(mouse),
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
        if let Some(window) = self.workspaces.fullscreen_window() {
            self.render_window(framebuffer, window as usize);
            framebuffer.cursor(self.pointer_x, self.pointer_y);
            return;
        }
        self.render_bar(framebuffer);

        for index in 0..WINDOW_COUNT {
            if self.windows[index].open && !self.workspaces.window_is_floating(index as u32) {
                self.render_window(framebuffer, index);
            }
        }
        for z in 0..WINDOW_COUNT {
            if let Some(window) = self.workspaces.floating_window_at_z(z) {
                self.render_window(framebuffer, window as usize);
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
            self.wallpaper
                .clear_color()
                .unwrap_or(self.workspaces.config().background_color),
        );
        if self.wallpaper.current_image().is_none() {
            return;
        }
        let current = self
            .wallpaper_current_image
            .unwrap_or_else(|| crate::fatal("swww current image bytes are unavailable"));
        let previous = self.wallpaper_previous_image.unwrap_or(current);
        if previous.width() != current.width() || previous.height() != current.height() {
            crate::fatal("swww transition image dimensions differ");
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
        self.wallpaper_previous_image = None;
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

        let mut left_x = BAR_LEFT_START_X;
        for module in self.bar.modules_left.iter() {
            left_x += self.render_bar_module(framebuffer, module, left_x, baseline, bar_height)
                + i32::from(self.bar.spacing);
        }

        let center_width = self.bar_modules_width(self.bar.modules_center);
        let mut center_x = (self.screen_width - center_width) / 2;
        for module in self.bar.modules_center.iter() {
            center_x += self.render_bar_module(framebuffer, module, center_x, baseline, bar_height)
                + i32::from(self.bar.spacing);
        }

        let right_width = self.bar_modules_width(self.bar.modules_right);
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

    fn bar_modules_width(&self, modules: BarModuleList<'_>) -> i32 {
        modules
            .iter()
            .enumerate()
            .map(|(index, module)| {
                self.bar_module_width(module)
                    + if index == 0 {
                        0
                    } else {
                        i32::from(self.bar.spacing)
                    }
            })
            .sum()
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

    fn apply_service_update(&mut self, snapshot: crate::desktop_service::DesktopServiceSnapshot) {
        if !crate::desktop_service::snapshot_is_valid(snapshot)
            || snapshot.generation <= self.service_generation
        {
            crate::fatal("desktop service snapshot failed validation");
        }
        self.provider_cpu_usage = snapshot.cpu_usage;
        self.provider_memory_percentage = snapshot.memory_percentage;
        if snapshot.wallpaper == WALLPAPER_AURORA
            && self.wallpaper.current_image() != Some(AURORA_PATH)
        {
            let SwwwCommand::Img(request) = parse_swww_command(
                "swww img /usr/share/backgrounds/slopos-aurora.ppm",
                self.swww_defaults,
            )
            .unwrap_or_else(|_| crate::fatal("desktop service wallpaper command is invalid")) else {
                crate::fatal("desktop service wallpaper command changed kind");
            };
            let image = wallpaper_asset(request.path)
                .unwrap_or_else(|| crate::fatal("desktop service wallpaper asset disappeared"));
            self.apply_wallpaper_image(request, image)
                .unwrap_or_else(|_| crate::fatal("desktop service wallpaper policy failed"));
            serialln(format_args!(
                "SLOPOS-SWWW: policy applied owner_pid={} image={} output=SLOPOS-1 transition={} step={} fps={}",
                snapshot.owner_pid,
                AURORA_PATH,
                self.wallpaper.transition().kind.name(),
                self.wallpaper.transition().step,
                self.wallpaper.transition().fps
            ));
        }
        self.service_generation = snapshot.generation;
        serialln(format_args!(
            "SLOPOS-DESKTOP-SERVICE: policy applied generation={} owner_pid={} capabilities=waybar-provider/swww-policy cpu={} memory={} wallpaper={} renderer=kernel-mechanism",
            snapshot.generation,
            snapshot.owner_pid,
            snapshot.cpu_usage,
            snapshot.memory_percentage,
            AURORA_PATH
        ));
        crate::desktop_service::acknowledge_applied(snapshot.generation);
    }

    fn apply_wallpaper_image(
        &mut self,
        mut request: ImgRequest<'_>,
        image: PpmImage<'static>,
    ) -> Result<(), SwwwDaemonError> {
        let previous = self.wallpaper_current_image;
        if previous.is_some_and(|previous| {
            previous.width() != image.width() || previous.height() != image.height()
        }) && request.transition.kind != TransitionType::None
        {
            serialln(format_args!(
                "SLOPOS-SWWW: transition fallback requested={} applied=none reason=dimension-change new={}x{}",
                request.transition.kind.name(),
                image.width(),
                image.height()
            ));
            request.transition.kind = TransitionType::None;
            request.transition.step = u8::MAX;
        }
        self.wallpaper.apply(request)?;
        self.wallpaper_previous_image = previous;
        self.wallpaper_current_image = Some(image);
        if !self.wallpaper.transition_active() {
            self.wallpaper_previous_image = None;
        }
        Ok(())
    }

    fn apply_wallpaper_file_update(
        &mut self,
        update: crate::wallpaper_file::WallpaperFileUpdate,
    ) -> (bool, bool) {
        match update {
            crate::wallpaper_file::WallpaperFileUpdate::Ready(source) => {
                let image = parse_ppm(source.image)
                    .unwrap_or_else(|_| crate::fatal("published swww VFS PPM became invalid"));
                let request = ImgRequest {
                    path: source.request_path,
                    output: source.output,
                    transition: source.transition,
                };
                match self.apply_wallpaper_image(request, image) {
                    Ok(()) => {
                        self.set_response("SWWW VFS IMAGE APPLIED");
                        serialln(format_args!(
                            "SLOPOS-SWWW: image={} resolved={} source=vfs output={} transition={} step={} fps={}",
                            source.request_path,
                            source.resolved_path,
                            source.output.unwrap_or("*"),
                            self.wallpaper.transition().kind.name(),
                            self.wallpaper.transition().step,
                            self.wallpaper.transition().fps
                        ));
                        (self.wallpaper.transition_active(), true)
                    }
                    Err(error) => {
                        self.set_response(swww_error(error));
                        (false, false)
                    }
                }
            }
            crate::wallpaper_file::WallpaperFileUpdate::Failed {
                request_path,
                resolved_path,
                error,
                ..
            } => {
                self.set_response(wallpaper_file_error_response(error));
                serialln(format_args!(
                    "SLOPOS-SWWW: image={} resolved={} source=vfs applied=false error={}",
                    request_path,
                    resolved_path,
                    wallpaper_file_error_name(error)
                ));
                (false, false)
            }
        }
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
            niri.workspaces.len(),
            u16::try_from(self.screen_width).unwrap_or(u16::MAX),
            u16::try_from(self.screen_height).unwrap_or(u16::MAX),
            bar.height,
            layout,
        )
        .unwrap_or_else(|_| crate::fatal("published niri workspace capacity mismatch"));
        let mut opening_focus = None;
        for window in 0..WINDOW_COUNT {
            if !self.windows[window].open {
                continue;
            }
            let app_id = app_id(window_kind(window));
            let workspace = niri
                .window_rules
                .workspace_for(app_id)
                .and_then(|name| niri.workspaces.index_of(name))
                .unwrap_or(0);
            let maximized_to_edges = niri
                .window_rules
                .maximized_to_edges_for(app_id)
                .unwrap_or(false);
            let fullscreen = niri.window_rules.fullscreen_for(app_id).unwrap_or(false);
            let open_focused = niri.window_rules.focused_for(app_id);
            let focus_ring = niri.window_rules.focus_ring_for(app_id, layout.focus_ring);
            let opacity = niri.window_rules.opacity_for(app_id);
            let floating_position = niri.window_rules.floating_position_for(app_id);
            let floating = niri
                .window_rules
                .floating_for(app_id)
                .unwrap_or_else(|| self.workspaces.window_is_floating_anywhere(window as u32))
                && !maximized_to_edges;
            let maximized = niri.window_rules.maximized_for(app_id).unwrap_or(false);
            let column_width = niri.window_rules.column_width_for(app_id);
            let window_height = niri.window_rules.window_height_for(app_id);
            let column_display = niri.window_rules.column_display_for(app_id);
            if floating {
                workspaces
                    .open_floating_window_with_properties(
                        workspace,
                        window as u32,
                        column_width,
                        window_height,
                        floating_position,
                        open_focused != Some(false),
                    )
                    .unwrap_or_else(|_| crate::fatal("published niri floating seed failed"));
                if let Some(position) = floating_position {
                    let rect = workspaces
                        .window_rect_in_workspace(workspace, window as u32)
                        .unwrap_or_else(|_| {
                            crate::fatal("published niri floating position geometry failed")
                        });
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=default-floating-position x={} y={} relative-to={} applied=true workspace={} window_x={} window_y={} width={} height={} source=config",
                        app_id,
                        position.x,
                        position.y,
                        position.relative_to.name(),
                        workspace + 1,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height
                    ));
                }
            } else {
                workspaces
                    .open_window_with_properties_and_focus(
                        workspace,
                        window as u32,
                        column_width,
                        window_height,
                        column_display,
                        open_focused != Some(false),
                    )
                    .unwrap_or_else(|_| crate::fatal("published niri layout seed failed"));
                if column_display == Some(ColumnDisplay::Tabbed) {
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=default-column-display value=tabbed applied=true workspace={} source=config",
                        app_id,
                        workspace + 1
                    ));
                }
                if maximized {
                    workspaces
                        .set_window_maximized(workspace, window as u32, true)
                        .unwrap_or_else(|_| crate::fatal("published niri maximize seed failed"));
                    let rect = workspaces
                        .window_rect_in_workspace(workspace, window as u32)
                        .unwrap_or_else(|_| {
                            crate::fatal("published niri maximize geometry failed")
                        });
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=open-maximized value=true applied=true workspace={} x={} y={} width={} height={} mode=maximized-column source=config",
                        app_id,
                        workspace + 1,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height
                    ));
                }
                if maximized_to_edges {
                    workspaces
                        .set_window_maximized_to_edges(workspace, window as u32, true)
                        .unwrap_or_else(|_| {
                            crate::fatal("published niri edge maximize seed failed")
                        });
                    let rect = workspaces
                        .window_rect_in_workspace(workspace, window as u32)
                        .unwrap_or_else(|_| {
                            crate::fatal("published niri edge maximize geometry failed")
                        });
                    serialln(format_args!(
                        "SLOPOS-NIRI: window rule app_id={} property=open-maximized-to-edges value=true applied=true workspace={} x={} y={} width={} height={} mode=maximized-to-edges source=config",
                        app_id,
                        workspace + 1,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height
                    ));
                }
            }
            if fullscreen {
                workspaces
                    .set_window_fullscreen(workspace, window as u32, true)
                    .unwrap_or_else(|_| crate::fatal("published niri fullscreen seed failed"));
                let rect = workspaces
                    .window_rect_in_workspace(workspace, window as u32)
                    .unwrap_or_else(|_| crate::fatal("published niri fullscreen geometry failed"));
                serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={} property=open-fullscreen value=true applied=true workspace={} x={} y={} width={} height={} mode=fullscreen source=config",
                    app_id,
                    workspace + 1,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height
                ));
            }
            if let Some(focus_ring) = focus_ring {
                serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={} property=focus-ring enabled={} width={} active={:#08x} inactive={:#08x} applied=true source=config",
                    app_id,
                    focus_ring.enabled,
                    focus_ring.width,
                    focus_ring.active_color,
                    focus_ring.inactive_color
                ));
            }
            if let Some(opacity) = opacity {
                serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={app_id} property=opacity value={opacity}/1000 applied=true fullscreen_ignored={fullscreen} source=config"
                ));
            }
            match open_focused {
                Some(true) => {
                    opening_focus = Some((workspace, window, app_id));
                }
                Some(false) => serialln(format_args!(
                    "SLOPOS-NIRI: window rule app_id={} property=open-focused value=false applied=true workspace={} activated=false source=config",
                    app_id,
                    workspace + 1
                )),
                None => {}
            }
        }
        if let Some(window) = preferred_window {
            workspaces.focus_window_without_workspace_switch(window);
        }
        if let Some((workspace, window, app_id)) = opening_focus {
            workspaces
                .focus_workspace(workspace)
                .unwrap_or_else(|_| crate::fatal("published niri opening workspace focus failed"));
            workspaces
                .focus_window(window as u32)
                .unwrap_or_else(|_| crate::fatal("published niri opening window focus failed"));
            serialln(format_args!(
                "SLOPOS-NIRI: window rule app_id={} property=open-focused value=true applied=true workspace={} focused={} activated=true source=config",
                app_id,
                workspace + 1,
                window
            ));
        } else {
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
        }

        self.workspaces = workspaces;
        self.niri = niri;
        self.bind_cooldown_until = [0; MAX_NIRI_BINDINGS];
        self.bar = bar;
        self.bar_style = bar_style;
        self.bar_alternate_formats = 0;
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
        let cpu_usage = DecimalU8::new(self.provider_cpu_usage);
        let memory_percentage = DecimalU8::new(self.provider_memory_percentage);
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
                    value: cpu_usage.as_str(),
                };
                ("CPU OK", 1)
            }
            "memory" => {
                values[0] = BarFormatValue {
                    name: "percentage",
                    value: memory_percentage.as_str(),
                };
                ("MEM 36%", 1)
            }
            "clock" => ("UTC", 0),
            _ => (module, 0),
        };
        let module_config = self.bar.module_configs.get(module);
        let template = if self.bar_module_alternate_format_active(module) {
            module_config
                .and_then(|config| config.format_alt)
                .unwrap_or(default)
        } else if module == "network" {
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

    fn bar_module_alternate_format_active(&self, module: &str) -> bool {
        self.bar
            .module_configs
            .index_of(module)
            .is_some_and(|index| self.bar_alternate_formats & (1u32 << index) != 0)
    }

    fn toggle_bar_module_format(
        &mut self,
        module: &'static str,
        button: BarButton,
        button_name: &'static str,
    ) -> bool {
        let Some(config) = self.bar.module_configs.get(module) else {
            return false;
        };
        if config.format_alt.is_none() || config.format_alt_click != button {
            return false;
        }
        let Some(index) = self.bar.module_configs.index_of(module) else {
            crate::fatal("Waybar alternate format config lost its index");
        };
        self.bar_alternate_formats ^= 1u32 << index;
        let active = self.bar_module_alternate_format_active(module);
        let text = self.bar_module_text(module);
        serialln(format_args!(
            "SLOPOS-WAYBAR: format toggled name={module} button={button_name} alternate={active} text=\"{}\"",
            text.as_str()
        ));
        true
    }

    fn render_window(&self, framebuffer: &mut Framebuffer, index: usize) {
        let Some(window) = self.positioned_window(index) else {
            return;
        };
        if self.workspaces.window_is_fullscreen(index as u32) {
            framebuffer.rect(window.x, window.y, window.width, window.height, WINDOW_ALT);
            let mut content = window;
            content.y -= TITLE_HEIGHT + 2;
            match content.kind {
                WindowKind::Terminal => self.render_terminal(framebuffer, content),
                WindowKind::System => self.render_system(framebuffer, content),
                WindowKind::Config => self.render_config(framebuffer, content),
            }
            return;
        }
        let active = self.workspaces.focused_window() == Some(index as u32);
        let focus_ring = self
            .niri
            .window_rules
            .focus_ring_for(app_id(window.kind), self.workspaces.config().focus_ring)
            .unwrap_or(self.workspaces.config().focus_ring);
        let opacity = self
            .niri
            .window_rules
            .opacity_for(app_id(window.kind))
            .unwrap_or(1000);
        // The early renderer models niri's shadow as hard right/bottom strips.
        // Keep it outside the surface so an opacity rule reveals the wallpaper
        // rather than an opaque shadow rectangle hidden underneath the window.
        framebuffer.rect(
            window.x + window.width,
            window.y + 8,
            7,
            window.height,
            0x080a12,
        );
        framebuffer.rect(
            window.x + 7,
            window.y + window.height,
            window.width,
            8,
            0x080a12,
        );
        // Like niri's default draw-border-with-background mode, the focus
        // ring is compositor background: a translucent surface shows it
        // through rather than changing the ring's own opacity.
        if focus_ring.enabled {
            let width = if active {
                i32::from(focus_ring.width)
            } else {
                1
            };
            framebuffer.rect(
                window.x - width,
                window.y - width,
                window.width + width * 2,
                window.height + width * 2,
                if active {
                    focus_ring.active_color
                } else {
                    focus_ring.inactive_color
                },
            );
        }
        let previous_opacity = framebuffer.set_opacity(opacity);
        framebuffer.rect(window.x, window.y, window.width, window.height, WINDOW_ALT);
        framebuffer.rect(
            window.x,
            window.y,
            window.width,
            TITLE_HEIGHT,
            if active { self.accent() } else { WINDOW },
        );
        framebuffer.text(window.x + 12, window.y + 9, title(window.kind), WHITE, 1);
        framebuffer.rect(window.x + window.width - 26, window.y + 5, 20, 20, RED);
        framebuffer.text(window.x + window.width - 20, window.y + 11, "X", WHITE, 1);

        match window.kind {
            WindowKind::Terminal => self.render_terminal(framebuffer, window),
            WindowKind::System => self.render_system(framebuffer, window),
            WindowKind::Config => self.render_config(framebuffer, window),
        }
        framebuffer.set_opacity(previous_opacity);

        if let Some(info) = self.workspaces.tabbed_column_info(index as u32) {
            let tab_count = i32::try_from(info.tab_count).unwrap_or(i32::MAX).max(1);
            let indicator_gap = 3;
            let segment_height = window
                .height
                .saturating_sub(indicator_gap * (tab_count - 1))
                .checked_div(tab_count)
                .unwrap_or(1)
                .max(1);
            let mut y = window.y;
            for tab in 0..info.tab_count {
                framebuffer.rect(
                    window.x - 8,
                    y,
                    5,
                    segment_height,
                    if tab == info.active_tab {
                        self.accent()
                    } else {
                        focus_ring.inactive_color
                    },
                );
                y += segment_height + indicator_gap;
            }
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
        framebuffer.text(x + 84, y + 164, "PID 1/2 SERVICES", GREEN, 1);
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
        let modifiers = binding_modifiers(event.modifiers);
        if let Some(key) = binding_key(event.key)
            && let Some((index, binding)) = self.niri.bindings.binding(modifiers, key)
        {
            self.execute_niri_binding(index, binding, modifiers, "keyboard", None);
            return false;
        }
        if event.modifiers.logo || event.modifiers.control || event.modifiers.alt {
            return false;
        }
        match event.key {
            Key::Tab => self.focus_next(),
            Key::Escape => {
                self.scrolling_view = false;
                self.resizing_column = false;
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

    fn mouse(&mut self, event: MouseEvent) -> bool {
        self.pointer_x = (self.pointer_x + event.dx as i32).clamp(0, self.screen_width - 1);
        self.pointer_y = (self.pointer_y + event.dy as i32).clamp(0, self.screen_height - 1);
        let left = event.buttons & 1 != 0;
        let left_was_down = self.previous_buttons & 1 != 0;
        let right = event.buttons & 2 != 0;
        let right_was_down = self.previous_buttons & 2 != 0;
        let middle = event.buttons & 4 != 0;
        let middle_was_down = self.previous_buttons & 4 != 0;
        let mut animate = false;

        if left && !left_was_down {
            animate = self.pointer_pressed();
        } else if !left {
            self.scrolling_view = false;
        }
        if right && !right_was_down {
            if event.modifiers.logo {
                self.pointer_resize_pressed();
            } else {
                animate |= self.pointer_right_pressed();
            }
        } else if !right {
            self.resizing_column = false;
        }
        if middle && !middle_was_down {
            animate |= self.pointer_middle_pressed();
        }
        if event.wheel != 0 {
            animate |= self.pointer_scrolled(event.wheel, event.modifiers);
        }

        if left && self.scrolling_view && (event.dx != 0 || event.dy != 0) {
            if self.workspaces.focused_window_is_floating() {
                let changed = self
                    .workspaces
                    .move_focused_floating(i32::from(event.dx), i32::from(event.dy));
                if changed && let Some(window) = self.positioned_window(self.active) {
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: window moved kind={} x={} y={} layout=floating gesture=titlebar-drag",
                        title(window.kind),
                        window.x,
                        window.y
                    ));
                }
            } else if event.dx != 0 {
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
        }
        if right && self.resizing_column && (event.dx != 0 || event.dy != 0) {
            let floating = self.workspaces.focused_window_is_floating();
            let changed = if floating {
                self.workspaces
                    .resize_focused_floating(i32::from(event.dx), i32::from(event.dy))
            } else {
                self.workspaces
                    .change_focused_column_width(slopos_shell::ColumnWidthChange::AdjustFixed(
                        i32::from(event.dx),
                    ))
                    .unwrap_or_else(|_| crate::fatal("pointer column resize failed"))
            };
            self.sync_focused_window();
            if changed && let Some(window) = self.positioned_window(self.active) {
                if floating {
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: pointer resized kind={} width={} height={} delta={}/{} layout=floating gesture=mod-right-drag",
                        title(window.kind),
                        window.width,
                        window.height,
                        event.dx,
                        event.dy
                    ));
                } else {
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: pointer resized kind={} width={} delta={} gesture=mod-right-drag",
                        title(window.kind),
                        window.width,
                        event.dx
                    ));
                }
            }
        }
        self.previous_buttons = event.buttons;
        animate
    }

    fn pointer_pressed(&mut self) -> bool {
        if let Some(workspace) = self.bar_workspace_at(self.pointer_x, self.pointer_y) {
            let changed = self
                .workspaces
                .focus_workspace(workspace)
                .unwrap_or_else(|_| crate::fatal("Waybar workspace click selected invalid index"));
            self.sync_focused_window();
            serialln(format_args!(
                "SLOPOS-WAYBAR: workspace clicked index={} name={} changed={} module=niri/workspaces",
                workspace + 1,
                self.active_workspace_name(),
                changed
            ));
            return false;
        }
        if let Some(module) = self.bar_module_at(self.pointer_x, self.pointer_y) {
            self.toggle_bar_module_format(module, BarButton::Left, "left");
            if let Some(action) = self
                .bar
                .module_configs
                .get(module)
                .and_then(|config| config.on_click)
            {
                return self.execute_bar_action(module, action, "left");
            }
            return false;
        }
        if let Some(index) = self.window_at_pointer() {
            let window = self
                .positioned_window(index)
                .expect("pointer-selected window remains positioned");
            self.focus(index);
            if self.workspaces.window_is_fullscreen(index as u32) {
                return false;
            }
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
            return false;
        }
        false
    }

    fn pointer_right_pressed(&mut self) -> bool {
        let Some(module) = self.bar_module_at(self.pointer_x, self.pointer_y) else {
            return false;
        };
        self.toggle_bar_module_format(module, BarButton::Right, "right");
        let Some(action) = self
            .bar
            .module_configs
            .get(module)
            .and_then(|config| config.on_click_right)
        else {
            return false;
        };
        self.execute_bar_action(module, action, "right")
    }

    fn pointer_middle_pressed(&mut self) -> bool {
        let Some(module) = self.bar_module_at(self.pointer_x, self.pointer_y) else {
            return false;
        };
        self.toggle_bar_module_format(module, BarButton::Middle, "middle");
        let Some(action) = self
            .bar
            .module_configs
            .get(module)
            .and_then(|config| config.on_click_middle)
        else {
            return false;
        };
        self.execute_bar_action(module, action, "middle")
    }

    fn pointer_scrolled(&mut self, wheel: i8, modifiers: KeyModifiers) -> bool {
        let (key, direction) = if wheel > 0 {
            (BindingKey::WheelScrollUp, "up")
        } else {
            (BindingKey::WheelScrollDown, "down")
        };
        let modifiers = binding_modifiers(modifiers);
        if let Some((index, binding)) = self.niri.bindings.binding(modifiers, key) {
            self.execute_niri_binding(
                index,
                binding,
                modifiers,
                "ps2-intellimouse",
                Some(direction),
            );
            return false;
        }

        let Some(module) = self.bar_module_at(self.pointer_x, self.pointer_y) else {
            return false;
        };
        let Some(config) = self.bar.module_configs.get(module) else {
            return false;
        };
        let (action, button) = if wheel > 0 {
            (config.on_scroll_up, "scroll-up")
        } else {
            (config.on_scroll_down, "scroll-down")
        };
        action.is_some_and(|action| self.execute_bar_action(module, action, button))
    }

    fn execute_niri_binding(
        &mut self,
        index: usize,
        binding: NiriBinding<'static>,
        modifiers: BindingModifiers,
        source: &'static str,
        wheel_direction: Option<&'static str>,
    ) {
        let now = crate::timer::ticks();
        let cooldown_ms = binding.cooldown_ms.unwrap_or(0);
        let deadline = self.bind_cooldown_until[index];
        if cooldown_ms != 0 && now < deadline {
            let remaining_ms = deadline.saturating_sub(now).saturating_mul(10);
            if let Some(direction) = wheel_direction {
                serialln(format_args!(
                    "SLOPOS-NIRI: wheel binding direction={} modifiers={:#x} action={} source={} accepted=false cooldown_ms={} remaining_ms={}",
                    direction,
                    modifiers.bits(),
                    action_name(binding.action),
                    source,
                    cooldown_ms,
                    remaining_ms
                ));
            } else {
                serialln(format_args!(
                    "SLOPOS-NIRI: binding modifiers={:#x} action={} source={} accepted=false cooldown_ms={} remaining_ms={}",
                    modifiers.bits(),
                    action_name(binding.action),
                    source,
                    cooldown_ms,
                    remaining_ms
                ));
            }
            return;
        }
        let cooldown_ticks = u64::from(cooldown_ms).div_ceil(10);
        self.bind_cooldown_until[index] = now.saturating_add(cooldown_ticks);
        if let Some(direction) = wheel_direction {
            serialln(format_args!(
                "SLOPOS-NIRI: wheel binding direction={} modifiers={:#x} action={} source={} accepted=true cooldown_ms={}",
                direction,
                modifiers.bits(),
                action_name(binding.action),
                source,
                cooldown_ms
            ));
        } else if cooldown_ms != 0 {
            serialln(format_args!(
                "SLOPOS-NIRI: binding modifiers={:#x} action={} source={} accepted=true cooldown_ms={}",
                modifiers.bits(),
                action_name(binding.action),
                source,
                cooldown_ms
            ));
        }
        self.execute_niri_action(binding.action);
    }

    fn bar_workspace_at(&self, x: i32, y: i32) -> Option<usize> {
        if self.workspaces.fullscreen_window().is_some() || y < 0 || y >= i32::from(self.bar.height)
        {
            return None;
        }
        let center_width = self.bar_modules_width(self.bar.modules_center);
        let right_width = self.bar_modules_width(self.bar.modules_right);
        self.bar_workspace_in_modules(self.bar.modules_left, BAR_LEFT_START_X, x)
            .or_else(|| {
                self.bar_workspace_in_modules(
                    self.bar.modules_center,
                    (self.screen_width - center_width) / 2,
                    x,
                )
            })
            .or_else(|| {
                self.bar_workspace_in_modules(
                    self.bar.modules_right,
                    self.screen_width - right_width - 12,
                    x,
                )
            })
    }

    fn bar_workspace_in_modules(
        &self,
        modules: BarModuleList<'_>,
        mut module_x: i32,
        x: i32,
    ) -> Option<usize> {
        for module in modules.iter() {
            let module_width = self.bar_module_width(module);
            if module == "niri/workspaces" {
                let text = self.bar_module_text(module);
                let style = self.bar_module_style(module);
                let workspace_label =
                    workspace_label(self.workspaces.active(), self.workspaces.len());
                let Some(label_offset) = text.as_str().find(workspace_label) else {
                    module_x += module_width + i32::from(self.bar.spacing);
                    continue;
                };
                let label_x =
                    module_x + i32::from(style.margin_left) + i32::from(style.padding_left);
                let label_x = label_x
                    + i32::try_from(label_offset)
                        .unwrap_or(i32::MAX / 6)
                        .saturating_mul(6);
                let relative = x - label_x;
                if relative >= 0 && relative < text_width(workspace_label) {
                    let byte_index = usize::try_from(relative / 6).ok()?;
                    let digit = *workspace_label.as_bytes().get(byte_index)?;
                    if (b'1'..=b'4').contains(&digit) {
                        let workspace = usize::from(digit - b'1');
                        return (workspace < self.workspaces.len()).then_some(workspace);
                    }
                }
            }
            module_x += module_width + i32::from(self.bar.spacing);
        }
        None
    }

    fn bar_module_at(&self, x: i32, y: i32) -> Option<&'static str> {
        if self.workspaces.fullscreen_window().is_some() || y < 0 || y >= i32::from(self.bar.height)
        {
            return None;
        }
        let center_width = self.bar_modules_width(self.bar.modules_center);
        let right_width = self.bar_modules_width(self.bar.modules_right);
        self.bar_module_in_modules(self.bar.modules_left, BAR_LEFT_START_X, x)
            .or_else(|| {
                self.bar_module_in_modules(
                    self.bar.modules_center,
                    (self.screen_width - center_width) / 2,
                    x,
                )
            })
            .or_else(|| {
                self.bar_module_in_modules(
                    self.bar.modules_right,
                    self.screen_width - right_width - 12,
                    x,
                )
            })
    }

    fn bar_module_in_modules(
        &self,
        modules: BarModuleList<'static>,
        mut module_x: i32,
        x: i32,
    ) -> Option<&'static str> {
        for module in modules.iter() {
            let text = self.bar_module_text(module);
            let style = self.bar_module_style(module);
            let box_x = module_x + i32::from(style.margin_left);
            let box_width = i32::from(style.padding_left)
                + text_width(text.as_str())
                + i32::from(style.padding_right);
            if x >= box_x && x < box_x + box_width {
                return Some(module);
            }
            module_x += self.bar_module_width(module) + i32::from(self.bar.spacing);
        }
        None
    }

    fn pointer_resize_pressed(&mut self) {
        if let Some(index) = self.window_at_pointer() {
            if self.workspaces.window_is_fullscreen(index as u32) {
                return;
            }
            self.focus(index);
            self.resizing_column = true;
        }
    }

    fn window_at_pointer(&self) -> Option<usize> {
        for z in (0..WINDOW_COUNT).rev() {
            let Some(window) = self.workspaces.floating_window_at_z(z) else {
                continue;
            };
            let index = window as usize;
            if self
                .positioned_window(index)
                .is_some_and(|window| inside(self.pointer_x, self.pointer_y, window))
            {
                return Some(index);
            }
        }
        (0..WINDOW_COUNT).rev().find(|index| {
            !self.workspaces.window_is_floating(*index as u32)
                && self
                    .positioned_window(*index)
                    .is_some_and(|window| inside(self.pointer_x, self.pointer_y, window))
        })
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
                        self.wallpaper_current_image = None;
                        self.wallpaper_previous_image = None;
                        self.set_response("SWWW DAEMON STARTED");
                        serialln(format_args!("SLOPOS-SWWW: daemon started"));
                    }
                    Err(error) => self.set_response(swww_error(error)),
                },
                Ok(SwwwCommand::Img(request)) => {
                    if let Some(image) = wallpaper_asset(request.path) {
                        match self.apply_wallpaper_image(request, image) {
                            Ok(()) => {
                                self.set_response("SWWW IMAGE APPLIED");
                                serialln(format_args!(
                                    "SLOPOS-SWWW: image={} output={} transition={} step={} fps={} source=embedded",
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
                    } else if !self.wallpaper.is_running() {
                        self.set_response(swww_error(SwwwDaemonError::NotRunning));
                    } else if request.output.is_some_and(|output| {
                        output != "*" && !output.eq_ignore_ascii_case("SLOPOS-1")
                    }) {
                        self.set_response(swww_error(SwwwDaemonError::UnknownOutput));
                    } else {
                        match crate::wallpaper_file::request(request) {
                            Ok(generation) => {
                                self.set_response("SWWW VFS IMAGE LOAD REQUESTED");
                                serialln(format_args!(
                                    "SLOPOS-SWWW-VFS: load requested generation={generation} request={} output={} transition={} step={} fps={} async=true",
                                    request.path,
                                    request.output.unwrap_or("*"),
                                    request.transition.kind.name(),
                                    request.transition.step,
                                    request.transition.fps
                                ));
                            }
                            Err(error) => self.set_response(wallpaper_file_request_error(error)),
                        }
                    }
                }
                Ok(SwwwCommand::Clear(request)) => match self.wallpaper.clear(request) {
                    Ok(()) => {
                        self.wallpaper_current_image = None;
                        self.wallpaper_previous_image = None;
                        self.set_response("SWWW COLOR APPLIED");
                        serialln(format_args!(
                            "SLOPOS-SWWW: clear color={:06X} output={}",
                            request.color,
                            request.output.unwrap_or("*")
                        ));
                    }
                    Err(error) => self.set_response(swww_error(error)),
                },
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
                        self.wallpaper_current_image = None;
                        self.wallpaper_previous_image = None;
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

    fn execute_bar_action(
        &mut self,
        module: &'static str,
        action: &'static str,
        button: &'static str,
    ) -> bool {
        let saved_command = self.command;
        let saved_length = self.command_length;
        self.command.fill(0);
        self.command_length = action.len();
        for (destination, source) in self.command.iter_mut().zip(action.bytes()) {
            *destination = source.to_ascii_uppercase();
        }
        let allowed = matches!(
            &self.command[..self.command_length],
            b"HELP" | b"STATUS" | b"ABOUT" | b"CLEAR" | b"RELOAD" | b"SWWW-DAEMON"
        ) || self.command[..self.command_length].starts_with(b"SWWW ");
        let animate = if allowed {
            self.execute_command()
        } else {
            self.set_response("WAYBAR ACTION UNSUPPORTED");
            false
        };
        self.command = saved_command;
        self.command_length = saved_length;
        serialln(format_args!(
            "SLOPOS-WAYBAR: module clicked name={module} button={button} action={action} accepted={allowed} animate={animate}"
        ));
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

    fn execute_niri_action(&mut self, action: NiriAction<'static>) {
        let focused_window = self
            .workspaces
            .focused_window()
            .and_then(|window| usize::try_from(window).ok())
            .filter(|window| *window < WINDOW_COUNT);
        let rule_column_display = focused_window.and_then(|window| {
            self.niri
                .window_rules
                .column_display_for(app_id(window_kind(window)))
        });
        let rule_floating_position = focused_window.and_then(|window| {
            self.niri
                .window_rules
                .floating_position_for(app_id(window_kind(window)))
        });
        let floating_position_was_remembered = focused_window.is_some_and(|window| {
            self.workspaces
                .floating_position_is_remembered(window as u32)
        });
        let blocked_by_fullscreen = self.workspaces.fullscreen_window().is_some()
            && !matches!(
                action,
                NiriAction::FullscreenWindow
                    | NiriAction::CloseWindow
                    | NiriAction::FocusWorkspaceUp
                    | NiriAction::FocusWorkspaceDown
                    | NiriAction::FocusWorkspacePrevious
                    | NiriAction::FocusWorkspace(_)
                    | NiriAction::MoveWorkspaceUp
                    | NiriAction::MoveWorkspaceDown
            );
        let changed = if blocked_by_fullscreen {
            false
        } else {
            match action {
                NiriAction::FocusColumnLeft => self.workspaces.focus_column_left(),
                NiriAction::FocusColumnRight => self.workspaces.focus_column_right(),
                NiriAction::FocusColumnFirst => self.workspaces.focus_column_first(),
                NiriAction::FocusColumnLast => self.workspaces.focus_column_last(),
                NiriAction::FocusWindowUp => self.workspaces.focus_window_up(),
                NiriAction::FocusWindowDown => self.workspaces.focus_window_down(),
                NiriAction::MoveColumnLeft => self.workspaces.move_column_left(),
                NiriAction::MoveColumnRight => self.workspaces.move_column_right(),
                NiriAction::MoveColumnToFirst => self.workspaces.move_column_to_first(),
                NiriAction::MoveColumnToLast => self.workspaces.move_column_to_last(),
                NiriAction::MoveWindowUp => self.workspaces.move_window_up(),
                NiriAction::MoveWindowDown => self.workspaces.move_window_down(),
                NiriAction::FocusWorkspaceUp => self.workspaces.focus_workspace_up(),
                NiriAction::FocusWorkspaceDown => self.workspaces.focus_workspace_down(),
                NiriAction::FocusWorkspacePrevious => self.workspaces.focus_workspace_previous(),
                NiriAction::FocusWorkspace(reference) => {
                    let workspace = self.resolve_workspace_reference(reference);
                    self.workspaces
                        .focus_workspace(workspace)
                        .unwrap_or_else(|_| crate::fatal("niri focus-workspace failed"))
                }
                NiriAction::MoveWorkspaceUp => self.workspaces.move_workspace_up(),
                NiriAction::MoveWorkspaceDown => self.workspaces.move_workspace_down(),
                NiriAction::MoveColumnToWorkspaceUp => {
                    let active = self.workspaces.active();
                    active > 0
                        && self
                            .workspaces
                            .move_focused_column_to_workspace(active - 1)
                            .unwrap_or_else(|_| crate::fatal("niri move-to-workspace-up failed"))
                }
                NiriAction::MoveColumnToWorkspaceDown => {
                    let active = self.workspaces.active();
                    active + 1 < self.workspaces.len()
                        && self
                            .workspaces
                            .move_focused_column_to_workspace(active + 1)
                            .unwrap_or_else(|_| crate::fatal("niri move-to-workspace-down failed"))
                }
                NiriAction::MoveColumnToWorkspace(reference) => {
                    let workspace = self.resolve_workspace_reference(reference);
                    self.workspaces
                        .move_focused_column_to_workspace(workspace)
                        .unwrap_or_else(|_| crate::fatal("niri move-to-workspace failed"))
                }
                NiriAction::MoveWindowToWorkspaceUp => {
                    let active = self.workspaces.active();
                    active > 0
                        && self
                            .workspaces
                            .move_focused_window_to_workspace_with_display(
                                active - 1,
                                rule_column_display,
                            )
                            .unwrap_or_else(|_| {
                                crate::fatal("niri move-window-to-workspace-up failed")
                            })
                }
                NiriAction::MoveWindowToWorkspaceDown => {
                    let active = self.workspaces.active();
                    active + 1 < self.workspaces.len()
                        && self
                            .workspaces
                            .move_focused_window_to_workspace_with_display(
                                active + 1,
                                rule_column_display,
                            )
                            .unwrap_or_else(|_| {
                                crate::fatal("niri move-window-to-workspace-down failed")
                            })
                }
                NiriAction::MoveWindowToWorkspace(reference) => {
                    let workspace = self.resolve_workspace_reference(reference);
                    self.workspaces
                        .move_focused_window_to_workspace_with_display(
                            workspace,
                            rule_column_display,
                        )
                        .unwrap_or_else(|_| crate::fatal("niri move-window-to-workspace failed"))
                }
                NiriAction::ConsumeWindowIntoColumn => self.workspaces.consume_window_into_column(),
                NiriAction::ExpelWindowFromColumn => self
                    .workspaces
                    .expel_window_from_column_with_display(rule_column_display),
                NiriAction::ConsumeOrExpelWindowLeft => self
                    .workspaces
                    .consume_or_expel_focused_window_left_with_display(rule_column_display),
                NiriAction::ConsumeOrExpelWindowRight => self
                    .workspaces
                    .consume_or_expel_focused_window_right_with_display(rule_column_display),
                NiriAction::ToggleColumnTabbedDisplay => {
                    self.workspaces.toggle_focused_column_tabbed_display()
                }
                NiriAction::ToggleWindowFloating => self
                    .workspaces
                    .toggle_focused_window_floating_with_properties(
                        rule_column_display,
                        rule_floating_position,
                    ),
                NiriAction::SwitchFocusBetweenFloatingAndTiling => {
                    self.workspaces.switch_focus_between_floating_and_tiling()
                }
                NiriAction::MoveWindowToFloating => self
                    .workspaces
                    .move_focused_window_to_floating_with_position(rule_floating_position),
                NiriAction::MoveWindowToTiling => self
                    .workspaces
                    .move_focused_window_to_tiling_with_display(rule_column_display),
                NiriAction::FocusFloating => self.workspaces.focus_floating(),
                NiriAction::FocusTiling => self.workspaces.focus_tiling(),
                NiriAction::SwitchPresetColumnWidth => self.workspaces.switch_preset_column_width(),
                NiriAction::SwitchPresetColumnWidthBack => {
                    self.workspaces.switch_preset_column_width_back()
                }
                NiriAction::SwitchPresetWindowHeight => {
                    self.workspaces.switch_preset_window_height()
                }
                NiriAction::SwitchPresetWindowHeightBack => {
                    self.workspaces.switch_preset_window_height_back()
                }
                NiriAction::MaximizeColumn => self
                    .workspaces
                    .maximize_focused_column_with_display(rule_column_display),
                NiriAction::FullscreenWindow => self.workspaces.toggle_focused_window_fullscreen(),
                NiriAction::MaximizeWindowToEdges => self
                    .workspaces
                    .maximize_focused_window_to_edges_with_display(rule_column_display),
                NiriAction::CenterColumn => self.workspaces.center_focused_column(),
                NiriAction::CenterVisibleColumns => self.workspaces.center_visible_columns(),
                NiriAction::ExpandColumnToAvailableWidth => {
                    self.workspaces.expand_focused_column_to_available_width()
                }
                NiriAction::SetColumnWidth(change) => self
                    .workspaces
                    .change_focused_column_width(change)
                    .unwrap_or_else(|_| crate::fatal("niri set-column-width failed")),
                NiriAction::SetWindowHeight(change) => self
                    .workspaces
                    .change_focused_window_height(change)
                    .unwrap_or_else(|_| crate::fatal("niri set-window-height failed")),
                NiriAction::ResetWindowHeight => self.workspaces.reset_focused_window_height(),
                NiriAction::CloseWindow => {
                    if let Some(window) = self.workspaces.focused_window() {
                        self.close_window(window as usize);
                        true
                    } else {
                        false
                    }
                }
            }
        };
        if changed
            && matches!(
                action,
                NiriAction::MoveColumnToWorkspaceUp
                    | NiriAction::MoveColumnToWorkspaceDown
                    | NiriAction::MoveColumnToWorkspace(_)
                    | NiriAction::MoveWindowToWorkspaceUp
                    | NiriAction::MoveWindowToWorkspaceDown
                    | NiriAction::MoveWindowToWorkspace(_)
            )
        {
            self.normalize_dynamic_workspaces(
                if matches!(
                    action,
                    NiriAction::MoveWindowToWorkspaceUp
                        | NiriAction::MoveWindowToWorkspaceDown
                        | NiriAction::MoveWindowToWorkspace(_)
                ) {
                    "move-window"
                } else {
                    "move-column"
                },
            );
        }
        self.sync_focused_window();
        if changed
            && matches!(
                action,
                NiriAction::MoveWorkspaceUp | NiriAction::MoveWorkspaceDown
            )
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: workspace reordered action={} workspace={} name={} previous={} focused={} layout=niri",
                action_name(action),
                self.workspaces.active() + 1,
                self.active_workspace_name(),
                self.workspaces.previous() + 1,
                self.workspaces
                    .focused_window()
                    .map(|window| window as i32)
                    .unwrap_or(-1)
            ));
        }
        if changed && matches!(action, NiriAction::FullscreenWindow) {
            self.scrolling_view = false;
            self.resizing_column = false;
        }
        if changed
            && matches!(action, NiriAction::FullscreenWindow)
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: fullscreen toggled state={} kind={} restore_layer={} x={} y={} width={} height={} bar={} layout=niri",
                if self.workspaces.window_is_fullscreen(self.active as u32) {
                    "active"
                } else {
                    "inactive"
                },
                title(window.kind),
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "tiling"
                },
                window.x,
                window.y,
                window.width,
                window.height,
                if self.workspaces.fullscreen_window().is_some() {
                    "covered"
                } else {
                    "visible"
                }
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::MoveColumnToWorkspaceUp
                    | NiriAction::MoveColumnToWorkspaceDown
                    | NiriAction::MoveColumnToWorkspace(_)
                    | NiriAction::MoveWindowToWorkspaceUp
                    | NiriAction::MoveWindowToWorkspaceDown
                    | NiriAction::MoveWindowToWorkspace(_)
            )
        {
            let scope = if matches!(
                action,
                NiriAction::MoveWindowToWorkspaceUp
                    | NiriAction::MoveWindowToWorkspaceDown
                    | NiriAction::MoveWindowToWorkspace(_)
            ) {
                "window"
            } else {
                "column"
            };
            for index in 0..self.windows.len() {
                if let Some(window) = self.positioned_window(index) {
                    serialln(format_args!(
                        "SLOPOS-DESKTOP: workspace transfer scope={} action={} member={} workspace={} name={} x={} y={} width={} height={} layout=niri",
                        scope,
                        action_name(action),
                        title(window.kind),
                        self.workspaces.active() + 1,
                        self.active_workspace_name(),
                        window.x,
                        window.y,
                        window.width,
                        window.height
                    ));
                }
            }
        }
        if changed
            && matches!(
                action,
                NiriAction::SetColumnWidth(_)
                    | NiriAction::SwitchPresetColumnWidth
                    | NiriAction::SwitchPresetColumnWidthBack
                    | NiriAction::MaximizeColumn
                    | NiriAction::ExpandColumnToAvailableWidth
            )
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: window resized kind={} width={} layout={}",
                title(window.kind),
                window.width,
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "scrolling"
                }
            ));
        }
        if changed
            && matches!(action, NiriAction::MaximizeWindowToEdges)
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: window edge maximize toggled kind={} x={} y={} width={} height={} layout=scrolling",
                title(window.kind),
                window.x,
                window.y,
                window.width,
                window.height
            ));
        }
        if changed
            && matches!(action, NiriAction::CenterColumn)
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: column centered kind={} x={} offset={} layout={}",
                title(window.kind),
                window.x,
                self.workspaces.view_offset(),
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "scrolling"
                }
            ));
        }
        if changed
            && matches!(action, NiriAction::CenterVisibleColumns)
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: visible columns centered kind={} x={} offset={} layout=scrolling",
                title(window.kind),
                window.x,
                self.workspaces.view_offset()
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::SetWindowHeight(_)
                    | NiriAction::ResetWindowHeight
                    | NiriAction::SwitchPresetWindowHeight
                    | NiriAction::SwitchPresetWindowHeightBack
            )
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: window height changed kind={} height={} layout={}",
                title(window.kind),
                window.height,
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "scrolling"
                }
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::MoveColumnLeft
                    | NiriAction::MoveColumnRight
                    | NiriAction::MoveColumnToFirst
                    | NiriAction::MoveColumnToLast
            )
            && !self.workspaces.focused_window_is_floating()
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: column reordered kind={} x={} direction={} layout=scrolling",
                title(window.kind),
                window.x,
                action_name(action)
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::MoveColumnLeft
                    | NiriAction::MoveColumnRight
                    | NiriAction::MoveWindowUp
                    | NiriAction::MoveWindowDown
            )
            && self.workspaces.focused_window_is_floating()
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: floating window moved kind={} x={} y={} direction={} layout=floating",
                title(window.kind),
                window.x,
                window.y,
                action_name(action)
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::ConsumeOrExpelWindowLeft | NiriAction::ConsumeOrExpelWindowRight
            )
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: window consume-or-expel kind={} direction={} x={} y={} width={} height={} layout=scrolling",
                title(window.kind),
                if matches!(action, NiriAction::ConsumeOrExpelWindowLeft) {
                    "left"
                } else {
                    "right"
                },
                window.x,
                window.y,
                window.width,
                window.height
            ));
        }
        if changed
            && matches!(action, NiriAction::ToggleColumnTabbedDisplay)
            && let Some(window) = self.positioned_window(self.active)
            && let Some(info) = self.workspaces.tabbed_column_info(self.active as u32)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: column display toggled mode=tabbed kind={} tab={}/{} x={} y={} width={} height={} layout=scrolling",
                title(window.kind),
                info.active_tab + 1,
                info.tab_count,
                window.x,
                window.y,
                window.width,
                window.height
            ));
        } else if changed
            && matches!(action, NiriAction::ToggleColumnTabbedDisplay)
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: column display toggled mode=normal kind={} x={} y={} width={} height={} layout=scrolling",
                title(window.kind),
                window.x,
                window.y,
                window.width,
                window.height
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::FocusWindowUp | NiriAction::FocusWindowDown
            )
            && let Some(window) = self.positioned_window(self.active)
            && let Some(info) = self.workspaces.tabbed_column_info(self.active as u32)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: tab focused kind={} tab={}/{} direction={} layout=scrolling",
                title(window.kind),
                info.active_tab + 1,
                info.tab_count,
                action_name(action)
            ));
        }
        if changed
            && matches!(action, NiriAction::ToggleWindowFloating)
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: window layer toggled kind={} layer={} x={} y={} width={} height={} layout=niri",
                title(window.kind),
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "tiling"
                },
                window.x,
                window.y,
                window.width,
                window.height
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::ToggleWindowFloating | NiriAction::MoveWindowToFloating
            )
            && self.workspaces.focused_window_is_floating()
            && let Some(position) = rule_floating_position
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-NIRI: window rule app_id={} property=default-floating-position x={} y={} relative-to={} applied={} remembered={} window_x={} window_y={} width={} height={} transition={} source=config",
                app_id(window.kind),
                position.x,
                position.y,
                position.relative_to.name(),
                !floating_position_was_remembered,
                floating_position_was_remembered,
                window.x,
                window.y,
                window.width,
                window.height,
                action_name(action)
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::MoveWindowToFloating | NiriAction::MoveWindowToTiling
            )
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: window layer moved action={} kind={} layer={} x={} y={} width={} height={} layout=niri",
                action_name(action),
                title(window.kind),
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "tiling"
                },
                window.x,
                window.y,
                window.width,
                window.height
            ));
        }
        if changed
            && matches!(
                action,
                NiriAction::SwitchFocusBetweenFloatingAndTiling
                    | NiriAction::FocusFloating
                    | NiriAction::FocusTiling
            )
            && let Some(window) = self.positioned_window(self.active)
        {
            serialln(format_args!(
                "SLOPOS-DESKTOP: layer focus switched layer={} kind={} layout=niri action={}",
                if self.workspaces.focused_window_is_floating() {
                    "floating"
                } else {
                    "tiling"
                },
                title(window.kind),
                action_name(action)
            ));
        }
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
        match action {
            NiriAction::FocusWorkspace(reference)
            | NiriAction::MoveColumnToWorkspace(reference)
            | NiriAction::MoveWindowToWorkspace(reference) => match reference {
                WorkspaceReference::Index(index) => serialln(format_args!(
                    "SLOPOS-NIRI: workspace target action={} kind=index value={}",
                    action_name(action),
                    index
                )),
                WorkspaceReference::Name(name) => serialln(format_args!(
                    "SLOPOS-NIRI: workspace target action={} kind=name value={}",
                    action_name(action),
                    name
                )),
            },
            _ => {}
        }
    }

    fn resolve_workspace_reference(&self, reference: WorkspaceReference<'_>) -> usize {
        match reference {
            WorkspaceReference::Index(workspace) => usize::from(workspace)
                .saturating_sub(1)
                .min(self.workspaces.len() - 1),
            WorkspaceReference::Name(name) => {
                let identity = self
                    .niri
                    .workspaces
                    .index_of(name)
                    .and_then(|identity| u8::try_from(identity).ok())
                    .unwrap_or_else(|| crate::fatal("validated niri workspace name disappeared"));
                self.workspaces
                    .workspace_for_identity(identity)
                    .unwrap_or_else(|| crate::fatal("named niri workspace identity disappeared"))
            }
        }
    }

    fn close_window(&mut self, index: usize) {
        if !self.windows[index].open {
            return;
        }
        self.workspaces
            .close_window(index as u32)
            .unwrap_or_else(|_| crate::fatal("niri close lost a tiled window"));
        self.windows[index].open = false;
        self.normalize_dynamic_workspaces("close-window");
        serialln(format_args!(
            "SLOPOS-DESKTOP: window closed kind={} workspace={}",
            title(self.windows[index].kind),
            self.workspaces.active() + 1
        ));
        self.sync_focused_window();
    }

    fn normalize_dynamic_workspaces(&mut self, reason: &'static str) {
        let before = self.workspaces.len();
        if self
            .workspaces
            .normalize_dynamic(self.niri.workspaces.len())
            .unwrap_or_else(|_| crate::fatal("niri dynamic workspace normalization failed"))
        {
            let after = self.workspaces.len();
            let trailing_empty = self
                .workspaces
                .workspace_is_empty(after - 1)
                .unwrap_or_else(|_| crate::fatal("niri trailing workspace disappeared"));
            serialln(format_args!(
                "SLOPOS-NIRI: dynamic workspaces reason={} count={}->{} named={} active={} trailing_empty={}",
                reason,
                before,
                after,
                self.niri.workspaces.len(),
                self.workspaces.active() + 1,
                trailing_empty
            ));
        }
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
        self.workspaces
            .workspace_identity(self.workspaces.active())
            .unwrap_or_else(|_| crate::fatal("active niri workspace disappeared"))
            .and_then(|identity| self.niri.workspaces.get(usize::from(identity)))
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
        if !self.windows[index].open || !self.workspaces.window_is_visible(index as u32) {
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
        Key::Home => BindingKey::Home,
        Key::End => BindingKey::End,
        Key::Enter => BindingKey::Return,
        Key::Tab => BindingKey::Tab,
        Key::Escape => BindingKey::Escape,
        Key::Character(b'-' | b'_') => BindingKey::Minus,
        Key::Character(b'=' | b'+') => BindingKey::Equal,
        Key::Character(b'!') => BindingKey::Character(b'1'),
        Key::Character(b'@') => BindingKey::Character(b'2'),
        Key::Character(b'#') => BindingKey::Character(b'3'),
        Key::Character(b'$') => BindingKey::Character(b'4'),
        Key::Character(b'%') => BindingKey::Character(b'5'),
        Key::Character(b'^') => BindingKey::Character(b'6'),
        Key::Character(b'&') => BindingKey::Character(b'7'),
        Key::Character(b'*') => BindingKey::Character(b'8'),
        Key::Character(b'(') => BindingKey::Character(b'9'),
        Key::Character(b')') => BindingKey::Character(b'0'),
        Key::Character(character) => BindingKey::Character(character.to_ascii_uppercase()),
        Key::Backspace => return None,
    })
}

const fn action_name(action: NiriAction<'_>) -> &'static str {
    match action {
        NiriAction::FocusColumnLeft => "focus-column-left",
        NiriAction::FocusColumnRight => "focus-column-right",
        NiriAction::FocusColumnFirst => "focus-column-first",
        NiriAction::FocusColumnLast => "focus-column-last",
        NiriAction::FocusWindowUp => "focus-window-up",
        NiriAction::FocusWindowDown => "focus-window-down",
        NiriAction::MoveColumnLeft => "move-column-left",
        NiriAction::MoveColumnRight => "move-column-right",
        NiriAction::MoveColumnToFirst => "move-column-to-first",
        NiriAction::MoveColumnToLast => "move-column-to-last",
        NiriAction::MoveWindowUp => "move-window-up",
        NiriAction::MoveWindowDown => "move-window-down",
        NiriAction::FocusWorkspaceUp => "focus-workspace-up",
        NiriAction::FocusWorkspaceDown => "focus-workspace-down",
        NiriAction::FocusWorkspacePrevious => "focus-workspace-previous",
        NiriAction::FocusWorkspace(_) => "focus-workspace",
        NiriAction::MoveWorkspaceUp => "move-workspace-up",
        NiriAction::MoveWorkspaceDown => "move-workspace-down",
        NiriAction::MoveColumnToWorkspaceUp => "move-column-to-workspace-up",
        NiriAction::MoveColumnToWorkspaceDown => "move-column-to-workspace-down",
        NiriAction::MoveColumnToWorkspace(_) => "move-column-to-workspace",
        NiriAction::MoveWindowToWorkspaceUp => "move-window-to-workspace-up",
        NiriAction::MoveWindowToWorkspaceDown => "move-window-to-workspace-down",
        NiriAction::MoveWindowToWorkspace(_) => "move-window-to-workspace",
        NiriAction::ConsumeWindowIntoColumn => "consume-window-into-column",
        NiriAction::ExpelWindowFromColumn => "expel-window-from-column",
        NiriAction::ConsumeOrExpelWindowLeft => "consume-or-expel-window-left",
        NiriAction::ConsumeOrExpelWindowRight => "consume-or-expel-window-right",
        NiriAction::ToggleColumnTabbedDisplay => "toggle-column-tabbed-display",
        NiriAction::ToggleWindowFloating => "toggle-window-floating",
        NiriAction::SwitchFocusBetweenFloatingAndTiling => {
            "switch-focus-between-floating-and-tiling"
        }
        NiriAction::MoveWindowToFloating => "move-window-to-floating",
        NiriAction::MoveWindowToTiling => "move-window-to-tiling",
        NiriAction::FocusFloating => "focus-floating",
        NiriAction::FocusTiling => "focus-tiling",
        NiriAction::SwitchPresetColumnWidth => "switch-preset-column-width",
        NiriAction::SwitchPresetColumnWidthBack => "switch-preset-column-width-back",
        NiriAction::SwitchPresetWindowHeight => "switch-preset-window-height",
        NiriAction::SwitchPresetWindowHeightBack => "switch-preset-window-height-back",
        NiriAction::MaximizeColumn => "maximize-column",
        NiriAction::FullscreenWindow => "fullscreen-window",
        NiriAction::MaximizeWindowToEdges => "maximize-window-to-edges",
        NiriAction::CenterColumn => "center-column",
        NiriAction::CenterVisibleColumns => "center-visible-columns",
        NiriAction::ExpandColumnToAvailableWidth => "expand-column-to-available-width",
        NiriAction::SetColumnWidth(_) => "set-column-width",
        NiriAction::SetWindowHeight(_) => "set-window-height",
        NiriAction::ResetWindowHeight => "reset-window-height",
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

struct DecimalU8 {
    bytes: [u8; 3],
    start: usize,
}

impl DecimalU8 {
    fn new(value: u8) -> Self {
        let mut bytes = [b'0'; 3];
        let start = if value >= 100 {
            bytes[0] += value / 100;
            bytes[1] += (value / 10) % 10;
            bytes[2] += value % 10;
            0
        } else if value >= 10 {
            bytes[1] += value / 10;
            bytes[2] += value % 10;
            1
        } else {
            bytes[2] += value;
            2
        };
        Self { bytes, start }
    }

    fn as_str(&self) -> &str {
        // SAFETY: the constructor emits only ASCII decimal digits.
        unsafe { core::str::from_utf8_unchecked(&self.bytes[self.start..]) }
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

fn wallpaper_file_request_error(
    error: crate::wallpaper_file::WallpaperFileRequestError,
) -> &'static str {
    match error {
        crate::wallpaper_file::WallpaperFileRequestError::Busy => "SWWW VFS IMAGE LOAD IS BUSY",
        crate::wallpaper_file::WallpaperFileRequestError::InvalidPath => {
            "SWWW VFS IMAGE PATH IS INVALID"
        }
    }
}

const fn wallpaper_file_error_response(
    error: crate::wallpaper_file::WallpaperFileError,
) -> &'static str {
    match error {
        crate::wallpaper_file::WallpaperFileError::InvalidPath => "SWWW VFS IMAGE PATH IS INVALID",
        crate::wallpaper_file::WallpaperFileError::NotFound => "SWWW VFS IMAGE NOT FOUND",
        crate::wallpaper_file::WallpaperFileError::FileTooLarge => "SWWW VFS IMAGE EXCEEDS 8K",
        crate::wallpaper_file::WallpaperFileError::InvalidUtf8 => "SWWW VFS IMAGE IS NOT ASCII P3",
        crate::wallpaper_file::WallpaperFileError::InvalidPpm => "SWWW VFS IMAGE P3 IS INVALID",
    }
}

const fn wallpaper_file_error_name(
    error: crate::wallpaper_file::WallpaperFileError,
) -> &'static str {
    match error {
        crate::wallpaper_file::WallpaperFileError::InvalidPath => "invalid-path",
        crate::wallpaper_file::WallpaperFileError::NotFound => "not-found",
        crate::wallpaper_file::WallpaperFileError::FileTooLarge => "file-size",
        crate::wallpaper_file::WallpaperFileError::InvalidUtf8 => "invalid-utf8",
        crate::wallpaper_file::WallpaperFileError::InvalidPpm => "invalid-ppm",
    }
}
