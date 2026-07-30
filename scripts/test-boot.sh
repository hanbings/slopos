#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
ovmf_vars="${repo_dir}/target/OVMF_VARS_4M.test.fd"
serial_log="${repo_dir}/evidence/serial.log"
debug_log="${repo_dir}/evidence/uefi-debugcon.log"
qemu_log="${repo_dir}/evidence/qemu-test.log"

mkdir -p "${repo_dir}/evidence"
cp /usr/share/OVMF/OVMF_VARS_4M.fd "${ovmf_vars}"

set +e
timeout 10s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${image}" \
    -drive "if=virtio,format=raw,file=${root_image}" \
    -serial "file:${serial_log}" \
    -debugcon "file:${debug_log}" \
    -global isa-debugcon.iobase=0x402 \
    -display none \
    -monitor none \
    -no-reboot >"${qemu_log}" 2>&1
qemu_status=$?
set -e

if [[ ${qemu_status} -ne 0 && ${qemu_status} -ne 124 ]]; then
    echo "QEMU failed with status ${qemu_status}" >&2
    exit "${qemu_status}"
fi

required_markers=(
    "SLOPOS-UEFI: loader entered"
    "SLOPOS-UEFI: Boot Services exited"
    "SLOPOS-KERNEL: entry reached"
    "SLOPOS-KERNEL: boot info valid"
    "SLOPOS-ACPI: XSDT MADT validated"
    "SLOPOS-MM: frame allocator initialized"
    "SLOPOS-MM: CR3 switched"
    "SLOPOS-MM: kernel heap initialized"
    "SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42"
    "SLOPOS-PCI: mechanism1 devices="
    "SLOPOS-VIRTIO: modern block queue ready size=8 capacity_sectors=524288 flush=true"
    "SLOPOS-EXT4: superblock valid label=SLOPOS_ROOT"
    "blocks=65536 inodes=32 groups=2"
    "SLOPOS-EXT4: root directory valid group_inode_table=37 inode=2 extent_block=39 entries=5 etc_inode=13 lost_found_inode=11 metadata_checksums=group/inode/directory"
    "SLOPOS-EXT4: async path read valid release_inode=17 release_bytes=40 config_inode=16 config_bytes=76 paths=/etc/slopos-release,/etc/slopos/system.conf"
    "SLOPOS-EXT4: group descriptor valid group=1 inode_table=38"
    "SLOPOS-EXT4: multiblock file valid inode=24 inode_group=1 bytes=6144 logical_blocks=2 path=/usr/share/slopos/multiblock.bin"
    "SLOPOS-EXT4: depth-one extent valid inode=21 leaf_block=85 logical_block=8 bytes=4096 metadata_checksum=valid path=/usr/share/slopos/deep-extent.bin"
    "SLOPOS-EXT4: sparse read valid inode=21 logical_block=7 zero_bytes=4096"
    "SLOPOS-EXT4: cross-block directory valid directory_inode=22 directory_blocks=2 entry_block=1 target_inode=23 target_bytes=40 metadata_checksums=valid path=/usr/share/slopos/large-directory/tail-29"
    "SLOPOS-EXT4: fast symlink valid link_inode=14 target_inode=17 target_bytes=40 target=slopos-release path=/etc/current-release"
    "SLOPOS-VFS: namespace valid mounts=1 root_fs=1 fd=3 inode=16 bytes=76 chunk_reads=5 seek_offset=7 seek_bytes=11"
    "SLOPOS-VFS: writable descriptor valid fd=3 inode=25 physical_block=98 offset=123 bytes=73 writes=2 flushes=2 cache_invalidations=2 restored=true path=/usr/share/slopos/write-probe.bin"
    "SLOPOS-EXT4: journal superblock valid inode=8 physical_block=32801 blocks=4096 first=1 sequence=1 start=0 users=1 features=0x0/0x0/0x0 uuid=match endian=big"
    "SLOPOS-EXT4: journal records staged sequence=1 target_block=98 descriptor_block=32802 data_block=32803 commit_block=32804 writes=6 flushes=3 verified=true restored=true active=false"
    "SLOPOS-EXT4: journal state transition recovery=true sequence=1 start=1 readback=valid checkpoint_start=0 restored=true transactions=0 writes=4 flushes=4"
    "SLOPOS-EXT4: active journal transaction valid sequence=1 target_block=98 recovery=true start=1 records=descriptor/data/commit replayable_readback=true home_checkpointed=true next_sequence=2 test_sequence_rewound=true restored=true writes=13 flushes=10"
    "SLOPOS-EXT4: metadata journal transactions valid inode=25 inode_table_block=38 size=4095/4096 checksums=valid transactions=2 sequences=1/2 final_sequence=3 test_sequence_rewound=true restored=true writes=23 flushes=17"
    "SLOPOS-EXT4: allocation journal transactions valid inode=25 block=99 bitmap_block=33 group_descriptor_block=1 inode_table_block=38 size=4096/8192/4096 extent_blocks=1/2/1 checksums=superblock/group/bitmap/inode transactions=2 sequences=1/2 final_sequence=3 test_sequence_rewound=true restored=true"
    "SLOPOS-EXT4: create journal transactions valid inode=26 parent_inode=20 inode_bitmap_block=36 group_descriptor_block=1 inode_table_block=38 directory_block=83 free_inodes=7/6/7 size=0 checksums=superblock/group/bitmap/inode/directory transactions=2 sequences=1/2 final_sequence=3 test_sequence_rewound=true restored=true path=/usr/share/slopos/create-probe"
    "SLOPOS-FS: block cache entries=8 hits=74 misses=69 batched_pairs=1 invalidations=16"
    "SLOPOS-VIRTIO: bounded block sequence complete requests=447 max_in_flight=2 interrupts=446 queue_interrupts=446"
    "SLOPOS-KERNEL: framebuffer ownership accepted"
    "SLOPOS-INPUT: PS/2 keyboard and mouse IRQ queue armed"
    "SLOPOS-INTERRUPT: GDT IDT LAPIC IOAPIC PIT initialized"
    "SLOPOS-DESKTOP: interactive compositor loop entered"
    "SLOPOS-ASYNC: executor entered tasks=3"
    "SLOPOS-ASYNC: timer future completed"
)

for marker in "${required_markers[@]}"; do
    if ! grep -Fq "${marker}" "${serial_log}"; then
        echo "missing boot marker: ${marker}" >&2
        echo "serial output follows:" >&2
        sed -n '1,200p' "${serial_log}" >&2
        exit 1
    fi
done

echo "SlopOS UEFI-to-desktop boot markers verified"
