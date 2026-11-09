// SPDX-License-Identifier: LGPL-3.0-only
//! Palette presets, durable resident identities and disposable property tables.
use super::*;
use otrios_interfaces::layout::Responder;

pub fn initial_allocations(seed: &Seed) -> Vec<Allocation> {
    if let Some(graph) = &seed.graph {
        let mut roots: Vec<_> = graph
            .instances
            .iter()
            .filter_map(|i| {
                let preset = graph
                    .presets
                    .iter()
                    .find(|p| p.id == i.preset && p.palette)?;
                Some((
                    i.order,
                    Allocation {
                        id: i.id.0,
                        name: i.name.clone(),
                        component: preset.definition.0,
                        prototype: preset.id,
                        slot: 0,
                    },
                ))
            })
            .collect();
        roots.sort_by_key(|(order, a)| (*order, a.id));
        let mut ids: Vec<_> = roots.iter().map(|(_, a)| a.id).collect();
        ids.sort_unstable();
        return roots
            .into_iter()
            .map(|(_, mut a)| {
                a.slot = ids.iter().position(|id| *id == a.id).unwrap_or(0) as u16;
                a
            })
            .collect();
    }
    vec![]
}

pub fn cell_id(seed: &Seed, allocations: &[Allocation], owner: u64, property: u64) -> Result<u64> {
    if seed.designer.is_some() {
        let aliases: Vec<_> = seed
            .cells
            .iter()
            .filter(|c| {
                c.owner
                    == allocations
                        .iter()
                        .find(|a| a.id == owner)
                        .map_or(0, |a| a.prototype)
                    && c.inherit == Some(property)
            })
            .collect();
        if aliases.len() != 1 {
            return Err("Ambiguous inherited property; use a full semantic address".into());
        }
        return document::cell_for_alias(seed, allocations, owner, aliases[0].id);
    }
    Err("Canonical graph property metadata is required".into())
}

pub fn expanded_seed(seed: &Seed, allocations: &[Allocation]) -> Result<Seed> {
    if seed.expanded {
        return Ok(seed.clone());
    }
    if seed.graph.is_none() || seed.designer.is_none() {
        return Err("Canonical graph property metadata is required".into());
    }
    let mut expanded = expanded_designer_seed(seed, allocations)?;
    expanded.expanded = true;
    Ok(expanded)
}
fn find(root: &TemplateNode, id: u16) -> Option<&TemplateNode> {
    if root.id == id {
        Some(root)
    } else {
        root.children.iter().find_map(|c| find(c, id))
    }
}
fn controls(node: &TemplateNode, out: &mut BTreeSet<u8>) {
    if let Some(Responder::AppEvent { control, .. }) = node.responder {
        out.insert(control);
    }
    for c in &node.children {
        controls(c, out);
    }
}
/// Resolve the current canonical graph into disposable layout and input resources.
pub fn expand(
    ui: &Ui,
    allocations: &[Allocation],
    live: &BTreeSet<u64>,
    selected: Option<u64>,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<Ui> {
    super::graph::expand(ui, allocations, live, selected, values)
}
pub fn validate_surface(ui: &Ui) -> Result<()> {
    let seed = seed(ui).ok_or("Missing OIB seed")?;
    let surface = seed
        .instance_surface
        .as_ref()
        .ok_or("Missing instance surface")?;
    let blueprint = Layout::from_bytes(&ui.layout)?.blueprint().clone();
    let preview = blueprint
        .templates
        .iter()
        .find_map(|t| find(&t.root, surface.preview_viewport))
        .ok_or("Missing preview")?;
    let inspector = blueprint
        .templates
        .iter()
        .find_map(|t| find(&t.root, seed.inspector_viewport))
        .ok_or("Missing inspector")?;
    fn maximum(root: &TemplateNode) -> u16 {
        root.children.iter().map(maximum).fold(root.id, u16::max)
    }
    if blueprint
        .templates
        .iter()
        .any(|t| maximum(&t.root) >= surface.node_base)
        || (surface.node_stride == 0 && !surface.prototypes.is_empty())
    {
        return Err("OIB node allocation overlaps the signed seed".into());
    }
    let mut seen = BTreeSet::new();
    fn visit(n: &TemplateNode, seen: &mut BTreeSet<u16>) -> Result<()> {
        if n.id >= 512
            || !seen.insert(n.id)
            || !matches!(
                n.shape,
                Shape::Canvas { .. }
                    | Shape::Text { .. }
                    | Shape::Superellipse { .. }
                    | Shape::Circle { .. }
            )
        {
            return Err("Unsupported or overlapping OIB factory nodes".into());
        }
        for c in &n.children {
            visit(c, seen)?;
        }
        Ok(())
    }
    for prototype in &surface.prototypes {
        let a = preview
            .children
            .iter()
            .find(|c| c.id == prototype.preview_root)
            .ok_or("Factory preview must be a direct viewport child")?;
        let b = prototype
            .inspector_root
            .map(|id| {
                inspector
                    .children
                    .iter()
                    .find(|c| c.id == id)
                    .ok_or("Factory inspector must be a direct viewport child")
            })
            .transpose()?;
        let before = seen.clone();
        visit(a, &mut seen)?;
        if let Some(b) = b {
            visit(b, &mut seen)?;
        }
        let actual: Vec<_> = seen.difference(&before).copied().collect();
        if actual != prototype.nodes || actual.len() > usize::from(surface.node_stride) {
            return Err("Invalid signed OIB node allocation table".into());
        }
        let mut set = BTreeSet::new();
        controls(a, &mut set);
        if let Some(b) = b {
            controls(b, &mut set);
        }
        for h in ui.handlers.iter().filter(|h| set.contains(&h.control)) {
            match h.operation {
                Operation::Set if h.fields.len() == 1 => {}
                Operation::Toggle
                    if seed.designer.is_some()
                        && h.kind == GestureKind::Activate
                        && h.fields.len() == 1
                        && ui.state_schema.fields.iter().any(|f| {
                            f.id == h.fields[0]
                                && f.writable
                                && matches!(f.default, Value::Number(n) if n == 0. || n == 1000.)
                        }) => {}
                Operation::Oib { ref address, .. }
                    if matches!(address, otrios_interfaces::oib::designer::PropertyAddress::Occurrence { instance, .. } if instance.0 == prototype.instance)
                        && seed.cells.iter().any(|cell| {
                            document::address_for_cell(seed, cell.id).ok().as_ref() == Some(address)
                        }) => {}
                Operation::OibInstance {
                    target,
                    action:
                        InstanceAction::Inspect | InstanceAction::Duplicate | InstanceAction::Remove,
                } if target == prototype.instance => {}
                _ => return Err("Factory includes an undeclared action".into()),
            }
        }
        for cell in seed.cells.iter().filter(|c| c.owner == prototype.instance) {
            if cell.targets.iter().any(|t| find(a, t.node).is_none()) {
                return Err("Instance property escapes its signed preview template".into());
            }
        }
    }
    if let Some(layout) = &surface.layout {
        let pitch = seed
            .cells
            .iter()
            .find(|c| c.id == layout.preview_pitch_cell)
            .ok_or("Missing row pitch")?;
        for prototype in &surface.prototypes {
            if !seed
                .instances
                .iter()
                .any(|i| i.id == prototype.instance && i.component == Some(pitch.owner))
                || !pitch.targets.iter().any(|t| {
                    t.node == prototype.preview_root
                        && t.property == Property::Height
                        && t.scale == 1.
                        && t.offset == 0.
                })
            {
                return Err("Row pitch must size every component preview row".into());
            }
        }
        for cell in seed
            .cells
            .iter()
            .filter(|c| seed.components.iter().any(|o| o.id == c.owner))
        {
            for target in &cell.targets {
                if !surface.prototypes.iter().any(|p| {
                    preview
                        .children
                        .iter()
                        .find(|c| c.id == p.preview_root)
                        .is_some_and(|r| find(r, target.node).is_some())
                        && seed
                            .instances
                            .iter()
                            .any(|i| i.id == p.instance && i.component == Some(cell.owner))
                }) {
                    return Err("Component layout target escapes its preview factories".into());
                }
            }
        }
    }
    Ok(())
}

/// Painted content and touch targets must fit within their admitted row, even
/// when nonpainting component containers extend past it.
pub fn validate_row_geometry(scene: &crate::dashboard::scene::Scene, viewport: u16) -> Result<()> {
    use otrios_interfaces::scene::{Primitive, SceneNode, TouchResponder};
    fn visit(node: &SceneNode, parent: [f32; 2], size: [f32; 2]) -> Result<()> {
        let at = [parent[0] + node.frame.x, parent[1] + node.frame.y];
        if !matches!(
            node.primitive,
            Primitive::Canvas { .. } | Primitive::Viewport { .. }
        ) && (at.iter().any(|v| *v < -0.01)
            || at[0] + node.frame.width > size[0] + 0.01
            || at[1] + node.frame.height > size[1] + 0.01)
        {
            return Err("OIB painted content escapes its row; adjust geometry or row pitch".into());
        }
        if matches!(node.responder, Some(TouchResponder::Drag(_)))
            && (node.frame.width < 48. || node.frame.height < 32.)
        {
            return Err("OIB slider touch target is too small".into());
        }
        for child in &node.children {
            visit(child, at, size)?;
        }
        Ok(())
    }
    let path = scene
        .path(u32::from(viewport))
        .ok_or("Missing row viewport")?;
    for row in &path.last().ok_or("Missing row viewport")?.node.children {
        let size = [row.frame.width, row.frame.height];
        for child in &row.children {
            visit(child, [0., 0.], size)?;
        }
    }
    Ok(())
}

fn expanded_designer_seed(seed: &Seed, allocations: &[Allocation]) -> Result<Seed> {
    let doc = document::metadata(seed)?;
    let count = doc
        .mappings
        .definitions
        .len()
        .checked_add(allocations.iter().try_fold(0usize, |total, a| {
            let count = doc
                .mappings
                .roots
                .iter()
                .find(|r| r.prototype.0 == a.prototype)
                .ok_or("Unknown palette preset")?
                .cells
                .len();
            total
                .checked_add(count)
                .ok_or("OIB resident cell count overflow")
        })?)
        .ok_or("OIB resident cell count overflow")?;
    if count > MAX_CELLS {
        return Err("OIB resident property capacity exhausted".into());
    }
    let mut resolved = seed.clone();
    resolved.designer = Some(document::expanded_metadata(seed, allocations)?);
    resolved.instances = allocations
        .iter()
        .map(|a| Object {
            id: a.id,
            name: a.name.clone(),
            component: Some(a.component),
        })
        .collect();
    resolved.cells.retain(|c| c.inherit.is_none());
    // These rows are property storage projections. Renderer targets are emitted
    // only by graph::expand after the actual resident paint order is known.
    for cell in &mut resolved.cells {
        cell.targets.clear();
    }
    for a in allocations {
        let table = doc
            .mappings
            .roots
            .iter()
            .find(|r| r.prototype.0 == a.prototype)
            .ok_or("Missing preset property paths")?;
        for row in &table.cells {
            let mut cell = seed
                .cells
                .iter()
                .find(|c| c.id == row.initial_cell)
                .ok_or("Missing source alias")?
                .clone();
            cell.id = document::lower(seed, a, row)?;
            cell.owner = a.id;
            cell.targets.clear();
            resolved.cells.push(cell);
        }
    }
    Ok(resolved)
}
