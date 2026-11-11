// SPDX-License-Identifier: LGPL-3.0-only
//! One canonical accepted structural source; no independent overlay roots.
use super::*;
use otrios_interfaces::oib::authoring::*;

impl Document {
    pub(super) fn oib_source(&self) -> Result<DocumentSource> {
        Ok(self.oib_source_shared()?.as_ref().clone())
    }

    pub(super) fn oib_source_shared(&self) -> Result<std::sync::Arc<DocumentSource>> {
        Ok(self.oib_source_entry()?.source.clone())
    }

    pub(super) fn oib_source_entry(&self) -> Result<std::sync::Arc<CachedSource>> {
        let leaf = self.tree.subtree(&[4, 4]).map_err(err)?.hash();
        let binding = self.tree.subtree(&[4, 0]).map_err(err)?.hash();
        {
            let cache = self.oib_source_cache.lock().map_err(err)?;
            if let Some(cached) = cache.as_ref()
                && cached.leaf == leaf
                && cached.binding == binding
                && match (&cached.ui, &self.oib_ui) {
                    (Some(previous), Some(current)) => std::sync::Arc::ptr_eq(previous, current),
                    (None, None) => true,
                    _ => false,
                }
            {
                return Ok(cached.clone());
            }
        }
        let _profile = crate::canvas::profiling::span("oib.source.validate");
        let data = payload(&self.tree, &[4, 4])?;
        if data.len() > MAX_DOCUMENT_BYTES {
            return Err("OIB document source budget".into());
        }
        let source: DocumentSource = decode(&data)?;
        if bytes(&source)? != data
            || source.instances.len() > MAX_ALLOCATIONS
            || source.definitions.len() > otrios_interfaces::oib::designer::MAX_DEFINITIONS
            || source.presets.len() > otrios_interfaces::oib::designer::MAX_DEFINITIONS
            || source
                .presets
                .windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
            || bytes(&source.definitions)?.len() > MAX_SOURCE_BYTES
            || bytes(&source.placements)?.len() > MAX_PLACEMENT_BYTES
            || bytes(&source.connections)?.len() > MAX_CONNECTION_BYTES
            || source.placements.len() > MAX_ALLOCATIONS
            || source.connections.len() > otrios_interfaces::oib::MAX_CELLS
            || source
                .placements
                .windows(2)
                .any(|w| w[0].instance >= w[1].instance)
            || source
                .connections
                .windows(2)
                .any(|w| (&w[0].object, w[0].property) >= (&w[1].object, w[1].property))
        {
            return Err("Noncanonical OIB document source".into());
        }
        for placement in &source.placements {
            super::super::super::oib::document::placement::validate_frame(placement.frame)?;
        }
        let baseline = self.oib_binding()?;
        let signed = &baseline.seed;
        let graph = signed.graph.as_ref().ok_or("Missing canonical OIB graph")?;
        let shell = graph
            .instances
            .iter()
            .find(|instance| instance.id == graph.presented_root)
            .ok_or("Missing signed shell")?;
        let shell_preset = graph
            .presets
            .iter()
            .find(|preset| preset.id == shell.preset)
            .ok_or("Missing signed shell preset")?;
        let shell_definition = graph
            .definitions
            .iter()
            .find(|definition| definition.id == shell_preset.definition)
            .ok_or("Missing signed shell definition")?;
        if !source.presets.iter().any(|preset| {
            preset.id == shell.preset
                && preset.definition == shell_preset.definition
                && !preset.palette
        }) || !source.definitions.iter().any(|definition| {
            definition.id == shell_definition.id && definition.root == shell_definition.root
        }) {
            return Err("The presented shell identity cannot be removed or rebound".into());
        }
        let role_limit = graph.definitions.len()
            + graph.presets.len()
            + graph.instances.len()
            + graph
                .definitions
                .iter()
                .map(|definition| {
                    definition.nodes.len() + definition.slots.len() + definition.properties.len()
                })
                .sum::<usize>();
        if source.baseline.len() > role_limit {
            return Err("OIB baseline role budget".into());
        }
        let first = baseline.identity_floor;
        let mut roles = BTreeSet::new();
        let mut assigned = BTreeSet::new();
        let mut assigned_properties = BTreeSet::new();
        for identity in &source.baseline {
            if let BaselineIdentity::Property {
                definition,
                source: property,
                current,
            } = identity
            {
                let owner = source
                    .baseline
                    .iter()
                    .find_map(|role| match role {
                        BaselineIdentity::Definition { source, current }
                            if source == definition =>
                        {
                            Some(*current)
                        }
                        _ => None,
                    })
                    .unwrap_or(*definition);
                let admitted = graph
                    .definitions
                    .iter()
                    .find(|d| d.id == *definition)
                    .is_some_and(|d| d.properties.iter().any(|p| p.id == *property));
                let cursor_valid =
                    if let Some(owner) = source.definitions.iter().find(|d| d.id == owner) {
                        let root = owner
                            .nodes
                            .iter()
                            .find(|n| n.id == owner.root)
                            .ok_or("Missing baseline property owner")?;
                        let inherited = super::super::super::oib::document::class_descriptors(
                            &graph.classes,
                            root.class,
                        )?;
                        current.0 > inherited.iter().map(|p| p.id.0).max().unwrap_or(0)
                            && current.0 < owner.next_property
                    } else {
                        true
                    };
                if !admitted
                    || current.0 == 0
                    || current.0 == u64::MAX
                    || !roles.insert(vec![5, definition.0, property.0])
                    || !assigned_properties.insert((owner, *current))
                    || !cursor_valid
                {
                    return Err("Invalid OIB baseline property mapping".into());
                }
                continue;
            }
            let fixed = match identity {
                BaselineIdentity::Definition { source, .. } => *source == shell_definition.id,
                BaselineIdentity::Node {
                    definition, source, ..
                } => *definition == shell_definition.id && *source == shell_definition.root,
                BaselineIdentity::Preset { source, .. } => *source == shell.preset,
                BaselineIdentity::Instance { source, .. } => *source == graph.presented_root,
                BaselineIdentity::Slot { .. } | BaselineIdentity::Property { .. } => false,
            };
            let (key, original, current, admitted) = match identity {
                BaselineIdentity::Definition { source, current } => (
                    vec![0, source.0],
                    source.0,
                    current.0,
                    graph
                        .definitions
                        .iter()
                        .any(|definition| definition.id == *source),
                ),
                BaselineIdentity::Node {
                    definition,
                    source,
                    current,
                } => (
                    vec![1, definition.0, source.0],
                    source.0,
                    current.0,
                    graph
                        .definitions
                        .iter()
                        .find(|candidate| candidate.id == *definition)
                        .is_some_and(|definition| {
                            definition.nodes.iter().any(|node| node.id == *source)
                        }),
                ),
                BaselineIdentity::Slot {
                    definition,
                    source,
                    current,
                } => (
                    vec![2, definition.0, source.0],
                    source.0,
                    current.0,
                    graph
                        .definitions
                        .iter()
                        .find(|candidate| candidate.id == *definition)
                        .is_some_and(|definition| {
                            definition.slots.iter().any(|slot| slot.id == *source)
                        }),
                ),
                BaselineIdentity::Preset { source, current } => (
                    vec![3, *source],
                    *source,
                    *current,
                    graph.presets.iter().any(|preset| preset.id == *source),
                ),
                BaselineIdentity::Instance { source, current } => (
                    vec![4, source.0],
                    source.0,
                    current.0,
                    graph
                        .instances
                        .iter()
                        .any(|instance| instance.id == *source),
                ),
                BaselineIdentity::Property { .. } => {
                    unreachable!("property roles checked separately")
                }
            };
            if !admitted
                || !roles.insert(key)
                || current == 0
                || fixed && current != original
                || (current != original
                    && (!(first..source.next_identity).contains(&current)
                        || !assigned.insert(current)))
            {
                return Err("Invalid OIB baseline identity mapping".into());
            }
        }
        // Materialization depends on definitions/presets, while placement and
        // resident changes still require their own full-source validation.
        // Compute this structural key once per source miss, never per UI hit.
        let structural = digest(&bytes(&(&source.definitions, &source.presets))?);
        let validated = std::sync::Arc::new(CachedSource {
            leaf,
            binding,
            ui: self.oib_ui.clone(),
            source: std::sync::Arc::new(source),
            structural,
        });
        *self.oib_source_cache.lock().map_err(err)? = Some(validated.clone());
        Ok(validated)
    }
    pub(super) fn oib_source_edit(&self, change: impl FnOnce(&mut DocumentSource)) -> Result<Edit> {
        let mut source = self.oib_source()?;
        change(&mut source);
        let value = bytes(&source)?;
        if value.len() > MAX_DOCUMENT_BYTES {
            return Err("OIB document source budget".into());
        }
        Ok(Edit {
            path: vec![4, 4],
            schema: self.extra_schema(4, 4),
            value: Some(value),
        })
    }
}
