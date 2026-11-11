// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded conditional inverse planning. No authority, live document or app data.
use super::*;
use crate::merkle::Snapshot;
use otrios_interfaces::oib::{authoring::*, designer::*};
use serde::{Deserialize, Serialize};

pub(crate) const MAX_HISTORY_BYTES: usize = 64 * 1024;
pub(crate) type CapturedProperties = Vec<(PropertyAddress, Vec<Option<Value>>)>;
mod restore;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Entry {
    epoch: Vec<u8>,
    baseline: Vec<u8>,
    generation: u64,
    destructive: bool,
    guards: Vec<Guard>,
    operations: Vec<DocumentEdit>,
    properties: Vec<PropertyInverse>,
    restoration: restore::Graph,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PropertyInverse {
    address: PropertyAddress,
    before: Option<Value>,
    applied: Option<Value>,
    version: Vec<u8>,
    before_version: Vec<u8>,
    label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
enum Key {
    Definition(DefinitionId),
    DefinitionName(DefinitionId),
    Node(DefinitionId, NodeId),
    NodeName(DefinitionId, NodeId),
    Visual(DefinitionId, NodeId),
    Default(DefinitionId, NodeId, PropertyId),
    PropertyDescriptor(DefinitionId, PropertyId),
    Slots(DefinitionId),
    Exposed(DefinitionId, PropertyId),
    Reference(DefinitionId, NodeId, PropertyPath),
    NodeOwner(DefinitionId, NodeId),
    Children(DefinitionId, NodeId),
    Preset(u64),
    Instance(InstanceId),
    InstanceName(InstanceId),
    Placement(InstanceId),
    Siblings(Destination),
    Connection(ObjectAddress, PropertyId),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Guard {
    key: Key,
    before: Vec<u8>,
    after: Vec<u8>,
    label: String,
    before_text: String,
    after_text: String,
}
/// Canonical typed field bytes are retained for the review UI, never executed.
#[derive(Clone, Debug)]
pub(crate) struct ReviewConflict {
    pub target: String,
    pub before: Vec<u8>,
    pub applied: Vec<u8>,
    pub current: Vec<u8>,
    pub before_text: String,
    pub applied_text: String,
    pub current_text: String,
    pub selection: Option<Selection>,
}
#[path = "history/text.rs"]
mod text;
use text::{label, property_label, property_selection, selection, source_text, value_text};

fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    flexbuffers::to_vec(value).map_err(err)
}
fn definition(source: &DocumentSource, id: DefinitionId) -> Result<&ComponentDefinition> {
    source
        .definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "History definition was removed".into())
}
fn node(source: &DocumentSource, d: DefinitionId, id: NodeId) -> Result<&DefinitionNode> {
    definition(source, d)?
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or_else(|| "History node was removed".into())
}
fn destination(source: &DocumentSource, id: InstanceId) -> Destination {
    source
        .placements
        .iter()
        .find(|p| p.instance == id)
        .map_or(Destination::Canvas, |p| p.destination.clone())
}
fn frame(source: &DocumentSource, id: InstanceId) -> Option<[f32; 4]> {
    source
        .placements
        .iter()
        .find(|p| p.instance == id)
        .and_then(|p| p.frame)
}
fn sibling_ids(source: &DocumentSource, dest: &Destination) -> Vec<InstanceId> {
    source
        .instances
        .iter()
        .map(|i| InstanceId(i.id))
        .filter(|id| destination(source, *id) == *dest)
        .collect()
}
fn next_sibling(source: &DocumentSource, id: InstanceId) -> Option<InstanceId> {
    let siblings = sibling_ids(source, &destination(source, id));
    siblings
        .iter()
        .position(|i| *i == id)
        .and_then(|i| siblings.get(i + 1))
        .copied()
}
fn visual_parent(source: &DocumentSource, d: DefinitionId, id: NodeId) -> Option<NodeId> {
    definition(source, d)
        .ok()?
        .nodes
        .iter()
        .find(|n| n.visual_children.contains(&id))
        .map(|n| n.id)
}
fn read(source: &DocumentSource, key: &Key) -> Result<Vec<u8>> {
    match key {
        Key::Definition(d) => encoded(&source.definitions.iter().find(|v| v.id == *d)),
        Key::DefinitionName(d) => encoded(&definition(source, *d)?.name),
        Key::Node(d, n) => encoded(&definition(source, *d)?.nodes.iter().find(|v| v.id == *n)),
        Key::NodeName(d, n) => encoded(&node(source, *d, *n)?.name),
        Key::Visual(d, n) => encoded(&node(source, *d, *n)?.visual),
        Key::Default(d, n, p) => encoded(
            &node(source, *d, *n)?
                .defaults
                .iter()
                .find(|v| v.property == *p)
                .map(|v| &v.value),
        ),
        Key::PropertyDescriptor(d, p) => encoded(
            &definition(source, *d)?
                .properties
                .iter()
                .find(|v| v.id == *p),
        ),
        Key::Slots(d) => encoded(&definition(source, *d)?.slots),
        Key::Exposed(d, p) => encoded(
            &definition(source, *d)?
                .exposed
                .iter()
                .find(|v| v.property == *p),
        ),
        Key::Reference(d, n, p) => encoded(
            &node(source, *d, *n)?
                .reference
                .as_ref()
                .ok_or("History reference was removed")?
                .assignments
                .iter()
                .find(|v| v.target == *p),
        ),
        Key::NodeOwner(d, n) => encoded(&node(source, *d, *n)?.owner),
        Key::Children(d, n) => encoded(&node(source, *d, *n)?.visual_children),
        Key::Preset(id) => encoded(&source.presets.iter().find(|v| v.id == *id)),
        Key::Instance(id) => encoded(&source.instances.iter().find(|v| v.id == id.0)),
        Key::InstanceName(id) => encoded(
            &source
                .instances
                .iter()
                .find(|v| v.id == id.0)
                .ok_or("History instance was removed")?
                .name,
        ),
        Key::Placement(id) => encoded(&source.placements.iter().find(|v| v.instance == *id)),
        Key::Siblings(dest) => encoded(&sibling_ids(source, dest)),
        Key::Connection(object, p) => encoded(
            &source
                .connections
                .iter()
                .find(|v| v.object == *object && v.property == *p),
        ),
    }
}
fn observed_values(bytes: &[u8]) -> Result<Vec<Option<Value>>> {
    let leaf = Snapshot::from_bytes(bytes).map_err(err)?;
    if leaf.to_bytes().map_err(err)? != bytes {
        return Err("Noncanonical history property context".into());
    }
    let register = leaf.register().ok_or("Missing history property register")?;
    let mut values = vec![];
    for (_, _, data) in register.values() {
        let value = data
            .map(|data| flexbuffers::from_slice(data).map_err(err))
            .transpose()?;
        if !values.contains(&value) {
            values.push(value);
        }
    }
    Ok(values)
}

impl Entry {
    /// Capture only affected source fields and prior causal values from the
    /// original Change.observed leaves. Legacy receipts cannot prove properties.
    #[cfg(test)]
    pub(crate) fn capture(
        before: &DocumentSource,
        after: &DocumentSource,
        apply: &Apply,
        receipt: &ApplyReceipt,
    ) -> Result<Self> {
        Self::capture_with_properties(before, after, apply, receipt, &[])
    }
    pub(crate) fn capture_with_properties(
        before: &DocumentSource,
        after: &DocumentSource,
        apply: &Apply,
        receipt: &ApplyReceipt,
        properties: &[(PropertyAddress, Vec<Option<Value>>)],
    ) -> Result<Self> {
        let ApplyResult::Accepted { generation, .. } = receipt.result else {
            return Err("Rejected Apply has no applied history".into());
        };
        if receipt.digest != hash(&encoded(&("oib-apply-v1", apply))?)
            || receipt.property_versions.len() != apply.changes.len()
            || receipt.property_versions.iter().any(|v| v.len() != 32)
        {
            return Err("Exact applied property proof is unavailable".into());
        }
        let mut entry = Self {
            epoch: apply.epoch.clone(),
            baseline: apply.baseline.clone(),
            generation,
            destructive: false,
            guards: vec![],
            operations: vec![],
            properties: vec![],
            restoration: restore::Graph::capture(before, after, properties)?,
        };
        for key in entry.restoration.guard_keys() {
            entry.guard(before, after, key)?;
        }
        for (change, version) in apply.changes.iter().zip(&receipt.property_versions) {
            let values = observed_values(&change.observed)?;
            if values.len() != 1 {
                return Err(
                    "Undo cannot recreate a prior causal conflict; review the alternatives".into(),
                );
            }
            entry.properties.push(PropertyInverse {
                address: change.address.clone(),
                before: values[0].clone(),
                applied: change.value.clone(),
                version: version.clone(),
                before_version: Snapshot::from_bytes(&change.observed)
                    .map_err(err)?
                    .hash()
                    .to_vec(),
                label: property_label(before, &change.address),
            });
        }
        let created_definitions: BTreeSet<_> = after
            .definitions
            .iter()
            .filter(|d| !before.definitions.iter().any(|b| b.id == d.id))
            .map(|d| d.id)
            .collect();
        let created_nodes: BTreeSet<_> = after
            .definitions
            .iter()
            .flat_map(|d| {
                d.nodes
                    .iter()
                    .filter(|n| {
                        before
                            .definitions
                            .iter()
                            .find(|b| b.id == d.id)
                            .is_some_and(|b| !b.nodes.iter().any(|old| old.id == n.id))
                    })
                    .map(move |n| (d.id, n.id))
            })
            .collect();
        let created_properties: BTreeSet<_> = after
            .definitions
            .iter()
            .flat_map(|d| {
                d.properties
                    .iter()
                    .filter(|p| {
                        before
                            .definitions
                            .iter()
                            .find(|b| b.id == d.id)
                            .is_some_and(|b| !b.properties.iter().any(|old| old.id == p.id))
                    })
                    .map(move |p| (d.id, p.id))
            })
            .collect();
        let created_presets: BTreeSet<_> = after
            .presets
            .iter()
            .filter(|p| !before.presets.iter().any(|b| b.id == p.id))
            .map(|p| p.id)
            .collect();
        let created_instances: BTreeSet<_> = after
            .instances
            .iter()
            .filter(|i| !before.instances.iter().any(|b| b.id == i.id))
            .map(|i| InstanceId(i.id))
            .collect();
        for op in apply.operations.iter().rev() {
            match op {
                DocumentEdit::Definition(edit) => {
                    let d = match edit {
                        DefinitionEdit::CreateDefinition { definition, .. }
                        | DefinitionEdit::DeleteDefinition { definition }
                        | DefinitionEdit::CreateNode { definition, .. }
                        | DefinitionEdit::DeleteNode { definition, .. }
                        | DefinitionEdit::SetVisual { definition, .. }
                        | DefinitionEdit::SetProperty { definition, .. }
                        | DefinitionEdit::DeleteProperty { definition, .. }
                        | DefinitionEdit::SetSlots { definition, .. }
                        | DefinitionEdit::SetDefault { definition, .. }
                        | DefinitionEdit::ClearDefault { definition, .. }
                        | DefinitionEdit::RenameDefinition { definition, .. }
                        | DefinitionEdit::RenameNode { definition, .. }
                        | DefinitionEdit::MoveNode { definition, .. }
                        | DefinitionEdit::Expose { definition, .. }
                        | DefinitionEdit::ConnectReference { definition, .. } => *definition,
                    };
                    if created_definitions.contains(&d) || entry.restoration.recreates_definition(d)
                    {
                        continue;
                    }
                    let target_node = match edit {
                        DefinitionEdit::SetVisual { node, .. }
                        | DefinitionEdit::SetDefault { node, .. }
                        | DefinitionEdit::ClearDefault { node, .. }
                        | DefinitionEdit::RenameNode { node, .. }
                        | DefinitionEdit::MoveNode { node, .. } => Some(*node),
                        DefinitionEdit::ConnectReference { reference, .. } => Some(*reference),
                        _ => None,
                    };
                    if target_node.is_some_and(|n| {
                        created_nodes.contains(&(d, n)) || entry.restoration.recreates_node(d, n)
                    }) {
                        continue;
                    }
                    let target_property = match edit {
                        DefinitionEdit::SetProperty { descriptor, .. } => Some(descriptor.id),
                        DefinitionEdit::DeleteProperty { property, .. }
                        | DefinitionEdit::Expose { property, .. } => Some(*property),
                        DefinitionEdit::SetDefault { node, property, .. }
                        | DefinitionEdit::ClearDefault { node, property, .. }
                            if definition(before, d)?.root == *node =>
                        {
                            Some(*property)
                        }
                        _ => None,
                    };
                    if target_property.is_some_and(|p| {
                        entry.restoration.recreates_property(d, p)
                            || created_properties.contains(&(d, p))
                    }) {
                        continue;
                    }
                    let (key, inverse) = match edit {
                        DefinitionEdit::SetProperty { descriptor, .. } => {
                            let Some(previous) = definition(before, d)?
                                .properties
                                .iter()
                                .find(|p| p.id == descriptor.id)
                            else {
                                return Err("Missing created endpoint history".into());
                            };
                            (
                                Key::PropertyDescriptor(d, descriptor.id),
                                DefinitionEdit::SetProperty {
                                    definition: d,
                                    descriptor: previous.clone(),
                                },
                            )
                        }
                        DefinitionEdit::RenameDefinition { .. } => (
                            Key::DefinitionName(d),
                            DefinitionEdit::RenameDefinition {
                                definition: d,
                                name: definition(before, d)?.name.clone(),
                            },
                        ),
                        DefinitionEdit::RenameNode { node: n, .. } => (
                            Key::NodeName(d, *n),
                            DefinitionEdit::RenameNode {
                                definition: d,
                                node: *n,
                                name: node(before, d, *n)?.name.clone(),
                            },
                        ),
                        DefinitionEdit::SetVisual { node: n, .. } => (
                            Key::Visual(d, *n),
                            DefinitionEdit::SetVisual {
                                definition: d,
                                node: *n,
                                visual: node(before, d, *n)?.visual.clone(),
                            },
                        ),
                        DefinitionEdit::SetDefault {
                            node: n,
                            property: p,
                            ..
                        }
                        | DefinitionEdit::ClearDefault {
                            node: n,
                            property: p,
                            ..
                        } => (
                            Key::Default(d, *n, *p),
                            match node(before, d, *n)?
                                .defaults
                                .iter()
                                .find(|v| v.property == *p)
                            {
                                Some(value) => DefinitionEdit::SetDefault {
                                    definition: d,
                                    node: *n,
                                    property: *p,
                                    value: value.value.clone(),
                                },
                                None => DefinitionEdit::ClearDefault {
                                    definition: d,
                                    node: *n,
                                    property: *p,
                                },
                            },
                        ),
                        DefinitionEdit::SetSlots { .. } => {
                            let old = &definition(before, d)?.slots;
                            (
                                Key::Slots(d),
                                DefinitionEdit::SetSlots {
                                    definition: d,
                                    slots: old.clone(),
                                },
                            )
                        }
                        DefinitionEdit::Expose { property: p, .. } => (
                            Key::Exposed(d, *p),
                            DefinitionEdit::Expose {
                                definition: d,
                                property: *p,
                                targets: definition(before, d)?
                                    .exposed
                                    .iter()
                                    .find(|e| e.property == *p)
                                    .ok_or("Undo cannot recreate an exposed descriptor")?
                                    .targets
                                    .clone(),
                            },
                        ),
                        DefinitionEdit::ConnectReference {
                            reference, target, ..
                        } => {
                            let previous = node(before, d, *reference)?
                                .reference
                                .as_ref()
                                .ok_or("Missing previous reference")?
                                .assignments
                                .iter()
                                .find(|v| v.target == *target);
                            let source = previous
                                .map(|v| match &v.source {
                                    AssignmentSource::Literal(v) => {
                                        Ok(LiteralSource::Literal(v.clone()))
                                    }
                                    AssignmentSource::Forward(p) => {
                                        Ok(LiteralSource::Forward(p.clone()))
                                    }
                                    _ => Err("History cannot recreate an app reference binding"
                                        .to_string()),
                                })
                                .transpose()?;
                            (
                                Key::Reference(d, *reference, target.clone()),
                                DefinitionEdit::ConnectReference {
                                    definition: d,
                                    reference: *reference,
                                    target: target.clone(),
                                    source,
                                },
                            )
                        }
                        DefinitionEdit::MoveNode { node: n, .. } => {
                            for source in [before, after] {
                                if let Some(parent) = visual_parent(source, d, *n) {
                                    entry.guard(before, after, Key::Children(d, parent))?;
                                }
                            }
                            let parent = visual_parent(before, d, *n);
                            let anchor = parent.and_then(|p| {
                                let children = &node(before, d, p).ok()?.visual_children;
                                children
                                    .iter()
                                    .position(|id| id == n)
                                    .and_then(|i| children.get(i + 1))
                                    .copied()
                            });
                            (
                                Key::NodeOwner(d, *n),
                                DefinitionEdit::MoveNode {
                                    definition: d,
                                    node: *n,
                                    owner: node(before, d, *n)?
                                        .owner
                                        .ok_or("Cannot move a history root")?,
                                    visual_parent: parent,
                                    before: anchor,
                                },
                            )
                        }
                        DefinitionEdit::CreateNode { .. } => continue,
                        DefinitionEdit::DeleteDefinition { .. }
                        | DefinitionEdit::DeleteNode { .. }
                        | DefinitionEdit::DeleteProperty { .. } => continue,
                        DefinitionEdit::CreateDefinition { .. } => {
                            return Err("Missing created history definition".into());
                        }
                    };
                    if entry.guard(before, after, key)? {
                        entry.operations.push(DocumentEdit::Definition(inverse));
                    }
                }
                DocumentEdit::Preset(PresetEdit::Update { preset })
                    if !created_presets.contains(&preset.id)
                        && !entry.restoration.recreates_preset(preset.id) =>
                {
                    if entry.guard(before, after, Key::Preset(preset.id))? {
                        entry
                            .operations
                            .push(DocumentEdit::Preset(PresetEdit::Update {
                                preset: before
                                    .presets
                                    .iter()
                                    .find(|p| p.id == preset.id)
                                    .ok_or("Missing previous preset")?
                                    .clone(),
                            }));
                    }
                }
                DocumentEdit::Preset(PresetEdit::Create { .. } | PresetEdit::Update { .. }) => {}
                DocumentEdit::Preset(PresetEdit::Delete { .. }) => {}
                DocumentEdit::Hierarchy(edit) => match edit {
                    HierarchyEdit::Connect {
                        object, property, ..
                    } if !created_instances.contains(&object.instance)
                        && !entry.restoration.recreates_instance(object.instance) =>
                    {
                        if entry.guard(before, after, Key::Connection(object.clone(), *property))? {
                            entry.operations.push(DocumentEdit::Hierarchy(
                                HierarchyEdit::Connect {
                                    object: object.clone(),
                                    property: *property,
                                    source: before
                                        .connections
                                        .iter()
                                        .find(|c| c.object == *object && c.property == *property)
                                        .map(|c| c.source.clone()),
                                },
                            ));
                        }
                    }
                    HierarchyEdit::Rename { instance, .. }
                        if !created_instances.contains(instance)
                            && !entry.restoration.recreates_instance(*instance) =>
                    {
                        if entry.guard(before, after, Key::InstanceName(*instance))? {
                            entry
                                .operations
                                .push(DocumentEdit::Hierarchy(HierarchyEdit::Rename {
                                    instance: *instance,
                                    name: before
                                        .instances
                                        .iter()
                                        .find(|i| i.id == instance.0)
                                        .ok_or("Missing previous instance")?
                                        .name
                                        .clone(),
                                }));
                        }
                    }
                    HierarchyEdit::Move { instance, .. }
                    | HierarchyEdit::Place { instance, .. }
                    | HierarchyEdit::Reorder { instance, .. }
                        if !created_instances.contains(instance)
                            && !entry.restoration.recreates_instance(*instance) =>
                    {
                        for source in [before, after] {
                            entry.guard(
                                before,
                                after,
                                Key::Siblings(destination(source, *instance)),
                            )?;
                        }
                        if entry.guard(before, after, Key::Placement(*instance))? {
                            entry
                                .operations
                                .push(DocumentEdit::Hierarchy(HierarchyEdit::Move {
                                    instance: *instance,
                                    destination: destination(before, *instance),
                                    before: next_sibling(before, *instance),
                                    frame: frame(before, *instance),
                                }));
                        }
                    }
                    HierarchyEdit::Remove { .. } => {}
                    _ => {}
                },
                DocumentEdit::Restore { .. } => {
                    return Err("Reset has no bounded conditional inverse".into());
                }
            }
        }
        // Creation inverses require the accepted revision to remain coherent
        // and unchanged, guarding unrecorded newly seeded property registers.
        entry.destructive = !created_definitions.is_empty()
            || !created_nodes.is_empty()
            || !created_properties.is_empty()
            || !created_presets.is_empty()
            || !created_instances.is_empty();
        // Unknown imported bodies are not evidence of absence. Discard only
        // targets positively introduced by this Apply and removed by its inverse.
        entry.properties.retain(|p| {
            !match &p.address {
                PropertyAddress::Definition {
                    definition, node, ..
                } => {
                    created_definitions.contains(definition)
                        || created_nodes.contains(&(*definition, *node))
                }
                PropertyAddress::Occurrence { instance, path } => {
                    created_instances.contains(instance)
                        || created_nodes
                            .iter()
                            .any(|(_, n)| *n == path.node || path.references.contains(n))
                }
                _ => false,
            } && !created_properties.iter().any(|(d, property)| {
                super::endpoint_impact::addresses_endpoint(after, &p.address, *d, *property)
            })
        });
        for (d, property) in created_properties {
            entry.guard(before, after, Key::Definition(d))?;
            entry
                .operations
                .push(DocumentEdit::Definition(DefinitionEdit::DeleteProperty {
                    definition: d,
                    property,
                }));
        }
        for id in created_definitions {
            entry.guard(before, after, Key::Definition(id))?;
            entry
                .operations
                .push(DocumentEdit::Definition(DefinitionEdit::DeleteDefinition {
                    definition: id,
                }));
        }
        for (d, n) in created_nodes {
            entry.guard(before, after, Key::Node(d, n))?;
            if let Some(p) = visual_parent(after, d, n)
                && node(before, d, p).is_ok()
            {
                entry.guard(before, after, Key::Children(d, p))?;
            }
            entry
                .operations
                .push(DocumentEdit::Definition(DefinitionEdit::DeleteNode {
                    definition: d,
                    node: n,
                }));
        }
        for id in created_presets {
            entry.guard(before, after, Key::Preset(id))?;
            entry
                .operations
                .push(DocumentEdit::Preset(PresetEdit::Delete { preset: id }));
        }
        for id in created_instances.into_iter().rev() {
            entry.guard(before, after, Key::Instance(id))?;
            entry.guard(before, after, Key::Siblings(destination(after, id)))?;
            entry
                .operations
                .push(DocumentEdit::Hierarchy(HierarchyEdit::Remove {
                    instance: id,
                }));
        }
        // Restore moved siblings from right to left in their original order.
        // Reversing the user's command order is insufficient (A and B may both
        // have moved past C, making their original anchors depend on each other).
        entry.operations.sort_by_key(|op| match op {
            DocumentEdit::Definition(DefinitionEdit::MoveNode {
                definition: d,
                node: n,
                ..
            }) => {
                let parent = visual_parent(before, *d, *n);
                let index = parent
                    .and_then(|p| {
                        node(before, *d, p)
                            .ok()?
                            .visual_children
                            .iter()
                            .position(|id| id == n)
                    })
                    .unwrap_or(0);
                (0, 0, d.0, parent.map_or(0, |p| p.0), usize::MAX - index)
            }
            DocumentEdit::Definition(_) => (0, 1, 0, 0, 0),
            DocumentEdit::Preset(_) => (1, 0, 0, 0, 0),
            DocumentEdit::Hierarchy(HierarchyEdit::Move { instance, .. }) => (
                2,
                0,
                0,
                0,
                usize::MAX
                    - before
                        .instances
                        .iter()
                        .position(|i| i.id == instance.0)
                        .unwrap_or(0),
            ),
            _ => (2, 1, 0, 0, 0),
        });
        if entry.encoded_bytes()? > MAX_HISTORY_BYTES {
            return Err("Applied change exceeds the 64 KiB history budget".into());
        }
        Ok(entry)
    }
    fn guard(&mut self, before: &DocumentSource, after: &DocumentSource, key: Key) -> Result<bool> {
        if self.guards.iter().any(|g| g.key == key) {
            return Ok(false);
        }
        self.guards.push(Guard {
            before: read(before, &key)?,
            after: read(after, &key)?,
            label: label(before, &key),
            before_text: source_text(before, &key),
            after_text: source_text(after, &key),
            key,
        });
        Ok(true)
    }
    pub(crate) fn encoded_bytes(&self) -> Result<usize> {
        Ok(encoded(self)?.len())
    }
    /// Called only after the manager matched the accepted inverse request's
    /// durable receipt. An unrelated equal-value write cannot supply this chain.
    pub(crate) fn reanchor_after_undo(&mut self, undone: &Entry, inverse: &Entry) -> usize {
        let mut changed = 0;
        for prior in &mut self.properties {
            let Some(removed) = undone
                .properties
                .iter()
                .find(|p| p.address == prior.address && p.before_version == prior.version)
            else {
                continue;
            };
            let Some(restored) = inverse.properties.iter().find(|p| {
                p.address == prior.address
                    && p.before == removed.applied
                    && p.applied == removed.before
                    && p.applied == prior.applied
            }) else {
                continue;
            };
            prior.version = restored.version.clone();
            changed += 1;
        }
        changed
    }
    pub(crate) fn restored_property_values(
        &self,
        current: &DocumentSource,
    ) -> Result<Vec<(PropertyAddress, Option<Value>)>> {
        self.restoration.values(current)
    }
    pub(crate) fn restored_selection(
        &self,
        current: &DocumentSource,
        selection: &Selection,
    ) -> Result<Selection> {
        self.restoration.selection(current, selection)
    }
    pub(crate) fn property_values(&self) -> Vec<(PropertyAddress, Option<Value>)> {
        self.properties
            .iter()
            .map(|p| (p.address.clone(), p.before.clone()))
            .collect()
    }
    pub(crate) fn review(
        &self,
        current: &DocumentSource,
        current_ui_generation: u64,
        changes: &[Change],
    ) -> Result<Vec<ReviewConflict>> {
        let mut result = vec![];
        if self.destructive && current_ui_generation != self.generation {
            result.push(ReviewConflict {
                target: "Created content has a later UI revision; review its uses and overrides"
                    .into(),
                before: encoded(&self.generation)?,
                applied: encoded(&self.generation)?,
                current: encoded(&current_ui_generation)?,
                before_text: "Before creation".into(),
                applied_text: "Created content".into(),
                current_text: "Later edits may affect its uses or control values".into(),
                selection: None,
            });
        }
        for guard in &self.guards {
            let now = read(current, &guard.key).unwrap_or_default();
            if now != guard.after {
                result.push(ReviewConflict {
                    target: guard.label.clone(),
                    before: guard.before.clone(),
                    applied: guard.after.clone(),
                    current: now,
                    before_text: guard.before_text.clone(),
                    applied_text: guard.after_text.clone(),
                    current_text: source_text(current, &guard.key),
                    selection: selection(current, &guard.key),
                });
            }
        }
        for property in &self.properties {
            let change = changes.iter().find(|c| c.address == property.address);
            let matches = change.is_some_and(|c| {
                Snapshot::from_bytes(&c.observed)
                    .is_ok_and(|leaf| leaf.hash().as_slice() == property.version)
            });
            if !matches {
                result.push(ReviewConflict {
                    target: property.label.clone(),
                    before: encoded(&property.before)?,
                    applied: encoded(&property.applied)?,
                    current: change
                        .map(|c| observed_values(&c.observed).and_then(|v| encoded(&v)))
                        .transpose()?
                        .unwrap_or_default(),
                    before_text: value_text(&property.before),
                    applied_text: value_text(&property.applied),
                    current_text: change
                        .map(|c| {
                            observed_values(&c.observed)
                                .map(|v| v.iter().map(value_text).collect::<Vec<_>>().join(" / "))
                        })
                        .transpose()?
                        .unwrap_or_else(|| "Unavailable".into()),
                    selection: property_selection(current, &property.address),
                });
            }
        }
        Ok(result)
    }
    /// Result is a new private command for review/Apply, never a network write.
    pub(crate) fn plan(
        &self,
        current: &DocumentSource,
        current_ui_generation: u64,
        mut base: Apply,
        changes: &[Change],
    ) -> Result<Apply> {
        if base.epoch != self.epoch || base.baseline != self.baseline {
            return Err("History admission changed; review the component library".into());
        }
        if !base.operations.is_empty() || !base.changes.is_empty() {
            return Err("Resolve the current working copy before Undo applied change".into());
        }
        if !self
            .review(current, current_ui_generation, changes)?
            .is_empty()
        {
            return Err(
                "Applied sources changed; review before, applied and current values".into(),
            );
        }
        if changes.len() != self.properties.len() {
            return Err("Unexpected history property contexts".into());
        }
        for p in &self.properties {
            let c = changes
                .iter()
                .find(|c| c.address == p.address)
                .ok_or("Missing history property context")?;
            if c.value != p.before {
                return Err("History inverse value changed".into());
            }
        }
        base.operations = self.restoration.operations(current, &self.operations)?;
        base.changes = changes.to_vec();
        if base.operations.len() + base.changes.len() > MAX_OPERATIONS
            || encoded(&base)?.len() > MAX_BATCH_BYTES
        {
            return Err("Conditional inverse exceeds the 16-operation / 8 KiB Apply budget".into());
        }
        Ok(base)
    }
}
