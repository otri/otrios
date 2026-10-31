// SPDX-License-Identifier: LGPL-3.0-only
//! Immutable bytes in one app collection. Digests identify bytes, never access.
//! Callers hold the owning authority lock and authorize a live message reference
//! before reading. Incomplete stages are unreachable; completed orphans are
//! bounded by the collection quota and reclaimed with the collection.
use super::{Asset, Result, err};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub use super::MAX_ASSET_BYTES;
pub const MAX_ASSETS: usize = 32;
pub struct Assets {
    root: PathBuf,
    collection: String,
}
impl Assets {
    pub fn new(storage: &Path, collection: &str) -> Result<Self> {
        if !crate::spaces::valid_id(collection)
            || !fs::symlink_metadata(storage).map_err(err)?.is_dir()
        {
            return Err("Invalid owned asset storage".into());
        }
        Ok(Self {
            root: storage.join(format!("collection-{collection}.data")),
            collection: collection.into(),
        })
    }
    pub fn collection(&self) -> &str {
        &self.collection
    }
    fn directory(&self, create: bool) -> Result<()> {
        if create && !self.root.exists() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&self.root).map_err(err)?;
            fs::File::open(self.root.parent().ok_or("Missing asset parent")?)
                .and_then(|file| file.sync_all())
                .map_err(err)?;
        }
        if !fs::symlink_metadata(&self.root).map_err(err)?.is_dir() {
            return Err("Owned asset directory is unavailable".into());
        }
        Ok(())
    }
    fn path(&self, asset: &Asset) -> Result<PathBuf> {
        super::validate_asset(asset)?;
        Ok(self.root.join(format!("asset-{}.blob", asset.sha256)))
    }
    pub fn read(&self, asset: &Asset) -> Result<Vec<u8>> {
        self.directory(false)?;
        let path = self.path(asset)?;
        let meta = fs::symlink_metadata(&path).map_err(err)?;
        if !meta.is_file() || meta.len() != u64::from(asset.bytes) {
            return Err("Owned asset metadata mismatch".into());
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(err)?
            .take(MAX_ASSET_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() != asset.bytes as usize
            || super::super::hex(&super::super::hash(&bytes)) != asset.sha256
        {
            return Err("Owned asset integrity mismatch".into());
        }
        Ok(bytes)
    }
    pub fn put(&self, name: &str, bytes: &[u8]) -> Result<Asset> {
        let asset = Asset {
            name: name.into(),
            bytes: bytes.len().try_into().map_err(err)?,
            sha256: super::super::hex(&super::super::hash(bytes)),
        };
        let path = self.path(&asset)?;
        self.directory(true)?;
        if fs::symlink_metadata(&path).is_ok() {
            self.read(&asset)?;
            return Ok(asset);
        }
        let mut count = 0;
        for (seen, entry) in fs::read_dir(&self.root).map_err(err)?.enumerate() {
            if seen >= 128 {
                return Err("Owned asset directory limit".into());
            }
            let entry = entry.map_err(err)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("Invalid owned filename")?;
            if name.starts_with("asset-") && name.ends_with(".blob") {
                let meta = fs::symlink_metadata(entry.path()).map_err(err)?;
                if !meta.is_file() || meta.len() > MAX_ASSET_BYTES as u64 {
                    return Err("Invalid owned asset inventory".into());
                }
                count += 1;
            } else if let Some(id) = name
                .strip_prefix(".asset-")
                .and_then(|n| n.strip_suffix(".tmp"))
                && super::valid_hex(id, 16)
            {
                // No journal can reference a stage. All producers are serialized
                // by the same authority lock, including publication and deletion.
                if !fs::symlink_metadata(entry.path()).map_err(err)?.is_file() {
                    return Err("Invalid asset stage".into());
                }
                fs::remove_file(entry.path()).map_err(err)?;
            }
        }
        if count >= MAX_ASSETS {
            return Err("Owned asset quota reached".into());
        }
        let stage = self.root.join(format!(".asset-{}.tmp", super::random(16)?));
        let result = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&stage).map_err(err)?;
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(err)?;
            // Do not replace an existing name, even after an external collision.
            fs::hard_link(&stage, &path).map_err(err)?;
            fs::remove_file(&stage).map_err(err)?;
            fs::File::open(&self.root)
                .and_then(|file| file.sync_all())
                .map_err(err)?;
            Ok(asset)
        })();
        if result.is_err() {
            let _ = fs::remove_file(stage);
        }
        result
    }
    pub fn copy_to(&self, destination: &Self, asset: &Asset) -> Result<Asset> {
        destination.put(&asset.name, &self.read(asset)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn storage() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "otrios-assets-{}",
            super::super::random(16).unwrap()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn immutable_copies_survive_source_removal_and_detect_corruption() {
        let root = storage();
        let source = Assets::new(&root, &"a".repeat(32)).unwrap();
        let destination = Assets::new(&root, &"b".repeat(32)).unwrap();
        let asset = source.put("photo.png", b"private image bytes").unwrap();
        assert!(
            destination.read(&asset).is_err(),
            "a hash does not locate another collection's bytes"
        );
        assert_eq!(source.copy_to(&destination, &asset).unwrap(), asset);
        assert_eq!(
            source.put("photo.png", b"private image bytes").unwrap(),
            asset
        );
        fs::remove_dir_all(source.root).unwrap();
        assert_eq!(destination.read(&asset).unwrap(), b"private image bytes");
        fs::write(destination.path(&asset).unwrap(), b"modified image data").unwrap();
        assert!(destination.read(&asset).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn stages_are_unreachable_and_quota_stays_bounded_after_restart() {
        let root = storage();
        let collection = "c".repeat(32);
        let assets = Assets::new(&root, &collection).unwrap();
        assets.directory(true).unwrap();
        let stage = assets.root.join(format!(".asset-{}.tmp", "d".repeat(32)));
        fs::write(&stage, b"interrupted unpublished bytes").unwrap();
        assert!(assets.put("../escape", b"x").is_err());
        assert!(assets.put("empty", b"").is_err());
        assert!(assets.put("large", &vec![0; MAX_ASSET_BYTES + 1]).is_err());
        for value in 0..MAX_ASSETS {
            assets.put("file.bin", &[value as u8]).unwrap();
        }
        assert!(!stage.exists());
        let reopened = Assets::new(&root, &collection).unwrap();
        assert!(reopened.put("file.bin", &[MAX_ASSETS as u8]).is_err());
        reopened.put("existing.bin", &[0]).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_namespaces_and_blobs_fail_closed() {
        use std::os::unix::fs::symlink;
        let root = storage();
        let assets = Assets::new(&root, &"e".repeat(32)).unwrap();
        let asset = assets.put("file.bin", b"confirmed").unwrap();
        let path = assets.path(&asset).unwrap();
        fs::remove_file(&path).unwrap();
        let external = root.join("external");
        fs::write(&external, b"confirmed").unwrap();
        symlink(&external, &path).unwrap();
        assert!(assets.read(&asset).is_err());
        assert!(assets.put("file.bin", b"confirmed").is_err());
        fs::remove_dir_all(&assets.root).unwrap();
        symlink(&root, &assets.root).unwrap();
        assert!(assets.put("file.bin", b"confirmed").is_err());
        fs::remove_file(&assets.root).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
