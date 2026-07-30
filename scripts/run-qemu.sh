#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
ovmf_code="/usr/share/OVMF/OVMF_CODE_4M.fd"
ovmf_vars="${repo_dir}/target/OVMF_VARS_4M.fd"

if [[ ! -f "${image}" || ! -f "${root_image}" ]]; then
    echo "missing SlopOS disk image; run 'make image' first" >&2
    exit 1
fi
if [[ ! -f "${ovmf_code}" ]]; then
    echo "missing OVMF firmware at ${ovmf_code}" >&2
    exit 1
fi

cp /usr/share/OVMF/OVMF_VARS_4M.fd "${ovmf_vars}"
mkdir -p "${repo_dir}/evidence"

exec qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=${ovmf_code}" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -drive "if=virtio,format=raw,file=${root_image}" \
    -serial "file:${repo_dir}/evidence/serial.log" \
    -debugcon "file:${repo_dir}/evidence/uefi-debugcon.log" \
    -global isa-debugcon.iobase=0x402 \
    -no-reboot
