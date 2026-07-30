# Services

`/sbin/slop-shell` 已作为 PID 2 执行一次 Waybar/swww policy 提交，但提交后即退出，不具备 supervision 或 service 生命周期。`slopd`、unit/dependency model、restart policy 与常驻服务管理仍未实现。
