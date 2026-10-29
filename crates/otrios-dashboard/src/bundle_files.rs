// SPDX-License-Identifier: Apache-2.0
//! Bounded host filesystem/key I/O. Content/signature validation stays in otrios-core.
use otrios_core::{
    bundle::{self, BundleTree, MAX_FILE_BYTES, MAX_FILES, MAX_PATH_DEPTH, MAX_TOTAL_BYTES},
    canvas::Result,
};
use std::{fs, io::Read, path::Path, process::Command};
pub(crate) fn read_file(path: &Path, remaining: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Bundle input must be a regular file (no links)".into());
    }
    let limit = remaining.min(MAX_FILE_BYTES);
    if metadata.len() > limit as u64 {
        return Err("Bundle input byte budget exceeded".into());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Opened bundle input is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Bundle input grew beyond byte budget".into());
    }
    Ok(bytes)
}

pub(crate) fn public_key(path: &Path, private: bool, openssl: &str) -> Result<[u8; 32]> {
    if !private {
        return bundle::public_key_pem(&read_file(path, 4096)?);
    }
    let mut command = Command::new(openssl);
    command.arg("pkey");
    if !private {
        command.arg("-pubin");
    }
    let output = command
        .arg("-in")
        .arg(path)
        .args(["-pubout", "-outform", "DER"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Cannot read Ed25519 publisher public key".into());
    }
    bundle::public_key_der(&output.stdout)
}

pub(crate) fn read_tree(root: &Path) -> Result<BundleTree> {
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
