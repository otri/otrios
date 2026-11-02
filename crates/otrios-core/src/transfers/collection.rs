// SPDX-License-Identifier: LGPL-3.0-only
//! Every declared app root and complete owned-asset closure cross together.
use super::*;
use crate::canvas::{self, Document, chat};
use otrios_interfaces::{canvas::StateSchema, spaces::RootSchema};
use std::collections::{BTreeMap, BTreeSet};

/// Supplied by the verified installed catalog, never the uploaded payload.
#[derive(Clone)]
pub struct RootPolicy {
    pub state_ref: String,
    pub schema: StateSchema,
    pub chat: bool,
}
pub struct OwnedRoot {
    pub state_ref: String,
    pub canvas: Vec<u8>,
    pub chat: Option<Vec<u8>>,
}
pub fn encode_collection(roots: &[OwnedRoot]) -> Result<Vec<u8>> {
    if roots.is_empty() || roots.len() > otrios_interfaces::spaces::MAX_ROOTS {
        return Err("Portable collection root budget".into());
    }
    let mut ordered = roots.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.state_ref.cmp(&b.state_ref));
    let mut bytes = COLLECTION_CHECKPOINT_HEADER.to_vec();
    bytes.extend((ordered.len() as u16).to_le_bytes());
    let mut previous = None;
    for root in ordered {
        if !valid_ref(&root.state_ref)
            || previous == Some(&root.state_ref)
            || root.canvas.is_empty()
            || root.canvas.len() > MAX_CANVAS_CHECKPOINT_BYTES
            || root
                .chat
                .as_ref()
                .is_some_and(|b| b.is_empty() || b.len() > MAX_CHECKPOINT_BYTES)
        {
            return Err("Portable collection root shape/budget".into());
        }
        previous = Some(&root.state_ref);
        let length = 2
            + root.state_ref.len()
            + 4
            + root.canvas.len()
            + 4
            + root.chat.as_ref().map_or(0, Vec::len);
        if bytes
            .len()
            .checked_add(length)
            .is_none_or(|n| n > MAX_CHECKPOINT_BYTES)
        {
            return Err("Portable collection byte budget".into());
        }
        bytes.extend((root.state_ref.len() as u16).to_le_bytes());
        bytes.extend(root.state_ref.as_bytes());
        bytes.extend((root.canvas.len() as u32).to_le_bytes());
        bytes.extend(&root.canvas);
        bytes.extend((root.chat.as_ref().map_or(0, Vec::len) as u32).to_le_bytes());
        if let Some(chat) = &root.chat {
            bytes.extend(chat);
        }
    }
    Ok(bytes)
}
fn valid_ref(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        if size > self.0.len() {
            return Err("Truncated portable collection".into());
        }
        let (v, rest) = self.0.split_at(size);
        self.0 = rest;
        Ok(v)
    }
    fn u16(&mut self) -> Result<usize> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(canvas::err)?) as usize)
    }
    fn sized(&mut self, budget: usize) -> Result<&'a [u8]> {
        let size = u32::from_le_bytes(self.take(4)?.try_into().map_err(canvas::err)?) as usize;
        if size > budget {
            return Err("Portable root byte budget".into());
        }
        self.take(size)
    }
}
pub struct QualifiedCollection {
    app: String,
    schema: String,
    roots: Vec<(String, Document, Option<chat::QualifiedHistory>)>,
    assets: BTreeMap<String, Vec<u8>>,
}
impl QualifiedCollection {
    pub fn qualify(
        data: InspectedData,
        app: &str,
        schema: &str,
        policies: &[RootPolicy],
    ) -> Result<Self> {
        let trusted = policies
            .iter()
            .map(|p| RootSchema {
                state_ref: p.state_ref.clone(),
                schema: p.schema.clone(),
            })
            .collect::<Vec<_>>();
        let unique = policies
            .iter()
            .map(|p| p.state_ref.as_str())
            .collect::<BTreeSet<_>>();
        if data.info().bundle_id != app
            || data.info().schema_sha256 != schema
            || data.info().codec != COLLECTION_CODEC
            || policies.len() != unique.len()
            || crate::spaces::collection_schema(&trusted).map_err(canvas::err)? != schema
        {
            return Err("Portable collection installed policy mismatch".into());
        }
        let mut reader = Reader(data.checkpoint());
        if reader.take(8)? != COLLECTION_CHECKPOINT_HEADER || reader.u16()? != policies.len() {
            return Err("Portable collection framing/root closure".into());
        }
        let mut result = Self {
            app: app.into(),
            schema: schema.into(),
            roots: vec![],
            assets: BTreeMap::new(),
        };
        let mut previous: Option<String> = None;
        let mut assets = BTreeMap::<String, chat::Asset>::new();
        for _ in policies {
            let length = reader.u16()?;
            if length > 128 {
                return Err("Portable root reference budget".into());
            }
            let state_ref = std::str::from_utf8(reader.take(length)?).map_err(canvas::err)?;
            if !valid_ref(state_ref) || previous.as_deref().is_some_and(|old| old >= state_ref) {
                return Err("Portable root order/duplicate".into());
            }
            let policy = policies
                .iter()
                .find(|p| p.state_ref == state_ref)
                .ok_or("Unknown portable app root")?;
            let canvas = Document::from_portable_checkpoint(
                policy.schema.clone(),
                state_ref,
                reader.sized(MAX_CANVAS_CHECKPOINT_BYTES)?,
            )?;
            let chat = reader.sized(MAX_CHECKPOINT_BYTES)?;
            let chat = if policy.chat {
                let chat = chat::QualifiedHistory::decode(chat)?;
                for (hash, asset) in chat.assets() {
                    if assets.get(hash).is_some_and(|old| old.bytes != asset.bytes) {
                        return Err("Inconsistent cross-root attachment".into());
                    }
                    assets.insert(hash.clone(), asset.clone());
                }
                Some(chat)
            } else {
                if !chat.is_empty() {
                    return Err("Unexpected portable chat root".into());
                }
                None
            };
            previous = Some(state_ref.into());
            result.roots.push((state_ref.into(), canvas, chat));
        }
        if !reader.0.is_empty() || assets.len() != data.assets().len() {
            return Err("Portable collection exact file closure".into());
        }
        for (hash, asset) in assets {
            let bytes = data.assets().get(&hash).ok_or("Missing owned attachment")?;
            if bytes.len() != asset.bytes as usize {
                return Err("Owned attachment byte length".into());
            }
        }
        result.assets = data.into_assets();
        Ok(result)
    }
    pub(crate) fn matches(&self, app: &str, schema: &str) -> bool {
        self.app == app && self.schema == schema
    }
    /// Write into an unreachable, authority-owned stage only. The Store performs
    /// the final Admin/generation/CID checks and durable replacement decision.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn write_stage(
        &self,
        stage: &std::path::Path,
        primary: &str,
        collection: &str,
    ) -> Result<()> {
        use std::{fs, io::Write};
        if !crate::spaces::valid_id(collection)
            || !self.roots.iter().any(|(r, _, _)| r == primary)
            || !fs::symlink_metadata(stage).map_err(canvas::err)?.is_dir()
            || fs::read_dir(stage).map_err(canvas::err)?.next().is_some()
        {
            return Err("Invalid portable replacement stage".into());
        }
        let directory = stage.join(format!("collection-{collection}.data"));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(canvas::err)?;
        let write = |path: &std::path::Path, bytes: &[u8]| -> Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(path).map_err(canvas::err)?;
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(canvas::err)
        };
        for (state_ref, document, chat) in &self.roots {
            let suffix = if state_ref == primary {
                "primary".into()
            } else {
                canvas::hex(&canvas::hash(state_ref.as_bytes()))
            };
            let path = if state_ref == primary {
                stage.join(format!("collection-{collection}.merkle"))
            } else {
                directory.join(format!("doc-{suffix}.merkle"))
            };
            write(&path, &document.save()?)?;
            if let Some(chat) = chat {
                chat.write_new_journal(&directory.join(format!("chat-{suffix}.journal")))?;
            }
        }
        let assets = chat::assets::Assets::new(stage, collection)?;
        for (hash, bytes) in &self.assets {
            let asset = assets.put("attachment", bytes)?;
            if asset.sha256 != *hash {
                return Err("Portable asset stage mismatch".into());
            }
        }
        fs::File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(canvas::err)?;
        fs::File::open(stage)
            .and_then(|f| f.sync_all())
            .map_err(canvas::err)
    }
}
