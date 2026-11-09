// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded semantic graph queries and disposable compiler metadata.
//! Authored definitions live in the canonical graph; values remain causal registers.
use super::*;
use otrios_interfaces::layout::{Binding, ValueType};
use otrios_interfaces::oib::designer::Document;
use otrios_interfaces::oib::designer::*;
mod admission;
pub mod authoring;
pub mod connections;
mod hierarchy;
pub mod placement;
mod sources;
pub use admission::{validate, validate_layout};
pub use hierarchy::{HierarchyKey, HierarchyRow, HierarchyScope, hierarchy};
pub use sources::{
    AuthoredOrigin, PropertyRegisterView, PropertySources, SourceHop, SourceRole, property_sources,
};

#[derive(Clone, Debug)]
pub struct ObjectInfo {
    pub selection: Selection,
    pub name: String,
    pub class: ClassId,
    pub depth: u16,
    pub layout_node: Option<u16>,
}
#[derive(Clone, Debug)]
pub struct PropertyInfo {
    pub address: Option<PropertyAddress>,
    pub cell: Option<u64>,
    pub descriptor: PropertyDescriptor,
    pub value: Option<Value>,
    pub inherited: Option<Value>,
    pub origin: String,
    pub source: String,
    pub overridden: bool,
}
pub(super) fn metadata(seed: &Seed) -> Result<&Document> {
    seed.designer
        .as_ref()
        .ok_or_else(|| "Missing signed OIB designer document".into())
}
fn definition(doc: &Document, id: DefinitionId) -> Result<&ComponentDefinition> {
    doc.definitions
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "Unknown OIB definition".into())
}
fn instance(doc: &Document, id: InstanceId) -> Result<&ComponentInstance> {
    doc.instances
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "Unknown OIB root instance".into())
}
fn class(doc: &Document, id: ClassId) -> Result<&ClassDescriptor> {
    doc.classes
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "Unknown OIB class".into())
}
pub(super) fn descriptors(doc: &Document, id: ClassId) -> Result<Vec<PropertyDescriptor>> {
    let mut result = Vec::new();
    let mut current = Some(id);
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id) || seen.len() > 16 {
            return Err("Cyclic OIB class ancestry".into());
        }
        let c = class(doc, id)?;
        result.extend(c.properties.iter().cloned());
        current = c.base;
    }
    result.sort_by_key(|p| p.id);
    if result.windows(2).any(|p| p[0].id == p[1].id) {
        return Err("Duplicate inherited OIB property".into());
    }
    Ok(result)
}
pub(super) fn graph_property(
    seed: &Seed,
    root: InstanceId,
    path: &PropertyPath,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<(Option<u64>, Option<Value>, Option<Binding>)> {
    let doc = metadata(seed)?;
    let root = instance(doc, root)?;
    let source = sources(doc, root)?;
    let binding = bound_source(&source, path, &mut BTreeSet::new())?;
    let value = if binding.is_some() {
        None
    } else {
        resolve(
            seed,
            root,
            &source,
            path,
            values,
            None,
            &mut BTreeSet::new(),
        )?
    };
    Ok((occurrence_cell(doc, root.id, path), value, binding))
}
pub(super) fn graph_descriptor(
    seed: &Seed,
    root: InstanceId,
    path: &PropertyPath,
) -> Result<PropertyDescriptor> {
    let doc = metadata(seed)?;
    property(doc, instance(doc, root)?.definition, path)
}
pub(super) fn graph_authored_literal(
    seed: &Seed,
    root: InstanceId,
    path: &PropertyPath,
) -> Result<Option<Value>> {
    let doc = metadata(seed)?;
    Ok(match sources(doc, instance(doc, root)?)?.get(path) {
        Some(Source::Literal(value)) => Some(value.clone()),
        _ => None,
    })
}
fn path_node<'a>(
    doc: &'a Document,
    start: DefinitionId,
    references: &[NodeId],
    node: NodeId,
) -> Result<(&'a ComponentDefinition, &'a DefinitionNode)> {
    if references.len() > 8 {
        return Err("OIB reference depth exceeded".into());
    }
    let mut def = definition(doc, start)?;
    for boundary in references {
        let reference = def
            .nodes
            .iter()
            .find(|n| n.id == *boundary)
            .and_then(|n| n.reference.as_ref())
            .ok_or("Unresolved OIB reference boundary")?;
        def = definition(doc, reference.definition)?;
    }
    let n = def
        .nodes
        .iter()
        .find(|n| n.id == node)
        .ok_or("Unknown OIB definition node")?;
    Ok((def, n))
}
fn property(
    doc: &Document,
    start: DefinitionId,
    path: &PropertyPath,
) -> Result<PropertyDescriptor> {
    let (_, node) = path_node(doc, start, &path.references, path.node)?;
    descriptors(doc, node.class)?
        .into_iter()
        .find(|p| p.id == path.property)
        .ok_or_else(|| "Unknown OIB property descriptor".into())
}
fn fallback(doc: &Document, def: DefinitionId, path: &PropertyPath) -> Result<Value> {
    let (_, n) = path_node(doc, def, &path.references, path.node)?;
    Ok(n.defaults
        .iter()
        .find(|d| d.property == path.property)
        .map(|d| d.value.clone())
        .unwrap_or(property(doc, def, path)?.default))
}
fn terminal_cell(doc: &Document, start: DefinitionId, path: &PropertyPath) -> Result<Option<u64>> {
    let (d, n) = path_node(doc, start, &path.references, path.node)?;
    Ok(doc
        .mappings
        .definitions
        .iter()
        .find(|m| m.definition == d.id && m.node == n.id && m.property == path.property)
        .map(|m| m.cell))
}
fn prefix_path(prefix: &[NodeId], path: &PropertyPath) -> PropertyPath {
    let mut references = prefix.to_vec();
    references.extend(&path.references);
    PropertyPath {
        references,
        node: path.node,
        property: path.property,
    }
}
#[derive(Clone, Debug)]
enum Source {
    Literal(Value),
    Forward(PropertyPath),
    Bound(Binding),
}
type Sources = BTreeMap<PropertyPath, Source>;
fn insert_source(sources: &mut Sources, target: PropertyPath, source: Source) -> Result<()> {
    if sources.insert(target, source).is_some() {
        return Err("Multiple OIB assignments to one property".into());
    }
    Ok(())
}
fn definition_sources(
    doc: &Document,
    id: DefinitionId,
    prefix: &[NodeId],
    sources: &mut Sources,
    ancestry: &mut Vec<DefinitionId>,
) -> Result<()> {
    if ancestry.contains(&id) || ancestry.len() >= 8 {
        return Err("Cyclic or too deep OIB reference graph".into());
    }
    ancestry.push(id);
    let def = definition(doc, id)?;
    for exposed in &def.exposed {
        let from = PropertyPath {
            references: prefix.to_vec(),
            node: def.root,
            property: exposed.property,
        };
        for target in &exposed.targets {
            insert_source(
                sources,
                prefix_path(prefix, target),
                Source::Forward(from.clone()),
            )?;
        }
    }
    for n in &def.nodes {
        if let Some(reference) = &n.reference {
            let mut nested = prefix.to_vec();
            nested.push(n.id);
            definition_sources(doc, reference.definition, &nested, sources, ancestry)?;
            for assignment in &reference.assignments {
                let source = match &assignment.source {
                    AssignmentSource::Literal(v) => Source::Literal(v.clone()),
                    AssignmentSource::Forward(path) => Source::Forward(prefix_path(prefix, path)),
                    AssignmentSource::AppBinding(binding) => Source::Bound(*binding),
                };
                insert_source(sources, prefix_path(&nested, &assignment.target), source)?;
            }
        }
    }
    ancestry.pop();
    Ok(())
}
fn sources(doc: &Document, root: &ComponentInstance) -> Result<Sources> {
    let mut result = BTreeMap::new();
    definition_sources(doc, root.definition, &[], &mut result, &mut Vec::new())?;
    for a in &root.assignments {
        let source = match &a.source {
            AssignmentSource::Literal(v) => Source::Literal(v.clone()),
            AssignmentSource::AppBinding(binding) => Source::Bound(*binding),
            AssignmentSource::Forward(_) => {
                return Err("Top-level OIB forwarding has no source scope".into());
            }
        };
        insert_source(&mut result, a.target.clone(), source)?;
    }
    Ok(result)
}
fn bound_source(
    sources: &Sources,
    path: &PropertyPath,
    seen: &mut BTreeSet<PropertyPath>,
) -> Result<Option<Binding>> {
    if !seen.insert(path.clone()) || seen.len() > 64 {
        return Err("Cyclic OIB property forwarding".into());
    }
    match sources.get(path) {
        Some(Source::Bound(b)) => Ok(Some(*b)),
        Some(Source::Forward(from)) => bound_source(sources, from, seen),
        _ => Ok(None),
    }
}
pub fn cell_for_address(
    seed: &Seed,
    allocations: &[Allocation],
    address: &PropertyAddress,
) -> Result<u64> {
    if seed.designer.is_none() {
        return seed
            .cells
            .iter()
            .find_map(|cell| {
                (address_for_cell(seed, cell.id).ok().as_ref() == Some(address)).then_some(cell.id)
            })
            .ok_or_else(|| "Unknown OIB property address".into());
    }
    let doc = metadata(seed)?;
    match address {
        PropertyAddress::Placement { .. } | PropertyAddress::Structural { .. } => {
            Err("Placement uses structural admission, not a causal property cell".into())
        }
        PropertyAddress::Definition {
            definition,
            node,
            property,
        } => doc
            .mappings
            .definitions
            .iter()
            .find(|m| m.definition == *definition && m.node == *node && m.property == *property)
            .map(|m| m.cell)
            .ok_or_else(|| "Read-only or unknown OIB definition property".into()),
        PropertyAddress::Occurrence { instance, path } => {
            let allocation = allocations
                .iter()
                .find(|a| a.id == instance.0)
                .ok_or("Unknown resident OIB root")?;
            let mapped = if seed.expanded {
                *instance
            } else {
                InstanceId(allocation.prototype)
            };
            let root = self::instance(doc, mapped)?;
            let descriptor = property(doc, root.definition, path)?;
            if !descriptor.policy.live_literal
                || descriptor.editor == PropertyEditor::ReadOnly
                || bound_source(&sources(doc, root)?, path, &mut BTreeSet::new())?.is_some()
            {
                return Err("OIB source is read-only".into());
            }
            let row = doc
                .mappings
                .roots
                .iter()
                .find(|m| m.prototype == mapped)
                .and_then(|m| m.cells.iter().find(|m| m.path == *path))
                .ok_or("Unmapped OIB occurrence property")?;
            if seed.expanded {
                Ok(row.initial_cell)
            } else {
                lower(seed, allocation, row)
            }
        }
    }
}
/// Inverse of the disposable lowering table. Only this semantic address is
/// written to the wire and Merkle state; numeric cells are compiler handles.
pub fn address_for_cell(seed: &Seed, cell: u64) -> Result<PropertyAddress> {
    if let Some(doc) = &seed.designer {
        if let Some(row) = doc.mappings.definitions.iter().find(|row| row.cell == cell) {
            return Ok(PropertyAddress::Definition {
                definition: row.definition,
                node: row.node,
                property: row.property,
            });
        }
        for table in &doc.mappings.roots {
            if let Some(row) = table.cells.iter().find(|row| row.initial_cell == cell) {
                return Ok(PropertyAddress::Occurrence {
                    instance: table.prototype,
                    path: row.path.clone(),
                });
            }
        }
        return Err("Unknown OIB property address".into());
    }
    let cell = seed
        .cells
        .iter()
        .find(|row| row.id == cell)
        .ok_or("Unknown OIB property")?;
    let property = PropertyId(cell.inherit.unwrap_or(cell.id));
    Ok(if cell.inherit.is_some() {
        PropertyAddress::Occurrence {
            instance: InstanceId(cell.owner),
            path: PropertyPath {
                references: vec![],
                node: NodeId(1),
                property,
            },
        }
    } else {
        PropertyAddress::Definition {
            definition: DefinitionId(cell.owner),
            node: NodeId(1),
            property,
        }
    })
}
pub(super) fn lower(seed: &Seed, allocation: &Allocation, row: &RootCellKey) -> Result<u64> {
    if allocation.id == allocation.prototype {
        return Ok(row.initial_cell);
    }
    let doc = metadata(seed)?;
    let table = doc
        .mappings
        .roots
        .iter()
        .find(|t| t.prototype.0 == allocation.prototype)
        .ok_or("Missing OIB preset lowering")?;
    let rank = table
        .cells
        .iter()
        .position(|r| r.path == row.path)
        .ok_or("Unknown OIB property")?;
    seed.cells
        .iter()
        .map(|c| c.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .and_then(|base| base.checked_add(u64::from(allocation.slot) * 64 + rank as u64))
        .ok_or_else(|| "OIB compiler handle capacity exhausted".into())
}
/// Resolve a signed source alias, never an inherited-property/name guess.
pub fn cell_for_alias(
    seed: &Seed,
    allocations: &[Allocation],
    owner: u64,
    alias: u64,
) -> Result<u64> {
    let doc = metadata(seed)?;
    let allocation = allocations
        .iter()
        .find(|a| a.id == owner)
        .ok_or("Unknown OIB instance")?;
    let row = doc
        .mappings
        .roots
        .iter()
        .find(|m| m.prototype.0 == allocation.prototype)
        .and_then(|m| m.cells.iter().find(|r| r.initial_cell == alias))
        .ok_or("Unknown OIB property alias")?;
    lower(seed, allocation, row)
}
fn occurrence_cell(doc: &Document, instance: InstanceId, path: &PropertyPath) -> Option<u64> {
    doc.mappings
        .roots
        .iter()
        .find(|m| m.prototype == instance)
        .and_then(|m| m.cells.iter().find(|m| m.path == *path))
        .map(|m| m.initial_cell)
}
fn resolve(
    seed: &Seed,
    root: &ComponentInstance,
    source: &Sources,
    path: &PropertyPath,
    values: &BTreeMap<u64, Option<Value>>,
    ignore: Option<u64>,
    seen: &mut BTreeSet<PropertyPath>,
) -> Result<Option<Value>> {
    if !seen.insert(path.clone()) || seen.len() > 64 {
        return Err("Cyclic OIB property forwarding".into());
    }
    let doc = metadata(seed)?;
    let descriptor = property(doc, root.definition, path)?;
    let cell = occurrence_cell(doc, root.id, path);
    let result = if let Some(value) = cell
        .filter(|c| Some(*c) != ignore)
        .and_then(|c| values.get(&c))
        .and_then(|v| v.clone())
    {
        Some(value)
    } else {
        match source.get(path) {
            Some(Source::Literal(v)) => Some(v.clone()),
            Some(Source::Forward(from)) => resolve(seed, root, source, from, values, ignore, seen)?,
            Some(Source::Bound(_)) => None,
            None => terminal_cell(doc, root.definition, path)?
                .and_then(|c| values.get(&c))
                .and_then(|v| v.clone())
                .or(Some(fallback(doc, root.definition, path)?)),
        }
    };
    if let Some(v) = &result {
        validate_property_value(&descriptor, v)?;
    }
    Ok(result)
}
pub fn effective(seed: &Seed, values: &BTreeMap<u64, Option<Value>>, id: u64) -> Result<Value> {
    let doc = metadata(seed)?;
    if let Some(m) = doc.mappings.definitions.iter().find(|m| m.cell == id) {
        let path = PropertyPath {
            references: vec![],
            node: m.node,
            property: m.property,
        };
        let value =
            values
                .get(&id)
                .and_then(|v| v.clone())
                .unwrap_or(fallback(doc, m.definition, &path)?);
        validate_property_value(&property(doc, m.definition, &path)?, &value)?;
        return Ok(value);
    }
    for table in &doc.mappings.roots {
        if let Some(row) = table.cells.iter().find(|r| r.initial_cell == id) {
            let root = instance(doc, table.prototype)?;
            return resolve(
                seed,
                root,
                &sources(doc, root)?,
                &row.path,
                values,
                None,
                &mut BTreeSet::new(),
            )?
            .ok_or_else(|| "Bound OIB source has no literal value".into());
        }
    }
    Err("Unknown OIB property address".into())
}
pub fn validate_edit(seed: &Seed, id: u64, value: Option<&Value>) -> Result<()> {
    if seed.designer.is_none() {
        return Ok(());
    }
    let doc = metadata(seed)?;
    let (descriptor, bound) =
        if let Some(m) = doc.mappings.definitions.iter().find(|m| m.cell == id) {
            (
                property(
                    doc,
                    m.definition,
                    &PropertyPath {
                        references: vec![],
                        node: m.node,
                        property: m.property,
                    },
                )?,
                false,
            )
        } else {
            let (table, row) = doc
                .mappings
                .roots
                .iter()
                .find_map(|t| {
                    t.cells
                        .iter()
                        .find(|r| r.initial_cell == id)
                        .map(|r| (t, r))
                })
                .ok_or("Unmapped OIB write")?;
            let root = instance(doc, table.prototype)?;
            (
                property(doc, root.definition, &row.path)?,
                bound_source(&sources(doc, root)?, &row.path, &mut BTreeSet::new())?.is_some(),
            )
        };
    if bound || !descriptor.policy.live_literal || descriptor.editor == PropertyEditor::ReadOnly {
        return Err("OIB property is read-only".into());
    }
    if let Some(v) = value {
        validate_property_value(&descriptor, v)?;
    }
    Ok(())
}
fn validate_property_value(p: &PropertyDescriptor, value: &Value) -> Result<()> {
    let valid = match (p.value_type, value) {
        (ValueType::Number, Value::Number(n)) => {
            n.is_finite()
                && !(*n == 0. && n.is_sign_negative())
                && (0. ..=1000.).contains(n)
                && p.number_range.is_none_or(|[lo, hi]| *n >= lo && *n <= hi)
        }
        (ValueType::Text, Value::Text(v)) => valid_text(v, 96),
        (ValueType::Boolean, Value::Boolean(_)) | (ValueType::Color, Value::Color(_)) => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("OIB descriptor value/type constraint violated".into())
    }
}
fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.contains('\0')
}

/// Expand identity/mapping metadata alongside disposable cells, never persist it.
pub(in crate::canvas) fn expanded_metadata(
    seed: &Seed,
    allocations: &[Allocation],
) -> Result<Document> {
    let original = metadata(seed)?;
    if seed.expanded {
        return Ok(original.clone());
    }
    let factories: BTreeSet<_> = original
        .mappings
        .roots
        .iter()
        .map(|r| r.prototype)
        .collect();
    let mut doc = original.clone();
    doc.instances.retain(|i| !factories.contains(&i.id));
    doc.mappings.roots.clear();
    doc.mappings
        .objects
        .retain(|o| !factories.contains(&o.address.instance));
    for (order, allocation) in allocations.iter().enumerate() {
        let prototype = InstanceId(allocation.prototype);
        let mut root = instance(original, prototype)?.clone();
        root.id = InstanceId(allocation.id);
        root.name = allocation.name.clone();
        root.order = u16::try_from(order).map_err(err)?;
        doc.instances.push(root);
        let mut table = original
            .mappings
            .roots
            .iter()
            .find(|t| t.prototype == prototype)
            .ok_or("Unmapped root factory")?
            .clone();
        table.prototype = InstanceId(allocation.id);
        for row in &mut table.cells {
            row.initial_cell = lower(seed, allocation, row)?;
        }
        doc.mappings.roots.push(table);
        for mapping in original
            .mappings
            .objects
            .iter()
            .filter(|m| m.address.instance == prototype)
        {
            let mut mapping = mapping.clone();
            mapping.address.instance = InstanceId(allocation.id);
            // Source expansion has no renderer allocation. The graph compiler
            // attaches actual nodes when materializing the current layout.
            mapping.layout_node = None;
            doc.mappings.objects.push(mapping);
        }
    }
    doc.mappings
        .objects
        .sort_by(|a, b| a.address.cmp(&b.address));
    doc.mappings.roots.sort_by_key(|m| m.prototype);
    placement::project_metadata(&mut doc, allocations)?;
    connections::project_metadata(&mut doc, &super::graph::source_selectors(seed)?)?;
    Ok(doc)
}
fn walk_objects(
    doc: &Document,
    root: InstanceId,
    def: DefinitionId,
    references: &[NodeId],
    id: NodeId,
    depth: u16,
    out: &mut Vec<ObjectInfo>,
) -> Result<()> {
    if depth > 12 || references.len() > 8 || out.len() >= 512 {
        return Err("OIB expanded object/depth budget".into());
    }
    let d = definition(doc, def)?;
    let n = d
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or("Missing OIB object")?;
    let address = ObjectAddress {
        instance: root,
        references: references.to_vec(),
        node: id,
    };
    let layout_node = doc
        .mappings
        .objects
        .iter()
        .find(|m| m.address == address)
        .ok_or("Missing OIB object mapping")?
        .layout_node;
    out.push(ObjectInfo {
        selection: Selection::Object(address),
        name: n.name.clone(),
        class: n.class,
        depth,
        layout_node,
    });
    if let Some(reference) = &n.reference {
        let mut nested = references.to_vec();
        nested.push(n.id);
        walk_objects(
            doc,
            root,
            reference.definition,
            &nested,
            definition(doc, reference.definition)?.root,
            depth + 1,
            out,
        )?;
    }
    // Paint order is semantic; append nonvisual/ownership-only children by ID.
    let mut children = n.visual_children.clone();
    children.extend(
        d.nodes
            .iter()
            .filter(|c| c.owner == Some(n.id) && !n.visual_children.contains(&c.id))
            .map(|c| c.id),
    );
    for child in children {
        walk_objects(doc, root, def, references, child, depth + 1, out)?;
    }
    Ok(())
}
fn object_rows(doc: &Document) -> Result<Vec<ObjectInfo>> {
    let mut out = vec![ObjectInfo {
        selection: Selection::Document(doc.id),
        name: doc.name.clone(),
        class: doc.class,
        depth: 0,
        layout_node: None,
    }];
    let mut pending: Vec<_> = doc.instances.iter().collect();
    pending.sort_by_key(|i| (i.id != doc.presented_root, i.order, i.id));
    let mut emitted = BTreeSet::new();
    while !pending.is_empty() {
        let index = pending
            .iter()
            .position(|i| match i.owner {
                Owner::Document => true,
                Owner::Instance(id) => emitted.contains(&id),
            })
            .ok_or("Cyclic document ownership")?;
        let root = pending.remove(index);
        let depth = match root.owner {
            Owner::Document => 1,
            Owner::Instance(owner) => {
                let parent = instance(doc, owner)?;
                let parent_root = definition(doc, parent.definition)?.root;
                out.iter().find(|row| matches!(&row.selection,
                    Selection::Object(a) if a.instance == owner && a.references.is_empty() && a.node == parent_root))
                    .map(|row| row.depth + 1).ok_or("Missing object owner")?
            }
        };
        walk_objects(
            doc,
            root.id,
            root.definition,
            &[],
            definition(doc, root.definition)?.root,
            depth,
            &mut out,
        )?;
        emitted.insert(root.id);
    }
    for def in &doc.definitions {
        out.push(ObjectInfo {
            selection: Selection::Definition(def.id),
            name: def.name.clone(),
            class: def
                .nodes
                .iter()
                .find(|n| n.id == def.root)
                .ok_or("Missing definition root")?
                .class,
            depth: 1,
            layout_node: None,
        });
        for n in &def.nodes {
            let mut depth = 2;
            let mut owner = n.owner;
            let mut seen = BTreeSet::new();
            while let Some(id) = owner {
                if !seen.insert(id) || depth > 12 {
                    return Err("Cyclic or deep definition ownership".into());
                }
                depth += 1;
                owner = def
                    .nodes
                    .iter()
                    .find(|n| n.id == id)
                    .ok_or("Missing definition owner")?
                    .owner;
            }
            out.push(ObjectInfo {
                selection: Selection::DefinitionNode {
                    definition: def.id,
                    node: n.id,
                },
                name: n.name.clone(),
                class: n.class,
                depth,
                layout_node: None,
            });
        }
    }
    if out.len() > 512 {
        return Err("OIB hierarchy budget".into());
    }
    Ok(out)
}
pub fn objects(seed: &Seed, allocations: &[Allocation]) -> Result<Vec<ObjectInfo>> {
    object_rows(&expanded_metadata(seed, allocations)?)
}
pub fn properties(
    seed: &Seed,
    allocations: &[Allocation],
    selection: &Selection,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<Vec<PropertyInfo>> {
    let expanded = super::instances::expanded_seed(seed, allocations)?;
    let doc = metadata(&expanded)?;
    let (class_id, scope) = match selection {
        Selection::Document(id) if *id == doc.id => (doc.class, None),
        Selection::Definition(id) => {
            let d = definition(doc, *id)?;
            (
                path_node(doc, d.id, &[], d.root)?.1.class,
                Some((d.id, d.root, None)),
            )
        }
        Selection::DefinitionNode {
            definition: id,
            node,
        } => (
            path_node(doc, *id, &[], *node)?.1.class,
            Some((*id, *node, None)),
        ),
        Selection::Object(address) => {
            let root = instance(doc, address.instance)?;
            let (_, n) = path_node(doc, root.definition, &address.references, address.node)?;
            (
                n.class,
                Some((root.definition, address.node, Some(address))),
            )
        }
        _ => return Err("Unknown OIB selection".into()),
    };
    let mut result = Vec::new();
    let mut presented = Vec::new();
    let mut ancestry = Vec::new();
    let mut next = Some(class_id);
    while let Some(id) = next {
        let c = class(doc, id)?;
        ancestry.push(c);
        next = c.base;
    }
    for c in ancestry.into_iter().rev() {
        let mut fields = c.properties.clone();
        fields.sort_by_key(|p| (p.group as u8, p.id));
        presented.extend(fields);
    }
    let exposed = scope.and_then(|(def, node, occurrence)| {
        let def = if let Some(a) = occurrence {
            path_node(doc, def, &a.references, node).ok()?.0
        } else {
            definition(doc, def).ok()?
        };
        (node == def.root).then_some(&def.exposed)
    });
    if let Some(exposed) = exposed {
        let (mut first, rest): (Vec<_>, Vec<_>) = presented
            .into_iter()
            .partition(|p| exposed.iter().any(|e| e.property == p.id));
        first.extend(rest);
        presented = first;
    }
    for descriptor in presented {
        let Some((def, node, occurrence)) = scope else {
            result.push(PropertyInfo {
                address: None,
                cell: None,
                value: Some(descriptor.default.clone()),
                inherited: Some(descriptor.default.clone()),
                descriptor,
                origin: "class".into(),
                source: "literal".into(),
                overridden: false,
            });
            continue;
        };
        let path = PropertyPath {
            references: occurrence.map_or_else(Vec::new, |a| a.references.clone()),
            node,
            property: descriptor.id,
        };
        let address = if let Some(a) = occurrence {
            PropertyAddress::Occurrence {
                instance: a.instance,
                path: path.clone(),
            }
        } else {
            PropertyAddress::Definition {
                definition: def,
                node,
                property: descriptor.id,
            }
        };
        let (cell, value, inherited, origin, source, overridden) = if let Some(a) = occurrence {
            let root = instance(doc, a.instance)?;
            let sources = sources(doc, root)?;
            let bound = bound_source(&sources, &path, &mut BTreeSet::new())?.is_some();
            let cell = occurrence_cell(doc, a.instance, &path).filter(|_| {
                !bound
                    && descriptor.policy.live_literal
                    && descriptor.editor != PropertyEditor::ReadOnly
            });
            let overridden = cell
                .and_then(|c| values.get(&c))
                .is_some_and(Option::is_some);
            let value = resolve(
                &expanded,
                root,
                &sources,
                &path,
                values,
                None,
                &mut BTreeSet::new(),
            )?;
            let inherited = resolve(
                &expanded,
                root,
                &sources,
                &path,
                values,
                cell,
                &mut BTreeSet::new(),
            )?;
            let source = if bound {
                "bound"
            } else if matches!(sources.get(&path), Some(Source::Forward(_))) {
                "forwarded"
            } else {
                "literal"
            };
            let origin = if overridden {
                "live"
            } else if sources.contains_key(&path) {
                "authored"
            } else if terminal_cell(doc, root.definition, &path)?.is_some() {
                "component"
            } else {
                "class"
            };
            (cell, value, inherited, origin, source, overridden)
        } else {
            let cell = terminal_cell(doc, def, &path)?;
            let inherited = fallback(doc, def, &path)?;
            let value = cell
                .and_then(|c| values.get(&c))
                .and_then(|v| v.clone())
                .unwrap_or_else(|| inherited.clone());
            let overridden = value != inherited;
            (
                cell,
                Some(value),
                Some(inherited),
                if overridden { "live" } else { "component" },
                "literal",
                overridden,
            )
        };
        result.push(PropertyInfo {
            address: Some(address),
            cell,
            descriptor,
            value,
            inherited,
            origin: origin.into(),
            source: source.into(),
            overridden,
        });
    }
    Ok(result)
}

pub(super) fn output_key(target: &Target) -> (u16, u8) {
    (
        target.node,
        if target.property == Property::FrameWidth {
            Property::Width as u8
        } else {
            target.property as u8
        },
    )
}
pub(super) fn output_cells(seed: &Seed) -> Result<BTreeMap<(u16, u8), u64>> {
    let mut result = BTreeMap::new();
    for cell in &seed.cells {
        for target in &cell.targets {
            let key = output_key(target);
            if let Some(old) = result.get(&key).copied() {
                let previous = seed
                    .cells
                    .iter()
                    .find(|c| c.id == old)
                    .ok_or("Missing terminal output cell")?;
                if seed.designer.is_none() {
                    return Err("Duplicated OIB target".into());
                }
                if cell.inherit == Some(old) {
                    result.insert(key, cell.id);
                } else if previous.inherit != Some(cell.id) {
                    return Err("Conflicting OIB terminal outputs".into());
                }
            } else {
                result.insert(key, cell.id);
            }
        }
    }
    Ok(result)
}

/// Canonical decoder for a standalone signed Seed envelope. Embedded UI decoders
/// must equivalently compare their complete reencoding to the signed input bytes.
pub fn decode_seed(bytes: &[u8]) -> Result<Seed> {
    if bytes.is_empty() || bytes.len() > 24 * 1024 {
        return Err("OIB seed byte budget".into());
    }
    crate::flex_guard::validate(bytes, 32, 48 * 1024, 24 * 1024)?;
    let seed: Seed = flexbuffers::from_slice(bytes).map_err(err)?;
    if flexbuffers::to_vec(&seed).map_err(err)? != bytes {
        return Err("Noncanonical OIB seed bytes".into());
    }
    let seed = super::graph::materialize_seed(&seed)?;
    super::validate_seed(&seed)?;
    Ok(seed)
}
