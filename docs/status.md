# Verified subsystem status

状态日期：2026-07-31。状态词严格使用任务规格定义的级别。

| 规格范围 | 状态 | 当前证据与边界 |
|---|---|---|
| 原生图形桌面 | 部分实现 | QEMU 中三个 surface 已进入独立 workspace 的 niri 式 tiled strip + floating layer；浮窗始终盖在 tile 上且不随 viewport 滚动，KDL toggle、显式 floating/tiling move、切换或显式 layer focus、有序 `default-column-width`/`default-window-height`/`default-column-display`/`open-floating`/`default-floating-position`/window `focus-ring`/`border`/`shadow`/`draw-border-with-background`/`opacity`/`open-focused`/`open-maximized`/`open-maximized-to-edges`/`open-fullscreen`、方向 move、标题栏移动与 pointer resize 已实现。规则初始宽高同时作用于 tiled/floating，动态 display 作用于 opening、拆列、单窗跨 workspace 和 floating→tiling；focus ring 只标示 active window，border 为所有普通窗选择 active/inactive 色，shadow 支持 offset/spread/softness/draw-behind 与 RGBA active/inactive 色；非全屏 opacity 对后方 decoration/background 做逐通道 alpha blend，background-mode 可选择实色填充或 surface 外空心边，全屏强制 1.0 并省略 decoration。custom 实机回归证明 `open-focused true` 会让 Config 激活后台 workspace，同时保留 main 的旧 Terminal 局部焦点；Waybar 4/12px margin 与 44px exclusive reserve 让 fullscreen 依次恢复为 x=0/y=44/1024×724 edge maximize、x=16/y=60/992×338 column maximize 与 x=184/656×338 的 app-specific 初始尺寸，并按 window rule 关闭 Config focus ring、启用 4 px 橙色空心 border、软阴影与 0.75 opacity。非固定 center 与三向 expand 又依 GTK packing 平分剩余空间，并在左右 block 宽度变化后完成 workspace 点击，top layer 在 fullscreen 上方保持可见并吞掉非 passthrough surface click；第二次 `slop-overlay` custom mode 启动又以 width=800 把 surface 居中到 x=112..912、以 no-center 移除中栏、把 reserve 归零，并让 surface 内左击穿透 bar 关闭 Terminal。`default-floating-position` 随新 working area 产生右下 24px 浮窗，移动后的 rect 经 tiling 往返仍恢复。tiled 部分另有 normal/tabbed column、preset/explicit width+height、column+edge maximize、single/focused/visible center、available-width expand、固定/方向 consume+expel、focus/reorder、动态 workspace 与 rule。`fullscreen-window` 可让 tiled/floating focused surface 无装饰覆盖完整 output 与兄弟窗，并在退出后恢复原 layer 和精确几何；bottom Waybar 被 fullscreen 覆盖，top/overlay Waybar 仍在其上合成。workspace reorder 又会整体保留名称、两层 layout、fullscreen/focus 与 previous 身份。官方 PageUp/PageDown aliases、150 ms 双包冷却与四组 Mod+IntelliMouse workspace/column wheel bindings 均已在裸机验证。PID 2 `/sbin/slop-shell` 仍作为常驻 provider/policy 服务跨 reload 休眠与恢复；compositor、surface 与 renderer 是 kernel early mechanism。 |
| Rust 实现语言 | 已实现并验证（当前代码范围） | loader、kernel、UI、输入和脚本所对应的 SlopOS 源码均为 Rust；x86 汇编限于 port/interrupt/CR3/CPL transition 边界。 |
| UEFI 引导 | 已实现并验证 | 独立 loader 加载 ELF kernel、Rust userspace ELF 校验副本、ACPI、GOP、bootstrap image、memory map，调用 `ExitBootServices` 并跳入内核；实际 PID 1 bytes 后续来自 ext4 root。 |
| 异步内核 | 部分实现 | 三任务 `Future` executor、task-ready bit queue、RawWaker、PIT timer、PS/2 input 与 virtio block INTx→waker→completion 已在 QEMU 运行；PID 1/2 的同步 `openat/read/write/close` 会保存各自 user frame、转为 Blocked 并返回 block task 异步等待 I/O，completion 再转为 Runnable、恢复对应 CR3/frame，而不是 busy-wait。100 Hz tick 还会在 CPL3 保存完整 interrupt frame并返回 block-task scheduler，kernel future 自身仍为 cooperative。动态 task arena、timer wheel、locks、cancellation、通用 backpressure 和 SMP 尚未实现。 |
| 进程/线程/调度 | 部分实现 | 两个独立 Rust ELF64 从 root `/sbin/slop-init` 与 `/sbin/slop-shell` 分别跨七/九块读入，经严格单 `PT_LOAD` parser 装入独立 CR3；init 另与 UEFI/BootInfo v2 副本比较。kernel 为 PID 1/2 各构造 `argc/argv/envp/auxv`、三页 code/三页 stack、pending syscall/frame 与容量 8 的 fd table。容量 4 的 process table 支持 Ready/Running/Blocked/Runnable/Exited；除 `sched_yield` round-robin 外，100 Hz PIT 已验证双向 user preemption。PID 1 常驻 `wait4`，PID 2 常驻 config event并已从用户态完成 AF_UNIX Wayland configure与两轮 presentation 往返。仍无任意路径/multi-segment exec、thread/kernel preemption、通用 wait/kill/signal、orphan adoption 或 PID reuse。 |
| 内存管理 | 部分实现 | kernel 解析 firmware descriptor stride，建立 frame allocator、自有四级页表并切换 CR3，建立 1 MiB bump heap；PID 1/2 各使用 private PML4、supervisor kernel mapping、三页 user read-only code/三页 writable stack，per-PID bounded copy、跨 code page ELF 装载与跨两个独立 stack physical frame 的 user copy已在 QEMU 验证。PID 2另以 `MAP_SHARED` 在 `0x40006000` 映射一个memfd physical frame；descriptor、mapping与AF_UNIX receiver各有generation-checked引用。reap释放四个table、三个code、三个stack共10 frame，并另释放shared mapping引用。没有NX、kernel section W^X、COW、demand paging、任意地址/多页mmap或munmap。 |
| ext4/btrfs 文件系统 | 部分实现 | QEMU 完成 VFS ELF 多块读取、fd 原位 write、五 tag block allocation/extent growth 和 inode 32/directory create→fd open→unlink transaction；两阶段重启验证 mount-time replay。mutation 仍是固定启动回归流程，btrfs 未实现。 |
| VFS 与文件描述符 | 部分实现 | `no_std` path/mount/fd crate 有 7 项宿主测试；descriptor object 可区分普通file、local socket与shared memory。QEMU中PID 1与PID 2首轮配置各自取得fd 3；PID 2随后让AF_UNIX socket常驻fd 3、memfd短暂占用fd 4并经`SCM_RIGHTS`发送，关闭后配置热重读文件自然复用fd 4。PID 1另以 `O_RDWR` + `lseek/write/read` 对inode 31完成跨两页的可逆64-byte patch并显式close。swww image broker另让同一个block task按命令路径跨两个block读取inode 30的6144-byte PNG。kernel probe fd另可EOF单块append/truncate，并以读写模式打开刚创建的空文件。mount/file backing object仍是单一block task专用，用户copy限于已知三页code/三页stack/单页shared mapping与256 bytes，尚非通用POSIX VFS。 |
| 设备与驱动 | 部分实现 | GOP、COM1、QEMU debugcon、PS/2 键鼠可用，IntelliMouse ID 3/4 协商后可解码四字节滚轮包并回退标准三字节包；校验 XSDT/MADT，自有 GDT/IDT、xAPIC/IOAPIC、100 Hz PIT；PCI/modern virtio-blk 支持 read/write/flush。抢占产生的合法 cache 交错会改变 clean-boot 绝对请求数，测试核对 `requests = interrupts + 1` 且 top-half/queue interrupt 相等。没有通用 descriptor allocator、MSI-X、其他设备类或 application processor。 |
| 图形系统与 Wayland | 部分实现 | framebuffer renderer、niri 式 tiled/floating workspace、Waybar/swww 子集继续可用。`slopos-wayland` 真实 wire/object server 维持分阶段单-client session：PID 2 经 Linux `socket/connect` 连到 `/run/slopos/wayland-0`，标准 request/event wire 通过 AF_UNIX `SOCK_STREAM` 的异步 `write/read` 往返。客户端先解析五个 registry global，再做无 buffer 初始 role commit；服务端发送 shm formats、toplevel geometry 与 xdg configure serial。PID 2 以 `memfd_create/ftruncate/mmap(MAP_SHARED)` 建立 3072-byte XRGB8888 backing，并在首个 configured batch 用 `sendmsg(SCM_RIGHTS)` 传 fd；正确 `ack_configure` 后服务端从同一 shared frame合成。服务端发 buffer release、frame callback done 与 delete-id后，PID 2覆写共享页，以普通 `write` 复用 buffer/callback ID完成第二轮 presentation且不重复传fd。私有像素 staging syscall已删除。仍无任意第三方 client、多 client ownership、用户态 listener、通用mmap/munmap、持续frame/event loop、client-driven xdg fullscreen/maximize、subsurface/popup、独立bar/layer-shell surface或用户态compositor。 |
| 声明式配置 | 部分实现 | niri KDL、Waybar JSONC/CSS 与 swww environment 按 user/system/fallback 顺序从 root VFS 发现；双 static bank 会先验证四份文本，再按 generation 原子发布。交互测试已验证 config generation 1→2 唤醒 PID 2、policy generation 2→3，以及非法 CSS 保留 generation 2且不唤醒服务；custom-config 回归同时注入单页上限内的 4082-byte niri 与 1541-byte Waybar override，验证 ordered niri window rule 的实际焦点、几何、显示、decoration/alpha/fullscreen 例外、三级恢复与浮动位置记忆，并验证 Waybar ordered array 命中 `SLOPOS-1`、`slop-main` name/namespace 与 666-byte name/output class CSS、signed margin/fixed-center/expand-left+center+right/exclusive/top layer、PID 2 有界分块 hash/连续发布、left/center module placement、alternate format 和三键/双向滚轮 action；1269-byte custom `slop-overlay` fixture 另以 PID 2 的 `SLOPOS_WAYBAR_OUTPUT=SLOPOS-1` 展开 `$SLOPOS_WAYBAR_OUTPUT` selector，并验证 width=800 的居中 surface、no-center、mode object 展开、零 reserve 与 surface 内 pointer passthrough；993-byte excluded-output fixture 再以 `["!SLOPOS-1", "*"]` 验证 bar 不实例化与 reserve 归零；1011-byte dimensions fixture 以 `["width > 2000", "height > 700"]` 验证实际 1024×768 条件拒绝、bar 不实例化与 reserve 归零；1012-byte hide fixture 最后以 `modifier-reset: release` 验证 binding 保护与 modifier-only 隐藏。仍无文件 watcher/inotify、普通配置编辑器、结构化 diff，配置发现/parser 也仍属于 kernel service。 |
| 文本编辑器 | 尚未实现 | kernel monitor 只编辑当前命令行，不是普通文本或配置文件编辑器。 |
| `slopd` | 尚未实现 | 当前 `/sbin/slop-init` 只是固定 ABI probe，没有 unit、dependency graph、service manager 或 supervision。 |
| eBPF | 部分实现 | 独立 `no_std` crate 提供 8-byte instruction decode、无分配 verifier、ALU64/前向 branch/512-byte stack/helper 子集解释器；10 项宿主测试与内核返回 42 均已验证。没有 ELF loader、map、program type、attach point、权限模型或 JIT。 |
| AMD SVM VMM | 尚未实现 | 尚未进行 SVM capability detection 或 `VMRUN`。 |
| Linux x86-64 ABI | 部分实现 | root VFS 中两个独立 Rust ELF64 `ET_EXEC` CPL3进程使用Linux x86-64 initial stack/register/syscall number，真实解析各自 `argc/argv/envp/auxv` 并执行 `openat/read/write/lseek/close/sched_yield/wait4/socket/connect/sendmsg/memfd_create/ftruncate/mmap`；同步fast return使用 `SYSRETQ`，异步file/socket I/O与timer saved frame经 `IRETQ` 恢复完整GPR。PID 1当前停在blocked wait4，PID 2由SlopOS私有policy/config event ABI跨reload恢复；exit/reap与immediate child lookup由6项process宿主测试继续覆盖。10项ELF测试和QEMU实测还覆盖syscall/interrupt两种frame的CR3 switch、file/socket/shared descriptor isolation、跨两页copy-user和单页shared mapping。尚无任意路径/多segment exec、通用页表/demand-page copy、广泛syscall surface、proxy或guest agent。 |
| 网络与 IPC | 部分实现 | `no_std` local-stream core支持固定容量具名bind/listen/connect/accept、全双工receive ring、backlog/backpressure、readiness、close/EOF、generation handle，以及与一批bytes原子关联的单个rights object；5项宿主测试已执行。kernel将它接入通用descriptor object、Linux `socket/connect/read/write/sendmsg/close` 与block-task suspend/resume，QEMU已由PID 2连接 `/run/slopos/wayland-0`，双向传输全部Wayland wire并以 `SCM_RIGHTS` 传递真实memfd shared backing。用户态 `bind/listen/accept`、poll/epoll、网络协议、pipe、通用ancillary data/credentials、通用shared memory、event object与message queue仍未实现。 |
| 许可证 | 已实现并验证（当前代码范围） | 原创源码为 0BSD；锁定的三个第三方 crate 均为 MIT OR Apache-2.0。 |
| 可重复构建与证据 | 已实现并验证（当前里程碑） | 工具链、镜像脚本、QEMU 命令、串口日志、debugcon、截图及交互测试均已保留。 |

`default-floating-position` 的当前验证边界是八种 working-area anchor、右/下坐标方向、单边居中、首次浮动定位，以及同一桌面会话内的最后 rect 恢复；custom QEMU 证据为 x=8/y=406/992×338 的右下 24px 锚点和移动后 x=8/y=430 的 tiling 往返恢复。位置不跨配置重建或重启持久化。

## 当前可运行状态

- Waybar hide mode 已通过第五次 custom QEMU 冷启动验证 `modifier-reset: release`：`Super+2` 因 binding action 保持 bar，modifier-only chord 在释放时隐藏；串口 reset marker 与 `161a2a→111144` 截图像素一致。
- 最后成功构建：2026-07-31，`make image`。
- 最后成功启动：2026-07-31，QEMU 10.0.11 + OVMF 2025.02，`make test-boot`。
- 最后成功交互测试：2026-07-31，`make test-interaction`。
- 最后成功自定义桌面配置测试：2026-07-31，`make test-desktop-custom-config`。
- 最后成功桌面截图：2026-07-31，`make image && ./scripts/capture-desktop.sh`；PNG 与四个 PID 2 第二帧像素均通过验证。
- 最后成功异常测试：2026-07-31，`make test-page-fault`。
- 最后成功 journal recovery 测试：2026-07-31，`make test-journal-replay`。
- 最后成功 eBPF 单元测试：2026-07-30，`make test-ebpf`，10 项。
- 最后成功 ELF 单元测试：2026-07-30，`make test-elf`，10 项。
- 最后成功 process 单元测试：2026-07-30，`make test-process`，6 项。
- 最后成功 shell/protocol 单元测试：2026-07-31，`make test-shell`，67 项（shell 56 + desktop protocol 11）。
- 最后成功 Wayland 单元测试：2026-07-31，`make test-wayland`，13 项。
- 最后成功 ACPI 单元测试：2026-07-30，`make test-acpi`，3 项。
- 最后成功 PCI 单元测试：2026-07-30，`make test-pci`，3 项。
- 最后成功 virtio 单元测试：2026-07-30，`make test-virtio`，4 项。
- 最后成功 ext4 单元测试：2026-07-30，`make test-ext4`，28 项。
- 最后成功 VFS 单元测试：2026-07-31，`make test-vfs`，6 项。
- 最后成功 IPC 单元测试：2026-07-31，`make test-ipc`，4 项。
- 已验证的 kernel entry：`0x04000000`。
- 已验证 GOP mode：1024×768，stride 1024。
- 当前 bootstrap image：186 bytes，临时 SlopOS 文本格式。

## 下一项最高价值工作

下一阶段为 AF_UNIX 增加用户态 `bind/listen/accept`、通用 ancillary data/credentials/poll，并把单 session扩成多 client object ownership；现有单页 memfd mapping也需扩成通用 mmap/munmap与多页生命周期。通用进程主线仍需任意路径/多 `PT_LOAD` exec、通用 wait/signal、独立线程/kernel stack 与 SMP run queue。
