// SPDX-License-Identifier: LGPL-3.0-only
//! Static Roboto Regular glyphs; tessellation happens in authoring tooling, never a frame.
//! Outlines retain the SIL Open Font License; the full notice is embedded with the data.
#[path = "roboto_data.rs"]
mod baked;
use baked::DATA;
const HEADER: usize = 16;
const RECORD: usize = 16;

fn u32_at(offset: usize) -> u32 {
    u32::from_le_bytes(DATA[offset..offset + 4].try_into().unwrap())
}
pub(super) fn notice() -> &'static str {
    let last = HEADER + (count() - 1) * RECORD;
    let end = HEADER + count() * RECORD + u32_at(last + 4) as usize + u32_at(last + 8) as usize * 4;
    std::str::from_utf8(&DATA[end..]).unwrap()
}
fn count() -> usize {
    u32_at(8) as usize
}

pub(super) struct Glyph {
    /// Width in logical units at text scale 1; proportional, including whitespace.
    pub advance: f32,
    triangles: &'static [u8],
    factor: f32,
}
impl Glyph {
    pub fn points(&self) -> impl Iterator<Item = [f32; 2]> + '_ {
        self.triangles.chunks_exact(4).map(|p| {
            [
                i16::from_le_bytes([p[0], p[1]]) as f32 * self.factor,
                i16::from_le_bytes([p[2], p[3]]) as f32 * self.factor,
            ]
        })
    }
}

/// Text scale 1 has an 8-unit capital height. Missing scalars use Roboto's `?`.
/// This deliberately bounded glyph set does not claim shaping, kerning, or bidi support.
pub(super) fn glyph(ch: char) -> Glyph {
    let (mut left, mut right) = (0, count());
    while left < right {
        let middle = left + (right - left) / 2;
        if u32_at(HEADER + middle * RECORD) < ch as u32 {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    let index = if left < count() && u32_at(HEADER + left * RECORD) == ch as u32 {
        left
    } else {
        ('?' as usize) - 32
    };
    let record = HEADER + index * RECORD;
    let start = HEADER + count() * RECORD + u32_at(record + 4) as usize;
    let bytes = u32_at(record + 8) as usize * 4;
    let factor = 8. / u32_at(12) as f32;
    Glyph {
        advance: u16::from_le_bytes([DATA[record + 12], DATA[record + 13]]) as f32 * factor,
        triangles: &DATA[start..start + bytes],
        factor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_font_is_bounded_and_has_proportional_advances() {
        assert_eq!(&DATA[..8], b"OTRFONT1");
        assert!(notice().contains("SIL OPEN FONT LICENSE Version 1.1"));
        assert!(notice().contains("The Roboto Project Authors"));
        assert!(u32_at(12) > 0);
        let mut previous = 0;
        for i in 0..count() {
            let record = HEADER + i * RECORD;
            let code = u32_at(record);
            assert!(code > previous);
            previous = code;
            assert_eq!(u32_at(record + 8) % 3, 0);
            let g = glyph(char::from_u32(code).unwrap());
            assert!(g.advance.is_finite() && g.advance > 0.);
            assert!(g.points().flatten().all(|v| v.is_finite() && v.abs() < 32.));
        }
        assert!(glyph('W').advance > 2. * glyph('i').advance);
        assert!(glyph(' ').points().next().is_none());
        assert!(glyph(' ').advance > 0.);
        assert_eq!(
            glyph('🦀').points().collect::<Vec<_>>(),
            glyph('?').points().collect::<Vec<_>>()
        );
    }
}
