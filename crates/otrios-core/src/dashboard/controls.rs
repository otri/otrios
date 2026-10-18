// SPDX-License-Identifier: LGPL-3.0-only
//! Small independent CRDT registers for the shared component playground.
use super::Result;
use automerge::{AutoCommit, ROOT, ReadDoc};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct State {
    pub level: u16,
    pub dial: u16,
    pub pad: [u16; 2],
}
impl Default for State {
    fn default() -> Self {
        Self {
            level: 650,
            dial: 350,
            pad: [500, 500],
        }
    }
}
impl State {
    pub fn read(doc: &AutoCommit) -> Result<Self> {
        let default = Self::default();
        let number = |key, default| -> Result<u16> {
            match doc.get(ROOT, key).map_err(|e| e.to_string())? {
                None => Ok(default),
                Some((value, _)) => value
                    .as_u64()
                    .filter(|v| *v <= 1000)
                    .map(|v| v as u16)
                    .ok_or_else(|| "Control value outside 0..1000".into()),
            }
        };
        let pad = match doc.get(ROOT, "control_pad_v1").map_err(|e| e.to_string())? {
            None => default.pad,
            Some((value, _)) => {
                let bytes = value
                    .as_bytes()
                    .filter(|v| v.len() == 4)
                    .ok_or("Invalid pad register")?;
                let point = [
                    u16::from_le_bytes([bytes[0], bytes[1]]),
                    u16::from_le_bytes([bytes[2], bytes[3]]),
                ];
                if point.iter().any(|v| *v > 1000) {
                    return Err("Pad outside 0..1000".into());
                }
                point
            }
        };
        Ok(Self {
            level: number("control_level_v1", default.level)?,
            dial: number("control_dial_v1", default.dial)?,
            pad,
        })
    }
    pub fn value(self, control: u8) -> Result<[u16; 2]> {
        match control {
            0 => Ok([self.level, 0]),
            1 => Ok([self.dial, 0]),
            2 => Ok(self.pad),
            _ => Err("Unknown control".into()),
        }
    }
}

use super::{input::Gesture, presentation::Model};
use otrios_interfaces::scene::{Action, PointerPhase};
use std::collections::{BTreeMap, VecDeque};
struct Drag {
    control: u8,
    value: [u16; 2],
    last: u64,
    dirty: bool,
}
#[derive(Default)]
pub struct Interactions {
    drags: BTreeMap<u32, Drag>,
    pub trail: VecDeque<[u16; 2]>,
    pub feedback: String,
}
impl Interactions {
    pub fn cancel(&mut self) {
        if !self.drags.is_empty() {
            self.feedback = "GESTURE CANCELLED".into();
        }
        self.drags.clear();
    }
    pub fn preview(&self, mut shared: State) -> State {
        for drag in self.drags.values() {
            match drag.control {
                0 => shared.level = drag.value[0],
                1 => shared.dial = drag.value[0],
                2 => shared.pad = drag.value,
                _ => {}
            }
        }
        shared
    }
    pub fn active(&self, control: u8) -> bool {
        self.drags.values().any(|d| d.control == control)
    }
    pub fn flush(&mut self, replica: &mut impl Model, now: u64) -> Result<bool> {
        let mut changed = false;
        for drag in self.drags.values_mut() {
            if drag.dirty && now.saturating_sub(drag.last) >= 50 {
                changed |= replica.set_control(drag.control, drag.value)?;
                drag.last = now;
                drag.dirty = false;
            }
        }
        Ok(changed)
    }
    pub fn gesture(
        &mut self,
        replica: &mut impl Model,
        pointer: u32,
        gesture: &Gesture,
        now: u64,
    ) -> Result<bool> {
        let Action::Control(control) = gesture.action else {
            return Ok(false);
        };
        if gesture.phase == PointerPhase::Cancel {
            let existed = self.drags.remove(&pointer).is_some();
            if existed {
                self.feedback = "CANCELLED / LAST SHARED VALUE RETAINED".into();
            }
            return Ok(existed);
        }
        if gesture.phase == PointerPhase::Down {
            if self.active(control) {
                return Ok(false);
            }
            self.drags.insert(
                pointer,
                Drag {
                    control,
                    value: replica.controls()?.value(control)?,
                    last: now,
                    dirty: false,
                },
            );
        }
        let Some(drag) = self.drags.get_mut(&pointer) else {
            return Ok(false);
        };
        if drag.control != control {
            return Ok(false);
        }
        let [x, y] = gesture.local;
        let [w, h] = gesture.extent;
        let quantize = |v: f32| (v.clamp(0., 1.) * 1000.).round() as u16;
        let value = match control {
            0 => [quantize((x - 20.) / (w - 40.).max(1.)), 0],
            1 => {
                let dx = x - w * 0.5;
                let dy = h * 0.5 - y;
                if dx.hypot(dy) < 12. {
                    drag.value
                } else {
                    [
                        quantize(
                            (dx.atan2(dy) + std::f32::consts::PI * 0.75)
                                / (std::f32::consts::PI * 1.5),
                        ),
                        0,
                    ]
                }
            }
            2 => [
                quantize((x - 14.) / (w - 28.).max(1.)),
                quantize(1. - (y - 14.) / (h - 28.).max(1.)),
            ],
            _ => return Err("Unknown gesture control".into()),
        };
        drag.dirty |= drag.value != value;
        drag.value = value;
        if control == 2 && self.trail.back() != Some(&value) {
            self.trail.push_back(value);
            while self.trail.len() > 12 {
                self.trail.pop_front();
            }
        }
        let name = ["SLIDER", "DIAL", "TOUCH PAD"][control as usize];
        self.feedback = format!(
            "{} / TOUCH {pointer} / {name}",
            if gesture.phase == PointerPhase::Up {
                "RELEASED"
            } else {
                "CAPTURED"
            }
        );
        if matches!(gesture.phase, PointerPhase::Down | PointerPhase::Up) {
            replica.set_control(control, value)?;
            drag.dirty = false;
            drag.last = now;
        }
        if gesture.phase == PointerPhase::Up {
            self.drags.remove(&pointer);
        } else {
            self.flush(replica, now)?;
        }
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::Replica;
    fn event(control: u8, phase: PointerPhase, p: [f32; 2]) -> Gesture {
        Gesture {
            action: Action::Control(control),
            phase,
            local: p,
            extent: [320., 200.],
        }
    }
    #[test]
    fn previews_are_local_and_publication_is_bounded_but_release_is_exact() {
        let mut r = Replica::new().unwrap();
        let mut input = Interactions::default();
        input
            .gesture(&mut r, 1, &event(0, PointerPhase::Down, [20., 100.]), 0)
            .unwrap();
        let first = r.snapshot().id();
        input
            .gesture(&mut r, 1, &event(0, PointerPhase::Move, [160., 100.]), 10)
            .unwrap();
        assert_eq!(r.snapshot().id(), first);
        assert_eq!(input.preview(r.controls().unwrap()).level, 500);
        assert!(!input.flush(&mut r, 49).unwrap());
        assert!(input.flush(&mut r, 50).unwrap());
        assert_eq!(r.controls().unwrap().level, 500);
        input
            .gesture(&mut r, 1, &event(0, PointerPhase::Up, [999., 100.]), 51)
            .unwrap();
        assert_eq!(r.controls().unwrap().level, 1000);
        assert!(!input.active(0));
    }
    #[test]
    fn cancellation_does_not_write_a_pending_preview_or_steal_another_pointer() {
        let mut r = Replica::new().unwrap();
        let mut input = Interactions::default();
        input
            .gesture(&mut r, 1, &event(2, PointerPhase::Down, [14., 186.]), 0)
            .unwrap();
        let before = r.snapshot().id();
        assert!(
            !input
                .gesture(&mut r, 2, &event(2, PointerPhase::Down, [306., 14.]), 1)
                .unwrap()
        );
        input
            .gesture(&mut r, 1, &event(2, PointerPhase::Move, [306., 14.]), 2)
            .unwrap();
        assert_eq!(input.preview(r.controls().unwrap()).pad, [1000, 1000]);
        input
            .gesture(&mut r, 1, &event(2, PointerPhase::Cancel, [306., 14.]), 3)
            .unwrap();
        assert_eq!(r.snapshot().id(), before);
        assert_eq!(input.preview(r.controls().unwrap()).pad, [0, 0]);
        assert!(!input.flush(&mut r, 1000).unwrap());
    }
}
