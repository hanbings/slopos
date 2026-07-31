#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail
export LC_ALL=C

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-root.ext4"
source_dir="${repo_dir}/rootfs"
user_binary="${repo_dir}/target/x86_64-unknown-none/release/slopos-init"
desktop_binary="${repo_dir}/target/x86_64-unknown-none/release/slopos-desktop"
mke2fs=/usr/sbin/mke2fs
debugfs=/usr/sbin/debugfs
export E2FSPROGS_FAKE_TIME=1785369600
fixed_time=1785369600
staging_dir="$(mktemp -d)"
trap 'rm -rf -- "${staging_dir}"' EXIT

if [[ ! -x "${mke2fs}" || ! -x "${debugfs}" ]]; then
    echo "missing build tools from Debian package e2fsprogs" >&2
    exit 1
fi
if ! command -v base64 >/dev/null 2>&1 || ! command -v gzip >/dev/null 2>&1; then
    echo "missing base64 or gzip required to unpack the deterministic PNG fixture" >&2
    exit 1
fi
if [[ ! -d "${source_dir}" ]]; then
    echo "missing root filesystem source: ${source_dir}" >&2
    exit 1
fi
for executable in "${user_binary}" "${desktop_binary}"; do
    if [[ ! -f "${executable}" ]]; then
        echo "missing root executable: ${executable}" >&2
        exit 1
    fi
done

mkdir -p "${repo_dir}/target"
cp -a "${source_dir}/." "${staging_dir}/"
mkdir -p "${staging_dir}/sbin"
cp "${user_binary}" "${staging_dir}/sbin/slop-init"
cp "${desktop_binary}" "${staging_dir}/sbin/slop-shell"
ln -s slopos-release "${staging_dir}/etc/current-release"
cp "${repo_dir}/assets/niri-config.kdl" "${staging_dir}/etc/slopos/niri.kdl"
cp "${repo_dir}/assets/waybar-config.jsonc" "${staging_dir}/etc/slopos/waybar.jsonc"
cp "${repo_dir}/assets/waybar-style.css" "${staging_dir}/etc/slopos/waybar.css"
cp "${repo_dir}/assets/swww.env" "${staging_dir}/etc/slopos/swww.env"
mkdir -p "${staging_dir}/usr/share/slopos"
wallpaper_probe="${staging_dir}/usr/share/slopos/vfs-wallpaper.png"
# The deterministic gzip contains a 6144-byte, 12x8 16-bit RGB PNG. A long tEXt
# chunk forces ancillary data across both ext4 blocks; two IDAT chunks carry
# a dynamic-Huffman zlib stream whose rows exercise PNG filters 0 through 4.
base64 --decode "${repo_dir}/assets/wallpapers/aurora.png.gz.base64" \
    | gzip --decompress --stdout >"${wallpaper_probe}"
if [[ "$(stat -c '%s' "${wallpaper_probe}")" -ne 6144 ]]; then
    echo "VFS PNG wallpaper probe has an unexpected size" >&2
    exit 1
fi
dd if=/dev/zero bs=4096 count=9 status=none \
    | tr '\000' 'D' >"${staging_dir}/usr/share/slopos/deep-extent.bin"
dd if=/dev/zero bs=4096 count=1 status=none \
    | tr '\000' 'P' >"${staging_dir}/usr/share/slopos/write-probe.bin"
large_directory="${staging_dir}/usr/share/slopos/large-directory"
mkdir -p "${large_directory}"
ln "${staging_dir}/etc/slopos-release" "${large_directory}/seed"
printf -v long_suffix '%220s' ''
long_suffix="${long_suffix// /x}"
for link_index in {00..17}; do
    ln "${large_directory}/seed" \
        "${large_directory}/entry-${link_index}-${long_suffix}"
done
for tail_index in {00..29}; do
    ln "${large_directory}/seed" "${large_directory}/tail-${tail_index}"
done
truncate -s 0 "${image}"
truncate -s 256M "${image}"
"${mke2fs}" \
    -q \
    -t ext4 \
    -F \
    -b 4096 \
    -N 32 \
    -L SLOPOS_ROOT \
    -U 534c4f50-4f53-4000-8000-000000000001 \
    -E lazy_itable_init=0,lazy_journal_init=0,root_owner=0:0,hash_seed=534c4f50-4f53-4000-8000-000000000002 \
    -d "${staging_dir}" \
    "${image}"

# Five initialized runs no longer fit in the inode's four-entry extent root.
# Punching alternating blocks deterministically forces a checksummed depth-1
# leaf while preserving initialized data in logical blocks 0, 2, 4, 6, and 8.
for logical_block in 1 3 5 7; do
    "${debugfs}" \
        -w \
        -R "punch /usr/share/slopos/deep-extent.bin ${logical_block} ${logical_block}" \
        "${image}" >/dev/null 2>&1
done
# e2fsprogs 1.47.2 can reuse a punched data block as the new depth-1 extent
# leaf without persisting its allocation bit. It can also clear the allocation
# bits for the first punched runs before persisting their inode extent removal.
# Reissuing one missing punch makes that interrupted mutation converge; stop as
# soon as the depth-1 tree exists because punching an existing hole again can
# create an unrelated orphan transaction.
extent_stat="$("${debugfs}" -R "stat /usr/share/slopos/deep-extent.bin" "${image}" 2>/dev/null)"
if [[ "${extent_stat}" != *"(ETB0):"* ]]; then
    for retry_block in 1 3 5 7 1 3 5 7; do
        "${debugfs}" \
            -w \
            -R "punch /usr/share/slopos/deep-extent.bin ${retry_block} ${retry_block}" \
            "${image}" >/dev/null 2>&1
        extent_stat="$(
            "${debugfs}" -R "stat /usr/share/slopos/deep-extent.bin" "${image}" 2>/dev/null
        )"
        if [[ "${extent_stat}" == *"(ETB0):"* ]]; then
            break
        fi
    done
fi
# Resolve the final leaf from the image instead of coupling the allocation-bit
# repair to the sizes of earlier files.
extent_leaf="$(sed -n 's/.*(ETB0):\([0-9][0-9]*\).*/\1/p' <<<"${extent_stat}")"
if [[ ! "${extent_leaf}" =~ ^[0-9]+$ ]]; then
    echo "failed to resolve the depth-1 extent leaf" >&2
    exit 1
fi
"${debugfs}" -w -R "setb ${extent_leaf}" "${image}" >/dev/null 2>&1

# mke2fs -d intentionally preserves source ownership and inode timestamps.
# Normalize every populated inode so a fresh checkout under another uid/umask
# produces the same test image with the pinned e2fsprogs version.
while IFS= read -r relative_path; do
    image_path="/${relative_path#./}"
    source_path="${staging_dir}/${relative_path#./}"
    if [[ -L "${source_path}" ]]; then
        inode_mode=0120777
    elif [[ -d "${source_path}" ]]; then
        inode_mode=040755
    elif [[ -x "${source_path}" ]]; then
        inode_mode=0100755
    else
        inode_mode=0100644
    fi
    for field_value in \
        "mode ${inode_mode}" \
        "uid 0" \
        "gid 0" \
        "atime @${fixed_time}" \
        "ctime @${fixed_time}" \
        "mtime @${fixed_time}" \
        "crtime @${fixed_time}"
    do
        "${debugfs}" \
            -w \
            -R "set_inode_field ${image_path} ${field_value}" \
            "${image}" >/dev/null 2>&1
    done
done < <(cd "${staging_dir}" && find . -mindepth 1 -print | LC_ALL=C sort)

echo "created ${image}"
