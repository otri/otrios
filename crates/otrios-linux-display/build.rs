// SPDX-License-Identifier: Apache-2.0
use std::{env, fs, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=src/platform.c");
    println!("cargo:rerun-if-changed=src/touch.c");
    println!("cargo:rerun-if-changed=src/platform.h");
    for path in ["shaders/slug.vert", "shaders/slug.frag"] {
        println!("cargo:rerun-if-changed={path}");
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux")
        || env::var_os("CARGO_FEATURE_PLATFORM").is_none()
    {
        return;
    }
    let cargo_target = env::var("TARGET").unwrap();
    let target = cargo_target.replace('-', "_");
    for variable in [
        format!("CC_{target}"),
        format!("AR_{target}"),
        "CC".into(),
        "AR".into(),
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    let compiler = env::var(format!("CC_{target}"))
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| {
            assert_eq!(
                env::var("HOST").unwrap(),
                cargo_target,
                "Set CC_{target} to the target C compiler when cross-compiling the display adapter"
            );
            "cc".into()
        });
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
    let mut shaders = String::new();
    for (name, path) in [
        ("ot_slug_vs", "shaders/slug.vert"),
        ("ot_slug_fs", "shaders/slug.frag"),
    ] {
        shaders.push_str(&format!("static const char *{name} =\n"));
        for line in fs::read_to_string(path).unwrap().lines() {
            shaders.push_str(&format!("{:?}\n", format!("{line}\n")));
        }
        shaders.push_str(";\n");
    }
    fs::write(out.join("slug_shaders.h"), shaders).unwrap();
    // The same GLSL ES 1.00 text feeds WebGL1 and native GLES2; no shader fork.
    let model_path = "../../web/shared/model-shaders.mjs";
    println!("cargo:rerun-if-changed={model_path}");
    let source = fs::read_to_string(model_path).unwrap();
    let mut model = String::new();
    for (symbol, export) in [
        ("ot_model_vs", "modelVertex"),
        ("ot_model_fs", "modelFragment"),
    ] {
        let marker = format!("export const {export}=`");
        let glsl = source
            .split_once(&marker)
            .unwrap()
            .1
            .split_once("`;")
            .unwrap()
            .0;
        model.push_str(&format!("static const char *{symbol} =\n"));
        for line in glsl.lines() {
            model.push_str(&format!("{:?}\n", format!("{line}\n")));
        }
        model.push_str(";\n");
    }
    fs::write(out.join("model_shaders.h"), model).unwrap();
    for source in ["platform", "touch"] {
        assert!(
            Command::new(&compiler)
                .args([
                    "-std=c11",
                    "-O2",
                    "-fPIC",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-I/usr/include/libdrm",
                    "-c",
                ])
                .arg(format!("src/{source}.c"))
                .arg("-o")
                .arg(out.join(format!("{source}.o")))
                .arg("-I")
                .arg(&out)
                .status()
                .unwrap()
                .success(),
            "Linux platform adapter compilation failed"
        );
    }
    assert!(
        Command::new(archiver)
            .arg("crs")
            .arg(out.join("libotrios_display.a"))
            .arg(out.join("platform.o"))
            .arg(out.join("touch.o"))
            .status()
            .unwrap()
            .success()
    );
    println!("cargo:rustc-link-search=native={}", out.display());
    for lib in ["static=otrios_display", "drm", "gbm", "EGL", "GLESv2", "m"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}
