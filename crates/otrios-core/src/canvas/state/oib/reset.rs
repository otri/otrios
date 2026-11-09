// SPDX-License-Identifier: LGPL-3.0-only
//! Restore signed design roles without reusing a deleted semantic identity.
use super::*;
use otrios_interfaces::oib::{authoring::*, designer::*, graph};
use std::collections::BTreeMap;

fn allocate(next: &mut u64) -> Result<u64> {
    let id = *next;
    if id == 0 {
        return Err("Invalid graph identity cursor".into());
    }
    *next = id.checked_add(1).ok_or("Graph identity space exhausted")?;
    Ok(id)
}

struct Remap<'a> {
    signed: &'a graph::Document,
    resolved: &'a [ComponentDefinition],
    definitions: BTreeMap<DefinitionId, DefinitionId>,
    nodes: BTreeMap<(DefinitionId, NodeId), NodeId>,
    slots: BTreeMap<(DefinitionId, SlotId), SlotId>,
    presets: BTreeMap<u64, u64>,
    instances: BTreeMap<InstanceId, InstanceId>,
}
impl Remap<'_> {
    fn definition(&self, id: DefinitionId) -> Result<&ComponentDefinition> {
        self.signed
            .definitions
            .iter()
            .find(|d| d.id == id)
            .or_else(|| self.resolved.iter().find(|definition| definition.id == id))
            .ok_or_else(|| "Missing signed definition".into())
    }
    fn node(&self, definition: DefinitionId, node: NodeId) -> Result<NodeId> {
        if let Some(current) = self.nodes.get(&(definition, node)) {
            return Ok(*current);
        }
        if !self.definitions.contains_key(&definition)
            && self
                .definition(definition)?
                .nodes
                .iter()
                .any(|current| current.id == node)
        {
            return Ok(node);
        }
        Err("Missing baseline node role".into())
    }
    fn path(&self, definition: DefinitionId, path: &PropertyPath) -> Result<PropertyPath> {
        let mut definition = definition;
        let mut references = Vec::new();
        for id in &path.references {
            references.push(self.node(definition, *id)?);
            definition = self
                .definition(definition)?
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .and_then(|n| n.reference.as_ref())
                .ok_or("Missing signed reference role")?
                .definition;
        }
        Ok(PropertyPath {
            references,
            node: self.node(definition, path.node)?,
            property: path.property,
        })
    }
    fn visual(&self, definition: DefinitionId, visual: &mut graph::Visual) -> Result<()> {
        for parameter in &mut visual.parameters {
            if let graph::ParameterSource::Property(path) = &mut parameter.source {
                *path = self.path(definition, path)?;
            }
        }
        if let Some(action) = &mut visual.action {
            action.source = self.path(definition, &action.source)?;
        }
        Ok(())
    }
    fn component(&self, value: &ComponentDefinition) -> Result<ComponentDefinition> {
        let mut result = value.clone();
        result.id = self.definitions[&value.id];
        result.root = self.node(value.id, value.root)?;
        for node in &mut result.nodes {
            node.id = self.node(value.id, node.id)?;
            node.owner = node.owner.map(|id| self.node(value.id, id)).transpose()?;
            for id in &mut node.visual_children {
                *id = self.node(value.id, *id)?;
            }
            if let Some(reference) = &mut node.reference {
                let target_definition = reference.definition;
                reference.definition = self
                    .definitions
                    .get(&target_definition)
                    .copied()
                    .unwrap_or(target_definition);
                for assignment in &mut reference.assignments {
                    assignment.target = self.path(target_definition, &assignment.target)?;
                    if let AssignmentSource::Forward(path) = &mut assignment.source {
                        *path = self.path(value.id, path)?;
                    }
                }
            }
            if let Some(visual) = &mut node.visual {
                self.visual(value.id, visual)?;
            }
        }
        for exposed in &mut result.exposed {
            for path in &mut exposed.targets {
                *path = self.path(value.id, path)?;
            }
        }
        for slot in &mut result.slots {
            let path = self.path(
                value.id,
                &PropertyPath {
                    references: slot.references.clone(),
                    node: slot.node,
                    property: PropertyId(0),
                },
            )?;
            slot.id = self.slots[&(value.id, slot.id)];
            slot.references = path.references;
            slot.node = path.node;
        }
        result.nodes.sort_by_key(|n| n.id);
        result.slots.sort_by_key(|s| s.id);
        Ok(result)
    }
    fn preset(&self, value: &graph::Preset) -> Result<graph::Preset> {
        let mut result = value.clone();
        result.id = self.presets[&value.id];
        result.definition = self
            .definitions
            .get(&value.definition)
            .copied()
            .unwrap_or(value.definition);
        for assignment in &mut result.assignments {
            assignment.target = self.path(value.definition, &assignment.target)?;
            if let graph::Source::Forward(path) = &mut assignment.source {
                *path = self.path(value.definition, path)?;
            }
        }
        Ok(result)
    }
}

/// Return the complete restored authored source and desired real roots. The
/// caller publishes this only after final class/layout/connection validation.
pub(super) fn restore(
    seed: &otrios_interfaces::oib::Seed,
    resolved: &graph::Document,
    source: &DocumentSource,
) -> Result<(DocumentSource, Vec<Allocation>)> {
    let signed = seed.graph.as_ref().ok_or("Missing signed baseline")?;
    let mut next = source.next_identity;
    let mut map = Remap {
        signed,
        resolved: &resolved.definitions,
        definitions: BTreeMap::new(),
        nodes: BTreeMap::new(),
        slots: BTreeMap::new(),
        presets: BTreeMap::new(),
        instances: BTreeMap::new(),
    };
    for definition in &signed.definitions {
        let current = source
            .baseline
            .iter()
            .find_map(|role| match role {
                BaselineIdentity::Definition { source, current } if *source == definition.id => {
                    Some(*current)
                }
                _ => None,
            })
            .unwrap_or(definition.id);
        let present = source.definitions.iter().find(|d| d.id == current);
        map.definitions.insert(
            definition.id,
            if present.is_some() {
                current
            } else {
                DefinitionId(allocate(&mut next)?)
            },
        );
        for node in &definition.nodes {
            let current = source
                .baseline
                .iter()
                .find_map(|role| match role {
                    BaselineIdentity::Node {
                        definition: owner,
                        source,
                        current,
                    } if *owner == definition.id && *source == node.id => Some(*current),
                    _ => None,
                })
                .unwrap_or(node.id);
            map.nodes.insert(
                (definition.id, node.id),
                if present.is_some_and(|d| d.nodes.iter().any(|n| n.id == current)) {
                    current
                } else {
                    NodeId(allocate(&mut next)?)
                },
            );
        }
        for slot in &definition.slots {
            let current = source
                .baseline
                .iter()
                .find_map(|role| match role {
                    BaselineIdentity::Slot {
                        definition: owner,
                        source,
                        current,
                    } if *owner == definition.id && *source == slot.id => Some(*current),
                    _ => None,
                })
                .unwrap_or(slot.id);
            map.slots.insert(
                (definition.id, slot.id),
                if present.is_some_and(|d| d.slots.iter().any(|s| s.id == current)) {
                    current
                } else {
                    SlotId(allocate(&mut next)?)
                },
            );
        }
    }
    for preset in &signed.presets {
        let current = source
            .baseline
            .iter()
            .find_map(|role| match role {
                BaselineIdentity::Preset { source, current } if *source == preset.id => {
                    Some(*current)
                }
                _ => None,
            })
            .unwrap_or(preset.id);
        map.presets.insert(
            preset.id,
            if source.presets.iter().any(|p| p.id == current) {
                current
            } else {
                allocate(&mut next)?
            },
        );
    }
    for instance in &signed.instances {
        let current = source
            .baseline
            .iter()
            .find_map(|role| match role {
                BaselineIdentity::Instance { source, current } if *source == instance.id => {
                    Some(*current)
                }
                _ => None,
            })
            .unwrap_or(instance.id);
        let fixed = signed
            .presets
            .iter()
            .any(|p| p.id == instance.preset && !p.palette);
        map.instances.insert(
            instance.id,
            if fixed || source.instances.iter().any(|i| i.id == current.0) {
                current
            } else {
                InstanceId(allocate(&mut next)?)
            },
        );
    }
    for instance in &signed.instances {
        let preset = signed
            .presets
            .iter()
            .find(|p| p.id == instance.preset)
            .ok_or("Missing baseline preset")?;
        if !preset.palette {
            let definition = map.definition(preset.definition)?;
            if map.instances[&instance.id] != instance.id
                || map.presets[&preset.id] != preset.id
                || map.definitions[&definition.id] != definition.id
                || map.node(definition.id, definition.root)? != definition.root
            {
                return Err("Fixed document root identity changed".into());
            }
        }
    }
    let mut result = source.clone();
    result.definitions = signed
        .definitions
        .iter()
        .map(|d| map.component(d))
        .collect::<Result<_>>()?;
    result.definitions.sort_by_key(|d| d.id);
    result.presets = signed
        .presets
        .iter()
        .map(|p| map.preset(p))
        .collect::<Result<_>>()?;
    result.presets.sort_by_key(|p| p.id);
    result.connections.clear();
    // Signed initial roots are canvas-owned. Restore discards authored slot
    // placements; owned initial content is outside this graph contract.
    result.placements.clear();
    result.baseline = map
        .definitions
        .iter()
        .map(|(source, current)| BaselineIdentity::Definition {
            source: *source,
            current: *current,
        })
        .chain(
            map.nodes
                .iter()
                .map(|((definition, source), current)| BaselineIdentity::Node {
                    definition: *definition,
                    source: *source,
                    current: *current,
                }),
        )
        .chain(
            map.slots
                .iter()
                .map(|((definition, source), current)| BaselineIdentity::Slot {
                    definition: *definition,
                    source: *source,
                    current: *current,
                }),
        )
        .chain(
            map.presets
                .iter()
                .map(|(source, current)| BaselineIdentity::Preset {
                    source: *source,
                    current: *current,
                }),
        )
        .chain(
            map.instances
                .iter()
                .map(|(source, current)| BaselineIdentity::Instance {
                    source: *source,
                    current: *current,
                }),
        )
        .collect();
    let mut initial: Vec<_> = signed
        .instances
        .iter()
        .filter(|i| {
            i.id != signed.presented_root
                && signed.presets.iter().any(|p| p.id == i.preset && p.palette)
        })
        .collect();
    initial.sort_by_key(|i| (i.order, i.id));
    let allocations: Vec<_> = initial
        .into_iter()
        .enumerate()
        .map(|(slot, i)| {
            let original_preset = signed
                .presets
                .iter()
                .find(|p| p.id == i.preset)
                .expect("filtered signed preset");
            Allocation {
                id: map.instances[&i.id].0,
                name: i.name.clone(),
                component: map
                    .definitions
                    .get(&original_preset.definition)
                    .copied()
                    .unwrap_or(original_preset.definition)
                    .0,
                prototype: map.presets[&i.preset],
                slot: slot as u16,
            }
        })
        .collect();
    result.next_identity = next;
    Ok((result, allocations))
}
