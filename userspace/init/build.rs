// SPDX-License-Identifier: 0BSD

fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rustc-link-arg-bin=slopos-init=-Tuserspace/init/linker.ld");
}
