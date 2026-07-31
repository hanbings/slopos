#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

if ! command -v socat >/dev/null 2>&1; then
    echo "test-wayland-input requires socat for QMP wheel injection" >&2
    exit 1
fi

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/wayland-input-serial.log"
debugcon_log="${repo_dir}/evidence/wayland-input-uefi-debugcon.log"
qemu_log="${repo_dir}/evidence/wayland-input-qemu.log"
screenshot="${repo_dir}/evidence/desktop.ppm"
runtime_dir="$(mktemp -d /tmp/slopos-wayland-input.XXXXXX)"
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
truncate -s 0 "${serial_log}" "${debugcon_log}" "${qemu_log}"

(
    sleep 17
    {
        printf '%s\n' '{"execute":"qmp_capabilities"}'
        printf '%s\n' \
            '{"execute":"input-send-event","arguments":{"events":[{"type":"btn","data":{"down":true,"button":"wheel-down"}},{"type":"btn","data":{"down":false,"button":"wheel-down"}}]}}'
    } | socat - "UNIX-CONNECT:${qmp_socket}" >/dev/null
) &
qmp_pid=$!

set +e
{
    sleep 8
    echo "sendkey r 20"
    echo "sendkey e 20"
    echo "sendkey l 20"
    echo "sendkey o 20"
    echo "sendkey a 20"
    echo "sendkey d 20"
    echo "sendkey ret 20"
    sleep 3
    echo "sendkey tab 50"
    sleep 1
    echo "sendkey a 50"
    sleep 1
    echo "mouse_move 420 -240"
    sleep 1
    echo "mouse_move 2 0"
    sleep 1
    echo "mouse_button 1"
    sleep 1
    echo "mouse_button 0"
    sleep 2
    echo "screendump ${screenshot}"
    sleep 2
    echo "quit"
} | timeout 25s qemu-system-x86_64 \
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
    -no-reboot >"${qemu_log}" 2>&1
qemu_status=$?
wait "${qmp_pid}"
qmp_status=$?
set -e

if [[ ${qemu_status} -ne 0 || ${qmp_status} -ne 0 ]]; then
    echo "Wayland input QEMU/QMP failed with status ${qemu_status}/${qmp_status}" >&2
    exit 1
fi
sed -i 's/\r$//' "${serial_log}" "${debugcon_log}" "${qemu_log}"

required_markers=(
    "SLOPOS-VFS: process event stream open complete pid=2 fd=6 object=desktop-events cursor_generation=1"
    "SLOPOS-SYSCALL: pid=2 abi=linux-x86_64 entry=syscall return=suspended nr=7 poll nfds=2 events=POLLIN timeout=-1"
    "SLOPOS-SYSCALL: pid=2 abi=linux-x86_64 entry=resume return=runnable nr=7 poll nfds=2 ready=1 timeout=-1 wake=descriptor-readiness"
    "SLOPOS-VFS: process event stream read complete pid=2 fd=6 object=desktop-events after_generation=1 generation=2 bytes=32"
    "SLOPOS-VFS: process open complete pid=2 fd=7 inode=20 bytes=904 access=readonly async=true path=/etc/slopos/waybar.jsonc"
    "SLOPOS-DESKTOP-SERVICE: policy submitted pid=2 generation=2"
    "SLOPOS-WAYLAND-INPUT: keyboard focus=enter"
    "SLOPOS-WAYLAND-INPUT: key code=30 state=pressed"
    "SLOPOS-WAYLAND-INPUT: key code=30 state=released"
    "SLOPOS-WAYLAND-INPUT: pointer surface_fixed="
    "buttons=0x1 wheel=0"
    "buttons=0x0 wheel=1"
    "SLOPOS-WAYLAND-CLIENT: live input parsed keyboard=a-press/a-release pointer=motion/button-press/button-release/axis framing=stream-safe wait=poll"
)
for marker in "${required_markers[@]}"; do
    if ! grep -Fq "${marker}" "${serial_log}"; then
        echo "missing Wayland input marker: ${marker}" >&2
        tail -n 160 "${serial_log}" >&2
        exit 1
    fi
done
if grep -Fq "FATAL" "${serial_log}" || grep -Fq "state=exited" "${serial_log}"; then
    echo "Wayland input run reached a fatal or unexpected userspace exit" >&2
    exit 1
fi
test -s "${screenshot}"

ppm_pixel_hex() {
    local x="$1"
    local y="$2"
    local offset=$((16 + 3 * (y * 1024 + x)))
    dd if="${screenshot}" bs=1 skip="${offset}" count=3 status=none \
        | od -An -tx1 \
        | tr -d ' \n'
}

if [[ "$(ppm_pixel_hex 900 123)" != "8be9fd" ]] \
    || [[ "$(ppm_pixel_hex 948 123)" != "ff5555" ]] \
    || [[ "$(ppm_pixel_hex 900 159)" != "bd93f9" ]] \
    || [[ "$(ppm_pixel_hex 948 159)" != "ffb86c" ]]; then
    echo "PID 2 Wayland surface pixels were absent from the input screenshot" >&2
    exit 1
fi

if command -v pnmtopng >/dev/null 2>&1; then
    pnmtopng "${screenshot}" >"${repo_dir}/evidence/desktop.png"
fi
echo "SlopOS live Wayland keyboard/pointer input and desktop screenshot verified"
