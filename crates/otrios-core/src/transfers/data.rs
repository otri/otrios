// SPDX-License-Identifier: LGPL-3.0-only
//! Exact-byte data-container inspection. Codec/schema/admission qualify separately.
use super::*;
use crate::{bundle::BundleTree, canvas};
use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;

fn digest(bytes: &[u8]) -> String {
    canvas::hex(&canvas::hash(bytes))
}
fn lower_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn canonical<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    let value: T = crate::bundle::metadata(bytes)?;
    if flexbuffers::to_vec(&value).map_err(canvas::err)? != bytes {
        return Err("Noncanonical data metadata".into());
    }
    Ok(value)
}
fn valid_info(info: &DataInfo) -> bool {
    info.format == DATA_FORMAT
        && otrios_interfaces::canvas::valid_destination(&info.bundle_id)
        && !info.bundle_id.contains('/')
        && lower_hash(&info.schema_sha256)
        && info.codec_version == 1
        && matches!(
            info.codec.as_str(),
            CANVAS_CODEC | CHAT_CODEC | COLLECTION_CODEC
        )
        && info.checkpoint == "State/checkpoint.bin"
}

/// Validated container integrity only. This is neither a qualified checkpoint nor
/// permission to replace a dataset; the authority must verify both before use.
pub struct InspectedData {
    info: DataInfo,
    checkpoint: Vec<u8>,
    assets: BTreeMap<String, Vec<u8>>,
}
impl InspectedData {
    pub fn info(&self) -> &DataInfo {
        &self.info
    }
    pub fn checkpoint(&self) -> &[u8] {
        &self.checkpoint
    }
    pub fn assets(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.assets
    }
    pub(super) fn into_assets(self) -> BTreeMap<String, Vec<u8>> {
        self.assets
    }
}

fn inspect_tree(tree: &BundleTree) -> Result<DataInfo> {
    if !(3..=35).contains(&tree.len())
        || tree
            .values()
            .try_fold(0usize, |n, bytes| n.checked_add(bytes.len()))
            .is_none_or(|n| n > MAX_DATA_BYTES)
    {
        return Err("Data payload budget".into());
    }
    let raw = tree.get("Info.flex").ok_or("Missing data Info")?;
    if raw.len() > MAX_METADATA_BYTES || !raw.starts_with(DATA_INFO_HEADER) {
        return Err("Data Info framing".into());
    }
    let info: DataInfo = canonical(&raw[DATA_INFO_HEADER.len()..])?;
    if !valid_info(&info) {
        return Err("Unsupported data metadata".into());
    }
    let checkpoint = tree
        .get("State/checkpoint.bin")
        .ok_or("Missing data checkpoint")?;
    if checkpoint.is_empty() || checkpoint.len() > MAX_CHECKPOINT_BYTES {
        return Err("Data checkpoint budget".into());
    }
    for (path, bytes) in tree {
        if matches!(
            path.as_str(),
            "Info.flex" | "Integrity/files.flex" | "State/checkpoint.bin"
        ) {
            continue;
        }
        let hash = path.strip_prefix("Assets/").ok_or("Unexpected data file")?;
        if !lower_hash(hash)
            || bytes.is_empty()
            || bytes.len() > crate::canvas::chat::MAX_ASSET_BYTES
        {
            return Err("Data owned asset integrity/budget".into());
        }
    }
    let manifest: DataFiles = canonical(
        tree.get("Integrity/files.flex")
            .ok_or("Missing data integrity")?,
    )?;
    if manifest.format != DATA_FILES_FORMAT
        || manifest.files.len() != tree.len() - 1
        || manifest
            .files
            .windows(2)
            .any(|pair| pair[0].path >= pair[1].path)
    {
        return Err("Data file closure".into());
    }
    for file in &manifest.files {
        if file.path == "Integrity/files.flex" || !lower_hash(&file.sha256) {
            return Err("Data manifest entry".into());
        }
        let bytes = tree.get(&file.path).ok_or("Unlisted data file")?;
        if file.bytes != bytes.len() as u64 || digest(bytes) != file.sha256 {
            return Err("Data file integrity".into());
        }
        if let Some(hash) = file.path.strip_prefix("Assets/")
            && hash != file.sha256
        {
            return Err("Data asset name differs from its digest".into());
        }
    }
    Ok(info)
}

pub fn inspect_data(mut tree: BundleTree) -> Result<InspectedData> {
    let info = inspect_tree(&tree)?;
    let checkpoint = tree
        .remove("State/checkpoint.bin")
        .ok_or("Missing data checkpoint")?;
    tree.remove("Info.flex");
    tree.remove("Integrity/files.flex");
    let mut assets = BTreeMap::new();
    for (path, bytes) in tree {
        let hash = path.strip_prefix("Assets/").ok_or("Unexpected data file")?;
        assets.insert(hash.into(), bytes);
    }
    Ok(InspectedData {
        info,
        checkpoint,
        assets,
    })
}

/// Encode a checked container. A caller still needs a qualified portable codec
/// and current source admission; arbitrary runtime files are not portable data.
pub fn encode_data(
    info: DataInfo,
    checkpoint: Vec<u8>,
    assets: BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>> {
    if !valid_info(&info)
        || checkpoint.is_empty()
        || checkpoint.len() > MAX_CHECKPOINT_BYTES
        || assets.len() > 32
        || assets.iter().any(|(hash, bytes)| {
            !lower_hash(hash)
                || bytes.is_empty()
                || bytes.len() > crate::canvas::chat::MAX_ASSET_BYTES
        })
    {
        return Err("Unsupported data metadata".into());
    }
    let mut raw = DATA_INFO_HEADER.to_vec();
    raw.extend(flexbuffers::to_vec(info).map_err(canvas::err)?);
    let mut tree = BundleTree::from([
        ("Info.flex".into(), raw),
        ("State/checkpoint.bin".into(), checkpoint),
    ]);
    for (hash, bytes) in assets {
        tree.insert(format!("Assets/{hash}"), bytes);
    }
    let manifest = DataFiles {
        format: DATA_FILES_FORMAT.into(),
        files: tree
            .iter()
            .map(|(path, bytes)| DataFile {
                path: path.clone(),
                bytes: bytes.len() as u64,
                sha256: digest(bytes),
            })
            .collect(),
    };
    tree.insert(
        "Integrity/files.flex".into(),
        flexbuffers::to_vec(manifest).map_err(canvas::err)?,
    );
    inspect_tree(&tree)?;
    archive::encode(archive::Kind::Data, &tree)
}
