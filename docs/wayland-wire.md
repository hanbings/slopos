# Wayland wire/object core

`crates/wayland` 是 `no_std`、无堆分配的 Wayland server-side protocol core。它把协议状态与现有 framebuffer desktop 分开，作为后续用户 client transport 和 compositor surface 接入的可测试边界。

当前已实现：

- little-endian 8-byte message header，按 `object_id + (size << 16 | opcode)` 解码，可从连续 byte stream 返回单帧及 remainder；
- 4-byte alignment、最小/最大长度、截断、null object、尾随参数与 UTF-8/NUL string 拒绝；
- `uint`、`int`、`fixed`、object/new-id、nullable object/string、string、array 的无分配 reader/builder；
- 固定容量 object map，区分 active/retired/empty；destroy 后在 `wl_display.delete_id` 发出前拒绝复用 object id；
- `wl_registry.global/global_remove`、`wl_display.error/delete_id`、`wl_callback.done` event 编码；
- `wl_seat.capabilities/name`、`wl_pointer.enter`、`wl_keyboard.keymap/enter/key/modifiers/repeat_info` 与 `wl_output.geometry/mode/done/scale/name/description` event 编码，包含正尺寸、正刷新率/scale与非零serial边界；
- 固定 global table：`wl_compositor` v6、`wl_shm` v1、`wl_seat` v9、`wl_output` v4、`xdg_wm_base` v6；bind 会核对 global name、interface 字符串、非零版本和 advertised maximum；
- request dispatch 子集：
  - `wl_display.sync/get_registry`；
  - `wl_registry.bind`；
  - `wl_compositor.create_surface/create_region`；
  - `wl_surface.destroy/attach/damage/frame/set_*_region/commit/set_buffer_transform/set_buffer_scale/damage_buffer/offset`；
  - `wl_region.destroy/add/subtract`；
  - `wl_shm.create_pool` 和 `wl_shm_pool.create_buffer/destroy/resize`，FD 通过独立 slice 表达，不混入 wire payload；
  - `wl_buffer.destroy`；
  - `wl_seat.get_pointer/get_keyboard/release`、`wl_pointer.set_cursor/release`、`wl_keyboard.release`、`wl_output.release`；
  - `xdg_wm_base.destroy/get_xdg_surface/pong`；
  - `xdg_surface.destroy/get_toplevel/set_window_geometry/ack_configure`；
  - `xdg_toplevel.destroy/set_title/set_app_id/move/resize/maximize/fullscreen/minimize` 常用请求。

宿主测试覆盖连续帧、header/string 拒绝边界、global bind/version、object retirement/delete-id、surface/frame/commit、带外 shm FD/buffer、seat/pointer/keyboard/output事件、xdg toplevel metadata，以及错误 interface/opcode/FD/duplicate id。运行：

```sh
make test-wayland
cargo clippy --locked -p slopos-wayland --all-targets -- -D warnings
```

该协议核心现已接入 kernel 的固定容量 AF_UNIX `SOCK_STREAM`：PID 2 通过 `/run/slopos/wayland-0` 的普通 `socket/connect/write/read/sendmsg/recvmsg` 完成 registry、device discovery、bufferless initial commit、xdg configure/ack、pointer/keyboard focus、两轮 buffer commit 与 presentation event。registry之后，PID 2绑定`wl_seat` object 12与`wl_output` object 13、从seat创建`wl_pointer` object 14和`wl_keyboard` object 15；308-byte initial batch保留256-byte通用syscall容量，以256+52两次write进入同一stream，服务端一次read后按连续frame解析。

sequence 2的288-byte server batch依次包含`wl_seat.capabilities(pointer|keyboard)`、`wl_seat.name("seat0")`、`wl_keyboard.keymap(xkb_v1,size=3307)`、`wl_keyboard.repeat_info(25,600)`、`wl_output.geometry(0,0,270×203,"SlopOS","Virtual Display")`、current+preferred `wl_output.mode(1024×768@60000)`、scale 1、name `SLOPOS-1`、description、done、两个shm format及两层xdg configure。客户端先以`recvmsg`取得前256 bytes与一个`SCM_RIGHTS` fd，再普通读取余下32 bytes；该fd是只读、完整、自包含且NUL-terminated的XKB v1 keymap。PID 2分块读完3307 bytes，核对FNV-1a、末尾NUL与EOF后关闭fd。generation 1呈现后，76-byte event以serial 2发送`wl_pointer.enter`，以serial 3发送带空按键数组的`wl_keyboard.enter`，再发送buffer release、callback done和delete-id；generation 2仍只发送后三项32 bytes。PID 2逐帧核对object/opcode与关键capability/mode/scale/serial值。

它用 `memfd_create/ftruncate/mmap(MAP_SHARED)` 建立一页 backing，首个 configured batch通过 `sendmsg` 的 `SOL_SOCKET/SCM_RIGHTS` control message附带 fd；socket core在两个方向上都能把一个 generation-checked rights object与对应stream bytes原子关联。服务端retain同一 shared frame，后续无 fd 的64-byte batch从该页读取已更新 pixels并复用既有pool/buffer。持久 `SingleSurfaceSession` 强制 object ownership、role exclusivity、seat/pointer/keyboard/output存在、configure serial、首次 pool FD、pending/current commit边界及callback retirement，并把generation 1/2交给现有kernel renderer。私有 `0x534c0005` staging syscall已删除。当前已交付keymap、repeat参数与初始keyboard focus，但尚无持续PS/2→`wl_pointer`/`wl_keyboard.key`/modifiers fan-out或用户态poll事件循环。transport仍限单一受信PID 2；也没有用户态 `bind/listen/accept`、通用 ancillary control、多页或可选地址mmap、`munmap`、多client隔离、layer-shell或持续frame loop。协议错误仍视为fatal connection error；dispatch不保证错误后的状态可继续使用。
