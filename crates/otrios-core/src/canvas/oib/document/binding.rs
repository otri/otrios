// SPDX-License-Identifier: LGPL-3.0-only
//! Derive a binding effect from admitted concrete control endpoints, never a type alone.
use super::*;

// These are the existing path and forwarding ceilings used by path_node and
// bound_source. Proofs retain their remaining depths so memoization cannot make
// a longer route appear shorter after another branch has visited its terminal.
const MAX_REFERENCES: usize = 8;
const MAX_ALIASES: usize = 64;
const MAX_ENDPOINTS: usize = MAX_AUTHORED_OBJECTS * MAX_PROPERTY_DESCRIPTORS;
type Endpoint = (DefinitionId, NodeId, PropertyId);

#[derive(Clone, Copy)]
struct Proof {
    kind: ClassKind,
    references: usize,
    aliases: usize,
}
struct Resolver<'a> {
    classes: &'a [ClassDescriptor],
    definitions: &'a [ComponentDefinition],
    active: BTreeSet<Endpoint>,
    done: BTreeMap<Endpoint, Proof>,
}
impl<'a> Resolver<'a> {
    fn definition(&self, id: DefinitionId) -> Result<&'a ComponentDefinition> {
        self.definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| "Unresolved binding definition".into())
    }
    fn terminal(
        &self,
        start: DefinitionId,
        path: &PropertyPath,
    ) -> Result<(&'a ComponentDefinition, &'a DefinitionNode)> {
        if path.references.len() > MAX_REFERENCES {
            return Err("Binding reference depth exceeded".into());
        }
        let mut definition = self.definition(start)?;
        let mut seen = BTreeSet::from([start]);
        for reference in &path.references {
            let next = definition
                .nodes
                .iter()
                .find(|node| node.id == *reference)
                .and_then(|node| node.reference.as_ref())
                .ok_or("Unresolved binding reference boundary")?
                .definition;
            if !seen.insert(next) {
                return Err("Cyclic binding reference path".into());
            }
            definition = self.definition(next)?;
        }
        let node = definition
            .nodes
            .iter()
            .find(|node| node.id == path.node)
            .ok_or("Unresolved binding endpoint node")?;
        Ok((definition, node))
    }
    fn descriptor(
        &self,
        definition: &ComponentDefinition,
        node: &DefinitionNode,
        property: PropertyId,
    ) -> Result<PropertyDescriptor> {
        node_descriptors_for(self.classes, definition, node)?
            .into_iter()
            .find(|p| p.id == property)
            .ok_or_else(|| "Unresolved binding endpoint property".into())
    }
    fn proof(&mut self, start: DefinitionId, path: &PropertyPath) -> Result<Proof> {
        let (definition, node) = self.terminal(start, path)?;
        let endpoint = (definition.id, node.id, path.property);
        let mut proof = if let Some(proof) = self.done.get(&endpoint) {
            *proof
        } else {
            if self.active.len() >= MAX_ALIASES
                || self.active.len() + self.done.len() >= MAX_ENDPOINTS
                || !self.active.insert(endpoint)
            {
                return Err("Cyclic or excessive binding forwarding".into());
            }
            let descriptor = self.descriptor(definition, node, path.property)?;
            if !descriptor.policy.app_binding {
                return Err("Binding endpoint does not admit app bindings".into());
            }
            let class = self
                .classes
                .iter()
                .find(|class| class.id == node.class)
                .ok_or("Unresolved binding endpoint class")?;
            // A declared Number on a Slider-class root is not a signed class
            // endpoint. It still needs an outgoing route to the actual control.
            let inherited = class_descriptors(self.classes, node.class)?;
            let concrete = inherited.iter().any(|p| p.id == descriptor.id)
                && matches!(
                    (class.implementation, descriptor.value_type),
                    (ClassKind::O2DSlider, ValueType::Number)
                        | (ClassKind::O2DToggle, ValueType::Boolean)
                );
            let proof = if concrete {
                Proof {
                    kind: class.implementation,
                    references: 0,
                    aliases: 1,
                }
            } else {
                if node.id != definition.root
                    || !definition.properties.iter().any(|p| p.id == descriptor.id)
                    || descriptor.policy.live_literal
                    || descriptor.editor != PropertyEditor::ReadOnly
                    || !matches!(
                        descriptor.value_type,
                        ValueType::Number | ValueType::Boolean
                    )
                {
                    return Err(
                        "A bound root endpoint requires a declared read-only control route".into(),
                    );
                }
                let mut rows = definition
                    .exposed
                    .iter()
                    .filter(|row| row.property == path.property);
                let row = rows
                    .next()
                    .ok_or("A bound root endpoint requires an outgoing control route")?;
                if rows.next().is_some()
                    || row.targets.is_empty()
                    || row.targets.len() > MAX_PROPERTY_DESCRIPTORS
                    || row.targets.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err("Ambiguous or empty bound endpoint forwarding".into());
                }
                let mut result: Option<Proof> = None;
                for target in &row.targets {
                    let (target_definition, target_node) = self.terminal(definition.id, target)?;
                    let target_descriptor =
                        self.descriptor(target_definition, target_node, target.property)?;
                    if descriptor.value_type != target_descriptor.value_type
                        || descriptor.number_range != target_descriptor.number_range
                    {
                        return Err(
                            "Bound endpoint type and range must match its control targets".into(),
                        );
                    }
                    let next = self.proof(definition.id, target)?;
                    if let Some(previous) = &mut result {
                        if previous.kind != next.kind {
                            return Err(
                                "Bound endpoint targets require one concrete control effect".into(),
                            );
                        }
                        previous.references = previous.references.max(next.references);
                        previous.aliases = previous.aliases.max(next.aliases);
                    } else {
                        result = Some(next);
                    }
                }
                let mut result = result.ok_or("Missing bound endpoint target")?;
                result.aliases += 1;
                result
            };
            self.active.remove(&endpoint);
            self.done.insert(endpoint, proof);
            proof
        };
        proof.references += path.references.len();
        if proof.references > MAX_REFERENCES || proof.aliases > MAX_ALIASES {
            return Err("Binding forwarding depth exceeded".into());
        }
        Ok(proof)
    }
}

/// Proof of the existing Slider/Set or Toggle/Toggle effect reached by a property.
/// The caller must still validate the exact signed app field/effect grant.
pub(in crate::canvas) fn binding_kind(
    classes: &[ClassDescriptor],
    definitions: &[ComponentDefinition],
    start: DefinitionId,
    path: &PropertyPath,
) -> Result<ClassKind> {
    if classes.len() > MAX_CLASS_DESCRIPTORS
        || definitions.len() > MAX_DEFINITIONS
        || definitions.iter().map(|d| d.nodes.len()).sum::<usize>() > MAX_AUTHORED_OBJECTS
        || classes.iter().map(|c| c.properties.len()).sum::<usize>()
            + definitions
                .iter()
                .map(|d| d.properties.len())
                .sum::<usize>()
            > MAX_PROPERTY_DESCRIPTORS
        || classes.iter().map(|c| c.id).collect::<BTreeSet<_>>().len() != classes.len()
        || definitions
            .iter()
            .map(|d| d.id)
            .collect::<BTreeSet<_>>()
            .len()
            != definitions.len()
        || definitions.iter().any(|d| {
            d.nodes
                .iter()
                .map(|node| node.id)
                .collect::<BTreeSet<_>>()
                .len()
                != d.nodes.len()
        })
    {
        return Err("Ambiguous or oversized binding contract source".into());
    }
    fn references(
        definitions: &[ComponentDefinition],
        id: DefinitionId,
        active: &mut BTreeSet<DefinitionId>,
        done: &mut BTreeSet<DefinitionId>,
    ) -> Result<()> {
        if active.contains(&id) {
            return Err("Cyclic binding component references".into());
        }
        if done.contains(&id) {
            return Ok(());
        }
        let definition = definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or("Unresolved binding reference definition")?;
        active.insert(id);
        for reference in definition
            .nodes
            .iter()
            .filter_map(|node| node.reference.as_ref())
        {
            references(definitions, reference.definition, active, done)?;
        }
        active.remove(&id);
        done.insert(id);
        Ok(())
    }
    let mut visited = BTreeSet::new();
    for definition in definitions {
        references(
            definitions,
            definition.id,
            &mut BTreeSet::new(),
            &mut visited,
        )?;
    }
    let mut resolver = Resolver {
        classes,
        definitions,
        active: BTreeSet::new(),
        done: BTreeMap::new(),
    };
    resolver.proof(start, path).map(|proof| proof.kind)
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
