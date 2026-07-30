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
    monitor_type "swww clear 1a2b3c"
    sleep 2
    echo "screendump ${repo_dir}/evidence/wallpaper-cleared.ppm"
    monitor_type "swww query"
    sleep 2
    monitor_type "swww img sunset.ppm --transition-type none"
    sleep 2
    echo "mouse_move -300 -300"
    sleep 1
    echo "mouse_button 1"
    sleep 0.2
    echo "mouse_move -110 0"
    sleep 1
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${repo_dir}/evidence/window-moved.ppm"
    echo "sendkey meta_l-equal 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/window-resized.ppm"
    echo "sendkey meta_l-minus 50"
    sleep 1
    echo "sendkey meta_l-shift-right 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/column-reordered.ppm"
    echo "sendkey meta_l-shift-left 50"
    sleep 1
    echo "sendkey meta_l 2000"
    sleep 0.5
    echo "mouse_button 2"
    sleep 0.2
    echo "mouse_move 96 0"
    sleep 1
    echo "screendump ${repo_dir}/evidence/mouse-resized.ppm"
    echo "mouse_move -96 0"
    sleep 1
    echo "mouse_button 0"
    sleep 1
    echo "sendkey meta_l-2 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-number.ppm"
    echo "sendkey meta_l-1 50"
    sleep 1
    echo "sendkey meta_l-ctrl-3 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-move-workspace-number.ppm"
    echo "sendkey meta_l-ctrl-1 50"
    sleep 1
    echo "sendkey meta_l-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-name.ppm"
    echo "sendkey meta_l-m 50"
    sleep 1
    echo "sendkey meta_l-ctrl-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-move-workspace-name.ppm"
    echo "sendkey meta_l-ctrl-m 50"
    sleep 1
    echo "sendkey meta_l-tab 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-previous.ppm"
    echo "sendkey meta_l-tab 50"
    sleep 1
    echo "mouse_move -22 -64"
    sleep 1
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${repo_dir}/evidence/waybar-workspace-click.ppm"
    echo "mouse_move -24 0"
    sleep 1
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
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

sed -i 's/\r$//' "${serial_log}" "${debugcon_log}"

grep -Fq \
    "SLOPOS-VFS: executable loaded path=/sbin/slop-init inode=23 bytes=26344 blocks=7 matches_boot=true" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: pid=1 parent=0 source=vfs path=/sbin/slop-init argv1=--system format=elf64" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: pid=2 parent=1 source=vfs path=/sbin/slop-shell argv1=--session format=elf64" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP-SERVICE: policy applied generation=1 owner_pid=2 capabilities=waybar-provider/swww-policy" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP-SERVICE: policy acknowledged generation=1 owner_pid=2 event=policy-applied wake=block-task" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-SCHED: pid=2 state=blocked->runnable reason=desktop-event event=policy-applied generation=1" \
    "${serial_log}"
grep -Fq "SLOPOS-SCHED: timer preempt from=1 to=2" "${serial_log}"
grep -Fq "SLOPOS-SCHED: timer preempt from=2 to=1" "${serial_log}"
grep -Fq \
    "SLOPOS-VFS: process read complete pid=1 fd=3 inode=18 offset=0 requested=76 bytes=76 user_pages=1 cross_page=false async=true" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-VFS: process write complete pid=1 fd=3 inode=31 offset=123 requested=64 bytes=64 user_pages=2 cross_page=true async=true flushed=true" \
    "${serial_log}"
grep -Fq "SLOPOS-VFS: process close complete pid=1 fd=3 inode=31 async=false" "${serial_log}"
grep -Fq \
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=kernel nr=61 wait4 child=any state=blocked origin=cpl3" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: userspace runtime parked init=wait4 desktop=config-applied after_generation=0 resources=retained" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: desktop service parked event=config-applied after_generation=1 init=wait4 resources=retained" \
    "${serial_log}"
grep -Fq "SLOPOS-TERMINAL: command=STATUS" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: reload requested generation=1 accepted=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: VFS load published initial=false generation=2 atomic=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: reload applied generation=2 atomic=true" "${serial_log}"
grep -Fq \
    "SLOPOS-SCHED: pid=2 state=blocked->runnable reason=desktop-event event=config-applied generation=2" \
    "${serial_log}"
grep -Fq "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2 generation=3" "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP-SERVICE: policy acknowledged generation=3 owner_pid=2 event=policy-applied wake=block-task" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: desktop service parked event=config-applied after_generation=2 init=wait4 resources=retained" \
    "${serial_log}"
grep -Fq "SLOPOS-CONFIG: invalid reload requested generation=2 accepted=true" "${serial_log}"
grep -Fq "SLOPOS-CONFIG: VFS load rejected initial=false error=invalid-waybar-style retained_generation=2" "${serial_log}"
if grep -Fq "SLOPOS-CONFIG: reload applied generation=3" "${serial_log}"; then
    echo "invalid desktop configuration was published" >&2
    exit 1
fi
if grep -Fq "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2 generation=4" "${serial_log}"; then
    echo "desktop service was woken by a rejected configuration" >&2
    exit 1
fi
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=center step=64 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=center step=64 fps=30 frames=5" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=SUNSET.PPM" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon stopped" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon started" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=none step=255 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SWWW: clear color=1A2B3C output=*" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=0x1A2B3C" "${serial_log}"
grep -Fq "SLOPOS-SHELL: view scrolled workspace=1 offset=" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window moved kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=614 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=512 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=set-column-width changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=TERMINAL x=496 direction=move-column-right layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=TERMINAL x=16 direction=move-column-left layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-right changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-left changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-to-workspace changed=true workspace=3 name=<empty> focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-to-workspace changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=index value=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=move-column-to-workspace kind=index value=3" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=name value=config" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=name value=main" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=move-column-to-workspace kind=name value=config" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=move-column-to-workspace kind=name value=main" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-previous changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-previous changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: pointer resized kind=TERMINAL width=608 delta=96 gesture=mod-right-drag" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: pointer resized kind=TERMINAL width=512 delta=-96 gesture=mod-right-drag" "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: workspace clicked index=2 name=config changed=true module=niri/workspaces" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: workspace clicked index=1 name=main changed=true module=niri/workspaces" \
    "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=close-window changed=true workspace=1 name=main" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-down changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=SYSTEM" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window closed kind=CONFIG" "${serial_log}"
test -s "${repo_dir}/evidence/terminal-status.ppm"
test -s "${repo_dir}/evidence/wallpaper-switched.ppm"
test -s "${repo_dir}/evidence/wallpaper-cleared.ppm"
test -s "${repo_dir}/evidence/window-moved.ppm"
test -s "${repo_dir}/evidence/window-resized.ppm"
test -s "${repo_dir}/evidence/column-reordered.ppm"
test -s "${repo_dir}/evidence/mouse-resized.ppm"
test -s "${repo_dir}/evidence/niri-workspace-number.ppm"
test -s "${repo_dir}/evidence/niri-move-workspace-number.ppm"
test -s "${repo_dir}/evidence/niri-workspace-name.ppm"
test -s "${repo_dir}/evidence/niri-move-workspace-name.ppm"
test -s "${repo_dir}/evidence/niri-workspace-previous.ppm"
test -s "${repo_dir}/evidence/waybar-workspace-click.ppm"
test -s "${repo_dir}/evidence/workspace-config.ppm"
test -s "${repo_dir}/evidence/wallpaper-only.ppm"
if grep -Fq "FATAL" "${serial_log}" || grep -Fq "state=exited" "${serial_log}"; then
    echo "persistent userspace reached an unexpected exit or fatal path" >&2
    exit 1
fi
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${repo_dir}/evidence/terminal-status.ppm" \
        >"${repo_dir}/evidence/terminal-status.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-switched.ppm" \
        >"${repo_dir}/evidence/wallpaper-switched.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-cleared.ppm" \
        >"${repo_dir}/evidence/wallpaper-cleared.png"
    pnmtopng "${repo_dir}/evidence/window-moved.ppm" \
        >"${repo_dir}/evidence/window-moved.png"
    pnmtopng "${repo_dir}/evidence/window-resized.ppm" \
        >"${repo_dir}/evidence/window-resized.png"
    pnmtopng "${repo_dir}/evidence/column-reordered.ppm" \
        >"${repo_dir}/evidence/column-reordered.png"
    pnmtopng "${repo_dir}/evidence/mouse-resized.ppm" \
        >"${repo_dir}/evidence/mouse-resized.png"
    pnmtopng "${repo_dir}/evidence/niri-workspace-number.ppm" \
        >"${repo_dir}/evidence/niri-workspace-number.png"
    pnmtopng "${repo_dir}/evidence/niri-move-workspace-number.ppm" \
        >"${repo_dir}/evidence/niri-move-workspace-number.png"
    pnmtopng "${repo_dir}/evidence/niri-workspace-name.ppm" \
        >"${repo_dir}/evidence/niri-workspace-name.png"
    pnmtopng "${repo_dir}/evidence/niri-move-workspace-name.ppm" \
        >"${repo_dir}/evidence/niri-move-workspace-name.png"
    pnmtopng "${repo_dir}/evidence/niri-workspace-previous.ppm" \
        >"${repo_dir}/evidence/niri-workspace-previous.png"
    pnmtopng "${repo_dir}/evidence/waybar-workspace-click.ppm" \
        >"${repo_dir}/evidence/waybar-workspace-click.png"
    pnmtopng "${repo_dir}/evidence/workspace-config.ppm" \
        >"${repo_dir}/evidence/workspace-config.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-only.ppm" \
        >"${repo_dir}/evidence/wallpaper-only.png"
fi
echo "SlopOS VFS config reload/rollback, swww, niri workspace/bind/rule, Waybar workspace click, viewport, keyboard/pointer resize, reorder, and close verified"
