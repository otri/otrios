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
