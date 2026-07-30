# eBPF

当前实现是独立 0BSD Rust eBPF verifier 和解释器子集，支持主要 ALU64、前向 branch、`LDDW`、512-byte stack load/store、helper allowlist、寄存器初始化检查与确定终止检查。

限制：拒绝所有 backward jump，因此尚不支持 verifier 可证明有界的 loop；没有 ELF loader、map、program type、attach point、权限模型或 JIT。状态和运行证据见 `docs/ebpf.md`。
