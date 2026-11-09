// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded ownership and parent-local placement over existing factory resources.
use super::*;
use otrios_interfaces::oib::authoring::{Destination, Placement};

pub fn validate_frame(frame: Option<[f32; 4]>) -> Result<()> {
    if frame.is_some_and(|f| {
        f.iter()
            .any(|n| !n.is_finite() || *n < 0. || *n > 16000. || (*n == 0. && n.is_sign_negative()))
            || f[2] <= 0.
            || f[2] > 1000.
            || f[3] <= 0.
            || f[3] > 1000.
            || f[0] + f[2] > 16000.
            || f[1] + f[3] > 16000.
    }) {
        Err("Placement requires finite coordinates within 16000 and sizes in (0,1000]".into())
    } else {
        Ok(())
    }
}

pub fn slot_host(
    doc: &Document,
    component: &ObjectAddress,
    slot: SlotId,
) -> Result<(ObjectAddress, SlotDescriptor)> {
    let parent = instance(doc, component.instance)?;
    let (scope, root) = path_node(
        doc,
        parent.definition,
        &component.references,
        component.node,
    )?;
    if root.id != scope.root {
        return Err("A slot belongs to a complete component occurrence".into());
    }
    let slot = scope
        .slots
        .iter()
        .find(|s| s.id == slot)
        .ok_or("Unknown admitted component slot")?
        .clone();
    let mut references = component.references.clone();
    references.extend(&slot.references);
    Ok((
        ObjectAddress {
            instance: component.instance,
            references,
            node: slot.node,
        },
        slot,
    ))
}

pub fn destination(placements: &[Placement], id: InstanceId) -> Destination {
    placements
        .iter()
        .find(|p| p.instance == id)
        .map_or(Destination::Canvas, |p| p.destination.clone())
}

pub fn slots(
    seed: &Seed,
    allocations: &[Allocation],
    component: &ObjectAddress,
) -> Result<Vec<(String, Destination)>> {
    let doc = expanded_metadata(seed, allocations)?;
    let instance = instance(&doc, component.instance)?;
    let (definition, node) = path_node(
        &doc,
        instance.definition,
        &component.references,
        component.node,
    )?;
    if node.id != definition.root {
        return Ok(vec![]);
    }
    Ok(definition
        .slots
        .iter()
        .map(|slot| {
            (
                slot.name.clone(),
                Destination::Slot {
                    component: component.clone(),
                    slot: slot.id,
                },
            )
        })
        .collect())
}

pub fn preview_content(seed: &Seed) -> [f32; 2] {
    let Some(surface) = &seed.instance_surface else {
        return [0., 0.];
    };
    let placements = seed
        .designer
        .as_ref()
        .map_or(&[][..], |d| d.placements.as_slice());
    let roots = seed
        .instances
        .iter()
        .filter(|i| destination(placements, InstanceId(i.id)) == Destination::Canvas)
        .count();
    let mut content = [0., roots as f32 * surface.preview_stride];
    for placement in placements.iter().filter(|p| {
        p.destination == Destination::Canvas && seed.instances.iter().any(|i| i.id == p.instance.0)
    }) {
        if let Some([x, y, w, h]) = placement.frame {
            content[0] = content[0].max(x + w);
            content[1] = content[1].max(y + h);
        }
    }
    content
}

/// Called after stable factory expansion, never on the signed seed itself.
pub(super) fn project_metadata(doc: &mut Document, allocations: &[Allocation]) -> Result<()> {
    let placements = doc.placements.clone();
    if placements.len() > MAX_ALLOCATIONS
        || placements
            .windows(2)
            .any(|w| w[0].instance >= w[1].instance)
    {
        return Err("Invalid OIB placement ordering or capacity".into());
    }
    let mut counts = BTreeMap::<(ObjectAddress, SlotId), usize>::new();
    for p in &placements {
        validate_frame(p.frame)?;
        if !allocations.iter().any(|a| a.id == p.instance.0) {
            return Err("Placement references a nonresident component".into());
        }
        let placed = instance(doc, p.instance)?;
        let root = definition(doc, placed.definition)?;
        let kind = class(doc, path_node(doc, root.id, &[], root.root)?.1.class)?.implementation;
        if matches!(
            kind,
            ClassKind::O2DButton | ClassKind::O2DToggle | ClassKind::O2DSlider
        ) && p.frame.is_some_and(|f| f[2] < 48. || f[3] < 32.)
        {
            return Err("Placed control must retain its minimum 48×32 hit region".into());
        }
        if let Destination::Slot { component, slot } = &p.destination {
            if component.instance == p.instance
                || !allocations.iter().any(|a| a.id == component.instance.0)
            {
                return Err("Slot owner must be another resident component".into());
            }
            let (host, descriptor) = slot_host(doc, component, *slot)?;
            let child = instance(doc, p.instance)?;
            let child_def = definition(doc, child.definition)?;
            let class = path_node(doc, child.definition, &[], child_def.root)?
                .1
                .class;
            if !admission::accepts(doc, &descriptor.admitted_classes, class)? {
                return Err("Component class is not admitted by this slot".into());
            }
            if descriptor.layout != SlotLayout::Absolute {
                return Err("Owned content currently requires an absolute signed slot".into());
            }
            *counts.entry((component.clone(), *slot)).or_default() += 1;
            let order = allocations
                .iter()
                .position(|a| a.id == p.instance.0)
                .ok_or("Missing resident child")?;
            let child = doc
                .instances
                .iter_mut()
                .find(|i| i.id == p.instance)
                .ok_or("Missing placed component")?;
            child.owner = Owner::Instance(component.instance);
            child.visual_parent = Some(host);
            child.order = u16::try_from(order).map_err(err)?;
        }
    }
    for ((component, slot), count) in counts {
        let (host, descriptor) = slot_host(doc, &component, slot)?;
        let parent = instance(doc, host.instance)?;
        let (scope, node) = path_node(doc, parent.definition, &host.references, host.node)?;
        let inherited = scope
            .nodes
            .iter()
            .filter(|n| n.owner == Some(node.id))
            .count();
        let count = count
            .checked_add(inherited)
            .ok_or("Slot cardinality overflow")?;
        if count < usize::from(descriptor.cardinality[0])
            || count > usize::from(descriptor.cardinality[1])
        {
            return Err("Owned slot cardinality exceeded".into());
        }
    }
    for child in &doc.instances {
        let mut owner = child.owner;
        let mut seen = BTreeSet::from([child.id]);
        while let Owner::Instance(id) = owner {
            if !seen.insert(id) || seen.len() > MAX_REFERENCE_DEPTH {
                return Err("Cyclic or too deep owned component hierarchy".into());
            }
            owner = instance(doc, id)?.owner;
        }
    }
    Ok(())
}

/// Reattach already expanded factory roots. No renderer record, handler, selector
/// or property mapping is invented by placement; identities keep their resources.
pub fn project_ui(ui: &mut Ui, allocations: &[Allocation], live: &BTreeSet<u64>) -> Result<()> {
    let seed = super::super::seed(ui).ok_or("Missing OIB seed")?;
    if seed
        .designer
        .as_ref()
        .is_none_or(|doc| doc.placements.is_empty())
    {
        return Ok(());
    }
    let mut blueprint = Layout::from_bytes(&ui.layout)?.blueprint().clone();
    project_blueprint(ui, &mut blueprint, allocations, live)?;
    ui.layout = Layout::encode(&blueprint)?;
    Ok(())
}

pub(crate) fn project_blueprint(
    ui: &Ui,
    blueprint: &mut Blueprint,
    allocations: &[Allocation],
    live: &BTreeSet<u64>,
) -> Result<()> {
    let seed = super::super::seed(ui).ok_or("Missing OIB seed")?;
    if seed.designer.is_none() {
        return Ok(());
    }
    let doc = metadata(seed)?;
    if doc.placements.is_empty() {
        return Ok(());
    }
    let surface = seed
        .instance_surface
        .as_ref()
        .ok_or("Missing OIB instance surface")?;
    let preview = blueprint
        .templates
        .iter_mut()
        .find_map(|t| super::super::node(&mut t.root, surface.preview_viewport))
        .ok_or("Missing canvas viewport")?;
    let mut roots = BTreeMap::new();
    for a in allocations.iter().filter(|a| live.contains(&a.id)) {
        let i = instance(doc, InstanceId(a.id))?;
        let address = ObjectAddress {
            instance: i.id,
            references: vec![],
            node: definition(doc, i.definition)?.root,
        };
        let id = doc
            .mappings
            .objects
            .iter()
            .find(|m| m.address == address)
            .and_then(|m| m.layout_node)
            .ok_or("Missing placed root mapping")?;
        let index = preview
            .children
            .iter()
            .position(|n| n.id == id)
            .ok_or("Missing expanded canvas child")?;
        roots.insert(a.id, preview.children.remove(index));
    }
    for p in &doc.placements {
        if !live.contains(&p.instance.0) {
            continue;
        }
        if let Destination::Slot { component, .. } = &p.destination
            && !live.contains(&component.instance.0)
        {
            return Err("Move or remove owned children before removing their owner".into());
        }
    }
    struct Projection<'a> {
        doc: &'a Document,
        allocations: &'a [Allocation],
        live: &'a BTreeSet<u64>,
        roots: BTreeMap<u64, TemplateNode>,
        visiting: BTreeSet<u64>,
    }
    impl Projection<'_> {
        fn build(&mut self, id: u64, canvas_row: usize, pitch: f32) -> Result<TemplateNode> {
            if !self.visiting.insert(id) || self.visiting.len() > MAX_LIVE_INSTANCES {
                return Err("Cyclic owned component projection".into());
            }
            let mut root = self
                .roots
                .remove(&id)
                .ok_or("Component was attached more than once")?;
            let placement = self.doc.placements.iter().find(|p| p.instance.0 == id);
            if let Some(frame) = placement.and_then(|p| p.frame) {
                root.frame = frame.map(|n| Expr::Literal(Value::Number(n)));
            } else {
                root.frame[1] = Expr::Literal(Value::Number(
                    if placement.is_some_and(|p| matches!(p.destination, Destination::Slot { .. }))
                    {
                        0.
                    } else {
                        canvas_row as f32 * pitch
                    },
                ));
            }
            let children: Vec<_> = self.allocations.iter().filter(|a| self.live.contains(&a.id)
                && matches!(destination(&self.doc.placements, InstanceId(a.id)), Destination::Slot { component, .. } if component.instance.0 == id))
                .map(|a| a.id).collect();
            for child in children {
                let owner = instance(self.doc, InstanceId(child))?
                    .visual_parent
                    .as_ref()
                    .ok_or("Missing owned-child attachment")?;
                let host = self
                    .doc
                    .mappings
                    .objects
                    .iter()
                    .find(|m| &m.address == owner)
                    .and_then(|m| m.layout_node)
                    .ok_or("Missing slot render mapping")?;
                let child = self.build(child, 0, pitch)?;
                super::super::node(&mut root, host)
                    .ok_or("Slot host escapes its owner")?
                    .children
                    .push(child);
            }
            self.visiting.remove(&id);
            Ok(root)
        }
    }
    let mut projection = Projection {
        doc,
        allocations,
        live,
        roots,
        visiting: BTreeSet::new(),
    };
    let mut row = 0;
    for a in allocations.iter().filter(|a| live.contains(&a.id)) {
        if destination(&doc.placements, InstanceId(a.id)) == Destination::Canvas {
            preview
                .children
                .push(projection.build(a.id, row, surface.preview_stride)?);
            row += 1;
        }
    }
    if !projection.roots.is_empty() {
        return Err("Owned component has no reachable canvas root".into());
    }
    Ok(())
}
