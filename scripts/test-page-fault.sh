#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/page-fault-serial.log"
runtime_dir="$(mktemp -d /tmp/slopos-page-fault.XXXXXX)"
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

{
    sleep 6
    echo "sendkey f"
    echo "sendkey a"
    echo "sendkey u"
    echo "sendkey l"
    echo "sendkey t"
    echo "sendkey ret"
    sleep 1
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
    -debugcon "file:${repo_dir}/evidence/page-fault-uefi-debugcon.log" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor stdio \
    -no-reboot >/dev/null

sed -i 's/\r$//' \
    "${serial_log}" \
    "${repo_dir}/evidence/page-fault-uefi-debugcon.log"

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
    "SLOPOS-PROCESS: userspace runtime parked reason=userspace-start init=wait4 desktop=config-applied after_generation=0 resources=retained" \
    "${serial_log}"
grep -Fq \
    "SLOPOS-PROCESS: userspace runtime parked reason=desktop-event init=wait4 desktop=poll pid=2 resources=retained" \
    "${serial_log}"
grep -Fq "SLOPOS-EXCEPTION: injecting page fault at 0x40000000" "${serial_log}"
grep -Fq "SLOPOS-EXCEPTION: vector=14" "${serial_log}"
grep -Fq "cr2=0x40000000" "${serial_log}"
grep -Fq "SLOPOS-KERNEL: FATAL unhandled CPU exception" "${serial_log}"
echo "SlopOS page-fault IDT path verified"
