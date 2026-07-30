#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/interaction-serial.log"
debugcon_log="${repo_dir}/evidence/interaction-uefi-debugcon.log"
runtime_dir="$(mktemp -d /tmp/slopos-interaction.XXXXXX)"
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
truncate -s 0 "${serial_log}" "${debugcon_log}"

monitor_type() {
    local text="$1"
    local character
    local index
    for ((index = 0; index < ${#text}; index++)); do
        character="${text:index:1}"
        case "${character}" in
            " ") echo "sendkey spc 10" ;;
            "-") echo "sendkey minus 10" ;;
            ".") echo "sendkey dot 10" ;;
            "/") echo "sendkey slash 10" ;;
            *) echo "sendkey ${character} 10" ;;
        esac
    done
    echo "sendkey ret 10"
}

{
    sleep 6
    monitor_type "status"
    sleep 2
    echo "screendump ${repo_dir}/evidence/terminal-status.ppm"
    monitor_type "swww img sunset.ppm --transition-type center --transition-step 64"
    sleep 8
    echo "screendump ${repo_dir}/evidence/wallpaper-switched.ppm"
    monitor_type "swww query"
    sleep 2
    monitor_type "swww kill"
    sleep 2
    monitor_type "swww-daemon"
    sleep 2
    monitor_type "swww img sunset.ppm --transition-type none"
    sleep 4
    echo "mouse_move -300 -300"
    echo "mouse_button 1"
    echo "mouse_move -110 0"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${repo_dir}/evidence/window-moved.ppm"
    echo "mouse_move 300 -13"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "mouse_move 110 0"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${repo_dir}/evidence/wallpaper-only.ppm"
    echo "quit"
} | qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${runtime_image}" \
    -drive "if=virtio,format=raw,file=${runtime_root_image}" \
    -serial "file:${serial_log}" \
    -debugcon "file:${debugcon_log}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot >/dev/null

grep -Fq "SLOPOS-TERMINAL: command=STATUS" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=center step=64 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=center step=64 fps=30 frames=5" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=SUNSET.PPM" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon stopped" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon started" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=none step=255 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SHELL: view scrolled offset=" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window moved kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=SYSTEM" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=CONFIG" "${serial_log}"
test -s "${repo_dir}/evidence/terminal-status.ppm"
test -s "${repo_dir}/evidence/wallpaper-switched.ppm"
test -s "${repo_dir}/evidence/window-moved.ppm"
test -s "${repo_dir}/evidence/wallpaper-only.ppm"
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${repo_dir}/evidence/terminal-status.ppm" \
        >"${repo_dir}/evidence/terminal-status.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-switched.ppm" \
        >"${repo_dir}/evidence/wallpaper-switched.png"
    pnmtopng "${repo_dir}/evidence/window-moved.ppm" \
        >"${repo_dir}/evidence/window-moved.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-only.ppm" \
        >"${repo_dir}/evidence/wallpaper-only.png"
fi
echo "SlopOS PS/2 command, swww transition/query, viewport drag, and tiled close verified"
