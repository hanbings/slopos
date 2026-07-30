#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
ovmf_vars="${repo_dir}/target/OVMF_VARS_4M.interaction.fd"
serial_log="${repo_dir}/evidence/interaction-serial.log"

mkdir -p "${repo_dir}/evidence"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${ovmf_vars}"

{
    sleep 6
    echo "sendkey s"
    echo "sendkey t"
    echo "sendkey a"
    echo "sendkey t"
    echo "sendkey u"
    echo "sendkey s"
    echo "sendkey ret"
    sleep 1
    echo "screendump ${repo_dir}/evidence/terminal-status.ppm"
    echo "mouse_move -300 -275"
    echo "mouse_button 1"
    echo "mouse_move 110 65"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${repo_dir}/evidence/window-moved.ppm"
    echo "quit"
} | qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -serial "file:${serial_log}" \
    -debugcon "file:${repo_dir}/evidence/interaction-uefi-debugcon.log" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot >/dev/null

grep -Fq "SLOPOS-TERMINAL: command=STATUS" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window moved kind=TERMINAL" "${serial_log}"
test -s "${repo_dir}/evidence/terminal-status.ppm"
test -s "${repo_dir}/evidence/window-moved.ppm"
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${repo_dir}/evidence/terminal-status.ppm" \
        >"${repo_dir}/evidence/terminal-status.png"
    pnmtopng "${repo_dir}/evidence/window-moved.ppm" \
        >"${repo_dir}/evidence/window-moved.png"
fi
echo "SlopOS PS/2 keyboard command and mouse window drag verified"
