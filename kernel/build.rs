// SPDX-License-Identifier: 0BSD

fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
}
