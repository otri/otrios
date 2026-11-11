// SPDX-License-Identifier: LGPL-3.0-only
//! Definition transforms over the canonical OIB source graph.
//!
//! This is a compiler primitive, not an authority endpoint. It cannot publish,
//! acknowledge, authorize a writer or overwrite a causal property register.
use super::*;
use otrios_interfaces::oib::authoring::{
    DefinitionBatch, DefinitionEdit, LiteralSource, MAX_BATCH_BYTES, MAX_OPERATIONS,
};

/// Transform one candidate and validate it only after all explicit repairs.
/// Failure leaves the caller's UI and its immutable layout bytes untouched.
pub fn apply(ui: &Ui, batch: &DefinitionBatch) -> Result<Ui> {
    if batch.generation == 0
        || batch.operations.is_empty()
        || batch.operations.len() > MAX_OPERATIONS
        || flexbuffers::to_vec(batch).map_err(err)?.len() > MAX_BATCH_BYTES
    {
        return Err("OIB definition batch budget or generation".into());
    }
    super::super::validate(ui)?;
    let original = super::super::seed(ui).ok_or("Missing OIB graph")?;
    let definitions = transform(original, &batch.operations)?;
    let candidate = restore(ui, &definitions)?;
    super::super::validate(&candidate)?;
    Ok(candidate)
}

/// Reconstruct a mutable definition source against the immutable signed classes
/// and field/effect grants. Renderer output is regenerated, never accepted from
/// an old parallel layout or a caller-provided mapping table.
pub fn restore_seed(signed: &Seed, definitions: &[ComponentDefinition]) -> Result<Seed> {
    let graph = signed.graph.as_ref().ok_or("Missing canonical graph")?;
    let candidate = super::super::graph::with_seed_source(signed, definitions, &graph.presets)?;
    super::super::validate_seed(&candidate)?;
    Ok(candidate)
}

pub fn restore(ui: &Ui, definitions: &[ComponentDefinition]) -> Result<Ui> {
    let graph = super::super::seed(ui)
        .and_then(|s| s.graph.as_ref())
        .ok_or("Missing canonical graph")?;
    let candidate = super::super::graph::with_source(ui, definitions, &graph.presets)?;
    super::super::validate(&candidate)?;
    Ok(candidate)
}

fn definition_mut(seed: &mut Seed, id: DefinitionId) -> Result<&mut ComponentDefinition> {
    seed.designer
        .as_mut()
        .ok_or("Missing OIB designer document")?
        .definitions
        .iter_mut()
        .find(|d| d.id == id)
        .ok_or_else(|| "Unknown OIB definition".into())
}

/// Pure source transform used by the authority before preset and resident
/// repairs. This intentionally defers complete graph validation until the
/// enclosing structural batch has staged every explicit repair.
pub fn transform(seed: &Seed, operations: &[DefinitionEdit]) -> Result<Vec<ComponentDefinition>> {
    let mut candidate = seed.clone();
    let imported = super::super::imports::imported_definitions(seed);
    for operation in operations {
        let definition = match operation {
            DefinitionEdit::CreateDefinition { definition, .. }
            | DefinitionEdit::DeleteDefinition { definition }
            | DefinitionEdit::CreateNode { definition, .. }
            | DefinitionEdit::DeleteNode { definition, .. }
            | DefinitionEdit::SetVisual { definition, .. }
            | DefinitionEdit::SetSlots { definition, .. }
            | DefinitionEdit::SetDefault { definition, .. }
            | DefinitionEdit::ClearDefault { definition, .. }
            | DefinitionEdit::RenameDefinition { definition, .. }
            | DefinitionEdit::RenameNode { definition, .. }
            | DefinitionEdit::MoveNode { definition, .. }
            | DefinitionEdit::Expose { definition, .. }
            | DefinitionEdit::ConnectReference { definition, .. } => definition,
        };
        if imported.contains(definition) {
            return Err(
                "Imported library definitions are read-only; create a new local component".into(),
            );
        }
        edit(&mut candidate, operation)?;
    }
    Ok(metadata(&candidate)?
        .definitions
        .iter()
        .filter(|definition| !imported.contains(&definition.id))
        .cloned()
        .collect())
}

fn name(value: &str) -> Result<()> {
    if valid_text(value, 64) {
        Ok(())
    } else {
        Err("OIB names require 1–64 NUL-free UTF-8 bytes".into())
    }
}

fn edit(seed: &mut Seed, operation: &DefinitionEdit) -> Result<()> {
    match operation {
        DefinitionEdit::CreateDefinition {
            definition,
            name: label,
            root,
        } => {
            name(label)?;
            name(&root.name)?;
            if definition.0 == 0
                || root.id.0 == 0
                || root.owner.is_some()
                || !root.visual_children.is_empty()
            {
                return Err(
                    "New definition requires a fresh root with no owner or children".into(),
                );
            }
            admissible_reference(root)?;
            let document = seed.designer.as_mut().ok_or("Missing OIB document")?;
            if document
                .definitions
                .iter()
                .any(|current| current.id == *definition)
            {
                return Err("Definition identity already exists".into());
            }
            document.definitions.push(ComponentDefinition {
                id: *definition,
                name: label.clone(),
                root: root.id,
                nodes: vec![root.clone()],
                exposed: vec![],
                slots: vec![],
            });
            document.definitions.sort_by_key(|definition| definition.id);
        }
        DefinitionEdit::DeleteDefinition { definition } => {
            let document = seed.designer.as_mut().ok_or("Missing OIB document")?;
            let index = document
                .definitions
                .iter()
                .position(|current| current.id == *definition)
                .ok_or("Unknown OIB definition")?;
            document.definitions.remove(index);
        }
        DefinitionEdit::CreateNode {
            definition,
            node,
            visual_parent,
            before,
        } => {
            name(&node.name)?;
            admissible_reference(node)?;
            if node.id.0 == 0
                || node.owner.is_none()
                || !node.visual_children.is_empty()
                || visual_parent.is_none() && before.is_some()
            {
                return Err("New node requires an owner and an empty child list".into());
            }
            let def = definition_mut(seed, *definition)?;
            if def.nodes.iter().any(|existing| existing.id == node.id) {
                return Err("Node identity already exists".into());
            }
            let owner = def
                .nodes
                .iter()
                .find(|existing| Some(existing.id) == node.owner)
                .ok_or("Unknown ownership parent")?;
            if owner.reference.is_some() {
                return Err("Inherited reference content is not an owned child list".into());
            }
            if let Some(parent) = visual_parent {
                let parent = def
                    .nodes
                    .iter_mut()
                    .find(|existing| existing.id == *parent)
                    .ok_or("Unknown visual parent")?;
                if parent.reference.is_some() {
                    return Err("Inherited reference content is not a visual child list".into());
                }
                let index = before
                    .map(|before| {
                        parent
                            .visual_children
                            .iter()
                            .position(|id| *id == before)
                            .ok_or("Unknown visual anchor")
                    })
                    .transpose()?
                    .unwrap_or(parent.visual_children.len());
                parent.visual_children.insert(index, node.id);
            }
            def.nodes.push(node.clone());
            def.nodes.sort_by_key(|node| node.id);
        }
        DefinitionEdit::DeleteNode { definition, node } => {
            let def = definition_mut(seed, *definition)?;
            if *node == def.root {
                return Err("Delete the definition to remove its root".into());
            }
            let index = def
                .nodes
                .iter()
                .position(|existing| existing.id == *node)
                .ok_or("Unknown definition node")?;
            def.nodes.remove(index);
            for parent in &mut def.nodes {
                parent.visual_children.retain(|child| child != node);
            }
        }
        DefinitionEdit::SetVisual {
            definition,
            node,
            visual,
        } => {
            definition_mut(seed, *definition)?
                .nodes
                .iter_mut()
                .find(|existing| existing.id == *node)
                .ok_or("Unknown definition node")?
                .visual = visual.clone();
        }
        DefinitionEdit::SetDefault {
            definition,
            node,
            property,
            value,
        } => {
            let document = seed.designer.as_ref().ok_or("Missing OIB document")?;
            let current = document
                .definitions
                .iter()
                .find(|current| current.id == *definition)
                .and_then(|definition| definition.nodes.iter().find(|current| current.id == *node))
                .ok_or("Unknown definition node")?;
            let descriptor = super::descriptors(document, current.class)?
                .into_iter()
                .find(|descriptor| descriptor.id == *property)
                .ok_or("Unknown definition property")?;
            if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
                return Err("OIB property is read-only".into());
            }
            super::validate_property_value(&descriptor, value)?;
            let current = definition_mut(seed, *definition)?
                .nodes
                .iter_mut()
                .find(|current| current.id == *node)
                .ok_or("Unknown definition node")?;
            if let Some(default) = current
                .defaults
                .iter_mut()
                .find(|default| default.property == *property)
            {
                default.value = value.clone();
            } else {
                current.defaults.push(PropertyDefault {
                    property: *property,
                    value: value.clone(),
                });
                current.defaults.sort_by_key(|default| default.property);
            }
        }
        DefinitionEdit::ClearDefault {
            definition,
            node,
            property,
        } => {
            let document = seed.designer.as_ref().ok_or("Missing OIB document")?;
            let current = document
                .definitions
                .iter()
                .find(|d| d.id == *definition)
                .and_then(|d| d.nodes.iter().find(|n| n.id == *node))
                .ok_or("Unknown definition node")?;
            let descriptor = super::descriptors(document, current.class)?
                .into_iter()
                .find(|d| d.id == *property)
                .ok_or("Unknown definition property")?;
            if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
                return Err("OIB property is read-only".into());
            }
            definition_mut(seed, *definition)?
                .nodes
                .iter_mut()
                .find(|n| n.id == *node)
                .ok_or("Unknown definition node")?
                .defaults
                .retain(|d| d.property != *property);
        }
        DefinitionEdit::SetSlots { definition, slots } => {
            if slots.len() > 16 || slots.windows(2).any(|pair| pair[0].id >= pair[1].id) {
                return Err("Slot declarations must be bounded and ordered by identity".into());
            }
            definition_mut(seed, *definition)?.slots = slots.clone();
        }
        DefinitionEdit::RenameDefinition {
            definition,
            name: n,
        } => {
            name(n)?;
            definition_mut(seed, *definition)?.name = n.clone();
        }
        DefinitionEdit::RenameNode {
            definition,
            node,
            name: n,
        } => {
            name(n)?;
            definition_mut(seed, *definition)?
                .nodes
                .iter_mut()
                .find(|n| n.id == *node)
                .ok_or("Unknown OIB definition node")?
                .name = n.clone();
        }
        DefinitionEdit::MoveNode {
            definition,
            node,
            owner,
            visual_parent,
            before,
        } => {
            let def = definition_mut(seed, *definition)?;
            if *node == def.root
                || *node == *owner
                || *visual_parent == Some(*node)
                || *before == Some(*node)
                || (visual_parent.is_none() && before.is_some())
            {
                return Err("Invalid OIB definition move target".into());
            }
            let parent = def
                .nodes
                .iter()
                .find(|n| n.id == *owner)
                .ok_or("Unknown OIB ownership parent")?;
            if parent.reference.is_some() {
                return Err("Inherited reference content is not an owned child list".into());
            }
            if let Some(parent) = visual_parent {
                let parent = def
                    .nodes
                    .iter()
                    .find(|n| n.id == *parent)
                    .ok_or("Unknown OIB visual parent")?;
                if parent.reference.is_some()
                    || before.is_some_and(|a| !parent.visual_children.contains(&a))
                {
                    return Err("Invalid OIB visual parent or sibling anchor".into());
                }
            }
            def.nodes
                .iter_mut()
                .find(|n| n.id == *node)
                .ok_or("Unknown OIB moved node")?
                .owner = Some(*owner);
            for parent in &mut def.nodes {
                parent.visual_children.retain(|id| id != node);
            }
            if let Some(parent) = visual_parent {
                let children = &mut def
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *parent)
                    .ok_or("Unknown OIB visual parent")?
                    .visual_children;
                let index = before
                    .map(|id| {
                        children
                            .iter()
                            .position(|n| *n == id)
                            .ok_or("Unknown OIB sibling anchor")
                    })
                    .transpose()?
                    .unwrap_or(children.len());
                children.insert(index, *node);
            }
        }
        DefinitionEdit::Expose {
            definition,
            property,
            targets,
        } => {
            if targets.len() > 64 || targets.windows(2).any(|w| w[0] >= w[1]) {
                return Err("Noncanonical OIB forwarding targets".into());
            }
            let root = self::definition(metadata(seed)?, *definition)?.root;
            let descriptor = self::property(
                metadata(seed)?,
                *definition,
                &PropertyPath {
                    references: vec![],
                    node: root,
                    property: *property,
                },
            )?;
            if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
                return Err("Exposed property requires an admitted writable root property".into());
            }
            let def = definition_mut(seed, *definition)?;
            if let Some(exposed) = def
                .exposed
                .iter_mut()
                .find(|exposed| exposed.property == *property)
            {
                exposed.targets = targets.clone();
            } else {
                def.exposed.push(ExposedProperty {
                    property: *property,
                    targets: targets.clone(),
                });
                def.exposed.sort_by_key(|exposed| exposed.property);
            }
        }
        DefinitionEdit::ConnectReference {
            definition,
            reference,
            target,
            source,
        } => {
            let doc = metadata(seed)?;
            let def = self::definition(doc, *definition)?;
            let linked = def
                .nodes
                .iter()
                .find(|n| n.id == *reference)
                .and_then(|n| n.reference.as_ref())
                .ok_or("Unknown OIB reference boundary")?;
            if source.is_some() {
                let descriptor = property(doc, linked.definition, target)?;
                if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly
                {
                    return Err("OIB app sources require signed binding/action admission".into());
                }
            }
            // Canonical references cannot carry app bindings. Explicit removal
            // must remain possible after its target was deleted earlier in the
            // same batch; final source validation checks all surviving edges.
            let assignments = &mut definition_mut(seed, *definition)?
                .nodes
                .iter_mut()
                .find(|n| n.id == *reference)
                .and_then(|n| n.reference.as_mut())
                .ok_or("Unknown OIB reference boundary")?
                .assignments;
            assignments.retain(|a| a.target != *target);
            if let Some(source) = source {
                assignments.push(Assignment {
                    target: target.clone(),
                    source: match source {
                        LiteralSource::Literal(v) => AssignmentSource::Literal(v.clone()),
                        LiteralSource::Forward(p) => AssignmentSource::Forward(p.clone()),
                    },
                });
            }
            assignments.sort_by(|a, b| a.target.cmp(&b.target));
        }
    }
    Ok(())
}

fn admissible_reference(node: &DefinitionNode) -> Result<()> {
    if node.reference.as_ref().is_some_and(|reference| {
        reference
            .assignments
            .iter()
            .any(|assignment| matches!(assignment.source, AssignmentSource::AppBinding(_)))
    }) {
        return Err("Reference assignments use literal or forwarded properties; app sources belong to typed presets".into());
    }
    Ok(())
}
