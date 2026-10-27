// SPDX-License-Identifier: LGPL-3.0-only
//! Mount-local document presentation. The input policy mirrors the existing
//! text viewport, using f64 offsets and the shared contact-history physics.
use super::{Frame, Reader, Result, Source, Stats};
use crate::dashboard::touch::Kinetics;
use crate::dashboard::{font::Font, scene::Scene};
use otrios_interfaces::{
    canvas::TextResource,
    scene::{PointerEvent, PointerPhase, Primitive},
};
use std::sync::Arc;

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
    motion: Kinetics,
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
            motion: Kinetics::default(),
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
        self.motion.touching()
    }
    pub fn cancel(&mut self) {
        self.motion.cancel();
    }
    pub fn active(&self) -> bool {
        self.motion.active()
    }
    pub fn stats(&self) -> Option<Stats> {
        self.reader.as_ref().map(|r| {
            let mut stats = r.stats();
            stats.active = self.active();
            stats.velocity = self.motion.velocity();
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
            let delta = self.motion.down(event.id, f64::from(y), now);
            if let Some(r) = &mut self.reader {
                let wanted = r.offset() + delta;
                if wanted != r.offset() {
                    r.scroll_to(wanted);
                }
                if r.offset() != wanted {
                    self.motion.stop_at_bound();
                }
            }
        } else if !self.motion.owns(event.id) {
            return None;
        } else if event.phase == PointerPhase::Cancel {
            self.cancel();
        } else {
            let delta = if event.phase == PointerPhase::Up {
                self.motion.up(f64::from(event.position[1]), now).1
            } else {
                self.motion.motion(f64::from(event.position[1]), now)
            };
            if let Some(r) = &mut self.reader {
                let at_bound = r.offset() == 0. || r.offset() == (r.extent().0 - r.height).max(0.);
                let wanted = r.offset()
                    + if event.phase == PointerPhase::Up && at_bound {
                        self.motion.stop_at_bound();
                        0.
                    } else {
                        delta
                    };
                if wanted != r.offset() {
                    r.scroll_to(wanted);
                }
                if r.offset() != wanted {
                    self.motion.stop_at_bound();
                }
                if r.extent().1
                    && ((r.offset() == 0. && self.motion.velocity() < 0.)
                        || (r.offset() == (r.extent().0 - r.height).max(0.)
                            && self.motion.velocity() > 0.))
                {
                    self.motion.stop_at_bound();
                }
            }
        }
        Some(true)
    }
    pub fn tick(&mut self, now: u64) -> Result<bool> {
        let was_active = self.active();
        let Some(r) = &mut self.reader else {
            return Ok(false);
        };
        let old = r.offset();
        let wanted = old + self.motion.tick(now);
        if wanted != r.offset() {
            r.scroll_to(wanted);
        }
        if r.offset() != wanted {
            self.motion.stop_at_bound();
        }
        let changed = r.work(16)?;
        let offset_changed = old != r.offset();
        Ok(changed || offset_changed || was_active != self.active())
    }
}
