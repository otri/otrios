// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded offline bundle inspection. No filesystem, signing, installation or guest execution.
use crate::canvas::{self, Result, hash, hex};
use flexbuffers::{FlexBufferType, Reader};
use otrios_interfaces::{bundle::*, canvas::CatalogEntry};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_FILES: usize = 64;
pub const MAX_FILE_BYTES: usize = 512 * 1024;
pub const MAX_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PATH_BYTES: usize = 240;
pub const MAX_PATH_DEPTH: usize = 8;
const DIGEST_DOMAIN: &[u8] = b"otrios-app-bundle-v1\0";
const PACKAGE_FEATURE: &str = "otrios-canvas-package-v1";
const BEHAVIOR_FEATURE: &str = "otrios-canvas-behavior-v1";

pub type BundleTree = BTreeMap<String, Vec<u8>>;

/// Integrity describes byte identity only. This type deliberately carries no trust/grant flag.
#[derive(Clone, Debug)]
pub struct InspectedBundle {
    pub info: BundleInfo,
    pub digest: String,
    pub files: usize,
    pub bytes: usize,
    pub signature: Option<BundleSignature>,
}

/// Portable, case-sensitive ASCII paths; reject case aliases across a tree too.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && path.split('/').count() <= MAX_PATH_DEPTH
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with('.')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                && !matches!(
                    part.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .as_str(),
                    "con"
                        | "prn"
                        | "aux"
                        | "nul"
                        | "com1"
                        | "com2"
                        | "com3"
                        | "com4"
                        | "com5"
                        | "com6"
                        | "com7"
                        | "com8"
                        | "com9"
                        | "lpt1"
                        | "lpt2"
                        | "lpt3"
                        | "lpt4"
                        | "lpt5"
                        | "lpt6"
                        | "lpt7"
                        | "lpt8"
                        | "lpt9"
                )
        })
}

fn payload_path(path: &str) -> bool {
    valid_path(path)
        && (path == INFO_PATH
            || [
                "Code/",
                "UI/",
                "Schemas/",
                "Resources/",
                "Dependencies/",
                "Notices/",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix)))
}

fn metadata<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MAX_METADATA_BYTES {
        return Err("Bundle metadata budget exceeded".into());
    }
    crate::flex_guard::validate(bytes, 16, MAX_METADATA_BYTES, MAX_METADATA_BYTES)?;
    // The span guard protects library readers. Also reject duplicate/unsorted map
    // keys (including unknown metadata) instead of relying on lookup behavior.
    fn maps(reader: Reader<&[u8]>) -> Result<()> {
        if reader.flexbuffer_type() == FlexBufferType::Map {
            let map = reader.get_map().map_err(canvas::err)?;
            let mut previous = None;
            for index in 0..map.len() {
                let key = map.keys_vector().index(index).map_err(canvas::err)?;
                let key = key.get_key().map_err(canvas::err)?;
                if previous.is_some_and(|before: &str| before >= key) {
                    return Err("Bundle map keys must be unique and sorted".into());
                }
                previous = Some(key);
                maps(map.index(index).map_err(canvas::err)?)?;
            }
        } else if reader.flexbuffer_type().is_vector() {
            for value in reader.get_vector().map_err(canvas::err)?.iter() {
                maps(value)?;
            }
        }
        Ok(())
    }
    maps(Reader::get_root(bytes).map_err(canvas::err)?)?;
    flexbuffers::from_slice(bytes).map_err(canvas::err)
}

fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let bytes = flexbuffers::to_vec(value).map_err(canvas::err)?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err("Bundle metadata budget exceeded".into());
    }
    Ok(bytes)
}

pub fn encode_info(info: &BundleInfo) -> Result<Vec<u8>> {
    let mut bytes = INFO_HEADER.to_vec();
    bytes.extend(encode(info)?);
    if bytes.len() > MAX_METADATA_BYTES {
        return Err("Bundle metadata budget exceeded".into());
    }
    Ok(bytes)
}

pub fn decode_info(bytes: &[u8]) -> Result<BundleInfo> {
    if !bytes.starts_with(INFO_HEADER) || bytes.len() > MAX_METADATA_BYTES {
        return Err("Unsupported bundle header/version or metadata size".into());
    }
    metadata(&bytes[INFO_HEADER.len()..])
}

fn file_manifest(tree: &BundleTree) -> BundleFiles {
    BundleFiles {
        format: FILES_FORMAT.into(),
        files: tree
            .iter()
            .filter(|(path, _)| !matches!(path.as_str(), FILES_PATH | SIGNATURE_PATH))
            .map(|(path, bytes)| BundleFile {
                path: path.clone(),
                bytes: bytes.len() as u64,
                sha256: hex(&hash(bytes)),
            })
            .collect(),
    }
}

/// Export exactly one current catalog screen/package. No resolution of external
/// navigation, authority services or named dependencies is implied.
pub fn pack(
    entry: &CatalogEntry,
    revision: u64,
    ui_bytes: &[u8],
    code: Option<&[u8]>,
) -> Result<BundleTree> {
    if entry.ui_hash != hash(ui_bytes) {
        return Err("Bundle UI differs from catalog digest".into());
    }
    let ui = canvas::decode_ui(ui_bytes)?;
    let mut required_features = vec![PACKAGE_FEATURE.to_string(), ui.format.clone()];
    if code.is_some() {
        required_features.push(BEHAVIOR_FEATURE.into());
    }
    required_features.sort();
    let info = BundleInfo {
        app: entry.app.clone(),
        revision,
        private: entry.private,
        required_features,
        capabilities: vec![],
        entry_ui: UI_PATH.into(),
        state_schema: SCHEMA_PATH.into(),
        behavior: code.map(|_| CODE_PATH.into()),
    };
    let mut tree = BTreeMap::from([
        (INFO_PATH.into(), encode_info(&info)?),
        (UI_PATH.into(), ui_bytes.to_vec()),
        (SCHEMA_PATH.into(), canvas::encode_schema(&ui.state_schema)?),
    ]);
    if let Some(code) = code {
        tree.insert(CODE_PATH.into(), code.to_vec());
    }
    tree.insert(FILES_PATH.into(), encode(&file_manifest(&tree))?);
    inspect(&tree)?;
    Ok(tree)
}

pub fn with_revision(mut tree: BundleTree, revision: u64) -> Result<BundleTree> {
    let mut info = inspect(&tree)?.info;
    if tree.get(INFO_PATH) != Some(&encode_info(&info)?) {
        return Err(
            "Extended/noncanonical metadata requires a compatible publisher for revision edits"
                .into(),
        );
    }
    info.revision = revision;
    tree.remove(SIGNATURE_PATH);
    tree.insert(INFO_PATH.into(), encode_info(&info)?);
    tree.insert(FILES_PATH.into(), encode(&file_manifest(&tree))?);
    inspect(&tree)?;
    Ok(tree)
}

/// Authoring transform. Preserve original metadata bytes and remove old proof;
/// the changed payload closure must be signed again before admission.
pub fn with_payload(mut tree: BundleTree, path: &str, bytes: &[u8]) -> Result<BundleTree> {
    inspect(&tree)?;
    if !payload_path(path) || matches!(path, INFO_PATH | UI_PATH | SCHEMA_PATH | CODE_PATH) {
        return Err("Use the typed authoring path for bundle contracts/code".into());
    }
    tree.remove(SIGNATURE_PATH);
    tree.insert(path.to_string(), bytes.to_vec());
    tree.insert(FILES_PATH.into(), encode(&file_manifest(&tree))?);
    inspect(&tree)?;
    Ok(tree)
}

/// Existing app upgrades preserve identity/admission and the live state schema.
/// Explicit operational rollback selects a previous verified release snapshot,
/// rather than disguising an older bundle as a forward update.
pub fn compatible_install(previous: &VerifiedBundle, next: &VerifiedBundle) -> Result<()> {
    let before = &previous.summary.info;
    let after = &next.summary.info;
    if before.app.id != after.app.id
        || before.app.ui_ref != after.app.ui_ref
        || before.app.state_ref != after.app.state_ref
        || before.private != after.private
        || canvas::decode_schema(previous.file(SCHEMA_PATH)?)?
            != canvas::decode_schema(next.file(SCHEMA_PATH)?)?
        || (previous.summary.digest != next.summary.digest && after.revision <= before.revision)
    {
        return Err("App install identity, state, policy or revision conflict".into());
    }
    Ok(())
}

pub fn inspect(tree: &BundleTree) -> Result<InspectedBundle> {
    if tree.is_empty() || tree.len() > MAX_FILES {
        return Err("Bundle file count budget exceeded".into());
    }
    let mut total = 0usize;
    let mut paths = BTreeSet::new();
    let mut folded = BTreeMap::new();
    for (path, bytes) in tree {
        if (!matches!(path.as_str(), FILES_PATH | SIGNATURE_PATH) && !payload_path(path))
            || bytes.len() > MAX_FILE_BYTES
        {
            return Err("Bundle path/file budget rejected".into());
        }
        // Detect aliases in directory segments as well as complete file names.
        let mut prefix = String::new();
        for part in path.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            if let Some(before) = folded.insert(prefix.to_ascii_lowercase(), prefix.clone())
                && before != prefix
            {
                return Err("Bundle case alias rejected".into());
            }
        }
        paths.insert(path.as_str());
        total = total
            .checked_add(bytes.len())
            .ok_or("Bundle size overflow")?;
        if total > MAX_TOTAL_BYTES {
            return Err("Bundle total byte budget exceeded".into());
        }
    }
    for path in &paths {
        for (index, _) in path.match_indices('/') {
            if paths.contains(&path[..index]) {
                return Err("Bundle file/directory collision".into());
            }
        }
    }
    let get = |path: &str| {
        tree.get(path)
            .map(Vec::as_slice)
            .ok_or("Missing bundle file".to_string())
    };
    let manifest_bytes = get(FILES_PATH)?;
    let manifest: BundleFiles = metadata(manifest_bytes)?;
    if manifest != file_manifest(tree) || encode(&manifest)? != manifest_bytes {
        return Err("Bundle file closure/digest or encoding mismatch".into());
    }
    let info = decode_info(get(INFO_PATH)?)?;
    if info.revision == 0
        || info.app.id.is_empty()
        || info.app.id.len() > 64
        || !info
            .app
            .id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        || info.app.title.is_empty()
        || info.app.title.len() > 256
        || info.app.ui_ref.is_empty()
        || info.app.ui_ref.len() > 128
        || info.app.state_ref.is_empty()
        || info.app.state_ref.len() > 128
        || info.app.dependencies.len() > 32
        || info
            .app
            .dependencies
            .iter()
            .any(|value| value.is_empty() || value.len() > 128)
        || info.entry_ui != UI_PATH
        || info.state_schema != SCHEMA_PATH
        || info
            .behavior
            .as_deref()
            .is_some_and(|path| path != CODE_PATH)
        || !info.capabilities.is_empty()
    {
        return Err("Unsupported bundle identity, entry paths or capabilities".into());
    }
    let features: BTreeSet<_> = info.required_features.iter().map(String::as_str).collect();
    if features.len() != info.required_features.len()
        || !features.contains(PACKAGE_FEATURE)
        || features.iter().any(|feature| {
            !matches!(
                *feature,
                PACKAGE_FEATURE
                    | BEHAVIOR_FEATURE
                    | "otrios-canvas-v2"
                    | "otrios-canvas-v3"
                    | "otrios-canvas-v4"
                    | "otrios-canvas-v5"
                    | "otrios-canvas-v6"
                    | "otrios-canvas-v7"
            )
        })
    {
        return Err("Unsupported required bundle feature".into());
    }
    let ui = canvas::decode_ui(get(UI_PATH)?)?;
    canvas::validate_ui_format(&ui)?;
    canvas::validate_schema(&ui.state_schema)?;
    if ui.ui_ref != info.app.ui_ref
        || canvas::decode_schema(get(SCHEMA_PATH)?)? != ui.state_schema
        || !features.contains(ui.format.as_str())
        || info.behavior.is_some() != ui.behavior.is_some()
        || features.contains(BEHAVIOR_FEATURE) != info.behavior.is_some()
        || tree.contains_key(CODE_PATH) != info.behavior.is_some()
    {
        return Err("Bundle UI/schema/behavior contract mismatch".into());
    }
    if let Some(reference) = ui.behavior {
        let bytes = get(CODE_PATH)?;
        if !bytes.starts_with(b"\0asm\x01\0\0\0")
            || bytes.len() > 65536
            || reference.abi != 1
            || reference.sha256 != hash(bytes)
        {
            return Err("Bundle behavior digest/ABI mismatch".into());
        }
    }
    let mut digest = Sha256::new();
    digest.update(DIGEST_DOMAIN);
    digest.update(manifest_bytes);
    let signature = tree
        .get(SIGNATURE_PATH)
        .map(|bytes| metadata::<BundleSignature>(bytes))
        .transpose()?;
    if signature.as_ref().is_some_and(|receipt| {
        receipt.format != SIGNATURE_FORMAT
            || receipt.signature.len() != 64
            || !valid_digest(&receipt.key_sha256)
    }) {
        return Err("Invalid bundle signature envelope".into());
    }
    Ok(InspectedBundle {
        info,
        digest: hex(&digest.finalize()),
        files: tree.len(),
        bytes: total,
        signature,
    })
}

/// Decode only Ed25519 SubjectPublicKeyInfo DER, never arbitrary algorithm keys.
pub fn public_key_der(bytes: &[u8]) -> Result<[u8; 32]> {
    const PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    if bytes.len() != PREFIX.len() + 32 || !bytes.starts_with(PREFIX) {
        return Err("Expected Ed25519 public key".into());
    }
    bytes[PREFIX.len()..].try_into().map_err(canvas::err)
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub fn public_key_pem(bytes: &[u8]) -> Result<[u8; 32]> {
    use base64::Engine;
    if bytes.len() > 4096 {
        return Err("Publisher key budget exceeded".into());
    }
    let text = std::str::from_utf8(bytes).map_err(canvas::err)?.trim();
    let encoded = text
        .strip_prefix("-----BEGIN PUBLIC KEY-----")
        .and_then(|s| s.strip_suffix("-----END PUBLIC KEY-----"))
        .ok_or("Expected public key PEM")?;
    let encoded: String = encoded
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    public_key_der(
        &base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(canvas::err)?,
    )
}

/// Fixed-size signing input: domain separator, raw public key, binary bundle hash.
pub fn signing_payload(digest: &str, key: &[u8; 32]) -> Result<Vec<u8>> {
    if !valid_digest(digest) {
        return Err("Invalid signing digest".into());
    }
    let mut bytes = b"otrios-app-bundle-signature-v1\0".to_vec();
    bytes.extend_from_slice(key);
    for index in (0..64).step_by(2) {
        bytes.push(u8::from_str_radix(&digest[index..index + 2], 16).map_err(canvas::err)?);
    }
    Ok(bytes)
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn attach_signature(tree: &mut BundleTree, key: &[u8; 32], signature: &[u8]) -> Result<()> {
    if signature.len() != 64 {
        return Err("Expected Ed25519 signature".into());
    }
    let receipt = BundleSignature {
        format: SIGNATURE_FORMAT.into(),
        key_sha256: hex(&hash(key)),
        signature: signature.to_vec(),
    };
    tree.insert(SIGNATURE_PATH.into(), encode(&receipt)?);
    inspect(tree)?;
    Ok(())
}

/// Owns the authenticated byte snapshot. No caller-supplied boolean can construct it.
#[derive(Debug)]
pub struct VerifiedBundle {
    summary: InspectedBundle,
    tree: BundleTree,
}
impl VerifiedBundle {
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.tree
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
    }
    pub fn summary(&self) -> &InspectedBundle {
        &self.summary
    }
    pub fn file(&self, path: &str) -> Result<&[u8]> {
        self.tree
            .get(path)
            .map(Vec::as_slice)
            .ok_or("Missing authenticated bundle file".into())
    }
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub fn verify(tree: BundleTree, key: &[u8; 32]) -> Result<VerifiedBundle> {
    let summary = inspect(&tree)?;
    let receipt = summary.signature.as_ref().ok_or("Unsigned app bundle")?;
    if receipt.key_sha256 != hex(&hash(key)) {
        return Err("Bundle publisher is not trusted".into());
    }
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(&signing_payload(&summary.digest, key)?, &receipt.signature)
        .map_err(|_| "Bundle publisher signature rejected")?;
    Ok(VerifiedBundle { summary, tree })
}

#[cfg(test)]
mod tests;
