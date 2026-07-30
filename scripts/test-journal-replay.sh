#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${HOME}/.cargo/bin/cargo"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
injection_vars="${repo_dir}/target/OVMF_VARS_4M.journal-injection.fd"
replay_vars="${repo_dir}/target/OVMF_VARS_4M.journal-replay.fd"
injection_serial="${repo_dir}/evidence/journal-injection-serial.log"
injection_debug="${repo_dir}/evidence/journal-injection-uefi-debugcon.log"
injection_qemu="${repo_dir}/evidence/journal-injection-qemu.log"
replay_serial="${repo_dir}/evidence/journal-replay-serial.log"
replay_debug="${repo_dir}/evidence/journal-replay-uefi-debugcon.log"
replay_qemu="${repo_dir}/evidence/journal-replay-qemu.log"

restore_clean_artifacts() {
    "${cargo_bin}" build --locked --release \
        -p slopos-kernel --target x86_64-unknown-none >/dev/null
    "${repo_dir}/scripts/make-rootfs.sh" >/dev/null
    "${repo_dir}/scripts/make-image.sh" >/dev/null
}
trap restore_clean_artifacts EXIT

block_is_byte() {
    local block="$1"
    local expected="$2"
    dd if="${root_image}" bs=4096 skip="${block}" count=1 status=none \
        | od -An -tu1 -v \
        | awk -v expected="${expected}" '{
            for (field = 1; field <= NF; field++) {
                if ($field != expected) {
                    exit 1
                }
            }
        }'
}

mkdir -p "${repo_dir}/evidence"
"${cargo_bin}" build --locked --release \
    -p slopos-kernel --target x86_64-unknown-none \
    --features journal-replay-injection
"${cargo_bin}" build --locked --release \
    -p slopos-boot --target x86_64-unknown-uefi
"${repo_dir}/scripts/make-rootfs.sh"
"${repo_dir}/scripts/make-image.sh"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${injection_vars}"

set +e
timeout 6s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${injection_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -drive "if=virtio,format=raw,file=${root_image}" \
    -serial "file:${injection_serial}" \
    -debugcon "file:${injection_debug}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor none \
    -no-reboot >"${injection_qemu}" 2>&1
injection_status=$?
set -e
if [[ ${injection_status} -ne 0 && ${injection_status} -ne 124 ]]; then
    echo "journal injection QEMU failed with status ${injection_status}" >&2
    exit "${injection_status}"
fi
grep -Fq \
    "SLOPOS-EXT4: journal crash injected sequence=1 start=1 target_block=98 old_home=J new_home=P crash_point=after_commit_before_home writes=6 flushes=5" \
    "${injection_serial}"
grep -Fq "needs_recovery" <(/usr/sbin/dumpe2fs -h "${root_image}" 2>/dev/null)
block_is_byte 98 74

"${cargo_bin}" build --locked --release \
    -p slopos-kernel --target x86_64-unknown-none
"${repo_dir}/scripts/make-image.sh"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${replay_vars}"

set +e
timeout 10s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${replay_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -drive "if=virtio,format=raw,file=${root_image}" \
    -serial "file:${replay_serial}" \
    -debugcon "file:${replay_debug}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor none \
    -no-reboot >"${replay_qemu}" 2>&1
replay_status=$?
set -e
if [[ ${replay_status} -ne 0 && ${replay_status} -ne 124 ]]; then
    echo "journal replay QEMU failed with status ${replay_status}" >&2
    exit "${replay_status}"
fi
grep -Fq \
    "SLOPOS-EXT4: journal recovery replayed sequence=1 start=1 target_block=98 escaped=false home_readback=true next_sequence=2 records_cleared=true recovery=false" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-EXT4: journal superblock valid inode=8 physical_block=32801 blocks=4096 first=1 sequence=2 start=0" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-VIRTIO: bounded block sequence complete requests=330 max_in_flight=2 interrupts=329 queue_interrupts=329" \
    "${replay_serial}"
grep -Fq "SLOPOS-DESKTOP: interactive compositor loop entered windows=3" "${replay_serial}"
if grep -Fq "FATAL" "${replay_serial}"; then
    echo "journal replay boot reached a fatal path" >&2
    exit 1
fi
if /usr/sbin/dumpe2fs -h "${root_image}" 2>/dev/null | grep -Fq "needs_recovery"; then
    echo "journal replay did not clear the ext4 recovery flag" >&2
    exit 1
fi
block_is_byte 98 80
/usr/sbin/e2fsck -fn "${root_image}"

sed -i 's/\r$//' \
    "${injection_serial}" \
    "${injection_debug}" \
    "${injection_qemu}" \
    "${replay_serial}" \
    "${replay_debug}" \
    "${replay_qemu}"

restore_clean_artifacts
trap - EXIT
clean_hash="$(sha256sum "${root_image}" | awk '{print $1}')"
if [[ "${clean_hash}" != "4aeb38e91e7436b303569e9bd48145e01458dcc513f8db230f20b90a5d4a1fe2" ]]; then
    echo "journal replay cleanup did not restore the reproducible root image" >&2
    exit 1
fi
echo "SlopOS committed JBD2 crash injection and mount-time replay verified"
