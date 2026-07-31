# Architecture

本文只描述当前实际存在的代码。规划中的子系统在 [status.md](status.md) 和 [async-kernel.md](async-kernel.md) 中单独标记。

## 启动链

```text
OVMF
  -> EFI/BOOT/BOOTX64.EFI (SlopOS Rust loader)
      -> FAT SimpleFileSystem: /slopos/kernel.elf
      -> FAT SimpleFileSystem: /slopos/initrd.slp
      -> FAT SimpleFileSystem: /slopos/init.elf
      -> ACPI configuration table
      -> GOP framebuffer
      -> final UEFI memory map
      -> ExitBootServices
  -> ELF entry at physical 0x04000000 (SlopOS Rust kernel)
      -> validate RSDP/XSDT/MADT and discover interrupt controllers
      -> initialize COM1
      -> establish frame allocator, page tables, heap, and eBPF self-test
      -> accept GOP framebuffer ownership
      -> initialize PS/2 keyboard and mouse
      -> initialize GDT/IDT/TSS and APIC interrupt routing
      -> mount ext4 root and load /sbin/slop-init
      -> enter PID 1 through a private CR3 at CPL3
      -> suspend openat/read/write/close onto the async block task
      -> restore user CR3/frame after virtio completion
      -> retain PID 1 supervisor and PID 2 desktop-service waits
      -> resume PID 2 across desktop config generations
      -> interactive early desktop loop
```

加载器不调用 GRUB、Linux 或其他操作系统。它使用 `uefi-raw` 的 ABI 类型，自己实现协议发现、UTF-16 路径、FAT 文件读取、GOP 模式选择、页分配、ELF64 校验与装载、memory map 取得和 `ExitBootServices` 重试。

内核是固定地址 `ET_EXEC` ELF64。链接脚本把它放在 64 MiB；加载器要求每个 `PT_LOAD` 的 virtual address 等于 physical address，先为整个映像分配连续的 `LOADER_CODE` 页，再清零并复制各 segment。入口必须落在已分配映像内。

## BootInfo ABI

`crates/boot-protocol` 是加载器和内核共享的 `no_std` crate。所有结构使用 `#[repr(C)]`，并通过 magic、版本号和结构大小进行校验。当前传递：

- GOP base、size、resolution、stride、pixel format；
- memory map base、总字节数、descriptor size/version/count；
- ACPI RSDP 地址；
- bootstrap image 地址和大小；
- 独立 userspace ELF 地址和大小；
- 内核物理范围和入口。

memory map 使用 firmware 返回的 descriptor size，而不是假设 Rust 结构大小。加载器在最后一次所有分配完成后取得 map，并在 `ExitBootServices` 失败时进行一次不分配内存的重试。

## 当前图形与输入

PS/2 controller 不再吞掉 Super/Ctrl/Shift/Alt 的状态变更，而是把带完整 modifier bitmap 的按下/释放边沿交给 desktop；普通按键仍只在 key-down 产生事件。Waybar hide mode 用 Super 边沿实现 `modifier-reset: press|release`：基础 mode/signal 可见性与临时 modifier 可见性分开保存，已接受的 niri binding 会清除 release 策略的 `no-action` 标志，非 hide mode 则保持 no-op。这复现了 Waybar Sway bar client 的可观察 reset 语义，但没有伪装成 Sway IPC。

`kernel/src/framebuffer.rs` 直接使用 volatile 32-bit framebuffer store，尊重 GOP stride 和 RGB/BGR 格式。`font.rs` 是项目内原创的 5×7 bitmap glyph 集。

`default-floating-position` 解析并有序覆盖 x/y 与八种 `relative-to` working-area 锚点；浮窗首次创建时解析为有界 rect，floating→tiling 会把最后 rect 存入固定容量记忆表，下一次 tiling→floating 优先恢复它。关闭窗口会清除该项，配置重建和系统重启则不会保留。

`crates/shell` 是无分配、无标准库的 niri/Waybar/swww 状态机与配置 parser。每个 workspace 保存独立的 persistent/anonymous identity、horizontal column strip、floating layer、active/previous focus、可选 fullscreen window 和有界动态尾部空位。`move-workspace-up/down` 原子交换 identity、两层 layout、fullscreen 与焦点状态，active 随当前 workspace 移动，previous 则继续指向原来的逻辑 workspace；空的尾部 anonymous workspace 不参与无意义交换。tiled layer 支持 edge reveal/manual scroll、首末/相邻列 focus+reorder、single/visible/focused center、normal/tabbed stack、固定与方向感知 consume/expel、上下 focus/reorder、gap-aware explicit/preset width/height、column/edge maximize、available-width expand 和 close；floating layer 始终位于 tile 上方且不滚动，保存 z-order/rect/default geometry，支持 layer focus、方向 focus/move、显式/preset resize、center/expand、pointer move/resize 和跨 workspace transfer。`fullscreen-window` 让任一层的 focused window 独占完整 output；底层的 tiled column 或 floating rect 保持不变，关闭全屏后精确恢复原 layer、尺寸与位置。跨 workspace 状态机明确区分完整 column 与 focused window：前者保留 stack/focus/width/display，后者以原列宽抽成目标 singleton，两者都先检查目标容量再修改。`toggle-window-floating` 从 tile 当前屏幕位置推导浮窗 rect，送回 tiling 时按当前 focused column 邻近插入；`move-window-to-floating`/`move-window-to-tiling` 和 `focus-floating`/`focus-tiling` 提供幂等的显式目标语义。KDL 以最多 80 项 keyboard/`WheelScrollUp|Down` bind、逐 binding `cooldown-ms`、named workspace、全局 `default-column-display`、`open-on-workspace`、`open-floating true|false`、`open-focused true|false`、八向 `default-floating-position`、逐属性 window `focus-ring`/`border`/`shadow`、`draw-border-with-background true|false`、动态 `opacity`、tiled/floating 共用的 app-specific `default-column-width`/`default-window-height { fixed|proportion; }`、动态 app-specific `default-column-display "normal"|"tabbed"`、`open-maximized true|false`、`open-maximized-to-edges true|false`、`open-fullscreen true|false` 和有序逐属性 rule override 驱动这些行为；规则 opacity 以千分之一存储并在渲染时 clamp 到 0..1，非全屏 surface 对后方 decoration/background 做逐通道 alpha blend，fullscreen 强制 1.0；focus ring 只围绕 active window，border 为所有普通窗选择 active/inactive 固色，shadow 则保存 on/off、offset、softness、spread、draw-behind-window 与 RGBA active/inactive color，inactive 未指定时把 active alpha 乘 0.75。early compositor 先用有界 CPU 栅格合成带二次 alpha falloff 的 shadow；background-mode 为 true 时再以 outer focus fill→border fill→surface 合成，为 false 时 focus ring/border 改画 surface 外侧的空心四边，border 启用时 focus ring 总在 border 外。未匹配 background-mode rule 的无 SSD surface 默认使用 true；fullscreen 省略全部 decoration。规则 display 会在初次打开、从 stack 拆列、单窗跨 workspace 与 floating→tiling 时决定新 singleton column。`open-focused false` 保留目标已有 tiled/floating 局部焦点，`true` 则聚焦新窗并激活其目标 workspace；配置重建会先恢复其他 workspace 的旧局部焦点。初始 column maximize 只作用于 tiled column，保持 Waybar 与 layout gap，取消后恢复规则列宽且不改变规则高度；初始 edge maximize 强制进入 scrolling layer，并让单窗占满 Waybar 下方工作区、去除 layout gap/border；初始 fullscreen 则让指定 tiled/floating window 覆盖完整 output 与兄弟窗，bottom Waybar 在它之前合成所以被覆盖，top/overlay Waybar 在它之后合成所以仍可见。三种规则同时为 true 时保持 `fullscreen > edge maximize > column maximize > rule geometry` 的可逆可见优先级。proportion 使用 `(working_size - gap) × proportion - gap`，当前 workspace 容量为 4。Waybar top-level 保存最多八项、每项 96-byte 的 ordered output selector，按当前 `SLOPOS-1` name/identifier 选择是否实例化并把 output name 作为 CSS class；以 `$` 开头的 selector 会按 PID 2 的实际环境查找整段变量名，`!$VAR` 也遵循相同 ordered 排除语义，未知或不匹配值再回退 literal 比较。当 `output` 未配置或为空 string 时，最多八项 `output-dimensions` 以严格的 `width|height <|> i32` 条件对实际输出尺寸做 AND 过滤，无效 string 与非 string 数组项按上游行为忽略。另保存 32-byte name/CSS class namespace、fixed width、三块 expand packing、no-center、signed 1/2/3/4-value margin、逐边优先级、fixed-center、exclusive、bottom/top/overlay layer、四个 preset、最多八个 custom mode、`start_hidden`、passthrough、visible effective state、`on-sigusr1`/`on-sigusr2` action 与可恢复 visibility state；统一 origin helper 驱动 bar/module raster 和 hit-test，exclusive top reserve 同时进入 tiled/floating layout。renderer 按 layer 在窗口前后选择合成顺序，pointer 使用相同 z-order并在 passthrough 时跳过 bar；preset/custom mode 都以官方组合方式覆盖 layer/exclusive/passthrough/visible，default 再叠加显式顶层字段，未知名回退 default。module normal/alternate format、format-alt-click、interval/length、三键与双向滚轮 action option、固定 96-byte text/action 与最多 32 条 CSS cascade rule 均不分配。swww 状态机保存 image/clear 生命周期、借用字节的 P3/P6 parser、统一 `RasterImage` 像素视图、resize/filter，以及 output-space wipe angle、grow/outer position 与 invert-y transition option；CLI 和 environment 使用同一解析结构。PNG decoder 在调用者提供的可变输入与 RGB 输出 bank 内就地聚合连续 IDAT，校验 CRC/Adler，解 stored/fixed/dynamic DEFLATE 与 filter 0–4，用固定 256-entry table 展开 indexed palette/`tRNS`，按原始 sample 深度展开 packed 1/2/4-bit grayscale，并将 16-bit gray/GA/RGB/RGBA 线性缩放为 RGB8；Adam7 路径在独立固定 scratch 中逐 pass unfilter 后 scatter 到最终 RGB，不使用堆分配。56 项 shell 宿主测试覆盖上述 parse/reject/layout/layer/workspace/rule/format/style、daemon、PNM、PNG、filter 与 transition。

swww 落屏层另保存 crop/fit/no/stretch、九向 crop gravity 与 fill color。kernel 用有理数 source-pixel 边界产生 destination rectangles，所以 fit/crop 保持比例且不会被整数倍缩放限制，stretch 则精确覆盖 output；当前固定使用 nearest-style rectangles。

`crates/desktop-protocol` 定义 40-byte `DesktopCommit`、32-byte `DesktopServiceEvent`，以及最大 3872-byte 的 `WaylandSurfaceCommit` bootstrap 信封；8 项测试覆盖版本、长度、capability、FD token、对齐和 reserved drift。`userspace/desktop` 仍以固定 256-byte buffer读取配置并提交 policy；首代 apply 后，它再编码 332-byte 标准 Wayland request stream与 3072-byte XRGB8888 snapshot。`kernel/wayland_service.rs` 只接受 PID 2，以 `slopos-wayland::SingleSurfaceSession` 分派 registry/compositor/shm/xdg-shell 对象并严格要求 attach、full damage、frame callback、commit完整 lifecycle，再用双 bank发布给 desktop。信封中的 `0x534c` 是当前内联 backing 的私有 token，不冒充 OS file descriptor；transport 也明确不是 Unix socket/`SCM_RIGHTS`。QEMU marker 证明 wire→server→generation→renderer acknowledge，`capture-desktop.sh` 又对用户态四象限图案取样。配置 hash/generation、custom Waybar 与 reload 行为保持原有验证。

swww duration parser 把 CLI/environment 的小数秒无浮点地截断为毫秒。同步 renderer 对非 simple transition 以 `ceil(duration_ms × fps / 1000)` 取得采样区间并限制到 1–16；simple 则沿用 `transition-step`，同样限制最坏帧数。四个 Bezier control component 以万分位有符号定点数存储；x1/x2 限制在 0..1 后可用整数二分反解，y 可超出区间并在最终进度处 clamp。fade 用 eased progress 混色，方向/wipe/grow/center/outer 用同一 progress 驱动几何 mask，wave 则在 wipe 的 output-space 投影线上按切向坐标查 16 段插值正弦表并叠加定点宽高，simple 按官方行为绕过 easing。duration 只驱动确定性采样密度，尚未接入 timer wheel 或 wall-clock frame pacing。

filter parser 接受官方五个名称。Nearest 继续以有理数 source-pixel rectangle 直接填充；其他名称通过最多 2,048 像素的栈上 decode bank，把 4×4 output block 中心以 16.16 坐标反投影到 source，再进入同一 transition mask。Bilinear 使用四点插值；CatmullRom 与 Mitchell 使用各自带边缘 tap clamp 和归一化的定点 4×4 cubic convolution；Lanczos3 以同样的 separable pipeline 执行 6×6 windowed-sinc convolution，并用 32 段 Q16 正弦表避免 kernel 浮点与堆分配。默认仍保持 Nearest，而不是 swww 的 Lanczos3；这是明确记录的 early-compositor 性能取舍。

`desktop.rs` 当前仍是内核态 early compositor。它先按 Waybar effective layer 选择是否合成 bottom bar，再按 tile 几何渲染 tiled surface、按 z-order 合成 floating surface 或单独合成 fullscreen focused surface，最后才合成 top/overlay bar；invisible bar 完全跳过；show/hide/toggle 会在 saved configured mode 与可覆盖的 invisible mode 间切换，并同步重算所有 workspace 的 exclusive reserve。pointer hit-test 反向遍历浮窗，因此重叠时总选最上层；bar 使用同一 layer 顺序决定是否优先于窗口，并在 passthrough 时跳过整个 input region。全屏仍省略 compositor decoration 和所有兄弟窗，但只覆盖 bottom bar，top/overlay bar 保留合成与输入。`Mod+V`/`Mod+Shift+V` 分别切换窗口所在层与层焦点，`Mod+Alt+V`/`Mod+Ctrl+V` 显式移入 floating/tiling，`Mod+Alt+G`/`Mod+Alt+T` 显式聚焦 floating/tiling；方向 move bind 对浮窗使用 50 px step，floating 标题栏拖动改变 rect而不是滚动 strip，Super+右键同时调整浮窗宽高，tiled 手势保持原语义。`Mod+W` tabbed display、`Mod+Shift+F` full-output toggle、官方 PageUp/PageDown workspace focus/column transfer/workspace reorder aliases、四组 Mod+IntelliMouse workspace/column focus+move+reorder、column/window/workspace 全套 bind、ordered open/size/display/floating-position/maximize/fullscreen rule、Waybar workspace/action hit-test 与 swww renderer 共享同一 active-window 状态。带修饰键的全局 niri wheel bind 优先；命中但处于 cooldown 的滚轮仍由 compositor 消费，未匹配的普通滚轮才进入 Waybar module action。kernel 以 100 Hz PIT tick 保存每个 bind 的独立 deadline，配置 generation swap 会清空旧 deadline。active workspace name 通过可移动 identity 解析，因此重排期间 named action 与 Waybar name 不会退化为旧物理索引。构造阶段使用 `assets/` bootstrap；ext4 mount 后由 `fs.rs` 按 user/system/fallback 候选加载四份配置，`desktop_config.rs` 通过双 bank generation parse-before-swap，reload 会尽量保留当前 floating/tiled layer并对重建窗口应用初始规则。`wallpaper_file.rs` 以每槽 8 KiB 压缩输入加 24 KiB decoded/Adam7 scratch 的双 bank 连接 desktop/block task：输入侧发布 path/transition generation，block task 通过 ext4 walker 异步读齐，随后验证 P3/P6 或在 inactive bank 解码 PNG，再把统一 raster 发布给 desktop；desktop 完成动画后才 acknowledge。失败 bank 不替换当前 pinned image，因此 missing/invalid path 保留旧壁纸。当前仍没有文件 watcher、Wayland object/surface IPC、普通用户 client、超过 4 个 workspace、浮窗位置跨配置重建/重启持久化或完整 niri rule/action/output/animation；Waybar provider/Pango/完整 GTK CSS 与真正的 swww socket/process/layer-shell 也未实现。兼容边界见 [desktop-shell.md](desktop-shell.md)。

PID 2 首次收到 `policy-applied` 后还会通过 `0x534c0003` 提交 332-byte 标准 Wayland request batch 和 3072-byte XRGB8888 snapshot；`slopos-wayland` dispatcher 校验 registry/compositor/shm/xdg-shell 对象关系、buffer geometry、full damage、frame callback 与 atomic commit，`wayland_service.rs` 以双 bank 发布给 desktop task，并在 System 窗口合成后 acknowledge。上段所称缺少 Wayland object/surface IPC 是指仍无可连接的 Unix socket、`SCM_RIGHTS`/共享 mapping、server→client configure 往返、buffer release 或普通第三方 client，而不是否认这条受限 bootstrap。

`memory.rs` 按 firmware 报告的 descriptor stride 解析 UEFI map，只收集 conventional memory，并提供并发保护的物理 frame/contiguous bump allocator。启动时实际分配一个 frame、volatile 写入、读回并清零。单页分配另先消费容量 256 的 LIFO recycled stack；释放路径校验对齐、来源、重复与容量，用户进程 reap 后的 page-table/code/stack frames 会进入该栈。

`paging.rs` 从 frame allocator 建立新的 x86-64 PML4/PDPT/PD，以 2 MiB page identity-map 当前 RAM，并以 cache-disabled 映射覆盖 MMIO。PID 1 与 PID 2 各有自己的 PML4；原 kernel huge-page entry 不带 U/S。每个进程的两个 code page 为 user read-only，两页 stack 为 user writable且 physical frame互相独立；ELF loader按页复制最多 8192 bytes，user-copy 对 code/stack 也逐页翻译。reap 会释放四个私有 table、两个 code、两个 stack，共 8 frame。当前还未启用 NX 或 kernel section W^X。

`crates/acpi` 校验 ACPI 1.0/2.0 RSDP、RSDT/XSDT 和 SDT checksum，并解析 MADT 的 local APIC、I/O APIC、processor、local APIC override 与 interrupt-source override。`apic.rs` 通过 MADT 路由把 PIT、keyboard、mouse 送入 IOAPIC，屏蔽 8259，启用 xAPIC 并从 local APIC 发 EOI；QEMU 的 IRQ0 实际按 override 路由到 GSI 2。

`interrupts.rs` 安装包含 kernel/user code/data 与 64-bit TSS 的 GDT、加载 task register、安装 IDT、配置 100 Hz PIT，并为 timer、keyboard、mouse、APIC spurious 及关键 CPU exception 安装 gate。TSS 的 `RSP0` 指向专用 16 KiB privilege stack，供 CPL3 hardware interrupt/exception 切栈；IDT 不再暴露 DPL3 vector `0x80`。设备 IRQ stub 保存 caller-clobbered context并调用有界 top half；timer stub 另保存全部 15 个 GPR。CPL3 tick 校验 hardware RIP/CS/RFLAGS/RSP/SS frame，在存在另一可运行进程时把它转换为 per-PID saved frame、发送 EOI，并舍弃 TSS privilege stack跳到 block-task continuation；否则原样 `IRETQ`。`process.rs` 还检查 CPUID SYSCALL bit，配置并读回 `EFER.SCE`、`STAR`、`LSTAR`、`FMASK`；fast entry 自行从 user RSP 切到暂停中的 kernel continuation stack，保存 15 个 GPR 与 RCX/R11/RSP。stdout write 与无需 I/O 的 `lseek(SEEK_SET)` 可直接 `SYSRETQ`；`openat/read/write/close` 会复制 frame、返回 kernel CR3/stack，让 block Future 等待 virtio IRQ，完成后经共享 resume entry 恢复 process CR3、全部 GPR、RIP/RFLAGS/RSP 并 `IRETQ`。timer resume 使用同一路径，因而不会丢失异步被抢占代码的 RCX/R11。PS/2 top half 读取一个字节、确认 local APIC 并写入固定 SPSC ring；`desktop` future 负责扫描码及三/四字节 mouse packet 的复杂解析。独立测试会在 PID 1/2 都保持 Blocked、block task 已恢复 kernel CR3 时访问未映射的 1 GiB 地址，实际验证 page-fault vector、error、RIP 和 CR2。

`crates/elf` 是无分配 `no_std` ELF64 parser；`crates/process` 提供固定容量 process/fd 生命周期。`userspace/init` 与 `userspace/desktop` 都是固定在 `0x40000000` 的独立 Rust executable；release artifacts 分别为 26344/33016 bytes，R+X `PT_LOAD` 为 2608/6456 bytes。rootfs builder安装为 inode 23/24；block task分别跨七/九块读入，只有 init 与 ESP 副本逐字节比较。kernel 为每个进程构造两页 code、两页 stack、Linux initial stack 与独立 CR3。PID 1 最终常驻 `wait4`，PID 2 常驻 config event并保留其已提交的 Wayland surface。完整边界见 [processes.md](processes.md)。

`crates/pci` 通过 `ConfigAccess` trait 把枚举逻辑与硬件访问分离，扫描完整 bus/device/function 空间，识别 multifunction header，以 visited mask 避免 capability 链环，并解码 BAR 与 virtio vendor capability region。内核后端使用 PCI configuration mechanism 1 的 `0xcf8/0xcfc` port，并以 16-bit command write 启用 memory space/bus master 而不误清 status。

`virtio.rs` 走 modern PCI transport，协商 `VIRTIO_F_VERSION_1` 和设备提供的 `VIRTIO_BLK_F_FLUSH`，拒绝 read-only block device，为 queue 0 分配独立 descriptor/available/used frame，并建立两个各有 control/data frame 的请求槽。每个槽保留三个 descriptor；read data 标为 device-writable，write data 保持 device-readable，flush 只链接 header/status。单请求或双请求批次都只在 descriptor 与 available entries 完成后一次发布 index。INTx top half 只读取并清除 ISR、累计计数、wake block task 和 EOI；Future 在下半部等待目标 used index 并检查各槽 status。共享 `crates/virtio` 负责可宿主测试的 split-ring layout 与可偏移 descriptor 构造。

构建产生两个 disk：64 MiB FAT32 ESP 只供 UEFI loader，256 MiB `SLOPOS_ROOT` ext4 image 作为独立 root disk。`fs.rs` 持有 `Ext4Mount`/`Ext4File` 和 8-entry FIFO cache；inode 31 用于 fd read-modify-write。隐藏 inode 8 的单一 extent 指向 4096-block journal；内核以 scratch frames 保留 ext4/JBD2 superblock 并编码 logical blocks 1–3。transaction engine 依次持久化 recovery bit、`start=1`、descriptor/data、commit，在各 durability boundary flush 并直接读回 replayable 状态；随后写 home block、推进 sequence/清 start、清 recovery，再清测试 records。除 data-block probe 外，engine 还对 inode-table block 38 执行 size 4096→4095→4096 的两笔 transaction，每次都重算并解析 inode checksum。

mount 首次解析若只因 `needs_recovery` 被拒绝，会进入受限 recovery parser：通过 journal inode 定位 active `s_start`，读取连续 descriptor/data/commit，联合校验 sequence、UUID、各 tag flags 与 target range，恢复 escaped data 后依次写 home 并 flush；随后清 records、推进 sequence/清 start并 flush，最后清 ext4 recovery bit/checksum。两阶段 feature kernel 先持久化已分配/已增长的 blocks 0/1/33/38/119，再发布目标为原始 free 状态的五 tag transaction，并在 commit 后/home 前停止；普通 kernel 重启同时重放五个 home，再从恢复后的 root VFS 读取 PID 1，随后继续正常桌面启动。当前 scanner 不支持多 transaction、revoke 或 ring wrap。

固定容量 multi-block engine 最多接受八个 home block；descriptor 的首 tag 携 UUID，后续 tag 使用 `SAME_UUID`，每个 tag 独立处理 escape。fd 3 在 EOF 请求 append 后，同一笔五 tag transaction 更新 block 0 superblock free count/checksum、block 1 group descriptor、block 33 bitmap、block 38 inode 31 与 block 119 data；home superblock 在 checkpoint 阶段保持 recovery bit，全部 home flush 后才清 recovery。inode 的 size/i_blocks/inline extent 从 4096/8 sectors/1 block 增至 8192/16/2，descriptor size/offset 随之更新，新增块经普通 fd read 回读；第二笔 truncate transaction 再释放回原始状态。replay scanner 已能解析最多八 tag，但仍只扫描一笔非 wrap transaction。

VFS create probe 复用相同 engine，把 blocks 0/1/36/38/104 作为一个原子集合：更新全局/group 1 free-inode count 与 `itable_unused`，重算 inode bitmap/group checksum，初始化带 extent header/checksum 的 inode 32，并在 inode 27 的线性目录块中拆分 slack、插入带 directory-tail checksum 的 `create-probe`。checkpoint 后正常 path walker 打开 size 0 文件，固定 descriptor table 为它分配读写 fd 3 并验证 EOF；close 后第二笔 unlink transaction 合并目录 record、释放 inode bit并逐字节恢复五块。

`crates/vfs` 是无分配、无标准库的 namespace 状态机：绝对路径最多 16 个 component，mount table 采用最长 component-prefix，fd table 从 3 开始分配并维护 vnode、size、offset 与 read/write access mode，并可在 owner exit 后执行有界 `close_all`。内核把 ext4 注册为 filesystem 1 并挂到 `/`；启动验证先经 path walker 跨七块读取 inode 23 的 `/sbin/slop-init`。PID 1 随后用 Linux `openat/read/close` 异步读出 inode 18 的 76 bytes，再用 `O_RDWR openat`、lseek、异步 write/read 对 inode 31 的 offset 123 执行跨两页的可逆 64-byte patch并显式 close。PID 2 在每代 config event 后重开并关闭 Waybar/swww 文件。kernel probe 的独立 block-task table 还会以五个 chunk/seek 读取 inode 18，并对 inode 31 写入/恢复 73 bytes；桌面配置候选与 swww path image loader 也复用同一 ext4 walker，后者已实测跨 inode 30 的两个 block 读齐并解码 6144-byte PNG。进程 fd 已接入有界读写 syscall，但 mount table、`Ext4File` backing slots 与其他 probe fd 仍由单一 block task 专用，不是并发全局 POSIX VFS。

`executor.rs` 当前固定运行 input、timer、block 三个 pinned future，以原子 ready mask 作为 task queue，以 RawWaker 标识 task，并在空闲时执行 race-free `cli` 检查和 `sti; hlt`。它仍缺动态 task arena、timer wheel、cancellation、async lock 和 SMP。

`ebpf` 是与内核分离的 `no_std` crate。它把标准 little-endian 8-byte instruction 解码成固定布局，以前向数据流交集跟踪已初始化寄存器，拒绝 backward jump、越界分支、对 frame pointer 的写入、越界 stack access、未知 helper 和没有可达 `EXIT` 的路径。解释器拥有 11 个 64-bit 寄存器和 512-byte stack；启动路径验证并执行一段 ALU/stack 程序，要求结果为 42。具体指令和未实现边界见 [ebpf.md](ebpf.md)。

## 汇编用途与安全边界

没有独立 `.S` 文件；Rust 中的 `asm!`/`global_asm!` 限于：

- `in` / `out`：COM1、QEMU debugcon 和 i8042 PS/2 port；
- `cli` / `sti`：中断初始化与 race-free idle；
- `pause`：当前早期轮询和 fatal loop 的处理器提示；
- `hlt`：panic 后停止处理器或等待下一个 IRQ；
- `lgdt` / `lidt` / `ltr`、segment reload 与 interrupt/exception entry；
- `mov cr3` 和 `IRETQ`：切换 PID 1 地址空间与 CPL；
- `SYSCALL`/`SYSRETQ` fast entry/同步 exit，以及 suspended syscall/timer frame 的 `IRETQ` resume：完整保存 GPR、调用 Rust handler、返回 CPL3 或恢复 kernel continuation。

调用约定：

- UEFI 入口和 firmware function pointer 使用 `extern "efiapi"`；
- loader 到 kernel 使用 `extern "sysv64"`；
- `BootInfo` 指针放在 SysV 第一个整数参数寄存器。
- 汇编到 Rust interrupt/syscall handler 使用 SysV64；stub 在调用前保证栈对齐。

每个 I/O port wrapper 都是局部 `unsafe`，调用点说明目标平台假设。framebuffer 和 BootInfo 的 raw pointer 在转为引用或写入前均检查范围或依赖加载器独占分配不变量。用户指针当前不是通用接口：handler 对地址加法做 overflow check，先验证整段位于已知 code/two-page stack mappings，再在 kernel CR3 下逐页翻译保存的 physical frame并执行有界复制；它不假定相邻 virtual page 物理连续。路径上限 128 bytes，单次 read/write 上限 256 bytes。PID 1/2 通过静态 per-PID 保存点与 block-task syscall broker 恢复 kernel/user CR3 和完整 frame；当前可交错运行两个固定进程并跨事件恢复 PID 2，但尚不能支持嵌套用户入口、动态并发进程或内核态抢占。
