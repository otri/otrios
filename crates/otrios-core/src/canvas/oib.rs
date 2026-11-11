// SPDX-License-Identifier: LGPL-3.0-only
//! Fixed signed OIB edit surface, inherited property resolution and layout compilation.
use super::*;
use crate::dashboard::layout::Layout;
use otrios_interfaces::{
    layout::{Blueprint, Expr, Shape, TemplateNode, Value},
    oib::*,
};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) mod component_behaviors;
pub mod contract;
pub mod document;
pub(crate) mod endpoint_impact;
pub mod graph;
pub(crate) mod history;
pub(crate) mod impact;
pub mod imports;
pub(crate) mod instance_impact;
pub mod instances;
pub(crate) mod paint;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Info {
    /// Read-only verification diagnostics; hashes confer no authority.
    pub semantic_epoch: String,
    pub baseline_lock: String,
    pub libraries: Vec<otrios_interfaces::oib::graph::LibraryLock>,
    pub active_generation: u64,
    pub presented_generation: u64,
    pub conflicts: Vec<u64>,
    pub status: String,
    pub properties: BTreeMap<u64, String>,
    pub instances: Vec<InstanceInfo>,
    pub available_instance_slots: usize,
    pub selected_instance: Option<u64>,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct InstanceInfo {
    pub id: u64,
    pub name: String,
    pub component: u64,
    pub prototype: u64,
    pub active: bool,
}

pub fn seed(ui: &Ui) -> Option<&Seed> {
    match &ui.presentation {
        Some(Presentation::Oib(seed)) => Some(seed),
        _ => None,
    }
}
pub fn validate_value(cell: &Cell, value: &Value) -> Result<()> {
    let valid = match (&cell.default, value) {
        (Value::Text(_), Value::Text(v)) => !v.is_empty() && v.len() <= 96 && !v.contains('\0'),
        (Value::Color(_), Value::Color(_)) => true,
        (Value::Boolean(_), Value::Boolean(_)) => true,
        (Value::Number(_), Value::Number(v)) => v.is_finite() && (0. ..=1000.).contains(v),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("Invalid OIB property type or value".into())
    }
}
fn node(root: &mut TemplateNode, id: u16) -> Option<&mut TemplateNode> {
    if root.id == id {
        return Some(root);
    }
    root.children.iter_mut().find_map(|child| node(child, id))
}
pub fn validate_seed(seed: &Seed) -> Result<()> {
    let _profile = crate::canvas::profiling::span("oib.seed.validate");
    let mut local_source = seed.clone();
    local_source.resolved_libraries.clear();
    if seed.format != FORMAT
        || seed.main_oib == 0
        || seed.main_class != "otrios.declarative-v1"
        || seed.components.is_empty()
        || seed.components.len() > 16
        || seed.components.len() + seed.instances.len() > 64
        || seed.cells.len()
            > if seed.graph.is_some() && !seed.expanded {
                MAX_CELLS * (designer::MAX_DEFINITIONS + 1)
            } else {
                MAX_CELLS
            }
        || flexbuffers::to_vec(&local_source).map_err(err)?.len() > 24 * 1024
        || flexbuffers::to_vec(seed).map_err(err)?.len() > otrios_interfaces::oib::MAX_BYTES
        || seed
            .inspector_content
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > 16000.)
    {
        return Err("Unsupported or oversized OIB seed".into());
    }
    let mut ids = BTreeSet::from([seed.main_oib]);
    let mut names = BTreeSet::new();
    for object in &seed.components {
        if object.component.is_some()
            || object.id == 0
            || !ids.insert(object.id)
            || object.name.is_empty()
            || object.name.len() > 64
            || (seed.designer.is_none() && !names.insert(object.name.as_str()))
            || (seed.designer.is_some() && object.name.contains('\0'))
        {
            return Err("Invalid OIB component identity".into());
        }
    }
    names.clear();
    for object in &seed.instances {
        if !seed
            .components
            .iter()
            .any(|c| Some(c.id) == object.component)
            || object.id == 0
            || !ids.insert(object.id)
            || object.name.is_empty()
            || object.name.len() > 64
            || (seed.designer.is_none() && !names.insert(object.name.as_str()))
            || (seed.designer.is_some() && object.name.contains('\0'))
        {
            return Err("Invalid OIB instance identity".into());
        }
    }
    if let Some(surface) = &seed.instance_surface
        && ((seed.designer.is_none() && surface.prototypes.iter().any(|p| p.palette_only))
            || surface.prototypes.len() != seed.instances.len()
            || seed.instances.len()
                > if seed.graph.is_some() && !seed.expanded {
                    designer::MAX_DEFINITIONS
                } else {
                    MAX_ALLOCATIONS
                }
            || seed.instances.len()
                > if seed.graph.is_some() && !seed.expanded {
                    designer::MAX_DEFINITIONS
                } else {
                    MAX_LIVE_INSTANCES
                }
            || [
                surface.preview_stride,
                surface.inspector_start,
                surface.inspector_stride,
            ]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > 1000.)
            || surface
                .prototypes
                .iter()
                .map(|p| p.instance)
                .collect::<BTreeSet<_>>()
                != seed.instances.iter().map(|i| i.id).collect())
    {
        return Err("Invalid bounded OIB instance surface".into());
    }
    let mut cells = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let mut properties = BTreeSet::new();
    for cell in &seed.cells {
        if cell.id == 0
            || !cells.insert(cell.id)
            || !ids.contains(&cell.owner)
            || cell.owner == seed.main_oib
            || cell.name.is_empty()
            || cell.name.len() > 64
            || (seed.designer.is_none() && !properties.insert((cell.owner, cell.name.as_str())))
            || cell.targets.len() > 16
        {
            return Err("Invalid OIB cell identity".into());
        }
        validate_value(cell, &cell.default)?;
        if let Some(value) = &cell.authored_override {
            if cell.inherit.is_none() {
                return Err("Only instances have authored overrides".into());
            }
            validate_value(cell, value)?;
        }
        if cell.choices.len() > 8 {
            return Err("OIB inspector choice limit".into());
        }
        for choice in &cell.choices {
            validate_value(cell, choice)?;
        }
        if let Some(parent) = cell.inherit {
            let base = seed
                .cells
                .iter()
                .find(|c| c.id == parent)
                .ok_or("Missing OIB inherited property")?;
            let owner = seed
                .instances
                .iter()
                .find(|i| i.id == cell.owner)
                .ok_or("Only instances inherit")?;
            if base.inherit.is_some()
                || (seed.designer.is_none()
                    && (Some(base.owner) != owner.component || base.name != cell.name))
                || std::mem::discriminant(&base.default) != std::mem::discriminant(&cell.default)
            {
                return Err("Invalid OIB inheritance".into());
            }
        } else if !seed.components.iter().any(|c| c.id == cell.owner) {
            return Err("Instance property needs an inherited definition".into());
        }
        for target in &cell.targets {
            if !target.scale.is_finite()
                || !target.offset.is_finite()
                || (seed.designer.is_none()
                    && !targets.insert((
                        target.node,
                        if target.property == Property::FrameWidth {
                            Property::Width as u8
                        } else {
                            target.property as u8
                        },
                    )))
            {
                return Err("Duplicated or nonfinite OIB target".into());
            }
        }
    }
    if let Some(layout) = seed
        .instance_surface
        .as_ref()
        .and_then(|s| s.layout.as_ref())
    {
        if !seed.cells.iter().any(|c| {
            c.id == layout.preview_pitch_cell
                && seed.components.iter().any(|o| o.id == c.owner)
                && matches!(c.default, Value::Number(_))
        }) {
            return Err("Invalid OIB layout surface".into());
        }
    } else if seed.graph.is_none()
        && seed.cells.iter().flat_map(|c| &c.targets).any(|t| {
            matches!(
                t.property,
                Property::X
                    | Property::Height
                    | Property::FrameWidth
                    | Property::OffsetY
                    | Property::Visible
            )
        })
    {
        return Err("Geometry targets require an OIB layout surface".into());
    }
    if seed.designer.is_some() {
        document::validate(seed)?;
    }
    Ok(())
}
pub fn validate(ui: &Ui) -> Result<()> {
    let seed = seed(ui).ok_or("Missing OIB seed")?;
    if ui.behavior.is_some() {
        return Err("O1 uses a declarative main class".into());
    }
    validate_seed(seed)?;
    if seed.designer.is_some() {
        document::validate_layout(ui)?;
    }
    let layout = compile(ui, &BTreeMap::new())?;
    component_behaviors::validate(ui, &layout)?;
    let blueprint = layout.blueprint();
    let mut found = blueprint.clone();
    let viewport = found
        .templates
        .iter_mut()
        .find_map(|t| node(&mut t.root, seed.inspector_viewport))
        .ok_or("Missing OIB inspector viewport")?;
    if !matches!(viewport.shape, Shape::Canvas { clip: true }) {
        return Err("OIB inspector must be a clipped scrolling panel".into());
    }
    if let Some(surface) = &seed.instance_surface {
        let preview = found
            .templates
            .iter_mut()
            .find_map(|t| node(&mut t.root, surface.preview_viewport))
            .ok_or("Missing OIB preview viewport")?;
        if !matches!(preview.shape, Shape::Canvas { clip: true }) {
            return Err("OIB preview must be clipped".into());
        }
        instances::validate_surface(ui)?;
    }
    Ok(())
}
pub fn effective(seed: &Seed, values: &BTreeMap<u64, Option<Value>>, id: u64) -> Result<Value> {
    if seed.designer.is_some() {
        return document::effective(seed, values, id);
    }
    let cell = seed
        .cells
        .iter()
        .find(|c| c.id == id)
        .ok_or("Unknown OIB cell")?;
    if let Some(Some(value)) = values.get(&id) {
        return Ok(value.clone());
    }
    if let Some(value) = &cell.authored_override {
        return Ok(value.clone());
    }
    if let Some(parent) = cell.inherit {
        let base = seed
            .cells
            .iter()
            .find(|c| c.id == parent)
            .ok_or("Missing OIB base cell")?;
        return Ok(values
            .get(&parent)
            .and_then(|v| v.clone())
            .unwrap_or_else(|| base.default.clone()));
    }
    Ok(cell.default.clone())
}
pub fn compile(ui: &Ui, values: &BTreeMap<u64, Option<Value>>) -> Result<Layout> {
    let layout = Layout::from_bytes(&ui.layout)?;
    compile_prepared(ui, layout.blueprint(), values)
}
/// Compile admitted typed graph output without a FlexBuffers round trip.
pub(crate) fn compile_prepared(
    ui: &Ui,
    blueprint: &Blueprint,
    values: &BTreeMap<u64, Option<Value>>,
) -> Result<Layout> {
    let _profile = crate::canvas::profiling::span("oib.compile_layout");
    let seed = seed(ui).ok_or("Missing OIB seed")?;
    let mut blueprint = blueprint.clone();
    let outputs = document::output_cells(seed)?;
    for cell in &seed.cells {
        let value = effective(seed, values, cell.id)?;
        validate_value(cell, &value)?;
        for target in &cell.targets {
            if outputs.get(&document::output_key(target)) != Some(&cell.id) {
                continue;
            }
            let node = blueprint
                .templates
                .iter_mut()
                .find_map(|t| node(&mut t.root, target.node))
                .ok_or("Missing OIB target node")?;
            match (target.property, &value) {
                (Property::OffsetY, Value::Number(_)) => {}
                (Property::Visible, Value::Boolean(v)) => {
                    node.visible = Expr::Literal(Value::Boolean(*v))
                }
                (Property::Text, Value::Text(v)) => match &mut node.shape {
                    Shape::Text { value, .. } => *value = Expr::Literal(Value::Text(v.clone())),
                    _ => return Err("OIB text target is not text".into()),
                },
                (Property::SemanticLabel, Value::Text(v)) => {
                    node.semantics
                        .as_mut()
                        .ok_or("OIB semantic target has no semantics")?
                        .label = Expr::Literal(Value::Text(v.clone()));
                }
                (Property::Color, Value::Color(v)) => match &mut node.shape {
                    Shape::Text { color, .. }
                    | Shape::Superellipse { color, .. }
                    | Shape::Circle { color } => *color = Expr::Literal(Value::Color(*v)),
                    _ => return Err("OIB color target is unsupported".into()),
                },
                (
                    Property::Width
                    | Property::Y
                    | Property::X
                    | Property::Height
                    | Property::FrameWidth,
                    Value::Number(v),
                ) => {
                    let index = match target.property {
                        Property::X => 0,
                        Property::Y => 1,
                        Property::Width | Property::FrameWidth => 2,
                        _ => 3,
                    };
                    let v = v * target.scale + target.offset;
                    if !v.is_finite()
                        || (target.property == Property::Width && v < 240.)
                        || (index >= 2 && v < 1.)
                    {
                        return Err("Invalid OIB geometry".into());
                    }
                    node.frame[index] = Expr::Literal(Value::Number(v));
                }
                _ => return Err("OIB property target type mismatch".into()),
            }
        }
    }
    for cell in &seed.cells {
        for target in cell.targets.iter().filter(|t| {
            t.property == Property::OffsetY
                && outputs.get(&document::output_key(t)) == Some(&cell.id)
        }) {
            let Value::Number(value) = effective(seed, values, cell.id)? else {
                return Err("OIB offset needs a number".into());
            };
            let node = blueprint
                .templates
                .iter_mut()
                .find_map(|t| node(&mut t.root, target.node))
                .ok_or("Missing OIB offset target")?;
            node.frame[1] = Expr::Add(
                Box::new(node.frame[1].clone()),
                Box::new(Expr::Literal(Value::Number(
                    value * target.scale + target.offset,
                ))),
            );
        }
    }
    let layout = Layout::from_blueprint(blueprint)?;
    let mut slots = BTreeMap::new();
    for selector in &ui.selectors {
        let field = ui
            .state_schema
            .fields
            .iter()
            .find(|f| f.id == selector.field);
        let value = match &selector.projection {
            Projection::OibStatus | Projection::OibCell { .. } => Value::Text("Live UI".into()),
            Projection::Percent => Value::Text("100%".into()),
            Projection::Number { scale, offset } => match field.map(|f| &f.default) {
                Some(Value::Number(n)) => Value::Number(n * scale + offset),
                _ => return Err("OIB numeric selector needs a numeric field".into()),
            },
            Projection::Identity => field.ok_or("OIB selector needs a field")?.default.clone(),
            Projection::Nonzero => match field.map(|f| &f.default) {
                Some(Value::Number(n)) => Value::Boolean(*n != 0.),
                _ => return Err("OIB nonzero selector needs a numeric field".into()),
            },
            Projection::ChooseText { zero, nonzero } => match field.map(|f| &f.default) {
                Some(Value::Number(n)) => {
                    Value::Text(if *n == 0. { zero } else { nonzero }.clone())
                }
                _ => return Err("OIB toggle selector needs a numeric field".into()),
            },
            Projection::ChooseColor { zero, nonzero } => match field.map(|f| &f.default) {
                Some(Value::Number(n)) => Value::Color(if *n == 0. { *zero } else { *nonzero }),
                _ => return Err("OIB toggle selector needs a numeric field".into()),
            },
            _ => return Err("Unsupported O1 selector projection".into()),
        };
        slots.insert(selector.local, value);
    }
    for viewport in [[1280., 800.], [800., 1280.]] {
        let scene = layout.resolve_ordered_with_font(
            &slots,
            viewport,
            &[],
            Some(crate::dashboard::font::admission_font()?),
        )?;
        if let Some(surface) = &seed.instance_surface
            && (surface.layout.is_some()
                || seed.graph.is_some()
                || seed
                    .designer
                    .as_ref()
                    .is_some_and(|d| !d.placements.is_empty()))
        {
            instances::validate_row_geometry(&scene, surface.preview_viewport)?;
        }
    }
    Ok(layout)
}
