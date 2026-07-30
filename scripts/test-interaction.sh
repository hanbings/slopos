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
    monitor_type "reload"
    sleep 3
    monitor_type "reload bad"
    sleep 3
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
    echo "sendkey meta_l-q 50"
    sleep 1
    echo "sendkey meta_l-q 50"
    sleep 1
    echo "sendkey meta_l-down 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/workspace-config.ppm"
    echo "sendkey meta_l-q 50"
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

grep -Fq \
    "SLOPOS-VFS: executable loaded path=/sbin/slop-init inode=23 bytes=26200 blocks=7 matches_boot=true" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: pid=1 parent=0 source=vfs path=/sbin/slop-init argv1=--system format=elf64" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: pid=2 parent=1 source=vfs path=/sbin/slop-worker argv1=--probe format=elf64" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-VFS: process read complete pid=1 fd=3 inode=18 offset=0 requested=76 bytes=76 user_pages=1 cross_page=false async=true" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-VFS: process write complete pid=1 fd=3 inode=31 offset=123 requested=64 bytes=64 user_pages=2 cross_page=true async=true flushed=true" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: pid=1 exit resources released descriptors=1 backing_objects=1 address_space_retained=true" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: pid=2 exit resources released descriptors=0 backing_objects=0 address_space_retained=true" \
    "${serial_log}"
grep -Fq "SLOPOS-TERMINAL: command=STATUS" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: reload requested generation=1 accepted=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: VFS load published initial=false generation=2 atomic=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: reload applied generation=2 atomic=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: invalid reload requested generation=2 accepted=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: VFS load rejected initial=false error=invalid-waybar-style retained_generation=2" "${serial_log}"
if grep -Fq "SLOPOS-CONFIG: reload applied generation=3" "${serial_log}"; then
    echo "invalid desktop configuration was published" >&2
    exit 1
fi
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=center step=64 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=center step=64 fps=30 frames=5" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=SUNSET.PPM" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon stopped" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon started" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=none step=255 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SHELL: view scrolled workspace=1 offset=" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window moved kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=close-window changed=true workspace=1 name=main" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-down changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=SYSTEM" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=CONFIG" "${serial_log}"
test -s "${repo_dir}/evidence/terminal-status.ppm"
test -s "${repo_dir}/evidence/wallpaper-switched.ppm"
test -s "${repo_dir}/evidence/window-moved.ppm"
test -s "${repo_dir}/evidence/workspace-config.ppm"
test -s "${repo_dir}/evidence/wallpaper-only.ppm"
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${repo_dir}/evidence/terminal-status.ppm" \
        >"${repo_dir}/evidence/terminal-status.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-switched.ppm" \
        >"${repo_dir}/evidence/wallpaper-switched.png"
    pnmtopng "${repo_dir}/evidence/window-moved.ppm" \
        >"${repo_dir}/evidence/window-moved.png"
    pnmtopng "${repo_dir}/evidence/workspace-config.ppm" \
        >"${repo_dir}/evidence/workspace-config.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-only.ppm" \
        >"${repo_dir}/evidence/wallpaper-only.png"
fi
echo "SlopOS VFS config reload/rollback, swww, niri workspace/bind/rule, viewport, and close verified"
