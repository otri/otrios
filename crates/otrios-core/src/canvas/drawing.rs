// SPDX-License-Identifier: LGPL-3.0-only
//! A shared world with a device-local camera. No camera state enters app data.
use super::{Result, state};
use otrios_interfaces::{
    drawing::{COORDINATE_LIMIT, Command, MAX_POINTS, Options, Stroke},
    scene::{PointerPhase, Primitive, Rect, SceneNode},
};
use std::collections::BTreeMap;
#[derive(Clone, Debug, serde::Serialize)]
pub struct Usage {
    pub center: [f32; 2],
    pub zoom: f32,
    pub segments: usize,
    pub points: usize,
}
#[derive(Clone)]
pub struct View {
    options: Options,
    center: [f32; 2],
    zoom: f32,
    contacts: BTreeMap<u32, [f32; 2]>,
    multi: bool,
    active: Option<(u32, Stroke)>,
    pub external_pan: bool,
}
impl View {
    pub fn new(options: Options) -> Self {
        Self {
            center: options.reference.map(|n| f32::from(n) / 2.),
            options,
            zoom: 1.,
            contacts: BTreeMap::new(),
            multi: false,
            active: None,
            external_pan: false,
        }
    }
    pub(crate) fn preserve_camera(&mut self, old: &Self) {
        self.center = old.center;
        self.zoom = old.zoom;
    }
    fn scale(&self, extent: [f32; 2]) -> f32 {
        self.zoom
            * (extent[0] / f32::from(self.options.reference[0]))
                .min(extent[1] / f32::from(self.options.reference[1]))
                .max(0.001)
    }
    pub fn world(&self, p: [f32; 2], extent: [f32; 2]) -> [f32; 2] {
        let s = self.scale(extent);
        std::array::from_fn(|i| self.center[i] + (p[i] - extent[i] / 2.) / s)
    }
    fn screen(&self, p: [f32; 2], extent: [f32; 2]) -> [f32; 2] {
        let s = self.scale(extent);
        std::array::from_fn(|i| (p[i] - self.center[i]) * s + extent[i] / 2.)
    }
    pub fn pan(&mut self, delta: [f32; 2], extent: [f32; 2]) -> bool {
        if delta.iter().any(|n| !n.is_finite() || n.abs() > 16384.) {
            return false;
        }
        self.active = None;
        let scale = self.scale(extent);
        for i in 0..2 {
            self.center[i] = (self.center[i] - delta[i] / scale).clamp(-65536., 65536.);
        }
        true
    }
    pub fn zoom(&mut self, factor: f32, pivot: [f32; 2], extent: [f32; 2]) -> bool {
        if !factor.is_finite() || factor <= 0. || pivot.iter().any(|n| !n.is_finite()) {
            return false;
        }
        self.active = None;
        let world = self.world(pivot, extent);
        self.zoom = (self.zoom * factor.clamp(0.25, 4.)).clamp(1. / 1024., 32.);
        let scale = self.scale(extent);
        for i in 0..2 {
            self.center[i] =
                (world[i] - (pivot[i] - extent[i] / 2.) / scale).clamp(-65536., 65536.);
        }
        true
    }
    pub fn fit(&mut self, board: &state::Drawing, extent: [f32; 2]) {
        self.active = None;
        if board.strokes.is_empty() {
            self.center = self.options.reference.map(|n| f32::from(n) / 2.);
            self.zoom = 1.;
            return;
        }
        let mut low = [f32::MAX; 2];
        let mut high = [f32::MIN; 2];
        for p in board.strokes.iter().flat_map(|s| &s.points) {
            for i in 0..2 {
                let n = p[i] as f32 / 4.;
                low[i] = low[i].min(n);
                high[i] = high[i].max(n);
            }
        }
        self.center = std::array::from_fn(|i| (low[i] + high[i]) / 2.);
        let scale = ((extent[0] - 64.).max(1.) / (high[0] - low[0]).max(80.))
            .min((extent[1] - 64.).max(1.) / (high[1] - low[1]).max(80.));
        self.zoom = 1.;
        self.zoom = (scale / self.scale(extent)).clamp(1. / 1024., 32.);
    }
    pub fn cancel(&mut self) {
        self.contacts.clear();
        self.active = None;
        self.multi = false;
        self.external_pan = false;
    }
    fn pinch(&self) -> Option<([f32; 2], f32)> {
        let mut p = self.contacts.values();
        let (a, b) = (p.next()?, p.next()?);
        Some((
            [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.],
            (a[0] - b[0]).hypot(a[1] - b[1]).max(1.),
        ))
    }
    fn point(&self, p: [f32; 2], extent: [f32; 2]) -> [i32; 2] {
        self.world(p, extent).map(|n| {
            (n * 4.)
                .round()
                .clamp(-(COORDINATE_LIMIT as f32), COORDINATE_LIMIT as f32) as i32
        })
    }
    pub fn pointer(
        &mut self,
        id: u32,
        phase: PointerPhase,
        p: [f32; 2],
        extent: [f32; 2],
        epoch: [u8; 16],
        editable: bool,
    ) -> Result<Vec<Command>> {
        let before = self.pinch();
        let mut commands = vec![];
        if phase == PointerPhase::Down {
            if self.contacts.len() >= 10 {
                return Ok(commands);
            }
            self.contacts.insert(id, p);
            if self.contacts.len() == 1 && !self.multi && editable {
                self.active = Some((
                    id,
                    Stroke {
                        epoch,
                        id: state::writer_nonce()?,
                        points: vec![self.point(p, extent)],
                    },
                ));
            } else {
                self.active = None;
                self.multi = true;
            }
        } else if self.contacts.contains_key(&id) {
            self.contacts.insert(id, p);
        } else {
            return Ok(commands);
        }
        if self.contacts.len() >= 2 {
            // Contact transitions establish/end a gesture, never transform it.
            // Cancel/up coordinates may be absent or stale on browser hosts.
            if phase == PointerPhase::Move
                && let (Some((old, old_distance)), Some((new, distance))) = (before, self.pinch())
            {
                let anchor = self.world(old, extent);
                self.zoom = (self.zoom * (distance / old_distance)).clamp(1. / 1024., 32.);
                let scale = self.scale(extent);
                for i in 0..2 {
                    self.center[i] =
                        (anchor[i] - (new[i] - extent[i] / 2.) / scale).clamp(-65536., 65536.);
                }
            }
        } else if !self.multi && phase != PointerPhase::Cancel {
            let point = self.point(p, extent);
            let slop = 12. / self.scale(extent);
            if let Some((pointer, stroke)) = &mut self.active {
                if *pointer == id && stroke.epoch == epoch {
                    if phase == PointerPhase::Move
                        && stroke.points.len() == 1
                        && ((point[0] - stroke.points[0][0]) as f32)
                            .hypot((point[1] - stroke.points[0][1]) as f32)
                            < slop
                    {
                        return Ok(commands);
                    }
                    if stroke.points.last() != Some(&point) {
                        if stroke.points.len() == MAX_POINTS {
                            commands.push(Command::Put(stroke.clone()));
                            *stroke = Stroke {
                                epoch,
                                id: state::writer_nonce()?,
                                points: vec![*stroke.points.last().unwrap(), point],
                            };
                        } else {
                            stroke.points.push(point);
                        }
                    }
                    if phase != PointerPhase::Down {
                        commands.push(Command::Put(stroke.clone()));
                    }
                }
            }
        }
        if matches!(phase, PointerPhase::Up | PointerPhase::Cancel) {
            self.contacts.remove(&id);
            if self.active.as_ref().is_some_and(|(p, _)| *p == id) {
                self.active = None;
            }
            if self.contacts.is_empty() {
                self.multi = false;
            }
        }
        Ok(commands)
    }
    pub fn usage(&self, board: &state::Drawing) -> Usage {
        Usage {
            center: self.center,
            zoom: self.zoom,
            segments: board.strokes.len(),
            points: board.strokes.iter().map(|s| s.points.len()).sum(),
        }
    }
    pub fn nodes(&self, board: &state::Drawing, extent: [f32; 2], base: u32) -> Vec<SceneNode> {
        let mut out = vec![];
        let mut add = |primitive: Primitive| {
            let mut node = crate::dashboard::components::node(
                base + out.len() as u32,
                Rect::new(0., 0., extent[0], extent[1]),
                primitive,
            );
            node.responder = Some(otrios_interfaces::scene::TouchResponder::PassThrough);
            out.push(node);
        };
        let scale = self.scale(extent);
        let mut step = f32::from(self.options.grid);
        while step * scale < 32_f32.max((extent[0] + extent[1]) / 192.) {
            step *= 2.;
        }
        while step * scale > 160. {
            step /= 2.;
        }
        let low = self.world([0., 0.], extent);
        let high = self.world(extent, extent);
        let mut x = (low[0] / step).floor() * step;
        while x <= high[0] {
            let p = self.screen([x, 0.], extent)[0].clamp(0., extent[0]);
            add(Primitive::Path {
                points: vec![[p, 0.], [p, extent[1]]],
                width: 1.,
                color: [112, 121, 131, 100],
            });
            x += step;
        }
        let mut y = (low[1] / step).floor() * step;
        while y <= high[1] {
            let p = self.screen([0., y], extent)[1].clamp(0., extent[1]);
            add(Primitive::Path {
                points: vec![[0., p], [extent[0], p]],
                width: 1.,
                color: [112, 121, 131, 100],
            });
            y += step;
        }
        let mut strokes = board.strokes.clone();
        if let Some((_, active)) = &self.active {
            if active.epoch == board.epoch {
                if let Some(old) = strokes.iter_mut().find(|s| s.id == active.id) {
                    *old = active.clone();
                } else {
                    strokes.push(active.clone());
                }
            }
        }
        for stroke in strokes {
            let mut points: Vec<_> = stroke
                .points
                .iter()
                .map(|p| self.screen(p.map(|n| n as f32 / 4.), extent))
                .collect();
            if points.len() == 1 {
                points.push([points[0][0] + 0.25, points[0][1]]);
            }
            // Clip before scene admission: zoomed world points can be far
            // beyond the scene coordinate budget. Keep disjoint runs separate.
            let width = (3. * scale).clamp(1., 16.);
            let mut triangles = vec![];
            for pair in points.windows(2) {
                if let Some([a, b]) = clip_segment(pair[0], pair[1], extent) {
                    let d = [b[0] - a[0], b[1] - a[1]];
                    let length = d[0].hypot(d[1]);
                    if length == 0. {
                        continue;
                    }
                    let n = [-d[1] / length * width / 2., d[0] / length * width / 2.];
                    let quad = [
                        [a[0] + n[0], a[1] + n[1]],
                        [b[0] + n[0], b[1] + n[1]],
                        [a[0] - n[0], a[1] - n[1]],
                        [b[0] - n[0], b[1] - n[1]],
                    ]
                    .map(|p| [p[0].clamp(0., extent[0]), p[1].clamp(0., extent[1])]);
                    triangles.extend([quad[0], quad[1], quad[2], quad[2], quad[1], quad[3]]);
                }
            }
            if !triangles.is_empty() {
                add(Primitive::Triangles {
                    points: triangles,
                    color: [94, 218, 231, 255],
                });
            }
        }
        out
    }
}
fn clip_segment(a: [f32; 2], b: [f32; 2], extent: [f32; 2]) -> Option<[[f32; 2]; 2]> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let (mut low, mut high) = (0_f32, 1_f32);
    for i in 0..2 {
        if d[i] == 0. {
            if a[i] < 0. || a[i] > extent[i] {
                return None;
            }
        } else {
            let u = -a[i] / d[i];
            let v = (extent[i] - a[i]) / d[i];
            low = low.max(u.min(v));
            high = high.min(u.max(v));
        }
    }
    (low <= high).then(|| {
        [low, high].map(|t| std::array::from_fn(|i| (a[i] + t * d[i]).clamp(0., extent[i])))
    })
}
impl super::Client {
    pub fn pan_drawing(&mut self, epoch: u32, dx: f32, dy: f32, active: bool) -> Result<bool> {
        if epoch != self.epoch() {
            return Ok(false);
        }
        let m = self.active_drawing_mut();
        let Some((view, extent)) = m else {
            return Ok(false);
        };
        view.external_pan = active;
        let changed = view.pan([dx, dy], extent);
        self.invalidate_drawing();
        Ok(changed)
    }
    pub fn zoom_drawing(&mut self, epoch: u32, factor: f32, x: f32, y: f32) -> Result<bool> {
        if epoch != self.epoch() {
            return Ok(false);
        }
        let Some((view, extent)) = self.active_drawing_mut() else {
            return Ok(false);
        };
        let changed = view.zoom(factor, [x, y], extent);
        self.invalidate_drawing();
        Ok(changed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinch_closes_below_initial_zoom_pans_with_fingers_and_lift_never_jumps() {
        let mut view = View::new(Options {
            reference: [1280, 800],
            grid: 80,
        });
        let extent = [400., 800.];
        for (id, phase, p) in [
            (1, PointerPhase::Down, [100., 300.]),
            (2, PointerPhase::Down, [200., 300.]),
            (2, PointerPhase::Move, [300., 300.]),
            (2, PointerPhase::Move, [150., 300.]),
        ] {
            assert!(
                view.pointer(id, phase, p, extent, [0; 16], true)
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            (view.zoom - 0.5).abs() < 0.001,
            "pinch closed zooms below the starting view"
        );
        for (id, p) in [(2, [200., 300.]), (1, [110., 320.]), (2, [210., 320.])] {
            view.pointer(id, PointerPhase::Move, p, extent, [0; 16], true)
                .unwrap();
        }
        assert!((view.zoom - 1.).abs() < 0.001);
        assert!(
            (view.center[0] - 608.).abs() < 0.001 && (view.center[1] - 336.).abs() < 0.001,
            "content follows the fingers right and down"
        );
        let before = (view.center, view.zoom);
        for (id, phase, p) in [
            (1, PointerPhase::Up, [0., 0.]),
            (2, PointerPhase::Move, [500., 500.]),
            (2, PointerPhase::Up, [0., 0.]),
        ] {
            assert!(
                view.pointer(id, phase, p, extent, [0; 16], true)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                (view.center, view.zoom),
                before,
                "contact transitions do not move the camera"
            );
        }
    }
    #[test]
    fn drawing_camera_preserves_anchor_and_pinch_creates_no_strokes() {
        let mut view = View::new(Options {
            reference: [1280, 800],
            grid: 80,
        });
        let extent = [400., 800.];
        let pivot = [120., 300.];
        let anchor = view.world(pivot, extent);
        assert!(view.zoom(2., pivot, extent));
        let after = view.world(pivot, extent);
        assert!((after[0] - anchor[0]).abs() < 0.001 && (after[1] - anchor[1]).abs() < 0.001);
        view.fit(&state::Drawing::default(), extent);
        let x = view.screen([80., 0.], extent)[0] - view.screen([0., 0.], extent)[0];
        let y = view.screen([0., 80.], extent)[1] - view.screen([0., 0.], extent)[1];
        assert!((x - y).abs() < 0.001);
        assert!(
            view.pointer(1, PointerPhase::Down, [100., 300.], extent, [0; 16], true)
                .unwrap()
                .is_empty()
        );
        assert!(
            view.pointer(2, PointerPhase::Down, [200., 300.], extent, [0; 16], true)
                .unwrap()
                .is_empty()
        );
        assert!(
            view.pointer(2, PointerPhase::Move, [300., 400.], extent, [0; 16], true)
                .unwrap()
                .is_empty()
        );
        assert!(view.zoom > 1.);
        let board = state::Drawing {
            epoch: [0; 16],
            strokes: vec![Stroke {
                epoch: [0; 16],
                id: [1; 16],
                points: vec![[-262144, 0], [262144, 0], [0, 262144]],
            }],
        };
        let mut far = view.clone();
        far.zoom = 32.;
        let nodes = far.nodes(&board, [8192., 8192.], 1);
        let root = crate::dashboard::components::canvas(0, Rect::new(0., 0., 8192., 8192.), nodes);
        crate::dashboard::scene::Scene::new(root)
            .expect("zoom and offscreen points stay within scene budgets");
        assert!(
            view.pointer(1, PointerPhase::Up, [100., 300.], extent, [0; 16], true)
                .unwrap()
                .is_empty()
        );
        assert!(
            view.pointer(2, PointerPhase::Up, [300., 400.], extent, [0; 16], true)
                .unwrap()
                .is_empty()
        );
    }
}
