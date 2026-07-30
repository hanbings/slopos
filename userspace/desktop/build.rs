// SPDX-License-Identifier: 0BSD

fn main() {
    println!("cargo:rerun-if-changed=linker.ld");
    println!("cargo:rerun-if-changed=../../assets/waybar-config.jsonc");
    println!("cargo:rerun-if-changed=../../assets/swww.env");
    println!("cargo:rustc-link-arg-bin=slopos-desktop=-Tuserspace/desktop/linker.ld");
}
