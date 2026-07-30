# User space

root VFS 当前提供两个 Rust `no_std` ELF：

- `/sbin/slop-init`（PID 1）验证 Linux x86-64 initial stack、异步文件 syscall、跨页 user copy、wait/reap 与资源回收；
- `/sbin/slop-shell`（PID 2）读取 Waybar JSONC 与 swww environment，通过 versioned SlopOS 私有 syscall 提交第一代 provider/wallpaper policy。

两者都在独立 CR3 中运行，并可被 100 Hz timer 抢占。当前 shell 只提交一次策略后退出；compositor、surface、输入、配置 reload、swww daemon 与 framebuffer renderer 仍在 kernel。尚无动态 exec、常驻 service manager、普通系统工具或完整用户态桌面。
