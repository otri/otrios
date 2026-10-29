// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "canvas")]
//! Build-tool filesystem/process checks, not runtime or Linux IPC acceptance.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn cli(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_canvas-bundle"))
        .args(args)
        .output()
        .unwrap()
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn rejected(output: Output) {
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.push((
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut result = vec![];
    walk(root, root, &mut result);
    result.sort();
    result
}

#[test]
fn offline_export_inspect_and_filesystem_failures_in_real_processes() {
    let root = std::env::temp_dir().join(format!("otrios-bundle-cli-{}", std::process::id()));
    // Fresh, isolated output; never touch apps/**/package or a signed release store.
    fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let apps = root.join("apps");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps");
    fs::create_dir_all(apps.join("component-playground/behavior")).unwrap();
    fs::copy(
        source.join("component-playground/behavior/main.wat"),
        apps.join("component-playground/behavior/main.wat"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_canvas-pack"))
        .arg(&apps)
        .output()
        .unwrap();
    success(output);
    let first = root.join("component.otapp");
    let second = root.join("copy.otapp");
    let export = |id: &str, path: &Path| {
        cli(&[
            Path::new("export"),
            &apps,
            Path::new(id),
            path,
            Path::new("--revision"),
            Path::new("1"),
        ])
    };
    let output = success(export("component-playground", &first));
    assert!(output.contains("publisher: unauthenticated; installable: no"));
    assert!(output.contains("private: false"));
    let inspected = success(cli(&[Path::new("inspect"), &first]));
    assert_eq!(output, inspected);
    assert_eq!(output, success(export("component-playground", &second)));
    assert_eq!(snapshot(&first), snapshot(&second));
    assert!(first.join("Code/behavior.wasm").exists());
    assert!(!first.join("Contents").exists());
    let original = snapshot(&first);
    rejected(export("component-playground", &first));
    assert_eq!(snapshot(&first), original);
    let private = root.join("monitoring.otapp");
    let output = success(export("monitoring", &private));
    assert!(output.contains("private: true"));
    assert!(!private.join("Code").exists());
    let invalid = root.join("invalid.otapp");
    rejected(export("../monitoring", &invalid));
    assert!(!invalid.exists());
    let ui = first.join("UI/main.flex");
    let old_ui = fs::read(&ui).unwrap();
    fs::write(&ui, b"tampered").unwrap();
    rejected(cli(&[Path::new("inspect"), &first]));
    fs::write(&ui, old_ui).unwrap();
    fs::write(first.join("Resources-unlisted"), b"extra").unwrap();
    rejected(cli(&[Path::new("inspect"), &first]));
    fs::remove_file(first.join("Resources-unlisted")).unwrap();
    fs::create_dir(first.join("Contents")).unwrap();
    rejected(cli(&[Path::new("inspect"), &first]));
    fs::remove_dir(first.join("Contents")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(&ui, first.join("UI/link.flex")).unwrap();
        rejected(cli(&[Path::new("inspect"), &first]));
        fs::remove_file(first.join("UI/link.flex")).unwrap();
        let linked = root.join("linked.otapp");
        symlink(&first, &linked).unwrap();
        rejected(cli(&[Path::new("inspect"), &linked]));
        let generated = apps.join("component-playground/package/ui.flex");
        fs::remove_file(&generated).unwrap();
        symlink(&ui, &generated).unwrap();
        rejected(export("component-playground", &invalid));
        assert!(!invalid.exists());
    }
    success(cli(&[Path::new("inspect"), &first]));
    assert!(!fs::read_dir(&root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("pending-")
    }));
}
