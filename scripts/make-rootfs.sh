#!/usr/bin/env bash
# SPDX-License-Identifier: 0BSD

set -euo pipefail

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="${repo_dir}/target/slopos-root.ext4"
source_dir="${repo_dir}/rootfs"
mke2fs=/usr/sbin/mke2fs
debugfs=/usr/sbin/debugfs
export E2FSPROGS_FAKE_TIME=1785369600

if [[ ! -x "${mke2fs}" || ! -x "${debugfs}" ]]; then
    echo "missing build tools from Debian package e2fsprogs" >&2
    exit 1
fi
if [[ ! -d "${source_dir}" ]]; then
    echo "missing root filesystem source: ${source_dir}" >&2
    exit 1
fi

mkdir -p "${repo_dir}/target"
truncate -s 128M "${image}"
"${mke2fs}" \
    -q \
    -t ext4 \
    -F \
    -b 4096 \
    -L SLOPOS_ROOT \
    -U 534c4f50-4f53-4000-8000-000000000001 \
    -E lazy_itable_init=0,lazy_journal_init=0,root_owner=0:0 \
    -d "${source_dir}" \
    "${image}"
"${debugfs}" \
    -w \
    -R "set_super_value hash_seed 534c4f50-4f53-4000-8000-000000000002" \
    "${image}" >/dev/null 2>&1

echo "created ${image}"
