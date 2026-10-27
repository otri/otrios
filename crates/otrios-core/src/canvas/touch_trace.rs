// SPDX-License-Identifier: LGPL-3.0-only
//! Portable, bounded pointer recordings for deterministic offline replay. A trace
//! contains input coordinates/timing only, never authority state or credentials.
use super::{Client, Result};
use otrios_interfaces::scene::{PointerEvent, PointerPhase};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub format: String,
    pub source: String,
    pub screen: String,
    pub control: String,
    pub viewport: [f32; 2],
    #[serde(default)]
    pub mapping: Option<Mapping>,
    pub points: Vec<Point>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub rotation: u16,
    pub surface: [u32; 2],
    pub calibration: [f32; 6],
}
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub at_ms: u64,
    pub phase: u8,
    pub id: u32,
    pub position: [f32; 2],
}
#[derive(Debug, Serialize)]
pub struct Replay {
    pub samples: usize,
    pub releases: Vec<Reading>,
    pub final_view: Reading,
    pub fields: Vec<(u16, u16)>,
}
#[derive(Debug, Serialize)]
pub struct Reading {
    pub at_ms: u64,
    pub offset: Option<[f64; 2]>,
    pub velocity: Option<[f64; 2]>,
    pub loading: bool,
}
impl Trace {
    pub fn validate(&self) -> Result<()> {
        if self.format != "otrios-touch-trace-v1"
            || self.points.is_empty()
            || self.points.len() > 16_384
            || self.source.len() > 96
            || self.control.len() > 96
            || !otrios_interfaces::canvas::valid_destination(&self.screen)
            || self
                .viewport
                .iter()
                .any(|n| !n.is_finite() || !(64. ..=4096.).contains(n))
        {
            return Err("Touch trace admission limits".into());
        }
        if let Some(m) = self.mapping {
            crate::dashboard::display::Surface::with_size(m.rotation, m.surface)?;
            crate::dashboard::display::Calibration::new(m.calibration)?;
        }
        let mut contacts = BTreeSet::new();
        let mut last = 0;
        for p in &self.points {
            if p.at_ms < last
                || p.at_ms > 120_000
                || p.position
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 100_000.)
            {
                return Err("Touch trace time/position".into());
            }
            last = p.at_ms;
            match p.phase {
                0 if contacts.len() < 16 && !contacts.contains(&p.id) => {
                    contacts.insert(p.id);
                }
                1 if contacts.contains(&p.id) => {}
                2 | 3 if contacts.remove(&p.id) => {}
                _ => return Err("Touch trace contact order".into()),
            }
        }
        if !contacts.is_empty() {
            return Err("Touch trace ends with active contacts; record an explicit cancel".into());
        }
        Ok(())
    }
    /// Replay into an already mounted disposable client. Zero batching delivers
    /// immediately; nonzero batches retain every source timestamp. No sleeping,
    /// hardware injection, network or persistent state is involved.
    pub fn replay(&self, client: &mut Client, batch_ms: u64) -> Result<Replay> {
        self.validate()?;
        if batch_ms > 200 {
            return Err("Touch replay batch limit".into());
        }
        if client.path() != self.screen {
            return Err("Trace screen differs from mounted app".into());
        }
        client.set_viewport(self.viewport)?;
        let mut result = Replay {
            samples: self.points.len(),
            releases: vec![],
            final_view: reading(client, 0),
            fields: vec![],
        };
        let mapping = self
            .mapping
            .map(|m| {
                Ok::<_, String>((
                    crate::dashboard::display::Surface::with_size(m.rotation, m.surface)?,
                    crate::dashboard::display::Calibration::new(m.calibration)?,
                ))
            })
            .transpose()?;
        let mut clock = 0;
        let mut next_tick = 16;
        for (i, p) in self.points.iter().enumerate() {
            let delivery = if batch_ms == 0 {
                p.at_ms
            } else {
                p.at_ms.div_ceil(batch_ms) * batch_ms
            };
            while next_tick < delivery {
                client.tick(next_tick, 0)?;
                next_tick += 16;
            }
            let phase = match p.phase {
                0 => PointerPhase::Down,
                1 => PointerPhase::Move,
                2 => PointerPhase::Up,
                _ => PointerPhase::Cancel,
            };
            client.pointer_at(
                client.epoch(),
                PointerEvent {
                    id: p.id,
                    phase,
                    position: if let Some((surface, calibration)) = mapping {
                        surface
                            .touch_for(calibration, p.position, self.viewport)
                            .ok_or("Invalid mapped touch")?
                    } else {
                        p.position
                    },
                },
                p.at_ms,
            )?;
            if p.phase == 2 {
                result.releases.push(reading(client, p.at_ms));
            }
            if self.points.get(i + 1).is_none_or(|q| {
                let at = if batch_ms == 0 {
                    q.at_ms
                } else {
                    q.at_ms.div_ceil(batch_ms) * batch_ms
                };
                at != delivery
            }) {
                client.tick(delivery, 0)?;
            }
            clock = delivery;
        }
        let end = clock + 2500;
        while clock < end {
            clock = (clock + 16).min(end);
            client.tick(clock, 0)?;
        }
        result.final_view = reading(client, clock);
        result.fields = (1..=32)
            .filter_map(|id| client.value(id).ok().map(|v| (id, v)))
            .collect();
        Ok(result)
    }
}
fn reading(client: &Client, at_ms: u64) -> Reading {
    let usage = client.resource_usage();
    let (offset, velocity, loading) = if let Some(s) = usage.text_scroll {
        (Some(s.offset.map(f64::from)), Some(s.velocity), false)
    } else if let Some(s) = usage.text_document {
        (Some([0., s.offset]), Some([0., s.velocity]), s.loading)
    } else {
        (None, None, false)
    };
    Reading {
        at_ms,
        offset,
        velocity,
        loading,
    }
}
