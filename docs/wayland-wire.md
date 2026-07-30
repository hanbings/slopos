# Wayland wire/object core

`crates/wayland` 是 `no_std`、无堆分配的 Wayland server-side protocol core。它把协议状态与现有 framebuffer desktop 分开，作为后续用户 client transport 和 compositor surface 接入的可测试边界。

当前已实现：

- little-endian 8-byte message header，按 `object_id + (size << 16 | opcode)` 解码，可从连续 byte stream 返回单帧及 remainder；
- 4-byte alignment、最小/最大长度、截断、null object、尾随参数与 UTF-8/NUL string 拒绝；
- `uint`、`int`、`fixed`、object/new-id、nullable object/string、string、array 的无分配 reader/builder；
- 固定容量 object map，区分 active/retired/empty；destroy 后在 `wl_display.delete_id` 发出前拒绝复用 object id；
- `wl_registry.global/global_remove`、`wl_display.error/delete_id`、`wl_callback.done` event 编码；
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

宿主测试覆盖连续帧、header/string 拒绝边界、global bind/version、object retirement/delete-id、surface/frame/commit、带外 shm FD/buffer、xdg toplevel metadata，以及错误 interface/opcode/FD/duplicate id。运行：

```sh
make test-wayland
cargo clippy --locked -p slopos-wayland --all-targets -- -D warnings
```

这只是协议核心，不代表 SlopOS 已经具备可连接的 Wayland compositor。尚未接入 Unix-domain socket/SCM_RIGHTS 等 transport、用户进程地址空间中的共享内存映射、kernel desktop renderer、surface pending/current 双缓冲状态、damage/frame scheduler、input/output event fan-out、xdg role exclusivity/configure state machine、多 client 隔离或 layer-shell。错误返回后的 connection 视为 fatal 并应由未来 transport 丢弃；dispatch 不保证错误后的状态可继续使用。
