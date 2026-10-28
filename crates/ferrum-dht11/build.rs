use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=memory/rp2350.x");

    if env::var("CARGO_FEATURE_RP2350").is_err() {
        return;
    }

    if env::var("CARGO_CFG_TARGET").is_ok() {
        // Target checking handled below.
    }

    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("arm") {
        return;
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    fs::copy("memory/rp2350.x", out.join("memory.x")).expect("failed to copy RP2350 memory layout");

    println!("cargo:rustc-link-search={}", out.display());
}
