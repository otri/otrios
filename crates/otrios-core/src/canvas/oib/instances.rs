// SPDX-License-Identifier: LGPL-3.0-only
//! Signed factories with durable object identities and reusable renderer slots.
use super::*;
use otrios_interfaces::layout::{Binding as LocalBinding, Responder};

pub fn initial_allocations(seed: &Seed) -> Vec<Allocation> {
    seed.instances
        .iter()
        .enumerate()
        .map(|(slot, i)| Allocation {
            id: i.id,
            name: i.name.clone(),
            component: i.component.expect("validated instance"),
            prototype: i.id,
            slot: slot as u16,
        })
        .collect()
}
pub fn cell_id(seed: &Seed, allocations: &[Allocation], owner: u64, property: u64) -> Result<u64> {
    let index = allocations
        .iter()
        .position(|a| a.id == owner)
        .ok_or("Unknown OIB instance")?;
    let source = seed
        .cells
        .iter()
        .find(|c| c.owner == allocations[index].prototype && c.inherit == Some(property))
        .ok_or("Undeclared instance property")?;
    if owner == allocations[index].prototype {
        return Ok(source.id);
    }
    let slot = seed
        .cells
        .iter()
        .filter(|c| c.owner == allocations[index].prototype)
        .position(|c| c.id == source.id)
        .ok_or("Missing instance property")?;
    owner
        .checked_sub(0x1_0000)
        .and_then(|id| id.checked_mul(64))
        .and_then(|id| id.checked_add(0x100_0000 + slot as u64))
        .ok_or_else(|| "OIB property identity exhausted".into())
}
fn mapped_node(seed: &Seed, index: usize, prototype: u64, id: u16) -> Result<u16> {
    let surface = seed
        .instance_surface
        .as_ref()
        .ok_or("Missing OIB instance surface")?;
    let source = surface
        .prototypes
        .iter()
        .find(|p| p.instance == prototype)
        .ok_or("Unknown OIB factory")?;
    let rank = source
        .nodes
        .iter()
        .position(|n| *n == id)
        .ok_or("Node escapes instance prototype")?;
    let mapped = usize::from(surface.node_base) + index * usize::from(surface.node_stride) + rank;
    if mapped >= 1000 {
        return Err("OIB renderer slot capacity exhausted".into());
    }
    u16::try_from(mapped).map_err(err)
}
pub fn expanded_seed(seed: &Seed, allocations: &[Allocation]) -> Result<Seed> {
    if seed.instance_surface.is_none() {
        return Ok(seed.clone());
    }
    let mut resolved = seed.clone();
    resolved.instances.clear();
    resolved.cells.retain(|c| c.inherit.is_none());
    for cell in &mut resolved.cells {
        cell.targets.clear();
    }
    for allocation in allocations {
        let index = usize::from(allocation.slot);
        let map = |id| {
            if allocation.id == allocation.prototype {
                Ok(id)
            } else {
                mapped_node(seed, index, allocation.prototype, id)
            }
        };
        let prototype = seed
            .instance_surface
            .as_ref()
            .ok_or("Missing instance surface")?
            .prototypes
            .iter()
            .find(|p| p.instance == allocation.prototype)
            .ok_or("Missing prototype")?;
        for source in seed
            .cells
            .iter()
            .filter(|c| c.owner == allocation.component)
        {
            let targets = source
                .targets
                .iter()
                .filter(|t| prototype.nodes.contains(&t.node))
                .map(|t| {
                    let mut target = t.clone();
                    target.node = map(target.node)?;
                    Ok(target)
                })
                .collect::<Result<Vec<_>>>()?;
            resolved
                .cells
                .iter_mut()
                .find(|c| c.id == source.id)
                .ok_or("Missing component property")?
                .targets
                .extend(targets);
        }
        resolved.instances.push(Object {
            id: allocation.id,
            name: allocation.name.clone(),
            component: Some(allocation.component),
        });
        for source in seed
            .cells
            .iter()
            .filter(|c| c.owner == allocation.prototype)
        {
            let mut cell = source.clone();
            cell.owner = allocation.id;
            cell.id = cell_id(
                seed,
                allocations,
                allocation.id,
                cell.inherit.ok_or("Instance needs inheritance")?,
            )?;
            for t in &mut cell.targets {
                t.node = map(t.node)?;
            }
            resolved.cells.push(cell);
        }
    }
    if resolved.cells.len() > MAX_CELLS {
        return Err("OIB resident property capacity exhausted".into());
    }
    Ok(resolved)
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
fn map_expr(expr: &mut Expr, locals: &BTreeMap<u16, u16>) {
    match expr {
        Expr::State(LocalBinding::Local(id)) => {
            if let Some(new) = locals.get(id) {
                *id = *new;
            }
        }
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            map_expr(a, locals);
            map_expr(b, locals);
        }
        Expr::Select { condition, yes, no } => {
            map_expr(condition, locals);
            map_expr(yes, locals);
            map_expr(no, locals);
        }
        _ => {}
    }
}
fn remap(
    root: &mut TemplateNode,
    seed: &Seed,
    prototype: u64,
    index: usize,
    control_ids: &BTreeMap<u8, u8>,
    locals: &BTreeMap<u16, u16>,
    names: (&str, &str),
) -> Result<()> {
    let (old_name, new_name) = names;
    root.id = mapped_node(seed, index, prototype, root.id)?;
    for expr in &mut root.frame {
        map_expr(expr, locals);
    }
    map_expr(&mut root.visible, locals);
    if let Some(Responder::AppEvent { control, .. }) = &mut root.responder {
        *control = control_ids[control];
    }
    if let Some(semantics) = &mut root.semantics {
        map_expr(&mut semantics.label, locals);
        map_expr(&mut semantics.checked, locals);
        if let Expr::Literal(Value::Text(text)) = &mut semantics.label {
            *text = text.replace(old_name, new_name);
        }
    }
    match &mut root.shape {
        Shape::Text {
            value,
            scale,
            color,
        } => {
            map_expr(value, locals);
            map_expr(scale, locals);
            map_expr(color, locals);
            if let Expr::Literal(Value::Text(text)) = value {
                *text = text.replace(old_name, new_name);
            }
        }
        Shape::Superellipse { radius, color, .. } => {
            map_expr(radius, locals);
            map_expr(color, locals);
        }
        Shape::Circle { color } => map_expr(color, locals),
        Shape::Canvas { .. } => {}
        _ => return Err("Unsupported OIB instance template shape".into()),
    }
    for c in &mut root.children {
        remap(c, seed, prototype, index, control_ids, locals, names)?;
    }
    Ok(())
}
/// Resolve the active instance graph into disposable layout, handler and selector caches.
pub fn expand(
    ui: &Ui,
    allocations: &[Allocation],
    live: &BTreeSet<u64>,
    selected: Option<u64>,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<Ui> {
    let seed = seed(ui).ok_or("Missing OIB seed")?;
    let Some(surface) = &seed.instance_surface else {
        return Ok(ui.clone());
    };
    let pitch = if let Some(layout) = &surface.layout {
        let Value::Number(pitch) = effective(seed, values, layout.preview_pitch_cell)? else {
            return Err("Row pitch must be numeric".into());
        };
        if !(48. ..=1000.).contains(&pitch) {
            return Err("Row pitch must be at least 48 pixels".into());
        }
        pitch
    } else {
        surface.preview_stride
    };
    if live.len() > MAX_LIVE_INSTANCES {
        return Err("OIB live instance capacity exhausted".into());
    }
    let selected = selected.filter(|id| live.contains(id)).or_else(|| {
        allocations
            .iter()
            .find(|a| live.contains(&a.id))
            .map(|a| a.id)
    });
    let mut names = BTreeSet::new();
    if allocations
        .iter()
        .filter(|a| live.contains(&a.id))
        .any(|a| !names.insert(a.name.as_str()))
    {
        return Err("Duplicate instance name; remove a pending instance to resolve".into());
    }
    let mut result = ui.clone();
    // Only the selected inspector exists in the expanded graph. Do not retain
    // selectors referencing a deleted factory instance's former property cells.
    result.selectors.retain(|s| match s.projection {
        Projection::OibCell { cell } => seed
            .cells
            .iter()
            .any(|c| c.id == cell && (c.inherit.is_none() || selected == Some(c.owner))),
        _ => true,
    });
    let mut blueprint = Layout::from_bytes(&ui.layout)?.blueprint().clone();
    blueprint.schema = otrios_interfaces::layout::SPARSE_EVENTS_FORMAT.into();
    let mut roots = BTreeMap::new();
    let mut all_controls = BTreeSet::new();
    let mut stride = 0;
    for prototype in &surface.prototypes {
        let preview = blueprint
            .templates
            .iter()
            .find_map(|t| find(&t.root, prototype.preview_root))
            .ok_or("Missing OIB preview prototype")?
            .clone();
        let inspector = blueprint
            .templates
            .iter()
            .find_map(|t| find(&t.root, prototype.inspector_root))
            .ok_or("Missing OIB inspector prototype")?
            .clone();
        let mut set = BTreeSet::new();
        controls(&preview, &mut set);
        controls(&inspector, &mut set);
        stride = stride.max(set.len());
        all_controls.extend(set.iter().copied());
        roots.insert(prototype.instance, (preview, inspector, set));
    }
    let original_max = usize::from(ui.handlers.iter().map(|h| h.control).max().unwrap_or(0));
    result
        .handlers
        .retain(|h| !all_controls.contains(&h.control));
    let removed: BTreeSet<_> = surface
        .prototypes
        .iter()
        .flat_map(|p| [p.preview_root, p.inspector_root])
        .collect();
    fn remove(root: &mut TemplateNode, ids: &BTreeSet<u16>) {
        root.children.retain(|c| !ids.contains(&c.id));
        for c in &mut root.children {
            remove(c, ids);
        }
    }
    for t in &mut blueprint.templates {
        remove(&mut t.root, &removed);
    }
    for (row, allocation) in allocations
        .iter()
        .filter(|a| live.contains(&a.id))
        .enumerate()
    {
        let index = usize::from(allocation.slot);
        let (mut preview, mut inspector, set) = roots
            .get(&allocation.prototype)
            .ok_or("Undeclared OIB prototype")?
            .clone();
        if allocation.id != allocation.prototype {
            let control_ids: BTreeMap<_, _> = set
                .iter()
                .enumerate()
                .map(|(n, old)| {
                    let new = original_max + 1 + index * stride + n;
                    Ok((*old, u8::try_from(new).map_err(err)?))
                })
                .collect::<Result<_>>()?;
            let mut locals = BTreeMap::new();
            for selector in &ui.selectors {
                if selected == Some(allocation.id)
                    && let Projection::OibCell { cell } = selector.projection
                    && let Some(source) = seed
                        .cells
                        .iter()
                        .find(|c| c.id == cell && c.owner == allocation.prototype)
                {
                    let mut selector = selector.clone();
                    selector.local =
                        u16::try_from(1024 + index * 128 + usize::from(selector.local))
                            .map_err(err)?;
                    selector.projection = Projection::OibCell {
                        cell: cell_id(
                            seed,
                            allocations,
                            allocation.id,
                            source.inherit.ok_or("Missing inherited property")?,
                        )?,
                    };
                    locals.insert(
                        ui.selectors
                            .iter()
                            .find(|s| s.projection == Projection::OibCell { cell })
                            .ok_or("Missing selector")?
                            .local,
                        selector.local,
                    );
                    result.selectors.push(selector);
                }
            }
            let old_name = &seed
                .instances
                .iter()
                .find(|i| i.id == allocation.prototype)
                .ok_or("Missing OIB prototype instance")?
                .name;
            remap(
                &mut preview,
                seed,
                allocation.prototype,
                index,
                &control_ids,
                &locals,
                (old_name, &allocation.name),
            )?;
            remap(
                &mut inspector,
                seed,
                allocation.prototype,
                index,
                &control_ids,
                &locals,
                (old_name, &allocation.name),
            )?;
            let mut visible_controls = BTreeSet::new();
            controls(&roots[&allocation.prototype].0, &mut visible_controls);
            if selected == Some(allocation.id) {
                controls(&roots[&allocation.prototype].1, &mut visible_controls);
            }
            for handler in ui
                .handlers
                .iter()
                .filter(|h| visible_controls.contains(&h.control))
            {
                let mut h = handler.clone();
                h.control = control_ids[&h.control];
                h.event = u16::from(h.control);
                match &mut h.operation {
                    Operation::Oib { cell, .. } => {
                        let source = seed
                            .cells
                            .iter()
                            .find(|c| c.id == *cell)
                            .ok_or("Missing OIB handler cell")?;
                        *cell = cell_id(
                            seed,
                            allocations,
                            allocation.id,
                            source.inherit.ok_or("Missing inherited property")?,
                        )?;
                    }
                    Operation::OibInstance { target, .. } => *target = allocation.id,
                    Operation::Set => {}
                    _ => return Err("OIB factory cannot instantiate this operation".into()),
                }
                result.handlers.push(h);
            }
        } else {
            let mut visible_controls = BTreeSet::new();
            controls(&preview, &mut visible_controls);
            if selected == Some(allocation.id) {
                controls(&inspector, &mut visible_controls);
            }
            result.handlers.extend(
                ui.handlers
                    .iter()
                    .filter(|h| visible_controls.contains(&h.control))
                    .cloned(),
            );
        }
        preview.frame[1] = Expr::Literal(Value::Number(row as f32 * pitch));
        inspector.frame[1] = Expr::Literal(Value::Number(surface.inspector_start));
        blueprint
            .templates
            .iter_mut()
            .find_map(|t| node(&mut t.root, surface.preview_viewport))
            .ok_or("Missing preview viewport")?
            .children
            .push(preview);
        if selected == Some(allocation.id) {
            blueprint
                .templates
                .iter_mut()
                .find_map(|t| node(&mut t.root, seed.inspector_viewport))
                .ok_or("Missing inspector viewport")?
                .children
                .push(inspector);
        }
    }
    let mut resolved = expanded_seed(seed, allocations)?;
    resolved
        .instance_surface
        .as_mut()
        .ok_or("Missing instance surface")?
        .preview_stride = pitch;
    fn has_node(blueprint: &Blueprint, id: u16) -> bool {
        blueprint
            .templates
            .iter()
            .any(|t| find(&t.root, id).is_some())
    }
    for cell in &mut resolved.cells {
        cell.targets.retain(|t| has_node(&blueprint, t.node));
    }
    resolved.instances.retain(|i| live.contains(&i.id));
    resolved
        .cells
        .retain(|c| seed.components.iter().any(|i| i.id == c.owner) || live.contains(&c.owner));
    resolved.inspector_content[1] = surface.inspector_start
        + if selected.is_some() {
            surface.inspector_stride
        } else {
            0.
        };
    result.presentation = Some(Presentation::Oib(resolved));
    result.layout = Layout::encode(&blueprint)?;
    if result.handlers.len() > 128 || result.selectors.len() > 128 {
        return Err("OIB expanded control/selector budget".into());
    }
    Ok(result)
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
        || surface.node_stride == 0
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
        let b = inspector
            .children
            .iter()
            .find(|c| c.id == prototype.inspector_root)
            .ok_or("Factory inspector must be a direct viewport child")?;
        let before = seen.clone();
        visit(a, &mut seen)?;
        visit(b, &mut seen)?;
        let actual: Vec<_> = seen.difference(&before).copied().collect();
        if actual != prototype.nodes || actual.len() > usize::from(surface.node_stride) {
            return Err("Invalid signed OIB node allocation table".into());
        }
        let mut set = BTreeSet::new();
        controls(a, &mut set);
        controls(b, &mut set);
        for h in ui.handlers.iter().filter(|h| set.contains(&h.control)) {
            match h.operation {
                Operation::Set if h.fields.len() == 1 => {}
                Operation::Oib { cell, .. }
                    if seed
                        .cells
                        .iter()
                        .any(|c| c.id == cell && c.owner == prototype.instance) => {}
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
