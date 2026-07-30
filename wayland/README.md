# Wayland

Wayland wire protocol、object model、globals、shared buffer、layer-shell 与 xdg-shell 尚未实现。当前 `/sbin/slop-shell` 只通过 SlopOS 私有固定 commit/event 消息发布初始桌面策略并等待应用确认；它不构成 Wayland compositor 或 client。
