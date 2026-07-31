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
home_snapshot="${repo_dir}/target/journal-replay-homes.bin"
home_blocks=(0 1 33 38 119)

restore_clean_artifacts() {
    "${cargo_bin}" build --locked --release \
        -p slopos-kernel --target x86_64-unknown-none >/dev/null
    "${repo_dir}/scripts/make-rootfs.sh" >/dev/null
    "${repo_dir}/scripts/make-image.sh" >/dev/null
    if [[ -e "${home_snapshot}" ]]; then
        unlink "${home_snapshot}"
    fi
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
    -p slopos-init --target x86_64-unknown-none
"${cargo_bin}" build --locked --release \
    -p slopos-desktop --target x86_64-unknown-none
"${cargo_bin}" build --locked --release \
    -p slopos-kernel --target x86_64-unknown-none \
    --features journal-replay-injection
"${cargo_bin}" build --locked --release \
    -p slopos-boot --target x86_64-unknown-uefi
"${repo_dir}/scripts/make-rootfs.sh"
"${repo_dir}/scripts/make-image.sh"
for block in "${home_blocks[@]}"; do
    dd if="${root_image}" bs=4096 skip="${block}" count=1 status=none
done >"${home_snapshot}"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${injection_vars}"

set +e
timeout 10s qemu-system-x86_64 \
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
    "SLOPOS-VFS: executable loaded path=/sbin/slop-init inode=23 bytes=26344 blocks=7 matches_boot=true" \
    "${injection_serial}"
grep -Fq \
    "SLOPOS-EXT4: allocation crash injected sequence=1 start=1 tags=5 targets=0/1/33/38/119 old_state=allocated/grown new_state=free/original crash_point=after_commit_before_home writes=14 flushes=5" \
    "${injection_serial}"
grep -Fq "needs_recovery" <(/usr/sbin/dumpe2fs -h "${root_image}" 2>/dev/null)
grep -Eq "^Free blocks:[[:space:]]+61290$" \
    <(/usr/sbin/dumpe2fs -h "${root_image}" 2>/dev/null)
grep -Fq "Block 119 marked in use" \
    <(/usr/sbin/debugfs -R "testb 119" "${root_image}" 2>/dev/null)
grep -Fq "Size: 8192" \
    <(/usr/sbin/debugfs -R "stat <31>" "${root_image}" 2>/dev/null)
grep -Fq "Blockcount: 16" \
    <(/usr/sbin/debugfs -R "stat <31>" "${root_image}" 2>/dev/null)
block_is_byte 119 71

"${cargo_bin}" build --locked --release \
    -p slopos-kernel --target x86_64-unknown-none
"${repo_dir}/scripts/make-image.sh"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${replay_vars}"

set +e
timeout 20s qemu-system-x86_64 \
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
    "SLOPOS-VFS: executable loaded path=/sbin/slop-init inode=23 bytes=26344 blocks=7 matches_boot=true" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-PROCESS: pid=1 parent=0 source=vfs path=/sbin/slop-init argv1=--system format=elf64" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-PROCESS: pid=2 parent=1 source=vfs path=/sbin/slop-shell argv1=--session format=elf64" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-DESKTOP-SERVICE: policy applied generation=1 owner_pid=2 capabilities=waybar-provider/swww-policy" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-DESKTOP-SERVICE: policy acknowledged generation=1 owner_pid=2 event=policy-applied wake=block-task" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-SCHED: pid=2 state=blocked->runnable reason=desktop-event event=policy-applied generation=1" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-SYSCALL: pid=2 abi=linux-x86_64 entry=syscall return=sysretq nr=42 connect fd=3 family=AF_UNIX path=/run/slopos/wayland-0 origin=cpl3 result=0" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-SERVER: registry advertised pid=2 sequence=1 registry=2 globals=wl_compositor/wl_shm/wl_seat/wl_output/xdg_wm_base wire_bytes=156" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-SERVER: configure emitted pid=2 sequence=2 serial=1 shm=4 formats=argb8888/xrgb8888 xdg_surface=9 toplevel=10 geometry=32x24 states=empty wire_bytes=56" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-SERVER: commit accepted pid=2 generation=1 transport=AF_UNIX/SOCK_STREAM backing=inline-bootstrap-v1 lifecycle=registry/configure/ack-configure objects=registry/compositor/shm/xdg_toplevel surface=6 buffer=8 callback=11 geometry=32x24 stride=128 format=1 title=\"SlopOS Userspace\" app_id=slopos-system wire_bytes=148 pixel_bytes=3072" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-SERVER: commit acknowledged generation=1 renderer=desktop active_bank=0 event_sequence=3 events=wl_buffer.release/wl_callback.done/wl_display.delete_id callback_data=1" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-COMPOSITOR: surface rendered generation=1 owner_pid=2 app_id=slopos-system title=\"SlopOS Userspace\" geometry=32x24 destination=system-window scale=3 buffer_format=xrgb8888 frame_callback=11" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-SERVER: commit accepted pid=2 generation=2 transport=AF_UNIX/SOCK_STREAM backing=inline-bootstrap-v1 lifecycle=configured-buffer-reuse objects=registry/compositor/shm/xdg_toplevel surface=6 buffer=8 callback=11 geometry=32x24 stride=128 format=1 title=\"SlopOS Userspace\" app_id=slopos-system wire_bytes=64 pixel_bytes=3072" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-SERVER: commit acknowledged generation=2 renderer=desktop active_bank=1 event_sequence=4 events=wl_buffer.release/wl_callback.done/wl_display.delete_id callback_data=2" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-WAYLAND-COMPOSITOR: surface rendered generation=2 owner_pid=2 app_id=slopos-system title=\"SlopOS Userspace\" geometry=32x24 destination=system-window scale=3 buffer_format=xrgb8888 frame_callback=11" \
    "${replay_serial}"
grep -Fq "SLOPOS-SCHED: timer preempt from=1 to=2" "${replay_serial}"
grep -Fq "SLOPOS-SCHED: timer preempt from=2 to=1" "${replay_serial}"
grep -Fq \
    "SLOPOS-VFS: process read complete pid=1 fd=3 inode=18 offset=0 requested=76 bytes=76 user_pages=1 cross_page=false async=true" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-VFS: process write complete pid=1 fd=3 inode=31 offset=123 requested=64 bytes=64 user_pages=2 cross_page=true async=true flushed=true" \
    "${replay_serial}"
grep -Fq "SLOPOS-VFS: process close complete pid=1 fd=3 inode=31 async=false" "${replay_serial}"
grep -Fq \
    "SLOPOS-PROCESS: userspace runtime parked init=wait4 desktop=config-applied after_generation=0 resources=retained" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-PROCESS: desktop service parked event=config-applied after_generation=1 init=wait4 resources=retained" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-EXT4: journal recovery replayed sequence=1 start=1 tags=5 first_target_block=0 escaped=false home_readback=true next_sequence=2 records_cleared=true recovery=false" \
    "${replay_serial}"
grep -Fq \
    "SLOPOS-EXT4: journal superblock valid inode=8 physical_block=32801 blocks=4096 first=1 sequence=2 start=0" \
    "${replay_serial}"
virtio_summary="$(grep -F "SLOPOS-VIRTIO: bounded block sequence complete requests=" "${replay_serial}" | tail -n 1)"
requests="${virtio_summary#*requests=}"
requests="${requests%% *}"
interrupts="${virtio_summary#*interrupts=}"
interrupts="${interrupts%% *}"
queue_interrupts="${virtio_summary#*queue_interrupts=}"
queue_interrupts="${queue_interrupts%% *}"
queue_interrupts="${queue_interrupts//$'\r'/}"
if (( requests != interrupts + 1 || interrupts != queue_interrupts )); then
    echo "journal replay virtio accounting diverged: ${virtio_summary}" >&2
    exit 1
fi
grep -Fq "SLOPOS-DESKTOP: interactive compositor loop entered windows=3" "${replay_serial}"
if grep -Fq "FATAL" "${replay_serial}"; then
    echo "journal replay boot reached a fatal path" >&2
    exit 1
fi
if /usr/sbin/dumpe2fs -h "${root_image}" 2>/dev/null | grep -Fq "needs_recovery"; then
    echo "journal replay did not clear the ext4 recovery flag" >&2
    exit 1
fi
grep -Eq "^Free blocks:[[:space:]]+61291$" \
    <(/usr/sbin/dumpe2fs -h "${root_image}" 2>/dev/null)
grep -Fq "Block 119 not in use" \
    <(/usr/sbin/debugfs -R "testb 119" "${root_image}" 2>/dev/null)
grep -Fq "Size: 4096" \
    <(/usr/sbin/debugfs -R "stat <31>" "${root_image}" 2>/dev/null)
grep -Fq "Blockcount: 8" \
    <(/usr/sbin/debugfs -R "stat <31>" "${root_image}" 2>/dev/null)
if ! cmp --silent "${home_snapshot}" <(
    for block in "${home_blocks[@]}"; do
        dd if="${root_image}" bs=4096 skip="${block}" count=1 status=none
    done
); then
    for index in "${!home_blocks[@]}"; do
        block="${home_blocks[index]}"
        if ! cmp --silent \
            <(dd if="${home_snapshot}" bs=4096 skip="${index}" count=1 status=none) \
            <(dd if="${root_image}" bs=4096 skip="${block}" count=1 status=none)
        then
            echo "journal replay home block differs: ${block}" >&2
            cmp -l \
                <(dd if="${home_snapshot}" bs=4096 skip="${index}" count=1 status=none) \
                <(dd if="${root_image}" bs=4096 skip="${block}" count=1 status=none) \
                | sed -n '1,16p' >&2 || true
        fi
    done
    echo "journal replay did not restore every allocation home block" >&2
    exit 1
fi
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
if [[ "${clean_hash}" != "c20b31e59588a2c04b289332e39157945fcdcda51634a69d36b3d4abfb9ea10e" ]]; then
    echo "journal replay cleanup did not restore the reproducible root image" >&2
    exit 1
fi
echo "SlopOS committed JBD2 crash injection and mount-time replay verified"
