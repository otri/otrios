// SPDX-License-Identifier: LGPL-3.0-only
//! Checked hierarchy coordinates. Q32 retains small offsets through cancellation;
//! float attributes are produced only after the complete ancestor transform.
use super::{Result, fixed::Scalar, render::Clip};
use otrios_interfaces::scene::{Primitive, Rect, SceneNode};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Placement {
    pub frame: [Scalar; 4],
    pub clip: [Scalar; 4],
}
impl Placement {
    pub fn rect(self) -> Rect {
        let [x, y, w, h] = self.frame.map(Scalar::to_f32);
        Rect::new(x, y, w, h)
    }
    pub fn clip(self) -> Clip {
        let [left, top, right, bottom] = self.clip.map(Scalar::to_f32);
        Clip {
            left,
            top,
            right,
            bottom,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Space {
    origin: [Scalar; 2],
    scale: Scalar,
    clip: [Scalar; 4],
}
impl Space {
    #[cfg(test)]
    fn place(self, node: &SceneNode) -> Result<(Placement, Self)> {
        let f = node.frame;
        self.place_local(
            [
                Scalar::from_f32(f.x)?,
                Scalar::from_f32(f.y)?,
                Scalar::from_f32(f.width)?,
                Scalar::from_f32(f.height)?,
            ],
            matches!(
                node.primitive,
                Primitive::Canvas { clip: true } | Primitive::Viewport { .. }
            ),
            if let Primitive::Viewport { scale } = node.primitive {
                Some(Scalar::from_f32(scale)?)
            } else {
                None
            },
        )
    }
    fn place_local(
        self,
        local: [Scalar; 4],
        clips: bool,
        viewport: Option<Scalar>,
    ) -> Result<(Placement, Self)> {
        let [lx, ly, width, height] = local;
        let x = self.origin[0].checked_add(lx.checked_mul(self.scale)?)?;
        let y = self.origin[1].checked_add(ly.checked_mul(self.scale)?)?;
        let width = width.checked_mul(self.scale)?;
        let height = height.checked_mul(self.scale)?;
        let mut next = Self {
            origin: [x, y],
            ..self
        };
        if clips {
            next.clip = [
                self.clip[0].max(x),
                self.clip[1].max(y),
                self.clip[2].min(x.checked_add(width)?),
                self.clip[3].min(y.checked_add(height)?),
            ];
        }
        if let Some(scale) = viewport {
            next.scale = self.scale.checked_mul(scale)?;
        }
        Ok((
            Placement {
                frame: [x, y, width, height],
                clip: self.clip,
            },
            next,
        ))
    }
}

/// Preorder slots retain admitted local inputs, dependency state, wide placements
/// and GPU-rounded counterparts. Topology changes invalidate the affected suffix;
/// ordinary edits update only changed local fields and dependent placements.
#[derive(Default)]
pub(super) struct Cache {
    entries: Vec<Entry>,
    extent: Option<([u32; 2], [Scalar; 4])>,
    pub changed: usize,
}
struct Entry {
    id: u32,
    parent: Option<usize>,
    source: [u32; 4],
    local: [Scalar; 4],
    kind: (bool, Option<u32>),
    viewport: Option<Scalar>,
    parent_space: Space,
    next: Space,
    placement: Placement,
    pub gpu: [f32; 6],
    pub rounded: Option<[Scalar; 6]>,
    pub factor: f32,
}
impl Cache {
    pub fn update(&mut self, root: &SceneNode) -> Result<()> {
        self.changed = 0;
        let result = self.update_inner(root);
        if result.is_err() {
            self.entries.clear();
            self.extent = None;
        }
        result
    }
    fn update_inner(&mut self, root: &SceneNode) -> Result<()> {
        let key = [root.frame.width.to_bits(), root.frame.height.to_bits()];
        if self.extent.as_ref().is_none_or(|(old, _)| *old != key) {
            self.extent = Some((
                key,
                [
                    Scalar::ZERO,
                    Scalar::ZERO,
                    Scalar::from_f32(root.frame.width)?,
                    Scalar::from_f32(root.frame.height)?,
                ],
            ));
        }
        let mut cursor = 0;
        self.walk(
            root,
            None,
            Space {
                origin: [Scalar::ZERO; 2],
                scale: Scalar::ONE,
                clip: self.extent.unwrap().1,
            },
            1.,
            &mut cursor,
        )?;
        self.entries.truncate(cursor);
        Ok(())
    }
    fn walk(
        &mut self,
        node: &SceneNode,
        parent: Option<usize>,
        space: Space,
        factor: f32,
        cursor: &mut usize,
    ) -> Result<()> {
        let index = *cursor;
        *cursor += 1;
        let f = node.frame;
        let values = [f.x, f.y, f.width, f.height];
        let source = values.map(f32::to_bits);
        let kind = (
            matches!(
                node.primitive,
                Primitive::Canvas { clip: true } | Primitive::Viewport { .. }
            ),
            if let Primitive::Viewport { scale } = node.primitive {
                Some(scale.to_bits())
            } else {
                None
            },
        );
        if self
            .entries
            .get(index)
            .is_some_and(|e| e.id != node.id || e.parent != parent)
        {
            self.entries.truncate(index);
        }
        if index == self.entries.len() {
            let local = [
                Scalar::from_f32(values[0])?,
                Scalar::from_f32(values[1])?,
                Scalar::from_f32(values[2])?,
                Scalar::from_f32(values[3])?,
            ];
            let viewport = kind
                .1
                .map(|v| Scalar::from_f32(f32::from_bits(v)))
                .transpose()?;
            let (placement, next) = space.place_local(local, kind.0, viewport)?;
            self.entries.push(Entry {
                id: node.id,
                parent,
                source,
                local,
                kind,
                viewport,
                parent_space: space,
                next,
                placement,
                gpu: [0.; 6],
                rounded: None,
                factor,
            });
            self.refresh(index);
        } else {
            let entry = &mut self.entries[index];
            let mut changed = entry.parent_space != space || entry.kind != kind;
            for i in 0..4 {
                if entry.source[i] != source[i] {
                    entry.local[i] = Scalar::from_f32(values[i])?;
                    changed = true;
                }
            }
            if entry.kind.1 != kind.1 {
                entry.viewport = kind
                    .1
                    .map(|v| Scalar::from_f32(f32::from_bits(v)))
                    .transpose()?;
            }
            entry.source = source;
            entry.kind = kind;
            entry.factor = factor;
            if changed {
                let (placement, next) = space.place_local(entry.local, kind.0, entry.viewport)?;
                entry.parent_space = space;
                entry.placement = placement;
                entry.next = next;
                self.refresh(index);
            }
        }
        let next = self.entries[index].next;
        let child_factor = factor * kind.1.map(f32::from_bits).unwrap_or(1.);
        for child in &node.children {
            self.walk(child, Some(index), next, child_factor, cursor)?;
        }
        Ok(())
    }
    fn refresh(&mut self, index: usize) {
        self.changed += 1;
        let e = &mut self.entries[index];
        let p = e.placement;
        let values = [
            p.frame[0], p.frame[1], p.clip[0], p.clip[1], p.clip[2], p.clip[3],
        ];
        e.gpu = values.map(Scalar::to_f32);
        let rounded = values.map(Scalar::gpu_rounded);
        e.rounded = if let [Some(x), Some(y), Some(l), Some(t), Some(r), Some(b)] = rounded {
            Some([x, y, l, t, r, b])
        } else {
            None
        };
    }
    pub fn slot(&self, id: u32) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }
    pub fn view(&self, slot: usize) -> ([f32; 6], Option<[Scalar; 6]>) {
        let e = &self.entries[slot];
        (e.gpu, e.rounded)
    }
    pub fn factor(&self, slot: usize) -> f32 {
        self.entries[slot].factor
    }
    pub fn placements(&self) -> BTreeMap<u32, Placement> {
        self.entries.iter().map(|e| (e.id, e.placement)).collect()
    }
}
/// Failure falls back for the entire frame; no partially transformed hierarchy
/// is published and existing float contract admission remains unchanged.
#[cfg(test)]
pub(super) fn placements(root: &SceneNode) -> Result<BTreeMap<u32, Placement>> {
    fn walk(node: &SceneNode, space: Space, out: &mut BTreeMap<u32, Placement>) -> Result<()> {
        let (placement, next) = space.place(node)?;
        out.insert(node.id, placement);
        for child in &node.children {
            walk(child, next, out)?;
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(
        root,
        Space {
            origin: [Scalar::ZERO; 2],
            scale: Scalar::ONE,
            clip: [
                Scalar::ZERO,
                Scalar::ZERO,
                Scalar::from_f32(root.frame.width)?,
                Scalar::from_f32(root.frame.height)?,
            ],
        },
        &mut out,
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::components::{WHITE, canvas, text};
    #[test]
    fn cache_updates_only_dependencies_and_recovers_after_failed_admission() {
        let mut root = canvas(
            0,
            Rect::new(0., 0., 200., 100.),
            vec![
                canvas(
                    1,
                    Rect::new(10., 10., 100., 60.),
                    vec![text(2, Rect::new(2., 3., 90., 20.), "ABC", 2, WHITE)],
                ),
                text(3, Rect::new(120., 10., 70., 20.), "XYZ", 2, WHITE),
            ],
        );
        let mut cache = Cache::default();
        cache.update(&root).unwrap();
        assert_eq!(cache.changed, 4);
        cache.update(&root).unwrap();
        assert_eq!(cache.changed, 0);
        root.children[0].children[0].frame.x = -4.25;
        cache.update(&root).unwrap();
        assert_eq!(cache.changed, 1);
        root.children[0].frame.x += 2.;
        cache.update(&root).unwrap();
        assert_eq!(cache.changed, 2);
        for (id, p) in placements(&root).unwrap() {
            let slot = cache.slot(id).unwrap();
            assert_eq!(cache.entries[slot].placement.frame, p.frame);
            assert_eq!(cache.entries[slot].placement.clip, p.clip);
        }
        root.children.swap(0, 1);
        cache.update(&root).unwrap();
        assert_eq!(
            cache.entries[cache.slot(2).unwrap()].placement.rect().x,
            7.75
        );
        let good = root.clone();
        root.children[1].frame.x = f32::INFINITY;
        assert!(cache.update(&root).is_err());
        assert!(cache.entries.is_empty());
        cache.update(&good).unwrap();
        assert_eq!(cache.changed, 4);
    }
    #[test]
    fn cancelling_large_ancestors_preserve_a_small_local_offset() {
        let mut inner = canvas(
            2,
            Rect::new(0.0001, 0., 100., 100.),
            vec![text(3, Rect::new(-99984., 8., 100., 20.), "ABC", 2, WHITE)],
        );
        inner.primitive = Primitive::Canvas { clip: false };
        let mut outer = canvas(1, Rect::new(100000., 0., 100., 100.), vec![inner]);
        outer.primitive = Primitive::Canvas { clip: false };
        let root = canvas(0, Rect::new(0., 0., 200., 100.), vec![outer]);
        let all = placements(&root).unwrap();
        assert!((all[&3].rect().x - 16.0001).abs() < 0.000002);
        assert_eq!(100000_f32 + 0.0001 - 99984., 16.);
    }
    #[test]
    fn nested_fractional_scales_intersect_all_clip_edges_in_integer_space() {
        let mut inner = canvas(
            2,
            Rect::new(-2.25, 3.5, 80., 50.),
            vec![text(3, Rect::new(-4., -5., 500., 20.), "jyABC", 2, WHITE)],
        );
        inner.primitive = Primitive::Viewport { scale: 0.33 };
        let mut outer = canvas(1, Rect::new(10.125, 4.75, 60., 40.), vec![inner]);
        outer.primitive = Primitive::Viewport { scale: 1.25 };
        let root = canvas(0, Rect::new(0., 0., 200., 100.), vec![outer]);
        let all = placements(&root).unwrap();
        let p = all[&3];
        assert_eq!(p.clip.map(Scalar::to_f32), [10.125, 9.125, 70.125, 44.75]);
        let expected = 10.125_f64 - 2.25 * 1.25 - 4. * 1.25 * f64::from(0.33_f32);
        assert!((f64::from(p.rect().x) - expected).abs() < 0.000001);
    }
}
