// SPDX-License-Identifier: LGPL-3.0-only
//! Mount-local document presentation. The input policy mirrors the existing
//! text viewport, using f64 offsets; lift-off filtering is a separate follow-up.
use super::{Frame, Reader, Result, Source, Stats};
use crate::dashboard::{font::Font, scene::Scene};
use otrios_interfaces::{
    canvas::TextResource,
    scene::{PointerEvent, PointerPhase, Primitive},
};
use std::sync::Arc;

struct Contact {
    id: u32,
    start: f64,
    last: f64,
    time: u64,
    moving: bool,
}
pub(crate) struct Viewport {
    node: u32,
    source: Source,
    scale: u8,
    color: [u8; 4],
    reader: Option<Reader>,
    font_key: Option<(String, u32)>,
    display_epoch: u64,
    reader_epoch: u64,
    origin: [f32; 2],
    clip: [f32; 4],
    contact: Option<Contact>,
    velocity: f64,
    last_tick: u64,
    pub bounds_dirty: bool,
}
impl Viewport {
    pub fn new(node: u16, source: &TextResource, scale: u8, color: [u8; 4]) -> Result<Self> {
        Ok(Self {
            node: u32::from(node),
            source: Source::admit(source)?,
            scale,
            color,
            reader: None,
            font_key: None,
            display_epoch: 0,
            reader_epoch: 0,
            origin: [0.; 2],
            clip: [0.; 4],
            contact: None,
            velocity: 0.,
            last_tick: 0,
            bounds_dirty: true,
        })
    }
    pub fn sync(&mut self, scene: &Scene, font: Option<&Arc<Font>>) -> Result<()> {
        let Some(font) = font else {
            return Ok(());
        };
        let path = scene.path(self.node).ok_or("Missing document viewport")?;
        let p = path.last().ok_or("Missing document viewport frame")?;
        self.origin = p.origin;
        self.clip = [0., 0., scene.root().frame.width, scene.root().frame.height];
        for a in &path {
            if matches!(
                a.node.primitive,
                Primitive::Canvas { clip: true } | Primitive::Viewport { .. }
            ) {
                self.clip[0] = self.clip[0].max(a.origin[0]);
                self.clip[1] = self.clip[1].max(a.origin[1]);
                self.clip[2] = self.clip[2].min(a.origin[0] + a.node.frame.width * a.scale);
                self.clip[3] = self.clip[3].min(a.origin[1] + a.node.frame.height * a.scale);
            }
        }
        let width = f64::from(p.node.frame.width * p.scale);
        let height = f64::from(p.node.frame.height * p.scale);
        let scale = f32::from(self.scale) * p.scale;
        let key = (font.hash().to_owned(), scale.to_bits());
        if self.font_key.as_ref() != Some(&key) {
            let anchor = self.reader.as_ref().map(Reader::anchor);
            let mut reader = Reader::new(
                self.source.clone(),
                font.clone(),
                scale,
                self.color,
                width,
                height,
            )?;
            if let Some(anchor) = anchor {
                reader.jump_to(anchor)?;
            }
            self.reader = Some(reader);
            self.reader_epoch = 0;
            self.font_key = Some(key);
        } else if let Some(reader) = &mut self.reader {
            reader.resize(width, height)?;
        }
        self.bounds_dirty = false;
        Ok(())
    }
    pub fn touching(&self) -> bool {
        self.contact.is_some()
    }
    pub fn cancel(&mut self) {
        self.contact = None;
        self.velocity = 0.;
    }
    pub fn active(&self) -> bool {
        self.touching() || self.velocity != 0.
    }
    pub fn stats(&self) -> Option<Stats> {
        self.reader.as_ref().map(|r| {
            let mut stats = r.stats();
            stats.active = self.active();
            stats
        })
    }
    pub fn frame(&mut self) -> Option<Frame> {
        let mut frame = self.reader.as_mut()?.frame();
        if self.reader_epoch != frame.epoch {
            self.display_epoch = self
                .display_epoch
                .checked_add(1)
                .expect("Document presentation epoch exhausted");
            self.reader_epoch = frame.epoch;
        }
        frame.epoch = self.display_epoch;
        for draw in &mut frame.draws {
            draw.view[2] += self.origin[0];
            draw.view[3] += self.origin[1];
            draw.view[4..].copy_from_slice(&self.clip);
        }
        frame.draws.retain(|draw| {
            let b = draw.chunk.bounds;
            let [x, y] = [draw.view[2], draw.view[3]];
            self.clip[0] < self.clip[2]
                && self.clip[1] < self.clip[3]
                && b[0] + x < self.clip[2]
                && b[2] + x > self.clip[0]
                && b[1] + y < self.clip[3]
                && b[3] + y > self.clip[1]
        });
        Some(frame)
    }
    pub fn pointer(&mut self, scene: &Scene, event: PointerEvent, now: u64) -> Option<bool> {
        if event.position.iter().any(|v| !v.is_finite()) {
            return None;
        }
        if event.phase == PointerPhase::Down {
            if self.touching() {
                return Some(false);
            }
            let [x, y] = event.position;
            if x < self.clip[0] || y < self.clip[1] || x >= self.clip[2] || y >= self.clip[3] {
                return None;
            }
            // An empty external viewport has no primitive hit in Scene. It
            // replaces earlier background hits, but must respect later paint
            // and controls. Do this bounded UI walk only at contact capture.
            if let Some(hit) = scene.hit_path(event.position).last() {
                fn order(
                    n: &otrios_interfaces::scene::SceneNode,
                    doc: u32,
                    hit: u32,
                    next: &mut usize,
                    positions: &mut [usize; 2],
                ) {
                    if n.id == doc {
                        positions[0] = *next;
                    }
                    if n.id == hit {
                        positions[1] = *next;
                    }
                    *next += 1;
                    for child in &n.children {
                        order(child, doc, hit, next, positions);
                    }
                }
                let mut positions = [0; 2];
                order(scene.root(), self.node, hit.node.id, &mut 0, &mut positions);
                if positions[1] > positions[0] {
                    return None;
                }
            }
            self.cancel();
            self.last_tick = now;
            self.contact = Some(Contact {
                id: event.id,
                start: f64::from(y),
                last: f64::from(y),
                time: now,
                moving: false,
            });
        } else if self.contact.as_ref().is_none_or(|c| c.id != event.id) {
            return None;
        } else if event.phase == PointerPhase::Cancel {
            self.cancel();
        } else {
            if let (Some(c), Some(r)) = (&mut self.contact, &mut self.reader) {
                let y = f64::from(event.position[1]);
                c.moving |= (y - c.start).abs() > 6.;
                if c.moving && y != c.last {
                    let old = r.offset();
                    r.scroll_to(old + c.last - y);
                    self.velocity = ((r.offset() - old) / now.saturating_sub(c.time).max(1) as f64)
                        .clamp(-4., 4.);
                    if r.offset() == old {
                        self.velocity = 0.;
                    }
                    c.last = y;
                    c.time = now;
                }
            }
            if event.phase == PointerPhase::Up {
                if self
                    .contact
                    .as_ref()
                    .is_some_and(|c| now.saturating_sub(c.time) > 100)
                {
                    self.velocity = 0.;
                }
                self.contact = None;
                self.last_tick = now;
            }
        }
        Some(true)
    }
    pub fn tick(&mut self, now: u64) -> Result<bool> {
        let was_active = self.active();
        let dt = now.saturating_sub(self.last_tick) as f64;
        self.last_tick = now;
        let Some(r) = &mut self.reader else {
            return Ok(false);
        };
        let old = r.offset();
        if self.contact.is_none() && self.velocity != 0. {
            let decay = (-0.006 * dt).exp();
            r.scroll_to(old + self.velocity * (1. - decay) / 0.006);
            self.velocity *= decay;
            if self.velocity.abs() < 0.01 || r.offset() == old {
                self.velocity = 0.;
            }
        }
        let changed = r.work(16)?;
        let offset_changed = old != r.offset();
        Ok(changed || offset_changed || was_active != self.active())
    }
}
