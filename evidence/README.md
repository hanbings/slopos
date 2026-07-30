# Runtime evidence

所有证据均可从源码重新生成，不提交 ESP、ELF、OVMF VARS 或 PPM 等大型生成物。

| 文件 | 生成方式 | 证明范围 |
|---|---|---|
| `serial.log` | `make test-boot` | OVMF/UEFI、ELF、`ExitBootServices`、XSDT/MADT、memory、两个常驻 CPL3 进程、cooperative/timer switch、policy/config event、eBPF、PCI/virtio INTx、ext4、async 与桌面循环 |
| `uefi-debugcon.log` | `make test-boot` | loader 独立 debugcon 日志 |
| `interaction-serial.log` | `make test-interaction` | PS/2 键盘触发 VFS 配置 reload/rollback、执行 `STATUS`/swww image/clear/query，鼠标横拖 viewport、Super+右键缩放、点击 Waybar workspace，Super bind 合并/显式与 preset 窗高/reset/同列重排与聚焦/拆列、gap-aware 显式/preset/maximized/centered/available-width 列操作/重排/按索引切换与移列/关闭窗口 |
| `custom-config-serial.log` | `make test-desktop-custom-config` | 不同长度、left/center placement 与 clock 三键/双向滚轮 action 的合法 Waybar override 经 PID 2 分块 hash、两代 policy、config apply、中央 workspace 点击与受限 action 生效 |
| `custom-config-uefi-debugcon.log` / `custom-config-qemu.log` | `make test-desktop-custom-config` | 自定义配置回归的 loader 与 QEMU 输出 |
| `page-fault-serial.log` | `make test-page-fault` | 自有页表的未映射访问、vector 14、error、RIP、CR2 与 fatal boundary |
| `journal-injection-serial.log` | `make test-journal-replay` phase 1 | commit 已 flush、home 尚未 checkpoint 的 dirty disk |
| `journal-replay-serial.log` | `make test-journal-replay` phase 2 | 普通 kernel mount-time replay、清理、继续完整启动 |
| `desktop.png` | `scripts/capture-desktop.sh` | niri 式 column strip 与 Waybar 式顶部三区域 |
| `terminal-status.png` | `make test-interaction` | 图形终端对键盘命令的实际响应 |
| `window-moved.png` | `make test-interaction` | titlebar drag 后 terminal 离屏、后续 column 进入 viewport |
| `window-resized.png` | `make test-interaction` | `Mod+Equal` 把 focused terminal column 从 488 px 放大至 588 px |
| `column-reordered.png` | `make test-interaction` | `Mod+Shift+Right` 把 focused terminal column 从 x=16 重排至 x=520 |
| `niri-column-stacked.png` | `make test-interaction` | `Mod+Comma` 把右侧 System 顶窗 consume 到 Terminal column 底部，两窗保持相同列宽并上下平铺 |
| `niri-window-height-increased.png` | `make test-interaction` | `Mod+Shift+Equal` 按 gap-aware 10% 把 focused System 从 340 px 增至 411 px，同时补偿同列 Terminal |
| `niri-preset-window-height.png` | `make test-interaction` | `Mod+Ctrl+Shift+R` 按 KDL preset 把 focused System 从 50%/340 px 切到 66.7%/458 px |
| `niri-preset-column-width.png` | `make test-interaction` | `Mod+R` 按 gap-aware KDL preset 把 Terminal 从 50%/488 px 切到 66.7%/656 px，`Mod+Shift+R` 再恢复 |
| `niri-window-moved-up.png` | `make test-interaction` | `Mod+Ctrl+K` 把 focused System 从列底移到 Terminal 上方，同时保持 System focus 与 Waybar title |
| `niri-window-focus-up.png` | `make test-interaction` | `Mod+K` 在纵向 stack 内从 System 聚焦到上方 Terminal，focus ring 与 Waybar title 同步变化 |
| `niri-column-expelled.png` | `make test-interaction` | `Mod+Period` 把底部 System expel 回右侧 column，并按 niri 语义把焦点留在 Terminal |
| `niri-column-centered.png` | `make test-interaction` | `Mod+C` 只移动 viewport，把 488 px Terminal 从 x=16 精确居中到 x=268，System 仍保留在右侧 strip |
| `niri-column-maximized.png` | `make test-interaction` | `Mod+F` 把 Terminal 从 488 px 最大化至保留两侧 gap 的 992 px，再次按键恢复原宽 |
| `niri-column-expanded.png` | `make test-interaction` | System 为 319 px 时，`Mod+Ctrl+F` 把 Terminal 从 488 px 扩到 657 px，使两列与三个 16 px gap 恰好填满 output |
| `mouse-resized.png` | `make test-interaction` | Super+右键横拖把 focused terminal column 从 488 px 放大至 584 px |
| `niri-workspace-number.png` | `make test-interaction` | KDL `focus-workspace 2` 经真实 `Mod+2` 输入切到 Config，顶部显示 active 2 |
| `niri-move-workspace-number.png` | `make test-interaction` | KDL `move-column-to-workspace 3` 经真实 `Mod+Ctrl+3` 把 Terminal 整列移到 active 3，并自动追加可见 workspace 4 |
| `niri-workspace-name.png` | `make test-interaction` | KDL `focus-workspace "config"` 经真实 `Mod+Alt+C` 输入按名称切到 Config |
| `niri-move-workspace-name.png` | `make test-interaction` | KDL `move-column-to-workspace "config"` 经真实 `Mod+Ctrl+Alt+C` 把 Terminal 整列移入 named workspace |
| `niri-workspace-previous.png` | `make test-interaction` | KDL `focus-workspace-previous` 经真实 `Mod+Tab` 返回 Config；第二次按键再回 main |
| `waybar-workspace-click.png` | `make test-interaction` | 点击顶部数字 `2` 后 `niri/workspaces` 显示 active 2，并切入 Config surface |
| `custom-config-workspace-click.png` | `make test-desktop-custom-config` | JSONC 把 `niri/workspaces` 移到 center 后，点击中央数字 `2` 切入 Config |
| `custom-config-on-click.png` | `make test-desktop-custom-config` | JSONC 为右侧 clock 配置 `on-click: status`；Terminal 保留点击前尚未执行的 `ABO`，并显示 STATUS 响应 |
| `custom-config-on-click-right.png` | `make test-desktop-custom-config` | clock 的 `on-click-right: help` 经真实 PS/2 右键命中，Terminal 显示受限 HELP 响应 |
| `custom-config-on-click-middle.png` | `make test-desktop-custom-config` | clock 的 `on-click-middle: swww query` 经真实 PS/2 中键命中，Terminal 显示 Aurora query 响应 |
| `custom-config-scroll-up.png` / `custom-config-scroll-down.png` | `make test-desktop-custom-config` | IntelliMouse 四字节滚轮向上/下命中 clock action，并无过渡切换 Sunset/Aurora |
| `workspace-config.png` | `make test-interaction` | `slopos-config` window rule 与 `Mod+Down` 切入 named workspace |
| `wallpaper-cleared.png` | `make test-interaction` | `swww clear 1a2b3c` 直接填充 framebuffer，Terminal 同时显示 `SWWW COLOR APPLIED` |
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

PID 2 首轮打开 inode 20 Waybar JSONC，以 offset 0/256/512/768 的四个 chunk 读齐默认 904 bytes并验证 EOF，再打开 inode 17 swww environment，读齐默认 172 bytes并验证 EOF；读取器允许非空 Waybar/swww 分别扩展至 4096/512 bytes。`SLOPOS-DESKTOP-SERVICE` submission marker 记录 protocol 1、40-byte commit、`waybar-provider/swww-policy` capability、CPU 0、Memory 36、Aurora，以及实际 VFS hash `0xd34d4a92c88d065b`/`0xc5edd6e5c5369f52`；私有 syscall marker 记录编号 `1397489665` 与 result 0。第二个私有 syscall（`1397489666`）以 32-byte user buffer、event kind 与 `after_generation` 进入 Blocked；desktop task随后记录 snapshot generation 1、相同 owner/capability/provider 值及 kernel renderer boundary，swww marker 证明初始 Aurora 只在 policy 到达后应用。ack marker 再唤醒 block task，completion marker 记录 `Blocked → Runnable`、`policy-applied` generation 1；PID 2 decode event 后只在首次写一次 ready message，然后等待 `config-applied`。

`make test-desktop-custom-config` 不修改仓库标准镜像：脚本复制 ESP/rootfs，用 `debugfs` 在临时 ext4 中把 inode 20 替换成 1267-byte、仍可解析的 JSONC，把 `niri/window`/`niri/workspaces` 分别放到 left/center，并为 clock 配置三键与双向滚轮 action。`custom-config-serial.log` 先证明 IntelliMouse 协商为 `wheel=true`；PID 2 在 policy generation 1 与 config generation 1 唤醒后的 generation 2 都读到 1267 bytes，并两次提交新的 Waybar hash `0x9a4d891048493fad`，而不是默认 `0xd34d4a92c88d065b`，swww hash 保持不变。末尾 marker 为 `desktop service parked ... after_generation=1`，随后真实 PS/2 点击中央 workspace `2`/`1` 往返；终端先收到尚未执行的 `ABO`，左击右侧 clock 后日志记录 action accepted 与 `command=STATUS`，截图保留 `ABO` 输入，补入 `UT` 后又产生 `command=ABOUT`；右击产生 HELP，中击产生 SWWW QUERY；滚轮 `dz=-1/+1` 分别记录 `scroll-up/scroll-down`，无过渡切到 Sunset 再回 Aurora。六张截图覆盖 workspace、输入保持/STATUS、HELP、query 与两向滚轮壁纸。全程没有 exit/FATAL，宿主 `e2fsck -fn` 也接受测试副本。这覆盖有效自定义文件、module placement、点击几何、三键/滚轮受限 action 和输入缓冲保持，不覆盖自动文件监听、POSIX shell 或完整 Waybar schema。

当前五条主要 QEMU 日志都出现 `userspace runtime parked init=wait4 desktop=config-applied`；初始 VFS config generation 1 的 acknowledge 唤醒 PID 2，服务重读两份文件、提交并收到 policy generation 2，随后在 `after_generation=1` 再次稳定休眠。交互日志进一步证明 config generation 2 唤醒 service 并产生 policy generation 3；非法 reload 保留 config generation 2，没有 policy generation 4。脚本同时拒绝任何 `state=exited` 或 `FATAL`。`make test-process` 的 6 项宿主测试另行覆盖 child-exit wake、immediate zombie reap、initial stack、Blocked/Runnable/round-robin、PID/parent/child lookup/lifecycle、exit cleanup 和 per-process fd isolation/seek。当前证据不证明任意 exec、多 segment mapping、动态并发 syscall、通用 wait selector/options/orphan adoption 或通用 namespace mutation。

ELF parser 的宿主测试由 `make test-elf` 执行。10 项测试验证 ELF64 little-endian/x86-64/`ET_EXEC` header、program-header table、`PT_LOAD` data/BSS view，并拒绝 truncation、越界、`p_filesz > p_memsz`、非法 alignment/congruence、重叠 segment、W+X 与不属于 executable segment 的 entry。parser 无分配且 `no_std`；section header 与 dynamic linking 不参与当前装载。

shell/protocol 状态机由 `make test-shell` 验证。13 项 layout 测试覆盖 niri KDL、稳定 strip coordinate、focus/scroll/center/expand/stack/consume/expel/window+column reorder/gap-aware explicit+preset+maximize width/height resize+reset/close；3 项 niri shell 测试覆盖 named workspace、bind/rule/size/preset parse/reject、ordered override、同 strip reorder、跨 workspace move 和容量内动态 normalize；4 项 Waybar JSONC/format 与 3 项 CSS 测试覆盖 module list/option、replacement、cascade、color/box/border 与拒绝边界；7 项 swww 测试覆盖 CLI/environment parse、daemon lifecycle/output、P3/PNM 边界、两个嵌入式 asset 和 transition mask/blend；5 项 desktop protocol 测试覆盖两种 event kind 的 round-trip 与 header/capability/range/generation/reserved 拒绝边界，共 35 项。裸机先由 PID 2 发布 provider/wallpaper policy并等待 apply event，再从 `/etc/slopos/{niri.kdl,waybar.jsonc,waybar.css,swww.env}` 读取 kernel reload bank并原子发布 config generation 1；apply marker 记录 3 个 workspace、37 个 bind、6 个 Waybar module config 与 12 条 CSS rule，service 随后重读并发布 policy generation 2。

交互日志用 `RELOAD` 发布/应用 config generation 2、唤醒 PID 2并发布 policy generation 3；诊断 `RELOAD BAD` 注入非法 CSS后记录 `invalid-waybar-style retained_generation=2`，没有 config generation 3或 policy generation 4。随后日志记录 Sunset `img`、5 帧 center transition、返回当前 image 的 `query`、kill/restart、无 transition 换图、`clear 1a2b3c`、返回 `0x1A2B3C` 的 query、恢复 Sunset；`Mod+Comma` 把 System consume 到 Terminal 列底部，`Mod+Shift+Equal` 产生 gap-aware 340→411 px 高度 marker，`Mod+Ctrl+R` reset 为 340 px，三次 `Mod+Ctrl+Shift+R` 产生 preset 340→458→221→340 px，再以 `Mod+Shift+Minus/Equal` 验证 269→340 px，并始终守恒同列像素。`Mod+Ctrl+K/J` 把 System 移到顶部再移回且 focused id 保持 1，`Mod+K/J` 上下聚焦，`Mod+Period` 把底窗 expel 到右侧并保留 Terminal 焦点；两次 `Mod+F` 把 Terminal 488→992→488 px 最大化/恢复，`Mod+R` 再按 KDL preset 把它切到 656 px，`Mod+Shift+R` 恢复 488 px。

后续 marker 还覆盖 titlebar drag、Terminal 488→588→488 px 相对缩放、Terminal x=16→520→16 的列重排、Super+右键把 Terminal 488→584→488 px逐像素缩放、`Mod+C` 把 Terminal x=16→268 精确居中且 focus 右/左恢复 edge layout、System 488→319 后 `Mod+Ctrl+F` 把 Terminal 488→657 并令可见列填满 output、`Mod+2/1` 与 `Mod+Alt+C/M` 分别按索引/名称在 config/main 间直接切换、`Mod+Ctrl+3` 把 Terminal 移入末尾空位并触发 workspace `3→4`、`Mod+Ctrl+1` 移回并触发 `4→3`、`Mod+Ctrl+Alt+C/M` 按名称移动整列再移回、连续两次 `Mod+Tab` 在 config/main 间往返、Waybar 点击 `2` 切入 `config` 并截图、点击 `1` 返回 `main`、两次 workspace 1 close、`Mod+Down` 切入 `config` 与 workspace 2 close。`desktop.png`/`terminal-status.png` 是 Aurora，并显示由用户 policy 初始化、CSS 驱动的 CPU/Memory 色块；`wallpaper-switched.png` 是 Sunset，`wallpaper-cleared.png` 显示纯色和成功响应；十张 `niri-column-*`/`niri-window-*`/`niri-preset-*` 截图显示两列→纵向 stack→显式与 preset 窗高→窗口上移→焦点上移→恢复两列→列居中→最大化列→preset 列宽→余宽扩展；`window-moved.png` 显示恢复后的 Sunset 和横移后的 main columns，`window-resized.png`/`mouse-resized.png` 分别显示键盘和 pointer 放大，`column-reordered.png` 显示重排后的 main columns，五张 `niri-workspace-*`/`niri-move-workspace-*` 截图显示索引/名称/previous/dynamic 切换和整列移动，`waybar-workspace-click.png` 显示 active workspace 2 与 Config，`workspace-config.png` 显示 window rule 放置的 Config 及 active workspace 2，`wallpaper-only.png` 显示关闭所有 tile 后仍由 Waybar 覆盖的完整壁纸。它不证明自动文件监听、超过 4 个 workspace、完整 niri/Waybar、通用 Waybar module action、独立 swww socket/layer-shell daemon、Wayland layer-shell 或其他图片格式。

eBPF 的宿主边界测试由 `make test-ebpf` 执行；裸机证据是 `serial.log` 中的 `SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42`。它只证明文档所列子集，不证明 map、attach point 或 Linux eBPF 兼容性。

ACPI parser 的宿主测试由 `make test-acpi` 执行。裸机日志记录 QEMU MADT 的 1 个 processor、1 个 IOAPIC、5 个 interrupt override，并记录硬件读取到的 LAPIC/IOAPIC ID、24 条 redirection 和 ISA route `2/1/12`；随后出现 timer Future 与 PS/2 交互事件，证明新路由实际收到了 IRQ。

PCI 枚举器的宿主测试由 `make test-pci` 执行。裸机日志包含 QEMU q35 的设备总数和实际 virtio-blk BDF；当前证据为 `00:03.0`、device ID `1001`，完整 region 校验后的 capability mask `0x1e`（configuration type 1–4）。OVMF 分配的 modern BAR base 为 `0xc000000000`，因此 CR3 证据同时包含跨 PML4 slot 的 7 个 table frame。

virtio layout 测试由 `make test-virtio` 执行。4 项宿主测试包含 read/write/flush descriptor direction。裸机 `SLOPOS-VIRTIO` 证据来自真实 descriptor DMA 与 INTx→waker→Future：queue size 8，root device 报告 524288 sectors并接受 flush，并执行 1 个双请求批次。timer preemption 与 desktop event wake 会改变两个进程和 cache probe 的合法交错；clean boot/interaction 日志覆盖 157–163 hit、119–128 miss、16–18 invalidation，以及 497/496 至 510/509 requests/interrupts。脚本解析最后一条 summary，核对 request 恒比 interrupt 多一、top-half 与 queue interrupt 相等，并把 summary 相对两次用户写的合法先后限制在 16–18 invalidation，而不要求某一种单一 interleaving。

ext4 parser 测试由 `make test-ext4` 执行。裸机日志证明 4096-byte block、65536 blocks、32 inodes、2 groups、group 0 inode table 37、root extent 39 和 6 个 root entries；superblock/group/inode/directory checksum 均由内核校验。`/sbin/slop-init` 是 inode 23、26344 bytes/seven blocks，`/sbin/slop-shell` 是 inode 24、26560 bytes/seven blocks。`multiblock.bin` 是 inode 30，`deep-extent.bin` 是 inode 28；后者从 root index 进入 leaf block 104，验证 extent-block checksum 后读取 logical block 8 的 physical block 111，并将 logical block 7 的 hole 零填充。inode 29 的两个目录块均经 checksum parser，目标 hard link 在第二块解析为 inode 21。path walker 还从 inode 14 取得 inline target，并在同一父目录解析到 inode 21。

`write-probe.bin` 是 inode 31 / physical block 116。PID 1 先在 offset 123 处理两次跨页 64-byte payload并恢复；kernel probe 再处理两次 73-byte payload。ext4 层每次都执行整块 read-modify-write、flush、cache invalidation 与 fd 读回。固定 metadata 后镜像 SHA-256 为 `f5d7bf06a24a0baf7a8a2ed350ce370f81ad6ab9198d3ab1b721ab40032e17d6`；启动测试后的 hash 相同且 `e2fsck -fn` 报告 31/32 files、4243/65536 blocks。这只证明已分配数据块的有界原位写，不证明用户态文件增长或 metadata mutation syscall。

JBD2 宿主测试解析 big-endian v2 superblock，并拒绝 truncation、非法 geometry 和未知 feature；round-trip/corruption 测试覆盖 descriptor/data/commit，状态测试覆盖 ext4 recovery-bit CRC32C 恢复与 JBD2 sequence/start 转换。裸机 marker 证明 journal inode 8 的单一 extent 从 physical block 32801 开始，superblock 报告 4096 blocks、first 1、sequence 1、start 0、users 1、零 feature words，UUID 与 ext4 匹配。它不证明 journal clean 或 replay。

第二个裸机 marker 证明 sequence 1 / target block 116 的 descriptor/data/commit records 被写到 32802–32804；descriptor+data 和 commit 分别由 flush 隔开，三块读回一致，之后清零恢复。marker 明确带 `active=false`，因为尚未写 journal state 或 ext4 recovery bit。

第三个 marker 独立证明 ext4 recovery bit/checksum 与 JBD2 sequence 1/start 1 被持久化并读回；普通 ext4 parser 在 active 状态拒绝 mount。清理先归零 journal start，再清 recovery bit，最终宿主 hash/fsck 证明恢复。`transactions=0` 表示它尚未与上一组 records 组合。

第四个 marker 证明真正组合的单块 active data transaction：recovery/start 和 descriptor/data/commit 均跨 flush 持久化，DMA readback 验证此时可 replay；home block 116 checkpoint 后推进 sequence 2/start 0 并清 recovery。测试收尾清 records、恢复全 `P` home block并将 sequence 回卷到 1，因此启动后的 image SHA-256 仍为固定值且 `e2fsck -fn` 通过。

第五个 marker 证明 inode 31 所在 inode-table block 38 也作为 JBD2 home target：sequence 1 transaction 把 size/checksum 更新为 4095/valid，sequence 2 transaction 恢复 4096/valid，最终 journal sequence 为 3。两次 cache 失效后的 inode parser 均接受整块 metadata；测试回卷 sequence 后，固定 image hash 与 `e2fsck -fn` 再次证明完整恢复。

第六个 marker 证明 fd 3 的 append/truncate 与五 tag allocation transaction 同步覆盖 blocks 0/1/33/38/117。descriptor 在 EOF 4096 取得 4096-byte append window；内核把 superblock/group free count 各减一、更新 block bitmap CRC32C 与 descriptor checksum、增长 inode size/i_blocks/extent，并把 node size 扩为 8192、offset 推进到 EOF。新增 logical block 1 经普通 fd read 路径读回全 `G`；第二笔 transaction 释放 block，descriptor truncate 回 4096，五块逐字节恢复。

第七个 marker 证明 VFS create/unlink transaction 同步覆盖 blocks 0/1/36/38/102。全局/group 1 free inode 与 `itable_unused` 由 1→0，inode bitmap CRC、group checksum、inode 32 checksum 和 directory tail checksum 全部重算；正常 path walker 打开 size 0 的 `create-probe`，固定表为它复用读写 fd 3 且 read 返回 EOF。close 后第二笔 transaction 经共享 directory remover 与 inode-bitmap encoder 回到原始五块。最终固定 hash/fsck 排除 inode、目录项或计数泄漏。

两阶段 recovery 证据来自独立 injection/replay 日志。phase 1 marker 明确记录 sequence 1/start 1、五个 targets 0/1/33/38/117、`allocated/grown` 旧状态、`free/original` 新状态与 `after_commit_before_home` 停止点；宿主同时确认 recovery feature、free blocks 61292、bitmap、inode 31 size/blockcount 与全 G data。phase 2 普通 kernel 在任何 ext4 path read 前报告五 tag replay、全部 home readback、next sequence 2、records cleared 和 recovery false，随后执行恢复后的两个 user ELF、cooperative yield、双向 timer preemption、desktop policy commit 与异步 VFS 读写，并用 sequence 2 继续完整 probes/config boot；当前日志为 539 requests/538 queue interrupts，脚本核对其恒差一。宿主把五个 crash home 与注入前快照逐块比较，确认 free blocks 61293、block 117 释放并运行五阶段 fsck；脚本最后恢复固定-hash 标准镜像。

VFS 宿主测试由 `make test-vfs` 执行。5 项测试覆盖 path/mount/fd offset、access mode 与 EOF growth。裸机前两个 `SLOPOS-VFS` marker 证明从固定 root path 读出 inode 23/24 的七块 ELF，init 通过引导副本比对；随后 PID 1 以 fd 3 异步读取 inode 18，PID 2 以自己的 fd 3 分块读取 inode 20/17 并提交 desktop policy，PID 1 另以 O_RDWR/lseek/write/read 对 inode 31 执行可逆 patch。kernel probe 的独立 namespace marker 仍证明 normalized absolute path 经 root mount 解析到 filesystem 1，以 5 个 chunk 读取配置，并在 offset 7 再读取 11 bytes；之后另以读写模式完成 inode 31 的 73-byte write/read/restore。后续 marker 覆盖 append/truncate 与 create/open/close/unlink；`SLOPOS-CONFIG` markers 还证明同一 root ext4 walker 发现四份配置、发布 config generation 1/2、逐次唤醒 PID 2并在非法 CSS 时保留 generation 2。process fd table 已实际连接有界 root ext4 读写；mount、二维 backing-object array 与其他 probe table 仍是 block task 局部的固定容量状态。
