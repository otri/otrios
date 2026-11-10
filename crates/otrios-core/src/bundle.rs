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

pub(crate) fn metadata<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    metadata_limit(bytes, MAX_METADATA_BYTES, 16)
}

pub(crate) fn metadata_limit<T: DeserializeOwned>(
    bytes: &[u8],
    max_bytes: usize,
    max_depth: usize,
) -> Result<T> {
    if bytes.len() > max_bytes {
        return Err("Bundle metadata budget exceeded".into());
    }
    crate::flex_guard::validate(bytes, max_depth, max_bytes, max_bytes)?;
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

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
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

pub(crate) fn file_manifest(tree: &BundleTree) -> BundleFiles {
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
    if canvas::oib::seed(&ui).is_some() {
        required_features.push(otrios_interfaces::oib::FEATURE.into());
        if canvas::oib::seed(&ui).is_some_and(|s| s.designer.is_some()) {
            required_features.push(otrios_interfaces::oib::designer::FEATURE.into());
        }
        if canvas::oib::seed(&ui)
            .is_some_and(|s| s.cells.iter().any(|c| c.authored_override.is_some()))
        {
            required_features.push(otrios_interfaces::oib::AUTHORED_DEFAULTS_FEATURE.into());
        }
        if canvas::oib::seed(&ui).is_some_and(|s| s.instance_surface.is_some()) {
            required_features.push(otrios_interfaces::oib::INSTANCES_FEATURE.into());
            if canvas::oib::seed(&ui).is_some_and(|s| {
                s.instance_surface
                    .as_ref()
                    .is_some_and(|s| s.layout.is_some())
            }) {
                required_features.push(otrios_interfaces::oib::LAYOUT_FEATURE.into());
            }
        }
    }
    if ui.handlers.iter().any(|h| {
        matches!(
            h.operation,
            otrios_interfaces::canvas::Operation::OibInstance {
                action: otrios_interfaces::oib::InstanceAction::Restore
                    | otrios_interfaces::oib::InstanceAction::ResetDefaults,
                ..
            }
        )
    }) {
        required_features.push(otrios_interfaces::oib::RESTORE_FEATURE.into());
    }
    if crate::dashboard::layout::Layout::from_bytes(&ui.layout)?
        .blueprint()
        .schema
        == otrios_interfaces::layout::SPARSE_EVENTS_FORMAT
    {
        required_features.push(otrios_interfaces::layout::SPARSE_EVENTS_FORMAT.into());
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
        libraries: vec![],
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

/// Canonical nested path selected only by the exact library revision.
pub fn library_root(revision: &otrios_interfaces::oib::library::Revision) -> String {
    format!("Dependencies/{}.otlib/", hex(revision))
}

fn library_trees(
    tree: &BundleTree,
    locks: &[otrios_interfaces::oib::graph::LibraryLock],
) -> Result<Vec<(otrios_interfaces::oib::graph::LibraryLock, BundleTree)>> {
    if locks.len() > crate::library::MAX_LIBRARIES
        || locks.windows(2).any(|w| w[0].library >= w[1].library)
    {
        return Err("Invalid app library lock ordering/count".into());
    }
    let mut result = vec![];
    let roots: BTreeSet<_> = locks
        .iter()
        .map(|pin| library_root(&pin.revision))
        .collect();
    if roots.len() != locks.len() {
        return Err("Duplicate app library revision".into());
    }
    for path in tree.keys() {
        if let Some(rest) = path.strip_prefix("Dependencies/")
            && let Some((segment, _)) = rest.split_once('/')
            && segment.ends_with(".otlib")
            && !roots.contains(&format!("Dependencies/{segment}/"))
        {
            return Err("Unpinned nested library package".into());
        }
    }
    for pin in locks {
        if !crate::library::valid_id(&pin.library)
            || pin.exports.is_empty()
            || pin.exports.windows(2).any(|w| w[0].export >= w[1].export)
        {
            return Err("Invalid app library export lock".into());
        }
        let root = library_root(&pin.revision);
        let nested: BundleTree = tree
            .iter()
            .filter_map(|(path, bytes)| {
                path.strip_prefix(&root)
                    .map(|p| (p.to_string(), bytes.clone()))
            })
            .collect();
        if nested.is_empty() {
            return Err("Missing pinned library package".into());
        }
        result.push((pin.clone(), nested));
    }
    Ok(result)
}

fn match_library_bodies(
    tree: &BundleTree,
    info: &BundleInfo,
    ui: &otrios_interfaces::canvas::Ui,
) -> Result<()> {
    let seed = canvas::oib::seed(ui);
    let locks = seed
        .and_then(|s| s.graph.as_ref())
        .and_then(|g| g.imports.as_ref())
        .map_or(&[][..], |i| i.locks.as_slice());
    if locks != info.libraries
        || info
            .required_features
            .iter()
            .any(|f| f == otrios_interfaces::oib::library::FEATURE)
            == locks.is_empty()
    {
        return Err("App manifest/UI library locks or feature differ".into());
    }
    let bodies = seed.map_or(&[][..], |s| s.resolved_libraries.as_slice());
    if bodies.len() != locks.len() {
        return Err("App resolved library closure differs".into());
    }
    let executable = bodies.iter().any(|body| body.component.is_some());
    if info
        .required_features
        .iter()
        .any(|f| f == otrios_interfaces::oib::component_behavior::FEATURE)
        != executable
    {
        return Err("App component behavior feature/closure differ".into());
    }
    let mut bytes = 0usize;
    let mut code_bytes = 0usize;
    for (pin, nested) in library_trees(tree, locks)? {
        let summary = crate::library::inspect(&nested)?;
        let body = bodies
            .iter()
            .find(|b| b.library == pin.library)
            .ok_or("Missing resolved library body")?;
        bytes = bytes
            .checked_add(nested[otrios_interfaces::oib::library::GRAPH_PATH].len())
            .ok_or("Imported byte count overflow")?;
        code_bytes = code_bytes
            .checked_add(summary.component.as_ref().map_or(0, |c| c.bytes.len()))
            .ok_or("Imported component byte count overflow")?;
        if bytes > crate::library::MAX_IMPORTED_GRAPH_BYTES
            || code_bytes > crate::library::MAX_IMPORTED_COMPONENT_BYTES
            || body.component != summary.component
            || summary.info.library != pin.library
            || summary.revision != pin.revision
            || body.revision != pin.revision
            || crate::library::encode_graph(&body.graph)?
                != nested[otrios_interfaces::oib::library::GRAPH_PATH]
        {
            return Err("Signed package and resolved library body/pin differ".into());
        }
        for expected in &pin.exports {
            if summary
                .graph
                .exports
                .iter()
                .find(|e| e.id == expected.export)
                .is_none_or(|e| e.contract != expected.contract)
            {
                return Err("Nested library export contract differs".into());
            }
        }
    }
    Ok(())
}

/// Rebuild one app's exact independently signed library closure. This authoring
/// transform never preserves the old app signature or bypasses current payload
/// admission. Unchanged nested libraries are reverified with the supplied key.
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub fn with_oib_ui_and_libraries(
    mut tree: BundleTree,
    ui: &otrios_interfaces::canvas::Ui,
    locks: &[otrios_interfaces::oib::graph::LibraryLock],
    changed: &[crate::library::VerifiedLibrary],
    key: &[u8; 32],
) -> Result<BundleTree> {
    let before = inspect(&tree)?;
    if tree.get(INFO_PATH) != Some(&encode_info(&before.info)?) {
        return Err("Extended/noncanonical app metadata requires a compatible publisher".into());
    }
    let old_ui = canvas::decode_ui(&tree[UI_PATH])?;
    if ui.ui_ref != before.info.app.ui_ref
        || ui.state_schema != old_ui.state_schema
        || ui.behavior != old_ui.behavior
        || canvas::oib::contract::admission(ui)? != canvas::oib::contract::admission(&old_ui)?
    {
        return Err("Library binding changed app state/behavior/admission".into());
    }
    if changed.len() > crate::library::MAX_LIBRARIES {
        return Err("Changed library count budget".into());
    }
    let previous = library_trees(&tree, &before.info.libraries)?;
    let mut available = BTreeMap::new();
    for (_, nested) in previous {
        let verified = crate::library::verify(nested, key)?;
        available.insert(verified.summary().info.library.clone(), verified);
    }
    let mut supplied = BTreeSet::new();
    for library in changed {
        let id = &library.summary().info.library;
        if !supplied.insert(id.clone()) || !locks.iter().any(|p| &p.library == id) {
            return Err("Unused/duplicate changed library".into());
        }
        let nested = library
            .files()
            .map(|(p, b)| (p.to_owned(), b.to_vec()))
            .collect();
        available.insert(id.clone(), crate::library::verify(nested, key)?);
    }
    for pin in &before.info.libraries {
        let root = library_root(&pin.revision);
        tree.retain(|path, _| !path.starts_with(&root));
    }
    for pin in locks {
        let library = available
            .get(&pin.library)
            .ok_or("Missing verified candidate library")?;
        if library.revision() != pin.revision {
            return Err("Candidate library revision differs from lock".into());
        }
        let root = library_root(&pin.revision);
        for (path, bytes) in library.files() {
            let path = format!("{root}{path}");
            if tree.get(&path).is_some_and(|existing| existing != bytes) {
                return Err("Nested library collides with unrelated app payload".into());
            }
            tree.insert(path, bytes.to_vec());
        }
    }
    let mut info = before.info;
    info.libraries = locks.to_vec();
    info.required_features.retain(|f| {
        f != otrios_interfaces::oib::library::FEATURE
            && f != otrios_interfaces::oib::component_behavior::FEATURE
    });
    if !locks.is_empty() {
        info.required_features
            .push(otrios_interfaces::oib::library::FEATURE.into());
        info.required_features.sort();
    }
    if canvas::oib::seed(ui).is_some_and(|seed| {
        seed.resolved_libraries
            .iter()
            .any(|body| body.component.is_some())
    }) {
        info.required_features
            .push(otrios_interfaces::oib::component_behavior::FEATURE.into());
        info.required_features.sort();
    }
    tree.remove(SIGNATURE_PATH);
    tree.insert(INFO_PATH.into(), encode_info(&info)?);
    tree.insert(UI_PATH.into(), canvas::encode_ui(ui)?);
    tree.insert(SCHEMA_PATH.into(), canvas::encode_schema(&ui.state_schema)?);
    tree.insert(FILES_PATH.into(), encode(&file_manifest(&tree))?);
    inspect(&tree)?;
    Ok(tree)
}

/// Signed release classification binds compatible package replacement to the
/// same app identity, delivery policy, state bytes and admitted requirements.
/// Revisions and exact library pins select content and are deliberately absent.
/// This is a compatibility fingerprint, not a signature or capability.
pub fn update_contract(bundle: &VerifiedBundle) -> Result<String> {
    let info = &bundle.summary.info;
    let mut bytes = b"otrios-app-update-contract-v1\0".to_vec();
    bytes.extend(encode(&(
        &info.app.id,
        &info.app.ui_ref,
        &info.app.state_ref,
        info.private,
        &info.required_features,
        &info.capabilities,
        &info.entry_ui,
        &info.state_schema,
        &info.behavior,
        hash(bundle.file(SCHEMA_PATH)?),
    ))?);
    Ok(hex(&hash(&bytes)))
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

/// The bounded Canvas host has stricter discovery/launch limits than offline
/// metadata inspection. Reject an ineligible bundle before composing a release.
pub fn qualify_install(bundle: &VerifiedBundle) -> Result<()> {
    let app = &bundle.summary.info.app;
    if app.title.trim().is_empty()
        || app.title.len() > 96
        || app.title.chars().any(char::is_control)
        || !otrios_interfaces::canvas::valid_destination(&app.id)
        || app.ui_ref != format!("app:{}/ui", app.id)
        || app.state_ref.is_empty()
        || app.state_ref.len() > 128
    {
        return Err("Bundle exceeds current host discovery/launch limits".into());
    }
    Ok(())
}

pub(crate) fn validate_files(
    tree: &BundleTree,
    max_files: usize,
    max_file_bytes: usize,
    max_total_bytes: usize,
    payload_path: fn(&str) -> bool,
) -> Result<usize> {
    if tree.is_empty() || tree.len() > max_files {
        return Err("Bundle file count budget exceeded".into());
    }
    let mut total = 0usize;
    let mut paths = BTreeSet::new();
    let mut folded = BTreeMap::new();
    for (path, bytes) in tree {
        if (!matches!(path.as_str(), FILES_PATH | SIGNATURE_PATH) && !payload_path(path))
            || bytes.len() > max_file_bytes
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
        if total > max_total_bytes {
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
    Ok(total)
}

fn inspect_envelope(tree: &BundleTree) -> Result<InspectedBundle> {
    let total = validate_files(
        tree,
        MAX_FILES,
        MAX_FILE_BYTES,
        MAX_TOTAL_BYTES,
        payload_path,
    )?;
    let get = |path: &str| {
        tree.get(path)
            .map(Vec::as_slice)
            .ok_or("Missing bundle file".to_string())
    };
    let manifest_bytes = get(FILES_PATH)?;
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
    {
        return Err("Unsupported bundle identity, entry paths or capabilities".into());
    }
    // These identifiers describe required semantics; recognizing them is a
    // separate host-admission step. Even metadata-only authentication is bounded.
    fn names(values: &[String], limit: usize) -> bool {
        values.len() <= limit
            && values.iter().all(|value| {
                !value.is_empty()
                    && value.len() <= 128
                    && value.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
                    })
            })
            && values.iter().collect::<BTreeSet<_>>().len() == values.len()
    }
    if !names(&info.required_features, 64)
        || !info
            .required_features
            .iter()
            .any(|feature| feature == PACKAGE_FEATURE)
        || !names(&info.capabilities, 32)
        || !tree.contains_key(UI_PATH)
        || !tree.contains_key(SCHEMA_PATH)
        || tree.contains_key(CODE_PATH) != info.behavior.is_some()
    {
        return Err("Invalid package feature names or entry file closure".into());
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

/// Inspect byte identity and admit the complete payload to this host's contracts.
pub fn inspect(tree: &BundleTree) -> Result<InspectedBundle> {
    let summary = inspect_envelope(tree)?;
    let info = &summary.info;
    if !info.capabilities.is_empty() {
        return Err("Unsupported bundle identity, entry paths or capabilities".into());
    }
    let get = |path: &str| {
        tree.get(path)
            .map(Vec::as_slice)
            .ok_or("Missing bundle file".to_string())
    };
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
                    | "otrios-canvas-v8"
                    | "otrios-oib-v2"
                    | "otrios-oib-instances-v1"
                    | "otrios-oib-layout-v1"
                    | "otrios-oib-restore-v1"
                    | "otrios-oib-authored-defaults-v1"
                    | "otrios-oib-designer-v2"
                    | "otrios-layout-v5"
                    | "otrios-oib-library-v1"
                    | "otrios-component-behavior-v1"
            )
        })
    {
        return Err("Unsupported required bundle feature".into());
    }
    let ui = canvas::decode_ui(get(UI_PATH)?)?;
    if canvas::oib::seed(&ui).is_some_and(|s| s.designer.is_some())
        && !features.contains(otrios_interfaces::oib::designer::FEATURE)
    {
        return Err("Missing required OIB designer feature".into());
    }
    if canvas::oib::seed(&ui).is_some_and(|s| s.cells.iter().any(|c| c.authored_override.is_some()))
        && !features.contains(otrios_interfaces::oib::AUTHORED_DEFAULTS_FEATURE)
    {
        return Err("Missing required OIB authored defaults feature".into());
    }
    if ui.handlers.iter().any(|h| {
        matches!(
            h.operation,
            otrios_interfaces::canvas::Operation::OibInstance {
                action: otrios_interfaces::oib::InstanceAction::Restore
                    | otrios_interfaces::oib::InstanceAction::ResetDefaults,
                ..
            }
        )
    }) && !features.contains(otrios_interfaces::oib::RESTORE_FEATURE)
    {
        return Err("Missing required OIB restore feature".into());
    }
    canvas::validate_ui_format(&ui)?;
    if canvas::oib::seed(&ui).is_some() && !features.contains(otrios_interfaces::oib::FEATURE) {
        return Err("Missing required OIB feature".into());
    }
    if canvas::oib::seed(&ui).is_some_and(|s| s.instance_surface.is_some())
        && !features.contains(otrios_interfaces::oib::INSTANCES_FEATURE)
    {
        return Err("Missing required OIB instance feature".into());
    }
    if canvas::oib::seed(&ui).is_some_and(|s| {
        s.instance_surface
            .as_ref()
            .is_some_and(|s| s.layout.is_some())
    }) && !features.contains(otrios_interfaces::oib::LAYOUT_FEATURE)
    {
        return Err("Missing required OIB layout feature".into());
    }
    if crate::dashboard::layout::Layout::from_bytes(&ui.layout)?
        .blueprint()
        .schema
        == otrios_interfaces::layout::SPARSE_EVENTS_FORMAT
        && !features.contains(otrios_interfaces::layout::SPARSE_EVENTS_FORMAT)
    {
        return Err("Missing required sparse-layout event feature".into());
    }
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
    if let Some(reference) = &ui.behavior {
        let bytes = get(CODE_PATH)?;
        if !bytes.starts_with(b"\0asm\x01\0\0\0")
            || bytes.len() > 65536
            || reference.abi != 1
            || reference.sha256 != hash(bytes)
        {
            return Err("Bundle behavior digest/ABI mismatch".into());
        }
    }
    match_library_bodies(tree, &summary.info, &ui)?;
    Ok(summary)
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
    signing_payload_in_domain(digest, key, b"otrios-app-bundle-signature-v1\0")
}

pub(crate) fn signing_payload_in_domain(
    digest: &str,
    key: &[u8; 32],
    domain: &[u8],
) -> Result<Vec<u8>> {
    if !valid_digest(digest) {
        return Err("Invalid signing digest".into());
    }
    let mut bytes = domain.to_vec();
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

/// Authenticated predecessor identity only. This record contains no executable
/// payload, grants or installable bundle, and cannot substitute for VerifiedBundle.
#[derive(Clone, Debug)]
pub struct AuthenticatedRevision {
    app_id: String,
    revision: u64,
    digest: String,
}
impl AuthenticatedRevision {
    pub fn app_id(&self) -> &str {
        &self.app_id
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
fn authenticate_signature(summary: &InspectedBundle, key: &[u8; 32]) -> Result<()> {
    verify_signature(
        summary.signature.as_ref().ok_or("Unsigned app bundle")?,
        &summary.digest,
        key,
        SIGNATURE_FORMAT,
        b"otrios-app-bundle-signature-v1\0",
    )
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub(crate) fn verify_signature(
    receipt: &BundleSignature,
    digest: &str,
    key: &[u8; 32],
    format: &str,
    domain: &[u8],
) -> Result<()> {
    if receipt.format != format
        || receipt.signature.len() != 64
        || receipt.key_sha256 != hex(&hash(key))
    {
        return Err("Bundle publisher is not trusted or signature envelope invalid".into());
    }
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, key)
        .verify(
            &signing_payload_in_domain(digest, key, domain)?,
            &receipt.signature,
        )
        .map_err(|_| "Bundle publisher signature rejected".to_string())
}

/// Authenticate a bounded canonical package's revision metadata without parsing
/// or admitting UI/schema/guest payloads or recognizing host-required features.
/// The caller supplies the independently trusted key. Retain the same immutable
/// input tree if copying ancillary bytes; this record grants no payload access.
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub fn authenticate_revision(tree: &BundleTree, key: &[u8; 32]) -> Result<AuthenticatedRevision> {
    let summary = inspect_envelope(tree)?;
    authenticate_signature(&summary, key)?;
    Ok(AuthenticatedRevision {
        app_id: summary.info.app.id,
        revision: summary.info.revision,
        digest: summary.digest,
    })
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub fn verify(tree: BundleTree, key: &[u8; 32]) -> Result<VerifiedBundle> {
    let summary = inspect(&tree)?;
    for (_, nested) in library_trees(&tree, &summary.info.libraries)? {
        crate::library::verify(nested, key)?;
    }
    authenticate_signature(&summary, key)?;
    Ok(VerifiedBundle { summary, tree })
}

#[cfg(test)]
mod tests;
