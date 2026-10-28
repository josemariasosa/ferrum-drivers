use std::{env, fs, path::PathBuf};

fn main() {
    if env::var("TARGET").as_deref() != Ok("thumbv8m.main-none-eabihf") {
        return;
    }

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    fs::copy("memory/rp2350.x", out_dir.join("memory.x"))
        .expect("Failed to copy RP2350 memory script");

    println!("cargo:rustc-link-search={}", out_dir.display());

    println!("cargo:rerun-if-changed=memory/rp2350.x");
}
