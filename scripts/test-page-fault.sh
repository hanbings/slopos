#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
ovmf_vars="${repo_dir}/target/OVMF_VARS_4M.page-fault.fd"
serial_log="${repo_dir}/evidence/page-fault-serial.log"

mkdir -p "${repo_dir}/evidence"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${ovmf_vars}"

{
    sleep 6
    echo "sendkey f"
    echo "sendkey a"
    echo "sendkey u"
    echo "sendkey l"
    echo "sendkey t"
    echo "sendkey ret"
    sleep 1
    echo "quit"
} | qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -drive "if=virtio,format=raw,file=${root_image}" \
    -serial "file:${serial_log}" \
    -debugcon "file:${repo_dir}/evidence/page-fault-uefi-debugcon.log" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot >/dev/null

grep -Fq "SLOPOS-EXCEPTION: injecting page fault at 0x40000000" "${serial_log}"
grep -Fq "SLOPOS-EXCEPTION: vector=14" "${serial_log}"
grep -Fq "cr2=0x40000000" "${serial_log}"
grep -Fq "SLOPOS-KERNEL: FATAL unhandled CPU exception" "${serial_log}"
echo "SlopOS page-fault IDT path verified"
