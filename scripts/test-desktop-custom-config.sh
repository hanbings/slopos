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
column_width_screenshot="${repo_dir}/evidence/custom-config-default-column-width.ppm"
action_screenshot="${repo_dir}/evidence/custom-config-on-click.ppm"
format_restored_screenshot="${repo_dir}/evidence/custom-config-format-restored.ppm"
right_action_screenshot="${repo_dir}/evidence/custom-config-on-click-right.ppm"
middle_action_screenshot="${repo_dir}/evidence/custom-config-on-click-middle.ppm"
scroll_up_screenshot="${repo_dir}/evidence/custom-config-scroll-up.ppm"
scroll_down_screenshot="${repo_dir}/evidence/custom-config-scroll-down.ppm"
runtime_dir="$(mktemp -d /tmp/slopos-custom-config.XXXXXX)"
runtime_esp="${runtime_dir}/slopos-esp.img"
runtime_root="${runtime_dir}/slopos-root.ext4"
runtime_vars="${runtime_dir}/OVMF_VARS_4M.fd"
custom_niri="${runtime_dir}/niri.kdl"
custom_waybar="${runtime_dir}/waybar.jsonc"
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
    -e '1i// user open-maximized override accepted by the SlopOS desktop service' \
    -e 's/open-maximized false/open-maximized true/' \
    -e '/match app-id="slopos-config"/,/^}/ s/proportion 0\.5/proportion 0.667/' \
    "${repo_dir}/assets/niri-config.kdl" >"${custom_niri}"
custom_niri_bytes="$(wc -c <"${custom_niri}")"
if (( custom_niri_bytes <= 3963 || custom_niri_bytes > 4096 )); then
    echo "custom niri fixture has unexpected size: ${custom_niri_bytes}" >&2
    exit 1
fi
sed \
    -e '1i// user override accepted by the SlopOS desktop service' \
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
    echo "mouse_move 5 -364"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "screendump ${workspace_screenshot}"
    echo "sendkey meta_l-f 50"
    sleep 1
    echo "screendump ${column_width_screenshot}"
    echo "sendkey meta_l-f 50"
    sleep 1
    echo "mouse_move -24 0"
    echo "mouse_button 1"
    echo "mouse_button 0"
    sleep 1
    echo "sendkey a"
    echo "sendkey b"
    echo "sendkey o"
    echo "mouse_move 503 0"
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
} | timeout 24s qemu-system-x86_64 \
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
grep -Fq \
    "SLOPOS-NIRI: window rule app_id=slopos-config property=open-maximized value=true applied=true workspace=2 x=16 y=56 width=992 height=696 mode=maximized-column source=config" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-DESKTOP: window resized kind=CONFIG width=656 layout=scrolling" \
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
test -s "${column_width_screenshot}"
test -s "${action_screenshot}"
test -s "${format_restored_screenshot}"
test -s "${right_action_screenshot}"
test -s "${middle_action_screenshot}"
test -s "${scroll_up_screenshot}"
test -s "${scroll_down_screenshot}"
if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${workspace_screenshot}" \
        >"${repo_dir}/evidence/custom-config-workspace-click.png"
    pnmtopng "${column_width_screenshot}" \
        >"${repo_dir}/evidence/custom-config-default-column-width.png"
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

set +e
"${e2fsck}" -fn "${runtime_root}" >"${fsck_log}" 2>&1
fsck_status=$?
set -e
if (( fsck_status > 1 )); then
    sed -n '1,160p' "${fsck_log}" >&2
    exit "${fsck_status}"
fi

echo "SlopOS bounded niri/Waybar user configuration override, initial width/maximize, placement, actions, and alternate format verified"
