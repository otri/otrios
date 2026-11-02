// SPDX-License-Identifier: LGPL-3.0-only
//! Confirmed immutable export pins. Hosts recheck admission before delivery.
use super::*;
use crate::{
    canvas::{self, Document, chat},
    spaces,
};
use otrios_interfaces::spaces::{AppPolicy, Registry, Role, RootSchema};
use std::collections::{BTreeMap, BTreeSet};

/// Authorize the exact active, preferred-detached or sealed dataset. Inactive
/// records never become mounts; knowing a collection/hash grants no access.
/// Invoke again at delivery after serialization outside the admission lock.
pub fn authorize_dataset(
    registry: &Registry,
    actor: &str,
    selected: &DatasetRef,
    installed: &AppPolicy,
) -> Result<()> {
    if !spaces::valid_identity(actor)
        || !spaces::valid_id(&selected.space)
        || !spaces::valid_id(&selected.collection)
        || selected.app != installed.app
        || selected.schema != installed.schema
    {
        return Err("Dataset identity/schema changed".into());
    }
    let space = registry
        .spaces
        .iter()
        .find(|s| s.id == selected.space)
        .filter(|s| spaces::role(s, Some(actor)) == Some(Role::Admin))
        .ok_or("Dataset Admin admission required")?;
    let policy = space
        .apps
        .iter()
        .find(|a| a.collection == selected.collection && a.policy.app == selected.app)
        .map(|a| &a.policy)
        .or_else(|| {
            registry
                .retained
                .iter()
                .find(|d| {
                    d.space == selected.space
                        && d.collection == selected.collection
                        && d.policy.app == selected.app
                })
                .map(|d| &d.policy)
        })
        .ok_or("Dataset binding changed")?;
    if policy.schema != installed.schema {
        return Err("Dataset schema unavailable".into());
    }
    Ok(())
}

/// Capture every distinct installed root under one authority admission lock.
/// A cloned Document and pinned history share immutable accepted snapshots;
/// subsequent confirmed writes cannot change what this export contains.
pub struct PinnedRoot {
    pub document: Document,
    pub chat: Option<chat::PinnedHistory>,
}
pub struct PinnedCollection {
    app: String,
    schema: String,
    roots: Vec<PinnedRoot>,
    assets: BTreeMap<String, Vec<u8>>,
}
impl PinnedCollection {
    /// `policies` come from verified installed code. Asset bytes are captured
    /// while the same scope is admitted, before reclamation can remove them.
    pub fn pin(
        app: &str,
        schema: &str,
        policies: &[RootPolicy],
        roots: Vec<PinnedRoot>,
        assets: BTreeMap<String, Vec<u8>>,
    ) -> Result<Self> {
        let trusted = policies
            .iter()
            .map(|p| RootSchema {
                state_ref: p.state_ref.clone(),
                schema: p.schema.clone(),
            })
            .collect::<Vec<_>>();
        if !otrios_interfaces::canvas::valid_destination(app)
            || app.contains('/')
            || crate::spaces::collection_schema(&trusted).map_err(canvas::err)? != schema
            || policies.len() != roots.len()
        {
            return Err("Export installed root/schema closure".into());
        }
        let mut seen = BTreeSet::new();
        let mut required = BTreeMap::<String, chat::Asset>::new();
        for root in &roots {
            let policy = policies
                .iter()
                .find(|p| p.state_ref == root.document.scope())
                .ok_or("Unexpected export root")?;
            if !seen.insert(root.document.scope())
                || root.document.schema() != &policy.schema
                || root.chat.is_some() != policy.chat
            {
                return Err("Export root identity/codec changed".into());
            }
            if let Some(history) = &root.chat {
                for (hash, asset) in history.assets()? {
                    if required
                        .get(&hash)
                        .is_some_and(|old| old.bytes != asset.bytes)
                    {
                        return Err("Conflicting export attachment".into());
                    }
                    required.insert(hash, asset);
                }
            }
        }
        if required.len() > 32 || required.len() != assets.len() {
            return Err("Export owned asset closure/budget".into());
        }
        for (hash, asset) in required {
            let bytes = assets.get(&hash).ok_or("Missing export owned asset")?;
            if bytes.len() != asset.bytes as usize || canvas::hex(&canvas::hash(bytes)) != hash {
                return Err("Export owned asset integrity".into());
            }
        }
        Ok(Self {
            app: app.into(),
            schema: schema.into(),
            roots,
            assets,
        })
    }

    /// May run outside the authority lock. This carries no mutable draft,
    /// admission receipt, session grant or source collection identity.
    pub fn encode(self) -> Result<Vec<u8>> {
        let mut roots = vec![];
        let mut checkpoint_bytes = COLLECTION_CHECKPOINT_HEADER.len() + 2;
        for root in &self.roots {
            let owned = OwnedRoot {
                state_ref: root.document.scope().into(),
                canvas: root.document.portable_checkpoint()?,
                chat: root
                    .chat
                    .as_ref()
                    .map(chat::PinnedHistory::encode)
                    .transpose()?,
            };
            checkpoint_bytes = checkpoint_bytes
                .checked_add(
                    2 + owned.state_ref.len()
                        + 4
                        + owned.canvas.len()
                        + 4
                        + owned.chat.as_ref().map_or(0, Vec::len),
                )
                .filter(|n| *n <= MAX_CHECKPOINT_BYTES)
                .ok_or("Export complete checkpoint exceeds its budget")?;
            roots.push(owned);
        }
        encode_data(
            DataInfo {
                format: DATA_FORMAT.into(),
                bundle_id: self.app,
                schema_sha256: self.schema,
                codec: COLLECTION_CODEC.into(),
                codec_version: 1,
                checkpoint: "State/checkpoint.bin".into(),
            },
            encode_collection(&roots)?,
            self.assets,
        )
    }
}
