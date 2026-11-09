// SPDX-License-Identifier: LGPL-3.0-only
//! Disposable D2 definition transforms over the signed D1 lowering.
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
    // The input must already be admitted, including render/control correspondence.
    super::super::validate(ui)?;
    let original = super::super::seed(ui).ok_or("Missing OIB seed")?;
    let mut seed = original.clone();
    for operation in &batch.operations {
        edit(&mut seed, operation)?;
    }
    let seed = restore_seed(original, &metadata(&seed)?.definitions)?;
    let mut candidate = ui.clone();
    candidate.layout = project_layout(ui, &seed)?;
    candidate.presentation = Some(Presentation::Oib(Box::new(seed)));
    super::super::validate(&candidate)?;
    Ok(candidate)
}

/// Reconstruct structural data only against its immutable signed admission
/// surface. This checks recovery snapshots as strictly as newly authored data.
/// Renderer mappings, node classes/defaults, reference definitions and slots
/// cannot be supplied or replaced by this snapshot format.
pub fn restore_seed(signed: &Seed, definitions: &[ComponentDefinition]) -> Result<Seed> {
    let original = metadata(signed)?;
    if definitions.len() != original.definitions.len() {
        return Err("OIB definition snapshot identity mismatch".into());
    }
    for (old, new) in original.definitions.iter().zip(definitions) {
        if old.id != new.id
            || old.root != new.root
            || old.slots != new.slots
            || old.nodes.len() != new.nodes.len()
            || old.exposed.iter().map(|p| p.property).collect::<Vec<_>>()
                != new.exposed.iter().map(|p| p.property).collect::<Vec<_>>()
        {
            return Err("OIB definition snapshot changed signed admission".into());
        }
        for (a, b) in old.nodes.iter().zip(&new.nodes) {
            if a.id != b.id
                || a.class != b.class
                || a.defaults != b.defaults
                || a.reference.as_ref().map(|r| r.definition)
                    != b.reference.as_ref().map(|r| r.definition)
            {
                return Err("OIB node snapshot changed signed admission".into());
            }
            if let (Some(a), Some(b)) = (&a.reference, &b.reference) {
                let bindings = |r: &ComponentReference| {
                    r.assignments
                        .iter()
                        .filter(|a| matches!(a.source, AssignmentSource::AppBinding(_)))
                        .cloned()
                        .collect::<Vec<_>>()
                };
                if bindings(a) != bindings(b) {
                    return Err("OIB snapshot changed signed app bindings".into());
                }
            }
        }
    }
    let mut candidate = signed.clone();
    candidate
        .designer
        .as_mut()
        .ok_or("Missing OIB document")?
        .definitions = definitions.to_vec();
    for component in &mut candidate.components {
        component.name = definitions
            .iter()
            .find(|d| d.id.0 == component.id)
            .ok_or("Unknown OIB snapshot component")?
            .name
            .clone();
    }
    let current = metadata(&candidate)?;
    // Indirectly app-bound endpoints are protected too. A literal or forwarding
    // edit must not disconnect a binding that the signed document owns.
    for root in &original.instances {
        let before = sources(original, root)?;
        let after = sources(current, instance(current, root.id)?)?;
        for path in before.keys() {
            if let Some(binding) = bound_source(&before, path, &mut BTreeSet::new())?
                && bound_source(&after, path, &mut BTreeSet::new())? != Some(binding)
            {
                return Err("OIB snapshot disconnected a signed app source".into());
            }
        }
    }
    synchronize_literals(&mut candidate)?;
    super::super::validate_seed(&candidate)?;
    Ok(candidate)
}

pub fn restore(ui: &Ui, definitions: &[ComponentDefinition]) -> Result<Ui> {
    let seed = restore_seed(
        super::super::seed(ui).ok_or("Missing OIB seed")?,
        definitions,
    )?;
    let mut candidate = ui.clone();
    candidate.layout = project_layout(ui, &seed)?;
    candidate.presentation = Some(Presentation::Oib(Box::new(seed)));
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

fn name(value: &str) -> Result<()> {
    if valid_text(value, 64) {
        Ok(())
    } else {
        Err("OIB names require 1–64 NUL-free UTF-8 bytes".into())
    }
}

fn edit(seed: &mut Seed, operation: &DefinitionEdit) -> Result<()> {
    match operation {
        DefinitionEdit::RenameDefinition {
            definition,
            name: n,
        } => {
            name(n)?;
            definition_mut(seed, *definition)?.name = n.clone();
            seed.components
                .iter_mut()
                .find(|c| c.id == definition.0)
                .ok_or("Missing OIB component")?
                .name = n.clone();
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
            let def = definition_mut(seed, *definition)?;
            def.exposed
                .iter_mut()
                .find(|p| p.property == *property)
                .ok_or("Only existing exposed OIB properties are editable")?
                .targets = targets.clone();
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
            let descriptor = property(doc, linked.definition, target)?;
            if !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
                return Err("OIB app sources require signed binding/action admission".into());
            }
            // This API must never disconnect a signed app binding, including
            // one forwarded indirectly to an otherwise editable endpoint.
            let root = ComponentInstance {
                id: InstanceId(1),
                name: "source-check".into(),
                definition: def.id,
                owner: Owner::Document,
                visual_parent: None,
                order: 0,
                assignments: vec![],
            };
            let mut full = target.clone();
            full.references.insert(0, *reference);
            if bound_source(&sources(doc, &root)?, &full, &mut BTreeSet::new())?.is_some() {
                return Err("OIB app-bound source is protected".into());
            }
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

fn synchronize_literals(seed: &mut Seed) -> Result<()> {
    let doc = metadata(seed)?;
    let mut literals = BTreeMap::new();
    for table in &doc.mappings.roots {
        let root = instance(doc, table.prototype)?;
        let assignments = sources(doc, root)?;
        for row in &table.cells {
            let literal = match assignments.get(&row.path) {
                Some(Source::Literal(v)) => Some(v.clone()),
                _ => None,
            };
            literals.insert(row.initial_cell, literal);
        }
    }
    for cell in &mut seed.cells {
        if let Some(literal) = literals.get(&cell.id) {
            cell.authored_override = literal.clone();
        }
    }
    Ok(())
}

/// Rebuild authored containment from full object identities. Original render
/// records supply painting, geometry, bindings and responders; a caller cannot
/// smuggle a replacement renderer or event handler through a structural edit.
fn project_layout(ui: &Ui, seed: &Seed) -> Result<Vec<u8>> {
    let original = super::super::seed(ui).ok_or("Missing OIB seed")?;
    let original_doc = metadata(original)?;
    let doc = metadata(seed)?;
    let mut blueprint = Layout::from_bytes(&ui.layout)?.blueprint().clone();
    let mut nodes = BTreeMap::new();
    fn flatten(n: &TemplateNode, out: &mut BTreeMap<u16, TemplateNode>) {
        out.insert(n.id, n.clone());
        for c in &n.children {
            flatten(c, out);
        }
    }
    for template in &blueprint.templates {
        flatten(&template.root, &mut nodes);
    }
    let mapping: BTreeMap<_, _> = original_doc
        .mappings
        .objects
        .iter()
        .filter_map(|m| m.layout_node.map(|id| (id, m.address.clone())))
        .collect();
    fn build(
        id: u16,
        doc: &Document,
        nodes: &BTreeMap<u16, TemplateNode>,
        mapping: &BTreeMap<u16, ObjectAddress>,
        visiting: &mut BTreeSet<u16>,
        work: &mut usize,
    ) -> Result<TemplateNode> {
        *work += 1;
        if *work > 512 || visiting.len() >= 12 || !visiting.insert(id) {
            return Err("OIB definition projection cycle or resource limit".into());
        }
        let mut node = nodes.get(&id).ok_or("Missing OIB render template")?.clone();
        let mut children: Vec<u16> = node.children.iter().map(|n| n.id).collect();
        if let Some(address) = mapping.get(&id) {
            let root = instance(doc, address.instance)?;
            let (_, authored) = path_node(doc, root.definition, &address.references, address.node)?;
            let mut desired: Vec<_> = authored
                .visual_children
                .iter()
                .map(|child| ObjectAddress {
                    instance: address.instance,
                    references: address.references.clone(),
                    node: *child,
                })
                .collect();
            if let Some(reference) = &authored.reference {
                let mut references = address.references.clone();
                references.push(authored.id);
                desired.push(ObjectAddress {
                    instance: address.instance,
                    references,
                    node: definition(doc, reference.definition)?.root,
                });
            }
            let desired: Vec<_> = desired
                .iter()
                .map(|a| {
                    doc.mappings
                        .objects
                        .iter()
                        .find(|m| m.address == *a)
                        .and_then(|m| m.layout_node)
                        .ok_or_else(|| "Missing OIB child render mapping".to_owned())
                })
                .collect::<Result<_>>()?;
            // Document-owned component roots and unowned tooling are separate
            // from this definition's local children. Retain them in their order.
            children.retain(|child| {
                mapping.get(child).is_none_or(|a| {
                    a.instance != address.instance
                        || a.references != address.references
                            && !a.references.starts_with(&address.references)
                })
            });
            let mut ordered = desired;
            ordered.extend(children);
            children = ordered;
        }
        node.children = children
            .into_iter()
            .map(|id| build(id, doc, nodes, mapping, visiting, work))
            .collect::<Result<_>>()?;
        visiting.remove(&id);
        Ok(node)
    }
    let mut work = 0;
    for template in &mut blueprint.templates {
        template.root = build(
            template.root.id,
            doc,
            &nodes,
            &mapping,
            &mut BTreeSet::new(),
            &mut work,
        )?;
    }
    Layout::encode(&blueprint)
}
