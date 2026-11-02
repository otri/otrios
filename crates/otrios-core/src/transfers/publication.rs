// SPDX-License-Identifier: LGPL-3.0-only
//! Explicit public code objects. This directory never aliases private releases.
use super::*;
use crate::{bundle, canvas};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
const MAX_SHARES: usize = 64;
const MAX_SHARE_BYTES: u64 = 128 * 1024 * 1024;
pub struct PublicationStore {
    root: PathBuf,
    key: [u8; 32],
    _owner: fs::File,
}
pub struct PublishedApp {
    pub summary: bundle::InspectedBundle,
    pub archive: Vec<u8>,
}
fn private_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(canvas::err)?;
    if !meta.is_dir() {
        return Err("Public object storage is not a directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("Public object storage must be private".into());
        }
    }
    Ok(())
}
fn options() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn app(value: &str) -> bool {
    otrios_interfaces::canvas::valid_destination(value) && !value.contains('/')
}
impl PublicationStore {
    /// One private owner serializes publication and withdrawal across processes.
    /// Opening this store alone publishes no app.
    pub fn open(root: PathBuf, key: [u8; 32]) -> Result<Self> {
        if !root.exists() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&root).map_err(canvas::err)?;
            fs::File::open(root.parent().ok_or("Missing publication parent")?)
                .and_then(|f| f.sync_all())
                .map_err(canvas::err)?;
        }
        private_directory(&root)?;
        let path = root.join(".owner.lock");
        let owner = match options().open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&path).map_err(canvas::err)?.is_file() {
                    return Err("Invalid publication owner".into());
                }
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(canvas::err)?
            }
            Err(error) => return Err(canvas::err(error)),
        };
        owner
            .try_lock()
            .map_err(|_| "Public object storage already owned")?;
        Ok(Self {
            root,
            key,
            _owner: owner,
        })
    }
    fn path(&self, id: &str, digest: &str) -> Result<PathBuf> {
        if !app(id) || !hash(digest) {
            return Err("Invalid public app reference".into());
        }
        Ok(self.root.join(format!("{id}-{digest}.otapp.zip")))
    }
    pub fn read(&self, id: &str, digest: &str) -> Result<Option<PublishedApp>> {
        private_directory(&self.root)?;
        let path = self.path(id, digest)?;
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(canvas::err(error)),
        };
        if !meta.is_file() || meta.len() > MAX_APP_ARCHIVE_BYTES as u64 {
            return Err("Invalid public app object".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(canvas::err)?
            .take(MAX_APP_ARCHIVE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(canvas::err)?;
        let bundle = bundle::verify(archive::decode(archive::Kind::App, &bytes)?, &self.key)?;
        if bundle.summary().info.app.id != id || bundle.summary().digest != digest {
            return Err("Public app identity mismatch".into());
        }
        Ok(Some(PublishedApp {
            summary: bundle.summary().clone(),
            archive: bytes,
        }))
    }
    /// Call only after current host-operator admission and explicit code-only
    /// publication review. Receipt verification still uses this host's pinned key.
    pub fn publish(&self, files: bundle::BundleTree) -> Result<bundle::InspectedBundle> {
        private_directory(&self.root)?;
        let verified = bundle::verify(files.clone(), &self.key)?;
        let summary = verified.summary();
        let path = self.path(&summary.info.app.id, &summary.digest)?;
        let bytes = archive::encode(archive::Kind::App, &files)?;
        if let Some(existing) = self.read(&summary.info.app.id, &summary.digest)? {
            if existing.archive != bytes {
                return Err("Public app object conflict".into());
            }
            return Ok(summary.clone());
        }
        let mut count = 0;
        let mut total = bytes.len() as u64;
        for (visits, entry) in fs::read_dir(&self.root).map_err(canvas::err)?.enumerate() {
            if visits > MAX_SHARES + 64 {
                return Err("Public object inventory budget".into());
            }
            let entry = entry.map_err(canvas::err)?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "Invalid public object filename")?;
            let meta = fs::symlink_metadata(entry.path()).map_err(canvas::err)?;
            if !meta.is_file() {
                return Err("Invalid public object inventory".into());
            }
            if name == ".owner.lock" {
                continue;
            }
            if let Some(nonce) = name
                .strip_prefix(".share-")
                .and_then(|v| v.strip_suffix(".tmp"))
            {
                if !crate::spaces::valid_id(nonce) {
                    return Err("Invalid public object stage".into());
                }
                fs::remove_file(entry.path()).map_err(canvas::err)?;
                continue;
            }
            let (id, digest) = name
                .strip_suffix(".otapp.zip")
                .and_then(|v| v.rsplit_once('-'))
                .ok_or("Invalid public object filename")?;
            if !app(id) || !hash(digest) || meta.len() > MAX_APP_ARCHIVE_BYTES as u64 {
                return Err("Invalid public object inventory".into());
            }
            count += 1;
            total = total
                .checked_add(meta.len())
                .ok_or("Public object byte budget")?;
        }
        if count >= MAX_SHARES || total > MAX_SHARE_BYTES {
            return Err("Public object quota reached".into());
        }
        let mut random = [0; 16];
        getrandom::fill(&mut random).map_err(canvas::err)?;
        let stage = self
            .root
            .join(format!(".share-{}.tmp", canvas::hex(&random)));
        let result = (|| {
            let mut file = options().open(&stage).map_err(canvas::err)?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(canvas::err)?;
            fs::rename(&stage, path).map_err(canvas::err)?;
            fs::File::open(&self.root)
                .and_then(|f| f.sync_all())
                .map_err(canvas::err)
        })();
        if result.is_err() {
            let _ = fs::remove_file(stage);
        }
        result?;
        Ok(summary.clone())
    }
    /// Stops future delivery; already downloaded copies cannot be recalled.
    pub fn withdraw(&self, id: &str, digest: &str) -> Result<()> {
        let path = self.path(id, digest)?;
        if self.read(id, digest)?.is_some() {
            fs::remove_file(path).map_err(canvas::err)?;
            fs::File::open(&self.root)
                .and_then(|f| f.sync_all())
                .map_err(canvas::err)?;
        }
        Ok(())
    }
}
impl Drop for PublicationStore {
    fn drop(&mut self) {
        let _ = self._owner.unlock();
    }
}
