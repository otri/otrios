// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded contact trajectories and shared scroll physics. Times are monotonic
//! sample milliseconds, not queue-delivery times. Positions remain unsmoothed
//! during contact; only the terminal contact is corrected against its history.
use std::collections::VecDeque;
pub const HISTORY_MS: u64 = 200;
pub const MAX_SAMPLES: usize = 64;
pub const MAX_STROKE_SPEED: f64 = 4.; // logical px/ms
pub const MAX_MOMENTUM_SPEED: f64 = 12.;
const DRAG: f64 = 0.006;
#[derive(Clone, Copy, Debug)]
pub struct Sample<const N: usize> {
    pub time_ms: u64,
    pub position: [f64; N],
}
#[derive(Clone, Copy, Debug)]
pub struct Estimate<const N: usize> {
    pub position: [f64; N],
    pub velocity: [f64; N],
    pub acceleration: [f64; N],
}
#[derive(Clone, Default)]
pub struct History<const N: usize> {
    samples: VecDeque<Sample<N>>,
}
impl<const N: usize> History<N> {
    pub fn push(&mut self, time_ms: u64, position: [f64; N]) -> bool {
        if position
            .iter()
            .any(|p| !p.is_finite() || p.abs() > 1_000_000_000.)
            || self.samples.back().is_some_and(|s| time_ms < s.time_ms)
        {
            return false;
        }
        // Retain at most one sample per 4 ms bucket, preserving the initial
        // contact. High-rate devices cannot crowd the entire 200 ms window out.
        if self.samples.back().is_some_and(|s| s.time_ms == time_ms)
            || (self.samples.len() > 1
                && self
                    .samples
                    .back()
                    .is_some_and(|s| s.time_ms / 4 == time_ms / 4))
        {
            self.samples.pop_back();
        }
        self.samples.push_back(Sample { time_ms, position });
        while self.samples.len() > MAX_SAMPLES
            || self
                .samples
                .front()
                .is_some_and(|s| time_ms - s.time_ms > HISTORY_MS)
        {
            self.samples.pop_front();
        }
        true
    }
    pub fn last(&self) -> Option<Sample<N>> {
        self.samples.back().copied()
    }
    fn at(&self, time: f64) -> [f64; N] {
        let first = self.samples.front().unwrap();
        let mut a = first;
        for b in &self.samples {
            if b.time_ms as f64 >= time {
                let dt = b.time_ms.saturating_sub(a.time_ms) as f64;
                let f = if dt == 0. {
                    0.
                } else {
                    (time - a.time_ms as f64) / dt
                };
                return std::array::from_fn(|i| {
                    a.position[i] + f * (b.position[i] - a.position[i])
                });
            }
            a = b;
        }
        a.position
    }
    /// Fit before release; the Up coordinate never supplies a velocity impulse.
    /// Uniform time samples prevent device/report rate from weighting the fit.
    pub fn estimate(&self, now: u64) -> Option<Estimate<N>> {
        let last = self.last()?;
        let mut estimate = Estimate {
            position: last.position,
            velocity: [0.; N],
            acceleration: [0.; N],
        };
        let age = now.checked_sub(last.time_ms)?;
        let start = self
            .samples
            .front()?
            .time_ms
            .max(last.time_ms.saturating_sub(120));
        let span = last.time_ms - start;
        if age >= 80 || span < 12 {
            return Some(estimate);
        }
        let origin = self.at(start as f64);
        let count = (span / 4).min(30) as usize;
        let mut points: Vec<(f64, [f64; N])> = Vec::with_capacity(count + 1);
        for j in 0..=count {
            let t = start as f64 + span as f64 * j as f64 / count as f64;
            let p = self.at(t);
            points.push((
                (t - last.time_ms as f64) / 100.,
                std::array::from_fn(|i| p[i] - origin[i]),
            ));
        }
        // Box-filter the final 16 ms residual against a pre-terminal quadratic
        // trajectory. A collapsing contact's last pixels cannot dominate a fit.
        // Do not interpolate an untrusted terminal sample into the prefix fit
        // on a slow device: exclude its complete raw sampling interval too.
        let trusted_end = self
            .samples
            .iter()
            .rev()
            .find(|s| s.time_ms <= last.time_ms.saturating_sub(16))
            .map_or(start, |s| s.time_ms);
        let cut_at = -((last.time_ms - trusted_end) as f64) / 100.;
        let cut = points.partition_point(|p| p.0 <= cut_at);
        if cut >= 5 && span >= 48 {
            let base = fit(&points[..cut]);
            let mut residual = [0.; N];
            let mut noise = [0.; N];
            for (t, p) in &points[..cut] {
                for i in 0..N {
                    noise[i] += (p[i] - eval(base[i], *t)).powi(2);
                }
            }
            for (t, p) in &points[cut..] {
                for i in 0..N {
                    residual[i] += p[i] - eval(base[i], *t);
                }
            }
            let n = (points.len() - cut) as f64;
            for i in 0..N {
                let tolerance = 2. + 3. * (noise[i] / cut as f64).sqrt();
                residual[i] = (residual[i] / n).clamp(-tolerance, tolerance);
            }
            for (t, p) in &mut points[cut..] {
                for i in 0..N {
                    p[i] = eval(base[i], *t) + residual[i];
                }
            }
        }
        let first = points.partition_point(|p| p.0 < -0.8);
        let coefficients = fit(&points[first..]);
        let horizon = age.min(24) as f64;
        let freshness = ((80 - age) as f64 / 56.).min(1.);
        for i in 0..N {
            let c = coefficients[i];
            let acceleration = (2. * c[2] / 10_000.).clamp(-0.04, 0.04);
            let velocity = c[1] / 100.;
            let projected = velocity + acceleration * horizon;
            estimate.position[i] = origin[i] + eval(c, horizon / 100.);
            estimate.acceleration[i] = acceleration;
            estimate.velocity[i] = if projected * velocity < 0. {
                0.
            } else {
                projected.clamp(-MAX_STROKE_SPEED, MAX_STROKE_SPEED) * freshness
            };
            // A stationary tail or a confirmed last-moment reversal must never
            // launch in the old direction because of an older polynomial fit.
            let tail = last.time_ms.saturating_sub(24).max(start);
            let delta = last.position[i] - self.at(tail as f64)[i];
            if last.time_ms - tail >= 16
                && (delta.abs() < 0.25 || delta * estimate.velocity[i] < 0.)
            {
                estimate.velocity[i] = 0.;
            }
        }
        Some(estimate)
    }
    pub fn release(&self, raw: [f64; N], now: u64) -> Option<Estimate<N>> {
        if self
            .last()?
            .time_ms
            .saturating_sub(self.samples.front()?.time_ms)
            < 12
        {
            return None;
        }
        let mut e = self.estimate(now)?;
        let last = self.last()?;
        if now.saturating_sub(last.time_ms) < 80 {
            for (i, p) in raw.into_iter().enumerate() {
                if p.is_finite() && e.velocity[i].abs() > 0.01 {
                    e.position[i] += (p - e.position[i]).clamp(-4., 4.) * 0.5;
                }
            }
        }
        Some(e)
    }
}
fn eval(c: [f64; 3], t: f64) -> f64 {
    c[0] + t * (c[1] + t * c[2])
}
fn fit<const N: usize>(points: &[(f64, [f64; N])]) -> [[f64; 3]; N] {
    let mut a = [[0.; 3]; 3];
    let mut b = [[0.; N]; 3];
    let degree = if points.len() >= 5 { 3 } else { 2 };
    for (t, p) in points {
        let x = [1., *t, t * t];
        for row in 0..degree {
            for col in 0..degree {
                a[row][col] += x[row] * x[col];
            }
            for (i, v) in p.iter().enumerate() {
                b[row][i] += x[row] * v;
            }
        }
    }
    for k in 0..degree {
        let pivot = (k..degree)
            .max_by(|i, j| a[*i][k].abs().total_cmp(&a[*j][k].abs()))
            .unwrap();
        a.swap(k, pivot);
        b.swap(k, pivot);
        if a[k][k].abs() < 1e-12 {
            return [[0.; 3]; N];
        }
        let scale = a[k][k];
        for col in k..degree {
            a[k][col] /= scale;
        }
        for i in 0..N {
            b[k][i] /= scale;
        }
        for row in 0..degree {
            if row == k {
                continue;
            }
            let scale = a[row][k];
            for col in k..degree {
                a[row][col] -= scale * a[k][col];
            }
            for i in 0..N {
                b[row][i] -= scale * b[k][i];
            }
        }
    }
    std::array::from_fn(|i| [b[0][i], b[1][i], b[2][i]])
}

#[derive(Clone)]
struct Contact {
    id: u32,
    start: f64,
    last: f64,
    at: u64,
    moving: bool,
    carry: f64,
    reverse_distance: f64,
    history: History<1>,
}
#[derive(Clone, Default)]
pub struct Kinetics {
    contact: Option<Contact>,
    velocity: f64,
    released_at: u64,
    released_velocity: f64,
    last_tick: u64,
}
impl Kinetics {
    pub fn touching(&self) -> bool {
        self.contact.is_some()
    }
    pub fn owns(&self, id: u32) -> bool {
        self.contact.as_ref().is_some_and(|c| c.id == id)
    }
    pub fn active(&self) -> bool {
        self.touching() || self.velocity != 0.
    }
    pub fn velocity(&self) -> f64 {
        self.velocity
    }
    pub fn cancel(&mut self) {
        self.contact = None;
        self.velocity = 0.;
        self.released_velocity = 0.;
    }
    pub fn stop_at_bound(&mut self) {
        self.velocity = 0.;
        self.released_velocity = 0.;
        if let Some(c) = &mut self.contact {
            c.carry = 0.;
        }
    }
    /// Returns displacement up to sample time, including a correction when a
    /// queued touch-down arrives after the presentation clock already advanced.
    pub fn down(&mut self, id: u32, position: f64, now: u64) -> f64 {
        let (carry, correction) = if self.released_velocity != 0. && now >= self.released_at {
            let at = self.released_velocity * (-DRAG * (now - self.released_at) as f64).exp();
            let shown = self.released_velocity
                * (-DRAG * self.last_tick.saturating_sub(self.released_at) as f64).exp();
            (at, (shown - at) / DRAG)
        } else {
            (0., 0.)
        };
        self.velocity = 0.;
        self.released_velocity = 0.;
        self.last_tick = now;
        let mut history = History::default();
        history.push(now, [position]);
        self.contact = Some(Contact {
            id,
            start: position,
            last: position,
            at: now,
            moving: false,
            carry,
            reverse_distance: 0.,
            history,
        });
        correction
    }
    pub fn motion(&mut self, position: f64, now: u64) -> f64 {
        let Some(c) = &mut self.contact else {
            return 0.;
        };
        if !c.history.push(now, [position]) {
            return 0.;
        }
        c.moving |= (position - c.start).abs() > 6.;
        if !c.moving {
            return 0.;
        }
        let delta = c.last - position;
        c.last = position;
        if delta * c.carry < 0. {
            c.reverse_distance += delta.abs();
        } else {
            c.reverse_distance = 0.;
        }
        if c.reverse_distance > 6. {
            c.carry = 0.;
        }
        delta
    }
    pub fn up(&mut self, position: f64, now: u64) -> (bool, f64) {
        let Some(c) = self.contact.take() else {
            return (false, 0.);
        };
        self.last_tick = now;
        if !c.moving {
            return (false, 0.);
        }
        let duration = now.saturating_sub(c.at);
        let e = c.history.release([position], now).unwrap_or_else(|| {
            // A long hold followed by only one reported move has no fitting
            // window. Use its conservative whole-contact average, never a 1 ms
            // terminal difference. No movement timing means no fling.
            let fresh = c
                .history
                .last()
                .is_some_and(|s| now >= s.time_ms && now - s.time_ms < 80);
            let velocity = if fresh && duration >= 12 {
                ((c.last - c.start) / duration as f64).clamp(-MAX_STROKE_SPEED, MAX_STROKE_SPEED)
            } else {
                0.
            };
            Estimate {
                position: [c.last],
                velocity: [velocity],
                acceleration: [0.],
            }
        });
        let speed = -e.velocity[0];
        let retained = c.carry * (-DRAG * duration as f64).exp();
        let carry = if duration <= 250
            && (c.last - c.start).abs() >= 24.
            && speed.abs() >= 0.15_f64.max(retained.abs() * 0.25)
            && speed * retained > 0.
        {
            retained
        } else {
            0.
        };
        self.velocity = (speed + carry).clamp(-MAX_MOMENTUM_SPEED, MAX_MOMENTUM_SPEED);
        self.released_at = now;
        self.released_velocity = self.velocity;
        (true, c.last - e.position[0])
    }
    pub fn tick(&mut self, now: u64) -> f64 {
        let dt = now.saturating_sub(self.last_tick) as f64;
        self.last_tick = self.last_tick.max(now);
        if self.touching() || self.velocity == 0. {
            return 0.;
        }
        let decay = (-DRAG * dt).exp();
        let delta = self.velocity * (1. - decay) / DRAG;
        self.velocity *= decay;
        if self.velocity.abs() < 0.01 {
            self.velocity = 0.;
            self.released_velocity = 0.;
        }
        delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn irregular_samples_recover_position_velocity_and_acceleration_before_lift() {
        let mut h = History::<2>::default();
        for t in [0, 7, 16, 30, 49, 61, 79, 100, 120] {
            let x = t as f64;
            h.push(t, [100. + 0.2 * x + 0.001 * x * x, 500. - 0.5 * x]);
        }
        let e = h.release([1138., -1000.], 121).unwrap();
        assert!((e.velocity[0] - 0.442).abs() < 0.025, "{e:?}");
        assert!((e.acceleration[0] - 0.002).abs() < 0.0005, "{e:?}");
        assert!((e.velocity[1] + 0.5).abs() < 0.01, "{e:?}");
        assert!((e.position[0] - 138.841).abs() < 2.2, "{e:?}");
        assert!((e.position[1] - 439.5).abs() < 2.2, "{e:?}");
    }
    #[test]
    fn noisy_terminal_contacts_do_not_kick_a_slow_release() {
        for step in [8, 10, 16, 25, 33] {
            for noise in [-35., 35.] {
                let end = 200 / step * step;
                let mut h = History::<1>::default();
                for t in (0..=end).step_by(step as usize) {
                    h.push(
                        t,
                        [100. + t as f64 * 0.08 + if t == end { noise } else { 0. }],
                    );
                }
                let e = h
                    .release([100. + end as f64 * 0.08 + noise * 2.], end + 1)
                    .unwrap();
                assert!(e.velocity[0].abs() < 0.3, "step={step} {e:?}");
                assert!(
                    (e.position[0] - (100. + end as f64 * 0.08)).abs() < 5.,
                    "step={step} {e:?}"
                );
            }
        }
    }
    #[test]
    fn stationary_tail_paused_release_and_reversal_do_not_revive_old_motion() {
        let mut h = History::<1>::default();
        for t in (0..=240).step_by(8) {
            h.push(t, [t.min(120) as f64]);
        }
        assert_eq!(h.release([500.], 241).unwrap().velocity, [0.]);
        assert_eq!(h.release([500.], 400).unwrap().position, [120.]);
        let mut h = History::<1>::default();
        for t in (0..=160).step_by(8) {
            h.push(t, [if t <= 128 { t as f64 } else { 256. - t as f64 }]);
        }
        assert!(h.release([96.], 160).unwrap().velocity[0] <= 0.);
    }
    fn stroke(k: &mut Kinetics, at: u64, direction: f64) -> f64 {
        k.down(1, 500., at);
        for dt in (8..=80).step_by(8) {
            k.motion(500. - direction * 4. * dt as f64, at + dt);
        }
        k.up(500. - direction * 320., at + 80);
        k.velocity()
    }
    #[test]
    fn repeated_strokes_add_decayed_momentum_but_taps_reversals_and_pauses_do_not() {
        let mut k = Kinetics::default();
        assert!((stroke(&mut k, 0, 1.) - 4.).abs() < 1e-6);
        for i in 1..8 {
            let speed = stroke(&mut k, i * 100, 1.);
            assert!(speed > 4. && speed <= MAX_MOMENTUM_SPEED);
        }
        assert!(k.velocity() > 8.);
        assert!((stroke(&mut k, 800, -1.) + 4.).abs() < 1e-6);
        k.down(1, 500., 900);
        assert_eq!(k.up(500., 920), (false, 0.));
        assert_eq!(k.velocity(), 0.);
        stroke(&mut k, 1000, 1.);
        k.down(1, 500., 1100);
        k.motion(400., 1180);
        k.up(400., 1300);
        assert_eq!(k.velocity(), 0.);
    }
    #[test]
    fn contact_history_is_bounded_by_time_and_capacity_and_rejects_old_samples() {
        let mut h = History::<1>::default();
        for t in 0..1600 {
            assert!(h.push(t, [t as f64]));
        }
        assert!(h.samples.len() <= MAX_SAMPLES);
        assert!(h.samples.front().unwrap().time_ms >= 1399);
        assert!(h.samples.front().unwrap().time_ms <= 1403);
        assert!(!h.push(1500, [0.]));
        assert!(!h.push(1600, [f64::NAN]));
        assert!((h.estimate(1599).unwrap().velocity[0] - 1.).abs() < 1e-8);
    }
    #[test]
    fn rapid_repeated_strikes_reach_but_never_exceed_the_momentum_cap() {
        let mut k = Kinetics::default();
        for stroke in 0..32 {
            let start = stroke * 20;
            k.down(1, 500., start);
            for dt in [4, 8, 12] {
                k.motion(500. - 4. * dt as f64, start + dt);
            }
            k.up(452., start + 12);
            assert!(k.velocity() <= MAX_MOMENTUM_SPEED);
        }
        assert_eq!(k.velocity(), MAX_MOMENTUM_SPEED);
    }
    #[test]
    fn duplicate_times_and_hard_stop_cannot_build_hidden_boost() {
        let mut k = Kinetics::default();
        stroke(&mut k, 0, 1.);
        k.down(1, 500., 100);
        k.motion(300., 150);
        k.motion(290., 150);
        k.stop_at_bound();
        k.up(290., 150);
        assert!(k.velocity() <= MAX_STROKE_SPEED);
        k.cancel();
        assert!(!k.active());
        assert_eq!(k.tick(1000), 0.);
    }
}
