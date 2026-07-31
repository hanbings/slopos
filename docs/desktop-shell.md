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

`make test-desktop-custom-config` 复制标准磁盘，在临时 ext4 副本中同时替换三份配置：3797-byte 默认 niri KDL 被改成单页上限内的 4082-byte 用户文件，把 Config 规则的 `open-maximized false`、`open-maximized-to-edges false`、`open-fullscreen false` 与 `open-focused false` 都覆盖为 `true`，把 app-specific `default-column-width` 从 0.5 改为 0.667、`default-window-height` 从 1.0 改为 0.5，并把 `default-column-display` 从 `normal` 改为 `tabbed`、加入 `focus-ring { off; }`、4 px `#ffb86c` border、softness 8/spread 2/offset `(6,4)`/active `#000c` shadow、`draw-border-with-background false` 与 `opacity 0.75`；904-byte Waybar 文件则替换为可解析的 1541-byte JSONC（增加用户注释与 ordered `output: ["!HDMI-A-1", "SLOPOS-1", "*"]`、`name: "slop-main"`/`margin: "4 12"`/`fixed-center: false`/三个 `expand-*`/`layer: "top"`/`exclusive: true`，把 `niri/window`/`niri/workspaces` 分别移到 left/center，并给 clock 加上 alternate format、三键及两向 scroll action）；548-byte CSS 同时扩为 666 bytes，加入 `window#waybar.SLOPOS-1` 的 `#ff79c6` 底边与 `window#waybar.slop-main` 的 `#202640` 背景，再从 OVMF 启动。geometry marker 记录 bar surface 为 x=12/y=4/1000×40、top layer 与 44px exclusive reserve；config generation 1 随后证明 `open-focused true` 让后台 workspace 2 的 Config 成为活动窗口，并按底层到顶层记录 x=16/y=60/992×338 column maximize、x=0/y=44/1024×724 edge maximize 与 x=0/y=0/1024×768 fullscreen。首张截图确认 fullscreen 窗口覆盖兄弟窗但 top Waybar 仍在其上合成，空白 surface click marker 又证明 `passthrough=false`；`171c2b` 像素则证明 fullscreen window 忽略 opacity rule。真实 `Mod+Shift+F` 先恢复 edge 几何，`Mod+M` 再恢复 992×338，随后两次 `Mod+F` 产生 656→992 px marker；独立截图证明按 `(1024−16)×0.667−16` 恢复 x=184 的规则列宽，同时按 `(768−44−16)×0.5−16` 保留 338 px 规则高度与 tab 指示条。Config 的青色 focus ring 已关闭，4 px active border 为 `ffb86c`；`draw-border-with-background false` 把 border 限制在 surface 外侧，因此脚本要求 x=200/800、y=350 分别为 Aurora 不同采样块经 0.75-alpha surface 合成后的 `222247`/`222a4b`，并在外边缘 x=182/y=350 精确读到 `ffb86c`。阴影外缘 x=844/y=200 的 `0a0a1f` 验证 softness/offset/spread/alpha；bar 的 x=0/12、y=20 与 x=20、y=2/4 四个像素以 `111144`/`202640` 同时验证横向/顶部 margin 与 name class 样式，x=20/y=43 的 `ff79c6` 再验证自动 output class。这十一点证明逐通道 blend、空心边框、shadow raster、Waybar surface geometry 与 top composition，而非只验证 marker。日志还要求 IntelliMouse 握手报告 `wheel=true`，PID 2 两轮都读到 1541 bytes，layout marker 把 class 与 namespace 都记录为 `slop-main`、policy generation 1/2 都携带非默认 Waybar hash `0x976a817ac7e7fb85`、服务最后阻塞在 `config-applied after_generation=1`。退出 fullscreen 后，center block 依据三块 GTK expand packing 与左侧 Config/Terminal title 宽度动态移动；x=331/y=20 的白色文本像素锁定新分配位置，真实 PS/2 仍按新位置点击数字 `1`/`2`/`1` 往返，证明 `fixed-center false` 和三个 `expand-*` 与其他 workspace 的旧 Terminal 局部焦点都生效。终端输入未执行的 `ABO`，左击右侧 clock 同时把 `UTC` 切成 `UTC ALT` 并触发 STATUS，再补成 `ABOUT` 执行；右击触发 HELP、中击触发 SWWW QUERY，向上/向下滚轮分别切到 Sunset/Aurora，第二次左击执行 STATUS 并切回 `UTC`。

同一回归随后从标准 rootfs 再启动 1256-byte fixture，以 string `output: "SLOPOS-1"` 命中当前显示，再以 `width: 800` 把 bar surface 居中限制到 x=112..912，以 `no-center: true` 完全移除 center module，再以 `mode: "slop-overlay"` 选择 custom `modes` 项并把有效状态覆盖为 overlay layer、`exclusive=false`、`passthrough=true`、visible；零 reserve 让 Terminal/System 从 y=16 开始，bar 在 x=600/y=39 的 `6558f5` 底边像素覆盖窗口 titlebar。x=100/112、y=10 的 `111144`/`161a2a` 锁定 fixed-width surface 左边界，x=488/y=16 的纯背景锁定 center text 未被实例化。真实左击在 x=487 的 bar surface 内命中下方 Terminal 关闭按钮并关闭 Terminal，同时没有产生 module action，直接证明受限宽度 custom mode 的 pointer 穿透。前两次启动都拒绝 exit/FATAL；第三次 output 排除回归见下段。这个 QEMU 回归证明当前支持有界非默认 niri/Waybar 文件、app-specific 几何/显示/decoration/opening state、Waybar output string/ordered-array 选择、name/output class namespace/width/no-center/margin/fixed-center/exclusive/layer/custom mode/passthrough、module placement/alternate format/点击/三键/滚轮 action 与输入保持，但不据此声称 Sway IPC、POSIX signal transport 或完整配置兼容。

第三次短启动注入 993-byte fixture：ordered `output: ["!SLOPOS-1", "*"]` 先排除当前显示，因此后面的 wildcard 不再生效。串口锁定 `selector=array entries=2 selected=false`、`visible=false` 与 `reserved_top=0`；x=100/y=10 保持 Aurora 的 `111144`，而不是默认 bar 的 `161a2a`，直接证明 surface 未被实例化。对应 PID 2 两代 policy hash 都是 `0x3d20db99f86c6b26`；第三次启动同样无 exit/FATAL，最后的只读 `e2fsck` 通过。

标准交互回归另从 monitor 依次输入两次 `WAYBAR SIGUSR1`。默认 `on-sigusr1=toggle` 首次把 effective mode 从 `default` 切到 `invisible`，移除 bar surface/pointer region 与 40 px exclusive reserve，并即时令 tiled/floating working area 从 40 扩到 0；第二次恢复保存的 `default` mode 与 40 px reserve。串口 marker 锁定 `default→invisible→default`、`state_visible=false→true` 和 `reserved_top=40→0→40`，两张截图分别以窗口上移 40 px 及 bar/间隙像素恢复验证真实合成结果。parser 同时支持 `on-sigusr1`/`on-sigusr2` 的 `show`、`hide`、`toggle`、`reload`、`noop`，默认分别为 toggle/reload；未知字符串按 Waybar 默认动作回退。`start_hidden` 会保留原 configured mode，随后 show 能恢复它；自定义 `invisible` 仍可覆盖隐藏态的 layer/exclusive/passthrough/visible。当前 `WAYBAR SIGUSR1|SIGUSR2` 是 early monitor 的显式触发边界，不是假装已经存在 Unix process signal。

新增的 floating-position 段落继续使用同一回归：恢复到 992×338 column state 后，`Mod+Alt+V` 应用 `default-floating-position x=24 y=24 relative-to="bottom-right"`，得到 x=8/y=406、右下各留 24 px 的浮窗；把它向下移动到 y=430、送回 tiling、再移入 floating 后，窗口恢复 x=8/y=430，而不是重新套用默认 y=406。两张截图与 `applied=true/remembered=false`、`applied=false/remembered=true` marker 分别证明首次锚定和会话内位置记忆。

## niri 式滚动平铺

`crates/shell` 提供无分配 `ScrollLayout`。当前已经成立的行为：

- 窗口位于向右延伸的 column strip；新窗口插在 focused column 右侧，不改变既有 column width；
- column 超出 output 后只移动 viewport，支持向左/右 focus、edge reveal、手动 scroll 与 titlebar horizontal drag；
- `focus-column-first/last` 直达 strip 边界，`move-column-to-first/last` 将完整 column（包括 stack、focused row、列宽和 display state）稳定重排到对应端；
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
    Mod+Home { focus-column-first; }
    Mod+End { focus-column-last; }
    Mod+Shift+Left { move-column-left; }
    Mod+Shift+Right { move-column-right; }
    Mod+Ctrl+Home { move-column-to-first; }
    Mod+Ctrl+End { move-column-to-last; }
    Mod+Down { focus-workspace-down; }
    Mod+Page_Up { focus-workspace-up; }
    Mod+Page_Down { focus-workspace-down; }
    Mod+Shift+Down { move-column-to-workspace-down; }
    Mod+Ctrl+Page_Up { move-column-to-workspace-up; }
    Mod+Ctrl+Page_Down { move-column-to-workspace-down; }
    Mod+Shift+Page_Up { move-workspace-up; }
    Mod+Shift+Page_Down { move-workspace-down; }
    Mod+WheelScrollDown cooldown-ms=150 { focus-workspace-down; }
    Mod+WheelScrollUp cooldown-ms=150 { focus-workspace-up; }
    Mod+Ctrl+WheelScrollDown cooldown-ms=150 { move-column-to-workspace-down; }
    Mod+Ctrl+WheelScrollUp cooldown-ms=150 { move-column-to-workspace-up; }
    Mod+Shift+WheelScrollDown { focus-column-right; }
    Mod+Shift+WheelScrollUp { focus-column-left; }
    Mod+Ctrl+Shift+WheelScrollDown { move-column-right; }
    Mod+Ctrl+Shift+WheelScrollUp { move-column-left; }
    Mod+2 { focus-workspace 2; }
    Mod+Ctrl+3 { move-column-to-workspace 3; }
    Mod+Alt+C { focus-workspace "config"; }
    Mod+Ctrl+Alt+M { move-column-to-workspace "main"; }
    Mod+Alt+Up { move-window-to-workspace-up; }
    Mod+Alt+Down { move-window-to-workspace-down; }
    Mod+Ctrl+Shift+2 { move-window-to-workspace 2; }
    Mod+Shift+Alt+M { move-window-to-workspace "main"; }
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
    Mod+Shift+F { fullscreen-window; }
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
    open-maximized false
    open-maximized-to-edges false
    open-fullscreen false
    open-focused false
    default-floating-position x=24 y=24 relative-to="bottom-right"
    default-column-width { proportion 0.5; }
    default-window-height { proportion 1.0; }
    default-column-display "normal"
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

parser 也接受 `fixed N`、空 `default-column-width {}`/`default-window-height {}`、全局与 rule-specific `default-column-display "normal"|"tabbed"`、空或最多 8 项的两类 preset block、小数 gap、`#rrggbb`/`#rrggbbaa`，并跳过 full config 中尚未消费的其他 top-level/nested node。workspace 状态机为每个 workspace 保留 persistent/anonymous identity、独立 tiled strip、floating layer、可选 fullscreen window 与各自焦点；声明的 named workspace 永久保留，其后维持一个匿名空 workspace，并保存 previous 身份。把 tiled/floating window 移入末尾空位会在容量允许时追加新的空位；移出、关闭或中间匿名空洞出现时会压缩多余空位，同时修正 active/previous。`move-workspace-up/down` 会原子交换 identity、两层 layout、fullscreen 与焦点状态，active 跟随当前 workspace，previous 继续指向同一逻辑 workspace；空的尾部 anonymous workspace 不参与无意义交换。当前 early desktop 容量为 4，所以这是可验证的有界动态语义，不是任意数量的完整 niri workspace 管理。window rule 按出现顺序叠加，后匹配规则可分别覆盖先前的 `open-on-workspace`、`open-floating true|false`、`open-focused true|false`、空/`fixed N`/`proportion 0..1` 的 `default-column-width`/`default-window-height`、`default-column-display`、window `focus-ring`、`open-maximized true|false`、`open-maximized-to-edges true|false` 与 `open-fullscreen true|false`；初始宽高按同一 gap-aware 公式作用于 tiled 和 floating，规则 display 则在初次打开、从 stack 拆成 singleton、单窗跨 workspace 和 floating→tiling 时动态决定新列模式。显式 `open-focused false` 会在目标已有 tiled/floating window 时保留其局部焦点，`true` 会聚焦新窗并激活 `open-on-workspace` 指定的后台 workspace；配置重建先恢复其他 workspace 的旧局部焦点，再应用强制聚焦，因此不会污染返回后的输入目标。column maximize 把 tiled column 设为 `output - 2 × gap` 且不抢走其他 workspace 焦点；edge maximize 强制 scrolling layer 并把单窗设为完整 working area，保留 Waybar但去掉 gap/border；fullscreen 覆盖完整 output 和兄弟窗，同时保留底层 tiled/floating 几何。bottom Waybar 会被 fullscreen 覆盖，top/overlay Waybar 则在其上继续合成和处理非 passthrough 输入。三种初态并存时可见优先级为 fullscreen、edge maximize、column maximize、规则尺寸，逐层取消会按相反顺序精确恢复，规则高度与 display 始终保持不变。window `focus-ring` block 另按匹配顺序逐属性覆盖 on/off、width 与 active/inactive color，未覆盖字段继承全局 layout。bind 表为无分配、最多 80 项；chord 支持 `Mod`、`Ctrl`、`Shift`、`Alt` 与方向键、`PageUp`/`PageDown` 及官方 `Page_Up`/`Page_Down` 别名、`WheelScrollUp`/`WheelScrollDown`、Home/End、Return、Tab、Escape、Comma、Period、BracketLeft/BracketRight 和单字符；Shift 数字产生的 `!`..`)` 会规范回物理 `1`..`0`，因此组合键仍按 niri 配置中的数字名匹配。当前只消费纵向 wheel key；`cooldown-ms=0..65535` 会按 100 Hz PIT 向上取整到 10 ms tick、逐 binding 限流，并在配置 generation 重建时复位。`WheelScrollLeft/Right`、`repeat` 和其他 bind property 行为尚未实现。当前 action 集为相邻及首末 column focus/reorder、window/workspace focus、`focus-workspace-previous`、workspace reorder、同列上下重排/显式调整/复位/preset 窗高、move column/window to workspace、固定与方向感知的 consume/expel window、normal/tabbed column display toggle、floating toggle、显式 floating/tiling move、切换或显式 layer focus、`fullscreen-window`、preset/explicit/maximized column、`maximize-window-to-edges`、focused+visible centered/available-width column 操作与 close window。全屏只覆盖当前 output 的 window 合成与输入可见性，不改写 tiled column 或 floating rect；再次切换会精确恢复原 layer 和几何。`move-column-to-workspace*` 原子转移完整 column，保留 stack、focused row、列宽与 display state；`move-window-to-workspace*` 只抽出 focused window，以原列宽和当前规则 display 在目标建立 singleton column，目标容量不足则两边均不修改。相对、索引、名称 focus 及两种跨 workspace move 都会保存旧 active；previous 动作交换两个身份，所以可连续往返。`focus-workspace`、`move-column-to-workspace` 与 `move-window-to-workspace` 可取 1..255 的一基索引或已声明 workspace 名称；索引超过当前数量时按 niri 的 best-effort 习惯指向当前末尾空 workspace，名称则在整份 KDL parse 完成后校验并在运行时通过可移动 identity 查找，因而可引用稍后声明或已被重排的 workspace，但不存在/空/过长名称会让原子配置发布失败。bind action 的列宽与窗高参数当前接受整数像素或 `1%..100%`，可用前缀 `+`/`-` 表示相对调整；尚不接受小数 action 参数。

window `border` 与 `focus-ring` 一样支持 layout default 及 window-rule 的 on/off、整数 width、active/inactive `#rrggbb[aa]` 固色，并按匹配顺序逐属性覆盖。focus ring 只围绕 active window；border 为每个普通窗口选择 active/inactive 色。两者同时启用时按 outer focus ring→border→surface 合成；fullscreen 省略全部 decoration。当前没有 gradient、urgent color、自由 corner radius，也没有为 border 额外扩大 layout reserved geometry。

window `shadow` 支持 layout default 与有序 window-rule 的 on/off、`softness 0..1024`、`spread -1024..1024`、`offset x/y=-65535..65535`、`draw-behind-window` 及 `#rgb[a]`/`#rrggbb[aa]` active/inactive color。默认与 niri 一致为 off、offset `(0,5)`、softness 30、spread 5、`#0007`，未配置 inactive color 时使用同 RGB 的 75% alpha。CPU renderer 以有界输出矩形、方框距离和二次 alpha falloff 近似 box shadow；`draw-behind-window false` 会从 shadow mask 剔除 surface rect，fullscreen 完全跳过 decoration。当前没有 niri GPU Gaussian shader、geometry corner radius 或 CSD shadow clipping。

动态 `opacity` rule 同样按出现顺序逐属性覆盖，KDL 接受带正负号且最多三位小数的数值，内部以千分之一保存，渲染时再像 niri 一样 clamp 到 0..1。非全屏 tiled/floating surface 会逐像素读取已合成 framebuffer 并做 rounded RGB alpha blend；fullscreen 强制 1.0。`draw-border-with-background true|false` 也按有序 exact `app-id` 规则覆盖；未指定时遵循当前无 SSD surface 的 niri heuristic，默认为 true。true 会在 surface 下方填充 border，且 border 关闭时也填充 active focus ring；false 改画 surface 外的四条空心边，border 启用时 focus ring 无论该值为何都只画在 border 外。tab indicator 是独立 compositor decoration。当前没有 client 自身 alpha、subsurface/popup 独立 opacity、`toggle-window-rule-opacity`、SSD/CSD 协商或动画期间向 1.0 插值。

`default-floating-position` 按 window rule 的出现顺序逐属性覆盖，接受 niri 的 `top-left`、`top-right`、`bottom-left`、`bottom-right`、`top`、`bottom`、`left`、`right` 八种 working-area 锚点。右/下锚点反转对应坐标方向，单边锚点在该边居中；小数逻辑坐标取最近整数，并限制到当前 output 内。规则在窗口直接打开为 floating 或首次从 tiling 移入 floating 时应用，之后同一桌面会话内优先恢复最后的 floating rect。

20 项 layout 测试和 10 项 workspace/bind/rule/layer 测试覆盖配置拒绝边界、keyboard/wheel chord lookup、open/focus/scroll/首末及相邻列 focus+reorder/focused+visible/single-column center/expand/normal+tabbed stack/floating move+resize+layer focus+workspace transfer/reorder/fullscreen/close、八种 floating anchor 与最后位置记忆、named/anonymous identity 与 previous 修正、完整 column 与单 window 跨 workspace 转移及容量 rollback、固定与左右方向感知的 consume/expel、gap-aware 绝对/相对/preset/maximized 列宽与窗高、app-specific tiled/floating 初始宽高、opening focus 和 opening/expel/workspace/floating→tiling display、指定非焦点列的 column/edge/fullscreen 初态及 tiled/floating 精确恢复、无 gap edge maximize、单窗高度、workspace switch/move 与逐属性规则顺序。设计语义依据 [niri 默认配置](https://github.com/niri-wm/niri/blob/main/resources/default-config.kdl)、[Layout 配置文档](https://github.com/niri-wm/niri/wiki/Configuration%3A-Layout)、[Fullscreen and Maximize](https://github.com/niri-wm/niri/wiki/Fullscreen-and-Maximize)、[Tabs](https://github.com/niri-wm/niri/wiki/Tabs)、[Floating Windows](https://github.com/niri-wm/niri/wiki/Floating-Windows)、[Key Bindings](https://github.com/niri-wm/niri/wiki/Configuration%3A-Key-Bindings) 与 [Window Rules](https://github.com/niri-wm/niri/wiki/Configuration%3A-Window-Rules)。

当前 kernel desktop 用三个固定 surface 演示这些行为：Terminal/System 位于 `main`，Config 的 `app-id` 规则把它放到 `config` 并以 `open-floating false`、三种几何 state 与 `open-focused false`、`default-column-width { proportion 0.5; }`、`default-window-height { proportion 1.0; }`、`default-column-display "normal"` 明确默认初态；custom-config 将四个 opening property 与后三项分别改为 true/true/true/true/0.667/0.5/tabbed，并加入 `focus-ring { off; }` 后，规则 marker 与截图共同证明 Config 无需点击就激活 workspace 2 并以 x=0/y=0/1024×768 fullscreen 打开，`Mod+Shift+F` 恢复 x=0/y=44/1024×724 edge maximize，`Mod+M` 恢复 x=16/y=60/992×338 column maximize，再由 `Mod+F` 恢复为 x=184/y=60/656×338 的 app-specific 宽高和 tab 指示条，并可重新最大化；随后 main/config/main 点击往返仍把键盘输入交给旧 Terminal。顶部 workspace module 显示 active index。PS/2 parser 跟踪 Super/Ctrl/Shift/Alt、扩展方向键、PageUp/PageDown、Home/End 与 IntelliMouse 四字节滚轮，使 KDL bind 实际驱动桌面。交互回归先把 Terminal/System 合并为 stack：`move-window-to-workspace 2` 只将 focused System 送到 Config 右侧 x=520 全高列，named action 送回后 main 恢复两条全高列；再次 stack 后，`move-column-to-workspace 2` 则让 Terminal x=520/y=56 与 System x=520/y=412 作为同一列一起移动，送回 main 后仍在 x=268 上下堆叠。随后再以单窗动作拆回两列并聚焦 Terminal。真实 `Mod+End` 聚焦末列 System，`Mod+Ctrl+Home` 把它移至 x=16 首列，`Mod+Ctrl+End` 恢复至 x=520，`Mod+Home` 再聚焦首列 Terminal。`Mod+Shift+PageDown` 让包含两窗的 `main` 从 workspace 1 整体移至 2，Waybar 显示 `1 [2] 3`；此时 `Mod+Alt+C/M` 仍分别按 identity 聚焦已经位于 1 的 `config` 和位于 2 的 `main`，再由 `Mod+Shift+PageUp` 把后者完整移回 1。紧接着的真实 IntelliMouse 回归让 `Mod+WheelScrollDown/Up` 在 config/main 往返，`Mod+Shift+WheelScrollDown/Up` 在 Terminal/System 两列往返聚焦，`Mod+Ctrl+WheelScrollDown/Up` 把 System 整列送到 config 后原位送回，`Mod+Ctrl+Shift+WheelScrollDown/Up` 又把 Terminal 右移至 x=520 再恢复 x=16。QMP 只负责显式保持跨设备 modifier key-down；wheel 仍经 QEMU i8042 产生真实 PS/2 四字节 packet，串口逐次记录 `0x1/0x5/0x3/0x7` modifier bitmap；双包 burst 的第二包另记录 `accepted=false cooldown_ms=150 remaining_ms=100`。`Mod+Shift+F` 随后让 tiled Terminal 以 x=0/y=0/1024×768 无装饰覆盖完整 output；覆盖期间点击原 workspace 2 的 bar 坐标不会切换 workspace，第二次按键则精确恢复 x=16/y=56/488×696。floating 回归接着以 `Mod+Alt+V` 把 Terminal 显式移成 x=16/y=161/488×485 浮窗，并用同一全屏动作覆盖 output 后恢复至该 floating rect；`Mod+Alt+T`/`Mod+Alt+G` 再分别显式聚焦下层 System 与上层 Terminal，`Mod+Ctrl+V` 把 Terminal 显式送回 x=520 tile。重排回左侧后，原有 toggle 回归再以 `Mod+V` 抽出 Terminal，`Mod+Shift+V` 聚焦下层 System并切回上层 Terminal，`Mod+Ctrl+J` 把浮窗下移到 y=211，第二次 `Mod+V` 送回 x=520 tile。

同一 custom-config 还加入 active `#ffb86c`、4 px `border`、softness 8/spread 2/offset `(6,4)`/active `#000c` 的 `shadow`、`draw-border-with-background false` 与 `opacity 0.75`：规则 marker 记录完整 decoration/background-mode 与 `value=750/1000 fullscreen_ignored=true`。初始 fullscreen 截图的 surface 像素仍为不透明 `171c2b`；退出全屏后 x=200/800、y=350 两个 surface 像素分别为 `222247`/`222a4b`，对应 `WINDOW_ALT` 对 Aurora 两个不同采样块的 0.75 rounded blend，x=182/y=350 为外侧边框固色 `ffb86c`，x=844/y=200 则为 shadow 把 Aurora `333399` 以 0.8 黑色合成后的 `0a0a1f`。这同时覆盖 ordered rule 解析、RGBA shorthand、clamp 后的动态取值、空心 decoration、shadow/opacity 的真实 framebuffer blend 和 niri 的 fullscreen 例外。

随后 `Mod+Comma` 产生 x=268 的居中 488 px stack；`Mod+W` 验证 System 2/2 与 Terminal 1/2 在 x=268/y=56/488×696 的 tabbed display 间切换，再恢复 340/340 normal stack。显式/preset 窗高覆盖 340→411→reset 340→458→221→340→269→340；`Mod+Ctrl+K/J` 与 `Mod+K/J` 分别验证同列重排/聚焦，`Mod+Period` 固定拆列，`Mod+BracketLeft/Right` 完成左合并→左拆出→右合并→右拆出。列操作还覆盖 x=16→268 居中、488→992→488 maximize、x=0/y=40/1024×728 edge maximize、488→656→488 preset、488→657 available-width expand、两条 319 px 列整体居中到 x=185、488→588→488 显式 resize、左右重排和 Super+右键 488→584→488 pointer resize。workspace 回归覆盖整体上下重排、重排后的 named 引用、keyboard/wheel 索引/名称 focus/move、末尾空位 3→4→3、previous 往返与 Waybar 数字点击。niri 配置已按 user/system/fallback 顺序从 VFS 加载并可整套原子重读；尚未实现超过 4 个 workspace、横向 wheel key、bind repeat、完整 niri action/XKB 命名、multi-output、浮窗位置跨重启持久化、tab 点击/拖曳、复杂 match、animation、overview、IPC、自动文件监听、Wayland surface 或普通用户 client。

## Waybar 式顶部栏

当前画面使用 top bar，并按 left/center/right 三个区域显示 workspace、focused title 与 system status。root VFS 中选中的 Waybar JSONC 已实际决定 `output`、`name`、`position`、`height`、`width`、`spacing`、`margin`/四个 `margin-*`、`fixed-center`、`exclusive` 和三个 module array；仓库默认源是 `assets/waybar-config.jsonc`。当前唯一显示以 port 名 `SLOPOS-1` 和 identifier `SlopOS Virtual Display 0x00000001` 暴露；`output` 接受一条最长 96 bytes 的 string，或最多八条的 ordered array。string 支持精确 name/identifier 与 `!` 反选；array 按出现顺序处理 `!`、精确匹配和首字符 `*` wildcard，先命中或先排除即结束。未指定或空 string 选择当前显示，空 array 不选择；配置了 selector 后，匹配完成前以及未命中时 effective visible 都为 false，signal 只改变保存的 mode/visibility，不能绕过 output 过滤。`$VAR` expansion、`output-dimensions`、root array 多 bar 与真实多 output 尚未实现。`name` 限 32-byte ASCII 字母数字、`-`/`_`；显式值同时作为 bar CSS class 与保留的 layer-shell namespace，未指定时 namespace 回退 `waybar`、不添加 class。`margin` 接受无单位整数或 CSS 1/2/3/4-value string；与 Waybar 一样，只要任一个逐边字段存在，就完全忽略 shorthand，并把未指定边归零。有符号 margin 以 i32 保存；top renderer 用 top/left/right 约束 surface，exclusive top reserve 为 `max(0, height + margin-top)`，bottom margin 在 top position 下不参与 anchored geometry。`fixed-center true` 优先把 center block 放在 bar 绝对中心，左右空间不足时推开；false 则在 left block 末端与 right block 起点之间居中。kernel 以同一组 origin helper 计算渲染与点击，无论 `niri/workspaces` 位于哪个区域，hit-test 都沿 surface margin、CSS margin/padding、6 px glyph advance 和完整 `{value}` label 的实际位置计算。

JSONC parser 支持 `//` 与 `/* */` comment、trailing comma、最多 16 个 module/区域、24 个 module config。module object 当前保存 `format`、`format-alt`、`format-alt-click`、`format-disconnected`、`interval`、`tooltip`、`min-length`、`max-length`、`on-click`、`on-click-right`、`on-click-middle`、`on-scroll-up`、`on-scroll-down`，跳过未知 nested option，并拒绝 duplicate、非法类型/范围、非 ASCII/控制字符/空 action 与冲突长度。`format-alt-click` 默认左键，也接受 left/middle/right/backward/forward 和对应 Waybar button number；当前 PS/2 鼠标可触发前三种。format renderer 支持 `{}`、named replacement 与 `:>N` 右对齐；当前 provider 实际提供 `{value/name/index/total}`、`{title}`、`{usage}`、`{percentage}`、`{ifname}`。

VFS 中选中的 CSS 使用 Waybar 同样的 GTK CSS selector 命名；仓库默认源是 `assets/waybar-style.css`。无分配 parser 支持 `*`、`window#waybar`、bar name 或自动加入的 `SLOPOS-1` output class 所对应的 `.class`/`window.class`/`window#waybar.class`，以及 module `#id` 的 source-order cascade、逗号 selector list、`color`、`background[-color]`、`padding`、`margin`、`border-bottom: Npx solid #rrggbb`；`transparent` background 和 1/2/3/4-value px box shorthand 可用。当前子集可独立匹配 name/output class，但尚不解析 `window#waybar.name.output` 这类多 class compound selector。renderer 将样式纳入左右/居中宽度计算，当前截图中的 CPU/Memory/Clock 色块、padding 和 bar 底边框都来自 CSS。字段与几何语义依据 [Waybar bar configuration manual](https://github.com/Alexays/Waybar/blob/master/man/waybar.5.scd.in)、[Waybar output 过滤实现](https://github.com/Alexays/Waybar/blob/master/src/config.cpp)、[官方 `config.jsonc`](https://github.com/Alexays/Waybar/blob/master/resources/config.jsonc)、[默认 `style.css`](https://github.com/Alexays/Waybar/blob/master/resources/style.css) 与 [niri/workspaces module manual](https://github.com/Alexays/Waybar/blob/master/man/waybar-niri-workspaces.5.scd)。

6 项 JSONC/format 与 5 项 CSS 测试覆盖 parse、format/action option、replacement、ordered output selection、bar name/output class namespace、fixed width 居中、no-center、三块 expand packing、margin shorthand/逐边优先级、fixed-center/exclusive default、bottom/top/overlay layer、四个官方 mode preset、最多 8 个 custom `modes`、`start_hidden`/`visible`、passthrough、signal action、cascade、transparent、box shorthand 和拒绝边界。JSONC/CSS 已从 VFS 成对参与原子 generation reload。top-level 未指定 mode 时使用 default 并叠加显式 layer/exclusive/passthrough/visible；`dock`、`hide`、`invisible`、`overlay` 和 custom mode 都依 [Waybar bar implementation](https://github.com/Alexays/Waybar/blob/master/src/bar.cpp) 成组覆盖这四项状态。内建名可被 `modes` 对象局部覆盖；新名称以 bottom/false/false/false 为基线；未知选中名回退 default；`start_hidden` 选择可配置的 invisible 作为初态，但保存 configured mode 供 show 恢复。模式名限 32-byte ASCII 字母数字、`-`/`_`；top bar 的 `width` 为 0/1 时保持横向双边 anchor，值大于 1 时在 output 中居中并限制 surface/module origin/hit-test，超出可用宽度则夹到 margin 内；`no-center` 同时跳过 center module 的绘制与输入；`fixed-center=false` 时 center 总参与剩余空间分配，启用 expand 的 left/right block 与它等分余量，`expand-center` 再决定 center 内容贴住分配区起点或在区内居中；`on-sigusr1`/`on-sigusr2` 运行时可 show/hide/toggle/reload/noop，但没有 Sway IPC 驱动的任意 mode 切换。early compositor 在普通窗/fullscreen 之前画 bottom bar，在它们之后画 top/overlay bar；pointer hit-test 遵循同一 z-order，`passthrough=true` 会跳过整个 bar input region。visibility action 改变 exclusive reserve 时，所有 workspace 的 tiled 几何立即重算，floating rect 与会话内 remembered rect 也会夹回新 working area。custom QEMU 第一轮先以 ordered array 命中 `SLOPOS-1`，再用五个像素锁定 x/y surface margin 与 expand center origin、用 44 px working-area reserve 锁定 exclusive，并在 left title 宽度变化时按非固定 center 的新位置完成 workspace 三次点击；top bar 在 fullscreen 上方吞掉空白 surface click。第二次短启动以 string selector 命中当前 output，又以 x=112..912 surface 边界、center 背景、y=39 的 bar 像素和 Terminal close marker 证明 fixed width、no-center、overlay 合成与 click-through；第三次短启动则用 `["!SLOPOS-1", "*"]` 排除当前 output，以 `selected=false`/reserve 0 和壁纸像素证明 bar 未实例化；标准交互则证明 SIGUSR1 toggle 的隐藏/恢复。当前 registry 包含 `niri/workspaces`、`niri/window`、`custom/launcher`、`network`、`cpu`、`memory`、`clock`：CPU/Memory 的初始值来自 `/sbin/slop-shell` 发布的 snapshot，workspace/window 仍由 kernel niri 状态机提供，network/clock 仍是固定 kernel 值。interval 被验证并保留为 provider 更新策略，但没有常驻用户 provider 或真实 network/CPU/RTC polling。module 的 alternate-format bit 以 config index 有界保存；匹配点击会先切换格式再执行同一按键 action，随后重算 region width 与 hit-test，配置 generation swap 会像重建 Waybar module 一样复位这些 bits。workspace 左击是 registry 中该 module 的直接行为，只识别完整 `{value}` label 内当前固定容量最多四个 workspace 的单字符数字。其他 module 的左/右/中键和滚轮会分别读取五个 action option，保留用户当前输入，经 ASCII 大写化后进入同一个受限桌面命令分派器；当前允许 `HELP`、`STATUS`、`ABOUT`、`CLEAR`、`RELOAD`、`WAYBAR SIGUSR1|SIGUSR2`、`SWWW-DAEMON` 和 `SWWW ...`，显式拒绝 `FAULT`、`RELOAD BAD` 与任意 shell command。带修饰键且命中 niri 全局 bind 的滚轮不会再传给 bar；未匹配的普通滚轮才由 Waybar module 消费。Super+右键仍优先进入 niri 列缩放，不触发 bar action。`name` namespace 目前只作为 early surface metadata 与日志保留，尚无真正 layer-shell transport；也尚无 Sway IPC、POSIX signal delivery、Waybar `smooth-scrolling-threshold`、POSIX shell、Pango markup/strftime、format-icons/state、完整 GTK CSS/alpha blend、真实 multi-output/root-array multi-bar/`$VAR` output expansion、tray、network/audio/battery backend 或 niri IPC module。parser 接受 bottom/left/right position，但 early framebuffer renderer 当前只允许 top，并在发布 VFS generation 前拒绝其他位置。

这里的 Super+右键 compositor resize 优先级只适用于 bar 不接收该坐标的情况；位于窗口前方且 `passthrough=false` 的 top/overlay bar 会像其他 pointer button 一样先吞掉它，`passthrough=true` 或被普通窗口覆盖的 bottom bar 才把它交给下方窗口。

## swww 式壁纸控制

`crates/shell` 已提供无分配 swww 风格 CLI parser 与 `WallpaperDaemon` 状态机。kernel 启动 daemon 状态机但不选择图片；PID 2 的首个有效 desktop policy commit 才等价选择 `swww img /usr/share/backgrounds/slopos-aurora.ppm`。后续控制仍由 kernel 图形 monitor 接受带或不带 `swww` 前缀的命令：

- `img <path>` 设置图片，可选 output；两个兼容短名继续命中 bootstrap asset，其余绝对路径由 root VFS 读取，相对路径当前以 `/usr/share/slopos/` 为基准；
- `clear [RRGGBB]` 设置六位十六进制纯色，省略颜色时为黑色，也可选 `--outputs/-o`；
- `query` 返回 output geometry 与当前 image；
- `kill` 停止 daemon；
- `swww-daemon` 在 kill 后重新启动并清空旧 image；
- `--outputs/-o`、`--resize crop|fit|no|stretch`、等价于 resize no 的 `--no-resize`、`--fill-color RRGGBB`、`--filter/-f Nearest|Bilinear|CatmullRom|Mitchell|Lanczos3`、九向 `--crop-gravity`、`--transition-type/-t`、`--transition-step`、`--transition-fps`、支持小数秒的 `--transition-duration`、四分量 `--transition-bezier`、宽高二分量 `--transition-wave`、`--transition-angle`、`--transition-pos` 与 `--invert-y true|false`；
- VFS 中发现的 environment 文件以 `SWWW_TRANSITION*`（包括 `SWWW_TRANSITION_BEZIER/WAVE`）与 `SWWW_INVERT_Y` 提供 boot/reload 默认值，仓库默认源是 `assets/swww.env`；CLI 逐项覆盖 environment；
- `none`、step 驱动且不使用 easing 的 `simple`、使用 easing 混色的 `fade`、`left/right/top/bottom`、带任意整数角度的 `wipe`、在同一方向扫线上叠加可配置周期/振幅的 `wave`、带 bottom-origin pixel/percentage position 的 `grow/outer`、固定中心别名 `center`，以及确定性解析的 `any/random` transition。位置接受 `0.5,0.5`、`200,400` 与 center/top/left/right/bottom/四角别名；`invert-y` 切换为 framebuffer top-origin。

两个 12×8 bootstrap P3/PNM asset 在启动时完整校验 header、尺寸、max value、component 范围和精确 pixel 数。非 registry 路径则进入独立的 8 KiB 双 bank broker：desktop task 发布原始/规范化路径、output 与已解析 transition generation，block task 通过 ext4 walker 异步读齐一个或两个 block，在借用字节上用同一 parser 校验 P3 plain 或 P6 binary header/raster 后唤醒 desktop；renderer 完成 transition 后才 acknowledge，因此 block task 不会覆写仍被 current/previous image 引用的 bank。失败结果占用非 active bank，不改变当前图片。

renderer 把同尺寸 current/previous image 逐像素 blend 或 mask 到 GOP；wipe 用 output-space 投影线，wave 在投影线切向坐标上用 16 段插值正弦表叠加宽度周期与高度振幅，grow/outer 用相对所选 origin 最远角归一化的圆形半径，因此 percent、pixel position 与 wave 尺寸都按 1024×768 output 而不是 12×8 source asset 解释。落屏几何使用有理数边界把每个 source pixel 映射到 destination rect：crop/fit 保持宽高比、no 居中原尺寸、stretch 覆盖完整 output；crop 的九向 gravity 选择负 excess 的锚点，fit/no 的 padding 使用 fill color。默认 Nearest 沿用 source-pixel rectangle；显式 Bilinear 使用四点插值，CatmullRom/Mitchell 使用独立 4×4 cubic convolution，Lanczos3 使用 6×6 windowed-sinc；四者共享固定 2,048-pixel decode bank、16.16 反投影和 4×4 output block，no-resize/原尺寸时 filter 无作用。不同尺寸的 bounded P3/P6/PNG 仍会加载，但 transition 明确回退为 `none`。纯色状态直接填充 framebuffer，并让 `query` 以 `0xRRGGBB` 报告当前值。非 simple transition 以向上取整的 duration×fps 推导采样区间，再限制为最多 16 个区间/17 帧；每个线性进度用整数二分反解 cubic Bezier 的 x，再把有符号 y clamp 到 0..1 后用于 fade 混色或几何 mask，simple 保留 step 驱动的无 easing 有界采样。交互测试先切到 embedded Sunset 并以默认 2 秒、30 fps 完成 17 个 center 采样帧，再以 `/usr/share/slopos/vfs-wallpaper.png` 跨 inode 30 的两个 block 读入 6144-byte Adam7 16-bit RGB PNG、完成动态 DEFLATE、七 pass filter decode、RGB8 scatter 与第二段 center transition，并由 `query` 返回原始路径；随后依次请求不存在的 `missing.ppm` 与存在但并非 PNM/PNG 的 `/etc/slopos/system.conf`，后续三次 query 都仍返回前一图片。最后继续验证 kill/restart、`none`、`clear 1a2b3c`、纯色 query、origin `0,0` 的 grow、angle 30° 的 wipe、`.1` 秒产生 4 帧的 wipe、`0,0,1,0` 曲线令 fade 中点从 128 变为 32、`40,24` 的 wave，以及 Bilinear/CatmullRom/Lanczos3 的精确 stretch pixel；再恢复默认 Nearest/crop 后执行全部 niri 回归。11 项 swww/PNM/PNG、31 项 niri layout/shell、11 项 Waybar JSONC/CSS 与 5 项 desktop commit/event protocol 测试，共 58 项。

命令与 transition 语义依据 [swww 官方 README](https://github.com/LGFae/swww)、[`swww-img(1)`](https://raw.githubusercontent.com/LGFae/swww/main/doc/swww-img.1.scd) 与 [`swww-clear(1)`](https://raw.githubusercontent.com/LGFae/swww/main/doc/swww-clear.1.scd)。初始 environment/hash/image policy 已由用户进程提交，environment 默认值也参与后续四文件 VFS 原子重载；daemon state、path broker、image decode/render 仍在 kernel，而不是常驻用户进程或 Unix socket。当前路径最长 96 ASCII bytes、压缩文件最多 8 KiB，支持 max value 1..255 的 P3、单 byte sample P6，以及 non-interlaced/Adam7 的 1/2/4/8/16-bit grayscale、8/16-bit grayscale+alpha/RGB/RGBA 与 1/2/4/8-bit indexed PNG。PNG 逐 chunk 校验 CRC，zlib 路径校验 Adler-32并完整支持 stored/fixed/dynamic DEFLATE、连续多个 IDAT、filter 0–4，以及 palette/灰度/RGB `tRNS`；packed grayscale 会逐 sample 展开，16-bit sample 则线性缩放到 RGB8，透明 key 都在缩放前按原始 bit depth 比较，alpha 当前合成到黑底。Adam7 分别计算并 unfilter 七个 pass，在独立固定 scratch 区解码后 scatter 到 RGB8，避免覆盖未消费的交错 sample。压缩输入仍受 8 KiB bank 限制，解压 scratch/RGB 共受 24 KiB 与 2,048-pixel 上限。图形终端会把路径转大写，loader 因而以 ASCII 小写查找，尚不能表达大小写敏感的混合大小写文件名。也没有 Wayland layer-shell、多 output、JPEG/GIF decode、animated image cache、frame callback/timing 或 damage tracking。filter 默认仍为兼容既有 early desktop 的 Nearest，而非官方 Lanczos3；Bilinear 是真实四点插值，CatmullRom 与 Mitchell-Netravali 分别使用独立的定点 4×4 cubic convolution 核，Lanczos3 使用 6×6 windowed-sinc 核与 32 段定点正弦插值。angle 只接受 0–359 整数并以四象限线性方向近似而非浮点三角函数，wave 也用定点查表正弦而非逐像素浮点 `sin`。Bezier 与 wave 分量截断到万分位，Bezier x1/x2 依 CSS 型单值 easing 约束在 0..1，wave width 必须为正。duration 接受小数秒并截断到毫秒精度，参与帧采样数推导，但尚不按真实墙钟调度帧。同步 framebuffer renderer 为限制最坏 CPU 时间，把非 simple transition 限制为最多 17 帧，simple 也继续把极小 step 限制到同一上限，因此不声称二进制或动画时序完全兼容 swww。
