// SPDX-License-Identifier: LGPL-3.0-only
//! Local single-line panning. Neither the viewport nor its animation is replicated.
#[derive(Clone, Default)]
pub(super) struct TextViewport {
    pub offset: f32,
    limit: f32,
    target: Option<f32>,
    velocity: f32, // logical pixels per millisecond
    last_tick: u64,
    contact: Option<Contact>,
}
#[derive(Clone)]
struct Contact {
    id: u32,
    start: f32,
    last: f32,
    time: u64,
    moving: bool,
}
impl TextViewport {
    pub fn bounds(&mut self, content: f32, width: f32) {
        self.limit = (content - width).max(0.);
        self.offset = self.offset.clamp(0., self.limit);
        self.target = self.target.map(|t| t.clamp(0., self.limit));
    }
    pub fn follow(&mut self, caret: f32, marker: f32, width: f32, now: u64) {
        self.cancel();
        let margin = 48_f32.min(width * 0.2);
        let target = if caret < self.offset + margin {
            caret - margin
        } else if caret + marker > self.offset + width - margin {
            caret + marker + margin - width
        } else {
            self.offset
        }
        .clamp(0., self.limit);
        if (target - self.offset).abs() > 0.1 {
            self.target = Some(target);
        }
        self.last_tick = now;
    }
    pub fn animating(&self) -> bool {
        self.contact.is_some() || self.target.is_some() || self.velocity != 0.
    }
    pub fn limit(&self) -> f32 {
        self.limit
    }
    pub fn active(&self) -> bool {
        self.contact.is_some()
    }
    pub fn owns(&self, id: u32) -> bool {
        self.contact.as_ref().is_some_and(|c| c.id == id)
    }
    pub fn down(&mut self, id: u32, x: f32, now: u64) {
        self.cancel();
        self.last_tick = now;
        self.contact = Some(Contact {
            id,
            start: x,
            last: x,
            time: now,
            moving: false,
        });
    }
    pub fn motion(&mut self, x: f32, now: u64) {
        let Some(c) = &mut self.contact else { return };
        if (x - c.start).abs() > 6. {
            c.moving = true;
        }
        if c.moving {
            let next = (self.offset + c.last - x).clamp(0., self.limit);
            if x != c.last {
                let dt = now.saturating_sub(c.time).max(1) as f32;
                self.velocity = ((next - self.offset) / dt).clamp(-4., 4.);
                c.last = x;
                c.time = now;
            }
            self.offset = next;
            if self.offset == 0. || self.offset == self.limit {
                self.velocity = 0.;
            }
        }
    }
    /// True means a drag, so release must not move or focus the caret.
    pub fn up(&mut self, now: u64) -> bool {
        let Some(c) = self.contact.take() else {
            return false;
        };
        if now.saturating_sub(c.time) > 100 {
            self.velocity = 0.;
        }
        self.last_tick = now;
        c.moving
    }
    pub fn cancel(&mut self) {
        self.contact = None;
        self.velocity = 0.;
        self.target = None;
    }
    pub fn tick(&mut self, now: u64) -> bool {
        let dt = now.saturating_sub(self.last_tick) as f32;
        self.last_tick = now;
        let old = self.offset;
        if self.contact.is_none() {
            if let Some(target) = self.target {
                self.offset = target + (self.offset - target) * (-0.03 * dt).exp();
                if (self.offset - target).abs() < 0.25 {
                    self.offset = target;
                    self.target = None;
                }
            } else if self.velocity != 0. {
                let decay = (-0.006 * dt).exp();
                self.offset += self.velocity * (1. - decay) / 0.006;
                self.velocity *= decay;
                if self.velocity.abs() < 0.01 {
                    self.velocity = 0.;
                }
            }
            self.offset = self.offset.clamp(0., self.limit);
            if self.offset == 0. || self.offset == self.limit {
                self.velocity = 0.;
            }
        }
        old != self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_coasts_and_hard_stops_without_a_reverse_dead_zone() {
        let mut v = TextViewport::default();
        v.bounds(1000., 200.);
        v.down(7, 200., 0);
        v.motion(100., 50);
        assert_eq!(v.offset, 100.);
        assert!(v.up(50));
        assert!(v.tick(100));
        assert!(v.offset > 100. && v.offset < 800.);
        v.down(7, 200., 100);
        v.motion(-2000., 150);
        assert_eq!(v.offset, 800.);
        v.motion(-1990., 160);
        assert_eq!(
            v.offset, 790.,
            "reversing at the hard stop responds immediately"
        );
        v.motion(2000., 200);
        assert_eq!(v.offset, 0.);
        v.up(200);
        assert!(!v.tick(500));
        assert_eq!(v.offset, 0.);
    }
    #[test]
    fn only_follow_requests_recover_caret_with_side_context() {
        let mut v = TextViewport::default();
        v.bounds(1000., 200.);
        v.down(1, 200., 0);
        v.motion(-200., 200);
        v.up(400); // A paused release has no momentum.
        assert_eq!(v.offset, 400.);
        assert!(!v.tick(900));
        v.follow(20., 5., 200., 900);
        v.tick(1050);
        assert!(v.offset < 5., "caret recovery is rapid");
        v.tick(1200);
        assert_eq!(v.offset, 0.);
        v.follow(500., 5., 200., 1200);
        v.tick(1600);
        assert_eq!(v.offset, 345.);
        assert_eq!(200. - (500. + 5. - v.offset), 40.);
    }
    #[test]
    fn cancel_short_text_and_bounds_changes_stop_motion() {
        let mut v = TextViewport::default();
        v.bounds(1000., 200.);
        v.down(1, 200., 0);
        v.motion(100., 50);
        v.up(50);
        v.cancel();
        assert!(!v.tick(500));
        assert_eq!(v.offset, 100.);
        v.bounds(50., 200.);
        assert_eq!(v.offset, 0.);
        v.down(1, 200., 500);
        v.motion(-100., 550);
        v.up(550);
        assert!(!v.tick(1000));
        assert_eq!(v.offset, 0.);
    }
}
