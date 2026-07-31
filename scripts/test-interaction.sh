#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

if ! command -v socat >/dev/null 2>&1; then
    echo "test-interaction requires socat for deterministic QMP modifier+wheel input" >&2
    exit 1
fi

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/interaction-serial.log"
debugcon_log="${repo_dir}/evidence/interaction-uefi-debugcon.log"
runtime_dir="$(mktemp -d /tmp/slopos-interaction.XXXXXX)"
runtime_image="${runtime_dir}/slopos-esp.img"
runtime_root_image="${runtime_dir}/slopos-root.ext4"
ovmf_vars="${runtime_dir}/OVMF_VARS_4M.fd"
qmp_socket="${runtime_dir}/qmp.sock"

cleanup() {
    unlink "${runtime_image}" "${runtime_root_image}" "${ovmf_vars}" "${qmp_socket}" \
        2>/dev/null || true
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
            ",") echo "sendkey comma 10" ;;
            "/") echo "sendkey slash 10" ;;
            *) echo "sendkey ${character} 10" ;;
        esac
    done
    echo "sendkey ret 10"
}

qmp_wheel() {
    local modifiers="$1"
    local direction="$2"
    local press
    local release
    local button

    case "${modifiers}" in
        mod)
            press='{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"meta_l"}}}'
            release='{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"meta_l"}}}'
            ;;
        mod-shift)
            press='{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"meta_l"}}},{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"shift"}}}'
            release='{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"shift"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"meta_l"}}}'
            ;;
        mod-ctrl)
            press='{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"meta_l"}}},{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"ctrl"}}}'
            release='{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"ctrl"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"meta_l"}}}'
            ;;
        mod-ctrl-shift)
            press='{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"meta_l"}}},{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"ctrl"}}},{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"shift"}}}'
            release='{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"shift"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"ctrl"}}},{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"meta_l"}}}'
            ;;
        *) return 2 ;;
    esac
    # QEMU's PS/2 IntelliMouse packet sign is opposite the QMP wheel-button name.
    case "${direction}" in
        down) button="wheel-up" ;;
        up) button="wheel-down" ;;
        *) return 2 ;;
    esac

    {
        printf '%s\n' '{"execute":"qmp_capabilities"}'
        printf '%s\n' \
            "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[${press}]}}"
        sleep 0.3
        printf '%s\n' \
            "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[{\"type\":\"btn\",\"data\":{\"down\":true,\"button\":\"${button}\"}},{\"type\":\"btn\",\"data\":{\"down\":false,\"button\":\"${button}\"}}]}}"
        sleep 0.3
        printf '%s\n' \
            "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[${release}]}}"
    } | socat - "UNIX-CONNECT:${qmp_socket}" >/dev/null
}

qmp_wheel_burst() {
    local direction="$1"
    local button

    # QEMU's PS/2 IntelliMouse packet sign is opposite the QMP wheel-button name.
    case "${direction}" in
        down) button="wheel-up" ;;
        up) button="wheel-down" ;;
        *) return 2 ;;
    esac

    {
        printf '%s\n' '{"execute":"qmp_capabilities"}'
        printf '%s\n' \
            '{"execute":"input-send-event","arguments":{"events":[{"type":"key","data":{"down":true,"key":{"type":"qcode","data":"meta_l"}}}]}}'
        sleep 0.03
        printf '%s\n' \
            "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[{\"type\":\"btn\",\"data\":{\"down\":true,\"button\":\"${button}\"}},{\"type\":\"btn\",\"data\":{\"down\":false,\"button\":\"${button}\"}}]}}"
        printf '%s\n' \
            "{\"execute\":\"input-send-event\",\"arguments\":{\"events\":[{\"type\":\"btn\",\"data\":{\"down\":true,\"button\":\"${button}\"}},{\"type\":\"btn\",\"data\":{\"down\":false,\"button\":\"${button}\"}}]}}"
        sleep 0.03
        printf '%s\n' \
            '{"execute":"input-send-event","arguments":{"events":[{"type":"key","data":{"down":false,"key":{"type":"qcode","data":"meta_l"}}}]}}'
    } | socat - "UNIX-CONNECT:${qmp_socket}" >/dev/null
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
    monitor_type "swww img /usr/share/slopos/vfs-wallpaper.png --transition-type center --transition-step 64"
    sleep 8
    echo "screendump ${repo_dir}/evidence/wallpaper-vfs-loaded.ppm"
    monitor_type "swww query"
    sleep 2
    monitor_type "swww img /usr/share/slopos/missing.ppm --transition-type none"
    sleep 4
    monitor_type "swww query"
    sleep 2
    monitor_type "swww img /etc/slopos/system.conf --transition-type none"
    sleep 4
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
    monitor_type "swww img aurora.ppm --transition-type grow --transition-pos top-left --transition-step 64"
    sleep 8
    echo "screendump ${repo_dir}/evidence/wallpaper-grow-top-left.ppm"
    monitor_type "swww img sunset.ppm --transition-type wipe --transition-angle 30 --transition-step 64"
    sleep 8
    echo "screendump ${repo_dir}/evidence/wallpaper-wipe-angle.ppm"
    monitor_type "img aurora.ppm -t wipe --transition-duration .1"
    sleep 4
    monitor_type "img sunset.ppm -t fade --transition-bezier 0,0,1,0"
    sleep 8
    monitor_type "img aurora.ppm -t wave --transition-wave 40,24"
    sleep 8
    echo "screendump ${repo_dir}/evidence/wallpaper-wave.ppm"
    monitor_type "img aurora.ppm -t none --resize fit --fill-color 123456"
    sleep 3
    echo "screendump ${repo_dir}/evidence/wallpaper-fit-fill.ppm"
    sleep 1
    monitor_type "img aurora.ppm -t none --resize crop --crop-gravity right"
    sleep 3
    echo "screendump ${repo_dir}/evidence/wallpaper-crop-right.ppm"
    sleep 1
    monitor_type "img sunset.ppm -t none --resize stretch"
    sleep 3
    echo "screendump ${repo_dir}/evidence/wallpaper-stretched.ppm"
    sleep 1
    monitor_type "img aurora.ppm -t none --resize stretch -f bilinear"
    sleep 8
    echo "screendump ${repo_dir}/evidence/wallpaper-bilinear.ppm"
    sleep 1
    monitor_type "clear"
    sleep 20
    monitor_type "img aurora.ppm -t none --resize stretch -f catmullrom"
    sleep 12
    echo "screendump ${repo_dir}/evidence/wallpaper-catmullrom.ppm"
    sleep 1
    monitor_type "clear"
    sleep 20
    monitor_type "img aurora.ppm -t none --resize stretch -f lanczos3"
    sleep 20
    echo "screendump ${repo_dir}/evidence/wallpaper-lanczos3.ppm"
    sleep 1
    monitor_type "clear"
    sleep 30
    monitor_type "img sunset.ppm -t none"
    sleep 45
    echo "sendkey meta_l-comma 50"
    sleep 1
    echo "sendkey meta_l-ctrl-shift-2 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-workspace-target.ppm"
    echo "sendkey meta_l-alt-shift-m 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-workspace-returned.ppm"
    echo "sendkey meta_l-left 50"
    echo "sendkey meta_l-comma 50"
    sleep 1
    echo "sendkey meta_l-ctrl-2 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-workspace-target.ppm"
    echo "sendkey meta_l-ctrl-alt-m 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-workspace-returned.ppm"
    sleep 1
    echo "sendkey meta_l-ctrl-shift-2 50"
    sleep 1
    echo "sendkey meta_l-alt-shift-m 50"
    sleep 1
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-end 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-focus-column-last.ppm"
    echo "sendkey meta_l-ctrl-home 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-moved-first.ppm"
    echo "sendkey meta_l-ctrl-end 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-moved-last.ppm"
    echo "sendkey meta_l-home 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-focus-column-first.ppm"
    echo "sendkey meta_l-shift-pgdn 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-moved-down.ppm"
    echo "sendkey meta_l-alt-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-reordered-name.ppm"
    echo "sendkey meta_l-alt-m 50"
    sleep 1
    echo "sendkey meta_l-shift-pgup 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-moved-up.ppm"
    qmp_wheel_burst down
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-wheel-cooldown.ppm"
    qmp_wheel mod up
    sleep 1
    qmp_wheel mod down
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-wheel-workspace-down.ppm"
    qmp_wheel mod up
    sleep 1
    qmp_wheel mod-shift down
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-wheel-column-focus-right.ppm"
    qmp_wheel mod-shift up
    sleep 1
    qmp_wheel mod-shift down
    sleep 1
    qmp_wheel mod-ctrl down
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-wheel-column-workspace-down.ppm"
    qmp_wheel mod-ctrl up
    sleep 1
    qmp_wheel mod-shift up
    sleep 1
    qmp_wheel mod-ctrl-shift down
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-wheel-column-moved-right.ppm"
    qmp_wheel mod-ctrl-shift up
    sleep 1
    echo "sendkey meta_l-shift-f 50"
    sleep 1
    echo "mouse_move -441 -364"
    echo "mouse_button 1"
    echo "mouse_button 0"
    echo "mouse_move 441 364"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-tiled-fullscreen.ppm"
    echo "sendkey meta_l-shift-f 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-tiled-fullscreen-restored.ppm"
    echo "sendkey meta_l-alt-v 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-explicit-floating.ppm"
    echo "sendkey meta_l-shift-f 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-floating-fullscreen.ppm"
    echo "sendkey meta_l-shift-f 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-floating-fullscreen-restored.ppm"
    echo "sendkey meta_l-alt-t 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-explicit-focus-tiling.ppm"
    echo "sendkey meta_l-alt-g 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-explicit-focus-floating.ppm"
    echo "sendkey meta_l-ctrl-v 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-explicit-tiling.ppm"
    echo "sendkey meta_l-shift-left 50"
    sleep 1
    echo "sendkey meta_l-v 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-floating.ppm"
    echo "sendkey meta_l-shift-v 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-floating-focus-tiling.ppm"
    echo "sendkey meta_l-shift-v 50"
    sleep 1
    echo "sendkey meta_l-ctrl-j 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-floating-window-moved.ppm"
    echo "sendkey meta_l-v 50"
    sleep 1
    echo "sendkey meta_l-shift-left 50"
    sleep 1
    echo "sendkey meta_l-comma 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-stacked.ppm"
    echo "sendkey meta_l-w 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-tabbed-system.ppm"
    echo "sendkey meta_l-k 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-tabbed-terminal.ppm"
    echo "sendkey meta_l-j 50"
    sleep 1
    echo "sendkey meta_l-w 50"
    sleep 1
    echo "sendkey meta_l-shift-equal 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-height-increased.ppm"
    echo "sendkey meta_l-ctrl-r 50"
    sleep 1
    echo "sendkey meta_l-ctrl-shift-r 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-preset-window-height.ppm"
    echo "sendkey meta_l-ctrl-shift-r 50"
    sleep 1
    echo "sendkey meta_l-ctrl-shift-r 50"
    sleep 1
    echo "sendkey meta_l-shift-minus 50"
    sleep 1
    echo "sendkey meta_l-shift-equal 50"
    sleep 1
    echo "sendkey meta_l-ctrl-k 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-moved-up.ppm"
    echo "sendkey meta_l-ctrl-j 50"
    sleep 1
    echo "sendkey meta_l-k 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-focus-up.ppm"
    echo "sendkey meta_l-j 50"
    sleep 1
    echo "sendkey meta_l-dot 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-expelled.ppm"
    echo "sendkey meta_l-right 50"
    sleep 1
    echo "sendkey meta_l-bracket_left 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-consume-or-expel-left-stacked.ppm"
    echo "sendkey meta_l-bracket_left 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-consume-or-expel-left-expelled.ppm"
    echo "sendkey meta_l-bracket_right 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-consume-or-expel-right-stacked.ppm"
    echo "sendkey meta_l-bracket_right 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-consume-or-expel-right-expelled.ppm"
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-centered.ppm"
    echo "sendkey meta_l-right 50"
    sleep 1
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-f 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-maximized.ppm"
    echo "sendkey meta_l-f 50"
    sleep 1
    echo "sendkey meta_l-r 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-preset-column-width.ppm"
    echo "sendkey meta_l-shift-r 50"
    sleep 1
    echo "sendkey meta_l-right 50"
    sleep 1
    echo "sendkey meta_l-shift-r 50"
    sleep 1
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-ctrl-f 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-column-expanded.ppm"
    echo "sendkey meta_l-shift-r 50"
    sleep 1
    echo "sendkey meta_l-shift-r 50"
    sleep 1
    echo "sendkey meta_l-right 50"
    sleep 1
    echo "sendkey meta_l-r 50"
    sleep 1
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-shift-r 50"
    sleep 1
    echo "sendkey meta_l-right 50"
    sleep 1
    echo "sendkey meta_l-shift-r 50"
    sleep 1
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-ctrl-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-visible-columns-centered.ppm"
    echo "sendkey meta_l-r 50"
    sleep 1
    echo "sendkey meta_l-right 50"
    sleep 1
    echo "sendkey meta_l-r 50"
    sleep 1
    echo "sendkey meta_l-left 50"
    sleep 1
    echo "sendkey meta_l-m 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-window-maximized-to-edges.ppm"
    echo "sendkey meta_l-m 50"
    sleep 1
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
    echo "sendkey meta_l-alt-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-workspace-name.ppm"
    echo "sendkey meta_l-alt-m 50"
    sleep 1
    echo "sendkey meta_l-ctrl-alt-c 50"
    sleep 1
    echo "screendump ${repo_dir}/evidence/niri-move-workspace-name.ppm"
    echo "sendkey meta_l-ctrl-alt-m 50"
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
    -qmp "unix:${qmp_socket},server=on,wait=off" \
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
grep -Fq "SLOPOS-SWWW: transition complete type=center step=64 fps=30 duration_ms=2000 sampled_step=16 frames=17" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=SUNSET.PPM" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: load requested generation=1 request=/USR/SHARE/SLOPOS/VFS-WALLPAPER.PNG output=* transition=center step=64 fps=30 async=true" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: load published generation=1 request=/USR/SHARE/SLOPOS/VFS-WALLPAPER.PNG resolved=/usr/share/slopos/vfs-wallpaper.png inode=30 bytes=6144 blocks=2 format=PNG async=true" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=/USR/SHARE/SLOPOS/VFS-WALLPAPER.PNG resolved=/usr/share/slopos/vfs-wallpaper.png source=vfs output=* transition=center step=64 fps=30 format=PNG" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: result acknowledged generation=1 renderer=desktop active_image=true" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=/USR/SHARE/SLOPOS/VFS-WALLPAPER.PNG" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: load rejected generation=2 request=/USR/SHARE/SLOPOS/MISSING.PPM resolved=/usr/share/slopos/missing.ppm error=not-found retained=previous" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=/USR/SHARE/SLOPOS/MISSING.PPM resolved=/usr/share/slopos/missing.ppm source=vfs applied=false error=not-found" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: result acknowledged generation=2 renderer=desktop active_image=false" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: load requested generation=3 request=/ETC/SLOPOS/SYSTEM.CONF output=* transition=none step=255 fps=30 async=true" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: load rejected generation=3 request=/ETC/SLOPOS/SYSTEM.CONF resolved=/etc/slopos/system.conf error=invalid-ppm retained=previous" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=/ETC/SLOPOS/SYSTEM.CONF resolved=/etc/slopos/system.conf source=vfs applied=false error=invalid-ppm" "${serial_log}"
grep -Fq "SLOPOS-SWWW-VFS: result acknowledged generation=3 renderer=desktop active_image=false" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=/USR/SHARE/SLOPOS/VFS-WALLPAPER.PNG" "${serial_log}")" -ne 3 ]]; then
    echo "rejected swww VFS image did not retain the previous wallpaper" >&2
    exit 1
fi
grep -Fq "SLOPOS-SWWW: daemon stopped" "${serial_log}"
grep -Fq "SLOPOS-SWWW: daemon started" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=none step=255 fps=30" "${serial_log}"
grep -Fq "SLOPOS-SWWW: clear color=1A2B3C output=*" "${serial_log}"
grep -Fq "SLOPOS-SWWW: query output=SLOPOS-1 geometry=1024x768 image=0x1A2B3C" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=AURORA.PPM output=* transition=grow step=64 fps=30 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=grow step=64 fps=30 duration_ms=2000 sampled_step=16 frames=17 angle=45 position=0,0 invert_y=false" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=wipe step=64 fps=30 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=wipe step=64 fps=30 duration_ms=2000 sampled_step=16 frames=17 angle=30 position=512,384 invert_y=false" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=AURORA.PPM output=* transition=wipe step=90 fps=30 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=wipe step=90 fps=30 duration_ms=100 sampled_step=85 frames=4 angle=45 position=512,384 invert_y=false" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=SUNSET.PPM output=* transition=fade step=2 fps=30 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=fade step=2 fps=30 duration_ms=2000 sampled_step=16 frames=17 angle=45 position=512,384 invert_y=false bezier=0,0,10000,0 midpoint=32" "${serial_log}"
grep -Fq "SLOPOS-SWWW: image=AURORA.PPM output=* transition=wave step=90 fps=30 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: transition complete type=wave step=90 fps=30 duration_ms=2000 sampled_step=16 frames=17 angle=45 position=512,384 invert_y=false bezier=5400,0,3400,9900 midpoint=155 wave=400000,240000" "${serial_log}"
grep -Fq "SLOPOS-SWWW: geometry resize=fit x=0 y=43 width=1024 height=682 crop_gravity=center fill=123456 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: geometry resize=crop x=-128 y=0 width=1152 height=768 crop_gravity=right fill=000000 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: geometry resize=stretch x=0 y=0 width=1024 height=768 crop_gravity=center fill=000000 source=embedded" "${serial_log}"
grep -Fq "SLOPOS-SWWW: geometry resize=stretch x=0 y=0 width=1024 height=768 crop_gravity=center fill=000000 source=embedded filter=Bilinear" "${serial_log}"
grep -Fq "SLOPOS-SWWW: geometry resize=stretch x=0 y=0 width=1024 height=768 crop_gravity=center fill=000000 source=embedded filter=CatmullRom" "${serial_log}"
grep -Fq "SLOPOS-SWWW: geometry resize=stretch x=0 y=0 width=1024 height=768 crop_gravity=center fill=000000 source=embedded filter=Lanczos3" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=window action=move-window-to-workspace member=SYSTEM workspace=2 name=config x=520 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=window action=move-window-to-workspace member=CONFIG workspace=2 name=config x=16 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=window action=move-window-to-workspace member=TERMINAL workspace=1 name=main x=16 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=window action=move-window-to-workspace member=SYSTEM workspace=1 name=main x=520 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace member=TERMINAL workspace=2 name=config x=520 y=56 width=488 height=340 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace member=SYSTEM workspace=2 name=config x=520 y=412 width=488 height=340 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace member=CONFIG workspace=2 name=config x=16 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace member=TERMINAL workspace=1 name=main x=268 y=56 width=488 height=340 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace member=SYSTEM workspace=1 name=main x=268 y=412 width=488 height=340 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-column-last changed=true workspace=1 name=main focused=1" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=SYSTEM x=16 direction=move-column-to-first layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=SYSTEM x=520 direction=move-column-to-last layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-column-first changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace reordered action=move-workspace-down workspace=2 name=main previous=1 focused=0 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-workspace-down changed=true workspace=2 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace changed=true workspace=1 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=name value=config" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace changed=true workspace=2 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=name value=main" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace reordered action=move-workspace-up workspace=1 name=main previous=2 focused=0 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-workspace-up changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=down modifiers=0x1 action=focus-workspace-down source=ps2-intellimouse accepted=false cooldown_ms=150" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: wheel binding direction=down modifiers=0x1 action=focus-workspace-down source=ps2-intellimouse accepted=true cooldown_ms=150" "${serial_log}")" -ne 2 ]]; then
    echo "niri cooldown did not accept exactly one event from the two-packet burst" >&2
    exit 1
fi
if [[ "$(grep -Fc "source=ps2-intellimouse accepted=false cooldown_ms=150" "${serial_log}")" -ne 1 ]]; then
    echo "niri cooldown suppressed an unexpected number of wheel events" >&2
    exit 1
fi
grep -Fq "SLOPOS-NIRI: wheel binding direction=down modifiers=0x1 action=focus-workspace-down source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=up modifiers=0x1 action=focus-workspace-up source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=down modifiers=0x5 action=focus-column-right source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=up modifiers=0x5 action=focus-column-left source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=down modifiers=0x3 action=move-column-to-workspace-down source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=up modifiers=0x3 action=move-column-to-workspace-up source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=down modifiers=0x7 action=move-column-right source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: wheel binding direction=up modifiers=0x7 action=move-column-left source=ps2-intellimouse" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-down changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-up changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace-down member=SYSTEM workspace=2 name=config x=520 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: workspace transfer scope=column action=move-column-to-workspace-up member=SYSTEM workspace=1 name=main x=520 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=TERMINAL x=520 direction=move-column-right layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=TERMINAL x=16 direction=move-column-left layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: fullscreen toggled state=active kind=TERMINAL restore_layer=tiling x=0 y=0 width=1024 height=768 bar=covered layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: fullscreen toggled state=inactive kind=TERMINAL restore_layer=tiling x=16 y=56 width=488 height=696 bar=visible layout=niri" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-window-to-floating changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window layer moved action=move-window-to-floating kind=TERMINAL layer=floating x=16 y=161 width=488 height=485 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: fullscreen toggled state=active kind=TERMINAL restore_layer=floating x=0 y=0 width=1024 height=768 bar=covered layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: fullscreen toggled state=inactive kind=TERMINAL restore_layer=floating x=16 y=161 width=488 height=485 bar=visible layout=niri" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=fullscreen-window changed=true workspace=1 name=main focused=0" "${serial_log}")" -ne 4 ]]; then
    echo "niri tiled/floating fullscreen did not toggle and restore" >&2
    exit 1
fi
if [[ "$(grep -Fc "SLOPOS-WAYBAR: workspace clicked" "${serial_log}")" -ne 2 ]]; then
    echo "covered Waybar accepted pointer input during fullscreen" >&2
    exit 1
fi
grep -Fq "SLOPOS-NIRI: binding action=focus-tiling changed=true workspace=1 name=main focused=1" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: layer focus switched layer=tiling kind=SYSTEM layout=niri action=focus-tiling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-floating changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: layer focus switched layer=floating kind=TERMINAL layout=niri action=focus-floating" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-window-to-tiling changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window layer moved action=move-window-to-tiling kind=TERMINAL layer=tiling x=520 y=56 width=488 height=696 layout=niri" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=toggle-window-floating changed=true workspace=1 name=main focused=0" "${serial_log}")" -ne 2 ]]; then
    echo "niri floating window did not toggle and restore" >&2
    exit 1
fi
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=switch-focus-between-floating-and-tiling changed=true workspace=1 name=main" "${serial_log}")" -ne 2 ]]; then
    echo "niri floating/tiling focus did not switch in both directions" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: window layer toggled kind=TERMINAL layer=floating x=16 y=161 width=488 height=485 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: layer focus switched layer=tiling kind=SYSTEM layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: layer focus switched layer=floating kind=TERMINAL layout=niri" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: floating window moved kind=TERMINAL x=16 y=211 direction=move-window-down layout=floating" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window layer toggled kind=TERMINAL layer=tiling x=520 y=56 width=488 height=696 layout=niri" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=consume-window-into-column changed=true workspace=1 name=main focused=1" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=toggle-column-tabbed-display changed=true workspace=1 name=main focused=1" "${serial_log}")" -ne 2 ]]; then
    echo "niri tabbed column display did not toggle and restore" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: column display toggled mode=tabbed kind=SYSTEM tab=2/2 x=268 y=56 width=488 height=696 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: tab focused kind=TERMINAL tab=1/2 direction=focus-window-up layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: tab focused kind=SYSTEM tab=2/2 direction=focus-window-down layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column display toggled mode=normal kind=SYSTEM x=268 y=412 width=488 height=340 layout=scrolling" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=set-window-height changed=true workspace=1 name=main focused=1" "${serial_log}")" -ne 3 ]]; then
    echo "niri set-window-height bindings did not cover grow, shrink, and restore" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: window height changed kind=SYSTEM height=411 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=reset-window-height changed=true workspace=1 name=main focused=1" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window height changed kind=SYSTEM height=340 layout=scrolling" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=switch-preset-window-height changed=true workspace=1 name=main focused=1" "${serial_log}")" -ne 3 ]]; then
    echo "niri switch-preset-window-height did not cycle 50% -> 66.7% -> 33.3% -> 50%" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: window height changed kind=SYSTEM height=458 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window height changed kind=SYSTEM height=221 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window height changed kind=SYSTEM height=269 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-window-up changed=true workspace=1 name=main focused=1" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-window-down changed=true workspace=1 name=main focused=1" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-window-up changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-window-down changed=true workspace=1 name=main focused=1" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=expel-window-from-column changed=true workspace=1 name=main focused=0" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=consume-or-expel-window-left changed=true workspace=1 name=main focused=1" "${serial_log}")" -ne 2 ]]; then
    echo "niri consume-or-expel-window-left did not consume and expel the focused window" >&2
    exit 1
fi
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=consume-or-expel-window-right changed=true workspace=1 name=main focused=1" "${serial_log}")" -ne 2 ]]; then
    echo "niri consume-or-expel-window-right did not consume and expel the focused window" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: window consume-or-expel kind=SYSTEM direction=left x=268 y=412 width=488 height=340 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window consume-or-expel kind=SYSTEM direction=left x=268 y=56 width=488 height=696 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window consume-or-expel kind=SYSTEM direction=right x=268 y=412 width=488 height=340 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window consume-or-expel kind=SYSTEM direction=right x=520 y=56 width=488 height=696 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=center-column changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column centered kind=TERMINAL x=268 offset=-252 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=expand-column-to-available-width changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=657 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=center-visible-columns changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: visible columns centered kind=TERMINAL x=185 offset=-169 layout=scrolling" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=maximize-window-to-edges changed=true workspace=1 name=main focused=0" "${serial_log}")" -ne 2 ]]; then
    echo "niri maximize-window-to-edges did not toggle and restore" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: window edge maximize toggled kind=TERMINAL x=0 y=40 width=1024 height=728 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window edge maximize toggled kind=TERMINAL x=16 y=56 width=488 height=696 layout=scrolling" "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=maximize-column changed=true workspace=1 name=main focused=0" "${serial_log}")" -ne 2 ]]; then
    echo "niri maximize-column did not toggle full width and restore" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=992 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=488 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=switch-preset-column-width changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=switch-preset-column-width-back changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=656 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=488 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-SHELL: view scrolled workspace=1 offset=" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window moved kind=TERMINAL" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=588 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: window resized kind=TERMINAL width=488 layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=set-column-width changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=TERMINAL x=520 direction=move-column-right layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: column reordered kind=TERMINAL x=16 direction=move-column-left layout=scrolling" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-right changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-left changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-to-workspace changed=true workspace=3 name=<empty> focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=move-column-to-workspace changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=index value=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=move-column-to-workspace kind=index value=3" "${serial_log}"
grep -Fq "SLOPOS-NIRI: dynamic workspaces reason=move-column count=3->4 named=2 active=3 trailing_empty=true" "${serial_log}"
grep -Fq "SLOPOS-NIRI: dynamic workspaces reason=move-column count=4->3 named=2 active=1 trailing_empty=true" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=name value=config" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=focus-workspace kind=name value=main" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=move-column-to-workspace kind=name value=config" "${serial_log}"
grep -Fq "SLOPOS-NIRI: workspace target action=move-column-to-workspace kind=name value=main" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-previous changed=true workspace=2 name=config focused=2" "${serial_log}"
grep -Fq "SLOPOS-NIRI: binding action=focus-workspace-previous changed=true workspace=1 name=main focused=0" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: pointer resized kind=TERMINAL width=584 delta=96 gesture=mod-right-drag" "${serial_log}"
grep -Fq "SLOPOS-DESKTOP: pointer resized kind=TERMINAL width=488 delta=-96 gesture=mod-right-drag" "${serial_log}"
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
test -s "${repo_dir}/evidence/wallpaper-vfs-loaded.ppm"
test -s "${repo_dir}/evidence/wallpaper-cleared.ppm"
test -s "${repo_dir}/evidence/wallpaper-grow-top-left.ppm"
test -s "${repo_dir}/evidence/wallpaper-wipe-angle.ppm"
test -s "${repo_dir}/evidence/wallpaper-wave.ppm"
test -s "${repo_dir}/evidence/wallpaper-fit-fill.ppm"
test -s "${repo_dir}/evidence/wallpaper-crop-right.ppm"
test -s "${repo_dir}/evidence/wallpaper-stretched.ppm"
test -s "${repo_dir}/evidence/wallpaper-bilinear.ppm"
test -s "${repo_dir}/evidence/wallpaper-catmullrom.ppm"
test -s "${repo_dir}/evidence/wallpaper-lanczos3.ppm"
test -s "${repo_dir}/evidence/niri-window-workspace-target.ppm"
test -s "${repo_dir}/evidence/niri-window-workspace-returned.ppm"
test -s "${repo_dir}/evidence/niri-column-workspace-target.ppm"
test -s "${repo_dir}/evidence/niri-column-workspace-returned.ppm"
test -s "${repo_dir}/evidence/niri-focus-column-last.ppm"
test -s "${repo_dir}/evidence/niri-column-moved-first.ppm"
test -s "${repo_dir}/evidence/niri-column-moved-last.ppm"
test -s "${repo_dir}/evidence/niri-focus-column-first.ppm"
test -s "${repo_dir}/evidence/niri-workspace-moved-down.ppm"
test -s "${repo_dir}/evidence/niri-workspace-reordered-name.ppm"
test -s "${repo_dir}/evidence/niri-workspace-moved-up.ppm"
test -s "${repo_dir}/evidence/niri-wheel-cooldown.ppm"
test -s "${repo_dir}/evidence/niri-wheel-workspace-down.ppm"
test -s "${repo_dir}/evidence/niri-wheel-column-focus-right.ppm"
test -s "${repo_dir}/evidence/niri-wheel-column-workspace-down.ppm"
test -s "${repo_dir}/evidence/niri-wheel-column-moved-right.ppm"
test -s "${repo_dir}/evidence/niri-tiled-fullscreen.ppm"
test -s "${repo_dir}/evidence/niri-tiled-fullscreen-restored.ppm"
test -s "${repo_dir}/evidence/niri-floating-fullscreen.ppm"
test -s "${repo_dir}/evidence/niri-floating-fullscreen-restored.ppm"
test -s "${repo_dir}/evidence/niri-explicit-floating.ppm"
test -s "${repo_dir}/evidence/niri-explicit-focus-tiling.ppm"
test -s "${repo_dir}/evidence/niri-explicit-focus-floating.ppm"
test -s "${repo_dir}/evidence/niri-explicit-tiling.ppm"
test -s "${repo_dir}/evidence/niri-window-floating.ppm"
test -s "${repo_dir}/evidence/niri-floating-focus-tiling.ppm"
test -s "${repo_dir}/evidence/niri-floating-window-moved.ppm"
test -s "${repo_dir}/evidence/niri-column-stacked.ppm"
test -s "${repo_dir}/evidence/niri-column-tabbed-system.ppm"
test -s "${repo_dir}/evidence/niri-column-tabbed-terminal.ppm"
test -s "${repo_dir}/evidence/niri-window-height-increased.ppm"
test -s "${repo_dir}/evidence/niri-preset-window-height.ppm"
test -s "${repo_dir}/evidence/niri-window-moved-up.ppm"
test -s "${repo_dir}/evidence/niri-window-focus-up.ppm"
test -s "${repo_dir}/evidence/niri-column-expelled.ppm"
test -s "${repo_dir}/evidence/niri-consume-or-expel-left-stacked.ppm"
test -s "${repo_dir}/evidence/niri-consume-or-expel-left-expelled.ppm"
test -s "${repo_dir}/evidence/niri-consume-or-expel-right-stacked.ppm"
test -s "${repo_dir}/evidence/niri-consume-or-expel-right-expelled.ppm"
test -s "${repo_dir}/evidence/niri-column-centered.ppm"
test -s "${repo_dir}/evidence/niri-column-maximized.ppm"
test -s "${repo_dir}/evidence/niri-preset-column-width.ppm"
test -s "${repo_dir}/evidence/niri-column-expanded.ppm"
test -s "${repo_dir}/evidence/niri-visible-columns-centered.ppm"
test -s "${repo_dir}/evidence/niri-window-maximized-to-edges.ppm"
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

ppm_pixel_hex() {
    local image="$1"
    local x="$2"
    local y="$3"
    local offset=$((16 + 3 * (y * 1024 + x)))
    dd if="${image}" bs=1 skip="${offset}" count=3 status=none \
        | od -An -tx1 \
        | tr -d ' \n'
}

if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/wallpaper-fit-fill.ppm" 0 767)" != "123456" ]]; then
    echo "swww fit did not pad the output with --fill-color" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/wallpaper-crop-right.ppm" 512 100)" != "442299" ]]; then
    echo "swww crop did not anchor the image to --crop-gravity right" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/wallpaper-stretched.ppm" 512 767)" != "221133" ]]; then
    echo "swww stretch did not map the image to the complete output" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/wallpaper-bilinear.ppm" 512 300)" != "2bc5ce" ]]; then
    echo "swww Bilinear filter did not interpolate source pixels" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/wallpaper-catmullrom.ppm" 512 300)" != "27d2d4" ]]; then
    echo "swww CatmullRom filter did not use its cubic convolution kernel" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/wallpaper-lanczos3.ppm" 512 300)" != "25d5d6" ]]; then
    echo "swww Lanczos3 filter did not use its windowed-sinc convolution kernel" >&2
    exit 1
fi
for image in niri-window-workspace-target niri-window-workspace-returned; do
    if [[ "$(ppm_pixel_hex "${repo_dir}/evidence/${image}.ppm" 10 10)" != "6558f5" ]] \
        || [[ "$(ppm_pixel_hex "${repo_dir}/evidence/${image}.ppm" 100 100)" != "171c2b" ]] \
        || [[ "$(ppm_pixel_hex "${repo_dir}/evidence/${image}.ppm" 512 700)" != "221133" ]]; then
        echo "swww filter recovery did not restore a complete niri desktop frame" >&2
        exit 1
    fi
done

if grep -Fq "FATAL" "${serial_log}" || grep -Fq "state=exited" "${serial_log}"; then
    echo "persistent userspace reached an unexpected exit or fatal path" >&2
    exit 1
fi
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${repo_dir}/evidence/terminal-status.ppm" \
        >"${repo_dir}/evidence/terminal-status.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-switched.ppm" \
        >"${repo_dir}/evidence/wallpaper-switched.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-vfs-loaded.ppm" \
        >"${repo_dir}/evidence/wallpaper-vfs-loaded.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-cleared.ppm" \
        >"${repo_dir}/evidence/wallpaper-cleared.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-grow-top-left.ppm" \
        >"${repo_dir}/evidence/wallpaper-grow-top-left.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-wipe-angle.ppm" \
        >"${repo_dir}/evidence/wallpaper-wipe-angle.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-wave.ppm" \
        >"${repo_dir}/evidence/wallpaper-wave.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-fit-fill.ppm" \
        >"${repo_dir}/evidence/wallpaper-fit-fill.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-crop-right.ppm" \
        >"${repo_dir}/evidence/wallpaper-crop-right.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-stretched.ppm" \
        >"${repo_dir}/evidence/wallpaper-stretched.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-bilinear.ppm" \
        >"${repo_dir}/evidence/wallpaper-bilinear.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-catmullrom.ppm" \
        >"${repo_dir}/evidence/wallpaper-catmullrom.png"
    pnmtopng "${repo_dir}/evidence/wallpaper-lanczos3.ppm" \
        >"${repo_dir}/evidence/wallpaper-lanczos3.png"
    pnmtopng "${repo_dir}/evidence/niri-window-workspace-target.ppm" \
        >"${repo_dir}/evidence/niri-window-workspace-target.png"
    pnmtopng "${repo_dir}/evidence/niri-window-workspace-returned.ppm" \
        >"${repo_dir}/evidence/niri-window-workspace-returned.png"
    pnmtopng "${repo_dir}/evidence/niri-column-workspace-target.ppm" \
        >"${repo_dir}/evidence/niri-column-workspace-target.png"
    pnmtopng "${repo_dir}/evidence/niri-column-workspace-returned.ppm" \
        >"${repo_dir}/evidence/niri-column-workspace-returned.png"
    pnmtopng "${repo_dir}/evidence/niri-focus-column-last.ppm" \
        >"${repo_dir}/evidence/niri-focus-column-last.png"
    pnmtopng "${repo_dir}/evidence/niri-column-moved-first.ppm" \
        >"${repo_dir}/evidence/niri-column-moved-first.png"
    pnmtopng "${repo_dir}/evidence/niri-column-moved-last.ppm" \
        >"${repo_dir}/evidence/niri-column-moved-last.png"
    pnmtopng "${repo_dir}/evidence/niri-focus-column-first.ppm" \
        >"${repo_dir}/evidence/niri-focus-column-first.png"
    pnmtopng "${repo_dir}/evidence/niri-workspace-moved-down.ppm" \
        >"${repo_dir}/evidence/niri-workspace-moved-down.png"
    pnmtopng "${repo_dir}/evidence/niri-workspace-reordered-name.ppm" \
        >"${repo_dir}/evidence/niri-workspace-reordered-name.png"
    pnmtopng "${repo_dir}/evidence/niri-workspace-moved-up.ppm" \
        >"${repo_dir}/evidence/niri-workspace-moved-up.png"
    pnmtopng "${repo_dir}/evidence/niri-wheel-cooldown.ppm" \
        >"${repo_dir}/evidence/niri-wheel-cooldown.png"
    pnmtopng "${repo_dir}/evidence/niri-wheel-workspace-down.ppm" \
        >"${repo_dir}/evidence/niri-wheel-workspace-down.png"
    pnmtopng "${repo_dir}/evidence/niri-wheel-column-focus-right.ppm" \
        >"${repo_dir}/evidence/niri-wheel-column-focus-right.png"
    pnmtopng "${repo_dir}/evidence/niri-wheel-column-workspace-down.ppm" \
        >"${repo_dir}/evidence/niri-wheel-column-workspace-down.png"
    pnmtopng "${repo_dir}/evidence/niri-wheel-column-moved-right.ppm" \
        >"${repo_dir}/evidence/niri-wheel-column-moved-right.png"
    pnmtopng "${repo_dir}/evidence/niri-tiled-fullscreen.ppm" \
        >"${repo_dir}/evidence/niri-tiled-fullscreen.png"
    pnmtopng "${repo_dir}/evidence/niri-tiled-fullscreen-restored.ppm" \
        >"${repo_dir}/evidence/niri-tiled-fullscreen-restored.png"
    pnmtopng "${repo_dir}/evidence/niri-floating-fullscreen.ppm" \
        >"${repo_dir}/evidence/niri-floating-fullscreen.png"
    pnmtopng "${repo_dir}/evidence/niri-floating-fullscreen-restored.ppm" \
        >"${repo_dir}/evidence/niri-floating-fullscreen-restored.png"
    pnmtopng "${repo_dir}/evidence/niri-explicit-floating.ppm" \
        >"${repo_dir}/evidence/niri-explicit-floating.png"
    pnmtopng "${repo_dir}/evidence/niri-explicit-focus-tiling.ppm" \
        >"${repo_dir}/evidence/niri-explicit-focus-tiling.png"
    pnmtopng "${repo_dir}/evidence/niri-explicit-focus-floating.ppm" \
        >"${repo_dir}/evidence/niri-explicit-focus-floating.png"
    pnmtopng "${repo_dir}/evidence/niri-explicit-tiling.ppm" \
        >"${repo_dir}/evidence/niri-explicit-tiling.png"
    pnmtopng "${repo_dir}/evidence/niri-window-floating.ppm" \
        >"${repo_dir}/evidence/niri-window-floating.png"
    pnmtopng "${repo_dir}/evidence/niri-floating-focus-tiling.ppm" \
        >"${repo_dir}/evidence/niri-floating-focus-tiling.png"
    pnmtopng "${repo_dir}/evidence/niri-floating-window-moved.ppm" \
        >"${repo_dir}/evidence/niri-floating-window-moved.png"
    pnmtopng "${repo_dir}/evidence/niri-column-stacked.ppm" \
        >"${repo_dir}/evidence/niri-column-stacked.png"
    pnmtopng "${repo_dir}/evidence/niri-column-tabbed-system.ppm" \
        >"${repo_dir}/evidence/niri-column-tabbed-system.png"
    pnmtopng "${repo_dir}/evidence/niri-column-tabbed-terminal.ppm" \
        >"${repo_dir}/evidence/niri-column-tabbed-terminal.png"
    pnmtopng "${repo_dir}/evidence/niri-window-height-increased.ppm" \
        >"${repo_dir}/evidence/niri-window-height-increased.png"
    pnmtopng "${repo_dir}/evidence/niri-preset-window-height.ppm" \
        >"${repo_dir}/evidence/niri-preset-window-height.png"
    pnmtopng "${repo_dir}/evidence/niri-window-moved-up.ppm" \
        >"${repo_dir}/evidence/niri-window-moved-up.png"
    pnmtopng "${repo_dir}/evidence/niri-window-focus-up.ppm" \
        >"${repo_dir}/evidence/niri-window-focus-up.png"
    pnmtopng "${repo_dir}/evidence/niri-column-expelled.ppm" \
        >"${repo_dir}/evidence/niri-column-expelled.png"
    pnmtopng "${repo_dir}/evidence/niri-consume-or-expel-left-stacked.ppm" \
        >"${repo_dir}/evidence/niri-consume-or-expel-left-stacked.png"
    pnmtopng "${repo_dir}/evidence/niri-consume-or-expel-left-expelled.ppm" \
        >"${repo_dir}/evidence/niri-consume-or-expel-left-expelled.png"
    pnmtopng "${repo_dir}/evidence/niri-consume-or-expel-right-stacked.ppm" \
        >"${repo_dir}/evidence/niri-consume-or-expel-right-stacked.png"
    pnmtopng "${repo_dir}/evidence/niri-consume-or-expel-right-expelled.ppm" \
        >"${repo_dir}/evidence/niri-consume-or-expel-right-expelled.png"
    pnmtopng "${repo_dir}/evidence/niri-column-centered.ppm" \
        >"${repo_dir}/evidence/niri-column-centered.png"
    pnmtopng "${repo_dir}/evidence/niri-column-maximized.ppm" \
        >"${repo_dir}/evidence/niri-column-maximized.png"
    pnmtopng "${repo_dir}/evidence/niri-preset-column-width.ppm" \
        >"${repo_dir}/evidence/niri-preset-column-width.png"
    pnmtopng "${repo_dir}/evidence/niri-column-expanded.ppm" \
        >"${repo_dir}/evidence/niri-column-expanded.png"
    pnmtopng "${repo_dir}/evidence/niri-visible-columns-centered.ppm" \
        >"${repo_dir}/evidence/niri-visible-columns-centered.png"
    pnmtopng "${repo_dir}/evidence/niri-window-maximized-to-edges.ppm" \
        >"${repo_dir}/evidence/niri-window-maximized-to-edges.png"
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
