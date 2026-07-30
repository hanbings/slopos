#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
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
    "SLOPOS-KERNEL: ACPI RSDP validated"
    "SLOPOS-KERNEL: framebuffer ownership accepted"
    "SLOPOS-DESKTOP: interactive compositor loop entered"
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
