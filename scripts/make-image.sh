#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
boot_binary="${repo_dir}/target/x86_64-unknown-uefi/release/slopos-boot.efi"
kernel_binary="${repo_dir}/target/x86_64-unknown-none/release/slopos-kernel"
user_binary="${repo_dir}/target/x86_64-unknown-none/release/slopos-init"
initrd="${repo_dir}/assets/initrd.slp"

for required in "${boot_binary}" "${kernel_binary}" "${user_binary}" "${initrd}"; do
    if [[ ! -f "${required}" ]]; then
        echo "missing build input: ${required}" >&2
        exit 1
    fi
done

mkdir -p "${repo_dir}/target"
truncate -s 64M "${image}"
/usr/sbin/mkfs.vfat -F 32 -n SLOPOS_ESP "${image}" >/dev/null
mmd -i "${image}" ::/EFI
mmd -i "${image}" ::/EFI/BOOT
mmd -i "${image}" ::/slopos
mcopy -o -i "${image}" "${boot_binary}" ::/EFI/BOOT/BOOTX64.EFI
mcopy -o -i "${image}" "${kernel_binary}" ::/slopos/kernel.elf
mcopy -o -i "${image}" "${user_binary}" ::/slopos/init.elf
mcopy -o -i "${image}" "${initrd}" ::/slopos/initrd.slp

echo "created ${image}"
