// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded rows/columns allocate outer component frames; pins resolve their interiors.
use crate::dashboard::Result;
use otrios_interfaces::layout::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn validate(composition: &Composition, root: &TemplateNode) -> Result<()> {
    if !composition.wide_min_width.is_finite()
        || !(1. ..=8192.).contains(&composition.wide_min_width)
    {
        return Err("Invalid composition breakpoint".into());
    }
    let children: BTreeSet<_> = root.children.iter().map(|n| n.id).collect();
    let mut expected = None;
    for plan in [&composition.wide, &composition.tall] {
        let mut ids = BTreeSet::new();
        let mut budget = 128;
        fn walk(
            flow: &Flow,
            depth: usize,
            budget: &mut usize,
            ids: &mut BTreeSet<u16>,
        ) -> Result<()> {
            if depth > 8 || *budget == 0 {
                return Err("Composition depth/count exceeds limit".into());
            }
            *budget -= 1;
            match &flow.kind {
                FlowKind::Item {
                    node,
                    preferred,
                    minimum,
                } => {
                    if !ids.insert(*node)
                        || preferred
                            .iter()
                            .chain(minimum)
                            .any(|v| !v.is_finite() || !(1. ..=8192.).contains(v))
                        || (0..2).any(|a| minimum[a] > preferred[a])
                    {
                        return Err("Invalid/duplicate composition item".into());
                    }
                }
                FlowKind::Group {
                    axis,
                    gap,
                    children,
                    ..
                } => {
                    if *axis > 1
                        || !gap.is_finite()
                        || !(0. ..=512.).contains(gap)
                        || children.is_empty()
                    {
                        return Err("Invalid composition group".into());
                    }
                    for child in children {
                        walk(child, depth + 1, budget, ids)?;
                    }
                }
            }
            Ok(())
        }
        walk(&plan.content, 0, &mut budget, &mut ids)?;
        if !ids.is_subset(&children) || expected.as_ref().is_some_and(|old| old != &ids) {
            return Err(
                "Composition variants must place the same immediate component instances".into(),
            );
        }
        if plan
            .insets
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=2048.).contains(v))
        {
            return Err("Invalid composition insets".into());
        }
        let (minimum, preferred) = measure(&plan.content);
        if minimum
            .iter()
            .chain(&preferred)
            .any(|v| !v.is_finite() || *v > 8192.)
        {
            return Err("Composition size exceeds limit".into());
        }
        expected = Some(ids);
    }
    Ok(())
}

fn measure(flow: &Flow) -> ([f32; 2], [f32; 2]) {
    match &flow.kind {
        FlowKind::Item {
            preferred, minimum, ..
        } => {
            let mut min = *preferred;
            for a in 0..2 {
                if flow.flexible[a] {
                    min[a] = minimum[a];
                }
            }
            (min, *preferred)
        }
        FlowKind::Group {
            axis,
            gap,
            children,
            ..
        } => {
            let a = usize::from(*axis);
            let b = 1 - a;
            let mut min = [0_f32; 2];
            let mut pref = [0_f32; 2];
            for child in children {
                let (lo, hi) = measure(child);
                min[a] += lo[a];
                pref[a] += hi[a];
                min[b] = min[b].max(lo[b]);
                pref[b] = pref[b].max(hi[b]);
            }
            let gaps = *gap * (children.len() - 1) as f32;
            min[a] += gaps;
            pref[a] += gaps;
            if !flow.flexible[0] {
                min[0] = pref[0];
            }
            if !flow.flexible[1] {
                min[1] = pref[1];
            }
            (min, pref)
        }
    }
}

pub fn resolve(composition: &Composition, size: [f32; 2]) -> Result<BTreeMap<u16, [f32; 4]>> {
    if size
        .iter()
        .any(|v| !v.is_finite() || !(1. ..=8192.).contains(v))
    {
        return Err("Invalid composition viewport".into());
    }
    let plan = if size[0] >= composition.wide_min_width {
        &composition.wide
    } else {
        &composition.tall
    };
    let [left, top, right, bottom] = plan.insets;
    let frame = [left, top, size[0] - left - right, size[1] - top - bottom];
    let (minimum, _) = measure(&plan.content);
    if (0..2).any(|a| frame[a + 2] + 0.01 < minimum[a]) {
        return Err("Viewport is smaller than the composition's supported minimum".into());
    }
    let mut frames = BTreeMap::new();
    place(&plan.content, frame, &mut frames)?;
    Ok(frames)
}

fn place(flow: &Flow, frame: [f32; 4], out: &mut BTreeMap<u16, [f32; 4]>) -> Result<()> {
    match &flow.kind {
        FlowKind::Item { node, .. } => {
            out.insert(*node, frame);
        }
        FlowKind::Group {
            axis,
            gap,
            alignment,
            children,
        } => {
            let a = usize::from(*axis);
            let b = 1 - a;
            let measured: Vec<_> = children.iter().map(measure).collect();
            let available = frame[a + 2] - *gap * (children.len() - 1) as f32;
            let preferred: f32 = measured.iter().map(|(_, hi)| hi[a]).sum();
            let delta = available - preferred;
            let capacity: f32 = children
                .iter()
                .zip(&measured)
                .filter(|(n, _)| n.flexible[a])
                .map(|(_, (lo, hi))| if delta < 0. { hi[a] - lo[a] } else { 1. })
                .sum();
            let mut position = frame[a];
            for (child, (lo, hi)) in children.iter().zip(measured) {
                let mut actual = [frame[0], frame[1], hi[0], hi[1]];
                actual[a] = position;
                if child.flexible[a] && capacity > 0. {
                    actual[a + 2] += delta
                        * if delta < 0. {
                            (hi[a] - lo[a]) / capacity
                        } else {
                            1. / capacity
                        };
                }
                if child.flexible[b] {
                    actual[b + 2] = frame[b + 2];
                }
                if actual[a + 2] + 0.01 < lo[a]
                    || actual[b + 2] + 0.01 < lo[b]
                    || actual[b + 2] > frame[b + 2] + 0.01
                {
                    return Err("Component does not fit its allocated frame".into());
                }
                let spare = frame[b + 2] - actual[b + 2];
                actual[b] += match alignment {
                    Alignment::Start => 0.,
                    Alignment::Center => spare / 2.,
                    Alignment::End => spare,
                };
                place(child, actual, out)?;
                position += actual[a + 2] + *gap;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item(node: u16, preferred: f32, minimum: f32) -> Flow {
        Flow {
            flexible: [true, false],
            kind: FlowKind::Item {
                node,
                preferred: [preferred, 100.],
                minimum: [minimum, 100.],
            },
        }
    }
    fn plan() -> Composition {
        let flow = Flow {
            flexible: [true, false],
            kind: FlowKind::Group {
                axis: 0,
                gap: 10.,
                alignment: Alignment::Center,
                children: vec![item(1, 300., 100.), item(2, 200., 50.)],
            },
        };
        let plan = FlowPlan {
            insets: [0.; 4],
            content: flow,
        };
        Composition {
            wide_min_width: 600.,
            wide: plan.clone(),
            tall: plan,
        }
    }
    #[test]
    fn flexible_rows_share_space_without_distorting_fixed_cross_axis() {
        let p = plan();
        for width in [160., 400., 800.] {
            let frames = resolve(&p, [width, 200.]).unwrap();
            let a = frames[&1];
            let b = frames[&2];
            assert!((a[2] + b[2] + 10. - width).abs() < 0.001);
            assert_eq!(a[1], 50.);
            assert_eq!(a[3], 100.);
            assert_eq!(b[0], a[2] + 10.);
            assert!(a[2] >= 99.99 && b[2] >= 49.99);
        }
        assert!(resolve(&p, [159., 200.]).is_err());
        assert!(resolve(&p, [400., 99.]).is_err());
        assert!(resolve(&p, [f32::NAN, 200.]).is_err());
    }
    #[test]
    fn profiles_cannot_duplicate_omit_or_reference_nested_components() {
        fn node(id: u16) -> TemplateNode {
            TemplateNode {
                id,
                resize: None,
                frame: std::array::from_fn(|_| Expr::Literal(Value::Number(1.))),
                visible: Expr::Literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                responder: None,
                semantics: None,
                children: vec![],
            }
        }
        let mut root = node(0);
        root.children = vec![node(1), node(2)];
        let mut p = plan();
        validate(&p, &root).unwrap();
        p.tall.content = item(1, 300., 100.);
        assert!(validate(&p, &root).is_err());
        p = plan();
        if let FlowKind::Group { children, .. } = &mut p.wide.content.kind {
            children.push(item(1, 300., 100.));
        }
        assert!(validate(&p, &root).is_err());
        p = plan();
        root.children[1].id = 3;
        assert!(validate(&p, &root).is_err());
    }
}
