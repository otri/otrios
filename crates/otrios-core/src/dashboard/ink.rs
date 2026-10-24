// SPDX-License-Identifier: LGPL-3.0-only
//! Local presentation ink. Remote samples expire; none are CRDT edits or files.
use super::{Result, components::node, input::Gesture};
use otrios_interfaces::{
    ink::{self, Point},
    scene::{PointerPhase, Primitive, Rect, SceneNode},
};
use std::collections::{BTreeMap, VecDeque};
#[derive(Clone)]
struct Stroke {
    peer: u32,
    id: u32,
    last: i64,
    points: Vec<[u16; 2]>,
}
#[derive(Clone, Default)]
pub struct Ink {
    strokes: VecDeque<Stroke>,
    active: BTreeMap<u32, u32>,
    next: u32,
    own: VecDeque<u32>,
    outgoing: Vec<Point>,
    now: u64,
    bucket: u64,
    sent: Option<u64>,
}
impl Ink {
    pub fn step(&mut self, from: [u16; 2], to: [u16; 2]) {
        if from == to {
            return;
        }
        self.next = self.next.wrapping_add(1).max(1);
        for (phase, position) in [(0, from), (2, to)] {
            let p = Point {
                phase,
                stroke: self.next,
                position,
            };
            self.add(0, p, self.now as i64);
            if self.outgoing.len() < 32 {
                self.outgoing.push(p);
            }
        }
    }
    /// Coordinates are top-left normalized in the admitted app's touch pad.
    pub fn gesture(&mut self, pointer: u32, g: &Gesture, now: u64) {
        self.now = now;
        if g.phase == PointerPhase::Down {
            self.next = self.next.wrapping_add(1).max(1);
            self.active.insert(pointer, self.next);
        }
        let Some(&stroke) = self.active.get(&pointer) else {
            return;
        };
        if g.phase == PointerPhase::Cancel {
            self.active.remove(&pointer);
            if let Some(last) = self
                .strokes
                .iter()
                .find(|s| s.peer == 0 && s.id == stroke)
                .and_then(|s| s.points.last())
                .copied()
            {
                let point = Point {
                    phase: 3,
                    stroke,
                    position: last,
                };
                self.add(0, point, now as i64);
                if self.outgoing.len() < 32 {
                    self.outgoing.push(point);
                }
            }
            return;
        }
        let [x, y] = g.local;
        let [w, h] = g.extent;
        let point = Point {
            phase: match g.phase {
                PointerPhase::Down => 0,
                PointerPhase::Move => 1,
                PointerPhase::Up => 2,
                PointerPhase::Cancel => 3,
            },
            stroke,
            position: [
                (x / w.max(1.)).clamp(0., 1.) * 1000.,
                (y / h.max(1.)).clamp(0., 1.) * 1000.,
            ]
            .map(|n| n.round() as u16),
        };
        self.add(0, point, now as i64);
        // Coalesce only unsent moves. Down/up boundaries remain in order.
        if point.phase == 1
            && self
                .outgoing
                .last()
                .is_some_and(|p| p.phase == 1 && p.stroke == stroke)
        {
            self.outgoing.pop();
        }
        if self.outgoing.len() < 32 {
            self.outgoing.push(point);
        }
        if matches!(g.phase, PointerPhase::Up | PointerPhase::Cancel) {
            self.active.remove(&pointer);
        }
    }
    pub fn cancel(&mut self) {
        let active = std::mem::take(&mut self.active);
        for (_, stroke) in active {
            if let Some(s) = self.strokes.iter().find(|s| s.peer == 0 && s.id == stroke)
                && self.outgoing.len() < 32
            {
                self.outgoing.push(Point {
                    phase: 3,
                    stroke,
                    position: *s.points.last().unwrap_or(&[0, 0]),
                });
            }
        }
    }
    pub fn outgoing(&mut self, now: u64) -> Vec<u8> {
        if self.outgoing.is_empty() || self.sent.is_some_and(|sent| now.saturating_sub(sent) < 33) {
            return vec![];
        }
        self.sent = Some(now);
        std::mem::take(&mut self.outgoing)
            .into_iter()
            .flat_map(Point::encode)
            .collect()
    }
    pub fn receive(&mut self, b: &[u8], now: u64) -> Result<bool> {
        let (peer, age, point) = ink::decode_server(b)?;
        let Some(point) = point else {
            self.own.push_back(peer);
            while self.own.len() > 8 {
                self.own.pop_front();
            }
            return Ok(false);
        };
        if self.own.contains(&peer) || u64::from(age) >= ink::LIFETIME_MS {
            return Ok(false);
        }
        self.add(peer, point, now as i64 - i64::from(age));
        Ok(true)
    }
    fn add(&mut self, peer: u32, p: Point, now: i64) {
        if let Some(s) = self
            .strokes
            .iter_mut()
            .find(|s| s.peer == peer && s.id == p.stroke)
        {
            s.last = s.last.max(now);
            if s.points.last() != Some(&p.position) {
                s.points.push(p.position);
                if s.points.len() > 192 {
                    s.points.remove(0);
                }
            }
        } else {
            if self.strokes.len() >= 24 {
                self.strokes.pop_front();
            }
            self.strokes.push_back(Stroke {
                peer,
                id: p.stroke,
                last: now,
                points: vec![p.position],
            });
        }
    }
    pub fn tick(&mut self, now: u64) -> bool {
        self.now = now;
        let before = self.strokes.len();
        self.strokes
            .retain(|s| now as i64 - s.last < ink::LIFETIME_MS as i64);
        let changed =
            before != self.strokes.len() || (!self.strokes.is_empty() && self.bucket != now / 100);
        self.bucket = now / 100;
        changed
    }
    pub fn nodes(&self, extent: [f32; 2], base: u32) -> Vec<SceneNode> {
        let palette = [
            [94, 218, 231],
            [255, 161, 94],
            [208, 160, 255],
            [255, 116, 172],
            [185, 238, 143],
            [255, 222, 123],
            [125, 172, 255],
            [241, 149, 255],
        ];
        let mut out = vec![];
        let position = |p: [u16; 2]| {
            [
                f32::from(p[0]) / 1000. * extent[0],
                f32::from(p[1]) / 1000. * extent[1],
            ]
        };
        for (index, s) in self.strokes.iter().enumerate() {
            let age = (self.now as i64 - s.last).max(0) as u64;
            if age >= ink::LIFETIME_MS {
                continue;
            }
            let peer = if s.peer == 0 {
                self.own.back().copied().unwrap_or(0)
            } else {
                s.peer
            };
            let rgb = palette[peer as usize % palette.len()];
            let alpha = ((ink::LIFETIME_MS - age) * 255 / ink::LIFETIME_MS) as u8;
            let color = [rgb[0], rgb[1], rgb[2], alpha];
            out.push(node(
                base + index as u32 * 3,
                Rect::new(0., 0., extent[0], extent[1]),
                Primitive::Path {
                    points: s.points.iter().copied().map(position).collect(),
                    width: 3.,
                    color,
                },
            ));
            if self
                .strokes
                .iter()
                .skip(index + 1)
                .any(|next| next.peer == s.peer)
            {
                continue;
            }
            let p = position(*s.points.last().unwrap());
            out.push(node(
                base + index as u32 * 3 + 1,
                Rect::new(p[0] - 5., p[1] - 5., 10., 10.),
                Primitive::Circle { color },
            ));
            out.push(node(
                base + index as u32 * 3 + 2,
                Rect::new(
                    (p[0] + 9.).min(extent[0] - 85.),
                    (p[1] - 18.).max(2.),
                    84.,
                    16.,
                ),
                Primitive::Text {
                    value: if s.peer == 0 {
                        "YOU".into()
                    } else {
                        format!("GHOST {:02X}", s.peer % 256)
                    },
                    scale: 1,
                    color,
                },
            ));
        }
        out
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn point(stroke: u32, phase: u8, x: u16) -> Point {
        Point {
            phase,
            stroke,
            position: [x, 500],
        }
    }
    #[test]
    fn remote_letters_stay_separate_and_fade_five_seconds_after_the_last_point() {
        let mut ink = Ink::default();
        ink.receive(&ink::welcome(1), 0).unwrap();
        for p in [
            point(1, 0, 100),
            point(1, 1, 200),
            point(1, 2, 300),
            point(2, 0, 700),
            point(2, 2, 900),
        ] {
            ink.receive(&ink::envelope(2, 0, p), 1000).unwrap();
        }
        assert_eq!(ink.strokes.len(), 2);
        assert_eq!(ink.strokes[0].points.len(), 3);
        assert_eq!(ink.strokes[1].points.len(), 2);
        assert!(
            !ink.receive(&ink::envelope(1, 0, point(99, 0, 50)), 1000)
                .unwrap()
        );
        ink.tick(3500);
        let nodes = ink.nodes([352., 210.], 100);
        assert!(nodes.iter().any(|n| matches!(
            n.primitive,
            Primitive::Path {
                color: [_, _, _, 127],
                ..
            }
        )));
        ink.tick(5999);
        assert_eq!(ink.strokes.len(), 2);
        ink.tick(6000);
        assert!(ink.strokes.is_empty());
    }
    #[test]
    fn late_join_age_does_not_restart_fading_and_memory_is_bounded() {
        let mut ink = Ink::default();
        ink.receive(&ink::envelope(2, 4000, point(1, 0, 100)), 0)
            .unwrap();
        ink.tick(999);
        assert_eq!(ink.strokes.len(), 1);
        ink.tick(1000);
        assert!(ink.strokes.is_empty());
        for s in 1..100 {
            for x in 0..500 {
                ink.receive(&ink::envelope(3, 0, point(s, 1, x)), 2000)
                    .unwrap();
            }
        }
        assert_eq!(ink.strokes.len(), 24);
        assert!(ink.strokes.iter().all(|s| s.points.len() <= 192));
        let scene = super::super::scene::Scene::new(super::super::components::canvas(
            0,
            Rect::new(0., 0., 1280., 800.),
            ink.nodes([352., 210.], 100),
        ))
        .unwrap();
        assert!(!super::super::render_scene(&scene).vertices.is_empty());
        assert!(ink.receive(b"invalid", 2000).is_err());
    }
}
