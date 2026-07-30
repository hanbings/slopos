# 滚动桌面兼容方向

SlopOS 桌面以 niri、Waybar 和 swww 的用户习惯作为兼容目标，但不会把尚未实现的协议或选项标成已兼容。当前代码首先复现可独立测试的状态机与配置语义，再逐步替换 early kernel surfaces。

## 用户态桌面策略边界

root image 把第二个 Rust `no_std` ELF 安装为 inode 24 `/sbin/slop-shell`。PID 2 使用独立 CR3、Linux 风格 initial stack 和 100 Hz TSC preemption 路径启动，以异步 `openat/read/close` 每次最多 256 bytes 分块读取 `/etc/slopos/waybar.jsonc` 与 `/etc/slopos/swww.env`，分别限制为 4096/512 bytes并验证非空 EOF；这些 bytes 不再只是 kernel 内的编译期常量。

`crates/desktop-protocol` 定义 40-byte、显式 magic/version/size 的 `DesktopCommit`。消息包含 Waybar provider 与 swww policy capability、两份实际文件的增量 FNV-1a hash、CPU/Memory 初始值范围和 wallpaper id；reserved bit/byte 必须为零。私有 syscall `0x534c0001` 只接受 PID 2，拒绝零摘要，并要求上一代 policy 已被实际应用后才接受下一代；kernel 不再把摘要与编译期默认 asset 比较。语法、UTF-8 与 top-position 等语义仍由独立 VFS config bank 在发布 `config-applied` 前验证，摘要本身是 service provenance，不是 parser 或认证机制。当前 PID 2 发布 CPU 0%、Memory 36% 和 Aurora wallpaper，desktop 在首个 snapshot 到达前明确保持 `awaiting-user-policy`，而不是自行选择初始壁纸。

私有 syscall `0x534c0002` 建立反向生命周期事件。PID 2 传入 event kind、上一代 generation 与 writable user buffer 后阻塞；desktop task 真正应用 policy 时发布 32-byte `policy-applied`，真正 swap 一套 VFS 配置时发布同结构的 `config-applied`。kernel 分别保存两条单调 generation，验证 kind/generation/capability/reserved fields，复制 event 到 PID 2 user stack并恢复其 CR3/frame。事件可以先于 block task 进入等待而到达，generation 状态仍保证不丢通知。五条主要 QEMU 回归都出现 `Blocked → desktop-event → Runnable`，所以这不是同步伪造的成功返回。

`/sbin/slop-shell` 现在是跨 reload 常驻的 service：初次提交 policy generation 1 并收到确认后，它等待 config generation 1；收到后重新读取 Waybar/swww、提交 policy generation 2，再等待下一代 config。交互回归中的有效 `RELOAD` 发布 config generation 2，令 PID 2 再读文件并提交 policy generation 3；`RELOAD BAD` 保持 config generation 2，所以 PID 2 继续休眠且没有 policy generation 4。PID 1 在关闭自己的最后一个 fd 后常驻 `wait4` 作为 supervisor，block task 持有两者的 process/VFS runtime。

这仍不是完整的用户态桌面。PS/2 输入、四份配置的发现与 parse bank、niri 状态机、swww daemon/transition、surface、GOP renderer 与 composition 仍在 kernel。当前还没有通用 message queue/socket、共享 surface buffer、Wayland protocol 或普通用户 client。

## VFS 配置发现与原子重载

桌面构造时先用编译进 kernel 的同源 asset 作为 bootstrap；ext4 root mount 完成后，block task 从 VFS 读取四份文本并发布 generation 1，desktop task 再把整套状态一次替换。root image 把仓库默认配置安装在 `/etc/slopos/`，但发现顺序优先兼容常见位置：

- niri：`/home/slop/.config/niri/config.kdl`、`/etc/niri/config.kdl`、`/etc/slopos/niri.kdl`；
- Waybar JSONC：`/home/slop/.config/waybar/config.jsonc`、同目录 `config`、`/etc/xdg/waybar/config.jsonc`、同目录 `config`、`/etc/slopos/waybar.jsonc`；
- Waybar CSS：`/home/slop/.config/waybar/style.css`、`/etc/xdg/waybar/style.css`、`/etc/slopos/waybar.css`；
- swww environment：`/home/slop/.config/swww/env`、`/etc/swww/env`、`/etc/slopos/swww.env`。

前三份上限各为 4096 bytes，swww environment 上限为 512 bytes；每份都必须是非空 UTF-8。block task 是唯一 writer，在 inactive static bank 中读齐四份文本，先验证 niri layout/shell、Waybar JSONC/top position、Waybar CSS 与 swww environment，再用 release/acquire generation 发布。desktop task 在 local value 中再次 parse，重建 workspace 状态并尽量保留窗口、当前 workspace 与 focus，全部成功后才 swap 并 acknowledge；双 bank 因此不会暴露半套新配置或覆写仍被 renderer 引用的字符串。

Config surface 的按钮或图形 monitor 的 `RELOAD` 命令会唤醒 block task 并触发运行时 VFS 重读。缺失、超长、非法 UTF-8、parse 错误或 early renderer 不支持的非 top Waybar position 都保留上一代。`make test-interaction` 先确认 config generation 1→2 的完整 reload、`config-applied` 唤醒和 PID 2 policy generation 2→3，再以仅供诊断的 `RELOAD BAD` 注入非法 CSS，确认 config 保持 generation 2、service 不被唤醒且没有 policy generation 4；这证明 request/reload/rollback 与常驻 user service 串接路径，不是文件 watcher 或 inotify。当前没有内建配置编辑器，也没有自动监听磁盘变更。

`make test-desktop-custom-config` 复制标准磁盘，在临时 ext4 副本中把默认 904-byte Waybar 文件替换为可解析的 960-byte JSONC（增加用户注释，并把 `niri/window`/`niri/workspaces` 分别移到 left/center），再从 OVMF 启动。日志要求 PID 2 两轮都读到 960 bytes、policy generation 1/2 都携带相同的非默认 hash `0x0c1727e886f1ceac`、config generation 1 正常应用、服务最后阻塞在 `config-applied after_generation=1`；随后真实 PS/2 点击中央数字 `2`/`1` 并拒绝任何 exit/FATAL，宿主最后运行 `e2fsck -fn`。这个回归证明当前支持有界非默认文件、module placement 与相应点击几何，而不是证明完整 Waybar 配置兼容。

## niri 式滚动平铺

`crates/shell` 提供无分配 `ScrollLayout`。当前已经成立的行为：

- 窗口位于向右延伸的 column strip；新窗口插在 focused column 右侧，不改变既有 column width；
- column 超出 output 后只移动 viewport，支持向左/右 focus、edge reveal、手动 scroll 与 titlebar horizontal drag；
- 一个 column 可纵向包含多个 window，up/down focus 不改变 column width；
- close 最后一个 window 会删除 column 并修正 focus/view；
- 支持 fixed、proportional 与 client-selected width；
- `set-column-width` 支持固定像素、绝对百分比与 `+/-` 相对像素/百分比，变更后重新执行 focused-column 可见性约束；
- pointer event 保留实时 keyboard modifier；Super+右键横拖按 mouse delta 逐像素调整 focused column；
- 支持 `never`、`always`、`on-overflow` 三种 focused-column centering，以及 single-column centering。

KDL parser 当前从 `assets/niri-config.kdl` 读取以下 niri 同名子集：

```kdl
workspace "main"
workspace "config" { open-on-output "SLOPOS-1" }

binds {
    Mod+Left { focus-column-left; }
    Mod+Shift+Left { move-column-left; }
    Mod+Shift+Right { move-column-right; }
    Mod+Down { focus-workspace-down; }
    Mod+Shift+Down { move-column-to-workspace-down; }
    Mod+Minus { set-column-width "-10%"; }
    Mod+Equal { set-column-width "+10%"; }
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

parser 也接受 `fixed N`、空 `default-column-width {}`、小数 gap、`#rrggbb`/`#rrggbbaa`，并跳过 full config 中尚未消费的其他 top-level/nested node。workspace 状态机为每个 workspace 保留独立 column strip；named workspace 后附加一个空 workspace。window rule 按出现顺序叠加，后匹配规则可覆盖先前的 `open-on-workspace`。bind chord 支持 `Mod`、`Ctrl`、`Shift`、`Alt` 与方向键、PageUp/PageDown、Return、Tab、Escape、单字符；当前 action 集为 focus column/workspace、同 strip 左右重排列、move column to workspace、`set-column-width` 与 close window。列宽参数当前接受整数像素或 `1%..100%`，可用前缀 `+`/`-` 表示相对调整；尚不接受小数参数。

10 项 layout 测试和 3 项 workspace/bind/rule 测试覆盖配置拒绝边界、open/focus/scroll/stack/close、绝对/相对列宽、列重排、workspace switch/move 与规则顺序。设计语义依据 [niri 默认配置](https://github.com/YaLTeR/niri/blob/main/resources/default-config.kdl)、[Layout 配置文档](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Layout)、[Key Bindings](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Key-Bindings) 与 [Window Rules](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Window-Rules)。

当前 kernel desktop 用三个固定 surface 演示这些行为：Terminal/System 位于 `main`，Config 的 `app-id` 规则把它放到 `config`；顶部 workspace module 显示 active index。PS/2 parser 跟踪 Super/Ctrl/Shift/Alt 与扩展方向键，使 KDL bind 实际驱动桌面。交互回归用 `Mod+Equal` 把 Terminal 从 512 px 放大至 614 px并截图，再以 `Mod+Minus` 恢复 512 px；随后用 `Mod+Shift+Right` 把整列从 x=16 重排至 x=496并截图，再以 `Mod+Shift+Left` 恢复；最后用 Super+右键横拖把列从 512 px 放大至 608 px并恢复，并点击顶部数字 `2`/`1` 在 `config`/`main` 间往返。niri 配置已按上述 user/system/fallback 顺序从 VFS 加载，并能整套原子重读；尚未实现动态 workspace 创建/销毁、完整 niri action/XKB 命名、multi-output、floating/tabbed column、复杂 match、animation、overview、IPC、自动文件监听、Wayland surface 或普通用户 client。

## Waybar 式顶部栏

当前画面使用 top bar，并按 left/center/right 三个区域显示 workspace、focused title 与 system status。root VFS 中选中的 Waybar JSONC 已实际决定 `position`、`height`、`spacing` 和三个 module array；仓库默认源是 `assets/waybar-config.jsonc`。kernel 依 array 顺序查询 module registry并以同一个 region-width helper 计算渲染与点击对齐位置。无论 `niri/workspaces` 位于 left/center/right，hit-test 都沿 CSS margin/padding、6 px glyph advance 和格式化文本中完整 `{value}` workspace label 的实际位置计算；点击 label 内已渲染的 ASCII workspace 数字会聚焦对应 workspace、同步 focused window，并立即重绘 active 标记。

JSONC parser 支持 `//` 与 `/* */` comment、trailing comma、最多 16 个 module/区域、24 个 module config。module object 当前保存 `format`、`format-alt`、`format-disconnected`、`interval`、`tooltip`、`min-length`、`max-length`，跳过未知 nested option，并拒绝 duplicate、非法类型/范围与冲突长度。format renderer 支持 `{}`、named replacement 与 `:>N` 右对齐；当前 provider 实际提供 `{value/name/index/total}`、`{title}`、`{usage}`、`{percentage}`、`{ifname}`。

VFS 中选中的 CSS 使用 Waybar 同样的 GTK CSS selector 命名；仓库默认源是 `assets/waybar-style.css`。无分配 parser 支持 `*`、`window#waybar` 和 module `#id` 的 source-order cascade、逗号 selector list，以及 `color`、`background[-color]`、`padding`、`margin`、`border-bottom: Npx solid #rrggbb`；`transparent` background 和 1/2/3/4-value px box shorthand 可用。renderer 将样式纳入左右/居中宽度计算，当前截图中的 CPU/Memory/Clock 色块、padding 和 bar 底边框都来自 CSS。字段与 selector 依据 [Waybar 官方 `config.jsonc`](https://github.com/Alexays/Waybar/blob/master/resources/config.jsonc)、[默认 `style.css`](https://github.com/Alexays/Waybar/blob/master/resources/style.css) 与 [niri/workspaces module manual](https://github.com/Alexays/Waybar/blob/master/man/waybar-niri-workspaces.5.scd)。

4 项 JSONC/format 与 3 项 CSS 测试覆盖 parse、format replacement、cascade、transparent、box shorthand 和拒绝边界。JSONC/CSS 已从 VFS 成对参与原子 generation reload。当前 registry 包含 `niri/workspaces`、`niri/window`、`custom/launcher`、`network`、`cpu`、`memory`、`clock`：CPU/Memory 的初始值来自 `/sbin/slop-shell` 发布的 snapshot，workspace/window 仍由 kernel niri 状态机提供，network/clock 仍是固定 kernel 值。interval 被验证并保留为 provider 更新策略，但没有常驻用户 provider 或真实 network/CPU/RTC polling。workspace 点击是 registry 中该 module 的直接行为，只识别完整 `{value}` label 内当前固定容量最多四个 workspace 的单字符数字；尚无 JSONC `on-click`/`on-scroll` 命令执行、其他 module action、Pango markup/strftime、format-icons/state、完整 GTK CSS/alpha blend、per-output bar、tray、network/audio/battery backend 或 niri IPC module。parser 接受 bottom/left/right position，但 early framebuffer renderer 当前只允许 top，并在发布 VFS generation 前拒绝其他位置。

## swww 式壁纸控制

`crates/shell` 已提供无分配 swww 风格 CLI parser 与 `WallpaperDaemon` 状态机。kernel 启动 daemon 状态机但不选择图片；PID 2 的首个有效 desktop policy commit 才等价选择 `swww img /usr/share/backgrounds/slopos-aurora.ppm`。后续控制仍由 kernel 图形 monitor 接受带或不带 `swww` 前缀的命令：

- `img <path>` 设置图片，可选 output；
- `query` 返回 output geometry 与当前 image；
- `kill` 停止 daemon；
- `swww-daemon` 在 kill 后重新启动并清空旧 image；
- `--outputs/-o`、`--resize crop|fit|no`、`--transition-type`、`--transition-step`、`--transition-fps`、`--transition-duration`、`--transition-angle`；
- VFS 中发现的 environment 文件以同名 `SWWW_TRANSITION*` 变量提供 boot/reload 默认值，仓库默认源是 `assets/swww.env`；
- `none`、`simple`、`fade`、`left/right/top/bottom`、`center/outer`、`any/random` transition。

两个 12×8 P3/PNM asset 在启动时完整校验 header、尺寸、max value、component 范围和精确 pixel 数。renderer 实际把 current/previous image 逐像素 blend 或 mask 到 GOP；交互测试通过 PS/2 输入切到 Sunset，完成 5 个 center 采样帧，由 `query` 读回 `SLOPOS-1`、1024×768 和当前路径，再验证 kill/restart 与 `none` 重设。7 项 swww/PNM、13 项 niri layout/shell、7 项 Waybar JSONC/CSS 与 5 项 desktop commit/event protocol 测试，共 32 项。

命令与 transition 语义依据 [swww 官方 README](https://github.com/LGFae/swww)。初始 environment/hash/image policy 已由用户进程提交，environment 默认值也参与后续四文件 VFS 原子重载；daemon state 和 image decode/render 仍在 kernel，而不是常驻用户进程或 Unix socket。也没有 Wayland layer-shell、多 output、从 VFS 解码任意图片路径、PNG/JPEG/GIF decode、animated image cache、frame callback/timing、transition position/bezier/wave/grow 或 damage tracking。同步 framebuffer renderer 为限制最坏 CPU 时间，会把极小 step 最多采样成 17 帧，因此不声称二进制或动画时序完全兼容 swww。
