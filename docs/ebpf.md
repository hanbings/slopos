# eBPF verifier and interpreter subset

当前 `ebpf` crate 是一个无标准库、无动态分配的 eBPF 指令子集。它用于逐步建立内核内的可验证执行机制；不是 Linux eBPF 的兼容性声明。

## 已实现指令

| 类别 | 当前支持 |
|---|---|
| 编码 | 标准 little-endian 8-byte instruction decode；寄存器 nibble、signed offset、signed immediate |
| 立即数 | `LDDW`，并校验完整、规范的第二槽 |
| ALU64 | immediate/register 的 `ADD`、`SUB`、`MUL`、`DIV`、`OR`、`AND`、`LSH`、`RSH`、`MOD`、`XOR`、`ARSH`；`MOV` 与 `NEG` |
| 控制流 | forward-only `JA`；immediate/register 的 `JEQ`、`JGT`、`JGE`、`JSET`、`JNE`、`JSGT`、`JSGE`、`JLT`、`JLE`、`JSLT`、`JSLE`；`CALL` 与 `EXIT` |
| 内存 | 以 `r10` 为基址的 `B/H/W/DW` stack load、immediate store 和 register store |
| helper | 加载时显式 allowlist；调用后 `r0` 初始化，`r1`–`r5` 按易失寄存器失效 |

解释器提供 11 个 64-bit 寄存器、512-byte 零初始化 stack 和 4096 步硬上限。`r1` 初始保存调用者提供的 context 数值，`r10` 固定为 stack frame pointer。

## Verifier 不变量

- 程序为 1–4096 条 instruction，且所有 opcode 和寄存器编号有效；
- 只接受前向跳转，目标必须在程序内且不能落入 `LDDW` continuation，因此当前程序结构可确定终止；
- 每个可达路径都必须以已初始化的 `r0` 执行 `EXIT`，不能落出程序末尾；
- 控制流汇合以交集保留“所有前驱均初始化”的寄存器；
- 禁止写 `r10`，stack 访问必须完整落在 512-byte 边界内；
- `CALL` 只能使用加载方 allowlist 中的 helper id。

## 明确未实现

- ALU32、endian conversion、atomic、packet/context memory dereference；
- backward jump 和可证明有界的 loop；
- stack byte 初始化跟踪；当前解释器把整个 stack 初始化为零；
- ELF/BTF loader、relocation、map、tail call、program type；
- tracepoint、syscall、network 等 attach point；
- capability/权限模型、JIT、SMP 并发执行和 Linux verifier 兼容性。

`make test-ebpf` 覆盖指令解码、ALU/stack、helper allowlist、易失寄存器、未初始化寄存器、非法 backward jump、stack 越界、畸形 `LDDW` 与除零。`make test-boot` 还在真实 SlopOS 内核中验证并解释执行 5 条指令，串口必须报告结果 `42`。
