// SPDX-License-Identifier: LGPL-3.0-only
//! OIB shares the app checkpoint, journal, receipts and publication root.
use super::super::oib::document::PropertyRegisterView;
use super::*;
use otrios_interfaces::oib::{
    Allocation, Change, InspectorAction, InstanceAction, InstanceOperation, MAX_ALLOCATIONS,
    MAX_CHANGES,
};
use std::collections::BTreeSet;
mod address;
mod baseline;
#[cfg(test)]
mod cache_tests;
mod hierarchy;
mod inspector;
mod lifecycle;
mod reset;
mod source;
mod structure;

/// One disposable compiled structural revision, shared by document clones and
/// keyed by signed admission identity and validated structural-source digest.
pub(super) struct AuthoredUi {
    signed: std::sync::Arc<Ui>,
    source: merkle::Hash,
    binding: merkle::Hash,
    ui: std::sync::Arc<Ui>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub generation: u64,
    pub structural_generation: u64,
    pub active_root: merkle::Hash,
    pub source_draft: merkle::Hash,
    pub conflicts: Vec<otrios_interfaces::oib::designer::PropertyAddress>,
    pub error: Option<String>,
}
/// One normalized immutable binding. The key includes admitted chrome identity
/// because pure graph materialization and admitted UI can use different renderer
/// bases. This cache is disposable and never changes a publication or wire byte.
pub(super) struct CachedBinding {
    leaf: merkle::Hash,
    ui: Option<std::sync::Arc<Ui>>,
    binding: std::sync::Arc<Binding>,
}

/// One canonically validated source, shared by snapshot clones. Source validity
/// also depends on signed baseline roles, so both immutable leaf identities and
/// the admitted UI identity participate in the key. No cache bytes are saved.
pub(super) struct CachedSource {
    leaf: merkle::Hash,
    binding: merkle::Hash,
    ui: Option<std::sync::Arc<Ui>>,
    source: std::sync::Arc<otrios_interfaces::oib::authoring::DocumentSource>,
    structural: merkle::Hash,
}

/// At most two successful compiler results (validation and selected presentation).
/// The semantic input digest deliberately
/// excludes draft/active causal metadata, so equivalent branches share a
/// compilation. Raw snapshot and journal validation still run independently.
pub(super) struct CachedCompilation {
    signed: std::sync::Arc<Ui>,
    binding: merkle::Hash,
    source: merkle::Hash,
    semantic: merkle::Hash,
    selected: Option<u64>,
    ui: Ui,
    layout: crate::dashboard::layout::Layout,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    format: String,
    identity_floor: u64,
    adoption: Option<baseline::Adoption>,
    seed: otrios_interfaces::oib::Seed,
    admission: Vec<u8>,
    ui_hash: Vec<u8>,
    ui_ref: String,
}
pub fn cell_path(branch: u64, id: u64) -> Vec<u64> {
    let mut p = vec![4, branch];
    p.extend((0..13).rev().map(|i| (id >> (i * 5)) & 31));
    p
}
impl Document {
    /// Warm transient compilation from the installed, verified UI before journal
    /// replay. Never initialize state or update a binding in the checkpoint's
    /// root chain. A compatible presentation change is configured after replay.
    pub fn prepare_oib_replay(&mut self, ui: &Ui) -> Result<()> {
        if self.tree.subtree(&[4]).is_err() {
            return Ok(());
        }
        let normalized = super::super::oib::graph::materialize(ui)?;
        let mut candidate = self.clone();
        candidate.oib_ui = Some(std::sync::Arc::new(normalized));
        candidate.oib_reconcile_ui()?;
        if candidate.oib_ui.is_some() {
            self.oib_ui = candidate.oib_ui;
            self.oib_authored = candidate.oib_authored;
        }
        Ok(())
    }

    pub fn configure_oib(&mut self, ui: &Ui) -> Result<bool> {
        let _profile = crate::canvas::profiling::span("oib.configure");
        if super::super::oib::seed(ui).is_none() {
            if self.tree.subtree(&[4]).is_ok() && self.oib_binding()?.ui_ref == ui.ui_ref {
                return Err("Installed app no longer supports retained OIB data".into());
            }
            return Ok(false);
        }
        let normalized = super::super::oib::graph::materialize(ui)?;
        let ui = &normalized;
        let seed = super::super::oib::seed(ui).ok_or("Missing canonical OIB graph")?;
        super::super::oib::validate(ui)?;
        let mut binding = Binding {
            format: baseline::FORMAT.into(),
            identity_floor: lifecycle::first_id(seed)?,
            adoption: None,
            seed: seed.clone(),
            admission: super::super::oib::contract::admission(ui)?,
            ui_hash: hash(&super::super::encode_ui(ui)?),
            ui_ref: ui.ui_ref.clone(),
        };
        let existing = payload(&self.tree, &[4, 0]).ok();
        if let Some(existing) = &existing {
            let previous: Binding = decode(existing)?;
            if !same_source(&previous, &binding) {
                return Err(
                    "OIB format or signed source changed; initialize a fresh collection".into(),
                );
            }
            binding.identity_floor = previous.identity_floor;
            binding.adoption = previous.adoption;
            if *existing == bytes(&binding)? {
                self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
                self.oib_compiled(2)?;
                return Ok(false);
            }
        }
        self.oib_ui = Some(std::sync::Arc::new(ui.clone()));
        let mut status = if existing.is_some() {
            self.oib_status()?
        } else {
            Status {
                generation: 1,
                structural_generation: 1,
                active_root: [0; 32],
                source_draft: [0; 32],
                conflicts: vec![],
                error: None,
            }
        };
        if existing.is_some() {
            status.generation = status
                .generation
                .checked_add(1)
                .ok_or("OIB generation exhausted")?;
        }
        let mut edits = vec![Edit {
            path: vec![4, 0],
            schema: self.extra_schema(4, 0),
            value: Some(bytes(&binding)?),
        }];
        if existing.is_none() {
            let allocations = super::super::oib::instances::initial_allocations(seed);
            let resolved = super::super::oib::instances::expanded_seed(seed, &allocations)?;
            for cell in &resolved.cells {
                // Missing overrides inherit the current authored source. Storing
                // its present default would freeze untouched properties.
                let value = None;
                for branch in [1, 2] {
                    edits.push(Edit {
                        path: address::cell_path(&resolved, branch, cell.id)?,
                        schema: address::schema(&self.scope, &resolved, cell.id)?,
                        value: value.clone(),
                    });
                }
            }
            let next = allocations
                .iter()
                .map(|a| a.id.saturating_add(1))
                .max()
                .unwrap_or(0)
                .max(lifecycle::first_id(seed)?);
            let mut presets = seed
                .graph
                .as_ref()
                .ok_or("Missing canonical graph")?
                .presets
                .clone();
            presets.sort_by_key(|preset| preset.id);
            let source = otrios_interfaces::oib::authoring::DocumentSource {
                definitions: seed
                    .graph
                    .as_ref()
                    .ok_or("Missing canonical graph")?
                    .definitions
                    .clone(),
                presets,
                instances: allocations.iter().map(Into::into).collect(),
                next_identity: next,
                baseline: vec![],
                placements: vec![],
                connections: vec![],
            };
            edits.push(Edit {
                path: vec![4, 4],
                schema: self.extra_schema(4, 4),
                value: Some(bytes(&source)?),
            });
            if seed.instance_surface.is_some() {
                for allocation in &allocations {
                    for branch in [5, 6] {
                        edits.push(Edit {
                            path: cell_path(branch, allocation.id),
                            schema: self.oib_instance_schema(allocation.id),
                            value: Some(bytes(&true)?),
                        });
                    }
                }
            }
        }
        self.change(&[], edits)?;
        status.active_root = self.oib_active_root()?;
        status.source_draft = self.oib_draft_root()?;
        self.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )?;
        Ok(true)
    }
    fn oib_binding(&self) -> Result<std::sync::Arc<Binding>> {
        let leaf = self.tree.subtree(&[4, 0]).map_err(err)?.hash();
        {
            let cache = self.oib_binding_cache.lock().map_err(err)?;
            if let Some(cached) = cache.as_ref()
                && cached.leaf == leaf
                && match (&cached.ui, &self.oib_ui) {
                    (Some(previous), Some(current)) => std::sync::Arc::ptr_eq(previous, current),
                    (None, None) => true,
                    _ => false,
                }
            {
                return Ok(cached.binding.clone());
            }
        }
        let _profile = crate::canvas::profiling::span("oib.binding.normalize");
        let mut binding: Binding = decode(&payload(&self.tree, &[4, 0])?)?;
        baseline::validate_metadata(&binding, None)?;
        if binding.format != baseline::FORMAT
            || binding.identity_floor != lifecycle::first_id(&binding.seed)?
        {
            return Err("Unsupported OIB baseline binding".into());
        }
        if let Some(ui) = &self.oib_ui
            && let Some(seed) = super::super::oib::seed(ui)
            && bytes(seed)? == bytes(&binding.seed)?
        {
            binding.seed = seed.clone();
        } else {
            binding.seed = super::super::oib::graph::materialize_seed(&binding.seed)?;
        }
        // The normalized signed seed is immutable for this cache key. Validate
        // new baseline bytes here, rather than once per property delta.
        super::super::oib::validate_seed(&binding.seed)?;
        let binding = std::sync::Arc::new(binding);
        *self.oib_binding_cache.lock().map_err(err)? = Some(CachedBinding {
            leaf,
            ui: self.oib_ui.clone(),
            binding: binding.clone(),
        });
        Ok(binding)
    }
    fn oib_authored_ui(&self) -> Result<std::sync::Arc<Ui>> {
        let _profile = crate::canvas::profiling::span("oib.authored_ui");
        let signed = self.oib_ui.clone().ok_or("Missing admitted OIB UI")?;
        if super::super::oib::seed(&signed).is_none_or(|seed| seed.designer.is_none()) {
            return Ok(signed);
        }
        let validated = self.oib_source_entry()?;
        let authored = &validated.source;
        let source = validated.structural;
        let binding = self.tree.subtree(&[4, 0]).map_err(err)?.hash();
        let mut cache = self.oib_authored.lock().map_err(err)?;
        if let Some(cached) = cache.as_ref()
            && std::sync::Arc::ptr_eq(&signed, &cached.signed)
            && cached.source == source
            && cached.binding == binding
        {
            return Ok(cached.ui.clone());
        }
        let ui = std::sync::Arc::new(super::super::oib::graph::with_source(
            &signed,
            &authored.definitions,
            &authored.presets,
        )?);
        *cache = Some(AuthoredUi {
            signed,
            source,
            binding,
            ui: ui.clone(),
        });
        Ok(ui)
    }
    fn oib_authored_seed(&self) -> Result<otrios_interfaces::oib::Seed> {
        let _profile = crate::canvas::profiling::span("oib.authored_seed");
        let mut seed = if self.oib_ui.is_some() {
            super::super::oib::seed(&*self.oib_authored_ui()?)
                .ok_or("Missing OIB seed")?
                .clone()
        } else {
            let signed = self.oib_binding()?.seed.clone();
            let authored = self.oib_source_shared()?;
            super::super::oib::graph::with_seed_source(
                &signed,
                &authored.definitions,
                &authored.presets,
            )?
        };
        if let Some(doc) = &mut seed.designer {
            doc.placements = self.oib_placements()?;
            doc.connections = self.oib_connections()?;
        }
        Ok(seed)
    }
    /// Collection incarnation plus signed source. Fresh initialization cannot
    /// accept an old client's still-in-flight property or structural intent.
    pub fn oib_epoch(&self) -> Result<Vec<u8>> {
        let binding = self.oib_binding()?;
        Ok(hash(&bytes(&(
            "otrios-oib-epoch-v3",
            self.actor,
            &self.scope,
            &binding.ui_ref,
            binding
                .seed
                .graph
                .as_ref()
                .ok_or("Missing canonical graph")?
                .id,
            &binding.admission,
        ))?))
    }
    /// Read a semantic register through a disposable compiler handle. The handle
    /// is resolved anew; it is never the register key or a state request field.
    pub fn oib_property_leaf(&self, branch: u64, cell: u64) -> Result<Snapshot> {
        self.tree
            .subtree(&address::cell_path(
                &self.oib_resolved_seed()?,
                branch,
                cell,
            )?)
            .map_err(err)
    }
    pub(super) fn check_oib_epoch(&self, epoch: &[u8]) -> Result<()> {
        if epoch != self.oib_epoch()? {
            return Err("OIB document changed; reconnect before editing".into());
        }
        Ok(())
    }
    pub fn oib_status(&self) -> Result<Status> {
        decode(&payload(&self.tree, &[4, 3])?)
    }
    fn oib_instance_schema(&self, id: u64) -> merkle::Hash {
        digest(
            &[
                b"OTriOS\0oib-instance-v1\0".as_slice(),
                self.scope.as_bytes(),
                &id.to_le_bytes(),
            ]
            .concat(),
        )
    }
    pub fn oib_allocations(&self) -> Result<Vec<Allocation>> {
        let source = self.oib_source_shared()?;
        let mut ids: Vec<_> = source
            .instances
            .iter()
            .map(|instance| instance.id)
            .collect();
        ids.sort_unstable();
        source
            .instances
            .iter()
            .map(|instance| {
                let slot = ids
                    .iter()
                    .position(|id| *id == instance.id)
                    .ok_or("Unknown resident OIB identity")? as u16;
                Ok(Allocation {
                    id: instance.id,
                    name: instance.name.clone(),
                    component: instance.component,
                    prototype: instance.prototype,
                    slot,
                })
            })
            .collect()
    }
    pub fn oib_live(&self, branch: u64) -> Result<BTreeSet<u64>> {
        let seed = if let Some(ui) = &self.oib_ui {
            super::super::oib::seed(ui)
                .ok_or("Missing OIB seed")?
                .clone()
        } else {
            self.oib_binding()?.seed.clone()
        };
        if seed.instance_surface.is_none() {
            return Ok(seed.instances.iter().map(|i| i.id).collect());
        }
        let mut live = BTreeSet::new();
        for a in self.oib_allocations()? {
            let leaf = match self
                .tree
                .subtree(&cell_path(if branch == 1 { 5 } else { 6 }, a.id))
            {
                Ok(leaf) => leaf,
                Err(_) if branch == 2 => continue,
                Err(e) => return Err(err(e)),
            };
            let register = leaf.register().ok_or("Missing instance presence")?;
            // Removal wins: editing a retained property cannot resurrect an object.
            if register.values().all(|(_, _, v)| v.is_some()) {
                live.insert(a.id);
            }
        }
        Ok(live)
    }
    fn oib_resolved_seed(&self) -> Result<otrios_interfaces::oib::Seed> {
        let _profile = crate::canvas::profiling::span("oib.seed.resolve");
        let seed = self.oib_authored_seed()?;
        super::super::oib::instances::expanded_seed(&seed, &self.oib_allocations()?)
    }
    fn oib_active_root(&self) -> Result<merkle::Hash> {
        let properties = self.oib_property_root(2)?;
        if self.tree.subtree(&[4, 6]).is_err() {
            return self.oib_structural_root(properties, 2);
        }
        let instances = self.tree.subtree(&[4, 6]).map_err(err)?.hash();
        let root = digest(
            &[
                b"OTriOS\0oib-active-v2\0".as_slice(),
                &properties,
                &instances,
            ]
            .concat(),
        );
        self.oib_structural_root(root, 2)
    }
    fn oib_draft_root(&self) -> Result<merkle::Hash> {
        let properties = self.oib_property_root(1)?;
        let Ok(instances) = self.tree.subtree(&[4, 5]) else {
            return self.oib_structural_root(properties, 1);
        };
        let root = digest(
            &[
                b"OTriOS\0oib-draft-v2\0".as_slice(),
                &properties,
                &instances.hash(),
            ]
            .concat(),
        );
        self.oib_structural_root(root, 1)
    }
    fn oib_property_root(&self, branch: u64) -> Result<merkle::Hash> {
        match self.tree.subtree(&[4, branch]) {
            Ok(tree) => Ok(tree.hash()),
            Err(merkle::Error::Missing) => Ok(Snapshot::empty().hash()),
            Err(error) => Err(err(error)),
        }
    }
    fn oib_structural_root(
        &self,
        properties_and_presence: merkle::Hash,
        branch: u64,
    ) -> Result<merkle::Hash> {
        let _profile = crate::canvas::profiling::span("oib.structural_root");
        let mut source = self.oib_source()?;
        let live = self.oib_live(branch)?;
        source
            .instances
            .retain(|instance| live.contains(&instance.id));
        // The allocation cursor and retained inactive descriptors are residency
        // bookkeeping. They cannot change the published graph of a slow reader.
        source.next_identity = 0;
        source.baseline.clear();
        Ok(digest(
            &[
                b"OTriOS\0oib-document-v2\0".as_slice(),
                &properties_and_presence,
                &digest(&bytes(&source)?),
                &baseline::digest(self.oib_binding()?.as_ref())?,
            ]
            .concat(),
        ))
    }
    pub fn oib_compiled(&self, branch: u64) -> Result<(Ui, crate::dashboard::layout::Layout)> {
        self.oib_compiled_selected(branch, None)
    }
    pub fn oib_compiled_selected(
        &self,
        branch: u64,
        selected: Option<u64>,
    ) -> Result<(Ui, crate::dashboard::layout::Layout)> {
        let _profile = crate::canvas::profiling::span("oib.compiled");
        let signed = self.oib_ui.clone().ok_or("Missing admitted OIB UI")?;
        let binding = self.tree.subtree(&[4, 0]).map_err(err)?.hash();
        let source = self.tree.subtree(&[4, 4]).map_err(err)?.hash();
        let values = self.oib_values(branch)?;
        let live = self.oib_live(branch)?;
        // Ordered vectors retain exact encoded values (including numeric
        // representations), rather than relying on float PartialEq or causal
        // register identities that differ between equivalent draft/active data.
        let semantic = digest(&bytes(&(
            live.iter().collect::<Vec<_>>(),
            values.iter().collect::<Vec<_>>(),
        ))?);
        let same_key = |cached: &CachedCompilation| {
            std::sync::Arc::ptr_eq(&signed, &cached.signed)
                && cached.binding == binding
                && cached.source == source
                && cached.semantic == semantic
                && cached.selected == selected
        };
        {
            let mut cache = self.oib_compiled_cache.lock().map_err(err)?;
            if let Some(index) = cache.iter().position(same_key) {
                let _profile = crate::canvas::profiling::span("oib.compiled.hit");
                let cached = cache.remove(index);
                let result = (cached.ui.clone(), cached.layout.clone());
                cache.push(cached);
                return Ok(result);
            }
        }
        let _profile = crate::canvas::profiling::span("oib.compiled.miss");
        let mut ui = (*self.oib_authored_ui()?).clone();
        if let Some(Presentation::Oib(seed)) = &mut ui.presentation
            && let Some(doc) = &mut seed.designer
        {
            doc.placements = self.oib_placements()?;
            doc.connections = self.oib_connections()?;
        }
        let resolved = super::super::oib::instances::expand(
            &ui,
            &self.oib_allocations()?,
            &live,
            selected,
            &values,
        )?;
        let layout = super::super::oib::compile(&resolved, &values)?;
        let mut cache = self.oib_compiled_cache.lock().map_err(err)?;
        // Sibling snapshots share this small MRU. Never accumulate historical
        // revisions, even when two owners compile concurrently.
        cache.retain(|cached| !same_key(cached));
        if cache.len() == 2 {
            cache.remove(0);
        }
        cache.push(CachedCompilation {
            signed,
            binding,
            source,
            semantic,
            selected,
            ui: resolved.clone(),
            layout: layout.clone(),
        });
        Ok((resolved, layout))
    }
    pub fn oib_values(&self, branch: u64) -> Result<BTreeMap<u64, Option<Value>>> {
        let _profile = crate::canvas::profiling::span("oib.values");
        let seed = self.oib_resolved_seed()?;
        let live = self.oib_live(branch)?;
        let mut values = BTreeMap::new();
        for cell in &seed.cells {
            let leaf = match self
                .tree
                .subtree(&address::cell_path(&seed, branch, cell.id)?)
            {
                Ok(leaf) => leaf,
                Err(_) if branch == 2 && cell.inherit.is_some() => {
                    values.insert(cell.id, None);
                    continue;
                }
                Err(e) => return Err(err(e)),
            };
            let register = leaf.register().ok_or("Missing OIB register")?;
            let mut selected = None;
            let mut first = true;
            for (_, _, data) in register.values() {
                let value = data.map(decode::<Value>).transpose()?;
                if !first && selected != value {
                    if branch == 1 && cell.inherit.is_some() && !live.contains(&cell.owner) {
                        selected = None;
                        break;
                    }
                    return Err("Unresolved OIB conflict".into());
                }
                selected = value;
                first = false;
            }
            values.insert(cell.id, selected);
        }
        Ok(values)
    }
    pub(super) fn validate_oib(&self, old: Option<&Snapshot>, tree: &Snapshot) -> Result<()> {
        let _profile = crate::canvas::profiling::span("oib.validate");
        let baseline_changed = if let Some(old) = old {
            self.oib_validate_baseline_transition(old, tree)?
        } else {
            false
        };
        let mut candidate = self.clone();
        candidate.tree = tree.clone();
        if baseline_changed {
            candidate.oib_reconcile_ui()?;
        }
        let binding = candidate.oib_binding()?;
        if binding.ui_hash.len() != 32
            || binding.admission.len() != 32
            || binding.ui_ref.is_empty()
            || binding.ui_ref.len() > 128
        {
            return Err("Invalid OIB seed digest".into());
        }
        candidate.validate_allocations(old)?;
        let resolved = candidate.oib_resolved_seed()?;
        let seed = &resolved;
        let branch = tree.subtree(&[4]).map_err(err)?;
        if branch.register().is_some()
            || branch.child_keys().any(|k| {
                k > if binding.seed.instance_surface.is_some() {
                    6
                } else {
                    4
                }
            })
        {
            return Err("Invalid OIB branch".into());
        }
        for index in [0, 3, 4] {
            let leaf = tree.subtree(&[4, index]).map_err(err)?;
            if leaf.child_keys().next().is_some()
                || leaf
                    .register()
                    .is_none_or(|r| r.schema() != self.extra_schema(4, index as usize))
            {
                return Err("Invalid OIB metadata".into());
            }
        }
        candidate.oib_source()?;
        let total = [1, 2, 4, 5, 6]
            .iter()
            .filter_map(|kind| tree.subtree(&[4, *kind]).ok())
            .map(|t| t.canonical_bytes())
            .sum::<usize>()
            .checked_add(bytes(&binding.seed.resolved_libraries)?.len())
            .ok_or("OIB resident byte count overflow")?;
        if total > otrios_interfaces::oib::MAX_BYTES {
            return Err("OIB combined authored/causal byte budget".into());
        }
        let status = candidate.oib_status()?;
        baseline::validate_metadata(&binding, Some(&status))?;
        if status.generation == 0
            || status.structural_generation == 0
            || status.structural_generation > status.generation
            || status.conflicts.len() > 64
            || status.error.as_ref().is_some_and(|s| s.len() > 256)
            || status.conflicts.iter().any(|address| {
                !seed.cells.iter().any(|c| {
                    super::super::oib::document::address_for_cell(seed, c.id)
                        .ok()
                        .as_ref()
                        == Some(address)
                })
            })
        {
            return Err("Invalid OIB status".into());
        }
        for kind in [1, 2] {
            let branch = match tree.subtree(&[4, kind]) {
                Ok(branch) => Some(branch),
                // Live read-only coverage omits the draft and its presence
                // branch entirely. A full checkpoint must still contain every
                // admitted property; omission is not an empty authoring table.
                Err(merkle::Error::Missing)
                    if kind == 1
                        && candidate.oib_projection
                        && matches!(tree.subtree(&[4, 5]), Err(merkle::Error::Missing)) =>
                {
                    continue;
                }
                Err(merkle::Error::Missing) => None,
                Err(merkle::Error::Forbidden) => continue,
                Err(error) => return Err(err(error)),
            };
            let mut seen = BTreeSet::new();
            if let Some(branch) = &branch {
                address::leaves(branch, &mut vec![4, kind], &mut seen)?;
            }
            let expected: BTreeSet<_> = seed
                .cells
                .iter()
                .filter(|c| {
                    kind == 1
                        || seed.instance_surface.is_none()
                        || c.inherit.is_none()
                        || tree.subtree(&cell_path(6, c.owner)).is_ok()
                })
                .map(|c| address::cell_path(seed, kind, c.id))
                .collect::<Result<_>>()?;
            if seen != expected {
                return Err("OIB property table changed".into());
            }
            if kind == 1
                && branch.as_ref().is_some_and(|branch| {
                    branch.canonical_bytes() > otrios_interfaces::oib::MAX_BYTES
                })
            {
                return Err("OIB draft byte budget".into());
            }
            for cell in &seed.cells {
                if kind == 2 && !seen.contains(&address::cell_path(seed, kind, cell.id)?) {
                    continue;
                }
                let leaf = tree
                    .subtree(&address::cell_path(seed, kind, cell.id)?)
                    .map_err(err)?;
                let r = leaf.register().ok_or("Missing OIB cell")?;
                if r.schema() != address::schema(&self.scope, seed, cell.id)? {
                    return Err("Wrong OIB cell schema".into());
                }
                for (_, _, value) in r.values() {
                    if let Some(value) = value {
                        let value = decode(value)?;
                        super::super::oib::validate_value(cell, &value)?;
                        super::super::oib::document::validate_edit(seed, cell.id, Some(&value))?;
                    }
                }
            }
            if kind == 2 {
                if status.active_root != candidate.oib_active_root()? {
                    return Err("OIB active closure mismatch".into());
                }
                candidate.oib_values(2)?;
                if self.oib_ui.is_some() {
                    candidate.oib_compiled(2)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn oib_write(&mut self, writer: Actor, changes: &[Change]) -> Result<()> {
        let _profile = crate::canvas::profiling::span("oib.write");
        if changes.is_empty() || changes.len() > MAX_CHANGES {
            return Err("OIB change batch limit".into());
        }
        let seed = self.oib_resolved_seed()?;
        let mut next = self.tree.clone();
        let mut seen = BTreeSet::new();
        for change in changes {
            if self.oib_retired_property(&change.address)? {
                continue;
            }
            let id = super::super::oib::document::cell_for_address(
                &seed,
                &self.oib_allocations()?,
                &change.address,
            )?;
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == id)
                .ok_or("Unknown OIB cell")?;
            if !seen.insert(cell.id) {
                return Err("Duplicate OIB cell edit".into());
            }
            super::super::oib::document::validate_edit(&seed, cell.id, change.value.as_ref())?;
            if let Some(value) = &change.value {
                super::super::oib::validate_value(cell, value)?;
            }
            let observed = Snapshot::from_bytes(&change.observed).map_err(err)?;
            let edit = Edit {
                path: address::cell_path(&seed, 1, cell.id)?,
                schema: address::schema(&self.scope, &seed, cell.id)?,
                value: change.value.as_ref().map(bytes).transpose()?,
            };
            next = next.edit_observed(writer, &edit, &observed).map_err(err)?;
        }
        if seen.is_empty() {
            return Ok(());
        }
        self.activate_oib(next)
    }
    fn activate_oib(&mut self, next: Snapshot) -> Result<()> {
        let _profile = crate::canvas::profiling::span("oib.activate");
        let seed = self.oib_resolved_seed()?;
        let mut status = self.oib_status()?;
        status.conflicts.clear();
        status.error = None;
        let mut draft = self.clone();
        draft.tree = next;
        let live = draft.oib_live(1)?;
        for cell in &seed.cells {
            if cell.inherit.is_some() && !live.contains(&cell.owner) {
                continue;
            }
            let leaf = draft
                .tree
                .subtree(&address::cell_path(&seed, 1, cell.id)?)
                .map_err(err)?;
            let alternatives: BTreeSet<_> = leaf
                .register()
                .ok_or("Missing OIB cell")?
                .values()
                .map(|(_, _, v)| v)
                .collect();
            if alternatives.len() > 1 {
                status
                    .conflicts
                    .push(super::super::oib::document::address_for_cell(
                        &seed, cell.id,
                    )?);
            }
        }
        let mut edits = Vec::new();
        if status.conflicts.is_empty() {
            let values = draft.oib_values(1)?;
            match draft.oib_compiled(1) {
                Ok(_) => {
                    status.generation = status
                        .generation
                        .checked_add(1)
                        .ok_or("OIB generation exhausted")?;
                    status.source_draft = draft.oib_draft_root()?;
                    if seed.instance_surface.is_some() {
                        for allocation in draft.oib_allocations()? {
                            let value = live
                                .contains(&allocation.id)
                                .then(|| bytes(&true))
                                .transpose()?;
                            let matches = self
                                .tree
                                .subtree(&cell_path(6, allocation.id))
                                .ok()
                                .and_then(|leaf| {
                                    leaf.register()
                                        .and_then(|r| r.values().next())
                                        .map(|(_, _, v)| v == value.as_deref())
                                })
                                .unwrap_or(false);
                            if !matches {
                                edits.push(Edit {
                                    path: cell_path(6, allocation.id),
                                    schema: self.oib_instance_schema(allocation.id),
                                    value,
                                });
                            }
                        }
                    }
                    for cell in &seed.cells {
                        let value = values[&cell.id].as_ref().map(bytes).transpose()?;
                        if self
                            .tree
                            .subtree(&address::cell_path(&seed, 2, cell.id)?)
                            .is_ok_and(|old| {
                                old.register().is_some_and(|r| {
                                    r.values()
                                        .next()
                                        .is_some_and(|(_, _, v)| v == value.as_deref())
                                })
                            })
                        {
                            continue;
                        }
                        edits.push(Edit {
                            path: address::cell_path(&seed, 2, cell.id)?,
                            schema: address::schema(&self.scope, &seed, cell.id)?,
                            value,
                        });
                    }
                }
                Err(e) => status.error = Some(e.chars().take(256).collect()),
            }
        }
        draft.change(&[], edits)?;
        draft.oib_reclaim()?;
        if status.error.is_none() && status.conflicts.is_empty() {
            status.source_draft = draft.oib_draft_root()?;
        }
        status.active_root = draft.oib_active_root()?;
        draft.change(
            &[],
            vec![Edit {
                path: vec![4, 3],
                schema: self.extra_schema(4, 3),
                value: Some(bytes(&status)?),
            }],
        )?;
        draft.validate_oib(Some(&self.tree), &draft.tree)?;
        self.tree = draft.tree;
        Ok(())
    }
}

impl StateAdapter {
    pub(in crate::canvas) fn oib_saving(&self) -> bool {
        self.flight
            .as_ref()
            .is_some_and(|f| !f.oib.is_empty() || f.oib_instance.is_some())
    }
    pub(in crate::canvas) fn oib_root(&self) -> Option<merkle::Hash> {
        self.doc.tree.subtree(&[4]).ok().map(|n| n.hash())
    }
    pub(in crate::canvas) fn oib_info(&self) -> Result<super::super::oib::Info> {
        let _profile = crate::canvas::profiling::span("oib.info");
        let resolved = self.doc.oib_resolved_seed()?;
        let seed = &resolved;
        let status = self.doc.oib_status()?;
        let values = self.doc.oib_values(2)?;
        let mut properties = BTreeMap::new();
        for cell in &seed.cells {
            properties.insert(cell.id, property_text(seed, cell, &status, &values)?);
        }
        Ok(super::super::oib::Info {
            semantic_epoch: super::super::hex(&self.doc.oib_epoch()?),
            baseline_lock: super::super::hex(&self.doc.oib_baseline()?.1),
            libraries: self
                .doc
                .oib_binding()?
                .seed
                .graph
                .as_ref()
                .and_then(|graph| graph.imports.as_ref())
                .map_or_else(Vec::new, |imports| imports.locks.clone()),
            active_generation: status.generation,
            presented_generation: 0,
            conflicts: self.oib_conflicts(),
            status: self.oib_status_text(),
            properties,
            instances: self
                .doc
                .oib_allocations()?
                .into_iter()
                .map(|a| super::super::oib::InstanceInfo {
                    active: self.doc.oib_live(2).is_ok_and(|live| live.contains(&a.id)),
                    id: a.id,
                    name: a.name,
                    component: a.component,
                    prototype: a.prototype,
                })
                .collect(),
            selected_instance: None,
            available_instance_slots: self.oib_available_slots(),
        })
    }
    pub(in crate::canvas) fn admitted_oib_hash(&self) -> Option<Vec<u8>> {
        self.doc.oib_binding().ok().map(|b| b.ui_hash.clone())
    }
    pub fn oib_instances(&self) -> Result<Vec<super::super::oib::InstanceInfo>> {
        Ok(self.oib_info()?.instances)
    }
    pub fn oib_conflicts(&self) -> Vec<u64> {
        let Ok(seed) = self.doc.oib_resolved_seed() else {
            return vec![];
        };
        let Ok(status) = self.doc.oib_status() else {
            return vec![];
        };
        seed.cells
            .iter()
            .filter_map(|cell| {
                let address = super::super::oib::document::address_for_cell(&seed, cell.id).ok()?;
                status.conflicts.contains(&address).then_some(cell.id)
            })
            .collect()
    }
    pub fn oib_property_text(&self, id: u64) -> String {
        let result = (|| -> Result<String> {
            let resolved = self.doc.oib_resolved_seed()?;
            let seed = &resolved;
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == id)
                .ok_or("Unknown OIB property")?;
            let status = self.doc.oib_status()?;
            let values = self.doc.oib_values(2)?;
            property_text(seed, cell, &status, &values)
        })();
        result.unwrap_or_else(|_| "UI unavailable".into())
    }

    pub fn oib_generation(&self) -> Option<u64> {
        self.doc.oib_status().ok().map(|s| s.generation)
    }
    pub fn oib_structural_generation(&self) -> Option<u64> {
        self.doc
            .oib_status()
            .ok()
            .map(|status| status.structural_generation)
    }

    pub fn oib_active(&self) -> Result<BTreeMap<u64, Option<Value>>> {
        self.doc.oib_values(2)
    }
    pub(in crate::canvas) fn oib_compiled(
        &self,
        selected: Option<u64>,
    ) -> Result<(Ui, crate::dashboard::layout::Layout)> {
        self.doc.oib_compiled_selected(2, selected)
    }
    pub fn oib_status_text(&self) -> String {
        let Ok(status) = self.doc.oib_status() else {
            return "Live UI unavailable".into();
        };
        if !status.conflicts.is_empty() {
            format!(
                "{} conflicting properties — resolve in inspector",
                status.conflicts.len()
            )
        } else if let Some(error) = &status.error {
            format!("Draft: {}", error.chars().take(84).collect::<String>())
        } else if self.oib_saving() {
            "Saving UI change…".into()
        } else if self
            .doc
            .oib_binding()
            .is_ok_and(|b| b.seed.instance_surface.is_some())
        {
            format!(
                "Live · revision {} · {} instances · {} slots available",
                status.generation,
                self.doc.oib_live(2).map_or(0, |v| v.len()),
                self.oib_available_slots()
            )
        } else {
            format!("Live · revision {}", status.generation)
        }
    }

    pub fn edit_oib(&mut self, changes: &[(u64, Option<Value>)]) -> Result<bool> {
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        if changes.is_empty() || changes.len() > MAX_CHANGES {
            return Err("OIB change batch limit".into());
        }
        let resolved = self.doc.oib_resolved_seed()?;
        let seed = &resolved;
        let mut edits = Vec::new();
        let mut seen = BTreeSet::new();
        for (id, value) in changes {
            if !seen.insert(*id) {
                return Err("Duplicate OIB cell edit".into());
            }
            let cell = seed
                .cells
                .iter()
                .find(|c| c.id == *id)
                .ok_or("Unknown OIB property")?;
            super::super::oib::document::validate_edit(seed, *id, value.as_ref())?;
            if let Some(value) = value {
                super::super::oib::validate_value(cell, value)?;
            }
            edits.push(Change {
                address: super::super::oib::document::address_for_cell(seed, *id)?,
                value: value.clone(),
                observed: self
                    .doc
                    .tree
                    .subtree(&address::cell_path(seed, 1, *id)?)
                    .map_err(err)?
                    .to_bytes()
                    .map_err(err)?,
            });
        }
        // This is an intent, not an optimistic UI activation. Its observed
        // registers come directly from the admitted replica; the authority
        // compiles/validates the draft and journals the active result atomically.
        // Repeating that work here only blocks delivery of the button release.
        self.flight = Some(Flight {
            oib_epoch: Some(self.doc.oib_epoch()?),
            sequence: self
                .acknowledged
                .checked_add(1)
                .ok_or("Intent sequence exhausted")?,
            values: vec![],
            oib: edits,
            oib_instance: None,
            sent: false,
            issued_at: self.clock,
        });
        Ok(true)
    }
    pub fn oib_address_action(
        &mut self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
        action: InspectorAction,
    ) -> Result<bool> {
        if let otrios_interfaces::oib::designer::PropertyAddress::Occurrence { instance, .. } =
            address
            && self.doc.oib_retired_instance(instance.0)?
        {
            return Ok(false);
        }
        let seed = self.doc.oib_resolved_seed()?;
        let cell = super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        self.oib_inspector_action(cell, action)
    }
    pub fn oib_inspector_action(&mut self, id: u64, action: InspectorAction) -> Result<bool> {
        let resolved = self.doc.oib_resolved_seed()?;
        let seed = &resolved;
        let cell = seed
            .cells
            .iter()
            .find(|c| c.id == id)
            .ok_or("Unknown OIB cell")?;
        let value = match action {
            InspectorAction::Clear => None,
            InspectorAction::Resolve => Some(
                cell.authored_override
                    .as_ref()
                    .unwrap_or(&cell.default)
                    .clone(),
            ),
            InspectorAction::Cycle => {
                let active = self.doc.oib_values(2)?;
                let current = super::super::oib::effective(seed, &active, id)?;
                if cell.choices.is_empty() {
                    return Err("No inspector choices declared for this property".into());
                }
                let index = cell
                    .choices
                    .iter()
                    .position(|v| *v == current)
                    .map_or(0, |i| (i + 1) % cell.choices.len());
                Some(cell.choices[index].clone())
            }
        };
        self.edit_oib(&[(id, value)])
    }
}

fn property_text(
    seed: &otrios_interfaces::oib::Seed,
    cell: &otrios_interfaces::oib::Cell,
    status: &Status,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<String> {
    if status
        .conflicts
        .contains(&super::super::oib::document::address_for_cell(
            seed, cell.id,
        )?)
    {
        return Ok(format!("{}: conflict", cell.name));
    }
    let value = match super::super::oib::effective(seed, values, cell.id)? {
        Value::Text(v) => v,
        Value::Number(v) => format!("{v:.0}"),
        Value::Color(c) => format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]),
        Value::Boolean(v) => v.to_string(),
        _ => String::new(),
    };
    let kind = if cell.inherit.is_none() {
        "default"
    } else if values[&cell.id].is_some() {
        "override"
    } else if cell.authored_override.is_some() {
        "authored"
    } else {
        "inherited"
    };
    Ok(format!("{}: {} ({})", cell.name, value, kind))
}

fn same_source(old: &Binding, new: &Binding) -> bool {
    old.ui_ref == new.ui_ref
        && bytes(&old.seed)
            .ok()
            .zip(bytes(&new.seed).ok())
            .is_some_and(|(old, new)| old == new)
        && old.admission == new.admission
}
fn admitted_instance_name(seed: &otrios_interfaces::oib::Seed, name: &str) -> bool {
    if seed.designer.is_some() {
        !name.is_empty() && name.len() <= 64 && !name.contains('\0')
    } else {
        instance_name(name)
    }
}
fn instance_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn trie_ids(tree: &Snapshot, depth: usize, id: u64, seen: &mut BTreeSet<u64>) -> Result<()> {
    if depth == 13 {
        if tree.register().is_none() || tree.child_keys().next().is_some() || !seen.insert(id) {
            return Err("Invalid instance leaf".into());
        }
    } else {
        if tree.register().is_some() {
            return Err("Invalid instance trie".into());
        }
        for key in tree.child_keys() {
            if key > 31 || (depth == 0 && key > 15) {
                return Err("Invalid instance ID digit".into());
            }
            trie_ids(
                &tree.subtree(&[key]).map_err(err)?,
                depth + 1,
                (id << 5) | key,
                seen,
            )?;
        }
    }
    Ok(())
}
/// Private allocator primitives. Only EditBatch crosses the state protocol.
enum InstanceStage {
    Insert {
        prototype: u64,
        name: Option<String>,
    },
    Duplicate {
        instance: u64,
        name: Option<String>,
    },
    Remove {
        instance: u64,
    },
}
impl Document {
    fn validate_allocations(&self, old: Option<&Snapshot>) -> Result<()> {
        let seed = self.oib_binding()?.seed.clone();
        if seed.instance_surface.is_none() {
            let source = self.oib_source()?;
            let initial: Vec<otrios_interfaces::oib::authoring::InstanceRecord> =
                super::super::oib::instances::initial_allocations(&seed)
                    .iter()
                    .map(Into::into)
                    .collect();
            if source.instances != initial
                || source.next_identity != self.oib_binding()?.identity_floor
                || !source.placements.is_empty()
                || !source.connections.is_empty()
            {
                return Err("Invalid fixed OIB document source".into());
            }
            return Ok(());
        }
        self.validate_resident_allocations(old)
    }
    pub(super) fn oib_instance_write(
        &mut self,
        writer: Actor,
        batch: &InstanceOperation,
    ) -> Result<()> {
        self.oib_structure_write(writer, batch)
    }
    fn oib_instance_stage(&mut self, operation: &InstanceStage) -> Result<()> {
        let mut candidate = self.clone();
        let mut allocations = self.oib_allocations()?;
        match operation {
            InstanceStage::Remove { instance } => {
                if self.oib_retired_instance(*instance)? {
                    return Ok(());
                }
                if !allocations.iter().any(|a| a.id == *instance) {
                    return Err("Unknown instance".into());
                }
                candidate.change(
                    &[],
                    vec![Edit {
                        path: cell_path(5, *instance),
                        schema: self.oib_instance_schema(*instance),
                        value: None,
                    }],
                )?;
            }
            InstanceStage::Insert { .. } | InstanceStage::Duplicate { .. } => {
                let ui = self.oib_authored_ui()?;
                let seed = super::super::oib::seed(&ui).ok_or("Missing authored graph")?;

                if allocations.len() >= MAX_ALLOCATIONS {
                    return Err("OIB retained identity capacity exhausted".into());
                }
                let (component, prototype, source, name) = match operation {
                    InstanceStage::Insert { prototype, name } => {
                        let source = seed
                            .instances
                            .iter()
                            .find(|i| i.id == *prototype)
                            .ok_or("Undeclared signed preset")?;
                        (
                            source.component.ok_or("Missing preset component")?,
                            *prototype,
                            None,
                            name,
                        )
                    }
                    InstanceStage::Duplicate { instance, name } => {
                        let source = allocations
                            .iter()
                            .find(|a| a.id == *instance)
                            .ok_or("Unknown source instance")?;
                        if !self.oib_live(1)?.contains(instance) {
                            return Err("Source instance is not live".into());
                        }
                        (source.component, source.prototype, Some(*instance), name)
                    }
                    InstanceStage::Remove { .. } => unreachable!(),
                };
                let new = candidate.oib_allocate(&allocations, prototype, name.clone())?;
                debug_assert_eq!(new.component, component);
                let id = new.id;
                allocations.push(new.clone());
                let expanded =
                    super::super::oib::instances::expanded_seed(seed, std::slice::from_ref(&new))?;
                let mut edits = vec![
                    candidate.oib_source_edit(|source| {
                        source.instances = allocations.iter().map(Into::into).collect()
                    })?,
                    Edit {
                        path: cell_path(5, id),
                        schema: self.oib_instance_schema(id),
                        value: Some(bytes(&true)?),
                    },
                ];
                for cell in expanded.cells.iter().filter(|c| c.owner == id) {
                    let value = if let Some(source) = source {
                        let metadata = expanded
                            .designer
                            .as_ref()
                            .ok_or("Missing occurrence metadata")?;
                        let path = metadata
                            .mappings
                            .roots
                            .iter()
                            .find(|table| table.prototype.0 == id)
                            .and_then(|table| {
                                table.cells.iter().find(|row| row.initial_cell == cell.id)
                            })
                            .ok_or("Missing copied property path")?
                            .path
                            .clone();
                        let source_address =
                            otrios_interfaces::oib::designer::PropertyAddress::Occurrence {
                                instance: otrios_interfaces::oib::designer::InstanceId(source),
                                path,
                            };
                        match self.tree.subtree(&address::path(1, &source_address)?) {
                            Ok(leaf) => {
                                let register =
                                    leaf.register().ok_or("Missing copied property register")?;
                                let mut values = register.values();
                                let value = values
                                    .next()
                                    .ok_or("Empty copied property register")?
                                    .2
                                    .map(<[u8]>::to_vec);
                                if values.any(|(_, _, other)| other != value.as_deref()) {
                                    return Err("Resolve copied property conflicts first".into());
                                }
                                value
                            }
                            Err(merkle::Error::Missing) => None,
                            Err(error) => return Err(err(error)),
                        }
                    } else {
                        None
                    };
                    edits.push(Edit {
                        path: address::cell_path(&expanded, 1, cell.id)?,
                        schema: address::schema(&self.scope, &expanded, cell.id)?,
                        value,
                    });
                }

                candidate.change(&[], edits)?;
            }
        }
        self.tree = candidate.tree;
        Ok(())
    }
}
impl StateAdapter {
    /// Canonical signed factory metadata, before resident expansion removes
    /// palette templates or replaces prototype IDs with allocated identities.
    pub(in crate::canvas) fn oib_authoring_seed(&self) -> Result<otrios_interfaces::oib::Seed> {
        self.doc.oib_authored_seed()
    }
    pub fn restore_oib(&mut self, defaults: bool) -> Result<bool> {
        self.edit_oib_instance(InstanceOperation::restore(
            self.doc.oib_status()?.structural_generation,
            defaults,
        ))
    }
    pub fn edit_oib_instance_property(
        &mut self,
        instance: u64,
        property: &str,
        value: Option<Value>,
    ) -> Result<bool> {
        let seed = self.doc.oib_resolved_seed()?;
        let mut cells = seed.cells.iter().filter(|cell| {
            cell.owner == instance && cell.inherit.is_some() && cell.name == property
        });
        let cell = cells.next().ok_or("Unknown declared instance property")?.id;
        if cells.next().is_some() {
            return Err("Ambiguous property label; use its full PropertyAddress".into());
        }
        self.edit_oib(&[(cell, value)])
    }
    pub fn instantiate_oib(&mut self, component: u64, name: Option<String>) -> Result<bool> {
        let seed = self.doc.oib_authored_seed()?;
        let prototype = seed
            .instances
            .iter()
            .find(|preset| preset.component == Some(component))
            .ok_or("Undeclared component preset")?
            .id;
        self.edit_oib_instance(InstanceOperation::insert(
            self.doc.oib_status()?.structural_generation,
            prototype,
            None,
            name,
        ))
    }
    /// Admitted presets are available independently of current selection or
    /// capacity. Each offered insertion is separately checked against bounds.
    pub fn oib_canvas_factories(&self) -> Result<Vec<(u64, String)>> {
        let seed = self.doc.oib_authored_seed()?;
        if seed.instance_surface.is_none() {
            return Ok(vec![]);
        }
        Ok(seed
            .instances
            .iter()
            .map(|preset| (preset.id, preset.name.clone()))
            .collect())
    }
    /// Submit the captured generation unchanged; rejection is durably receipted.
    pub fn edit_oib_structure(&mut self, operation: InstanceOperation) -> Result<bool> {
        self.edit_oib_instance(operation)
    }
    pub fn insert_oib(&mut self, prototype: u64, before: Option<u64>) -> Result<bool> {
        self.edit_oib_instance(InstanceOperation::insert(
            self.doc.oib_status()?.structural_generation,
            prototype,
            before,
            None,
        ))
    }
    pub fn reorder_oib(&mut self, instance: u64, before: Option<u64>) -> Result<bool> {
        if before == Some(instance) {
            return Ok(false);
        }
        self.edit_oib_instance(InstanceOperation::reorder(
            self.doc.oib_status()?.structural_generation,
            instance,
            before,
        ))
    }
    pub fn duplicate_oib(&mut self, instance: u64, name: Option<String>) -> Result<bool> {
        use otrios_interfaces::oib::{authoring::*, designer::InstanceId};
        if !self.doc.oib_live(2)?.contains(&instance) {
            return Ok(false);
        }
        let placements = self.doc.oib_placements()?;
        let destination =
            super::super::oib::document::placement::destination(&placements, InstanceId(instance));
        let frame = placements
            .iter()
            .find(|placement| placement.instance.0 == instance)
            .and_then(|placement| placement.frame);
        self.edit_oib_instance(InstanceOperation::hierarchy(HierarchyBatch {
            generation: self.doc.oib_status()?.structural_generation,
            operations: vec![HierarchyEdit::Duplicate {
                instance: InstanceId(instance),
                name,
                destination,
                before: None,
                frame,
            }],
        }))
    }
    pub fn remove_oib(&mut self, instance: u64) -> Result<bool> {
        if !self.doc.oib_live(1)?.contains(&instance) {
            return Ok(false);
        }
        self.edit_oib_instance(InstanceOperation::remove(
            self.doc.oib_status()?.structural_generation,
            instance,
        ))
    }
    fn edit_oib_instance(&mut self, operation: InstanceOperation) -> Result<bool> {
        use otrios_interfaces::oib::authoring::*;
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        if self.doc.oib_resolved_seed()?.instance_surface.is_none() {
            return Err("OIB instance surface unavailable".into());
        }
        if operation.generation == 0
            || operation.operations.is_empty()
            || operation.operations.len() > MAX_OPERATIONS
            || bytes(&operation)?.len() > MAX_BATCH_BYTES
        {
            return Err("Invalid OIB edit batch envelope".into());
        }
        if operation.generation == self.doc.oib_status()?.structural_generation
            && !operation
                .operations
                .iter()
                .any(|edit| matches!(edit, DocumentEdit::Definition(_) | DocumentEdit::Preset(_)))
        {
            // Capacity-only preflight avoids showing exhausted convenience
            // actions. Invalid/stale structure still goes to durable rejection.
            for edit in &operation.operations {
                let prototype = match edit {
                    DocumentEdit::Hierarchy(HierarchyEdit::Insert { prototype, .. }) => {
                        Some(prototype.0)
                    }
                    DocumentEdit::Hierarchy(HierarchyEdit::Duplicate { instance, .. }) => self
                        .doc
                        .oib_allocations()?
                        .iter()
                        .find(|allocation| allocation.id == instance.0)
                        .map(|allocation| allocation.prototype),
                    _ => None,
                };
                if prototype.is_some_and(|prototype| self.oib_available_slots_for(prototype) == 0)
                    && !operation.operations.iter().any(|edit| {
                        matches!(edit, DocumentEdit::Hierarchy(HierarchyEdit::Remove { .. }))
                    })
                {
                    return Ok(false);
                }
            }
        }
        self.flight = Some(Flight {
            oib_epoch: Some(self.doc.oib_epoch()?),
            sequence: self
                .acknowledged
                .checked_add(1)
                .ok_or("Intent sequence exhausted")?,
            values: vec![],
            oib: vec![],
            oib_instance: Some(operation),
            sent: false,
            issued_at: self.clock,
        });
        Ok(true)
    }
    pub fn oib_instance_action(&mut self, target: u64, action: InstanceAction) -> Result<bool> {
        match action {
            InstanceAction::Restore => self.restore_oib(false),
            InstanceAction::ResetDefaults => self.restore_oib(true),
            InstanceAction::Inspect => Ok(false),
            InstanceAction::RemovePending => {
                let active = self.doc.oib_live(2)?;
                let draft = self.doc.oib_live(1)?;
                let pending = self
                    .doc
                    .oib_allocations()?
                    .into_iter()
                    .rev()
                    .find(|a| draft.contains(&a.id) && !active.contains(&a.id));
                if let Some(a) = pending {
                    self.remove_oib(a.id)
                } else {
                    Ok(false)
                }
            }
            InstanceAction::Add => self.instantiate_oib(target, None),
            InstanceAction::Duplicate => self.duplicate_oib(target, None),
            InstanceAction::Remove => self.remove_oib(target),
        }
    }
}

impl StateAdapter {
    /// Actual authority-owned resident descriptors, including reusable slots.
    pub(in crate::canvas) fn oib_allocations(&self) -> Result<Vec<Allocation>> {
        self.doc.oib_allocations()
    }
    pub fn edit_oib_property(
        &mut self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
        value: Option<Value>,
    ) -> Result<bool> {
        if let otrios_interfaces::oib::designer::PropertyAddress::Placement { object, property } =
            address
        {
            return self.edit_oib_placement_property(object, *property, value);
        }
        let seed = self.doc.oib_authored_seed()?;
        let id = super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        self.edit_oib(&[(id, value)])
    }
}

impl StateAdapter {
    pub fn oib_property_register(
        &self,
        address: &otrios_interfaces::oib::designer::PropertyAddress,
    ) -> Result<PropertyRegisterView> {
        let seed = self.doc.oib_resolved_seed()?;
        let cell = super::super::oib::document::cell_for_address(
            &seed,
            &self.doc.oib_allocations()?,
            address,
        )?;
        let leaf = self
            .doc
            .tree
            .subtree(&address::path(1, address)?)
            .map_err(err)?;
        let register = leaf.register().ok_or("Missing OIB property register")?;
        let mut alternatives = Vec::new();
        for (_, _, data) in register.values() {
            let value = data.map(decode::<Value>).transpose()?;
            if !alternatives.contains(&value) {
                alternatives.push(value);
            }
        }
        Ok(PropertyRegisterView {
            address: address.clone(),
            cell,
            version: leaf.hash(),
            alternatives,
        })
    }

    /// Enqueue a choice only from this exact observed draft version. A stale
    /// view must be refreshed; no unseen alternative is implicitly resolved.
    /// True means queued, not authority acceptance or active publication.
    pub fn choose_oib_property(
        &mut self,
        view: &PropertyRegisterView,
        value: Option<Value>,
    ) -> Result<bool> {
        if self.read_only || !self.confirmed() {
            return Ok(false);
        }
        let current = self.oib_property_register(&view.address)?;
        if current.cell != view.cell || current.version != view.version {
            return Err("OIB property register changed; refresh choices".into());
        }
        if !current.alternatives.contains(&value) {
            return Err("Value is not an observed OIB property alternative".into());
        }
        // No replica mutation occurs between the version check and edit_oib's
        // capture of the observed register. Existing type/binding/write guards
        // and authority-side validation remain in force.
        self.edit_oib(&[(current.cell, value)])
    }
}
