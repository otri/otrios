// SPDX-License-Identifier: LGPL-3.0-only
//! Validated, explicit quadratic font resources. Hosts cache compressed bytes and retain an
//! `Arc<Font>` for presentation; no font parser, decompressor or atlas build runs per frame.
use ruzstd::decoding::StreamingDecoder;
use sha2::{Digest, Sha256};
use std::io::Read;

pub const MAX_COMPRESSED_BYTES: usize = 1024 * 1024;
pub const MAX_DECODED_BYTES: usize = 2 * 1024 * 1024;
const HEADER: usize = 40;
const RECORD: usize = 32;
const BANDS: usize = 8;
const TEXTURE_WIDTH: usize = 256;
const MAX_GLYPHS: usize = 1024;
const MAX_CURVES: usize = 16384;
const MAX_BAND_CURVES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub units_per_em: u16,
    pub ascent: i16,
    /// Negative, below the baseline in font coordinates (+Y up).
    pub descent: i16,
    pub line_gap: i16,
    pub cap_height: i16,
}
impl Metrics {
    /// Existing UI text scale means an eight-logical-unit capital height.
    pub fn scale(self, text_scale: f32) -> f32 {
        8. * text_scale / f32::from(self.cap_height)
    }
    pub fn baseline_from_top(self, text_scale: f32) -> f32 {
        f32::from(self.ascent) * self.scale(text_scale)
    }
    pub fn line_height(self, text_scale: f32) -> f32 {
        (f32::from(self.ascent) - f32::from(self.descent) + f32::from(self.line_gap))
            * self.scale(text_scale)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Curve {
    /// Quadratic p0/control/p2, in font units with +Y up. Lines have midpoint controls.
    pub points: [[f32; 2]; 3],
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub scalar: char,
    pub id: u32,
    pub advance: f32,
    /// xmin/ymin/xmax/ymax in font units, including negative side bearings and descenders.
    pub ink_bounds: [f32; 4],
    pub curve_start: u32,
    pub curve_count: u32,
    /// First of eight horizontal then eight vertical band records in the band texture.
    pub band_offset: u16,
    /// Normalized-em coordinate -> band coordinate (scaleX,scaleY,offsetX,offsetY).
    pub band_transform: [f32; 4],
}
impl Glyph {
    pub fn band_data(&self) -> [f32; 4] {
        [f32::from(self.band_offset), 7., 7., 0.]
    }
}

#[derive(Debug)]
pub struct Atlas {
    pub curve_width: u32,
    pub curve_height: u32,
    /// RGBA32F source texels, two per curve: (p0.xy,p1.xy), (p2.xy,0,0), in em units.
    /// GLES hosts may convert once to RGBA16F if float32 textures are unavailable.
    pub curves: Vec<f32>,
    pub band_width: u32,
    pub band_height: u32,
    /// RGBA8 texels, each two little-endian u16 values. Metadata=(count,listOffset),
    /// list entries=(global curve texel offset,0). Indices are linear texel offsets.
    pub bands: Vec<u8>,
    pub max_band_curves: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct TextMetrics {
    pub advance: f32,
    /// Logical units relative to the baseline, +Y up. Includes negative side bearings.
    pub ink_bounds: [f32; 4],
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

#[derive(Debug)]
pub struct Font {
    metrics: Metrics,
    glyphs: Vec<Glyph>,
    curves: Vec<Curve>,
    fallback: usize,
    notice: String,
    atlas: Atlas,
    hash: String,
}
impl Font {
    /// The expected digest must come from an admitted resource reference, not the payload.
    /// Verify compressed identity before any decompression or GPU preparation.
    pub fn from_zstd(bytes: &[u8], expected_sha256: &str) -> Result<Self, String> {
        if bytes.is_empty() || bytes.len() > MAX_COMPRESSED_BYTES {
            return Err("Font compressed resource exceeds its bound".into());
        }
        if expected_sha256.len() != 64
            || !expected_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Invalid font resource SHA-256".into());
        }
        let hash = sha256(bytes);
        if hash != expected_sha256 {
            return Err("Font resource SHA-256 mismatch".into());
        }
        let declared = check_zstd_header(bytes)?;
        let mut decoder = StreamingDecoder::new(bytes).map_err(|e| format!("Font zstd: {e}"))?;
        let mut decoded = Vec::with_capacity(declared.min(MAX_DECODED_BYTES));
        (&mut decoder)
            .take(MAX_DECODED_BYTES as u64 + 1)
            .read_to_end(&mut decoded)
            .map_err(|e| format!("Font decompression failed: {e}"))?;
        if decoded.len() > MAX_DECODED_BYTES || decoded.len() != declared {
            return Err("Font decoded length exceeds bound or disagrees with frame".into());
        }
        if !decoder.get_ref().is_empty() || !decoder.decoder.is_finished() {
            return Err("Font resource must contain exactly one complete zstd frame".into());
        }
        if decoder.decoder.get_checksum_from_data().is_some()
            && decoder.decoder.get_checksum_from_data() != decoder.decoder.get_calculated_checksum()
        {
            return Err("Font zstd checksum mismatch".into());
        }
        Self::from_packet(&decoded, hash)
    }

    fn from_packet(bytes: &[u8], hash: String) -> Result<Self, String> {
        if bytes.len() < HEADER || bytes.len() > MAX_DECODED_BYTES || &bytes[..8] != b"OTRQFNT1" {
            return Err("Invalid quadratic font header".into());
        }
        let mut read = Reader::new(&bytes[8..]);
        let length = read.u32()? as usize;
        let metrics = Metrics {
            units_per_em: read.u16()?,
            ascent: read.i16()?,
            descent: read.i16()?,
            line_gap: read.i16()?,
            cap_height: read.i16()?,
        };
        let reserved = read.u16()?;
        let glyph_count = read.u32()? as usize;
        let curve_count = read.u32()? as usize;
        let notice_len = read.u32()? as usize;
        let fallback_id = read.u32()?;
        if length != bytes.len()
            || reserved != 0
            || !(1..=MAX_GLYPHS).contains(&glyph_count)
            || curve_count > MAX_CURVES
            || notice_len > 32768
            || !(16..=16384).contains(&metrics.units_per_em)
            || metrics.ascent <= 0
            || metrics.descent > 0
            || metrics.line_gap < 0
            || metrics.cap_height <= 0
            || metrics.cap_height > metrics.ascent
        {
            return Err("Invalid quadratic font limits or line metrics".into());
        }
        let exact = HEADER
            .checked_add(glyph_count * RECORD)
            .and_then(|n| n.checked_add(curve_count * 12))
            .and_then(|n| n.checked_add(notice_len))
            .ok_or("Font length overflow")?;
        if exact != bytes.len() {
            return Err("Font packet length mismatch".into());
        }
        let mut glyphs = Vec::with_capacity(glyph_count);
        let mut next_curve = 0;
        let mut previous = None;
        for _ in 0..glyph_count {
            let code = read.u32()?;
            let scalar = char::from_u32(code).ok_or("Invalid font Unicode scalar")?;
            let id = read.u32()?;
            let start = read.u32()?;
            let count = read.u32()?;
            let advance = f32::from(read.i16()?);
            let bounds = [read.i16()?, read.i16()?, read.i16()?, read.i16()?].map(f32::from);
            let zero = read.u16()?;
            let zero2 = read.u32()?;
            if previous.is_some_and(|p| p >= code)
                || start != next_curve
                || count > 512
                || advance < 0.
                || bounds[0] > bounds[2]
                || bounds[1] > bounds[3]
                || zero != 0
                || zero2 != 0
                || id > 65535
            {
                return Err("Invalid font glyph record".into());
            }
            next_curve = start
                .checked_add(count)
                .ok_or("Font curve range overflow")?;
            if next_curve as usize > curve_count {
                return Err("Font curve range exceeds packet".into());
            }
            previous = Some(code);
            glyphs.push(Glyph {
                scalar,
                id,
                advance,
                ink_bounds: bounds,
                curve_start: start,
                curve_count: count,
                band_offset: 0,
                band_transform: [0.; 4],
            });
        }
        if next_curve as usize != curve_count {
            return Err("Unreferenced font curves".into());
        }
        let fallback = glyphs
            .iter()
            .position(|g| g.id == fallback_id)
            .ok_or("Missing font fallback glyph")?;
        let mut curves = Vec::with_capacity(curve_count);
        for _ in 0..curve_count {
            let mut points = [[0.; 2]; 3];
            for point in &mut points {
                *point = [f32::from(read.i16()?), f32::from(read.i16()?)];
            }
            curves.push(Curve { points });
        }
        // Bounds are an admission invariant: glyph quads must cover every curve extremum.
        for glyph in &glyphs {
            for curve in &curves
                [glyph.curve_start as usize..(glyph.curve_start + glyph.curve_count) as usize]
            {
                for axis in 0..2 {
                    let p = curve.points.map(|p| p[axis]);
                    let mut extrema = vec![p[0], p[2]];
                    let denominator = p[0] - 2. * p[1] + p[2];
                    if denominator != 0. {
                        let t = (p[0] - p[1]) / denominator;
                        if (0. ..1.).contains(&t) {
                            extrema.push(
                                (1. - t).powi(2) * p[0] + 2. * (1. - t) * t * p[1] + t * t * p[2],
                            );
                        }
                    }
                    if extrema.iter().any(|v| {
                        *v < glyph.ink_bounds[axis] - 1. || *v > glyph.ink_bounds[axis + 2] + 1.
                    }) {
                        return Err("Font ink bounds do not contain quadratic curve".into());
                    }
                }
            }
        }
        let notice = String::from_utf8(read.take(notice_len)?.to_vec())
            .map_err(|_| "Invalid font notice UTF-8")?;
        if notice.is_empty() {
            return Err("Font license notice is missing".into());
        }
        let atlas = build_atlas(metrics, &mut glyphs, &curves)?;
        Ok(Self {
            metrics,
            glyphs,
            curves,
            fallback,
            notice,
            atlas,
            hash,
        })
    }
    pub fn metrics(&self) -> Metrics {
        self.metrics
    }
    pub fn glyph(&self, ch: char) -> &Glyph {
        let index = self
            .glyphs
            .binary_search_by_key(&(ch as u32), |g| g.scalar as u32)
            .unwrap_or(self.fallback);
        &self.glyphs[index]
    }
    pub fn glyph_by_id(&self, id: u32) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.id == id)
    }
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }
    pub fn curves(&self) -> &[Curve] {
        &self.curves
    }
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }
    pub fn notice(&self) -> &str {
        &self.notice
    }
    pub fn hash(&self) -> &str {
        &self.hash
    }
    pub fn text_width(&self, value: &str, scale: f32) -> f32 {
        value.chars().map(|c| self.glyph(c).advance).sum::<f32>() * self.metrics.scale(scale)
    }
    pub fn measure_text(&self, value: &str, scale: f32) -> TextMetrics {
        let mut x = 0.;
        let mut bounds: Option<[f32; 4]> = None;
        for ch in value.chars() {
            let g = self.glyph(ch);
            if g.curve_count != 0 {
                let b = [
                    x + g.ink_bounds[0],
                    g.ink_bounds[1],
                    x + g.ink_bounds[2],
                    g.ink_bounds[3],
                ];
                bounds = Some(bounds.map_or(b, |old| {
                    [
                        old[0].min(b[0]),
                        old[1].min(b[1]),
                        old[2].max(b[2]),
                        old[3].max(b[3]),
                    ]
                }));
            }
            x += g.advance;
        }
        let factor = self.metrics.scale(scale);
        TextMetrics {
            advance: x * factor,
            ink_bounds: bounds.unwrap_or([0.; 4]).map(|v| v * factor),
            ascent: f32::from(self.metrics.ascent) * factor,
            descent: f32::from(self.metrics.descent) * factor,
            line_gap: f32::from(self.metrics.line_gap) * factor,
        }
    }
}

fn build_atlas(metrics: Metrics, glyphs: &mut [Glyph], curves: &[Curve]) -> Result<Atlas, String> {
    let curve_height = (curves.len() * 2).max(1).div_ceil(TEXTURE_WIDTH);
    let mut curve_texels = vec![0.; TEXTURE_WIDTH * curve_height * 4];
    let em = f32::from(metrics.units_per_em);
    // Adjacent bands overlap conservatively so RGBA16F curve quantization cannot
    // move an edge into a neighboring band that omits it (Slug reference guidance).
    let band_epsilon = em / 1024.;
    for (i, curve) in curves.iter().enumerate() {
        let p = curve.points.map(|p| p.map(|v| v / em));
        curve_texels[i * 8..i * 8 + 8]
            .copy_from_slice(&[p[0][0], p[0][1], p[1][0], p[1][1], p[2][0], p[2][1], 0., 0.]);
    }
    let mut bands = vec![[0_u16; 2]; glyphs.len() * BANDS * 2];
    let mut max_band = 0;
    for (glyph_index, glyph) in glyphs.iter_mut().enumerate() {
        let base = glyph_index * BANDS * 2;
        glyph.band_offset = u16::try_from(base).map_err(|_| "Font band offset overflow")?;
        let bounds = glyph.ink_bounds.map(|v| v / em);
        let width = bounds[2] - bounds[0];
        let height = bounds[3] - bounds[1];
        if glyph.curve_count == 0 {
            continue;
        }
        if width <= 0. || height <= 0. {
            return Err("Drawable glyph has empty ink bounds".into());
        }
        let sx = BANDS as f32 / width;
        let sy = BANDS as f32 / height;
        glyph.band_transform = [sx, sy, -bounds[0] * sx, -bounds[1] * sy];
        let slice =
            &curves[glyph.curve_start as usize..(glyph.curve_start + glyph.curve_count) as usize];
        for direction in 0..2 {
            let axis = 1 - direction;
            let lo = bounds[axis] * em;
            let step = (bounds[axis + 2] - bounds[axis]) * em / BANDS as f32;
            for band in 0..BANDS {
                let min = lo + step * band as f32;
                let max = min + step;
                let mut indices: Vec<_> = slice
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| {
                        let p = c.points.map(|p| p[axis]);
                        if p[0] == p[1] && p[1] == p[2] {
                            return None;
                        }
                        let lower = p.into_iter().fold(f32::INFINITY, f32::min);
                        let upper = p.into_iter().fold(f32::NEG_INFINITY, f32::max);
                        if upper >= min - band_epsilon && lower <= max + band_epsilon {
                            Some(i)
                        } else {
                            None
                        }
                    })
                    .collect();
                if indices.len() > MAX_BAND_CURVES {
                    return Err("Font band exceeds shader curve bound".into());
                }
                max_band = max_band.max(indices.len());
                indices.sort_by(|a, b| {
                    let max = |i: usize| {
                        slice[i]
                            .points
                            .iter()
                            .map(|p| p[direction])
                            .fold(f32::NEG_INFINITY, f32::max)
                    };
                    max(*b).total_cmp(&max(*a)).then(a.cmp(b))
                });
                let offset =
                    u16::try_from(bands.len()).map_err(|_| "Font band texture offset overflow")?;
                bands[base + direction * BANDS + band] = [indices.len() as u16, offset];
                for i in indices {
                    bands.push([
                        u16::try_from((glyph.curve_start as usize + i) * 2)
                            .map_err(|_| "Font curve texture offset overflow")?,
                        0,
                    ]);
                }
                if bands.len() > 65535 {
                    return Err("Font band texture exceeds 16-bit address space".into());
                }
            }
        }
    }
    let band_height = bands.len().max(1).div_ceil(TEXTURE_WIDTH);
    bands.resize(TEXTURE_WIDTH * band_height, [0; 2]);
    let bands = bands
        .into_iter()
        .flat_map(|pair| pair.into_iter().flat_map(u16::to_le_bytes))
        .collect();
    Ok(Atlas {
        curve_width: TEXTURE_WIDTH as u32,
        curve_height: curve_height as u32,
        curves: curve_texels,
        band_width: TEXTURE_WIDTH as u32,
        band_height: band_height as u32,
        bands,
        max_band_curves: max_band as u16,
    })
}

/// Check memory-sensitive fields before constructing a decoder. This private resource
/// format requires an ordinary single zstd frame, known content size and no dictionary.
fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn check_zstd_header(bytes: &[u8]) -> Result<usize, String> {
    let mut r = Reader::new(bytes);
    if r.take(4)? != [0x28, 0xb5, 0x2f, 0xfd] {
        return Err("Invalid font zstd frame magic".into());
    }
    let flag = r.take(1)?[0];
    if flag & 0x1b != 0 {
        return Err("Unsupported font zstd reserved/dictionary flags".into());
    }
    let single = flag & 0x20 != 0;
    let window = if single {
        None
    } else {
        let wd = r.take(1)?[0];
        let base = 1_u64 << (10 + (wd >> 3));
        Some(base + (base / 8) * u64::from(wd & 7))
    };
    let fcs = match flag >> 6 {
        0 if single => 1,
        0 => 0,
        1 => 2,
        2 => 4,
        _ => 8,
    };
    if fcs == 0 {
        return Err("Font zstd frame must declare its decoded size".into());
    }
    let mut size = [0_u8; 8];
    size[..fcs].copy_from_slice(r.take(fcs)?);
    let size = u64::from_le_bytes(size) + if fcs == 2 { 256 } else { 0 };
    if size == 0
        || size > MAX_DECODED_BYTES as u64
        || window.unwrap_or(size) > MAX_DECODED_BYTES as u64
    {
        return Err("Font zstd window or decoded size exceeds bound".into());
    }
    Ok(size as usize)
}
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.position.checked_add(n).ok_or("Font offset overflow")?;
        let result = self
            .bytes
            .get(self.position..end)
            .ok_or("Truncated font resource")?;
        self.position = end;
        Ok(result)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

#[cfg(test)]
pub(crate) fn test_font() -> Font {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../web/dashboard/fonts/Roboto-Regular.otq.zst"),
    )
    .unwrap();
    Font::from_zstd(&bytes, super::font_reference::RESOURCE_SHA256).unwrap()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn compressed() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../web/dashboard/fonts/Roboto-Regular.otq.zst"),
        )
        .unwrap()
    }
    fn decoded() -> Vec<u8> {
        let bytes = compressed();
        let mut out = Vec::new();
        StreamingDecoder::new(bytes.as_slice())
            .unwrap()
            .read_to_end(&mut out)
            .unwrap();
        out
    }
    #[test]
    fn resource_metrics_preserve_descenders_accents_and_negative_bearings() {
        let f = test_font();
        assert_eq!(f.metrics.units_per_em, 4096);
        assert_eq!(f.metrics.ascent, 3800);
        assert_eq!(f.metrics.descent, -1000);
        assert!(f.glyph('j').ink_bounds[0] < 0.);
        assert!(f.glyph('g').ink_bounds[1] < 0.);
        assert!(f.glyph('É').ink_bounds[3] > f32::from(f.metrics.cap_height));
        let text = f.measure_text("jgyÉ", 3.);
        assert!(text.ink_bounds[0] < 0. && text.ink_bounds[1] < 0.);
        assert!(text.ink_bounds[3] > 24.);
        assert!(f.metrics.line_height(3.) > text.ink_bounds[3] - text.ink_bounds[1]);
        assert!(f.glyph('W').advance > 2. * f.glyph('i').advance);
        assert!(f.glyph(' ').curve_count == 0 && f.glyph(' ').advance > 0.);
        assert_eq!(f.glyph('🦀').id, 0);
        assert!(f.notice.contains("SIL OPEN FONT LICENSE Version 1.1"));
    }
    #[test]
    fn atlas_is_prepared_and_all_band_references_are_bounded() {
        let f = test_font();
        let a = f.atlas();
        assert_eq!(
            a.curves.len(),
            a.curve_width as usize * a.curve_height as usize * 4
        );
        assert_eq!(
            a.bands.len(),
            a.band_width as usize * a.band_height as usize * 4
        );
        assert!(a.curves.iter().all(|x| x.is_finite()));
        assert!(a.max_band_curves <= 128);
        for glyph in f.glyphs() {
            for band in 0..16 {
                let b = (usize::from(glyph.band_offset) + band) * 4;
                let count = u16::from_le_bytes([a.bands[b], a.bands[b + 1]]) as usize;
                let offset = u16::from_le_bytes([a.bands[b + 2], a.bands[b + 3]]) as usize;
                assert!(count <= 128);
                assert!((offset + count) * 4 <= a.bands.len());
                for i in offset..offset + count {
                    let curve = u16::from_le_bytes([a.bands[i * 4], a.bands[i * 4 + 1]]) as usize;
                    assert_eq!(curve % 2, 0);
                    assert!(curve / 2 >= glyph.curve_start as usize);
                    assert!(curve / 2 < (glyph.curve_start + glyph.curve_count) as usize);
                }
            }
        }
        println!(
            "font atlas: {}x{} curves,{}x{} bands,max {} curves/band",
            a.curve_width, a.curve_height, a.band_width, a.band_height, a.max_band_curves
        );
    }
    #[test]
    fn adjacent_bands_overlap_to_cover_half_float_rounding() {
        let metrics = Metrics {
            units_per_em: 1024,
            ascent: 800,
            descent: -224,
            line_gap: 0,
            cap_height: 700,
        };
        let mut glyphs = [Glyph {
            scalar: 'A',
            id: 1,
            advance: 1024.,
            ink_bounds: [0., 0., 1024., 1024.],
            curve_start: 0,
            curve_count: 1,
            band_offset: 0,
            band_transform: [0.; 4],
        }];
        // The curve begins one font unit above horizontal band 0's upper edge.
        // 1/1024 em overlap includes it, preventing cracks from half-float rounding.
        let curves = [Curve {
            points: [[512., 129.], [512., 200.], [512., 400.]],
        }];
        let atlas = build_atlas(metrics, &mut glyphs, &curves).unwrap();
        let count = u16::from_le_bytes([atlas.bands[0], atlas.bands[1]]);
        assert_eq!(count, 1);
    }
    #[test]
    fn corrupt_hash_truncation_and_trailing_frames_are_rejected() {
        let original = compressed();
        assert!(Font::from_zstd(&original, &"0".repeat(64)).is_err());
        for n in [0, 1, 4, 8, 20, original.len() - 1] {
            let bytes = &original[..n];
            let hash = sha256(bytes);
            assert!(Font::from_zstd(bytes, &hash).is_err());
        }
        let mut corrupt_checksum = original.clone();
        let last = corrupt_checksum.len() - 1;
        corrupt_checksum[last] ^= 1;
        assert!(
            Font::from_zstd(&corrupt_checksum, &sha256(&corrupt_checksum))
                .unwrap_err()
                .contains("checksum")
        );
        let mut doubled = original.clone();
        doubled.extend_from_slice(&original);
        let hash = sha256(&doubled);
        assert!(Font::from_zstd(&doubled, &hash).is_err());
        let mut bytes = decoded();
        bytes[8..12].copy_from_slice(&0_u32.to_le_bytes());
        assert!(Font::from_packet(&bytes, String::new()).is_err());
    }
    #[test]
    fn oversized_windows_and_decompression_bombs_reject_before_decoder_creation() {
        let mut oversized = vec![0x28, 0xb5, 0x2f, 0xfd, 0xe0];
        oversized.extend_from_slice(&(MAX_DECODED_BYTES as u64 + 1).to_le_bytes());
        let hash = sha256(&oversized);
        assert!(
            Font::from_zstd(&oversized, &hash)
                .unwrap_err()
                .contains("exceeds bound")
        );
        let mut huge_window = vec![0x28, 0xb5, 0x2f, 0xfd, 0x80, 0xf8];
        huge_window.extend_from_slice(&100_u32.to_le_bytes());
        assert!(
            check_zstd_header(&huge_window)
                .unwrap_err()
                .contains("exceeds bound")
        );
        assert!(Font::from_zstd(&vec![0; MAX_COMPRESSED_BYTES + 1], &"0".repeat(64)).is_err());
    }
    #[test]
    fn invalid_glyph_bounds_and_offsets_never_reach_atlas_builder() {
        let f = test_font();
        let index = f.glyphs.iter().position(|g| g.scalar == 'O').unwrap();
        let mut bytes = decoded();
        let offset = HEADER + index * RECORD + 18;
        bytes[offset..offset + 8].fill(0);
        assert!(Font::from_packet(&bytes, String::new()).is_err());
        let mut bytes = decoded();
        bytes[HEADER + 8..HEADER + 12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Font::from_packet(&bytes, String::new()).is_err());
    }
}
