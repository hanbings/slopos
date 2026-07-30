#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
ovmf_vars="${repo_dir}/target/OVMF_VARS_4M.test.fd"
serial_log="${repo_dir}/evidence/serial.log"
debug_log="${repo_dir}/evidence/uefi-debugcon.log"
qemu_log="${repo_dir}/evidence/qemu-test.log"

mkdir -p "${repo_dir}/evidence"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${ovmf_vars}"

set +e
timeout 10s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -drive "if=virtio,format=raw,file=${root_image}" \
    -serial "file:${serial_log}" \
    -debugcon "file:${debug_log}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor none \
    -no-reboot >"${qemu_log}" 2>&1
qemu_status=$?
set -e

if [[ ${qemu_status} -ne 0 && ${qemu_status} -ne 124 ]]; then
    echo "QEMU failed with status ${qemu_status}" >&2
    exit "${qemu_status}"
fi

required_markers=(
    "SLOPOS-UEFI: loader entered"
    "SLOPOS-UEFI: Boot Services exited"
    "SLOPOS-KERNEL: entry reached"
    "SLOPOS-KERNEL: boot info valid"
    "SLOPOS-ACPI: XSDT MADT validated"
    "SLOPOS-MM: frame allocator initialized"
    "SLOPOS-MM: CR3 switched"
    "SLOPOS-MM: kernel heap initialized"
    "SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42"
    "SLOPOS-PCI: mechanism1 devices="
    "SLOPOS-VIRTIO: modern block queue ready size=8 capacity_sectors=524288"
    "SLOPOS-EXT4: superblock valid label=SLOPOS_ROOT"
    "blocks=65536 inodes=32 groups=2"
    "SLOPOS-EXT4: root directory valid group_inode_table=37 inode=2 extent_block=39 entries=5 etc_inode=13 lost_found_inode=11 metadata_checksums=group/inode/directory"
    "SLOPOS-EXT4: async path read valid release_inode=16 release_bytes=40 config_inode=15 config_bytes=76 paths=/etc/slopos-release,/etc/slopos/system.conf"
    "SLOPOS-EXT4: group descriptor valid group=1 inode_table=38"
    "SLOPOS-EXT4: multiblock file valid inode=20 inode_group=1 bytes=6144 logical_blocks=2 path=/usr/share/slopos/multiblock.bin"
    "SLOPOS-VIRTIO: async block sequence complete requests=33 interrupts=33 queue_interrupts=33"
    "SLOPOS-KERNEL: framebuffer ownership accepted"
    "SLOPOS-INPUT: PS/2 keyboard and mouse IRQ queue armed"
    "SLOPOS-INTERRUPT: GDT IDT LAPIC IOAPIC PIT initialized"
    "SLOPOS-DESKTOP: interactive compositor loop entered"
    "SLOPOS-ASYNC: executor entered tasks=3"
    "SLOPOS-ASYNC: timer future completed"
)

for marker in "${required_markers[@]}"; do
    if ! grep -Fq "${marker}" "${serial_log}"; then
        echo "missing boot marker: ${marker}" >&2
        echo "serial output follows:" >&2
        sed -n '1,200p' "${serial_log}" >&2
        exit 1
    fi
done

echo "SlopOS UEFI-to-desktop boot markers verified"
