#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
runtime_dir="$(mktemp -d /tmp/slopos-capture.XXXXXX)"
runtime_image="${runtime_dir}/slopos-esp.img"
runtime_root_image="${runtime_dir}/slopos-root.ext4"
ovmf_vars="${runtime_dir}/OVMF_VARS_4M.fd"

cleanup() {
    unlink "${runtime_image}" "${runtime_root_image}" "${ovmf_vars}" 2>/dev/null || true
    rmdir "${runtime_dir}" 2>/dev/null || true
}
trap cleanup EXIT

mkdir -p "${repo_dir}/evidence"
cp --reflink=auto --sparse=always "${image}" "${runtime_image}"
cp --reflink=auto --sparse=always "${root_image}" "${runtime_root_image}"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${ovmf_vars}"

{
    sleep 6
    echo "screendump ${repo_dir}/evidence/desktop.ppm"
    sleep 1
    echo "quit"
} | qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${runtime_image}" \
    -drive "if=virtio,format=raw,file=${runtime_root_image}" \
    -serial "file:${repo_dir}/evidence/capture-serial.log" \
    -debugcon "file:${repo_dir}/evidence/capture-uefi-debugcon.log" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot

sed -i 's/\r$//' \
    "${repo_dir}/evidence/capture-serial.log" \
    "${repo_dir}/evidence/capture-uefi-debugcon.log"

test -s "${repo_dir}/evidence/desktop.ppm"

ppm_pixel_hex() {
    local x="$1"
    local y="$2"
    local offset=$((16 + 3 * (y * 1024 + x)))
    dd if="${repo_dir}/evidence/desktop.ppm" bs=1 skip="${offset}" count=3 status=none \
        | od -An -tx1 \
        | tr -d ' \n'
}

if [[ "$(ppm_pixel_hex 900 123)" != "8be9fd" ]] \
    || [[ "$(ppm_pixel_hex 948 123)" != "ff5555" ]] \
    || [[ "$(ppm_pixel_hex 900 159)" != "bd93f9" ]] \
    || [[ "$(ppm_pixel_hex 948 159)" != "ffb86c" ]]; then
    echo "PID 2 repeated Wayland surface frame was not composited into the System window" >&2
    exit 1
fi

if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${repo_dir}/evidence/desktop.ppm" >"${repo_dir}/evidence/desktop.png"
fi
echo "captured ${repo_dir}/evidence/desktop.ppm"
