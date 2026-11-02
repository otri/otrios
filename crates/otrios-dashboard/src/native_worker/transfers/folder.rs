// SPDX-License-Identifier: Apache-2.0
//! Private native picker/export I/O. No app receives a filesystem path.
use otrios_interfaces::transfers::{MAX_APP_ARCHIVE_BYTES, MAX_DATA_ARCHIVE_BYTES, TransferFile};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, String>;
const QUOTA: u64 = 128 * 1024 * 1024;
const FILES: usize = 64;
pub(super) fn nonce() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Transfer identifier unavailable")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn private(metadata: &Metadata) -> bool {
    metadata.is_file() && metadata.mode() & 0o077 == 0 && metadata.nlink() == 1
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}
fn open(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "Cannot open transfer file")?;
    if !private(&file.metadata().map_err(|e| e.to_string())?) {
        return Err("Transfer files must be private regular files without links".into());
    }
    Ok(file)
}
fn matches(name: &str, app: bool) -> bool {
    name.len() <= 96
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        && name.ends_with(if app { ".otapp.zip" } else { ".otdata" })
}
pub(super) struct Folder {
    root: PathBuf,
    selected: BTreeMap<String, (PathBuf, Metadata)>,
    _owner: File,
}
impl Folder {
    pub(super) fn bind(root: &Path) -> Result<Self> {
        match fs::create_dir(root) {
            Ok(()) => fs::set_permissions(root, fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(_) => return Err("Transfers folder parent is unavailable".into()),
        }
        let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
            return Err("Transfers folder must be a private directory".into());
        }
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        let owner = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join(".owner.lock"))
            .map_err(|e| e.to_string())?;
        if !private(&owner.metadata().map_err(|e| e.to_string())?) {
            return Err("Transfers lock must be private".into());
        }
        owner
            .try_lock()
            .map_err(|_| "Transfers folder already has an owner")?;
        let folder = Self {
            root,
            selected: BTreeMap::new(),
            _owner: owner,
        };
        // Only this adapter's orphaned temporary downloads, after exclusive ownership.
        for entry in fs::read_dir(&folder.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name
                .strip_prefix(".download-")
                .and_then(|s| s.strip_suffix(".tmp"))
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
                && private(&fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?)
            {
                fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            }
        }
        folder.usage(None)?;
        Ok(folder)
    }
    fn usage(&self, omit: Option<&Path>) -> Result<(usize, u64)> {
        let mut count = 0;
        let mut bytes = 0u64;
        for entry in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_name() == ".owner.lock" || omit == Some(entry.path().as_path()) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
            if !private(&metadata) {
                return Err("Transfers folder contains a nonprivate file or link".into());
            }
            count += 1;
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or("Transfer quota exceeded")?;
            if count > FILES || bytes > QUOTA {
                return Err("Transfers folder is full; remove unused files".into());
            }
        }
        Ok((count, bytes))
    }
    pub(super) fn clear(&mut self) {
        self.selected.clear();
    }
    pub(super) fn list(&mut self, app: bool) -> Result<Vec<TransferFile>> {
        self.clear();
        self.usage(None)?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !matches(&name, app) {
                continue;
            }
            let metadata = open(&entry.path())?.metadata().map_err(|e| e.to_string())?;
            let limit = if app {
                MAX_APP_ARCHIVE_BYTES
            } else {
                MAX_DATA_ARCHIVE_BYTES
            };
            if metadata.len() == 0 || metadata.len() > limit as u64 {
                continue;
            }
            let id = nonce()?;
            files.push(TransferFile {
                id: id.clone(),
                name,
                bytes: metadata.len(),
            });
            self.selected.insert(id, (entry.path(), metadata));
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    }
    pub(super) fn selected(&mut self, id: &str, app: bool) -> Result<(File, u64)> {
        let (path, before) = self.selected.remove(id).ok_or("Choose the file again")?;
        self.clear();
        if !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| matches(n, app))
        {
            return Err("Choose a compatible transfer file".into());
        }
        let file = open(&path)?;
        let after = file.metadata().map_err(|e| e.to_string())?;
        if !same(&before, &after) {
            return Err("File changed; choose it again".into());
        }
        Ok((file, after.len()))
    }
    pub(super) fn download(&self, app: &str) -> Result<Download> {
        if !otrios_interfaces::canvas::valid_destination(app) || app.contains('/') {
            return Err("Invalid transfer app".into());
        }
        let (count, bytes) = self.usage(None)?;
        if count >= FILES {
            return Err("Transfers folder is full; remove unused files".into());
        }
        let id = nonce()?;
        let path = self.root.join(format!(".download-{id}.tmp"));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|e| e.to_string())?;
        Ok(Download {
            file,
            path,
            name: format!("{app}-{}.otdata", &id[..16]),
            limit: (QUOTA - bytes).min(MAX_DATA_ARCHIVE_BYTES as u64) as usize,
        })
    }
    pub(super) fn finish(&self, download: Download) -> Result<String> {
        download.file.sync_all().map_err(|e| e.to_string())?;
        let (count, bytes) = self.usage(Some(&download.path))?;
        let size = download.file.metadata().map_err(|e| e.to_string())?.len();
        if count >= FILES || size == 0 || bytes + size > QUOTA {
            return Err("Transfers folder is full; remove unused files".into());
        }
        // Atomic, no-overwrite promotion on the same filesystem; Drop removes temp.
        fs::hard_link(&download.path, self.root.join(&download.name)).map_err(|e| e.to_string())?;
        let name = download.name.clone();
        drop(download);
        File::open(&self.root)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(name)
    }
}
pub(super) struct Download {
    pub(super) file: File,
    pub(super) limit: usize,
    path: PathBuf,
    name: String,
}
impl Drop for Download {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::io::Write;
    pub(in super::super) struct Fixture(pub(in super::super) PathBuf);
    impl Fixture {
        pub(in super::super) fn new() -> Self {
            let root = std::env::temp_dir().join(format!("otrios-transfers-{}", nonce().unwrap()));
            fs::create_dir(&root).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
            Self(root)
        }
        pub(in super::super) fn put(&self, name: &str, bytes: &[u8]) {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(self.0.join(name))
                .unwrap();
            file.write_all(bytes).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn picker_uses_single_use_metadata_bound_ids_and_rejects_changed_or_linked_files() {
        let fixture = Fixture::new();
        fixture.put("sample.otdata", b"original");
        fixture.put("sample.otapp.zip", b"code");
        let mut folder = Folder::bind(&fixture.0).unwrap();
        assert!(Folder::bind(&fixture.0).is_err());
        let files = folder.list(false).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "sample.otdata");
        assert_eq!(files[0].id.len(), 32);
        fs::write(fixture.0.join("sample.otdata"), b"changed!").unwrap();
        assert!(folder.selected(&files[0].id, false).is_err());
        let files = folder.list(false).unwrap();
        assert!(folder.selected(&files[0].id, true).is_err());
        let files = folder.list(false).unwrap();
        assert_eq!(folder.selected(&files[0].id, false).unwrap().1, 8);
        assert!(folder.selected(&files[0].id, false).is_err());
        let files = folder.list(false).unwrap();
        folder.clear();
        assert!(folder.selected(&files[0].id, false).is_err());
        fs::hard_link(
            fixture.0.join("sample.otdata"),
            fixture.0.join("alias.otdata"),
        )
        .unwrap();
        assert!(folder.list(false).is_err());
        fs::remove_file(fixture.0.join("alias.otdata")).unwrap();
        fs::set_permissions(
            fixture.0.join("sample.otdata"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(folder.list(false).is_err());
        fs::remove_file(fixture.0.join("sample.otdata")).unwrap();
        std::os::unix::fs::symlink("sample.otapp.zip", fixture.0.join("sample.otdata")).unwrap();
        assert!(folder.list(false).is_err());
    }
    #[test]
    fn exports_publish_complete_private_files_clean_failures_and_enforce_quota() {
        let fixture = Fixture::new();
        fixture.put(
            ".download-0123456789abcdef0123456789abcdef.tmp",
            b"interrupted",
        );
        fixture.put(".unrelated.tmp", b"keep");
        let mut folder = Folder::bind(&fixture.0).unwrap();
        assert!(
            !fixture
                .0
                .join(".download-0123456789abcdef0123456789abcdef.tmp")
                .exists()
        );
        assert!(fixture.0.join(".unrelated.tmp").exists());
        let mut download = folder.download("counter").unwrap();
        download.file.write_all(b"complete portable data").unwrap();
        let name = folder.finish(download).unwrap();
        assert_eq!(
            fs::read(fixture.0.join(&name)).unwrap(),
            b"complete portable data"
        );
        let metadata = fs::metadata(fixture.0.join(&name)).unwrap();
        assert!(private(&metadata));
        let failed = folder.download("counter").unwrap();
        let path = failed.path.clone();
        drop(failed);
        assert!(!path.exists());
        assert_eq!(folder.list(false).unwrap().len(), 1);
        let quota = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(fixture.0.join("quota.bin"))
            .unwrap();
        quota.set_len(QUOTA).unwrap();
        assert!(folder.download("counter").is_err());
        assert!(folder.list(false).is_err());
    }
}
