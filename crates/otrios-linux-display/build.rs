// SPDX-License-Identifier: Apache-2.0
use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=src/platform.c");
    println!("cargo:rerun-if-changed=src/platform.h");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
        || env::var_os("CARGO_FEATURE_PLATFORM").is_none()
    {
        return;
    }
    let target = env::var("TARGET").unwrap().replace('-', "_");
    let compiler = env::var(format!("CC_{target}"))
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| "cc".into());
    let archiver = env::var(format!("AR_{target}"))
        .or_else(|_| env::var("AR"))
        .unwrap_or_else(|_| {
            if compiler.ends_with("gcc") {
                compiler.trim_end_matches("gcc").to_owned() + "ar"
            } else {
                "ar".into()
            }
        });
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    assert!(
        Command::new(compiler)
            .args([
                "-std=c11",
                "-O2",
                "-fPIC",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-I/usr/include/libdrm",
                "-c",
                "src/platform.c",
                "-o"
            ])
            .arg(out.join("platform.o"))
            .status()
            .unwrap()
            .success(),
        "Linux graphics adapter compilation failed"
    );
    assert!(
        Command::new(archiver)
            .arg("crs")
            .arg(out.join("libotrios_display.a"))
            .arg(out.join("platform.o"))
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    for lib in ["static=otrios_display", "drm", "gbm", "EGL", "GLESv2", "m"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}
