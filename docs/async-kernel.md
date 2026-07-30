# Async kernel model

当前状态：**仅完成设计，尚未实现**。本文件不能作为 executor 已完成的证据。

计划模型：

1. 每 CPU run queue 保存可运行 kernel task ID；task 拥有 pinned future 和显式生命周期状态。
2. waker 只把 task ID 从 waiting 原子转换为 runnable 并入队，不在 wake 路径 poll future。
3. interrupt top half 只读取设备完成状态、确认中断并写入固定容量 completion ring；bottom-half task 消费 ring。
4. timer 使用分层 timing wheel；timeout 与 cancellation 都生成一次性的 completion result。
5. async mutex 在 contention 时挂起 task；禁止 interrupt context 获取可能等待的锁。
6. bounded channel 和 request queue 提供 backpressure；满队列返回 pending，并由可用容量事件唤醒 producer。
7. user thread 与 kernel async task 是不同调度实体。同步 syscall 只挂起调用 user thread，其内核 operation 由 async task/completion 推进。

计划调度策略为每 CPU 公平队列加 work stealing；设备 affinity 和持锁 task 默认留在本 CPU。preemption timer 可以抢占 user thread，但 kernel future 的 poll 必须有预算并在边界 cooperative yield。跨 CPU wake 通过 IPI 通知目标 scheduler。

计划 cancellation 语义：drop request future 只撤销尚未提交的请求；已经提交给硬件的请求进入 detached completion，资源直到 completion/timeout reset 后释放。返回用户可见结果只能发生一次。

在 IDT、timer、executor、waker 和至少一个真实中断驱动 completion path 全部运行并有 QEMU 证据前，本子系统保持“仅完成设计”。
