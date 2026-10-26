// SPDX-License-Identifier: LGPL-3.0-only
//! Parent-local, deterministic pins and proportional springs. No constraint graph.
use crate::dashboard::{
    Result,
    fixed::{Fixed, Scalar},
};
use otrios_interfaces::layout::{AxisResize, ResizeRule};
const LIMIT: f32 = 8192.;
pub fn validate(rule: ResizeRule) -> Result<()> {
    if rule
        .reference_size
        .iter()
        .any(|v| !v.is_finite() || *v <= 0. || *v > LIMIT)
    {
        return Err("Invalid resize reference size".into());
    }
    for axis in [rule.horizontal, rule.vertical] {
        match axis {
            AxisResize::Pins { leading, trailing } => {
                if leading.is_none() && trailing.is_none() {
                    return Err("Pins need an edge".into());
                }
                if [leading, trailing]
                    .into_iter()
                    .flatten()
                    .any(|v| !v.is_finite() || !(0. ..=LIMIT).contains(&v))
                {
                    return Err("Invalid pin inset".into());
                }
            }
            AxisResize::Center { offset } if !offset.is_finite() || offset.abs() > LIMIT => {
                return Err("Invalid center offset".into());
            }
            _ => {}
        }
    }
    Ok(())
}
pub fn resolve(frame: [f32; 4], parent: [f32; 2], rule: ResizeRule) -> Result<[f32; 4]> {
    if cfg!(feature = "fixed-ui-math") {
        resolve_fixed(frame, parent, rule)
    } else {
        resolve_with(frame, parent, rule, axis_float32)
    }
}
pub fn resolve_float32(frame: [f32; 4], parent: [f32; 2], rule: ResizeRule) -> Result<[f32; 4]> {
    resolve_with(frame, parent, rule, axis_float32)
}
/// Checked fixed point candidate. Sub-grid positive dimensions or intermediate
/// overflow use the original reference rather than narrowing accepted geometry.
pub fn resolve_fixed(frame: [f32; 4], parent: [f32; 2], rule: ResizeRule) -> Result<[f32; 4]> {
    resolve_with(frame, parent, rule, |a, b, c, d, r| {
        axis_fixed(a, b, c, d, r).or_else(|_| axis_float32(a, b, c, d, r))
    })
}
type AxisFn = fn(f32, f32, f32, f32, AxisResize) -> Result<(f32, f32)>;
fn resolve_with(
    frame: [f32; 4],
    parent: [f32; 2],
    rule: ResizeRule,
    run_axis: AxisFn,
) -> Result<[f32; 4]> {
    validate(rule)?;
    if frame.iter().any(|v| !v.is_finite() || v.abs() > LIMIT)
        || frame[2] <= 0.
        || frame[3] <= 0.
        || parent
            .iter()
            .any(|v| !v.is_finite() || *v <= 0. || *v > LIMIT)
    {
        return Err("Invalid resize geometry".into());
    }
    let (x, w) = run_axis(
        frame[0],
        frame[2],
        parent[0],
        rule.reference_size[0],
        rule.horizontal,
    )?;
    let (y, h) = run_axis(
        frame[1],
        frame[3],
        parent[1],
        rule.reference_size[1],
        rule.vertical,
    )?;
    Ok([x, y, w, h])
}
fn axis_fixed(
    start: f32,
    length: f32,
    parent: f32,
    reference: f32,
    rule: AxisResize,
) -> Result<(f32, f32)> {
    let originals = [start, length, reference];
    let start = Fixed::from_f32(start)?;
    let length = Fixed::from_f32(length)?;
    let parent = Fixed::from_f32(parent)?;
    let reference = Fixed::from_f32(reference)?;
    let (start, length) = match rule {
        AxisResize::Fixed => (start, length),
        AxisResize::Pins {
            leading: Some(a),
            trailing: Some(b),
        } => {
            let a = Fixed::from_f32(a)?;
            (a, parent.checked_sub(a)?.checked_sub(Fixed::from_f32(b)?)?)
        }
        AxisResize::Pins {
            leading: Some(a),
            trailing: None,
        } => (Fixed::from_f32(a)?, length),
        AxisResize::Pins {
            leading: None,
            trailing: Some(b),
        } => (
            parent
                .checked_sub(Fixed::from_f32(b)?)?
                .checked_sub(length)?,
            length,
        ),
        AxisResize::Pins {
            leading: None,
            trailing: None,
        } => return Err("Pins need an edge".into()),
        AxisResize::Center { offset } => (
            parent
                .checked_sub(length)?
                .checked_div_int(2)?
                .checked_add(Fixed::from_f32(offset)?)?,
            length,
        ),
        AxisResize::Springs {
            leading,
            length: size,
            trailing,
        } => {
            let flexible = [leading, size, trailing];
            let [a, b, c] = Scalar::normalize_ratios(originals)?;
            let segments = [a, b, c.checked_sub(a)?.checked_sub(b)?];
            let mut total = Scalar::ZERO;
            for (value, flexible) in segments.into_iter().zip(flexible) {
                if flexible {
                    total = total.checked_add(value.max(Scalar::ZERO))?;
                }
            }
            let count = flexible.iter().filter(|f| **f).count() as i64;
            let delta = parent.checked_sub(reference)?;
            let share = |i: usize| -> Result<Fixed> {
                if !flexible[i] || count == 0 {
                    Ok(Fixed::ZERO)
                } else if total > Scalar::ZERO {
                    delta.checked_scale_ratio(segments[i].max(Scalar::ZERO), total)
                } else {
                    delta.checked_div_int(count)
                }
            };
            (
                start.checked_add(share(0)?)?,
                length.checked_add(share(1)?)?,
            )
        }
    };
    let limit = Fixed::from_f32(LIMIT)?;
    if start < Fixed::ZERO.checked_sub(limit)?
        || start > limit
        || length <= Fixed::ZERO
        || length > limit
    {
        return Err("Fixed resize produced an infeasible frame".into());
    }
    Ok((start.to_f32(), length.to_f32()))
}
fn axis_float32(
    start: f32,
    length: f32,
    parent: f32,
    reference: f32,
    rule: AxisResize,
) -> Result<(f32, f32)> {
    let (start, length) = match rule {
        AxisResize::Fixed => (start, length),
        AxisResize::Pins {
            leading: Some(a),
            trailing: Some(b),
        } => (a, parent - a - b),
        AxisResize::Pins {
            leading: Some(a),
            trailing: None,
        } => (a, length),
        AxisResize::Pins {
            leading: None,
            trailing: Some(b),
        } => (parent - b - length, length),
        AxisResize::Pins {
            leading: None,
            trailing: None,
        } => return Err("Pins need an edge".into()),
        AxisResize::Center { offset } => (parent / 2. - length / 2. + offset, length),
        AxisResize::Springs {
            leading,
            length: size,
            trailing,
        } => {
            let flexible = [leading, size, trailing];
            let segments = [start, length, reference - start - length];
            let total: f32 = segments
                .iter()
                .zip(flexible)
                .filter(|(_, f)| *f)
                .map(|(v, _)| v.max(0.))
                .sum();
            let count = flexible.iter().filter(|f| **f).count();
            let share = |i: usize| {
                if !flexible[i] || count == 0 {
                    0.
                } else if total > 0. {
                    segments[i].max(0.) / total
                } else {
                    1. / count as f32
                }
            };
            let delta = parent - reference;
            (start + delta * share(0), length + delta * share(1))
        }
    };
    if !start.is_finite()
        || start.abs() > LIMIT
        || !length.is_finite()
        || length <= 0.
        || length > LIMIT
    {
        return Err("Resize produced an infeasible frame".into());
    }
    Ok((start, length))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rule(horizontal: AxisResize) -> ResizeRule {
        ResizeRule {
            reference_size: [200., 120.],
            horizontal,
            vertical: AxisResize::Fixed,
        }
    }
    #[test]
    fn fixed_resize_matches_the_float_reference_across_pins_and_spring_combinations() {
        let mut rules = vec![
            AxisResize::Fixed,
            AxisResize::Center { offset: 3.125 },
            AxisResize::Pins {
                leading: Some(12.25),
                trailing: Some(18.75),
            },
            AxisResize::Pins {
                leading: None,
                trailing: Some(17.125),
            },
            AxisResize::Pins {
                leading: Some(17.125),
                trailing: None,
            },
        ];
        for bits in 0..8 {
            rules.push(AxisResize::Springs {
                leading: bits & 1 != 0,
                length: bits & 2 != 0,
                trailing: bits & 4 != 0,
            });
        }
        for r in rules {
            for parent in [120., 160., 200., 313.125, 2048., 8192.] {
                let frame = [17.125, 30.25, 100.5, 40.75];
                let mut rule = rule(r);
                rule.reference_size = [200.25, 120.];
                let reference = resolve_float32(frame, [parent, 120.], rule);
                let fixed = resolve_fixed(frame, [parent, 120.], rule);
                match (reference, fixed) {
                    (Ok(a), Ok(b)) => {
                        for i in 0..4 {
                            assert!(
                                (a[i] - b[i]).abs() <= 0.001,
                                "{r:?} parent {parent}, axis {i}: {} vs {}",
                                a[i],
                                b[i]
                            );
                        }
                    }
                    (Err(_), Err(_)) => {}
                    result => panic!("Resize admission differs: {r:?}, {parent}: {result:?}"),
                }
            }
        }
        // The pilot does not silently make a positive sub-grid size equal zero.
        assert!(resolve([0., 0., 1e-8, 1.], [1., 1.], rule(AxisResize::Fixed)).is_ok());
        assert_eq!(
            resolve_fixed([0., 0., 1e-8, 1.], [1., 1.], rule(AxisResize::Fixed)).unwrap(),
            [0., 0., 1e-8, 1.]
        );
    }
    #[test]
    fn pins_preserve_edge_gaps_and_centering_preserves_size() {
        let frame = [20., 30., 100., 40.];
        assert_eq!(
            resolve(
                frame,
                [300., 120.],
                rule(AxisResize::Pins {
                    leading: Some(16.),
                    trailing: Some(24.)
                })
            )
            .unwrap(),
            [16., 30., 260., 40.]
        );
        assert_eq!(
            resolve(
                frame,
                [300., 120.],
                rule(AxisResize::Pins {
                    leading: None,
                    trailing: Some(12.)
                })
            )
            .unwrap(),
            [188., 30., 100., 40.]
        );
        assert_eq!(
            resolve(
                frame,
                [300., 120.],
                rule(AxisResize::Pins {
                    leading: Some(12.),
                    trailing: None
                })
            )
            .unwrap(),
            [12., 30., 100., 40.]
        );
        assert_eq!(
            resolve(frame, [300., 120.], rule(AxisResize::Center { offset: 5. })).unwrap(),
            [105., 30., 100., 40.]
        );
    }
    #[test]
    fn springs_preserve_struts_and_share_delta_by_design_lengths() {
        let frame = [20., 30., 100., 40.];
        assert_eq!(
            resolve(
                frame,
                [300., 120.],
                rule(AxisResize::Springs {
                    leading: false,
                    length: true,
                    trailing: false
                })
            )
            .unwrap(),
            [20., 30., 200., 40.]
        );
        assert_eq!(
            resolve(
                frame,
                [300., 120.],
                rule(AxisResize::Springs {
                    leading: true,
                    length: false,
                    trailing: true
                })
            )
            .unwrap(),
            [40., 30., 100., 40.]
        );
        assert_eq!(
            resolve(
                frame,
                [320., 120.],
                rule(AxisResize::Springs {
                    leading: true,
                    length: true,
                    trailing: false
                })
            )
            .unwrap(),
            [40., 30., 200., 40.]
        );
        let all = rule(AxisResize::Springs {
            leading: true,
            length: true,
            trailing: true,
        });
        assert_eq!(
            resolve(frame, [100., 120.], all).unwrap(),
            [10., 30., 50., 40.]
        );
        assert_eq!(
            resolve(
                frame,
                [300., 120.],
                rule(AxisResize::Springs {
                    leading: false,
                    length: false,
                    trailing: false
                })
            )
            .unwrap(),
            frame
        );
        // Both selected margins start at zero: split the extra space equally.
        assert_eq!(
            resolve(
                [0., 30., 200., 40.],
                [300., 120.],
                rule(AxisResize::Springs {
                    leading: true,
                    length: false,
                    trailing: true
                })
            )
            .unwrap(),
            [50., 30., 200., 40.]
        );
    }
    #[test]
    fn all_six_spring_flags_preserve_the_reference_frame() {
        for flags in 0..64 {
            let rule = ResizeRule {
                reference_size: [200., 120.],
                horizontal: AxisResize::Springs {
                    leading: flags & 1 != 0,
                    length: flags & 2 != 0,
                    trailing: flags & 4 != 0,
                },
                vertical: AxisResize::Springs {
                    leading: flags & 8 != 0,
                    length: flags & 16 != 0,
                    trailing: flags & 32 != 0,
                },
            };
            assert_eq!(
                resolve([20., 30., 100., 40.], [200., 120.], rule).unwrap(),
                [20., 30., 100., 40.]
            );
        }
    }
    #[test]
    fn bad_rules_and_impossible_shrink_are_explicit_errors() {
        let frame = [20., 30., 100., 40.];
        for invalid in [
            AxisResize::Pins {
                leading: None,
                trailing: None,
            },
            AxisResize::Pins {
                leading: Some(f32::NAN),
                trailing: None,
            },
            AxisResize::Pins {
                leading: Some(-1.),
                trailing: None,
            },
            AxisResize::Center {
                offset: f32::INFINITY,
            },
        ] {
            assert!(resolve(frame, [200., 120.], rule(invalid)).is_err());
        }
        assert!(
            resolve(
                frame,
                [20., 120.],
                rule(AxisResize::Pins {
                    leading: Some(16.),
                    trailing: Some(24.)
                })
            )
            .is_err()
        );
        assert!(
            resolve(
                frame,
                [20., 120.],
                rule(AxisResize::Springs {
                    leading: false,
                    length: true,
                    trailing: false
                })
            )
            .is_err()
        );
        let mut bad = rule(AxisResize::Fixed);
        bad.reference_size[0] = 0.;
        assert!(resolve(frame, [200., 120.], bad).is_err());
        assert!(resolve(frame, [f32::NAN, 120.], rule(AxisResize::Fixed)).is_err());
    }
}
