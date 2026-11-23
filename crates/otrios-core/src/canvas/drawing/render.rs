// SPDX-License-Identifier: LGPL-3.0-only
//! Immutable world-space segment buffers. Camera motion changes uniforms only.
use super::*;
use std::sync::Arc;
#[derive(Clone)]
pub struct Resource {
    pub version: u64,
    pub vertices: Vec<f32>,
    pub bounds: [f32; 4],
    pub origin: [f32; 2],
}
#[derive(Clone, serde::Serialize)]
pub struct Draw {
    pub version: u64,
    pub origin: [f32; 2],
    #[serde(skip)]
    pub resource: Arc<Resource>,
}
#[derive(Clone, serde::Serialize)]
pub struct Frame {
    pub scale: f32,
    pub extent: [f32; 2],
    pub grid: [f32; 3],
    pub draws: Vec<Draw>,
    pub keep: Vec<u64>,
    #[serde(skip)]
    pub resources: Vec<Arc<Resource>>,
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Stats {
    pub rebuilt_chunks: u64,
    pub visible_chunks: usize,
    pub retained_chunks: usize,
    pub retained_bytes: usize,
}
#[derive(Clone, Default)]
pub struct Cache {
    sequence: u64,
    entries: BTreeMap<([u8; 16], usize), (Vec<[i32; 2]>, Arc<Resource>)>,
    pub stats: Stats,
}
impl Cache {
    pub fn frame(
        &mut self,
        board: &state::Drawing,
        center: [f32; 2],
        scale: f32,
        extent: [f32; 2],
        grid: u16,
    ) -> Frame {
        let mut live = std::collections::BTreeSet::new();
        let mut draws = vec![];
        let mut resources = vec![];
        for stroke in &board.strokes {
            for (index, start) in (0..stroke.points.len().saturating_sub(1).max(1))
                .step_by(63)
                .enumerate()
            {
                let points = &stroke.points[start..(start + 64).min(stroke.points.len())];
                let key = (stroke.id, index);
                live.insert(key);
                let resource = if let Some((old, r)) = self
                    .entries
                    .get(&key)
                    .filter(|(old, _)| old.as_slice() == points)
                {
                    let _ = old;
                    r.clone()
                } else {
                    self.sequence += 1;
                    self.stats.rebuilt_chunks += 1;
                    let origin = points[0].map(|n| n as f32 / 4.);
                    let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
                    let mut local = vec![];
                    for point in points {
                        let p = point.map(|n| n as f32 / 4.);
                        for i in 0..2 {
                            bounds[i] = bounds[i].min(p[i]);
                            bounds[i + 2] = bounds[i + 2].max(p[i]);
                        }
                        local.push([p[0] - origin[0], p[1] - origin[1]]);
                    }
                    if local.len() == 1 {
                        local.push([local[0][0] + 0.25, local[0][1]]);
                    }
                    let mut vertices = Vec::with_capacity(local.len() * 36);
                    for pair in local.windows(2) {
                        if pair[0] == pair[1] {
                            continue;
                        }
                        for corner in [
                            [0., -1.],
                            [1., -1.],
                            [0., 1.],
                            [0., 1.],
                            [1., -1.],
                            [1., 1.],
                        ] {
                            vertices.extend([
                                pair[0][0], pair[0][1], pair[1][0], pair[1][1], corner[0],
                                corner[1],
                            ]);
                        }
                    }
                    let r = Arc::new(Resource {
                        version: self.sequence,
                        vertices,
                        bounds,
                        origin,
                    });
                    self.entries.insert(key, (points.to_vec(), r.clone()));
                    r
                };
                // Reject complete offscreen chunks before host transfer/upload/draw.
                let low = [
                    (resource.bounds[0] - center[0]) * scale + extent[0] / 2.,
                    (resource.bounds[1] - center[1]) * scale + extent[1] / 2.,
                ];
                let high = [
                    (resource.bounds[2] - center[0]) * scale + extent[0] / 2.,
                    (resource.bounds[3] - center[1]) * scale + extent[1] / 2.,
                ];
                if high[0] >= -9.
                    && high[1] >= -9.
                    && low[0] <= extent[0] + 9.
                    && low[1] <= extent[1] + 9.
                {
                    draws.push(Draw {
                        version: resource.version,
                        origin: std::array::from_fn(|i| {
                            (resource.origin[i] - center[i]) * scale + extent[i] / 2.
                        }),
                        resource: resource.clone(),
                    });
                }
                resources.push(resource);
            }
        }
        self.entries.retain(|key, _| live.contains(key));
        self.stats.visible_chunks = draws.len();
        self.stats.retained_chunks = self.entries.len();
        self.stats.retained_bytes = self
            .entries
            .values()
            .map(|(_, r)| r.vertices.len() * 4)
            .sum();
        let mut step = f32::from(grid) * scale;
        while step < 40. {
            step *= 2.;
        }
        while step > 160. {
            step /= 2.;
        }
        let offset = std::array::from_fn::<_, 2, _>(|i| {
            (extent[i] / 2. - center[i] * scale).rem_euclid(step)
        });
        Frame {
            scale,
            extent,
            grid: [step, offset[0], offset[1]],
            draws,
            keep: resources.iter().map(|r| r.version).collect(),
            resources,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_chunks_survive_camera_changes_and_only_the_tail_rebuilds() {
        let mut cache = Cache::default();
        let mut board = state::Drawing {
            epoch: [0; 16],
            strokes: vec![Stroke {
                epoch: [0; 16],
                id: [1; 16],
                points: (0..130).map(|n| [n * 4, 100]).collect(),
            }],
        };
        let a = cache.frame(&board, [64., 40.], 1., [400., 800.], 80);
        let rebuilt = cache.stats.rebuilt_chunks;
        let b = cache.frame(&board, [80., 60.], 2., [400., 800.], 80);
        assert_eq!(rebuilt, cache.stats.rebuilt_chunks);
        assert!(Arc::ptr_eq(&a.resources[0], &b.resources[0]));
        board.strokes[0].points.push([130 * 4, 100]);
        let c = cache.frame(&board, [80., 60.], 2., [400., 800.], 80);
        assert_eq!(cache.stats.rebuilt_chunks, rebuilt + 1);
        assert!(Arc::ptr_eq(&b.resources[0], &c.resources[0]));
        assert!(!Arc::ptr_eq(
            b.resources.last().unwrap(),
            c.resources.last().unwrap()
        ));
        let hidden = cache.frame(&board, [60000., 60000.], 1., [400., 800.], 80);
        assert!(hidden.draws.is_empty());
        assert_eq!(hidden.resources.len(), c.resources.len());
        board.strokes.clear();
        assert!(
            cache
                .frame(&board, [0., 0.], 1., [400., 800.], 80)
                .keep
                .is_empty()
        );
    }
}
