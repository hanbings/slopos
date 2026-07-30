#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-esp.img"
root_image="${repo_dir}/target/slopos-root.ext4"
serial_log="${repo_dir}/evidence/serial.log"
debug_log="${repo_dir}/evidence/uefi-debugcon.log"
qemu_log="${repo_dir}/evidence/qemu-test.log"
runtime_dir="$(mktemp -d /tmp/slopos-boot.XXXXXX)"
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

set +e
timeout 10s qemu-system-x86_64 \
    -machine q35,accel=tcg \
    -cpu qemu64 \
    -m 256M \
    -drive "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd" \
    -drive "if=pflash,format=raw,file=${ovmf_vars}" \
    -drive "if=virtio,format=raw,file=${runtime_image}" \
    -drive "if=virtio,format=raw,file=${runtime_root_image}" \
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
    "SLOPOS-UEFI: user ELF loaded bytes="
    "SLOPOS-UEFI: Boot Services exited"
    "SLOPOS-KERNEL: entry reached"
    "SLOPOS-KERNEL: boot info valid"
    "SLOPOS-KERNEL: user ELF available base="
    "SLOPOS-ACPI: XSDT MADT validated"
    "SLOPOS-MM: frame allocator initialized"
    "SLOPOS-MM: CR3 switched"
    "SLOPOS-MM: kernel heap initialized"
    "SLOPOS-EBPF: verifier accepted instructions=5 interpreter_result=42"
    "SLOPOS-PCI: mechanism1 devices="
    "SLOPOS-VIRTIO: modern block queue ready size=8 capacity_sectors=524288 flush=true"
    "SLOPOS-EXT4: superblock valid label=SLOPOS_ROOT"
    "blocks=65536 inodes=32 groups=2"
    "SLOPOS-VFS: executable loaded path=/sbin/slop-init inode=23 bytes=26152 blocks=7 matches_boot=true"
    "SLOPOS-EXT4: root directory valid group_inode_table=37 inode=2 extent_block=39 entries=6 etc_inode=13 lost_found_inode=11 metadata_checksums=group/inode/directory"
    "SLOPOS-CONFIG: VFS load published initial=true generation=1 atomic=true paths=/etc/slopos/niri.kdl,/etc/slopos/waybar.jsonc,/etc/slopos/waybar.css,/etc/slopos/swww.env"
    "SLOPOS-CONFIG: reload applied generation=1 atomic=true niri=/etc/slopos/niri.kdl waybar=/etc/slopos/waybar.jsonc style=/etc/slopos/waybar.css swww=/etc/slopos/swww.env workspaces=3 module_configs=6 css_rules=12"
    "SLOPOS-EXT4: async path read valid release_inode=21 release_bytes=40 config_inode=18 config_bytes=76 paths=/etc/slopos-release,/etc/slopos/system.conf"
    "SLOPOS-EXT4: group descriptor valid group=1 inode_table=38"
    "SLOPOS-EXT4: multiblock file valid inode=30 inode_group=1 bytes=6144 logical_blocks=2 path=/usr/share/slopos/multiblock.bin"
    "SLOPOS-EXT4: depth-one extent valid inode=27 leaf_block=97 logical_block=8 bytes=4096 metadata_checksum=valid path=/usr/share/slopos/deep-extent.bin"
    "SLOPOS-EXT4: sparse read valid inode=27 logical_block=7 zero_bytes=4096"
    "SLOPOS-EXT4: cross-block directory valid directory_inode=28 directory_blocks=2 entry_block=1 target_inode=29 target_bytes=40 metadata_checksums=valid path=/usr/share/slopos/large-directory/tail-29"
    "SLOPOS-EXT4: fast symlink valid link_inode=14 target_inode=21 target_bytes=40 target=slopos-release path=/etc/current-release"
    "SLOPOS-VFS: namespace valid mounts=1 root_fs=1 fd=3 inode=18 bytes=76 chunk_reads=5 seek_offset=7 seek_bytes=11"
    "SLOPOS-VFS: writable descriptor valid fd=3 inode=31 physical_block=110 offset=123 bytes=73 writes=2 flushes=2 cache_invalidations=2 restored=true path=/usr/share/slopos/write-probe.bin"
    "SLOPOS-EXT4: journal superblock valid inode=8 physical_block=32801 blocks=4096 first=1 sequence=1 start=0 users=1 features=0x0/0x0/0x0 uuid=match endian=big"
    "SLOPOS-EXT4: journal records staged sequence=1 target_block=110 descriptor_block=32802 data_block=32803 commit_block=32804 writes=6 flushes=3 verified=true restored=true active=false"
    "SLOPOS-EXT4: journal state transition recovery=true sequence=1 start=1 readback=valid checkpoint_start=0 restored=true transactions=0 writes=4 flushes=4"
    "SLOPOS-EXT4: active journal transaction valid sequence=1 target_block=110 recovery=true start=1 records=descriptor/data/commit replayable_readback=true home_checkpointed=true next_sequence=2 test_sequence_rewound=true restored=true writes=13 flushes=10"
    "SLOPOS-EXT4: metadata journal transactions valid inode=31 inode_table_block=38 size=4095/4096 checksums=valid transactions=2 sequences=1/2 final_sequence=3 test_sequence_rewound=true restored=true writes=23 flushes=17"
    "SLOPOS-EXT4: fd append journal transactions valid fd=3 inode=31 block=111 bitmap_block=33 group_descriptor_block=1 inode_table_block=38 append_bytes=4096 size=4096/8192/4096 extent_blocks=1/2/1 checksums=superblock/group/bitmap/inode/data transactions=2 sequences=1/2 final_sequence=3 test_sequence_rewound=true restored=true"
    "SLOPOS-EXT4: VFS create journal transactions valid fd=3 inode=32 parent_inode=26 inode_bitmap_block=36 group_descriptor_block=1 inode_table_block=38 directory_block=95 free_inodes=1/0/1 size=0 access=readwrite checksums=superblock/group/bitmap/inode/directory transactions=2 sequences=1/2 final_sequence=3 test_sequence_rewound=true restored=true path=/usr/share/slopos/create-probe"
    "SLOPOS-FS: block cache entries=8 hits=143 misses=107 batched_pairs=1 invalidations=18"
    "SLOPOS-VIRTIO: bounded block sequence complete requests=489 max_in_flight=2 interrupts=488 queue_interrupts=488"
    "SLOPOS-KERNEL: framebuffer ownership accepted"
    "SLOPOS-INPUT: PS/2 keyboard and mouse IRQ queue armed"
    "SLOPOS-INTERRUPT: GDT IDT LAPIC IOAPIC PIT initialized"
    "SLOPOS-PROCESS: table initialized capacity=4 pid=1 state=ready fd_capacity=8 per_process_fds=true"
    "SLOPOS-PROCESS: pid=1 source=vfs path=/sbin/slop-init format=elf64 entry=0x40000000 segments=1 file_bytes="
    "load_bytes=2416 memory_bytes=2416 address_space="
    "user_code=0x40000000 user_stack=0x40003000"
    "code=user-readonly stack=user-writable kernel=supervisor"
    "stack_frames="
    "SLOPOS-PROCESS: pid=1 initial_stack abi=linux-x86_64 rsp=0x40002ec0 aligned=16 stack_pages=2 argc=2 argv0=/sbin/slop-init envc=3 auxv_pairs=9 bytes=320"
    "SLOPOS-SYSCALL: fast path ready instruction=syscall return=sysretq"
    "fmask=0x47700 efer_sce=true"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=suspended nr=257 openat dirfd=-100 flags=0 path=/etc/slopos/system.conf origin=cpl3"
    "SLOPOS-VFS: process open complete pid=1 fd=3 inode=18 bytes=76 access=readonly async=true path=/etc/slopos/system.conf"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=suspended nr=0 read fd=3 requested=76 user_pages=1 origin=cpl3"
    "SLOPOS-VFS: process read complete pid=1 fd=3 inode=18 offset=0 requested=76 bytes=76 user_pages=1 cross_page=false async=true"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=suspended nr=3 close fd=3 origin=cpl3"
    "SLOPOS-VFS: process close complete pid=1 fd=3 inode=18 async=false"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=suspended nr=257 openat dirfd=-100 flags=2 path=/usr/share/slopos/write-probe.bin origin=cpl3"
    "SLOPOS-VFS: process open complete pid=1 fd=3 inode=31 bytes=4096 access=readwrite async=true path=/usr/share/slopos/write-probe.bin"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=sysretq nr=8 lseek fd=3 offset=123 whence=0 async=false"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=suspended nr=1 write fd=3 requested=64 user_pages=2 origin=cpl3"
    "SLOPOS-VFS: process write complete pid=1 fd=3 inode=31 offset=123 requested=64 bytes=64 user_pages=2 cross_page=true async=true flushed=true"
    "SLOPOS-VFS: process read complete pid=1 fd=3 inode=31 offset=123 requested=64 bytes=64 user_pages=2 cross_page=true async=true"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=sysretq nr=1 write fd=1 bytes=18 origin=cpl3 result=18"
    "SLOPOS-SYSCALL: pid=1 abi=linux-x86_64 entry=syscall return=kernel nr=60 exit status=0 origin=cpl3"
    "SLOPOS-PROCESS: pid=1 state=exited status=0 syscalls=14 retained=true kernel_return=true"
    "SLOPOS-PROCESS: pid=1 exit resources released descriptors=1 backing_objects=1 address_space_retained=true"
    "SLOPOS-SHELL: config loaded niri_workspaces=3 named=2 binds=7 rules=1 active_columns=2 gaps=16 default_width=50% center=never waybar_position=top height=40 spacing=10 modules=1/1/4 module_configs=6 css_rules=12"
    "SLOPOS-WAYBAR: formats active workspace={value} window={title} cpu=\"CPU {usage}%\" memory=\"MEM {percentage}%\" intervals=5/10/30/60 css=foreground/background/padding/margin/border-bottom"
    "SLOPOS-SWWW: daemon=running output=SLOPOS-1 geometry=1024x768 image=/usr/share/backgrounds/slopos-aurora.ppm transition=simple step=32 fps=30"
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
