// SPDX-License-Identifier: LGPL-3.0-only
//! Local one-axis text panning. Neither the viewport nor its animation is replicated.
use crate::dashboard::touch::Kinetics;
#[derive(Clone, Default)]
pub(super) struct TextViewport {
    pub offset: f32,
    limit: f32,
    target: Option<f32>,
    motion: Kinetics,
    last_tick: u64,
}
impl TextViewport {
    pub fn bounds(&mut self, content: f32, width: f32) {
        self.limit = (content - width).max(0.);
        self.offset = self.offset.clamp(0., self.limit);
        self.target = self.target.map(|t| t.clamp(0., self.limit));
        if self.limit == 0.
            || (self.offset == 0. && self.motion.velocity() < 0.)
            || (self.offset == self.limit && self.motion.velocity() > 0.)
        {
            self.motion.stop_at_bound();
        }
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
        self.motion.active() || self.target.is_some()
    }
    pub fn velocity(&self) -> f64 {
        self.motion.velocity()
    }
    pub fn limit(&self) -> f32 {
        self.limit
    }
    pub fn active(&self) -> bool {
        self.motion.touching()
    }
    pub fn owns(&self, id: u32) -> bool {
        self.motion.owns(id)
    }
    pub fn down(&mut self, id: u32, x: f32, now: u64) {
        self.target = None;
        let delta = self.motion.down(id, f64::from(x), now);
        self.shift(delta);
        self.last_tick = now;
    }
    fn shift(&mut self, delta: f64) {
        let next = self.offset + delta as f32;
        self.offset = next.clamp(0., self.limit);
        if next != self.offset || self.limit == 0. {
            self.motion.stop_at_bound();
        }
    }
    pub fn motion(&mut self, x: f32, now: u64) {
        let delta = self.motion.motion(f64::from(x), now);
        self.shift(delta);
    }
    /// True means a drag, so release must not move or focus the caret.
    pub fn up(&mut self, x: f32, now: u64) -> bool {
        let (dragged, correction) = self.motion.up(f64::from(x), now);
        if self.offset == 0. || self.offset == self.limit {
            self.motion.stop_at_bound();
        } else {
            self.shift(correction);
        }
        dragged
    }
    pub fn cancel(&mut self) {
        self.motion.cancel();
        self.target = None;
    }
    pub fn tick(&mut self, now: u64) -> bool {
        let was_active = self.animating();
        let dt = now.saturating_sub(self.last_tick) as f32;
        self.last_tick = self.last_tick.max(now);
        let old = self.offset;
        if let Some(target) = self.target {
            self.offset = target + (self.offset - target) * (-0.03 * dt).exp();
            if (self.offset - target).abs() < 0.25 {
                self.offset = target;
                self.target = None;
            }
        } else {
            let delta = self.motion.tick(now);
            self.shift(delta);
        }
        old != self.offset || was_active != self.animating()
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
        assert!(v.up(100., 50));
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
        v.up(2000., 200);
        assert!(!v.tick(500));
        assert_eq!(v.offset, 0.);
    }
    #[test]
    fn only_follow_requests_recover_caret_with_side_context() {
        let mut v = TextViewport::default();
        v.bounds(1000., 200.);
        v.down(1, 200., 0);
        v.motion(-200., 200);
        v.up(-200., 400); // A paused release has no momentum.
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
        v.up(100., 50);
        v.cancel();
        assert!(!v.tick(500));
        assert_eq!(v.offset, 100.);
        v.bounds(50., 200.);
        assert_eq!(v.offset, 0.);
        v.down(1, 200., 500);
        v.motion(-100., 550);
        v.up(-100., 550);
        assert!(!v.tick(1000));
        assert_eq!(v.offset, 0.);
    }
}
