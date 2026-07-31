#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
esp_image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/custom-config-serial.log"
debug_log="${repo_dir}/evidence/custom-config-uefi-debugcon.log"
qemu_log="${repo_dir}/evidence/custom-config-qemu.log"
workspace_screenshot="${repo_dir}/evidence/custom-config-workspace-click.ppm"
edge_screenshot="${repo_dir}/evidence/custom-config-edge-maximized.ppm"
column_width_screenshot="${repo_dir}/evidence/custom-config-default-column-width.ppm"
floating_position_screenshot="${repo_dir}/evidence/custom-config-floating-position.ppm"
floating_remembered_screenshot="${repo_dir}/evidence/custom-config-floating-remembered.ppm"
action_screenshot="${repo_dir}/evidence/custom-config-on-click.ppm"
format_restored_screenshot="${repo_dir}/evidence/custom-config-format-restored.ppm"
right_action_screenshot="${repo_dir}/evidence/custom-config-on-click-right.ppm"
middle_action_screenshot="${repo_dir}/evidence/custom-config-on-click-middle.ppm"
scroll_up_screenshot="${repo_dir}/evidence/custom-config-scroll-up.ppm"
scroll_down_screenshot="${repo_dir}/evidence/custom-config-scroll-down.ppm"
overlay_serial_log="${repo_dir}/evidence/waybar-overlay-serial.log"
overlay_debug_log="${repo_dir}/evidence/waybar-overlay-uefi-debugcon.log"
overlay_qemu_log="${repo_dir}/evidence/waybar-overlay-qemu.log"
overlay_screenshot="${repo_dir}/evidence/waybar-overlay-passthrough.ppm"
overlay_clicked_screenshot="${repo_dir}/evidence/waybar-overlay-click-through.ppm"
runtime_dir="$(mktemp -d /tmp/slopos-custom-config.XXXXXX)"
runtime_esp="${runtime_dir}/slopos-esp.img"
runtime_root="${runtime_dir}/slopos-root.ext4"
runtime_vars="${runtime_dir}/OVMF_VARS_4M.fd"
custom_niri="${runtime_dir}/niri.kdl"
custom_waybar="${runtime_dir}/waybar.jsonc"
overlay_waybar="${runtime_dir}/waybar-overlay.jsonc"
fsck_log="${runtime_dir}/fsck.log"
debugfs=/usr/sbin/debugfs
e2fsck=/usr/sbin/e2fsck

cleanup() {
    for temporary_file in \
        "${runtime_esp}" \
        "${runtime_root}" \
        "${runtime_vars}" \
        "${custom_niri}" \
        "${custom_waybar}" \
        "${overlay_waybar}" \
        "${fsck_log}"
    do
        unlink "${temporary_file}" 2>/dev/null || true
    done
    rmdir "${runtime_dir}" 2>/dev/null || true
}
trap cleanup EXIT

for required in \
    "${esp_image}" \
    "${root_image}" \
    /usr/share/OVMF/OVMF_CODE_4M.fd \
    /usr/share/OVMF/OVMF_VARS_4M.fd
do
    if [[ ! -f "${required}" ]]; then
        echo "missing custom-config test input: ${required}" >&2
        exit 1
    fi
done
if [[ ! -x "${debugfs}" || ! -x "${e2fsck}" ]]; then
    echo "missing e2fsprogs custom-config test tools" >&2
    exit 1
fi

mkdir -p "${repo_dir}/evidence"
cp --reflink=auto --sparse=always "${esp_image}" "${runtime_esp}"
cp --reflink=auto --sparse=always "${root_image}" "${runtime_root}"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${runtime_vars}"
sed \
    -e '1i// user' \
    -e 's/open-maximized false/open-maximized true/' \
    -e 's/open-maximized-to-edges false/open-maximized-to-edges true/' \
    -e 's/open-fullscreen false/open-fullscreen true/' \
    -e 's/open-focused false/open-focused true/' \
    -e '/match app-id="slopos-config"/a\    focus-ring { off; }' \
    -e '/match app-id="slopos-config"/a\    border { on; width 4; active-color "#ffb86c"; inactive-color "#505050"; }' \
    -e '/match app-id="slopos-config"/a\    shadow { on; softness 8; spread 2; offset x=6 y=4; draw-behind-window false; color "#000c"; inactive-color "#0006"; }' \
    -e '/match app-id="slopos-config"/a\    draw-border-with-background false' \
    -e '/match app-id="slopos-config"/a\    opacity 0.75' \
    -e '/match app-id="slopos-config"/,/^}/ s/proportion 0\.5/proportion 0.667/' \
    -e '/match app-id="slopos-config"/,/^}/ s/proportion 1\.0/proportion 0.5/' \
    -e '/match app-id="slopos-config"/,/^}/ s/default-column-display "normal"/default-column-display "tabbed"/' \
    "${repo_dir}/assets/niri-config.kdl" >"${custom_niri}"
default_niri_bytes="$(wc -c <"${repo_dir}/assets/niri-config.kdl")"
custom_niri_bytes="$(wc -c <"${custom_niri}")"
if (( custom_niri_bytes <= default_niri_bytes || custom_niri_bytes > 4096 )); then
    echo "custom niri fixture has unexpected size: ${custom_niri_bytes}" >&2
    exit 1
fi
sed \
    -e '1i// user override accepted by the SlopOS desktop service' \
    -e '/"spacing": 10,/a\    "margin": "4 12",\n    "fixed-center": false,\n    "layer": "top",\n    "exclusive": true,' \
    -e '/"modules-left":/,/]/ s/"niri\/workspaces"/"niri\/window"/' \
    -e '/"modules-center":/,/]/ s/"niri\/window"/"niri\/workspaces"/' \
    -e '/"clock": {/a\        "on-click": "status",' \
    -e '/"clock": {/a\        "format-alt": "UTC ALT",' \
    -e '/"clock": {/a\        "on-click-right": "help",' \
    -e '/"clock": {/a\        "on-click-middle": "swww query",' \
    -e '/"clock": {/a\        "on-scroll-up": "swww img /usr/share/backgrounds/slopos-sunset.ppm --transition-type none",' \
    -e '/"clock": {/a\        "on-scroll-down": "swww img /usr/share/backgrounds/slopos-aurora.ppm --transition-type none",' \
    "${repo_dir}/assets/waybar-config.jsonc" >"${custom_waybar}"
custom_bytes="$(wc -c <"${custom_waybar}")"
if (( custom_bytes <= 904 || custom_bytes > 4096 )); then
    echo "custom Waybar fixture has unexpected size: ${custom_bytes}" >&2
    exit 1
fi
sed \
    -e '1i// Waybar overlay preset and pointer passthrough integration fixture' \
    -e '/"spacing": 10,/a\    "mode": "overlay",' \
    "${repo_dir}/assets/waybar-config.jsonc" >"${overlay_waybar}"
overlay_bytes="$(wc -c <"${overlay_waybar}")"
if (( overlay_bytes <= 904 || overlay_bytes > 4096 )); then
    echo "overlay Waybar fixture has unexpected size: ${overlay_bytes}" >&2
    exit 1
fi

"${debugfs}" -w -R "rm /etc/slopos/niri.kdl" "${runtime_root}" >/dev/null 2>&1
"${debugfs}" \
    -w \
    -R "write ${custom_niri} /etc/slopos/niri.kdl" \
    "${runtime_root}" >/dev/null 2>&1
"${debugfs}" -w -R "rm /etc/slopos/waybar.jsonc" "${runtime_root}" >/dev/null 2>&1
"${debugfs}" \
    -w \
    -R "write ${custom_waybar} /etc/slopos/waybar.jsonc" \
    "${runtime_root}" >/dev/null 2>&1

set +e
{
    sleep 7
    echo "mouse_move -212 -364"
    sleep 1
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "mouse_move 217 0"
    echo "screendump ${workspace_screenshot}"
    echo "sendkey meta_l-shift-f 50"
    sleep 1
    echo "screendump ${edge_screenshot}"
    echo "sendkey meta_l-m 50"
    sleep 1
    echo "sendkey meta_l-f 50"
    sleep 1
    echo "screendump ${column_width_screenshot}"
    echo "sendkey meta_l-f 50"
    sleep 1
    echo "sendkey meta_l-alt-v 50"
    sleep 1
    echo "screendump ${floating_position_screenshot}"
    echo "sendkey meta_l-ctrl-j 50"
    sleep 1
    echo "sendkey meta_l-ctrl-v 50"
    sleep 1
    echo "sendkey meta_l-alt-v 50"
    sleep 1
    echo "screendump ${floating_remembered_screenshot}"
    echo "sendkey meta_l-ctrl-v 50"
    sleep 1
    echo "mouse_move -90 0"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "mouse_move 30 0"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "mouse_move -30 0"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "sendkey a"
    echo "sendkey b"
    echo "sendkey o"
    echo "mouse_move 557 0"
    sleep 1
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${action_screenshot}"
    echo "sendkey u"
    echo "sendkey t"
    echo "sendkey ret"
    sleep 1
    echo "mouse_button 2"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${right_action_screenshot}"
    echo "mouse_button 4"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${middle_action_screenshot}"
    echo "mouse_move 0 0 -1"
    sleep 1
    echo "screendump ${scroll_up_screenshot}"
    echo "mouse_move 0 0 1"
    sleep 1
    echo "screendump ${scroll_down_screenshot}"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${format_restored_screenshot}"
    echo "quit"
} | timeout 40s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${runtime_vars}" \
    -drive "if=virtio,format=raw,file=${runtime_esp}" \
    -drive "if=virtio,format=raw,file=${runtime_root}" \
    -serial "file:${serial_log}" \
    -debugcon "file:${debug_log}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot >"${qemu_log}" 2>&1
qemu_status=$?
set -e

if [[ ${qemu_status} -ne 0 && ${qemu_status} -ne 124 ]]; then
    echo "custom-config QEMU failed with status ${qemu_status}" >&2
    exit "${qemu_status}"
fi
sed -i 's/\r$//' "${serial_log}" "${debug_log}" "${qemu_log}"

required_markers=(
    "SLOPOS-INPUT: PS/2 keyboard and mouse IRQ queue armed wheel=true"
    "SLOPOS-VFS: process open complete pid=2 fd=3 inode="
    "bytes=${custom_bytes} access=readonly async=true path=/etc/slopos/waybar.jsonc"
    "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2 generation=1 protocol=1"
    "SLOPOS-SYSCALL: pid=2 abi=slopos-desktop-v1 entry=syscall return=sysretq nr=1397489665 commit_bytes=40 generation=1 origin=cpl3 result=0"
    "SLOPOS-CONFIG: VFS load published initial=true generation=1 atomic=true paths=/etc/slopos/niri.kdl,/etc/slopos/waybar.jsonc,/etc/slopos/waybar.css,/etc/slopos/swww.env"
    "SLOPOS-CONFIG: reload applied generation=1 atomic=true niri=/etc/slopos/niri.kdl waybar=/etc/slopos/waybar.jsonc"
    "SLOPOS-DESKTOP-SERVICE: policy applied generation=1 owner_pid=2"
    "SLOPOS-DESKTOP-SERVICE: config acknowledged generation=1 event=config-applied wake=block-task"
    "SLOPOS-SCHED: pid=2 state=blocked->runnable reason=desktop-event event=config-applied generation=1"
    "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2 generation=2 protocol=1"
    "SLOPOS-SYSCALL: pid=2 abi=slopos-desktop-v1 entry=syscall return=sysretq nr=1397489665 commit_bytes=40 generation=2 origin=cpl3 result=0"
    "SLOPOS-DESKTOP-SERVICE: policy acknowledged generation=2 owner_pid=2 event=policy-applied wake=block-task"
    "SLOPOS-PROCESS: desktop service parked event=config-applied after_generation=1 init=wait4 resources=retained"
)
for marker in "${required_markers[@]}"; do
    if ! grep -Fq "${marker}" "${serial_log}"; then
        echo "missing custom-config marker: ${marker}" >&2
        exit 1
    fi
done

policy_markers="$(
    grep -F "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2" "${serial_log}"
)"
if [[ "$(grep -Fc "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2" "${serial_log}")" -lt 2 ]]; then
    echo "desktop service did not republish the custom policy" >&2
    exit 1
fi
if [[ "${policy_markers}" == *"config_hashes=0xd34d4a92c88d065b/"* ]]; then
    echo "desktop service reported the embedded default Waybar hash" >&2
    exit 1
fi
if grep -Fq "FATAL" "${serial_log}" || grep -Fq "state=exited" "${serial_log}"; then
    echo "persistent desktop service reached an unexpected exit or fatal path" >&2
    exit 1
fi
grep -Fq \
    "SLOPOS-WAYBAR: workspace clicked index=2 name=config changed=true module=niri/workspaces" \
    "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-WAYBAR: workspace clicked index=2 name=config changed=true module=niri/workspaces" "${serial_log}")" -ne 1 ]]; then
    echo "fullscreen click unexpectedly activated the hidden Waybar workspace target" >&2
    exit 1
fi
grep -Fq \
    "SLOPOS-WAYBAR: geometry position=top x=12 y=4 width=1000 height=40 margin=4/12/4/12 spacing=10 fixed_center=false layer=top mode=default exclusive=true passthrough=false visible=true reserved_top=44 source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: surface clicked button=left consumed=true layer=top passthrough=false" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=open-maximized value=true applied=true workspace=2 x=16 y=60 width=992 height=338 mode=maximized-column source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=open-maximized-to-edges value=true applied=true workspace=2 x=0 y=44 width=1024 height=724 mode=maximized-to-edges source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=open-fullscreen value=true applied=true workspace=2 x=0 y=0 width=1024 height=768 mode=fullscreen source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=open-focused value=true applied=true workspace=2 focused=2 activated=true source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=default-column-display value=tabbed applied=true workspace=2 source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=focus-ring enabled=false width=3 active=0x7fc8ff inactive=0x505050 applied=true source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=border enabled=true width=4 active=0xffb86c inactive=0x505050 applied=true source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=shadow enabled=true softness=8 spread=2 offset_x=6 offset_y=4 draw_behind_window=false color=0x000000 alpha=800/1000 inactive_color=0x000000 inactive_alpha=400/1000 applied=true source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=draw-border-with-background value=false applied=true source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=opacity value=750/1000 applied=true fullscreen_ignored=true source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP: window resized kind=CONFIG width=656 layout=scrolling" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=default-floating-position x=24 y=24 relative-to=bottom-right applied=true remembered=false window_x=8 window_y=406 width=992 height=338 transition=move-window-to-floating source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=default-floating-position x=24 y=24 relative-to=bottom-right applied=false remembered=true window_x=8 window_y=430 width=992 height=338 transition=move-window-to-floating source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: binding action=maximize-window-to-edges changed=true workspace=2 name=config focused=2" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP: window edge maximize toggled kind=CONFIG x=16 y=60 width=992 height=338 layout=scrolling" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP: fullscreen toggled state=inactive kind=CONFIG restore_layer=tiling x=0 y=44 width=1024 height=724 bar=visible layout=niri" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-NIRI: binding action=fullscreen-window changed=true workspace=2 name=config focused=2" \
    "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-NIRI: binding action=maximize-column changed=true workspace=2 name=config focused=2" "${serial_log}")" -ne 2 ]]; then
    echo "custom niri default-column-width did not survive one maximize restore cycle" >&2
    exit 1
fi
grep -Fq \
    "SLOPOS-WAYBAR: workspace clicked index=1 name=main changed=true module=niri/workspaces" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: module clicked name=clock button=left action=status accepted=true animate=false" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: format toggled name=clock button=left alternate=true text=\"UTC ALT\"" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: format toggled name=clock button=left alternate=false text=\"UTC\"" \
    "${serial_log}"
if [[ "$(grep -Fc "SLOPOS-WAYBAR: format toggled name=clock button=left" "${serial_log}")" -ne 2 ]]; then
    echo "Waybar alternate format did not complete one toggle cycle" >&2
    exit 1
fi
grep -Fq \
    "SLOPOS-WAYBAR: module clicked name=clock button=right action=help accepted=true animate=false" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: module clicked name=clock button=middle action=swww query accepted=true animate=false" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: module clicked name=clock button=scroll-up action=swww img /usr/share/backgrounds/slopos-sunset.ppm --transition-type none accepted=true animate=false" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: module clicked name=clock button=scroll-down action=swww img /usr/share/backgrounds/slopos-aurora.ppm --transition-type none accepted=true animate=false" \
    "${serial_log}"
grep -Fq "SLOPOS-TERMINAL: command=STATUS" "${serial_log}"
grep -Fq "SLOPOS-TERMINAL: command=ABOUT" "${serial_log}"
grep -Fq "SLOPOS-TERMINAL: command=HELP" "${serial_log}"
grep -Fq "SLOPOS-TERMINAL: command=SWWW QUERY" "${serial_log}"
test -s "${workspace_screenshot}"
test -s "${edge_screenshot}"
test -s "${column_width_screenshot}"
test -s "${floating_position_screenshot}"
test -s "${floating_remembered_screenshot}"
test -s "${action_screenshot}"
test -s "${format_restored_screenshot}"
test -s "${right_action_screenshot}"
test -s "${middle_action_screenshot}"
test -s "${scroll_up_screenshot}"
test -s "${scroll_down_screenshot}"

ppm_pixel_hex() {
    local image="$1"
    local x="$2"
    local y="$3"
    local offset=$((16 + 3 * (y * 1024 + x)))
    dd if="${image}" bs=1 skip="${offset}" count=3 status=none \
        | od -An -tx1 \
        | tr -d ' \n'
}

if [[ "$(ppm_pixel_hex "${column_width_screenshot}" 200 350)" != "222247" ]] \
    || [[ "$(ppm_pixel_hex "${column_width_screenshot}" 800 350)" != "222a4b" ]]; then
    echo "custom niri draw-border-with-background=false did not expose wallpaper through the translucent Config surface" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${workspace_screenshot}" 200 350)" != "171c2b" ]]; then
    echo "fullscreen Config surface incorrectly retained its niri opacity rule" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${column_width_screenshot}" 182 350)" != "ffb86c" ]]; then
    echo "custom niri border was not composited around the Config surface" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${column_width_screenshot}" 844 200)" != "0a0a1f" ]]; then
    echo "custom niri shadow did not apply its configured offset, spread, softness, and alpha" >&2
    exit 1
fi
if [[ "$(ppm_pixel_hex "${column_width_screenshot}" 0 20)" != "111144" ]] \
    || [[ "$(ppm_pixel_hex "${column_width_screenshot}" 12 20)" != "161a2a" ]] \
    || [[ "$(ppm_pixel_hex "${column_width_screenshot}" 20 2)" != "111144" ]] \
    || [[ "$(ppm_pixel_hex "${column_width_screenshot}" 20 4)" != "161a2a" ]]; then
    echo "custom Waybar margins did not constrain the rendered bar surface" >&2
    exit 1
fi

if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${workspace_screenshot}" \
        >"${repo_dir}/evidence/custom-config-workspace-click.png"
    pnmtopng "${edge_screenshot}" \
        >"${repo_dir}/evidence/custom-config-edge-maximized.png"
    pnmtopng "${column_width_screenshot}" \
        >"${repo_dir}/evidence/custom-config-default-column-width.png"
    pnmtopng "${floating_position_screenshot}" \
        >"${repo_dir}/evidence/custom-config-floating-position.png"
    pnmtopng "${floating_remembered_screenshot}" \
        >"${repo_dir}/evidence/custom-config-floating-remembered.png"
    pnmtopng "${action_screenshot}" \
        >"${repo_dir}/evidence/custom-config-on-click.png"
    pnmtopng "${format_restored_screenshot}" \
        >"${repo_dir}/evidence/custom-config-format-restored.png"
    pnmtopng "${right_action_screenshot}" \
        >"${repo_dir}/evidence/custom-config-on-click-right.png"
    pnmtopng "${middle_action_screenshot}" \
        >"${repo_dir}/evidence/custom-config-on-click-middle.png"
    pnmtopng "${scroll_up_screenshot}" \
        >"${repo_dir}/evidence/custom-config-scroll-up.png"
    pnmtopng "${scroll_down_screenshot}" \
        >"${repo_dir}/evidence/custom-config-scroll-down.png"
fi

cp --reflink=auto --sparse=always "${root_image}" "${runtime_root}"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${runtime_vars}"
"${debugfs}" -w -R "rm /etc/slopos/waybar.jsonc" "${runtime_root}" >/dev/null 2>&1
"${debugfs}" \
    -w \
    -R "write ${overlay_waybar} /etc/slopos/waybar.jsonc" \
    "${runtime_root}" >/dev/null 2>&1

set +e
{
    sleep 7
    echo "screendump ${overlay_screenshot}"
    echo "mouse_move 478 -358"
    sleep 1
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${overlay_clicked_screenshot}"
    echo "quit"
} | timeout 14s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${runtime_vars}" \
    -drive "if=virtio,format=raw,file=${runtime_esp}" \
    -drive "if=virtio,format=raw,file=${runtime_root}" \
    -serial "file:${overlay_serial_log}" \
    -debugcon "file:${overlay_debug_log}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot >"${overlay_qemu_log}" 2>&1
overlay_qemu_status=$?
set -e

if [[ ${overlay_qemu_status} -ne 0 && ${overlay_qemu_status} -ne 124 ]]; then
    echo "Waybar overlay QEMU failed with status ${overlay_qemu_status}" >&2
    exit "${overlay_qemu_status}"
fi
sed -i 's/\r$//' "${overlay_serial_log}" "${overlay_debug_log}" "${overlay_qemu_log}"
grep -Fq \
    "bytes=${overlay_bytes} access=readonly async=true path=/etc/slopos/waybar.jsonc" \
    "${overlay_serial_log}"
grep -Fq \
    "SLOPOS-WAYBAR: geometry position=top x=0 y=0 width=1024 height=40 margin=0/0/0/0 spacing=10 fixed_center=true layer=overlay mode=overlay exclusive=false passthrough=true visible=true reserved_top=0 source=config" \
    "${overlay_serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP: window closed kind=SYSTEM workspace=1" \
    "${overlay_serial_log}"
if grep -Fq "SLOPOS-WAYBAR: module clicked name=clock button=left" "${overlay_serial_log}"; then
    echo "Waybar overlay consumed a click despite passthrough=true" >&2
    exit 1
fi
if grep -Fq "FATAL" "${overlay_serial_log}" || grep -Fq "state=exited" "${overlay_serial_log}"; then
    echo "Waybar overlay integration reached an unexpected exit or fatal path" >&2
    exit 1
fi
test -s "${overlay_screenshot}"
test -s "${overlay_clicked_screenshot}"
if [[ "$(ppm_pixel_hex "${overlay_screenshot}" 600 39)" != "6558f5" ]]; then
    echo "Waybar overlay was not composited above the overlapping System titlebar" >&2
    exit 1
fi
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${overlay_screenshot}" \
        >"${repo_dir}/evidence/waybar-overlay-passthrough.png"
    pnmtopng "${overlay_clicked_screenshot}" \
        >"${repo_dir}/evidence/waybar-overlay-click-through.png"
fi

set +e
"${e2fsck}" -fn "${runtime_root}" >"${fsck_log}" 2>&1
fsck_status=$?
set -e
if (( fsck_status > 1 )); then
    sed -n '1,160p' "${fsck_log}" >&2
    exit "${fsck_status}"
fi

echo "SlopOS bounded niri/Waybar override, geometry, layer/mode, passthrough, actions, and alternate format verified"
