// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded bottom-up measure and top-down allocation for reusable node layout.
//! This runs before Scene/resource admission; it never iterates visual guesses.
use super::*;
use crate::dashboard::font::Font;

const LIMIT: f32 = 8192.;
fn failure(id: u16, message: &str) -> String {
    format!("Auto Layout node {id}: {message}")
}
fn checked(value: f32, id: u16) -> Result<f32> {
    if value.is_finite() && (0. ..=LIMIT).contains(&value) {
        Ok(value)
    } else {
        Err(failure(id, "length exceeds the finite coordinate budget"))
    }
}
fn axis(policy: NodeLayout, axis: usize) -> AxisSizing {
    if axis == 0 {
        policy.width
    } else {
        policy.height
    }
}
fn clamp(value: f32, sizing: AxisSizing, id: u16) -> Result<f32> {
    if !value.is_finite() || value < 0. {
        return Err(failure(id, "invalid measured length"));
    }
    checked(value.max(sizing.min).min(sizing.max.unwrap_or(LIMIT)), id)
}
/// Shared by renderer admission and canonical graph source admission.
pub fn validate_policy(policy: &NodeLayout) -> Result<()> {
    for sizing in [policy.width, policy.height] {
        if !sizing.min.is_finite()
            || !(0. ..=LIMIT).contains(&sizing.min)
            || sizing
                .max
                .is_some_and(|max| !max.is_finite() || max < sizing.min || max > LIMIT)
        {
            return Err("Invalid Auto Layout minimum/maximum".into());
        }
    }
    if let Some(container) = policy.container
        && container
            .padding
            .iter()
            .chain([&container.gap])
            .any(|v| !v.is_finite() || !(0. ..=LIMIT).contains(v))
    {
        return Err("Invalid Auto Layout padding/gap".into());
    }
    Ok(())
}
/// A dependency proof is checked even before a font or state values are present.
/// Free children do not contribute to Hug, and can use its completed allocation.
pub(super) fn validate_tree(node: &TemplateNode, path: &mut Vec<u16>) -> Result<()> {
    path.push(node.id);
    if let Some(policy) = node.layout {
        validate_policy(&policy)?;
        if node.resize.is_some() {
            return Err(failure(node.id, "layout and legacy resize cannot coexist"));
        }
        if policy.container.is_some() && !matches!(node.shape, Shape::Canvas { .. }) {
            return Err(failure(node.id, "only a Canvas can own flow"));
        }
        for child in &node.children {
            let child_policy = child.layout.unwrap_or_default();
            if child_policy.placement == Placement::Free {
                continue;
            }
            for a in 0..2 {
                let legacy_dependency = child.resize.is_some_and(|rule| {
                    let rule = if a == 0 {
                        rule.horizontal
                    } else {
                        rule.vertical
                    };
                    if policy.container.is_none() {
                        return rule != AxisResize::Fixed;
                    }
                    matches!(
                        rule,
                        AxisResize::Pins {
                            leading: Some(_),
                            trailing: Some(_)
                        } | AxisResize::Springs { length: true, .. }
                    )
                });
                if axis(policy, a).mode == Sizing::Hug
                    && (axis(child_policy, a).mode == Sizing::Fill || legacy_dependency)
                {
                    return Err(format!(
                        "Auto Layout dependency {:?} -> {}: {} hugs children but child fills it; use Fixed/Hug on child or Fixed/Fill on parent",
                        path,
                        child.id,
                        if a == 0 { "width" } else { "height" }
                    ));
                }
            }
        }
    }
    for child in &node.children {
        validate_tree(child, path)?;
    }
    path.pop();
    Ok(())
}
fn visible(node: &TemplateNode, evaluation: &NodeEvaluation<'_, '_>) -> Result<bool> {
    boolean(evaluation.run(&node.visible)?)
}
fn measured(
    node: &TemplateNode,
    evaluation: &NodeEvaluation<'_, '_>,
    available: [Option<f32>; 2],
    root: bool,
    schema: &str,
    frames: Option<&BTreeMap<u16, [f32; 4]>>,
    font: Option<&Font>,
) -> Result<[f32; 2]> {
    let policy = node.layout.unwrap_or_default();
    let base = evaluation.frame(
        node,
        available.map(|n| n.unwrap_or(0.)),
        root,
        schema,
        frames,
    )?;
    let authored = [base.width, base.height];
    let mut size = [None; 2];
    for a in 0..2 {
        size[a] = match axis(policy, a).mode {
            Sizing::Fixed => Some(clamp(authored[a], axis(policy, a), node.id)?),
            Sizing::Fill => Some(clamp(
                available[a].ok_or_else(|| {
                    failure(
                        node.id,
                        if a == 0 {
                            "width fills an indefinite Hug ancestor"
                        } else {
                            "height fills an indefinite Hug ancestor"
                        },
                    )
                })?,
                axis(policy, a),
                node.id,
            )?),
            Sizing::Hug => None,
        };
    }
    if size.iter().all(Option::is_some) {
        return Ok(size.map(Option::unwrap));
    }
    let mut intrinsic = authored;
    if let Shape::Text { value, scale, .. } = &node.shape {
        let value = text(evaluation.run(value)?)?;
        if value.contains(['\n', '\r']) {
            return Err(failure(
                node.id,
                "multiline Hug requires an admitted wrapping text painter",
            ));
        }
        let scale = number(evaluation.run(scale)?)?;
        let font =
            font.ok_or_else(|| failure(node.id, "Hug text requires the admitted font metrics"))?;
        intrinsic = [
            font.text_width(&value, scale),
            font.metrics().line_height(scale),
        ];
    } else if matches!(node.shape, Shape::Canvas { .. }) {
        intrinsic = [0., 0.];
        let flow = policy.container;
        let padding = flow.map_or([0.; 4], |flow| flow.padding);
        let inset = [padding[0] + padding[2], padding[1] + padding[3]];
        let inner = [0, 1].map(|a| size[a].map(|value| (value - inset[a]).max(0.)));
        let mut count: usize = 0;
        for child in &node.children {
            if !visible(child, evaluation)?
                || child.layout.unwrap_or_default().placement == Placement::Free
            {
                continue;
            }
            let child_size = measured(child, evaluation, inner, false, schema, None, font)?;
            if let Some(flow) = flow {
                let main = usize::from(flow.direction == Direction::Vertical);
                intrinsic[main] += child_size[main];
                intrinsic[1 - main] = intrinsic[1 - main].max(child_size[1 - main]);
            } else {
                let frame =
                    evaluation.frame(child, inner.map(|n| n.unwrap_or(0.)), false, schema, None)?;
                intrinsic[0] = intrinsic[0].max(frame.x + child_size[0]);
                intrinsic[1] = intrinsic[1].max(frame.y + child_size[1]);
            }
            count += 1;
        }
        if let Some(flow) = flow {
            let main = usize::from(flow.direction == Direction::Vertical);
            intrinsic[main] += flow.gap * count.saturating_sub(1) as f32;
        }
        for a in 0..2 {
            intrinsic[a] += inset[a];
        }
    }
    for a in 0..2 {
        if size[a].is_none() {
            size[a] = Some(clamp(intrinsic[a], axis(policy, a), node.id)?);
        }
    }
    Ok(size.map(Option::unwrap))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn frame(
    node: &TemplateNode,
    evaluation: &NodeEvaluation<'_, '_>,
    parent: [f32; 2],
    root: bool,
    schema: &str,
    frames: Option<&BTreeMap<u16, [f32; 4]>>,
    font: Option<&Font>,
) -> Result<Rect> {
    if node.reference_root {
        return Ok(Rect::new(0., 0., parent[0], parent[1]));
    }
    let mut frame = evaluation.frame(node, parent, root, schema, frames)?;
    if node.layout.is_some() {
        let size = measured(
            node,
            evaluation,
            parent.map(Some),
            root,
            schema,
            frames,
            font,
        )?;
        frame.width = size[0];
        frame.height = size[1];
    }
    Ok(frame)
}
/// Freeze bounded Fill items and redistribute only the unfrozen remainder.
fn distribute(available: f32, policies: &[AxisSizing]) -> Vec<f32> {
    let minima: f32 = policies.iter().map(|p| p.min).sum();
    if minima >= available {
        return policies.iter().map(|p| p.min).collect();
    }
    let mut result = vec![0.; policies.len()];
    let mut active: Vec<_> = (0..policies.len()).collect();
    let mut remaining = available;
    while !active.is_empty() {
        let share = remaining / active.len() as f32;
        let clamped_sum: f32 = active
            .iter()
            .map(|&i| share.clamp(policies[i].min, policies[i].max.unwrap_or(LIMIT)))
            .sum();
        let mut frozen = Vec::new();
        for &i in &active {
            // Freeze only the side that moves the feasible common share in the
            // required direction. Freezing both sides can strand usable space.
            let freeze_low = clamped_sum > remaining && share < policies[i].min;
            let freeze_high = clamped_sum < remaining && share > policies[i].max.unwrap_or(LIMIT);
            if freeze_low || freeze_high {
                let value = if freeze_low {
                    policies[i].min
                } else {
                    policies[i].max.unwrap_or(LIMIT)
                };
                result[i] = value;
                remaining -= value;
                frozen.push(i);
            }
        }
        if frozen.is_empty() {
            for i in active {
                result[i] = share.clamp(policies[i].min, policies[i].max.unwrap_or(LIMIT));
            }
            break;
        }
        active.retain(|i| !frozen.contains(i));
    }
    result
}
/// Only immediate children are allocated here. Descendant legacy resize and Hug
/// measurement see these final outer dimensions when the regular walker enters.
pub(super) fn children(
    node: &TemplateNode,
    evaluation: &NodeEvaluation<'_, '_>,
    size: [f32; 2],
    schema: &str,
    font: Option<&Font>,
) -> Result<BTreeMap<u16, Rect>> {
    let Some(flow) = node.layout.and_then(|p| p.container) else {
        return Ok(BTreeMap::new());
    };
    let main = usize::from(flow.direction == Direction::Vertical);
    let cross = 1 - main;
    let padding = flow.padding;
    let inner = [
        (size[0] - padding[0] - padding[2]).max(0.),
        (size[1] - padding[1] - padding[3]).max(0.),
    ];
    let mut items = Vec::new();
    let mut free = Vec::new();
    for child in &node.children {
        if !visible(child, evaluation)? {
            continue;
        }
        let policy = child.layout.unwrap_or_default();
        let frame = evaluation.frame(child, inner, false, schema, None)?;
        let lengths = measured(
            child,
            evaluation,
            inner.map(Some),
            false,
            schema,
            None,
            font,
        )?;
        let item = (child.id, policy, frame, lengths);
        if policy.placement == Placement::Free {
            free.push(item);
        } else {
            items.push(item);
        }
    }
    let gap = flow.gap * items.len().saturating_sub(1) as f32;
    let fixed: f32 = items
        .iter()
        .filter(|(_, p, _, _)| axis(*p, main).mode != Sizing::Fill)
        .map(|(_, _, _, s)| s[main])
        .sum();
    let fill_policies: Vec<_> = items
        .iter()
        .filter(|(_, p, _, _)| axis(*p, main).mode == Sizing::Fill)
        .map(|(_, p, _, _)| axis(*p, main))
        .collect();
    let allocation = distribute((inner[main] - fixed - gap).max(0.), &fill_policies);
    let mut next_fill = allocation.iter();
    for (_, policy, _, lengths) in &mut items {
        if axis(*policy, main).mode == Sizing::Fill {
            lengths[main] = *next_fill.next().ok_or("Missing Fill allocation")?;
        }
    }
    let used: f32 = items
        .iter()
        .map(|(_, _, _, lengths)| lengths[main])
        .sum::<f32>()
        + gap;
    let spare = (inner[main] - used).max(0.);
    let extra_gap = if flow.packing == Packing::SpaceBetween && items.len() > 1 {
        spare / (items.len() - 1) as f32
    } else {
        0.
    };
    let mut cursor = match flow.packing {
        Packing::Center => spare / 2.,
        Packing::End => spare,
        _ => 0.,
    };
    let mut result = BTreeMap::new();
    for (id, _, _, lengths) in items {
        let remaining = inner[cross] - lengths[cross];
        let offset = match flow.alignment {
            CrossAlignment::Center => remaining / 2.,
            CrossAlignment::End => remaining,
            _ => 0.,
        };
        let mut position = [padding[0], padding[1]];
        position[main] += cursor;
        position[cross] += offset;
        if position
            .iter()
            .any(|value| !value.is_finite() || value.abs() > LIMIT)
        {
            return Err(failure(id, "position exceeds the finite coordinate budget"));
        }
        result.insert(
            id,
            Rect::new(position[0], position[1], lengths[0], lengths[1]),
        );
        cursor += lengths[main] + flow.gap + extra_gap;
    }
    for (id, _, frame, lengths) in free {
        result.insert(
            id,
            Rect::new(
                padding[0] + frame.x,
                padding[1] + frame.y,
                lengths[0],
                lengths[1],
            ),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
