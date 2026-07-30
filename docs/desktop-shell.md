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

`make test-desktop-custom-config` 复制标准磁盘，在临时 ext4 副本中把默认 904-byte Waybar 文件替换为可解析的 1300-byte JSONC（增加用户注释，把 `niri/window`/`niri/workspaces` 分别移到 left/center，并给 clock 加上 `format-alt: UTC ALT`、`on-click: status`、`on-click-right: help`、`on-click-middle: swww query` 及两向 scroll action），再从 OVMF 启动。日志要求 IntelliMouse 握手报告 `wheel=true`，PID 2 两轮都读到 1300 bytes、policy generation 1/2 都携带相同的非默认 hash `0xb001e9dac40c640b`、config generation 1 正常应用、服务最后阻塞在 `config-applied after_generation=1`；随后真实 PS/2 点击中央数字 `2`/`1`，终端输入未执行的 `ABO`，左击右侧 clock 同时把 `UTC` 切成 `UTC ALT` 并触发 STATUS，再补成 `ABOUT` 执行，以证明格式切换与 action 均保留输入缓冲；右击触发 HELP、中击触发 SWWW QUERY，向上/向下滚轮分别无过渡切到 Sunset/Aurora，第二次左击同时执行 STATUS 并切回 `UTC`。测试同时拒绝任何 exit/FATAL，宿主最后运行 `e2fsck -fn`。这个回归证明当前支持有界非默认文件、module placement、alternate format、点击几何、左/右/中/滚轮受限 action 与输入保持，而不是证明完整 Waybar 配置兼容。

## niri 式滚动平铺

`crates/shell` 提供无分配 `ScrollLayout`。当前已经成立的行为：

- 窗口位于向右延伸的 column strip；新窗口插在 focused column 右侧，不改变既有 column width；
- column 超出 output 后只移动 viewport，支持向左/右 focus、edge reveal、手动 scroll 与 titlebar horizontal drag；
- 一个 column 可纵向包含多个 window，up/down focus 不改变 column width；
- `consume-window-into-column` 把右侧 column 的顶窗追加到 focused column 底部；`expel-window-from-column` 把底窗拆到右侧并把焦点留在原列；
- `consume-or-expel-window-left/right` 在 focused window 独占一列时把它追加到对应方向的邻列底部，在纵向 stack 中则把准确的 focused row 拆成左/右 singleton column；焦点始终跟随移动的 window，边界、列容量或目标行容量不足时不动作；
- `move-window-up/down` 在同一 column 内交换 focused window 与相邻窗口，焦点跟随原 window id；
- `set-window-height` 接受与列宽相同的固定/百分比绝对值和 `+/-` 相对值；调整 focused window 时从同列其他窗口补偿像素并保证每窗至少 1 px；
- `preset-column-widths` 与 `preset-window-heights` 各最多保存 8 个 fixed/proportion 项；正反向 action 从当前精确 preset 循环，非 preset 尺寸则选择对应方向的下一项，空 block 按 niri 语义恢复 1/3、1/2、2/3 默认表；
- proportion preset/default/set/adjust 使用 niri 的 `(working_size - gap) × proportion - gap` 几何，多个 tile 连同 gap 可恰好填满 output；`reset-window-height` 恢复当前列等高；
- `maximize-column` 在不覆盖普通列宽的情况下切换到 `output - 2 × gap`，再次调用恢复；显式、preset 或 pointer resize 会退出 maximized 状态；
- `maximize-window-to-edges` 把 focused window 切换到 Waybar 下方工作区的四条边缘（没有 layout gap），再次调用恢复普通几何；若它位于纵向 stack 中，会先按 niri 语义把该窗拆成右侧 singleton column，恢复后仍保持独立列，显式列宽/窗高调整或 consume 会退出 edge-maximized 状态；
- `center-column` 只调整当前 workspace 的 viewport，使 focused column 中点对齐 output 中点，不改变 strip 顺序或列宽；后续 edge focus 会恢复普通可见性约束；
- `center-visible-columns` 把包含 focused column 的完整可见列集合（保留内部 gap）作为整体放到 output 中央；focused column 不完整可见或布局已强制 focused-column centering 时不动作；
- `expand-column-to-available-width` 统计 viewport 中完整可见的列，让 focused column 吸收它们未占用的剩余宽度；若只有 focused column 完整可见则进入 full-width，若 focused column 不完整可见、已 full-width 或没有余宽则不动作；
- `toggle-column-tabbed-display` 在普通纵向 stack 与 tabbed display 间切换；tabbed 时仅 focused row 可见，所有 tab 共用列宽和工作区高度，`focus-window-up/down` 切换 tab，列左侧分段指示器显示当前序号；
- 每个 workspace 同时保存不随 viewport 滚动、始终合成在 tile 上方的 floating layer；`toggle-window-floating` 在两层间切换 focused window，`move-window-to-floating`/`move-window-to-tiling` 幂等地移动到指定层，`switch-focus-between-floating-and-tiling` 切换层焦点，`focus-floating`/`focus-tiling` 幂等地聚焦指定层；方向 focus/move、显式/preset resize、跨 workspace move、标题栏拖动和 Super+右键缩放均按 active layer 分派；
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
    Mod+2 { focus-workspace 2; }
    Mod+Ctrl+3 { move-column-to-workspace 3; }
    Mod+Alt+C { focus-workspace "config"; }
    Mod+Ctrl+Alt+M { move-column-to-workspace "main"; }
    Mod+Tab { focus-workspace-previous; }
    Mod+K { focus-window-up; }
    Mod+J { focus-window-down; }
    Mod+Ctrl+K { move-window-up; }
    Mod+Ctrl+J { move-window-down; }
    Mod+Comma { consume-window-into-column; }
    Mod+Period { expel-window-from-column; }
    Mod+BracketLeft { consume-or-expel-window-left; }
    Mod+BracketRight { consume-or-expel-window-right; }
    Mod+W { toggle-column-tabbed-display; }
    Mod+V { toggle-window-floating; }
    Mod+Shift+V { switch-focus-between-floating-and-tiling; }
    Mod+Alt+V { move-window-to-floating; }
    Mod+Ctrl+V { move-window-to-tiling; }
    Mod+Alt+G { focus-floating; }
    Mod+Alt+T { focus-tiling; }
    Mod+Minus { set-column-width "-10%"; }
    Mod+Equal { set-column-width "+10%"; }
    Mod+Shift+Minus { set-window-height "-10%"; }
    Mod+Shift+Equal { set-window-height "+10%"; }
    Mod+R { switch-preset-column-width; }
    Mod+Shift+R { switch-preset-column-width-back; }
    Mod+Ctrl+Shift+R { switch-preset-window-height; }
    Mod+Ctrl+R { reset-window-height; }
    Mod+F { maximize-column; }
    Mod+M { maximize-window-to-edges; }
    Mod+C { center-column; }
    Mod+Ctrl+C { center-visible-columns; }
    Mod+Ctrl+F { expand-column-to-available-width; }
    Mod+Q { close-window; }
}

window-rule {
    match app-id="slopos-config"
    open-on-workspace "config"
    open-floating false
}

layout {
    gaps 16
    center-focused-column "never"
    preset-column-widths {
        proportion 0.333
        proportion 0.5
        proportion 0.667
    }
    preset-window-heights {
        proportion 0.333
        proportion 0.5
        proportion 0.667
    }
    always-center-single-column
    default-column-display "normal"
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

parser 也接受 `fixed N`、空 `default-column-width {}`、`default-column-display "normal"|"tabbed"`、空或最多 8 项的两类 preset block、小数 gap、`#rrggbb`/`#rrggbbaa`，并跳过 full config 中尚未消费的其他 top-level/nested node。workspace 状态机为每个 workspace 保留独立 tiled strip、floating layer 与各自焦点；声明的 named workspace 永久保留，其后维持一个匿名空 workspace，并保存 previous 索引。把 tiled/floating window 移入末尾空位会在容量允许时追加新的空位；移出、关闭或中间空洞出现时会压缩多余匿名空位，同时修正 active/previous。当前 early desktop 容量为 4，所以这是可验证的有界动态语义，不是任意数量的完整 niri workspace 管理。window rule 按出现顺序叠加，后匹配规则可分别覆盖先前的 `open-on-workspace` 与 `open-floating true|false`。bind 表为无分配、最多 64 项；chord 支持 `Mod`、`Ctrl`、`Shift`、`Alt` 与方向键、PageUp/PageDown、Return、Tab、Escape、Comma、Period、BracketLeft/BracketRight 和单字符。当前 action 集为 focus column/window/workspace、`focus-workspace-previous`、同 strip 左右重排列、同列上下重排/显式调整/复位/preset 窗高、move column to workspace、固定与方向感知的 consume/expel window、normal/tabbed column display toggle、floating toggle、显式 floating/tiling move、切换或显式 layer focus、preset/explicit/maximized column、`maximize-window-to-edges`、focused+visible centered/available-width column 操作与 close window。相对、索引、名称 focus 及跨 workspace 移列都会保存旧 active；previous 动作交换两个索引，所以可连续往返。`focus-workspace`/`move-column-to-workspace` 可取 1..255 的一基索引或已声明 workspace 名称；索引超过当前数量时按 niri 的 best-effort 习惯指向当前末尾空 workspace，名称则在整份 KDL parse 完成后校验，因而可引用稍后声明的 workspace，但不存在/空/过长名称会让原子配置发布失败。列宽与窗高参数当前接受整数像素或 `1%..100%`，可用前缀 `+`/`-` 表示相对调整；尚不接受小数参数。

17 项 layout 测试和 4 项 workspace/bind/rule/layer 测试覆盖配置拒绝边界、open/focus/scroll/focused+visible/single-column center/expand/normal+tabbed stack/floating move+resize+layer focus+workspace transfer/close、固定与左右方向感知的 consume/expel、gap-aware 绝对/相对/preset/maximized 列宽与窗高、无 gap edge maximize、单窗高度、列重排、workspace switch/move 与规则顺序。设计语义依据 [niri 默认配置](https://github.com/YaLTeR/niri/blob/main/resources/default-config.kdl)、[Layout 配置文档](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Layout)、[Tabs](https://github.com/YaLTeR/niri/wiki/Tabs)、[Floating Windows](https://github.com/YaLTeR/niri/wiki/Floating-Windows)、[Key Bindings](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Key-Bindings) 与 [Window Rules](https://github.com/YaLTeR/niri/wiki/Configuration%3A-Window-Rules)。

当前 kernel desktop 用三个固定 surface 演示这些行为：Terminal/System 位于 `main`，Config 的 `app-id` 规则把它放到 `config` 并以 `open-floating false` 明确保持 tiled；顶部 workspace module 显示 active index。PS/2 parser 跟踪 Super/Ctrl/Shift/Alt 与扩展方向键，使 KDL bind 实际驱动桌面。交互回归先以 `Mod+Alt+V` 把 Terminal 从 x=16/y=56/488×696 tile 显式移成 x=16/y=161/488×485 浮窗；System 单列自动居中到 x=268。`Mod+Alt+T`/`Mod+Alt+G` 分别显式聚焦下层 System 与上层 Terminal，`Mod+Ctrl+V` 再把 Terminal 显式送回 x=520 tile。重排回左侧后，原有 toggle 回归再以 `Mod+V` 抽出 Terminal，`Mod+Shift+V` 聚焦下层 System并切回上层 Terminal，`Mod+Ctrl+J` 把浮窗下移到 y=211，第二次 `Mod+V` 送回 x=520 tile。

随后 `Mod+Comma` 产生 x=268 的居中 488 px stack；`Mod+W` 验证 System 2/2 与 Terminal 1/2 在 x=268/y=56/488×696 的 tabbed display 间切换，再恢复 340/340 normal stack。显式/preset 窗高覆盖 340→411→reset 340→458→221→340→269→340；`Mod+Ctrl+K/J` 与 `Mod+K/J` 分别验证同列重排/聚焦，`Mod+Period` 固定拆列，`Mod+BracketLeft/Right` 完成左合并→左拆出→右合并→右拆出。列操作还覆盖 x=16→268 居中、488→992→488 maximize、x=0/y=40/1024×728 edge maximize、488→656→488 preset、488→657 available-width expand、两条 319 px 列整体居中到 x=185、488→588→488 显式 resize、左右重排和 Super+右键 488→584→488 pointer resize。workspace 回归覆盖索引/名称 focus/move、末尾空位 3→4→3、previous 往返与 Waybar 数字点击。niri 配置已按 user/system/fallback 顺序从 VFS 加载并可整套原子重读；尚未实现超过 4 个 workspace、完整 niri action/XKB 命名、multi-output、浮窗位置跨重启持久化、tab 点击/拖曳、复杂 match、animation、overview、IPC、自动文件监听、Wayland surface 或普通用户 client。

## Waybar 式顶部栏

当前画面使用 top bar，并按 left/center/right 三个区域显示 workspace、focused title 与 system status。root VFS 中选中的 Waybar JSONC 已实际决定 `position`、`height`、`spacing` 和三个 module array；仓库默认源是 `assets/waybar-config.jsonc`。kernel 依 array 顺序查询 module registry并以同一个 region-width helper 计算渲染与点击对齐位置。无论 `niri/workspaces` 位于 left/center/right，hit-test 都沿 CSS margin/padding、6 px glyph advance 和格式化文本中完整 `{value}` workspace label 的实际位置计算；点击 label 内已渲染的 ASCII workspace 数字会聚焦对应 workspace、同步 focused window，并立即重绘 active 标记。

JSONC parser 支持 `//` 与 `/* */` comment、trailing comma、最多 16 个 module/区域、24 个 module config。module object 当前保存 `format`、`format-alt`、`format-alt-click`、`format-disconnected`、`interval`、`tooltip`、`min-length`、`max-length`、`on-click`、`on-click-right`、`on-click-middle`、`on-scroll-up`、`on-scroll-down`，跳过未知 nested option，并拒绝 duplicate、非法类型/范围、非 ASCII/控制字符/空 action 与冲突长度。`format-alt-click` 默认左键，也接受 left/middle/right/backward/forward 和对应 Waybar button number；当前 PS/2 鼠标可触发前三种。format renderer 支持 `{}`、named replacement 与 `:>N` 右对齐；当前 provider 实际提供 `{value/name/index/total}`、`{title}`、`{usage}`、`{percentage}`、`{ifname}`。

VFS 中选中的 CSS 使用 Waybar 同样的 GTK CSS selector 命名；仓库默认源是 `assets/waybar-style.css`。无分配 parser 支持 `*`、`window#waybar` 和 module `#id` 的 source-order cascade、逗号 selector list，以及 `color`、`background[-color]`、`padding`、`margin`、`border-bottom: Npx solid #rrggbb`；`transparent` background 和 1/2/3/4-value px box shorthand 可用。renderer 将样式纳入左右/居中宽度计算，当前截图中的 CPU/Memory/Clock 色块、padding 和 bar 底边框都来自 CSS。字段与 selector 依据 [Waybar 官方 `config.jsonc`](https://github.com/Alexays/Waybar/blob/master/resources/config.jsonc)、[默认 `style.css`](https://github.com/Alexays/Waybar/blob/master/resources/style.css) 与 [niri/workspaces module manual](https://github.com/Alexays/Waybar/blob/master/man/waybar-niri-workspaces.5.scd)。

4 项 JSONC/format 与 3 项 CSS 测试覆盖 parse、format/action option、replacement、cascade、transparent、box shorthand 和拒绝边界。JSONC/CSS 已从 VFS 成对参与原子 generation reload。当前 registry 包含 `niri/workspaces`、`niri/window`、`custom/launcher`、`network`、`cpu`、`memory`、`clock`：CPU/Memory 的初始值来自 `/sbin/slop-shell` 发布的 snapshot，workspace/window 仍由 kernel niri 状态机提供，network/clock 仍是固定 kernel 值。interval 被验证并保留为 provider 更新策略，但没有常驻用户 provider 或真实 network/CPU/RTC polling。module 的 alternate-format bit 以 config index 有界保存；匹配点击会先切换格式再执行同一按键 action，随后重算 region width 与 hit-test，配置 generation swap 会像重建 Waybar module 一样复位这些 bits。workspace 左击是 registry 中该 module 的直接行为，只识别完整 `{value}` label 内当前固定容量最多四个 workspace 的单字符数字。其他 module 的左/右/中键和滚轮会分别读取五个 action option，保留用户当前输入，经 ASCII 大写化后进入同一个受限桌面命令分派器；当前允许 `HELP`、`STATUS`、`ABOUT`、`CLEAR`、`RELOAD`、`SWWW-DAEMON` 和 `SWWW ...`，显式拒绝 `FAULT`、`RELOAD BAD` 与任意 shell command。Super+右键仍优先进入 niri 列缩放，不触发 bar action。滚轮先用 200/100/80 sample-rate 序列协商 IntelliMouse ID 3/4 与四字节 packet，失败则回退无滚轮的三字节 packet。尚无 Waybar `smooth-scrolling-threshold`、POSIX shell、Pango markup/strftime、format-icons/state、完整 GTK CSS/alpha blend、per-output bar、tray、network/audio/battery backend 或 niri IPC module。parser 接受 bottom/left/right position，但 early framebuffer renderer 当前只允许 top，并在发布 VFS generation 前拒绝其他位置。

## swww 式壁纸控制

`crates/shell` 已提供无分配 swww 风格 CLI parser 与 `WallpaperDaemon` 状态机。kernel 启动 daemon 状态机但不选择图片；PID 2 的首个有效 desktop policy commit 才等价选择 `swww img /usr/share/backgrounds/slopos-aurora.ppm`。后续控制仍由 kernel 图形 monitor 接受带或不带 `swww` 前缀的命令：

- `img <path>` 设置图片，可选 output；两个兼容短名继续命中 bootstrap asset，其余绝对路径由 root VFS 读取，相对路径当前以 `/usr/share/slopos/` 为基准；
- `clear [RRGGBB]` 设置六位十六进制纯色，省略颜色时为黑色，也可选 `--outputs/-o`；
- `query` 返回 output geometry 与当前 image；
- `kill` 停止 daemon；
- `swww-daemon` 在 kill 后重新启动并清空旧 image；
- `--outputs/-o`、`--resize crop|fit|no`、`--transition-type`、`--transition-step`、`--transition-fps`、`--transition-duration`、`--transition-angle`；
- VFS 中发现的 environment 文件以同名 `SWWW_TRANSITION*` 变量提供 boot/reload 默认值，仓库默认源是 `assets/swww.env`；
- `none`、`simple`、`fade`、`left/right/top/bottom`、`center/outer`、`any/random` transition。

两个 12×8 bootstrap P3/PNM asset 在启动时完整校验 header、尺寸、max value、component 范围和精确 pixel 数。非 registry 路径则进入独立的 8 KiB 双 bank broker：desktop task 发布原始/规范化路径、output 与已解析 transition generation，block task 通过 ext4 walker 异步读齐一个或两个 block，完成 UTF-8 与同一 P3 parser 校验后唤醒 desktop；renderer 完成 transition 后才 acknowledge，因此 block task 不会覆写仍被 current/previous image 引用的 bank。失败结果占用非 active bank，不改变当前图片。

renderer 把同尺寸 current/previous image 逐像素 blend 或 mask 到 GOP；不同尺寸的 bounded P3 仍会加载，但 transition 明确回退为 `none`。纯色状态直接填充 framebuffer，并让 `query` 以 `0xRRGGBB` 报告当前值。交互测试先切到 embedded Sunset 并完成 5 个 center 采样帧，再以 `/usr/share/slopos/vfs-wallpaper.ppm` 跨 inode 30 的两个 block 读入 6144 bytes、完成第二段 center transition并由 `query` 返回原始路径；随后依次请求不存在的 `missing.ppm` 与存在但并非 P3 的 `/etc/slopos/system.conf`，后续三次 query 都仍返回前一图片。最后继续验证 kill/restart、`none`、`clear 1a2b3c`、纯色 query 与恢复图片。7 项 swww/PNM、21 项 niri layout/shell、7 项 Waybar JSONC/CSS 与 5 项 desktop commit/event protocol 测试，共 40 项。

命令与 transition 语义依据 [swww 官方 README](https://github.com/LGFae/swww) 与 [`swww-clear(1)`](https://raw.githubusercontent.com/LGFae/swww/main/doc/swww-clear.1.scd)。初始 environment/hash/image policy 已由用户进程提交，environment 默认值也参与后续四文件 VFS 原子重载；daemon state、path broker、image decode/render 仍在 kernel，而不是常驻用户进程或 Unix socket。当前路径最长 96 ASCII bytes、文件最多 8 KiB、只支持 UTF-8 P3；图形终端会把路径转大写，loader 因而以 ASCII 小写查找，尚不能表达大小写敏感的混合大小写文件名。也没有 Wayland layer-shell、多 output、PNG/JPEG/GIF decode、animated image cache、frame callback/timing、transition position/bezier/wave/grow 或 damage tracking。同步 framebuffer renderer 为限制最坏 CPU 时间，会把极小 step 最多采样成 17 帧，因此不声称二进制或动画时序完全兼容 swww。
