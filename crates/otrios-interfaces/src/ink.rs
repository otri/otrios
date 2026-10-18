// SPDX-License-Identifier: Apache-2.0
//! Ephemeral drawing presence, independent of durable Automerge history.
pub const MAGIC: &[u8; 6] = b"OTINK1";
pub const CLIENT_BYTES: usize = 15;
pub const SERVER_BYTES: usize = 21;
pub const LIFETIME_MS: u64 = 5000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub phase: u8,
    pub stroke: u32,
    pub position: [u16; 2],
}
impl Point {
    pub fn encode(self) -> Vec<u8> {
        let mut b = MAGIC.to_vec();
        b.push(self.phase);
        b.extend(self.stroke.to_le_bytes());
        for v in self.position {
            b.extend(v.to_le_bytes());
        }
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self, String> {
        if b.len() != CLIENT_BYTES || &b[..6] != MAGIC || b[6] > 3 {
            return Err("Invalid ink packet".into());
        }
        let p = Self {
            phase: b[6],
            stroke: u32::from_le_bytes(b[7..11].try_into().unwrap()),
            position: [
                u16::from_le_bytes(b[11..13].try_into().unwrap()),
                u16::from_le_bytes(b[13..15].try_into().unwrap()),
            ],
        };
        if p.stroke == 0 || p.position.iter().any(|n| *n > 1000) {
            return Err("Invalid ink coordinates".into());
        }
        Ok(p)
    }
}
pub fn envelope(peer: u32, age: u16, point: Point) -> Vec<u8> {
    let mut b = MAGIC.to_vec();
    b.extend(peer.to_le_bytes());
    b.extend(age.to_le_bytes());
    b.extend(&point.encode()[6..]);
    b
}
pub fn welcome(peer: u32) -> Vec<u8> {
    let mut b = envelope(
        peer,
        0,
        Point {
            phase: 0,
            stroke: 1,
            position: [0, 0],
        },
    );
    b[12] = 4;
    b
}
pub fn decode_server(b: &[u8]) -> Result<(u32, u16, Option<Point>), String> {
    if b.len() != SERVER_BYTES || &b[..6] != MAGIC {
        return Err("Invalid ink envelope".into());
    }
    let peer = u32::from_le_bytes(b[6..10].try_into().unwrap());
    let age = u16::from_le_bytes(b[10..12].try_into().unwrap());
    if peer == 0 || u64::from(age) > LIFETIME_MS {
        return Err("Invalid ink age or peer".into());
    }
    if b[12] == 4 {
        return Ok((peer, age, None));
    }
    let mut point = MAGIC.to_vec();
    point.extend(&b[12..]);
    Ok((peer, age, Some(Point::decode(&point)?)))
}
