# Runtime evidence

所有证据均可从源码重新生成，不提交 ESP、ELF、OVMF VARS 或 PPM 等大型生成物。

| 文件 | 生成方式 | 证明范围 |
|---|---|---|
| `serial.log` | `make test-boot` | OVMF/UEFI、ELF、`ExitBootServices`、XSDT/MADT、memory、两个常驻 CPL3 进程、cooperative/timer switch、policy/config event、eBPF、PCI/virtio INTx、ext4、async 与桌面循环 |
| `uefi-debugcon.log` | `make test-boot` | loader 独立 debugcon 日志 |
| `interaction-serial.log` | `make test-interaction` | PS/2 键盘触发 VFS 配置 reload/rollback、embedded 与 root-VFS swww image/clear/query、missing-path 保留旧图、top-left grow、30° wipe、fit/fill、right crop 与 stretch，鼠标横拖 viewport、Super+右键缩放、点击 Waybar workspace，`cooldown-ms=150` 双包限流、keyboard 与四组 Mod+IntelliMouse bind 驱动首末列导航/重排、PageUp/PageDown workspace identity 重排、整列/单窗跨 workspace 转移、tiled/floating full-output 与精确恢复、显式与 toggle tiled/floating layer、normal/tabbed display、固定与左右方向合并/拆列、显式/preset 窗高/reset/重排/聚焦、gap-aware resize/maximize/center/expand 与关闭 |
| `custom-config-serial.log` | `make test-desktop-custom-config` | 4082-byte niri override 的 rule size/tabbed display、opening/focus/三级 state、opacity/空心 border/shadow/floating position 与 1392-byte Waybar override 一起从临时 ext4 原子应用；Waybar marker 记录 4/12px margin、1000×40 surface、44px exclusive reserve、非固定 center 与 top layer，随后经 PID 2 分块 hash、两代 policy、fullscreen 上方 surface click、left/center placement、真实 workspace 点击、alternate format 与三键/双向滚轮 action 生效 |
| `custom-config-uefi-debugcon.log` / `custom-config-qemu.log` | `make test-desktop-custom-config` | 自定义配置回归的 loader 与 QEMU 输出 |
| `waybar-overlay-serial.log` | `make test-desktop-custom-config` | 996-byte `mode: "overlay"` fixture 解析为 overlay layer、零 exclusive reserve、passthrough 与 visible，并让 bar 下方的 System 关闭按钮收到左击 |
| `waybar-overlay-uefi-debugcon.log` / `waybar-overlay-qemu.log` | `make test-desktop-custom-config` | overlay/passthrough 第二次短启动的 loader 与 QEMU 输出 |
| `page-fault-serial.log` | `make test-page-fault` | 自有页表的未映射访问、vector 14、error、RIP、CR2 与 fatal boundary |
| `journal-injection-serial.log` | `make test-journal-replay` phase 1 | commit 已 flush、home 尚未 checkpoint 的 dirty disk |
| `journal-replay-serial.log` | `make test-journal-replay` phase 2 | 普通 kernel mount-time replay、清理、继续完整启动 |
| `desktop.png` | `scripts/capture-desktop.sh` | niri 式 column strip 与 Waybar 式顶部三区域 |
| `terminal-status.png` | `make test-interaction` | 图形终端对键盘命令的实际响应 |
| `window-moved.png` | `make test-interaction` | titlebar drag 后 terminal 离屏、后续 column 进入 viewport |
| `window-resized.png` | `make test-interaction` | `Mod+Equal` 把 focused terminal column 从 488 px 放大至 588 px |
| `column-reordered.png` | `make test-interaction` | `Mod+Shift+Right` 把 focused terminal column 从 x=16 重排至 x=520 |
| `niri-window-workspace-target.png` | `make test-interaction` | stacked System 经 `move-window-to-workspace 2` 单独进入 Config，成为 x=520/y=56/488×696 全高列 |
| `niri-window-workspace-returned.png` | `make test-interaction` | `move-window-to-workspace "main"` 只送回 System，main 恢复 Terminal x=16/System x=520 两列 |
| `niri-column-workspace-target.png` | `make test-interaction` | `move-column-to-workspace 2` 将 Terminal/System stack 完整送入 Config，二者同在 x=520 且各高 340 |
| `niri-column-workspace-returned.png` | `make test-interaction` | `move-column-to-workspace "main"` 保留 stack 状态送回，二窗仍在居中的 x=268 上下排列 |
| `niri-focus-column-last.png` | `make test-interaction` | PS/2 Home/End 路径上的 `Mod+End` 直达末列 System，并同步焦点环与 Waybar title |
| `niri-column-moved-first.png` | `make test-interaction` | `Mod+Ctrl+Home` 把 focused System 完整列从 x=520 移到 strip 首端 x=16 |
| `niri-column-moved-last.png` | `make test-interaction` | `Mod+Ctrl+End` 把同一列送回 strip 末端 x=520 |
| `niri-focus-column-first.png` | `make test-interaction` | `Mod+Home` 直达首列 Terminal，并在不改变顺序的情况下恢复焦点 |
| `niri-workspace-moved-down.png` | `make test-interaction` | `Mod+Shift+PageDown` 把含 Terminal/System 的 named `main` 整体从 workspace 1 移至 2；Waybar 显示 `1 [2] 3` 且两窗几何不变 |
| `niri-workspace-reordered-name.png` | `make test-interaction` | 重排期间 `focus-workspace "config"` 按 identity 聚焦已位于 workspace 1 的 Config，而不是旧物理索引 2 |
| `niri-workspace-moved-up.png` | `make test-interaction` | named action 返回位于 workspace 2 的 `main` 后，`Mod+Shift+PageUp` 把完整 workspace 恢复至 `[1] 2 3` |
| `niri-wheel-workspace-down.png` | `make test-interaction` | `Mod+WheelScrollDown` 的真实 IntelliMouse 四字节 packet 将焦点从 main 切到 workspace 2 的 Config；串口记录 modifier bitmap `0x1` |
| `niri-wheel-cooldown.png` | `make test-interaction` | 两个紧邻的 `Mod+WheelScrollDown` packet 中首个切到 Config，第二个被同一 bind 的 150 ms deadline 抑制；截图仍停留在 workspace 2，串口记录 `accepted=false remaining_ms=100` |
| `niri-wheel-column-focus-right.png` | `make test-interaction` | `Mod+Shift+WheelScrollDown` 以 bitmap `0x5` 聚焦右列 System，列顺序和几何不变 |
| `niri-wheel-column-workspace-down.png` | `make test-interaction` | `Mod+Ctrl+WheelScrollDown` 以 bitmap `0x3` 将 focused System 整列移到 Config 右侧并切到 workspace 2 |
| `niri-wheel-column-moved-right.png` | `make test-interaction` | `Mod+Ctrl+Shift+WheelScrollDown` 以 bitmap `0x7` 将 Terminal 从 x=16 重排至 x=520；反向滚轮随后恢复原序 |
| `niri-tiled-fullscreen.png` | `make test-interaction` | `fullscreen-window` 让 tiled Terminal 无装饰独占 x=0/y=0/1024×768，Waybar 与 System 不参与合成或 pointer hit-test |
| `niri-tiled-fullscreen-restored.png` | `make test-interaction` | 第二次 `fullscreen-window` 恢复 Terminal 原 tiled x=16/y=56/488×696，System 与 Waybar 同时重新出现 |
| `niri-explicit-floating.png` | `make test-interaction` | `move-window-to-floating` 把 Terminal 幂等地移入 x=16/y=161/488×485 floating layer |
| `niri-floating-fullscreen.png` | `make test-interaction` | 同一动作让 floating Terminal 独占完整 output，隐藏下层 tiled System 与 Waybar |
| `niri-floating-fullscreen-restored.png` | `make test-interaction` | 退出全屏后 Terminal 精确恢复原 floating x=16/y=161/488×485 和 top-layer 关系 |
| `niri-explicit-focus-tiling.png` | `make test-interaction` | `focus-tiling` 显式聚焦下层 System，Terminal 仍在 floating layer 上方 |
| `niri-explicit-focus-floating.png` | `make test-interaction` | `focus-floating` 显式把 focus 与 Waybar title 恢复到上层 Terminal |
| `niri-explicit-tiling.png` | `make test-interaction` | `move-window-to-tiling` 把 Terminal 幂等地移回 x=520/y=56/488×696 tiled strip |
| `niri-window-floating.png` | `make test-interaction` | `Mod+V` 把 Terminal 从 tiled strip 抽成 x=16/y=161/488×485 浮窗；剩余 System 单列自动居中，浮窗仍合成在其上 |
| `niri-floating-focus-tiling.png` | `make test-interaction` | `Mod+Shift+V` 把 focus 切到浮窗下方的 tiled System；Waybar title/focus ring 指向 System，inactive Terminal 仍保持 top layer |
| `niri-floating-window-moved.png` | `make test-interaction` | 切回 floating focus 后，`Mod+Ctrl+J` 按 50 px step 把 Terminal 从 y=161 移到 y=211 |
| `niri-column-stacked.png` | `make test-interaction` | `Mod+Comma` 把右侧 System 顶窗 consume 到 Terminal column 底部，两窗保持相同列宽并由 `always-center-single-column` 在 x=268 居中上下平铺 |
| `niri-column-tabbed-system.png` | `make test-interaction` | `Mod+W` 把两窗 stack 切为 tabbed display，只有第 2/2 个 System 占据 x=268/y=56/488×696，左侧分段指示器标出 active tab |
| `niri-column-tabbed-terminal.png` | `make test-interaction` | tabbed display 中按 `Mod+K` 切到相同几何的第 1/2 个 Terminal，Waybar title、focus ring 与侧标同步 |
| `niri-window-height-increased.png` | `make test-interaction` | `Mod+Shift+Equal` 按 gap-aware 10% 把 focused System 从 340 px 增至 411 px，同时补偿同列 Terminal |
| `niri-preset-window-height.png` | `make test-interaction` | `Mod+Ctrl+Shift+R` 按 KDL preset 把 focused System 从 50%/340 px 切到 66.7%/458 px |
| `niri-preset-column-width.png` | `make test-interaction` | `Mod+R` 按 gap-aware KDL preset 把 Terminal 从 50%/488 px 切到 66.7%/656 px，`Mod+Shift+R` 再恢复 |
| `niri-window-moved-up.png` | `make test-interaction` | `Mod+Ctrl+K` 把 focused System 从列底移到 Terminal 上方，同时保持 System focus 与 Waybar title |
| `niri-window-focus-up.png` | `make test-interaction` | `Mod+K` 在纵向 stack 内从 System 聚焦到上方 Terminal，focus ring 与 Waybar title 同步变化 |
| `niri-column-expelled.png` | `make test-interaction` | `Mod+Period` 把底部 System expel 回右侧 column，并按 niri 语义把焦点留在 Terminal |
| `niri-consume-or-expel-left-stacked.png` | `make test-interaction` | `Mod+BracketLeft` 把右侧 singleton System 合并到左邻 Terminal 列底部，focused System 位于 x=268/y=412 |
| `niri-consume-or-expel-left-expelled.png` | `make test-interaction` | 再按 `Mod+BracketLeft` 把 stack 中 focused System 拆成左侧 singleton，System 继续居中且 Terminal 留在右侧 strip |
| `niri-consume-or-expel-right-stacked.png` | `make test-interaction` | `Mod+BracketRight` 把左侧 singleton System 合并到右邻 Terminal 列底部，焦点与 488×340 几何保持 |
| `niri-consume-or-expel-right-expelled.png` | `make test-interaction` | 再按 `Mod+BracketRight` 把 focused System 拆到右侧 x=520，恢复 Terminal x=16/System x=520 的两列 |
| `niri-column-centered.png` | `make test-interaction` | `Mod+C` 只移动 viewport，把 488 px Terminal 从 x=16 精确居中到 x=268，System 仍保留在右侧 strip |
| `niri-column-maximized.png` | `make test-interaction` | `Mod+F` 把 Terminal 从 488 px 最大化至保留两侧 gap 的 992 px，再次按键恢复原宽 |
| `niri-window-maximized-to-edges.png` | `make test-interaction` | `Mod+M` 把 Terminal 从 x=16/y=56/488×696 无 gap 最大化至 Waybar 下方工作区的 x=0/y=40/1024×728，再次按键精确恢复 |
| `niri-column-expanded.png` | `make test-interaction` | System 为 319 px 时，`Mod+Ctrl+F` 把 Terminal 从 488 px 扩到 657 px，使两列与三个 16 px gap 恰好填满 output |
| `niri-visible-columns-centered.png` | `make test-interaction` | `Mod+Ctrl+C` 把两条 319 px 列及 16 px 内部 gap 作为 654 px 整体居中到 x=185..839 |
| `mouse-resized.png` | `make test-interaction` | Super+右键横拖把 focused terminal column 从 488 px 放大至 584 px |
| `niri-workspace-number.png` | `make test-interaction` | KDL `focus-workspace 2` 经真实 `Mod+2` 输入切到 Config，顶部显示 active 2 |
| `niri-move-workspace-number.png` | `make test-interaction` | KDL `move-column-to-workspace 3` 经真实 `Mod+Ctrl+3` 把 Terminal 整列移到 active 3，并自动追加可见 workspace 4 |
| `niri-workspace-name.png` | `make test-interaction` | KDL `focus-workspace "config"` 经真实 `Mod+Alt+C` 输入按名称切到 Config |
| `niri-move-workspace-name.png` | `make test-interaction` | KDL `move-column-to-workspace "config"` 经真实 `Mod+Ctrl+Alt+C` 把 Terminal 整列移入 named workspace |
| `niri-workspace-previous.png` | `make test-interaction` | KDL `focus-workspace-previous` 经真实 `Mod+Tab` 返回 Config；第二次按键再回 main |
| `waybar-workspace-click.png` | `make test-interaction` | 点击顶部数字 `2` 后 `niri/workspaces` 显示 active 2，并切入 Config surface |
| `custom-config-workspace-click.png` | `make test-desktop-custom-config` | KDL `open-focused true` 在配置发布时直接激活 workspace `2`，并显示由 `open-fullscreen true` 产生的 Config x=0/y=0/1024×768；`layer: "top"` 让 inset Waybar 仍在 fullscreen 上方，空白 surface 左击被 `passthrough: false` 吞掉；x=200/y=350 像素为不透明 `171c2b`，证明 fullscreen 窗口忽略 `opacity 0.75` |
| `custom-config-edge-maximized.png` | `make test-desktop-custom-config` | 对初始 fullscreen Config 执行 `Mod+Shift+F` 后，精确恢复底层 x=0/y=44/1024×724 edge maximize；top Waybar 保持可见，layout gap/border 仍移除 |
| `custom-config-default-column-width.png` | `make test-desktop-custom-config` | 再执行 `Mod+M` 与 `Mod+F` 后，先恢复底层 992×338 column maximize，再恢复 KDL app rule 的 66.7%×50% 初始尺寸 x=184/y=60/656×338；左侧单 tab 指示条、4 px `ffb86c` 空心 active border 与软阴影可见。surface 两点、border、shadow 的五个像素验证 niri decoration/alpha；bar x=0/12、y=20 与 x=20、y=2/4 的四点验证 `margin: "4 12"` 将 surface 限制在 `(12,4)–(1012,44)`。workspace block 位于左右 block 间而非 output 固定中心 |
| `custom-config-floating-position.png` | `make test-desktop-custom-config` | `Mod+Alt+V` 对 992×338 Config 应用 `x=24 y=24 relative-to="bottom-right"`，得到 x=8/y=406，精确保留右、下各 24 px working-area 间距 |
| `custom-config-floating-remembered.png` | `make test-desktop-custom-config` | Config 向下移动到 y=430 后经历 floating→tiling→floating，仍恢复 x=8/y=430，而不是重新应用默认 y=406 |
| `custom-config-on-click.png` | `make test-desktop-custom-config` | JSONC 为右侧 clock 配置 `format-alt: UTC ALT` 与 `on-click: status`；同一次左击切到 alternate format、显示 STATUS 响应，并保留 Terminal 中尚未执行的 `ABO` |
| `custom-config-format-restored.png` | `make test-desktop-custom-config` | 第二次左击让 clock 从 `UTC ALT` 切回 `UTC`，并再次执行同键 STATUS action |
| `custom-config-on-click-right.png` | `make test-desktop-custom-config` | clock 的 `on-click-right: help` 经真实 PS/2 右键命中，Terminal 显示受限 HELP 响应 |
| `custom-config-on-click-middle.png` | `make test-desktop-custom-config` | clock 的 `on-click-middle: swww query` 经真实 PS/2 中键命中，Terminal 显示 Aurora query 响应 |
| `custom-config-scroll-up.png` / `custom-config-scroll-down.png` | `make test-desktop-custom-config` | IntelliMouse 四字节滚轮向上/下命中 clock action，并无过渡切换 Sunset/Aurora |
| `waybar-overlay-passthrough.png` | `make test-desktop-custom-config` | `mode: "overlay"` 把 Terminal/System 工作区扩到 y=16，同时 bar 的紫色底边在 y=39 覆盖两窗 titlebar，证明 overlay 层位于普通窗口上方 |
| `waybar-overlay-click-through.png` | `make test-desktop-custom-config` | 点击 overlay clock 下方的 System 关闭按钮后 System 消失、Terminal 重排居中；日志没有 clock action，证明 preset 的 `passthrough: true` 生效 |
| `workspace-config.png` | `make test-interaction` | `slopos-config` window rule 与 `Mod+Down` 切入 named workspace |
| `wallpaper-cleared.png` | `make test-interaction` | `swww clear 1a2b3c` 直接填充 framebuffer，Terminal 同时显示 `SWWW COLOR APPLIED` |
| `wallpaper-vfs-loaded.png` | `make test-interaction` | `swww img /usr/share/slopos/vfs-wallpaper.png` 令 block task 从 inode 30 异步读齐两个 block，经双 bank 解码并发布 PNG RGB raster 后完成 center transition；Terminal 显示 `SWWW VFS IMAGE APPLIED` |
| `wallpaper-grow-top-left.png` | `make test-interaction` | `grow --transition-pos top-left` 完成后显示 Aurora；serial 锁定解析后的 output origin 为 `0,0` |
| `wallpaper-wipe-angle.png` | `make test-interaction` | `wipe --transition-angle 30` 完成后恢复 Sunset；serial 锁定 angle 30 与默认 duration×fps 经上限约束后的 17 个采样帧 |
| `wallpaper-wave.png` | `make test-interaction` | `wave --transition-wave 40,24` 完成后显示 Aurora；serial 锁定定点宽高 `400000,240000`、默认 Bezier 中点 155 与 17 个采样帧 |
| `wallpaper-fit-fill.png` | `make test-interaction` | Aurora 以 1024×682 居中 fit，顶部/底部 padding 使用 `--fill-color 123456`；底部像素锁定为 `123456` |
| `wallpaper-crop-right.png` | `make test-interaction` | Aurora 以 1152×768 crop 并用 right gravity 锚在 x=-128；gap 像素锁定为 `442299` |
| `wallpaper-stretched.png` | `make test-interaction` | Sunset 用非等比 source-pixel rect 覆盖完整 1024×768；底边像素锁定为 `221133` |
| `wallpaper-bilinear.png` | `make test-interaction` | Aurora stretch 经 16.16 四点 Bilinear 与 4×4 bounded output block 落屏；窗口间隙 `(512,300)` 锁定插值色 `2bc5ce` |
| `wallpaper-switched.png` / `wallpaper-only.png` | `make test-interaction` | Sunset transition 后的窗口背景，以及关闭全部 tile 后的完整壁纸 |
| `qemu-test.log` | `make test-boot` | QEMU stderr/stdout；正常测试通常为空 |

核心启动命令由 `scripts/test-boot.sh` 固定，等价参数为：

```text
qemu-system-x86_64
  -machine q35,accel=tcg
  -cpu qemu64
  -m 256M
  -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd
  -drive if=pflash,format=raw,file=target/OVMF_VARS_4M.test.fd
  -drive if=virtio,format=raw,file=target/slopos-esp.img
  -drive if=virtio,format=raw,file=target/slopos-root.ext4
  -serial file:evidence/serial.log
  -debugcon file:evidence/uefi-debugcon.log
  -global isa-debugcon.iobase=0x402
  -display none
  -monitor none
  -no-reboot
```

用户进程证据从 UEFI 日志开始：loader 从 ESP 读取 26344-byte `/slopos/init.elf`，BootInfo v2 令 kernel 在 `LOADER_DATA` allocation 保留同一大小的校验副本。executor/block task 挂载 root 后，`SLOPOS-VFS` marker 证明从 inode 23 `/sbin/slop-init` 和 inode 24 `/sbin/slop-shell` 各跨七个逻辑块读取 26344/26560 bytes，init 为 `matches_boot=true`，shell role 为 `desktop-service`。process-table marker 记录 capacity 4、两个 Ready PID 与每进程 fd capacity 8；两个 load marker 分别记录 2608/2528 load/memory bytes，以及不同 CR3、user code、两页 stack和各自 physical frame。两个 initial-stack marker 都记录 `RSP=0x40002ec0`、`stack_pages=2`、16-byte alignment、`argc=2`、3 项 environment、9 对 auxv 与 320 used bytes，PID 2 的 argv 为 `/sbin/slop-shell --session`；两个 ELF 都在发出 syscall 前逐项核对。fast-path marker 证明 CPUID 检查后的 MSR readback 为 `STAR=0x10000800000000`、`FMASK=0x47700`、`EFER.SCE=true`，且 LSTAR 指向 kernel entry。

调度 marker 先证明 PID 1/2 cooperative 往返；随后两个 ELF 分别进入约 100,000,000 TSC tick 的无 syscall 窗口，`timer preempt from=1 to=2` 与反向 marker 记录 tick、每 PID 首次非零抢占计数、下一状态和独立 CR3。timer/suspended-syscall frame 均经 `IRETQ` resume，使 timer 捕获的 RCX/R11 按普通 GPR 恢复，而不套用 syscall clobber 语义。后续 yield 期间两者分别保持自己表中的 fd 3，证明同号 descriptor ownership 隔离。每个 async I/O 前都有 `Blocked`，completion marker 记录 `blocked->runnable`。PID 1 的 17 个 syscall 完成 system config 读取和跨页可逆 patch；buffer 位于 `0x40001fe0..0x40002020`，四个 I/O marker 均记录 `user_pages=2`/`cross_page=true`。它显式 close 第二个 fd，再进入 blocked `wait4` 常驻 supervisor；若 PID 2 更早到 config wait，broker 会继续运行 PID 1，因此稳定 marker 的 `init=wait4` 与实际 process state 一致。

PID 2 首轮打开 inode 20 Waybar JSONC，以 offset 0/256/512/768 的四个 chunk 读齐默认 904 bytes并验证 EOF，再打开 inode 17 swww environment，读齐默认 244 bytes并验证 EOF；读取器允许非空 Waybar/swww 分别扩展至 4096/512 bytes。swww 文件现在显式携带 angle 45、position center 与 invert-y false 默认值。`SLOPOS-DESKTOP-SERVICE` submission marker 记录 protocol 1、40-byte commit、`waybar-provider/swww-policy` capability、CPU 0、Memory 36、Aurora，以及实际 VFS hash `0xd34d4a92c88d065b`/`0xb6ea39f5fd14c33a`；私有 syscall marker 记录编号 `1397489665` 与 result 0。第二个私有 syscall（`1397489666`）以 32-byte user buffer、event kind 与 `after_generation` 进入 Blocked；desktop task随后记录 snapshot generation 1、相同 owner/capability/provider 值及 kernel renderer boundary，swww marker 证明初始 Aurora 只在 policy 到达后应用。ack marker 再唤醒 block task，completion marker 记录 `Blocked → Runnable`、`policy-applied` generation 1；PID 2 decode event 后只在首次写一次 ready message，然后等待 `config-applied`。

`make test-desktop-custom-config` 不修改仓库标准镜像：脚本复制 ESP/rootfs，用 `debugfs` 把默认 3797-byte niri KDL 替换成 4082-byte 用户文件，并将 904-byte Waybar JSONC 替换成 1392-byte 用户文件。niri override 覆盖 opening focus、三种 opening state、66.7%×50% 尺寸、tabbed display、floating anchor、focus ring、border、shadow、background-mode 与 opacity；Waybar override 加入 `margin: "4 12"`、`fixed-center: false`、`layer: "top"`、`exclusive: true`，交换 window/workspace 的 left/center placement，并给 clock 配置 alternate format、三键与双向滚轮 action。Waybar geometry marker 精确记录 x=12/y=4/1000×40、margin 4/12/4/12、top layer 与 reserved top 44；因此 Config 的三级底层几何变为 x=16/y=60/992×338 column maximize、x=0/y=44/1024×724 edge maximize 和 x=184/y=60/656×338 rule size，而 fullscreen 保持 x=0/y=0/1024×768。首张截图证明 top bar 合成在 fullscreen 上方，空白栏点击 marker 又证明 `passthrough=false`；脚本同时核对 fullscreen surface `171c2b`、普通 surface x=200/800、y=350 的 `222247`/`222a4b`、border x=182 的 `ffb86c`、shadow x=844/y=200 的 `0a0a1f`，以及 bar x=0/12、y=20 和 x=20、y=2/4 的 wallpaper/panel，直接证明装饰、alpha、横向与顶部 margin。首次 floating rect 为 x=8/y=406/992×338，移动后 y=430 经 tiling 往返仍恢复。`custom-config-serial.log` 证明 PID 2 两轮都读到 1392 bytes，并提交相同的非默认 hash `0xb3e52394b278b991`；末尾仍阻塞在 `config-applied after_generation=1`。退出 fullscreen 后，workspace block 会随左侧 Config/Terminal title 宽度改变而移动；更新后的真实 PS/2 坐标依次点击 `1`/`2`/`1`，既证明非固定 center/hit-test 共用几何，也证明 main 的旧 Terminal 局部焦点未被覆盖。clock 左击往返 `UTC`/`UTC ALT` 并执行 STATUS，右击执行 HELP，中击执行 SWWW QUERY，双向滚轮切换 Sunset/Aurora；输入缓冲仍补成 ABOUT。

同一脚本随后从标准 rootfs 重建短启动并注入 996-byte `mode: "overlay"` fixture。preset marker 为 `layer=overlay mode=overlay exclusive=false passthrough=true visible=true reserved_top=0`，Terminal/System 因零 reserve 从 y=16 开始，截图 y=39 的 `6558f5` 像素证明 bar 底边覆盖 titlebar；点击 clock 下方仍关闭 System，且没有产生 clock action，证明 pointer 穿透。十三张截图覆盖 opening focus、fullscreen/edge/column/rule 四态、Waybar geometry/workspace/top/overlay/passthrough、alternate、输入/action 与壁纸；两次启动全程没有 exit/FATAL，宿主 fsck 通过。当前不覆盖 custom `modes`、Sway IPC/visibility signal、自动文件监听、POSIX shell 或完整配置 schema。

当前五条主要 QEMU 日志都出现 `userspace runtime parked init=wait4 desktop=config-applied`；初始 VFS config generation 1 的 acknowledge 唤醒 PID 2，服务重读两份文件、提交并收到 policy generation 2，随后在 `after_generation=1` 再次稳定休眠。交互日志进一步证明 config generation 2 唤醒 service 并产生 policy generation 3；非法 reload 保留 config generation 2，没有 policy generation 4。脚本同时拒绝任何 `state=exited` 或 `FATAL`。`make test-process` 的 6 项宿主测试另行覆盖 child-exit wake、immediate zombie reap、initial stack、Blocked/Runnable/round-robin、PID/parent/child lookup/lifecycle、exit cleanup 和 per-process fd isolation/seek。当前证据不证明任意 exec、多 segment mapping、动态并发 syscall、通用 wait selector/options/orphan adoption 或通用 namespace mutation。

ELF parser 的宿主测试由 `make test-elf` 执行。10 项测试验证 ELF64 little-endian/x86-64/`ET_EXEC` header、program-header table、`PT_LOAD` data/BSS view，并拒绝 truncation、越界、`p_filesz > p_memsz`、非法 alignment/congruence、重叠 segment、W+X 与不属于 executable segment 的 entry。parser 无分配且 `no_std`；section header 与 dynamic linking 不参与当前装载。

shell/protocol 状态机由 `make test-shell` 验证。20 项 layout 测试覆盖 niri KDL、稳定 strip coordinate、相邻/首末列 focus+reorder、scroll/center/expand/normal+tabbed stack/consume+expel/gap-aware resize+maximize/close、app-specific 初始列宽高与拆列 display、指定非焦点列的 column/edge 初始最大化及完整列/单窗原子转移；10 项 niri shell 测试覆盖 named/anonymous workspace identity 与整体 reorder、80 项有界 keyboard/wheel bind 表、`cooldown-ms` 解析/拒绝、ordered workspace/size/display/floating-position/decoration/opening rule、tiled/floating 初始 geometry、opening focus、layer/fullscreen、column/window transfer 和动态 normalize；4 项 Waybar JSONC/format 测试另覆盖 signed 1/2/3/4-value margin、逐边优先级、fixed-center/exclusive default/reserve、bottom/top/overlay layer、四种 mode preset、`start_hidden`、passthrough、module option/format/action 与拒绝边界，3 项 CSS、8 项 swww、3 项 PNG 与 5 项 desktop protocol 测试覆盖各自 parse/state/reject 边界。swww/PNG 断言包含 resize/no-resize/fill/crop-gravity/filter parse/reject、Nearest/Bilinear sample、position alias/小数百分比/pixel、bottom-origin/invert-y、wipe/wave/grow/fade easing，以及 stored/fixed/dynamic DEFLATE、多 IDAT、RGBA、indexed palette、palette/灰度/RGB `tRNS`、filter 0–4、CRC 与容量拒绝，共 53 项。裸机从 `/etc/slopos/{niri.kdl,waybar.jsonc,waybar.css,swww.env}` 原子发布 config generation 1；apply marker 记录 3 个 workspace、71 个 bind、6 个 Waybar module config 与 12 条 CSS rule，PID 2 随后重读并发布 policy generation 2。

在既有 swww 回归之后，workspace transfer marker 先把 Terminal/System 合并：`move-window-to-workspace 2` 只将 focused System 送到 Config 右侧全高列，named action 送回后 main 恢复两列；再次合并后，`move-column-to-workspace 2` 将两窗 stack 一起送入 Config，二者同在 x=520 并各高 340，named action 送回时仍在 x=268 上下排列。第二次单窗往返再拆回两列并聚焦 Terminal。扩展 PS/2 marker 随后让 `Mod+End` 聚焦末列 System，`Mod+Ctrl+Home` 把它移到 x=16 首端，`Mod+Ctrl+End` 恢复至 x=520，`Mod+Home` 再聚焦首列 Terminal。`Mod+Shift+PageDown` 把包含两窗的 named `main` 连同焦点从 workspace 1 移到 2，日志保留 `previous=1`；重排中的 `Mod+Alt+C/M` 分别命中已位于 1/2 的 `config`/`main`，证明名称解析跟随 identity，`Mod+Shift+PageUp` 再恢复 `main` 到 workspace 1 和 `previous=2`。随后 QMP 只显式保持跨键鼠设备的 modifier key-down，QEMU i8042 仍为每次滚动生成 PS/2 IntelliMouse packet：`Mod` 的 `0x1` 双包 burst 先在 config 停留并抑制第二包，再与 main 往返，`Mod+Shift` 的 `0x5` 在 Terminal/System 往返聚焦，`Mod+Ctrl` 的 `0x3` 把 System 整列送到 config 后送回原 x=520，`Mod+Ctrl+Shift` 的 `0x7` 把 Terminal 右移至 x=520 后恢复 x=16。未匹配的无修饰滚轮仍由独立 custom-config 回归中的 Waybar clock action 消费。`Mod+Shift+F` 紧接着令 tiled Terminal 覆盖 x=0/y=0/1024×768，点击原 workspace 2 的隐藏 bar 坐标不会产生额外 Waybar marker，退出后恢复 x=16/y=56/488×696。随后显式 floating marker 证明 `Mod+Alt+V` 把 Terminal 移到 x=16/y=161/488×485，同一全屏动作也能覆盖 output 并精确恢复该 floating rect；`Mod+Alt+T`/`Mod+Alt+G` 精确聚焦下层 System/上层 Terminal，`Mod+Ctrl+V` 再把 Terminal 送回 x=520 tile。重排回左侧后，既有 toggle marker 继续证明 `Mod+V` 产生同一浮窗、`Mod+Shift+V` 在两层间双向切焦点、`Mod+Ctrl+J` 把浮窗下移到 y=211，第二次 `Mod+V` 将它送回 x=520 tile；原有完整 tiled 回归因而仍从相同初态执行。

swww VFS marker 在 floating 回归前证明 generation 1 把大写终端路径规范化为 `/usr/share/slopos/vfs-wallpaper.png`，从 inode 30 异步读取 6144 bytes/2 blocks，校验 PNG/CRC/Adler并解开 dynamic Huffman、两个 IDAT 与 filter 0–4 后以 `format=PNG` 发布；desktop 以默认 2 秒、30 fps 完成受限的 17-frame center transition 后才 acknowledge。generation 2 请求不存在的 `missing.ppm` 并得到 `not-found`；generation 3 实际读出存在的 `/etc/slopos/system.conf`，再由格式探测以 `invalid-ppm` 拒绝。三次后续 query 都返回 generation 1 的原始 image path，证明两类失败 bank 都没有替换或覆写当前像素。kill/restart/clear 回归之后，另外两组完整 parser→daemon→renderer marker 分别记录 grow 的 `position=0,0` 与 wipe 的 `angle=30 position=512,384`，两者也各完成 17 帧；紧接的 `.1` 秒 wipe 锁定 `duration_ms=100 sampled_step=85 frames=4`，其后的 fade 又锁定 `bezier=0,0,10000,0 midpoint=32`，证明自定义曲线经过 parser 后实际进入 renderer 进度函数；最后 wave marker 锁定 `wave=400000,240000 midpoint=155 frames=17`。随后 geometry marker/PPM 锁定 fit `0,43/1024×682/fill=123456`、right crop `-128,0/1152×768`、Nearest stretch 底边 `221133`、Bilinear stretch gap `2bc5ce`、CatmullRom `27d2d4` 与 Lanczos3 `25d5d6`；高成本 filter 后测试恢复默认 Nearest/crop，三点完整帧哨兵与既有 niri 截图 SHA-256 均保持不变。

交互日志用 `RELOAD` 发布/应用 config generation 2、唤醒 PID 2并发布 policy generation 3；诊断 `RELOAD BAD` 注入非法 CSS后记录 `invalid-waybar-style retained_generation=2`，没有 config generation 3或 policy generation 4。随后日志记录 Sunset `img`、17 帧 center transition、返回当前 image 的 `query`、kill/restart、无 transition 换图、`clear 1a2b3c`、返回 `0x1A2B3C` 的 query、恢复 Sunset；`Mod+Comma` 把 System consume 到 Terminal 列底部并让单列 x=268 居中。两次 `Mod+W` marker 验证 normal↔tabbed 往返：System 以 tab 2/2 独占 x=268/y=56/488×696，`Mod+K/J` 又精确切换 Terminal 1/2 和 System 2/2，切回 normal 后 System 恢复 x=268/y=412/488×340。`Mod+Shift+Equal` 产生 gap-aware 340→411 px 高度 marker，`Mod+Ctrl+R` reset 为 340 px，三次 `Mod+Ctrl+Shift+R` 产生 preset 340→458→221→340 px，再以 `Mod+Shift+Minus/Equal` 验证 269→340 px，并始终守恒同列像素。`Mod+Ctrl+K/J` 把 System 移到顶部再移回且 focused id 保持 1，`Mod+K/J` 上下聚焦，`Mod+Period` 把底窗 expel 到右侧并保留 Terminal 焦点；四条方向 marker 又证明 `Mod+BracketLeft/Right` 依次把 focused System 变为 x=268/y=412 stack、x=268/y=56 左 singleton、同一 stack 和 x=520/y=56 右 singleton。两次 `Mod+F` 把 Terminal 488→992→488 px 最大化/恢复，`Mod+R` 再按 KDL preset 把它切到 656 px，`Mod+Shift+R` 恢复 488 px。

后续 marker 还覆盖 titlebar viewport drag、键盘/pointer resize、相邻列重排、单列与可见集合居中、available-width expand、column/edge maximize、indexed/named/previous workspace focus+move、动态 `3→4→3`、Waybar 双向点击与逐窗 close。五十张 niri 行为截图现在覆盖 Mod+IntelliMouse workspace/column focus+move+reorder、workspace identity reorder 与重排后的 named focus、首末列 focus/reorder、完整列/单窗 workspace transfer、tiled/floating full-output 与原 layer/geometry 恢复、显式与 toggle tiled→floating→跨层 focus→浮窗 move→tiled、normal→双 tab→normal、显式/preset 窗高、窗口重排/聚焦、固定及双向合并拆列、column/preset/edge maximize、center 与 expand。它不证明自动文件监听、超过 4 个 workspace、横向 wheel key、bind repeat、`--focus=false` move flag、浮窗位置持久化、client-driven xdg fullscreen、完整 niri/Waybar、tab 点击/拖曳、通用 module action、独立 swww socket/layer-shell daemon、Wayland layer-shell 或其他图片格式。

eBPF 的宿主边界测试由 `make test-ebpf` 执行；裸机证据是 `serial.log` 中的 `SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42`。它只证明文档所列子集，不证明 map、attach point 或 Linux eBPF 兼容性。

ACPI parser 的宿主测试由 `make test-acpi` 执行。裸机日志记录 QEMU MADT 的 1 个 processor、1 个 IOAPIC、5 个 interrupt override，并记录硬件读取到的 LAPIC/IOAPIC ID、24 条 redirection 和 ISA route `2/1/12`；随后出现 timer Future 与 PS/2 交互事件，证明新路由实际收到了 IRQ。

PCI 枚举器的宿主测试由 `make test-pci` 执行。裸机日志包含 QEMU q35 的设备总数和实际 virtio-blk BDF；当前证据为 `00:03.0`、device ID `1001`，完整 region 校验后的 capability mask `0x1e`（configuration type 1–4）。OVMF 分配的 modern BAR base 为 `0xc000000000`，因此 CR3 证据同时包含跨 PML4 slot 的 7 个 table frame。

virtio layout 测试由 `make test-virtio` 执行。4 项宿主测试包含 read/write/flush descriptor direction。裸机 `SLOPOS-VIRTIO` 证据来自真实 descriptor DMA 与 INTx→waker→Future：queue size 8，root device 报告 524288 sectors并接受 flush，并执行 1 个双请求批次。timer preemption 与 desktop event wake 会改变两个进程和 cache probe 的合法交错；clean boot/interaction 日志覆盖 157–163 hit、119–128 miss、16–18 invalidation，以及 497/496 至 510/509 requests/interrupts。脚本解析最后一条 summary，核对 request 恒比 interrupt 多一、top-half 与 queue interrupt 相等，并把 summary 相对两次用户写的合法先后限制在 16–18 invalidation，而不要求某一种单一 interleaving。

ext4 parser 测试由 `make test-ext4` 执行。裸机日志证明 4096-byte block、65536 blocks、32 inodes、2 groups、group 0 inode table 37、root extent 39 和 6 个 root entries；superblock/group/inode/directory checksum 均由内核校验。`/sbin/slop-init` 是 inode 23、26344 bytes/seven blocks，`/sbin/slop-shell` 是 inode 24、26560 bytes/seven blocks。6144-byte PNG `vfs-wallpaper.png` 是 inode 30，长 `tEXt` ancillary chunk 延伸文件并让两个 IDAT 位于跨块读取路径，所以同时承担 multiblock/prefetch 与压缩 swww path-load probe；`deep-extent.bin` 是 inode 28，后者从 root index 进入 leaf block 104，验证 extent-block checksum 后读取 logical block 8 的 physical block 111，并将 logical block 7 的 hole 零填充。inode 29 的两个目录块均经 checksum parser，目标 hard link 在第二块解析为 inode 21。path walker 还从 inode 14 取得 inline target，并在同一父目录解析到 inode 21。

`write-probe.bin` 是 inode 31 / physical block 116。PID 1 先在 offset 123 处理两次跨页 64-byte payload并恢复；kernel probe 再处理两次 73-byte payload。ext4 层每次都执行整块 read-modify-write、flush、cache invalidation 与 fd 读回。固定 metadata 后镜像 SHA-256 为 `ab4fa4b5e50d258b74c47eb9474d2065b7d7673274b08f6446e65be4bc1b1309`；启动测试后的 hash 相同且 `e2fsck -fn` 报告 31/32 files、4243/65536 blocks。这只证明已分配数据块的有界原位写，不证明用户态文件增长或 metadata mutation syscall。

JBD2 宿主测试解析 big-endian v2 superblock，并拒绝 truncation、非法 geometry 和未知 feature；round-trip/corruption 测试覆盖 descriptor/data/commit，状态测试覆盖 ext4 recovery-bit CRC32C 恢复与 JBD2 sequence/start 转换。裸机 marker 证明 journal inode 8 的单一 extent 从 physical block 32801 开始，superblock 报告 4096 blocks、first 1、sequence 1、start 0、users 1、零 feature words，UUID 与 ext4 匹配。它不证明 journal clean 或 replay。

第二个裸机 marker 证明 sequence 1 / target block 116 的 descriptor/data/commit records 被写到 32802–32804；descriptor+data 和 commit 分别由 flush 隔开，三块读回一致，之后清零恢复。marker 明确带 `active=false`，因为尚未写 journal state 或 ext4 recovery bit。

第三个 marker 独立证明 ext4 recovery bit/checksum 与 JBD2 sequence 1/start 1 被持久化并读回；普通 ext4 parser 在 active 状态拒绝 mount。清理先归零 journal start，再清 recovery bit，最终宿主 hash/fsck 证明恢复。`transactions=0` 表示它尚未与上一组 records 组合。

第四个 marker 证明真正组合的单块 active data transaction：recovery/start 和 descriptor/data/commit 均跨 flush 持久化，DMA readback 验证此时可 replay；home block 116 checkpoint 后推进 sequence 2/start 0 并清 recovery。测试收尾清 records、恢复全 `P` home block并将 sequence 回卷到 1，因此启动后的 image SHA-256 仍为固定值且 `e2fsck -fn` 通过。

第五个 marker 证明 inode 31 所在 inode-table block 38 也作为 JBD2 home target：sequence 1 transaction 把 size/checksum 更新为 4095/valid，sequence 2 transaction 恢复 4096/valid，最终 journal sequence 为 3。两次 cache 失效后的 inode parser 均接受整块 metadata；测试回卷 sequence 后，固定 image hash 与 `e2fsck -fn` 再次证明完整恢复。

第六个 marker 证明 fd 3 的 append/truncate 与五 tag allocation transaction 同步覆盖 blocks 0/1/33/38/117。descriptor 在 EOF 4096 取得 4096-byte append window；内核把 superblock/group free count 各减一、更新 block bitmap CRC32C 与 descriptor checksum、增长 inode size/i_blocks/extent，并把 node size 扩为 8192、offset 推进到 EOF。新增 logical block 1 经普通 fd read 路径读回全 `G`；第二笔 transaction 释放 block，descriptor truncate 回 4096，五块逐字节恢复。

第七个 marker 证明 VFS create/unlink transaction 同步覆盖 blocks 0/1/36/38/102。全局/group 1 free inode 与 `itable_unused` 由 1→0，inode bitmap CRC、group checksum、inode 32 checksum 和 directory tail checksum 全部重算；正常 path walker 打开 size 0 的 `create-probe`，固定表为它复用读写 fd 3 且 read 返回 EOF。close 后第二笔 transaction 经共享 directory remover 与 inode-bitmap encoder 回到原始五块。最终固定 hash/fsck 排除 inode、目录项或计数泄漏。

两阶段 recovery 证据来自独立 injection/replay 日志。phase 1 marker 明确记录 sequence 1/start 1、五个 targets 0/1/33/38/117、`allocated/grown` 旧状态、`free/original` 新状态与 `after_commit_before_home` 停止点；宿主同时确认 recovery feature、free blocks 61292、bitmap、inode 31 size/blockcount 与全 G data。phase 2 普通 kernel 在任何 ext4 path read 前报告五 tag replay、全部 home readback、next sequence 2、records cleared 和 recovery false，随后执行恢复后的两个 user ELF、cooperative yield、双向 timer preemption、desktop policy commit 与异步 VFS 读写，并用 sequence 2 继续完整 probes/config boot；当前日志为 543 requests/542 queue interrupts，脚本核对其恒差一。宿主把五个 crash home 与注入前快照逐块比较，确认 free blocks 61293、block 117 释放并运行五阶段 fsck；脚本最后恢复固定-hash 标准镜像。

VFS 宿主测试由 `make test-vfs` 执行。5 项测试覆盖 path/mount/fd offset、access mode 与 EOF growth。裸机前两个 `SLOPOS-VFS` marker 证明从固定 root path 读出 inode 23/24 的七块 ELF，init 通过引导副本比对；随后 PID 1 以 fd 3 异步读取 inode 18，PID 2 以自己的 fd 3 分块读取 inode 20/17 并提交 desktop policy，PID 1 另以 O_RDWR/lseek/write/read 对 inode 31 执行可逆 patch。kernel probe 的独立 namespace marker 仍证明 normalized absolute path 经 root mount 解析到 filesystem 1，以 5 个 chunk 读取配置，并在 offset 7 再读取 11 bytes；之后另以读写模式完成 inode 31 的 73-byte write/read/restore。后续 marker 覆盖 append/truncate 与 create/open/close/unlink；`SLOPOS-CONFIG` markers 还证明同一 root ext4 walker 发现四份配置、发布 config generation 1/2、逐次唤醒 PID 2并在非法 CSS 时保留 generation 2。process fd table 已实际连接有界 root ext4 读写；mount、二维 backing-object array 与其他 probe table 仍是 block task 局部的固定容量状态。
