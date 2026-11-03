// SPDX-License-Identifier: LGPL-3.0-only
//! Disposable navigation over admitted metadata; never a second document store.
use super::*;

const MAX_ROWS: usize = 512;
const MAX_DEPTH: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HierarchyScope {
    Composition,
    Components,
}

/// Names, renderer slots and row positions are never navigation identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HierarchyKey {
    Selection(Selection),
    Objects(DocumentId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HierarchyRow {
    pub key: HierarchyKey,
    pub parent: Option<HierarchyKey>,
    pub selection: Option<Selection>,
    pub name: String,
    pub class: ClassId,
    /// Present at a component boundary and on definition headers/references.
    pub component: Option<DefinitionId>,
    pub layout_node: Option<u16>,
    /// Advanced inspection address of the outer fused wrapper, if any.
    pub reference_wrapper: Option<ObjectAddress>,
    /// Actual signed ownership, independent of navigation (may name a wrapper).
    pub owner: Option<Selection>,
}

fn key(selection: Selection) -> HierarchyKey {
    HierarchyKey::Selection(selection)
}
fn row(
    selection: Selection,
    parent: Option<HierarchyKey>,
    name: String,
    class: ClassId,
) -> HierarchyRow {
    HierarchyRow {
        key: key(selection.clone()),
        parent,
        selection: Some(selection),
        name,
        class,
        component: None,
        layout_node: None,
        reference_wrapper: None,
        owner: None,
    }
}
fn object(instance: InstanceId, references: &[NodeId], node: NodeId) -> ObjectAddress {
    ObjectAddress {
        instance,
        references: references.to_vec(),
        node,
    }
}
fn resolve_node<'a>(
    doc: &'a Document,
    a: &ObjectAddress,
) -> Result<(&'a ComponentDefinition, &'a DefinitionNode)> {
    path_node(
        doc,
        instance(doc, a.instance)?.definition,
        &a.references,
        a.node,
    )
}
fn layout(doc: &Document, a: &ObjectAddress) -> Result<Option<u16>> {
    doc.mappings
        .objects
        .iter()
        .find(|m| &m.address == a)
        .map(|m| m.layout_node)
        .ok_or_else(|| "Missing OIB object mapping".into())
}
fn fused(doc: &Document, mut a: ObjectAddress) -> Result<ObjectAddress> {
    while let Some(reference) = &resolve_node(doc, &a)?.1.reference {
        if a.references.len() >= 8 {
            return Err("OIB hierarchy reference budget".into());
        }
        a.references.push(a.node);
        a.node = definition(doc, reference.definition)?.root;
    }
    Ok(a)
}
fn instance_owner(doc: &Document, root: &ComponentInstance) -> Result<Selection> {
    Ok(match root.owner {
        Owner::Document => Selection::Document(doc.id),
        Owner::Instance(id) => Selection::Object(object(
            id,
            &[],
            definition(doc, instance(doc, id)?.definition)?.root,
        )),
    })
}

// Rank is a sibling order only: signed visual children precede explicitly ordered
// root attachments. Nonvisual children use canonical signed node/address order.
struct Projection {
    rows: Vec<(HierarchyRow, (u8, u64))>,
}
impl Projection {
    fn push(&mut self, row: HierarchyRow, rank: (u8, u64)) -> Result<()> {
        if self.rows.len() >= MAX_ROWS || self.rows.iter().any(|(r, _)| r.key == row.key) {
            return Err("OIB hierarchy budget or duplicate identity".into());
        }
        self.rows.push((row, rank));
        Ok(())
    }
    fn finish(mut self) -> Result<Vec<HierarchyRow>> {
        self.rows.sort_by_key(|(_, rank)| *rank);
        fn visit(
            rows: &[(HierarchyRow, (u8, u64))],
            parent: Option<&HierarchyKey>,
            depth: usize,
            out: &mut Vec<HierarchyRow>,
        ) -> Result<()> {
            if depth > MAX_DEPTH {
                return Err("OIB hierarchy depth budget".into());
            }
            for (r, _) in rows.iter().filter(|(r, _)| r.parent.as_ref() == parent) {
                if out.iter().any(|p| p.key == r.key) {
                    return Err("Cyclic OIB hierarchy".into());
                }
                out.push(r.clone());
                visit(rows, Some(&r.key), depth + 1, out)?;
            }
            Ok(())
        }
        let mut out = Vec::new();
        visit(&self.rows, None, 0, &mut out)?;
        if out.len() != self.rows.len() {
            return Err("Missing or cyclic OIB hierarchy parent".into());
        }
        Ok(out)
    }
}

/// Query an admitted seed. `allocations` is the resident closure; `live_ids`
/// contains active allocation IDs only. Retired residents are hidden; fixed
/// signed instances remain. Returns parent-before-child preorder, at most 512
/// rows and 12 edges deep, without changing cells, mappings or application state.
/// Components is a separate authored ownership tree without reference expansion.
/// Disclosure and depth derived from `parent` are the caller's responsibility.
/// Detached visual nodes and missing/cyclic parents reject the whole query;
/// ownership is never substituted for a missing signed visual edge.
pub fn hierarchy(
    seed: &Seed,
    allocations: &[Allocation],
    live_ids: &BTreeSet<u64>,
    scope: HierarchyScope,
) -> Result<Vec<HierarchyRow>> {
    if allocations.len() > MAX_ALLOCATIONS {
        return Err("OIB hierarchy allocation budget".into());
    }
    if allocations
        .iter()
        .filter(|a| live_ids.contains(&a.id))
        .count()
        > MAX_LIVE_INSTANCES
    {
        return Err("OIB hierarchy live instance budget".into());
    }
    let original = metadata(seed)?;
    let cells = allocations
        .iter()
        .try_fold(original.mappings.definitions.len(), |count, a| {
            let root = original
                .mappings
                .roots
                .iter()
                .find(|r| r.prototype.0 == a.prototype)
                .ok_or("Unknown OIB hierarchy factory")?;
            count
                .checked_add(root.cells.len())
                .ok_or("OIB hierarchy cell count overflow")
        })?;
    if cells > MAX_CELLS {
        return Err("OIB resident property capacity exhausted".into());
    }
    let doc = expanded_metadata(seed, allocations)?;
    let mut p = Projection { rows: Vec::new() };
    if scope == HierarchyScope::Components {
        for (index, d) in doc.definitions.iter().enumerate() {
            let selection = Selection::Definition(d.id);
            let mut header = row(
                selection.clone(),
                None,
                d.name.clone(),
                path_node(&doc, d.id, &[], d.root)?.1.class,
            );
            header.component = Some(d.id);
            p.push(header, (0, index as u64))?;
            for n in &d.nodes {
                let owner = n.owner.map(|node| Selection::DefinitionNode {
                    definition: d.id,
                    node,
                });
                let mut r = row(
                    Selection::DefinitionNode {
                        definition: d.id,
                        node: n.id,
                    },
                    Some(key(owner.clone().unwrap_or_else(|| selection.clone()))),
                    n.name.clone(),
                    n.class,
                );
                r.component = n.reference.as_ref().map(|r| r.definition);
                r.owner = owner;
                p.push(r, (0, n.id.0))?;
            }
        }
        return p.finish();
    }
    let document = Selection::Document(doc.id);
    p.push(
        row(document.clone(), None, doc.name.clone(), doc.class),
        (0, 0),
    )?;
    let group = HierarchyKey::Objects(doc.id);
    p.push(
        HierarchyRow {
            key: group.clone(),
            parent: Some(key(document.clone())),
            selection: None,
            name: "Objects".into(),
            class: doc.class,
            component: None,
            layout_node: None,
            reference_wrapper: None,
            owner: Some(document.clone()),
        },
        (2, 0),
    )?;
    let resident: BTreeSet<_> = allocations.iter().map(|a| a.id).collect();
    for mapping in &doc.mappings.objects {
        let a = &mapping.address;
        if resident.contains(&a.instance.0) && !live_ids.contains(&a.instance.0) {
            continue;
        }
        let root = instance(&doc, a.instance)?;
        let (def, n) = resolve_node(&doc, a)?;
        // The referenced root represents its wrapper, which remains available
        // through reference_wrapper and the unchanged signed metadata.
        if n.reference.is_some() {
            continue;
        }
        let owner = if let Some(id) = n.owner {
            Selection::Object(object(a.instance, &a.references, id))
        } else if let Some((&node, refs)) = a.references.split_last() {
            Selection::Object(object(a.instance, refs, node))
        } else {
            instance_owner(&doc, root)?
        };
        let mut r = row(Selection::Object(a.clone()), None, n.name.clone(), n.class);
        r.owner = Some(owner.clone());
        r.layout_node = mapping.layout_node;
        // Walk outward over fused roots to find the real visible boundary and
        // its authored occurrence name, retaining the full final selection.
        let mut boundary = a.clone();
        while boundary.node == resolve_node(&doc, &boundary)?.0.root {
            r.component = Some(def.id);
            let Some(node) = boundary.references.pop() else {
                break;
            };
            boundary.node = node;
            r.reference_wrapper = Some(boundary.clone());
        }
        if let Some(wrapper) = &r.reference_wrapper {
            r.name = resolve_node(&doc, wrapper)?.1.name.clone();
        }
        if boundary.references.is_empty()
            && boundary.node == definition(&doc, root.definition)?.root
        {
            r.name = root.name.clone();
        }
        let rank;
        if mapping.layout_node.is_none() {
            // The real root is owned by its wrapper, but navigation follows
            // the outer wrapper's ownership edge, avoiding a self-parent.
            let boundary_node = resolve_node(&doc, &boundary)?.1;
            let navigation_owner = if let Some(node) = boundary_node.owner {
                Selection::Object(object(a.instance, &boundary.references, node))
            } else {
                instance_owner(&doc, root)?
            };
            let nonvisual_owner = match &navigation_owner {
                Selection::Object(owner) => {
                    let owner = fused(&doc, owner.clone())?;
                    (owner != *a && layout(&doc, &owner)?.is_none()).then_some(owner)
                }
                _ => None,
            };
            r.parent =
                Some(nonvisual_owner.map_or_else(|| group.clone(), |a| key(Selection::Object(a))));
            rank = if boundary.references.is_empty()
                && boundary.node == definition(&doc, root.definition)?.root
            {
                (1, u64::from(root.order))
            } else {
                (0, n.id.0)
            };
        } else {
            let (containing, _) = resolve_node(&doc, &boundary)?;
            if let Some((parent, index)) = containing.nodes.iter().find_map(|p| {
                p.visual_children
                    .iter()
                    .position(|id| *id == boundary.node)
                    .map(|index| (p.id, index))
            }) {
                r.parent = Some(key(Selection::Object(fused(
                    &doc,
                    object(a.instance, &boundary.references, parent),
                )?)));
                rank = (0, index as u64);
            } else if boundary.references.is_empty() && boundary.node == containing.root {
                r.parent = Some(match &root.visual_parent {
                    Some(parent) => key(Selection::Object(fused(&doc, parent.clone())?)),
                    None => key(document.clone()),
                });
                rank = (1, u64::from(root.order));
            } else {
                return Err("Detached visual OIB hierarchy node".into());
            }
        }
        p.push(r, rank)?;
    }
    p.finish()
}
