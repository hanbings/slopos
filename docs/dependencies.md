# Dependency and license inventory

由 `Cargo.lock` 和 `cargo metadata --locked` 在 2026-07-30 生成并人工检查。所有依赖均未修改；没有 GPL、LGPL、AGPL、MPL 或其他 Copyleft 依赖。

| 名称 | 版本 | 上游 | 许可证 | 用途 | 修改 | 进入最终镜像 | 分发影响 |
|---|---:|---|---|---|---|---|---|
| `uefi-raw` | 0.13.0 | https://github.com/rust-osdev/uefi-rs | MIT OR Apache-2.0 | UEFI ABI 数据结构、GUID 和 firmware function table 定义 | 否 | 是，链接入 loader | 保留上游许可证声明；与 0BSD 原创代码兼容 |
| `bitflags` | 2.13.1 | https://github.com/bitflags/bitflags | MIT OR Apache-2.0 | `uefi-raw` 的 ABI flag 类型 | 否 | 是，按实际引用链接 | 与 0BSD 分发目标兼容 |
| `uguid` | 2.2.1 | https://github.com/google/gpt-disk-rs | MIT OR Apache-2.0 | `uefi-raw` 的 GUID 表示与 const macro | 否 | 是，链接入 loader | 与 0BSD 分发目标兼容 |

项目曾在开发中评估高层 `uefi` crate，但发现其当前依赖图包含 MPL-2.0 的 `ucs2`。该依赖在首次提交前已经彻底移除；当前 `Cargo.lock` 不包含它。

宿主构建工具 QEMU、OVMF、Rust、LLVM、mtools、dosfstools、e2fsprogs、socat、netpbm 和可选的 libxkbcommon-tools 不链接进 SlopOS 镜像，属于外部构建/测试工具。`llvm-strip`会移除desktop userspace符号，`llvm-objcopy`再加入不进入`PT_LOAD`的确定性padding，使rootfs中的`/sbin/slop-shell`维持十个ext4 block并避免改变存储探针布局；`mke2fs`/`debugfs`只生成可重复的ext4测试root disk。socat只连接交互回归的本地QMP Unix socket，用于跨PS/2键盘/鼠标设备维持modifier key-down或注入滚轮；`xkbcli compile-keymap --from-xkb`只独立校验仓库内0BSD `assets/slopos-keymap.xkb`的语法与自包含性。
