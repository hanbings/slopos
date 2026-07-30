# Async kernel model

当前状态：**部分实现**。

已经实际运行并由 `make test-boot` 验证：

- 三个 pinned `Future` task（input、timer、block）；
- 原子 ready bit queue；
- 每 task RawWaker；
- 100 Hz PIT interrupt 唤醒 timer future；
- PS/2 IRQ 上半部读取/确认设备并写入固定容量 SPSC ring；
- input future 在下半部解析扫描码和鼠标 packet；
- virtio INTx top half 读取 ISR 并 wake block future；future 在下半部消费 used ring；
- executor 空闲时以 `sti; hlt` 等待，避免忙轮询；
- ring 满时统计 drop，提供最早期 backpressure 可观察性。

当前 executor 是固定三任务、无动态 spawn 的早期实现，不满足最终完整异步内核范围。

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

真实单次 virtio I/O completion 已完成；在 task arena、timer wheel、async locks、cancellation、timeout、bounded multi-request producer wakeup 和多核 run queue 完成前，本子系统保持“部分实现”。
