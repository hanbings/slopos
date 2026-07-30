# 滚动桌面兼容方向

SlopOS 桌面以 niri、Waybar 和 swww 的用户习惯作为兼容目标，但不会把尚未实现的协议或选项标成已兼容。当前代码首先复现可独立测试的状态机与配置语义，再逐步替换 early kernel surfaces。

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

parser 也接受 `fixed N`、空 `default-column-width {}`、小数 gap、`#rrggbb`/`#rrggbbaa`，并跳过 full config 中尚未消费的其他 top-level/nested node。8 项 layout 测试覆盖配置拒绝边界、open/focus/scroll/stack/close 和稳定列宽。设计语义依据 [niri 默认配置](https://github.com/YaLTeR/niri/blob/main/resources/default-config.kdl)、[Layout 配置文档](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Layout) 与 [Design Principles](https://github.com/YaLTeR/niri/wiki/Development%3A-Design-Principles)。

当前 kernel desktop 用三个固定 surface 作为三列，Tab 沿列切换，鼠标标题栏横拖滚动 strip。尚未实现 workspace、多 output、floating/tabbed column、window rule、bind、animation、overview、IPC、live reload、Wayland surface 或普通用户 client。配置也尚未按 niri 的 `$XDG_CONFIG_HOME/niri/config.kdl` → `~/.config/niri/config.kdl` → `/etc/niri/config.kdl` 顺序从 VFS 加载。

## Waybar 式顶部栏

当前画面使用 top bar，并按 left/center/right 三个区域显示 workspace、focused title 与 system status。`assets/waybar-config.jsonc` 已实际决定 `position`、`height`、`spacing` 和三个 module array；kernel 依 array 顺序查询 module registry 并计算对齐位置。

JSONC parser 支持 `//` 与 `/* */` comment、trailing comma、未知 nested module option、最多 16 个 module/区域，并拒绝 duplicate top-level field、非法 position/number/module。3 项 parser 测试与 8 项 layout 测试由同一个 `make test-shell` 执行。字段与默认结构依据 [Waybar 官方 `config.jsonc`](https://github.com/Alexays/Waybar/blob/master/resources/config.jsonc)。

当前 registry 只有 `niri/workspaces`、`niri/window`、`custom/launcher`、`network`、`cpu`、`memory`、`clock` 的固定 kernel provider。尚未消费每个 module 的 `format`/`interval` option，也没有 CSS parser、click/scroll action、per-output bar、tray、network/audio/battery backend 或 niri IPC module。parser 接受 bottom/left/right position，但 early framebuffer renderer 当前只允许 top。

## swww 式壁纸控制

当前背景仍是 kernel 生成的色带，swww 兼容层尚未实现。计划保持 swww 的 daemon/client 行为：

- `img <path>` 设置图片，可选 output；
- `query` 返回 output geometry 与当前 image；
- `kill` 停止 daemon；
- `--transition-type`、`--transition-step`、`--transition-fps` 及对应环境变量；
- 至少提供 `simple`、`fade`、`left/right/top/bottom`、`center` 的有界 CPU transition。

命令与 transition 语义依据 [swww 官方 README](https://github.com/LGFae/swww)。在有 IPC、图片 decoder、VFS path 和独立用户进程前，不会声称兼容 swww daemon。
