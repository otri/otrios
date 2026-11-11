// SPDX-License-Identifier: LGPL-3.0-only
//! Dependency-qualified visual updates inside an unchanged, admitted template.
use super::*;

pub(super) fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.to_bits() == b.to_bits(),
        (Value::Series(a), Value::Series(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits())
        }
        (Value::TimedSeries(a), Value::TimedSeries(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits()))
        }
        _ => a == b,
    }
}
fn changed(old: &BTreeMap<u16, Value>, new: &BTreeMap<u16, Value>) -> BTreeSet<u16> {
    old.keys()
        .chain(new.keys())
        .filter(|id| match (old.get(id), new.get(id)) {
            (Some(a), Some(b)) => !same_value(a, b),
            (None, None) => false,
            _ => true,
        })
        .copied()
        .collect()
}
fn depends(expr: &Expr, changed: &BTreeSet<u16>) -> bool {
    match expr {
        Expr::State(Binding::Local(id)) => changed.contains(id),
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            depends(a, changed) || depends(b, changed)
        }
        Expr::Select { condition, yes, no } => {
            depends(condition, changed) || depends(yes, changed) || depends(no, changed)
        }
        Expr::Literal(_) | Expr::Parameter(_) | Expr::Width | Expr::Height => false,
    }
}
fn parameter(expr: &Expr) -> bool {
    match expr {
        Expr::Parameter(_) => true,
        Expr::Add(a, b) | Expr::Subtract(a, b) | Expr::Multiply(a, b) => {
            parameter(a) || parameter(b)
        }
        Expr::Select { condition, yes, no } => {
            parameter(condition) || parameter(yes) || parameter(no)
        }
        Expr::Literal(_) | Expr::State(_) | Expr::Width | Expr::Height => false,
    }
}
fn shape_exprs(shape: &Shape) -> impl Iterator<Item = &Expr> {
    match shape {
        Shape::Canvas { .. } | Shape::Slot => [None, None, None],
        Shape::Text {
            value,
            scale,
            color,
        } => [Some(value), Some(scale), Some(color)],
        Shape::Superellipse { radius, color, .. } => [Some(radius), Some(color), None],
        Shape::Circle { color } => [Some(color), None, None],
        Shape::SeriesPath { series, color, .. } | Shape::TimedSeriesPath { series, color, .. } => {
            [Some(series), Some(color), None]
        }
    }
    .into_iter()
    .flatten()
}
fn supported(node: &TemplateNode) -> bool {
    !matches!(node.shape, Shape::Slot)
        && !node.frame.iter().chain([&node.visible]).any(parameter)
        && !shape_exprs(&node.shape).any(parameter)
        && node
            .semantics
            .as_ref()
            .is_none_or(|s| !parameter(&s.label) && !parameter(&s.checked))
        && node.children.iter().all(supported)
}
fn template(blueprint: &Blueprint) -> Option<&TemplateNode> {
    if blueprint.schema == "otrios-layout-v1"
        || blueprint.templates.len() != 1
        || !blueprint.root.children.is_empty()
        || !blueprint.root.properties.is_empty()
    {
        return None;
    }
    let template = &blueprint.templates[0];
    (template.id == blueprint.root.template
        && template.parameters.is_empty()
        && supported(&template.root))
    .then_some(&template.root)
}
pub(super) fn input_bounds_stable(
    blueprint: &Blueprint,
    old: &BTreeMap<u16, Value>,
    new: &BTreeMap<u16, Value>,
) -> bool {
    let Some(root) = template(blueprint) else {
        return false;
    };
    let changed = changed(old, new);
    fn stable(node: &TemplateNode, changed: &BTreeSet<u16>) -> (bool, bool) {
        let mut responsive = node.responder.is_some();
        for child in &node.children {
            let (safe, child_responsive) = stable(child, changed);
            if !safe {
                return (false, true);
            }
            responsive |= child_responsive;
        }
        (
            !depends(&node.visible, changed)
                && (!responsive || !node.frame.iter().any(|e| depends(e, changed))),
            responsive,
        )
    }
    !measurement_changed(root, &changed, false) && stable(root, &changed).0
}
/// Paint-only reuse remains available in Auto Layout trees. Text whose intrinsic
/// size participates in Hug invalidates layout, while colors and semantic labels
/// do not. Flow allocation depends on authored fixed dimensions, not raw x/y.
fn measurement_changed(node: &TemplateNode, changed: &BTreeSet<u16>, in_flow: bool) -> bool {
    let policy = node.layout.unwrap_or_default();
    if (node.layout.is_some() || in_flow)
        && (node.frame[2..].iter().any(|expr| depends(expr, changed))
            || (!in_flow || policy.placement == Placement::Free)
                && node.frame[..2].iter().any(|expr| depends(expr, changed)))
    {
        return true;
    }
    if (policy.width.mode == Sizing::Hug || policy.height.mode == Sizing::Hug)
        && let Shape::Text { value, scale, .. } = &node.shape
        && (depends(value, changed) || depends(scale, changed))
    {
        return true;
    }
    node.children
        .iter()
        .any(|child| measurement_changed(child, changed, policy.container.is_some()))
}
fn auto_frame_ids(node: &TemplateNode, in_flow: bool, output: &mut BTreeSet<u16>) {
    if node.layout.is_some() || in_flow {
        output.insert(node.id);
    }
    let flow = node.layout.is_some_and(|p| p.container.is_some());
    for child in &node.children {
        auto_frame_ids(child, flow, output);
    }
}
fn eligible(
    node: &TemplateNode,
    changed: &BTreeSet<u16>,
    old: &NodeEvaluation<'_, '_>,
    overridden: &BTreeSet<u16>,
) -> Result<bool> {
    if depends(&node.visible, changed) {
        return Ok(false);
    }
    if !boolean(old.run(&node.visible)?)? {
        return Ok(true);
    }
    if !overridden.contains(&node.id) && node.frame[2..].iter().any(|expr| depends(expr, changed)) {
        return Ok(false);
    }
    for child in &node.children {
        if !eligible(child, changed, old, overridden)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn matches_shape(shape: &Shape, primitive: &Primitive) -> bool {
    match (shape, primitive) {
        (Shape::Canvas { clip: a }, Primitive::Canvas { clip: b }) => a == b,
        (Shape::Text { .. }, Primitive::Text { .. })
        | (Shape::Circle { .. }, Primitive::Circle { .. }) => true,
        (Shape::Superellipse { exponent: a, .. }, Primitive::Superellipse { exponent: b, .. }) => {
            a == b
        }
        (
            Shape::SeriesPath { width: a, .. } | Shape::TimedSeriesPath { width: a, .. },
            Primitive::Path { width: b, .. },
        ) => a == b,
        _ => false,
    }
}
struct Update<'a, 'slots> {
    blueprint: &'a Blueprint,
    changed: &'a BTreeSet<u16>,
    old: NodeEvaluation<'a, 'slots>,
    next: NodeEvaluation<'a, 'slots>,
    modified: bool,
    frames: BTreeMap<u16, [f32; 4]>,
}
impl Update<'_, '_> {
    fn patch(
        &mut self,
        node: &TemplateNode,
        target: &mut SceneNode,
        parent: [f32; 2],
        is_root: bool,
    ) -> Result<bool> {
        let expected_id = self
            .blueprint
            .root
            .id
            .checked_mul(1000)
            .and_then(|id| id.checked_add(u32::from(node.id)))
            .ok_or("Expanded scene identity overflow")?;
        if target.id != expected_id
            || target.responder != resolve_responder(&self.blueprint.schema, node.responder)?
            || !matches_shape(&node.shape, &target.primitive)
            || target.semantics.is_some() != node.semantics.is_some()
        {
            return Ok(false);
        }
        if node.frame.iter().any(|expr| depends(expr, self.changed)) {
            let frame = self.next.frame(
                node,
                parent,
                is_root,
                &self.blueprint.schema,
                Some(&self.frames),
            )?;
            if frame.width != target.frame.width || frame.height != target.frame.height {
                return Ok(false);
            }
            target.frame = frame;
            self.modified = true;
        }
        if shape_exprs(&node.shape).any(|expr| depends(expr, self.changed)) {
            target.primitive = self.next.primitive(&node.shape, target.frame)?;
            self.modified = true;
        }
        if node
            .semantics
            .as_ref()
            .is_some_and(|s| depends(&s.label, self.changed) || depends(&s.checked, self.changed))
        {
            target.semantics = self.next.semantics(node.semantics.as_ref())?;
            self.modified = true;
        }
        let mut index = 0;
        for child in &node.children {
            if !boolean(self.old.run(&child.visible)?)? {
                continue;
            }
            let Some(target_child) = target.children.get_mut(index) else {
                return Ok(false);
            };
            if !self.patch(
                child,
                target_child,
                [target.frame.width, target.frame.height],
                false,
            )? {
                return Ok(false);
            }
            index += 1;
        }
        Ok(index == target.children.len())
    }
}
pub(super) fn update(
    blueprint: &Blueprint,
    previous: &Scene,
    old_slots: &BTreeMap<u16, Value>,
    new_slots: &BTreeMap<u16, Value>,
    viewport: [f32; 2],
) -> Result<Option<Scene>> {
    let Some(template) = template(blueprint) else {
        return Ok(None);
    };
    let changed = changed(old_slots, new_slots);
    if changed.is_empty() {
        return Ok(Some(previous.clone()));
    }
    let mut instance_frame = blueprint.root.frame;
    let mut overridden = BTreeSet::new();
    if let Some(composition) = &blueprint.composition {
        instance_frame[2..].copy_from_slice(&viewport);
        fn ids(flow: &Flow, output: &mut BTreeSet<u16>) {
            match &flow.kind {
                FlowKind::Item { node, .. } => {
                    output.insert(*node);
                }
                FlowKind::Wrap { nodes, .. } => output.extend(nodes),
                FlowKind::Group { children, .. } => {
                    for child in children {
                        ids(child, output);
                    }
                }
            }
        }
        ids(
            if viewport[0] >= composition.wide_min_width {
                &composition.wide.content
            } else {
                &composition.tall.content
            },
            &mut overridden,
        );
    }
    auto_frame_ids(template, false, &mut overridden);
    let mut frames = BTreeMap::new();
    fn existing_frames(
        node: &SceneNode,
        overridden: &BTreeSet<u16>,
        frames: &mut BTreeMap<u16, [f32; 4]>,
    ) {
        if let Ok(id) = u16::try_from(node.id)
            && overridden.contains(&id)
        {
            let f = node.frame;
            frames.insert(id, [f.x, f.y, f.width, f.height]);
        }
        for child in &node.children {
            existing_frames(child, overridden, frames);
        }
    }
    existing_frames(previous.root(), &overridden, &mut frames);
    let params = BTreeMap::new();
    let old_bindings = Bindings { local: old_slots };
    let next_bindings = Bindings { local: new_slots };
    let old = NodeEvaluation {
        instance_frame,
        params: &params,
        bindings: &old_bindings,
    };
    if measurement_changed(template, &changed, false)
        || !eligible(template, &changed, &old, &overridden)?
    {
        return Ok(None);
    }
    let expected = instance_frame;
    if template.layout.is_none()
        && previous.root().frame != Rect::new(expected[0], expected[1], expected[2], expected[3])
    {
        return Ok(None);
    }
    let mut target = previous.root().clone();
    let mut update = Update {
        blueprint,
        changed: &changed,
        old,
        next: NodeEvaluation {
            instance_frame,
            params: &params,
            bindings: &next_bindings,
        },
        modified: false,
        frames,
    };
    if !update.patch(template, &mut target, [expected[2], expected[3]], true)? {
        return Ok(None);
    }
    if !update.modified {
        return Ok(Some(previous.clone()));
    }
    Scene::new(target).map(Some)
}

#[cfg(test)]
mod tests;
