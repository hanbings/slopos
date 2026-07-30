# Linux ABI

SlopOS 已能从 root ext4 装入两个固定 Rust ELF64 `ET_EXEC`，建立 Linux x86-64 `argc/argv/envp/auxv` initial stack，并执行当前子集的 `openat/read/write/lseek/close/sched_yield/wait4/exit`。同步文件调用通过保存 per-PID frame、异步 ext4/virtio completion 和 `SYSRETQ` 恢复。

桌面服务另使用 SlopOS 私有的 40-byte versioned commit syscall；它不是 Linux ABI。尚无任意路径/多 segment exec、动态链接、广泛 POSIX syscall、VM proxy 或 guest agent。
