// SPDX-License-Identifier: LGPL-3.0-only
//! Fixed glyph placement pilot; outlines and coverage remain the admitted font.
use super::{
    Result,
    fixed::{Scalar, rounded_ratio},
    font::Font,
};
use otrios_interfaces::scene::Color;

#[derive(Default)]
pub(super) struct GlyphMathCache {
    styles: Vec<(u32, Option<ScaledFont>)>,
    counts: [u64; 2],
}
struct ScaledFont {
    glyphs: Vec<ScaledGlyph>,
    max_x: i32,
}
struct ScaledGlyph {
    bounds: [i32; 4],
    em: [f32; 4],
    y: [f32; 2],
    advance: i64, // Q32.32: retain low bits across the entire run.
}
impl GlyphMathCache {
    pub fn counts(&self) -> [u64; 2] {
        self.counts
    }
    pub fn clear(&mut self) {
        self.styles.clear();
    }
    fn style(&mut self, font: &Font, scale: f32) -> Result<&ScaledFont> {
        let key = scale.to_bits();
        if let Some(index) = self.styles.iter().position(|(s, _)| *s == key) {
            return self.styles[index]
                .1
                .as_ref()
                .ok_or_else(|| "Fixed glyph scale outside local lane range".into());
        }
        if self.styles.len() == 8 {
            self.styles.clear();
        }
        self.styles.push((key, ScaledFont::new(font, scale).ok()));
        self.styles
            .last()
            .and_then(|(_, f)| f.as_ref())
            .ok_or_else(|| "Fixed glyph scale outside local lane range".into())
    }
}
impl ScaledFont {
    fn new(font: &Font, scale: f32) -> Result<Self> {
        let metrics = font.metrics();
        let scale = Scalar::from_f32(scale)?.raw();
        if scale <= 0 {
            return Err("Fixed glyph scale underflow".into());
        }
        let factor = rounded_ratio(i128::from(scale) * 8, i128::from(metrics.cap_height))?;
        let em = f32::from(metrics.units_per_em) * (factor as f32 / 4294967296.);
        if em <= 0. || !em.is_finite() {
            return Err("Fixed glyph em overflow".into());
        }
        let baseline = rounded_ratio(i128::from(metrics.ascent) * i128::from(factor), 65536)?;
        let mut glyphs = Vec::with_capacity(font.glyphs().len());
        for glyph in font.glyphs() {
            let b = glyph.ink_bounds.map(|v| v as i64);
            let units = [
                b[0],
                i64::from(metrics.ascent) - b[3],
                b[2],
                i64::from(metrics.ascent) - b[1],
            ];
            let mut bounds = [0; 4];
            for i in 0..4 {
                let value = rounded_ratio(i128::from(units[i]) * i128::from(factor), 65536)?;
                let pad = if i < 2 { -65536 } else { 65536 };
                bounds[i] = i32::try_from(
                    value
                        .checked_add(pad)
                        .ok_or("Fixed glyph bounds overflow")?,
                )
                .map_err(|_| "Fixed glyph lane overflow")?;
            }
            let positions = bounds.map(|v| v as f32 / 65536.);
            let baseline = baseline as f32 / 65536.;
            let em = [
                positions[0] / em,
                (baseline - positions[1]) / em,
                positions[2] / em,
                (baseline - positions[3]) / em,
            ];
            let advance = i64::try_from(i128::from(glyph.advance as i64) * i128::from(factor))
                .map_err(|_| "Fixed glyph advance overflow")?;
            glyphs.push(ScaledGlyph {
                bounds,
                em,
                y: [positions[1], positions[3]],
                advance,
            });
        }
        let max_x = glyphs
            .iter()
            .map(|g| g.bounds[0].max(g.bounds[2]))
            .max()
            .unwrap_or(0)
            .max(0);
        Ok(Self { glyphs, max_x })
    }
}

pub(super) fn append(
    font: &Font,
    cache: &mut GlyphMathCache,
    value: &str,
    scale: f32,
    color: Color,
    out: &mut Vec<f32>,
) -> Result<()> {
    let start = out.len();
    let result = (|| {
        let style = cache.style(font, scale)?;
        let tint = color.map(|v| f32::from(v) / 255.);
        // Admit the entire strip before emitting it. Advances are nonnegative
        // admitted font integers, so every intermediate pen is in [0,total].
        // Scaled glyph bounds already fit i32, Y never translates, and this
        // one maximum-X check therefore proves all four hot-loop lane sums.
        let mut indices = [0_usize; 257];
        let mut pens = [0_i64; 257];
        let mut count = 0;
        let mut total = 0_i64;
        for ch in value.chars() {
            let index = font.glyph_index(ch);
            if font.glyphs()[index].curve_count != 0 {
                if count == indices.len() {
                    return Err("Fixed text strip count overflow".into());
                }
                indices[count] = index;
                pens[count] = total;
                count += 1;
            }
            total = total
                .checked_add(style.glyphs[index].advance)
                .ok_or("Fixed text advance overflow")?;
        }
        let end = i32::try_from(
            total
                .checked_add(32768)
                .ok_or("Fixed text rounding overflow")?
                >> 16,
        )
        .map_err(|_| "Fixed text strip lane overflow")?;
        end.checked_add(style.max_x)
            .ok_or("Fixed text strip bounds overflow")?;
        let count = count.min((65536_usize.saturating_sub(out.len() / 16)) / 4);
        // Admit a complete strip, then convert contiguous lanes in one pass.
        // Keeping this separate from interleaved vertex emission lets LLVM use
        // NEON integer adds and vector conversion rather than scalar VFP casts.
        let mut pen = [0_i32; 257];
        let mut left = [0_i32; 257];
        let mut right = [0_i32; 257];
        let mut x0 = [0_f32; 257];
        let mut x1 = [0_f32; 257];
        for i in 0..count {
            let glyph = &style.glyphs[indices[i]];
            pen[i] = ((pens[i] + 32768) >> 16) as i32;
            left[i] = glyph.bounds[0];
            right[i] = glyph.bounds[2];
        }
        place_x(
            &pen[..count],
            &left[..count],
            &right[..count],
            &mut x0[..count],
            &mut x1[..count],
        );
        for i in 0..count {
            let index = indices[i];
            let source = &font.glyphs()[index];
            let glyph = &style.glyphs[index];
            let e = glyph.em;
            let [top, bottom] = glyph.y;
            for position in [
                [x0[i], top, e[0], e[1]],
                [x1[i], top, e[2], e[1]],
                [x0[i], bottom, e[0], e[3]],
                [x1[i], bottom, e[2], e[3]],
            ] {
                let band = source.band_transform;
                let data = source.band_data();
                out.extend([
                    position[0],
                    position[1],
                    position[2],
                    position[3],
                    tint[0],
                    tint[1],
                    tint[2],
                    tint[3],
                    band[0],
                    band[1],
                    band[2],
                    band[3],
                    data[0],
                    data[1],
                    data[2],
                    data[3],
                ]);
            }
        }
        Ok(())
    })();
    if result.is_err() {
        out.truncate(start);
        cache.counts[1] += 1;
    } else {
        cache.counts[0] += 1;
    }
    result
}

// Bounds were proved for the entire strip before entering this function.
// No unsafe intrinsics or wrapping arithmetic; scalar and SIMD share semantics.
#[inline(never)]
fn place_x(pen: &[i32], left: &[i32], right: &[i32], x0: &mut [f32], x1: &mut [f32]) {
    for ((((pen, left), right), x0), x1) in pen.iter().zip(left).zip(right).zip(x0).zip(x1) {
        *x0 = lane_float(*pen + *left);
        *x1 = lane_float(*pen + *right);
    }
}
#[inline]
fn lane_float(value: i32) -> f32 {
    // Every nonzero i32 converts to a normal f32 at least 1 in magnitude.
    // Scaling by 2^-16 therefore changes only its exponent, exactly. Avoiding
    // floating arithmetic lets strict ARM builds vectorize without fast-math
    // assumptions about subnormals or global rounding modes.
    let bits = (value as f32).to_bits();
    f32::from_bits(if value == 0 { 0 } else { bits - (16 << 23) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batch_conversion_matches_scalar_rounding_across_signed_lanes() {
        let mut values = vec![i32::MIN, i32::MAX, 0, 1, -1, 65535, 65536, -65537];
        let mut seed = 17_u32;
        for _ in 0..10000 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            values.push(seed as i32);
        }
        for v in values {
            assert_eq!(lane_float(v).to_bits(), (v as f32 / 65536.).to_bits());
        }
        let pen = [0, 65536, 1 << 24, -(1 << 24)];
        let left = [-65536, 1, -1, 0];
        let right = [65536, 65537, 0, 65536];
        let (mut a, mut b) = ([0.; 4], [0.; 4]);
        place_x(&pen, &left, &right, &mut a, &mut b);
        for i in 0..4 {
            assert_eq!(a[i], (pen[i] + left[i]) as f32 / 65536.);
            assert_eq!(b[i], (pen[i] + right[i]) as f32 / 65536.);
        }
    }
    #[test]
    fn accumulated_placement_stays_on_the_grid_of_the_high_precision_font_reference() {
        let font = Font::from_zstd(
            include_bytes!("../../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            super::super::font_reference::RESOURCE_SHA256,
        )
        .unwrap();
        let value = "jW ".repeat(80);
        for scale in [0.1_f32, 0.33, 1., 1.25, 2., 4., 8.] {
            let mut cache = GlyphMathCache::default();
            let mut out = vec![];
            append(&font, &mut cache, &value, scale, [255; 4], &mut out).unwrap();
            let factor = 8. * f64::from(scale) / f64::from(font.metrics().cap_height);
            let mut advance = 0_f64;
            let mut quads = out.chunks_exact(64);
            for ch in value.chars() {
                let glyph = font.glyph(ch);
                if glyph.curve_count != 0 {
                    let q = quads.next().unwrap();
                    let expected = advance + f64::from(glyph.ink_bounds[0]) * factor - 1.;
                    // Includes the final f32 presentation rounding at a long pen.
                    assert!(
                        (f64::from(q[0]) - expected).abs() < 0.0006,
                        "{scale}: {} vs {expected}",
                        q[0]
                    );
                }
                advance += f64::from(glyph.advance) * factor;
            }
            assert!(quads.next().is_none());
        }
    }
    #[test]
    fn normal_and_fractional_font_scales_use_fixed_lanes_without_fallback() {
        let font = Font::from_zstd(
            include_bytes!("../../../../web/shared/fonts/Roboto-Regular.otq.zst"),
            super::super::font_reference::RESOURCE_SHA256,
        )
        .unwrap();
        let mut cache = GlyphMathCache::default();
        for scale in [0.1, 0.33, 1., 1.25, 2., 4., 8.] {
            ScaledFont::new(&font, scale).unwrap();
            let mut vertices = vec![];
            append(
                &font,
                &mut cache,
                "The quick brown fox jumps over the lazy dog.",
                scale,
                [255; 4],
                &mut vertices,
            )
            .unwrap();
            assert!(!vertices.is_empty());
            assert!(vertices.iter().all(|v| v.is_finite()));
        }
        assert_eq!(cache.counts(), [7, 0]);
        let mut vertices = vec![123.];
        assert!(append(&font, &mut cache, "A", 1e8, [255; 4], &mut vertices).is_err());
        assert_eq!(
            vertices,
            [123.],
            "failed fixed admission leaves no partial mesh"
        );
    }
}
