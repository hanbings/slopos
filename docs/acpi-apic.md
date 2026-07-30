# ACPI and APIC interrupt path

当前实现把 firmware discovery 与硬件接管分成两个边界明确的部分。

`slopos-acpi` 是可在宿主测试的 `no_std` parser：

- 校验 ACPI 1.0 RSDP checksum；
- 对 revision 2+ 校验 declared length 与 extended checksum；
- 校验 RSDT/XSDT signature、长度、entry alignment 和完整 SDT checksum；
- 查找并校验 MADT；
- 解析 processor local APIC、I/O APIC、interrupt-source override、local APIC address override 和 processor x2APIC entry；
- 以固定容量保存最多 8 个 IOAPIC 与 16 个 override，不使用 heap。

内核在切换自有 CR3 前完成表发现，再把 MADT 给出的 LAPIC/IOAPIC page 加入 cache-disabled identity map。中断接管随后：

1. 以 CPUID 检查 xAPIC，配置 `IA32_APIC_BASE` 并启用 software APIC；
2. 屏蔽 local APIC 未使用的 LVT 与 legacy 8259；
3. 屏蔽 IOAPIC 的全部 redirection entry；
4. 按 MADT polarity/trigger override 配置 PIT、keyboard、mouse；
5. 把 destination 指向 bootstrap processor 的 physical APIC ID；
6. 从每个 IRQ top half 写 local APIC EOI。

在当前 QEMU q35 证据中，MADT 报告 1 个 processor、1 个 IOAPIC 和 5 个 override；IOAPIC version register 报告 24 个 redirection entry，IRQ0 被 override 到 GSI 2，keyboard/mouse 分别使用 GSI 1/12。`make test-boot` 必须在新路径上收到 100 Hz PIT 并唤醒 timer Future，`make test-interaction` 必须收到实际 PS/2 keyboard/mouse IRQ。

边界：只启动 bootstrap processor，只使用 physical destination 的 xAPIC 模式；没有 x2APIC、IPI、SMP、interrupt remapping、MSI/MSI-X、affinity、LAPIC timer 或 TSC-deadline。ACPI parser 也不是 AML interpreter。
