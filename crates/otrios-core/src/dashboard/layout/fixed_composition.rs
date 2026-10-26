// SPDX-License-Identifier: LGPL-3.0-only
//! Checked Q32 flow allocation, with one conversion at the public frame boundary.
use crate::dashboard::{Result, fixed::Scalar as Q};
use otrios_interfaces::layout::*;
use std::collections::BTreeMap;
type Size = [Q; 2];
type Measurement = (Size, Size);
fn q(v: f32) -> Result<Q> {
    Q::from_f32(v)
}
fn count(n: usize) -> Result<Q> {
    q(n as f32)
}
fn measure(flow: &Flow) -> Result<Measurement> {
    match &flow.kind {
        FlowKind::Wrap {
            preferred, minimum, ..
        } => Ok((
            [q(minimum[0])?, q(minimum[1])?],
            [q(preferred[0])?, q(preferred[1])?],
        )),
        FlowKind::Item {
            preferred, minimum, ..
        } => {
            let hi = [q(preferred[0])?, q(preferred[1])?];
            let mut lo = hi;
            for a in 0..2 {
                if flow.flexible[a] {
                    lo[a] = q(minimum[a])?;
                }
            }
            Ok((lo, hi))
        }
        FlowKind::Group {
            axis,
            gap,
            children,
            ..
        } => {
            let a = usize::from(*axis);
            let b = 1 - a;
            let mut lo = [Q::ZERO; 2];
            let mut hi = lo;
            for child in children {
                let (l, h) = measure(child)?;
                lo[a] = lo[a].checked_add(l[a])?;
                hi[a] = hi[a].checked_add(h[a])?;
                lo[b] = lo[b].max(l[b]);
                hi[b] = hi[b].max(h[b]);
            }
            let gaps = q(*gap)?.checked_mul(count(children.len() - 1)?)?;
            lo[a] = lo[a].checked_add(gaps)?;
            hi[a] = hi[a].checked_add(gaps)?;
            for a in 0..2 {
                if !flow.flexible[a] {
                    lo[a] = hi[a];
                }
            }
            Ok((lo, hi))
        }
    }
}
fn columns(width: Q, minimum: f32, gap: f32, n: usize) -> Result<usize> {
    let numerator = width.checked_add(q(gap)?)?.raw();
    let denominator = q(minimum)?.checked_add(q(gap)?)?.raw();
    if denominator <= 0 || numerator < 0 {
        return Err("Invalid fixed wrap dimensions".into());
    }
    // Floor the exact ratio, rather than rounding it onto another grid first.
    Ok(usize::try_from(numerator / denominator)
        .map_err(|_| "Wrap count overflow")?
        .clamp(1, n))
}
fn measure_at(flow: &Flow, width: Q) -> Result<Measurement> {
    match &flow.kind {
        FlowKind::Wrap {
            nodes,
            preferred,
            minimum,
            gap,
        } => {
            let cols = columns(width, minimum[0], gap[0], nodes.len())?;
            let rows = nodes.len().div_ceil(cols);
            let height = q(preferred[1])?
                .checked_mul(count(rows)?)?
                .checked_add(q(gap[1])?.checked_mul(count(rows - 1)?)?)?;
            Ok(([q(minimum[0])?, height], [width, height]))
        }
        FlowKind::Group {
            axis: 1,
            gap,
            children,
            ..
        } => {
            let mut lo = [Q::ZERO; 2];
            let mut hi = lo;
            for child in children {
                let (l, h) = measure_at(child, width)?;
                lo[0] = lo[0].max(l[0]);
                hi[0] = hi[0].max(h[0]);
                lo[1] = lo[1].checked_add(l[1])?;
                hi[1] = hi[1].checked_add(h[1])?;
            }
            let gaps = q(*gap)?.checked_mul(count(children.len() - 1)?)?;
            lo[1] = lo[1].checked_add(gaps)?;
            hi[1] = hi[1].checked_add(gaps)?;
            Ok((lo, hi))
        }
        _ => measure(flow),
    }
}
pub(super) fn resolve(
    composition: &Composition,
    size: [f32; 2],
) -> Result<BTreeMap<u16, [f32; 4]>> {
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
    let frame = [
        q(left)?,
        q(top)?,
        q(size[0])?.checked_sub(q(left)?)?.checked_sub(q(right)?)?,
        q(size[1])?.checked_sub(q(top)?)?.checked_sub(q(bottom)?)?,
    ];
    let (minimum, _) = measure_at(&plan.content, frame[2])?;
    for a in 0..2 {
        if frame[a + 2].checked_add(q(0.01)?)? < minimum[a] {
            return Err("Viewport below fixed composition minimum".into());
        }
    }
    let mut out = BTreeMap::new();
    place(&plan.content, frame, &mut out)?;
    Ok(out
        .into_iter()
        .map(|(id, f)| (id, f.map(Q::to_f32)))
        .collect())
}
fn place(flow: &Flow, frame: [Q; 4], out: &mut BTreeMap<u16, [Q; 4]>) -> Result<()> {
    let tolerance = q(0.01)?;
    match &flow.kind {
        FlowKind::Wrap {
            nodes,
            minimum,
            preferred,
            gap,
        } => {
            let cols = columns(frame[2], minimum[0], gap[0], nodes.len())?;
            let width = frame[2]
                .checked_sub(q(gap[0])?.checked_mul(count(cols - 1)?)?)?
                .checked_div_int(cols as i64)?;
            let rows = nodes.len().div_ceil(cols);
            let height = count(rows)?
                .checked_mul(q(preferred[1])?)?
                .checked_add(count(rows - 1)?.checked_mul(q(gap[1])?)?)?;
            if width.checked_add(tolerance)? < q(minimum[0])?
                || height > frame[3].checked_add(tolerance)?
            {
                return Err("Fixed wrapping flow does not fit".into());
            }
            for (i, node) in nodes.iter().enumerate() {
                out.insert(
                    *node,
                    [
                        frame[0].checked_add(
                            count(i % cols)?.checked_mul(width.checked_add(q(gap[0])?)?)?,
                        )?,
                        frame[1].checked_add(
                            count(i / cols)?
                                .checked_mul(q(preferred[1])?.checked_add(q(gap[1])?)?)?,
                        )?,
                        width,
                        q(preferred[1])?,
                    ],
                );
            }
        }
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
            let measured = children
                .iter()
                .map(|c| {
                    if a == 1 {
                        measure_at(c, frame[2])
                    } else {
                        measure(c)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let available =
                frame[a + 2].checked_sub(q(*gap)?.checked_mul(count(children.len() - 1)?)?)?;
            let mut preferred = Q::ZERO;
            for (_, hi) in &measured {
                preferred = preferred.checked_add(hi[a])?;
            }
            let delta = available.checked_sub(preferred)?;
            let mut capacity = Q::ZERO;
            for (child, (lo, hi)) in children.iter().zip(&measured) {
                if child.flexible[a] {
                    capacity = capacity.checked_add(if delta < Q::ZERO {
                        hi[a].checked_sub(lo[a])?
                    } else {
                        Q::ONE
                    })?;
                }
            }
            let mut position = frame[a];
            for (child, (lo, hi)) in children.iter().zip(measured) {
                let mut actual = [frame[0], frame[1], hi[0], hi[1]];
                actual[a] = position;
                if child.flexible[a] && capacity > Q::ZERO {
                    let share = if delta < Q::ZERO {
                        hi[a].checked_sub(lo[a])?
                    } else {
                        Q::ONE
                    };
                    actual[a + 2] =
                        actual[a + 2].checked_add(delta.checked_mul_ratio(share, capacity)?)?;
                }
                if child.flexible[b] {
                    actual[b + 2] = frame[b + 2];
                }
                if actual[a + 2].checked_add(tolerance)? < lo[a]
                    || actual[b + 2].checked_add(tolerance)? < lo[b]
                    || actual[b + 2] > frame[b + 2].checked_add(tolerance)?
                {
                    return Err("Component does not fit its fixed frame".into());
                }
                let spare = frame[b + 2].checked_sub(actual[b + 2])?;
                actual[b] = actual[b].checked_add(match alignment {
                    Alignment::Start => Q::ZERO,
                    Alignment::Center => spare.checked_div_int(2)?,
                    Alignment::End => spare,
                })?;
                place(child, actual, out)?;
                position = position.checked_add(actual[a + 2])?.checked_add(q(*gap)?)?;
            }
        }
    }
    Ok(())
}
