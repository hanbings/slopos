# Services

`/sbin/slop-shell` 已作为 PID 2 执行一次 Waybar/swww policy 提交，并在 blocking event syscall 上等待实际应用 acknowledgement；收到首个事件后仍会退出，不具备跨 reload supervision 或完整 service 生命周期。`slopd`、unit/dependency model、restart policy 与常驻服务管理仍未实现。
