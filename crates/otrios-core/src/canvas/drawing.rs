// SPDX-License-Identifier: LGPL-3.0-only
//! A shared world with a device-local camera. No camera state enters app data.
use super::{Result, state};
use otrios_interfaces::{
    drawing::{COORDINATE_LIMIT, Command, MAX_POINTS, Options, Stroke},
    scene::{PointerPhase, Rect, SceneNode},
};
use std::collections::BTreeMap;
pub mod render;
#[derive(Clone, Debug, serde::Serialize)]
pub struct Usage {
    pub center: [f32; 2],
    pub zoom: f32,
    pub segments: usize,
    pub points: usize,
    pub gpu: render::Stats,
    pub participants: usize,
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
    pub render: render::Cache,
    peer: u32,
    peers: BTreeMap<u32, (u64, String, otrios_interfaces::ink::Viewport)>,
    last_sent: Option<(u64, otrios_interfaces::ink::Viewport)>,
    camera_at: Option<u64>,
    presence_bucket: u64,
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
            render: render::Cache::default(),
            peer: 0,
            peers: BTreeMap::new(),
            last_sent: None,
            camera_at: None,
            presence_bucket: 0,
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
            gpu: self.render.stats.clone(),
            participants: self.peers.len(),
        }
    }
    pub fn frame(&mut self, board: &state::Drawing, extent: [f32; 2]) -> render::Frame {
        let scale = self.scale(extent);
        let mut board = board.clone();
        if let Some((_, active)) = &self.active
            && active.epoch == board.epoch
        {
            if let Some(old) = board.strokes.iter_mut().find(|s| s.id == active.id) {
                *old = active.clone();
            } else {
                board.strokes.push(active.clone());
            }
        }
        self.render
            .frame(&board, self.center, scale, extent, self.options.grid)
    }
    pub fn nodes(&self, _board: &state::Drawing, extent: [f32; 2], base: u32) -> Vec<SceneNode> {
        let Some(at) = self.camera_at else {
            return vec![];
        };
        let now = self.presence_bucket * 100;
        let elapsed = now.saturating_sub(at);
        if elapsed > 1500 {
            return vec![];
        }
        let alpha = if elapsed < 1000 {
            220
        } else {
            ((1500 - elapsed) * 220 / 500) as u8
        };
        let mut nodes = vec![];
        let palette = [
            [255, 161, 94],
            [208, 160, 255],
            [255, 116, 172],
            [185, 238, 143],
            [125, 172, 255],
        ];
        for (id, (_, name, view)) in self.peers.iter().take(64) {
            let low = self.screen(
                [view.bounds[0] as f32 / 4., view.bounds[1] as f32 / 4.],
                extent,
            );
            let high = self.screen(
                [view.bounds[2] as f32 / 4., view.bounds[3] as f32 / 4.],
                extent,
            );
            let rgb = palette[*id as usize % palette.len()];
            let color = [rgb[0], rgb[1], rgb[2], alpha];
            for (a, b) in [
                (low, [high[0], low[1]]),
                ([high[0], low[1]], high),
                (high, [low[0], high[1]]),
                ([low[0], high[1]], low),
            ] {
                if let Some([a, b]) = clip_segment(a, b, extent) {
                    let mut n = crate::dashboard::components::node(
                        base + nodes.len() as u32,
                        Rect::new(0., 0., extent[0], extent[1]),
                        otrios_interfaces::scene::Primitive::Path {
                            points: vec![a, b],
                            width: 1.5,
                            color,
                        },
                    );
                    n.responder = Some(otrios_interfaces::scene::TouchResponder::PassThrough);
                    nodes.push(n);
                }
            }
            if low[0] < extent[0] && low[1] < extent[1] && high[0] >= 0. && high[1] >= 0. {
                let label = format!(
                    "{} · {}",
                    name.chars().take(24).collect::<String>(),
                    id % 1000
                );
                let mut n = crate::dashboard::components::node(
                    base + nodes.len() as u32,
                    Rect::new(
                        low[0].clamp(4., (extent[0] - 220.).max(4.)),
                        low[1].clamp(4., (extent[1] - 24.).max(4.)),
                        216.,
                        24.,
                    ),
                    otrios_interfaces::scene::Primitive::Text {
                        value: label,
                        scale: 1,
                        color,
                    },
                );
                n.responder = Some(otrios_interfaces::scene::TouchResponder::PassThrough);
                nodes.push(n);
            }
        }
        nodes
    }
    pub fn camera(&self) -> ([f32; 2], f32) {
        (self.center, self.zoom)
    }
    pub fn reset_presence(&mut self) {
        self.peer = 0;
        self.peers.clear();
        self.last_sent = None;
    }
    pub fn camera_changed(&mut self, now: u64) {
        self.camera_at = Some(now);
    }
    pub fn presence_outgoing(&mut self, now: u64, extent: [f32; 2]) -> Vec<u8> {
        let low = self.world([0., 0.], extent);
        let high = self.world(extent, extent);
        let bounds = [low[0], low[1], high[0], high[1]].map(|n| {
            (n * 4.)
                .round()
                .clamp(-(1_i32 << 30) as f32, (1_i32 << 30) as f32) as i32
        });
        let view = otrios_interfaces::ink::Viewport {
            bounds,
            active: self
                .camera_at
                .is_some_and(|at| now.saturating_sub(at) < 200),
        };
        if self.last_sent.is_some_and(|(at, old)| {
            now.saturating_sub(at) < 100 || old == view && now.saturating_sub(at) < 1500
        }) {
            return vec![];
        }
        self.last_sent = Some((now, view));
        view.encode()
    }
    pub fn presence_receive(&mut self, mut bytes: &[u8], now: u64) -> Result<bool> {
        let mut changed = false;
        while !bytes.is_empty() {
            if bytes.starts_with(otrios_interfaces::ink::VIEW_MAGIC) {
                let (peer, age, name, view, count) = otrios_interfaces::ink::decode_view(bytes)?;
                bytes = &bytes[count..];
                if peer != self.peer {
                    if self.peers.len() < 64 || self.peers.contains_key(&peer) {
                        self.peers
                            .insert(peer, (now.saturating_sub(u64::from(age)), name, view));
                        changed = true;
                    }
                }
            } else {
                if bytes.len() < otrios_interfaces::ink::SERVER_BYTES {
                    return Err("Presence welcome bounds".into());
                }
                let (peer, _, point) = otrios_interfaces::ink::decode_server(
                    &bytes[..otrios_interfaces::ink::SERVER_BYTES],
                )?;
                if point.is_none() {
                    self.peer = peer;
                    self.peers.remove(&peer);
                }
                bytes = &bytes[otrios_interfaces::ink::SERVER_BYTES..];
            }
        }
        Ok(changed
            && self
                .camera_at
                .is_some_and(|at| now.saturating_sub(at) < 1500))
    }
    pub fn presence_tick(&mut self, now: u64) -> bool {
        let count = self.peers.len();
        self.peers
            .retain(|_, (at, _, _)| now.saturating_sub(*at) < 5000);
        let bucket = now / 100;
        let changed = !self.peers.is_empty()
            && self.presence_bucket != bucket
            && self
                .camera_at
                .is_some_and(|at| now.saturating_sub(at) < 1700);
        self.presence_bucket = bucket;
        changed || count != self.peers.len()
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
    fn viewport_presence_is_bounded_ephemeral_and_never_takes_input() {
        use otrios_interfaces::ink;
        let mut view = View::new(Options {
            reference: [1280, 800],
            grid: 80,
        });
        view.presence_receive(&ink::welcome(7), 0).unwrap();
        let camera = ink::Viewport {
            bounds: [0, 0, 5120, 3200],
            active: false,
        };
        view.presence_receive(&ink::view_envelope(8, 0, "Frame", camera), 100)
            .unwrap();
        assert!(
            view.nodes(&state::Drawing::default(), [400., 800.], 100)
                .is_empty()
        );
        view.camera_changed(200);
        view.presence_tick(200);
        let nodes = view.nodes(&state::Drawing::default(), [400., 800.], 100);
        assert!(!nodes.is_empty());
        assert!(nodes.iter().all(|n|n.responder==Some(otrios_interfaces::scene::TouchResponder::PassThrough)));
        let packet = view.presence_outgoing(200, [400., 800.]);
        assert!(ink::Viewport::decode(&packet).is_ok());
        assert!(view.presence_outgoing(250, [400., 800.]).is_empty());
        view.presence_tick(1800);
        assert!(
            view.nodes(&state::Drawing::default(), [400., 800.], 100)
                .is_empty()
        );
        view.presence_tick(6000);
        assert!(view.peers.is_empty());
        let mut invalid = camera;
        invalid.bounds = [10, 0, 0, 10];
        assert!(ink::Viewport::decode(&invalid.encode()).is_err());
    }
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
