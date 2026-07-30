#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
esp_image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/custom-config-serial.log"
debug_log="${repo_dir}/evidence/custom-config-uefi-debugcon.log"
qemu_log="${repo_dir}/evidence/custom-config-qemu.log"
runtime_dir="$(mktemp -d /tmp/slopos-custom-config.XXXXXX)"
runtime_esp="${runtime_dir}/slopos-esp.img"
runtime_root="${runtime_dir}/slopos-root.ext4"
runtime_vars="${runtime_dir}/OVMF_VARS_4M.fd"
custom_waybar="${runtime_dir}/waybar.jsonc"
fsck_log="${runtime_dir}/fsck.log"
debugfs=/usr/sbin/debugfs
e2fsck=/usr/sbin/e2fsck

cleanup() {
    for temporary_file in \
        "${runtime_esp}" \
        "${runtime_root}" \
        "${runtime_vars}" \
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
sed '1i// user override accepted by the SlopOS desktop service' \
    "${repo_dir}/assets/waybar-config.jsonc" >"${custom_waybar}"
custom_bytes="$(wc -c <"${custom_waybar}")"
if (( custom_bytes <= 904 || custom_bytes > 4096 )); then
    echo "custom Waybar fixture has unexpected size: ${custom_bytes}" >&2
    exit 1
fi

"${debugfs}" -w -R "rm /etc/slopos/waybar.jsonc" "${runtime_root}" >/dev/null 2>&1
"${debugfs}" \
    -w \
    -R "write ${custom_waybar} /etc/slopos/waybar.jsonc" \
    "${runtime_root}" >/dev/null 2>&1

set +e
timeout 10s qemu-system-x86_64 \
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
    -monitor none \
    -no-reboot >"${qemu_log}" 2>&1
qemu_status=$?
set -e

if [[ ${qemu_status} -ne 0 && ${qemu_status} -ne 124 ]]; then
    echo "custom-config QEMU failed with status ${qemu_status}" >&2
    exit "${qemu_status}"
fi
sed -i 's/\r$//' "${serial_log}" "${debug_log}" "${qemu_log}"

required_markers=(
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

set +e
"${e2fsck}" -fn "${runtime_root}" >"${fsck_log}" 2>&1
fsck_status=$?
set -e
if (( fsck_status > 1 )); then
    sed -n '1,160p' "${fsck_log}" >&2
    exit "${fsck_status}"
fi

echo "SlopOS bounded user desktop configuration override verified"
