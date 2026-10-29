// SPDX-License-Identifier: Apache-2.0
//! Offline bundle filesystem tooling; validation lives in LGPL otrios-core.
use otrios_core::{
    bundle::{self, BundleTree, MAX_FILE_BYTES, MAX_FILES, MAX_PATH_DEPTH, MAX_TOTAL_BYTES},
    canvas::{self, Result},
};
use std::{fs, io::Read, path::Path};

fn read_file(path: &Path, remaining: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Bundle input must be a regular file (no links)".into());
    }
    let limit = remaining.min(MAX_FILE_BYTES);
    if metadata.len() > limit as u64 {
        return Err("Bundle input byte budget exceeded".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Bundle input grew beyond byte budget".into());
    }
    Ok(bytes)
}

fn read_tree(root: &Path) -> Result<BundleTree> {
    fn walk(
        root: &Path,
        directory: &Path,
        tree: &mut BundleTree,
        total: &mut usize,
        entries: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth >= MAX_PATH_DEPTH
            || !fs::symlink_metadata(directory)
                .map_err(|e| e.to_string())?
                .is_dir()
        {
            return Err("Bundle directory/link/depth rejected".into());
        }
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            *entries += 1;
            if *entries > MAX_FILES * MAX_PATH_DEPTH {
                return Err("Bundle directory entry budget exceeded".into());
            }
            let path = entry.path();
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Non-UTF-8 bundle path")?
                .replace(std::path::MAIN_SEPARATOR, "/");
            if !bundle::valid_path(&name) {
                return Err("Invalid bundle path".into());
            }
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                let before = tree.len();
                walk(root, &path, tree, total, entries, depth + 1)?;
                if tree.len() == before {
                    return Err("Empty bundle directory rejected".into());
                }
            } else if kind.is_file() && tree.len() < MAX_FILES {
                let bytes = read_file(&path, MAX_TOTAL_BYTES - *total)?;
                *total += bytes.len();
                tree.insert(name, bytes);
            } else {
                return Err("Bundle link, special file or file count rejected".into());
            }
        }
        Ok(())
    }
    let mut tree = BundleTree::new();
    walk(root, root, &mut tree, &mut 0, &mut 0, 0)?;
    Ok(tree)
}

fn write_tree(destination: &Path, tree: &BundleTree) -> Result<()> {
    if destination.extension().and_then(|s| s.to_str()) != Some("otapp") {
        return Err("Bundle output must end in .otapp".into());
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err("Bundle output already exists".into());
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = destination
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid bundle output")?;
    let temporary = parent.join(format!(".{name}.pending-{}", std::process::id()));
    // A new sibling staging directory; never reuse or clean up somebody else's output.
    fs::create_dir(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        for (path, bytes) in tree {
            let output = temporary.join(path);
            fs::create_dir_all(output.parent().ok_or("Missing bundle file parent")?)
                .map_err(|e| e.to_string())?;
            fs::write(output, bytes).map_err(|e| e.to_string())?;
        }
        bundle::inspect(&read_tree(&temporary)?)?;
        if fs::symlink_metadata(destination).is_ok() {
            return Err("Bundle output appeared during staging".into());
        }
        fs::rename(&temporary, destination).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn report(tree: &BundleTree) -> Result<()> {
    let result = bundle::inspect(tree)?;
    println!(
        "{}: {} (revision {})",
        result.info.app.id, result.info.app.title, result.info.revision
    );
    println!("bundle-sha256: {}", result.digest);
    println!(
        "{} files; {} bytes; private: {}",
        result.files, result.bytes, result.info.private
    );
    println!(
        "integrity: verified; publisher: unauthenticated; installable: no (offline milestone)"
    );
    println!(
        "declared dependencies (unresolved): {:?}",
        result.info.app.dependencies
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("inspect") if args.len() == 2 => report(&read_tree(Path::new(&args[1]))?),
        Some("export") if args.len() == 6 && args[4] == "--revision" => {
            // Explicit app ID is one path component, never a caller-supplied filesystem path.
            if args[2].is_empty() || args[2].len() > 64
                || !args[2].bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err("Invalid app ID".into());
            }
            let root = Path::new(&args[1]);
            let catalog = canvas::decode_catalog(&read_file(&root.join("catalog.flex"), MAX_FILE_BYTES)?)?;
            let mut entries = catalog.apps.iter().filter(|entry| entry.app.id == args[2]);
            let entry = entries.next().ok_or("App absent from catalog")?;
            if catalog.format != "otrios-app-catalog-v1" || entries.next().is_some() {
                return Err("Unsupported or ambiguous app catalog".into());
            }
            let source = root.join(&args[2]).join("package");
            let app = canvas::decode_app(&read_file(&source.join("app.flex"), MAX_FILE_BYTES)?)?;
            if app != entry.app {
                return Err("App differs from catalog".into());
            }
            let ui = read_file(&source.join("ui.flex"), MAX_FILE_BYTES)?;
            let code = match fs::symlink_metadata(source.join("behavior.wasm")) {
                Ok(_) => Some(read_file(&source.join("behavior.wasm"), MAX_FILE_BYTES)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            let revision = args[5].parse().map_err(|_| "Invalid bundle revision")?;
            let tree = bundle::pack(entry, revision, &ui, code.as_deref())?;
            write_tree(Path::new(&args[3]), &tree)?;
            report(&tree)
        }
        _ => Err("Use canvas-bundle inspect BUNDLE or canvas-bundle export APPS APP_ID OUTPUT.otapp --revision N".into()),
    }
}
