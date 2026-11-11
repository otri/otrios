// SPDX-License-Identifier: LGPL-3.0-only
//! Real Auto Layout controls over generation-guarded canonical visual edits.
use super::*;
use otrios_interfaces::{
    layout::{
        AutoLayout, CrossAlignment, Direction, NodeLayout, Packing, Placement, Shape, Sizing,
    },
    oib::{
        authoring::{DefinitionBatch, DefinitionEdit},
        designer::{DefinitionId, NodeId, StructuralProperty as P, StructuralTarget},
        graph,
    },
};

#[derive(Clone)]
pub(super) struct Target {
    definition: DefinitionId,
    node: NodeId,
    visual: graph::Visual,
    generation: u64,
    reference: bool,
}
impl Target {
    fn operation(
        &self,
        change: impl FnOnce(&mut graph::Visual),
    ) -> otrios_interfaces::oib::InstanceOperation {
        let mut visual = self.visual.clone();
        change(&mut visual);
        // Explicit Auto Layout replaces the legacy node-relative resize policy.
        if visual.layout.is_some() {
            visual.resize = None;
        }
        otrios_interfaces::oib::InstanceOperation::definitions(DefinitionBatch {
            generation: self.generation,
            operations: vec![DefinitionEdit::SetVisual {
                definition: self.definition,
                node: self.node,
                visual: Some(visual),
            }],
        })
    }
    fn policy(&self) -> NodeLayout {
        self.visual.layout.unwrap_or_default()
    }
    fn action(&self, change: impl FnOnce(&mut NodeLayout)) -> LocalAction {
        LocalAction::Structure(
            self.operation(|visual| change(visual.layout.get_or_insert_with(Default::default))),
        )
    }
}

pub(super) fn target(
    seed: &otrios_interfaces::oib::Seed,
    properties: &[PropertyInfo],
    generation: u64,
) -> Option<Target> {
    let (definition, node) = properties.iter().find_map(|p| match &p.address {
        Some(PropertyAddress::Structural {
            target: StructuralTarget::Node { definition, node },
            ..
        }) => Some((*definition, *node)),
        _ => None,
    })?;
    let declaration = seed
        .graph
        .as_ref()?
        .definitions
        .iter()
        .find(|d| d.id == definition)?
        .nodes
        .iter()
        .find(|n| n.id == node)?;
    let visual = declaration.visual.clone()?;
    let reference = declaration.reference.is_some();
    Some(Target {
        definition,
        node,
        visual,
        generation,
        reference,
    })
}
fn property(info: &PropertyInfo) -> Option<P> {
    match &info.address {
        Some(PropertyAddress::Structural { property, .. }) => Some(*property),
        _ => None,
    }
}
pub(super) fn numeric(property: P) -> bool {
    matches!(
        property,
        P::X | P::Y
            | P::Width
            | P::Height
            | P::LayoutGap
            | P::Padding
            | P::PaddingLeft
            | P::PaddingTop
            | P::PaddingRight
            | P::PaddingBottom
            | P::MinWidth
            | P::MinHeight
            | P::MaxWidth
            | P::MaxHeight
    )
}
fn represented(property: P, target: &Target, bounds: bool, separate: bool) -> bool {
    let layout = target.policy();
    match property {
        P::X | P::Y => layout.placement == Placement::Free || layout.container.is_none(),
        P::Width => layout.width.mode == Sizing::Fixed,
        P::Height => layout.height.mode == Sizing::Fixed,
        P::LayoutGap => layout.container.is_some(),
        P::Padding => layout.container.is_some() && !separate,
        P::PaddingLeft | P::PaddingTop | P::PaddingRight | P::PaddingBottom => {
            layout.container.is_some() && separate
        }
        P::MinWidth | P::MinHeight | P::MaxWidth | P::MaxHeight => bounds,
        _ => true,
    }
}
pub(super) fn separate(target: &Target, preference: bool) -> bool {
    preference
        || target
            .policy()
            .container
            .is_some_and(|flow| flow.padding.iter().any(|v| *v != flow.padding[0]))
}
pub(super) fn visible(
    properties: &[PropertyInfo],
    target: Option<&Target>,
    bounds: bool,
    separate: bool,
) -> Vec<PropertyInfo> {
    properties
        .iter()
        .filter(|p| match target {
            Some(t) => property(p).is_none_or(|p| represented(p, t, bounds, separate)),
            None => property(p)
                .is_none_or(|p| !numeric(p) || matches!(p, P::X | P::Y | P::Width | P::Height)),
        })
        .cloned()
        .collect()
}

fn selected(b: &mut Builder, node: &mut SceneNode, value: bool) -> Result<()> {
    if let Some(semantics) = &mut node.semantics {
        semantics.checked = value;
    }
    if value {
        style::set_surface_fill(&mut node.children[0], b.palette.selected)?;
        if let Primitive::Text { color, .. } = &mut node.children[1].primitive {
            *color = b.palette.background;
        }
    }
    Ok(())
}
fn button(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    frame: Rect,
    label: &str,
    semantic: &str,
    action: LocalAction,
    checked: bool,
) -> Result<()> {
    let mut node = b.focus_button(frame.x, frame.y, frame.width, label, action)?;
    node.frame.height = frame.height;
    style::resize_surface(&mut node.children[0], frame.width, frame.height)?;
    node.children[1].frame.y = (frame.height - 28.) / 2.;
    if let Some(s) = &mut node.semantics {
        s.label = semantic.into();
    }
    selected(b, &mut node, checked)?;
    nodes.push(node);
    Ok(())
}
fn heading(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    x: f32,
    y: f32,
    width: f32,
    label: &str,
) -> Result<()> {
    let mut node = b.label(x, y, label, MUTED)?;
    node.frame.width = width;
    if let Primitive::Text { scale, .. } = &mut node.primitive {
        *scale = 2;
    }
    nodes.push(node);
    Ok(())
}
struct Fields<'a> {
    drafts: &'a mut PropertyDrafts,
    properties: &'a [PropertyInfo],
    revision: u64,
    saving: bool,
}
fn input(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    fields: &mut Fields<'_>,
    property: P,
    frame: Rect,
    label: &str,
) -> Result<()> {
    let Some(info) = fields
        .properties
        .iter()
        .find(|info| self::property(info) == Some(property))
    else {
        return Ok(());
    };
    let address = info
        .address
        .as_ref()
        .ok_or("Missing layout field address")?;
    if !(fields.saving && fields.drafts.draft(address).is_some())
        && fields.drafts.open(info, fields.revision).is_err()
    {
        return Ok(());
    }
    let draft = fields.drafts.draft(address).ok_or("Missing layout draft")?;
    let Some(field) = draft.fields.first() else {
        return Ok(());
    };
    let caption = if draft.error.is_some() {
        format!("{label} · invalid")
    } else {
        label.to_owned()
    };
    heading(b, nodes, frame.x, frame.y, frame.width, &caption)?;
    let mut node = b.focus_button(
        frame.x,
        frame.y + 28.,
        frame.width,
        field.buffer().text(),
        LocalAction::FocusField(field.id),
    )?;
    b.actions.remove(&node.id);
    node.id = field.id;
    node.semantics = Some(SwitchSemantics {
        label: info.descriptor.label.clone(),
        checked: false,
    });
    b.actions
        .insert(field.id, LocalAction::FocusField(field.id));
    style::set_surface_fill(&mut node.children[0], b.palette.background)?;
    nodes.push(node);
    Ok(())
}
fn modes(
    b: &mut Builder,
    nodes: &mut Vec<SceneNode>,
    target: &Target,
    y: f32,
    axis: usize,
) -> Result<()> {
    let current = if axis == 0 {
        target.policy().width.mode
    } else {
        target.policy().height.mode
    };
    heading(
        b,
        nodes,
        0.,
        y,
        320.,
        if axis == 0 { "Width" } else { "Height" },
    )?;
    for (index, (mode, label)) in [
        (Sizing::Fixed, "Fixed"),
        (Sizing::Hug, "Hug"),
        (Sizing::Fill, "Fill"),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            b,
            nodes,
            Rect::new(index as f32 * 108., y + 28., 104., 48.),
            label,
            &format!("{} {label}", if axis == 0 { "Width" } else { "Height" }),
            if current == mode {
                LocalAction::LayoutNoop
            } else {
                target.action(|layout| {
                    if axis == 0 {
                        layout.width.mode = mode
                    } else {
                        layout.height.mode = mode
                    }
                })
            },
            current == mode,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build(
    b: &mut Builder,
    drafts: &mut PropertyDrafts,
    properties: &[PropertyInfo],
    target: &Target,
    y: &mut f32,
    bounds: bool,
    independent: bool,
    saving: bool,
) -> Result<Vec<SceneNode>> {
    let mut fields = Fields {
        drafts,
        properties,
        revision: target.generation,
        saving,
    };
    let mut nodes = vec![];
    let layout = target.policy();
    modes(b, &mut nodes, target, *y, 0)?;
    *y += 84.;
    modes(b, &mut nodes, target, *y, 1)?;
    *y += 84.;
    if layout.width.mode == Sizing::Fixed || layout.height.mode == Sizing::Fixed {
        if layout.width.mode == Sizing::Fixed {
            input(
                b,
                &mut nodes,
                &mut fields,
                P::Width,
                Rect::new(0., *y, 156., 80.),
                "W",
            )?;
        }
        if layout.height.mode == Sizing::Fixed {
            input(
                b,
                &mut nodes,
                &mut fields,
                P::Height,
                Rect::new(164., *y, 156., 80.),
                "H",
            )?;
        }
        *y += 88.;
    }
    button(
        b,
        &mut nodes,
        Rect::new(0., *y, 320., 48.),
        if bounds {
            "Hide size bounds"
        } else {
            "Size bounds"
        },
        "Size bounds",
        LocalAction::LayoutBounds(!bounds),
        bounds,
    )?;
    *y += 56.;
    if bounds {
        for (left, right, a, z) in [
            (P::MinWidth, P::MaxWidth, "Min W", "Max W"),
            (P::MinHeight, P::MaxHeight, "Min H", "Max H"),
        ] {
            input(
                b,
                &mut nodes,
                &mut fields,
                left,
                Rect::new(0., *y, 156., 80.),
                a,
            )?;
            input(
                b,
                &mut nodes,
                &mut fields,
                right,
                Rect::new(164., *y, 156., 80.),
                z,
            )?;
            *y += 88.;
        }
    }
    heading(b, &mut nodes, 0., *y, 320., "Position")?;
    *y += 28.;
    for (index, (placement, label)) in [(Placement::InFlow, "In flow"), (Placement::Free, "Free")]
        .into_iter()
        .enumerate()
    {
        button(
            b,
            &mut nodes,
            Rect::new(index as f32 * 164., *y, 156., 48.),
            label,
            label,
            if layout.placement == placement {
                LocalAction::LayoutNoop
            } else {
                target.action(|l| l.placement = placement)
            },
            layout.placement == placement,
        )?;
    }
    *y += 56.;
    if layout.placement == Placement::Free || layout.container.is_none() {
        input(
            b,
            &mut nodes,
            &mut fields,
            P::X,
            Rect::new(0., *y, 156., 80.),
            "X",
        )?;
        input(
            b,
            &mut nodes,
            &mut fields,
            P::Y,
            Rect::new(164., *y, 156., 80.),
            "Y",
        )?;
        *y += 88.;
    }
    if !target.reference && matches!(target.visual.shape, Shape::Canvas { .. }) {
        heading(b, &mut nodes, 0., *y, 320., "Auto layout")?;
        *y += 28.;
        for (index, (flow, label)) in [
            (None, "Free"),
            (Some(Direction::Horizontal), "Row"),
            (Some(Direction::Vertical), "Column"),
        ]
        .into_iter()
        .enumerate()
        {
            button(
                b,
                &mut nodes,
                Rect::new(index as f32 * 108., *y, 104., 48.),
                label,
                &format!("Flow {label}"),
                if layout.container.map(|f| f.direction) == flow {
                    LocalAction::LayoutNoop
                } else {
                    target.action(|l| {
                        l.container = flow.map(|direction| AutoLayout {
                            direction,
                            ..l.container.unwrap_or_default()
                        })
                    })
                },
                layout.container.map(|f| f.direction) == flow,
            )?;
        }
        *y += 56.;
    }
    if let Some(flow) = layout.container {
        let top = *y;
        heading(b, &mut nodes, 0., top, 152., "Alignment")?;
        let main = match flow.packing {
            Packing::Start | Packing::SpaceBetween => 0,
            Packing::Center => 1,
            Packing::End => 2,
        };
        let cross = match flow.alignment {
            CrossAlignment::Start | CrossAlignment::Stretch => 0,
            CrossAlignment::Center => 1,
            CrossAlignment::End => 2,
        };
        let (current_x, current_y) = if flow.direction == Direction::Horizontal {
            (main, cross)
        } else {
            (cross, main)
        };
        for row in 0..3 {
            for col in 0..3 {
                let x = col as f32 * 52.;
                let y = top + 28. + row as f32 * 52.;
                let (main, cross) = if flow.direction == Direction::Horizontal {
                    (col, row)
                } else {
                    (row, col)
                };
                let action = target.action(|l| {
                    let f = l.container.as_mut().unwrap();
                    f.packing = [Packing::Start, Packing::Center, Packing::End][main];
                    f.alignment = [
                        CrossAlignment::Start,
                        CrossAlignment::Center,
                        CrossAlignment::End,
                    ][cross];
                });
                button(
                    b,
                    &mut nodes,
                    Rect::new(x, y, 48., 48.),
                    "",
                    &format!(
                        "Align {} {}",
                        ["top", "center", "bottom"][row],
                        ["left", "center", "right"][col]
                    ),
                    action,
                    current_x == col && current_y == row,
                )?;
                let node = nodes.last_mut().unwrap();
                for (i, width) in [10., 16., 12.].into_iter().enumerate() {
                    let dx = [8., (48. - width) / 2., 40. - width][col];
                    let dy = [8., 17., 26.][row] + i as f32 * 4.;
                    let mut line = background(
                        b.id()?,
                        width,
                        2.,
                        if current_x == col && current_y == row {
                            b.palette.background
                        } else {
                            b.palette.text
                        },
                        0.,
                    );
                    line.frame.x = dx;
                    line.frame.y = dy;
                    line.responder = Some(TouchResponder::PassThrough);
                    node.children.push(line);
                }
            }
        }
        input(
            b,
            &mut nodes,
            &mut fields,
            P::LayoutGap,
            Rect::new(164., top, 156., 80.),
            "Gap",
        )?;
        let normalize = (independent && flow.padding.iter().any(|value| *value != flow.padding[0]))
            .then(|| {
                target.operation(|v| {
                    v.layout
                        .as_mut()
                        .unwrap()
                        .container
                        .as_mut()
                        .unwrap()
                        .padding = [flow.padding[0]; 4]
                })
            });
        button(
            b,
            &mut nodes,
            Rect::new(0., top + 196., 152., 48.),
            if independent {
                "Link sides"
            } else {
                "Split sides"
            },
            "Independent padding",
            LocalAction::LayoutPadding(!independent, normalize),
            independent,
        )?;
        let distribution_y = if independent {
            for (index, (left, right, label_left, label_right)) in [
                (P::PaddingLeft, P::PaddingRight, "Left", "Right"),
                (P::PaddingTop, P::PaddingBottom, "Top", "Bottom"),
            ]
            .into_iter()
            .enumerate()
            {
                let field_y = top + 88. + index as f32 * 88.;
                input(
                    b,
                    &mut nodes,
                    &mut fields,
                    left,
                    Rect::new(164., field_y, 74., 80.),
                    label_left,
                )?;
                input(
                    b,
                    &mut nodes,
                    &mut fields,
                    right,
                    Rect::new(246., field_y, 74., 80.),
                    label_right,
                )?;
            }
            top + 264.
        } else {
            input(
                b,
                &mut nodes,
                &mut fields,
                P::Padding,
                Rect::new(164., top + 88., 156., 80.),
                "Padding",
            )?;
            top + 176.
        };
        heading(b, &mut nodes, 164., distribution_y, 156., "Distribution")?;
        button(
            b,
            &mut nodes,
            Rect::new(164., distribution_y + 28., 156., 48.),
            if flow.packing == Packing::SpaceBetween {
                "Space between"
            } else {
                "Packed"
            },
            "Space between",
            target.action(|l| {
                l.container.as_mut().unwrap().packing = if flow.packing == Packing::SpaceBetween {
                    Packing::Start
                } else {
                    Packing::SpaceBetween
                }
            }),
            flow.packing == Packing::SpaceBetween,
        )?;
        *y = (distribution_y + 88.).max(top + 252.);
    }
    if !target.reference
        && let Shape::Canvas { clip } = target.visual.shape
    {
        button(
            b,
            &mut nodes,
            Rect::new(0., *y, 320., 48.),
            "Clip content",
            "Clip content",
            LocalAction::Structure(
                target.operation(|visual| visual.shape = Shape::Canvas { clip: !clip }),
            ),
            false,
        )?;
        let row = nodes.last_mut().unwrap();
        row.semantics.as_mut().unwrap().checked = clip;
        row.children[1].frame.x = 36.;
        row.children[1].frame.width = 276.;
        let mut outer = background(b.id()?, 20., 20., b.palette.line, 3.);
        outer.frame.x = 8.;
        outer.frame.y = 14.;
        outer.responder = Some(TouchResponder::PassThrough);
        let mut inner = background(b.id()?, 17., 17., b.palette.surface, 2.);
        inner.frame.x = 9.5;
        inner.frame.y = 15.5;
        inner.responder = Some(TouchResponder::PassThrough);
        row.children.extend([outer, inner]);
        if clip {
            row.children.push(SceneNode {
                id: b.id()?,
                frame: Rect::new(12., 18., 12., 12.),
                primitive: Primitive::Path {
                    points: vec![[0., 6.], [4., 10.], [12., 0.]],
                    width: 2.,
                    color: b.palette.text,
                },
                children: vec![],
                responder: Some(TouchResponder::PassThrough),
                semantics: None,
            });
        }
        *y += 64.;
    }
    Ok(nodes)
}
