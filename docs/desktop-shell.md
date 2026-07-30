# 滚动桌面兼容方向

SlopOS 桌面以 niri、Waybar 和 swww 的用户习惯作为兼容目标，但不会把尚未实现的协议或选项标成已兼容。当前代码首先复现可独立测试的状态机与配置语义，再逐步替换 early kernel surfaces。

## VFS 配置发现与原子重载

桌面构造时先用编译进 kernel 的同源 asset 作为 bootstrap；ext4 root mount 完成后，block task 从 VFS 读取四份文本并发布 generation 1，desktop task 再把整套状态一次替换。root image 把仓库默认配置安装在 `/etc/slopos/`，但发现顺序优先兼容常见位置：

- niri：`/home/slop/.config/niri/config.kdl`、`/etc/niri/config.kdl`、`/etc/slopos/niri.kdl`；
- Waybar JSONC：`/home/slop/.config/waybar/config.jsonc`、同目录 `config`、`/etc/xdg/waybar/config.jsonc`、同目录 `config`、`/etc/slopos/waybar.jsonc`；
- Waybar CSS：`/home/slop/.config/waybar/style.css`、`/etc/xdg/waybar/style.css`、`/etc/slopos/waybar.css`；
- swww environment：`/home/slop/.config/swww/env`、`/etc/swww/env`、`/etc/slopos/swww.env`。

前三份上限各为 4096 bytes，swww environment 上限为 512 bytes；每份都必须是非空 UTF-8。block task 是唯一 writer，在 inactive static bank 中读齐四份文本，先验证 niri layout/shell、Waybar JSONC/top position、Waybar CSS 与 swww environment，再用 release/acquire generation 发布。desktop task 在 local value 中再次 parse，重建 workspace 状态并尽量保留窗口、当前 workspace 与 focus，全部成功后才 swap 并 acknowledge；双 bank 因此不会暴露半套新配置或覆写仍被 renderer 引用的字符串。

Config surface 的按钮或图形 monitor 的 `RELOAD` 命令会唤醒 block task 并触发运行时 VFS 重读。缺失、超长、非法 UTF-8、parse 错误或 early renderer 不支持的非 top Waybar position 都保留上一代。`make test-interaction` 先确认 generation 1→2 的完整 reload，再以仅供诊断的 `RELOAD BAD` 注入非法 CSS，确认拒绝并保持 generation 2；这证明 request/reload/rollback 路径，不是文件 watcher 或 inotify。当前没有内建配置编辑器，也没有自动监听磁盘变更。

## niri 式滚动平铺

`crates/shell` 提供无分配 `ScrollLayout`。当前已经成立的行为：

- 窗口位于向右延伸的 column strip；新窗口插在 focused column 右侧，不改变既有 column width；
- column 超出 output 后只移动 viewport，支持向左/右 focus、edge reveal、手动 scroll 与 titlebar horizontal drag；
- 一个 column 可纵向包含多个 window，up/down focus 不改变 column width；
- close 最后一个 window 会删除 column 并修正 focus/view；
- 支持 fixed、proportional 与 client-selected width；
- 支持 `never`、`always`、`on-overflow` 三种 focused-column centering，以及 single-column centering。

KDL parser 当前从 `assets/niri-config.kdl` 读取以下 niri 同名子集：

```kdl
workspace "main"
workspace "config" { open-on-output "SLOPOS-1" }

binds {
    Mod+Left { focus-column-left; }
    Mod+Down { focus-workspace-down; }
    Mod+Shift+Down { move-column-to-workspace-down; }
    Mod+Q { close-window; }
}

window-rule {
    match app-id="slopos-config"
    open-on-workspace "config"
}

layout {
    gaps 16
    center-focused-column "never"
    always-center-single-column
    default-column-width { proportion 0.5; }
    background-color "#101426"

    focus-ring {
        on
        width 3
        active-color "#7fc8ff"
        inactive-color "#505050"
    }
}
```

parser 也接受 `fixed N`、空 `default-column-width {}`、小数 gap、`#rrggbb`/`#rrggbbaa`，并跳过 full config 中尚未消费的其他 top-level/nested node。workspace 状态机为每个 workspace 保留独立 column strip；named workspace 后附加一个空 workspace。window rule 按出现顺序叠加，后匹配规则可覆盖先前的 `open-on-workspace`。bind chord 支持 `Mod`、`Ctrl`、`Shift`、`Alt` 与方向键、PageUp/PageDown、Return、Tab、Escape、单字符；当前 action 集为 focus column/workspace、move column to workspace 与 close window。

8 项 layout 测试和 3 项 workspace/bind/rule 测试覆盖配置拒绝边界、open/focus/scroll/stack/close、稳定列宽、workspace switch/move 与规则顺序。设计语义依据 [niri 默认配置](https://github.com/YaLTeR/niri/blob/main/resources/default-config.kdl)、[Layout 配置文档](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Layout)、[Key Bindings](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Key-Bindings) 与 [Window Rules](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Window-Rules)。

当前 kernel desktop 用三个固定 surface 演示这些行为：Terminal/System 位于 `main`，Config 的 `app-id` 规则把它放到 `config`；顶部 workspace module 显示 active index。PS/2 parser 跟踪 Super/Ctrl/Shift/Alt 与扩展方向键，使 KDL bind 实际驱动桌面。niri 配置已按上述 user/system/fallback 顺序从 VFS 加载，并能整套原子重读；尚未实现动态 workspace 创建/销毁、完整 niri action/XKB 命名、multi-output、floating/tabbed column、复杂 match、animation、overview、IPC、自动文件监听、Wayland surface 或普通用户 client。

## Waybar 式顶部栏

当前画面使用 top bar，并按 left/center/right 三个区域显示 workspace、focused title 与 system status。root VFS 中选中的 Waybar JSONC 已实际决定 `position`、`height`、`spacing` 和三个 module array；仓库默认源是 `assets/waybar-config.jsonc`。kernel 依 array 顺序查询 module registry 并计算对齐位置。

JSONC parser 支持 `//` 与 `/* */` comment、trailing comma、最多 16 个 module/区域、24 个 module config。module object 当前保存 `format`、`format-alt`、`format-disconnected`、`interval`、`tooltip`、`min-length`、`max-length`，跳过未知 nested option，并拒绝 duplicate、非法类型/范围与冲突长度。format renderer 支持 `{}`、named replacement 与 `:>N` 右对齐；当前 provider 实际提供 `{value/name/index/total}`、`{title}`、`{usage}`、`{percentage}`、`{ifname}`。

VFS 中选中的 CSS 使用 Waybar 同样的 GTK CSS selector 命名；仓库默认源是 `assets/waybar-style.css`。无分配 parser 支持 `*`、`window#waybar` 和 module `#id` 的 source-order cascade、逗号 selector list，以及 `color`、`background[-color]`、`padding`、`margin`、`border-bottom: Npx solid #rrggbb`；`transparent` background 和 1/2/3/4-value px box shorthand 可用。renderer 将样式纳入左右/居中宽度计算，当前截图中的 CPU/Memory/Clock 色块、padding 和 bar 底边框都来自 CSS。字段与 selector 依据 [Waybar 官方 `config.jsonc`](https://github.com/Alexays/Waybar/blob/master/resources/config.jsonc)、[默认 `style.css`](https://github.com/Alexays/Waybar/blob/master/resources/style.css) 与 [niri/workspaces module manual](https://github.com/Alexays/Waybar/blob/master/man/waybar-niri-workspaces.5.scd)。

4 项 JSONC/format 与 3 项 CSS 测试覆盖 parse、format replacement、cascade、transparent、box shorthand 和拒绝边界。JSONC/CSS 已从 VFS 成对参与原子 generation reload。当前 registry 仍只有 `niri/workspaces`、`niri/window`、`custom/launcher`、`network`、`cpu`、`memory`、`clock` 的固定 kernel provider；interval 被验证并保留为 provider 更新策略，但 early provider 没有真实 network/CPU/RTC polling。尚无 Pango markup/strftime、format-icons/state、完整 GTK CSS/alpha blend、click/scroll action、per-output bar、tray、network/audio/battery backend 或 niri IPC module。parser 接受 bottom/left/right position，但 early framebuffer renderer 当前只允许 top，并在发布 VFS generation 前拒绝其他位置。

## swww 式壁纸控制

`crates/shell` 已提供无分配 swww 风格 CLI parser 与 `WallpaperDaemon` 状态机。kernel 启动时等价执行 `swww-daemon` 和 `swww img /usr/share/backgrounds/slopos-aurora.ppm`；图形 monitor 接受带或不带 `swww` 前缀的命令：

- `img <path>` 设置图片，可选 output；
- `query` 返回 output geometry 与当前 image；
- `kill` 停止 daemon；
- `swww-daemon` 在 kill 后重新启动并清空旧 image；
- `--outputs/-o`、`--resize crop|fit|no`、`--transition-type`、`--transition-step`、`--transition-fps`、`--transition-duration`、`--transition-angle`；
- VFS 中发现的 environment 文件以同名 `SWWW_TRANSITION*` 变量提供 boot/reload 默认值，仓库默认源是 `assets/swww.env`；
- `none`、`simple`、`fade`、`left/right/top/bottom`、`center/outer`、`any/random` transition。

两个 12×8 P3/PNM asset 在启动时完整校验 header、尺寸、max value、component 范围和精确 pixel 数。renderer 实际把 current/previous image 逐像素 blend 或 mask 到 GOP；交互测试通过 PS/2 输入切到 Sunset，完成 5 个 center 采样帧，由 `query` 读回 `SLOPOS-1`、1024×768 和当前路径，再验证 kill/restart 与 `none` 重设。7 项 swww/PNM、11 项 niri layout/shell 与 7 项 Waybar JSONC/CSS 测试，共 25 项。

命令与 transition 语义依据 [swww 官方 README](https://github.com/LGFae/swww)。environment 默认值已参与四文件 VFS 原子重载，但当前 daemon 不是独立用户进程或 Unix socket，也没有 Wayland layer-shell、多 output、从 VFS 解码任意图片路径、PNG/JPEG/GIF decode、animated image cache、frame callback/timing、transition position/bezier/wave/grow 或 damage tracking。同步 framebuffer renderer 为限制最坏 CPU 时间，会把极小 step 最多采样成 17 帧，因此不声称二进制或动画时序完全兼容 swww。
