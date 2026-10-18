// SPDX-License-Identifier: Apache-2.0
//! Owned Linux graphics boundary approved in RFC 0001. No runtime state lives here.
#[cfg(all(target_os = "linux", feature = "platform"))]
#[allow(unsafe_code)]
mod ffi;
#[cfg(all(target_os = "linux", feature = "platform"))]
pub use ffi::{Display, Flip, Info, Touch};

pub const MAX_VERTICES: usize = 2_000_000;
/// Bound allocations and reject malformed geometry before FFI. Coordinates stay
/// logical; the independently checked projection performs output rotation.
pub fn validate_mesh(vertices: &[f32]) -> Result<(), String> {
    if !vertices.len().is_multiple_of(18) || vertices.len() / 6 > MAX_VERTICES {
        return Err("Expected bounded, complete triangles".into());
    }
    for vertex in vertices.chunks_exact(6) {
        if vertex[..2]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 100_000.)
            || vertex[2..]
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("Invalid position or RGBA value".into());
        }
    }
    Ok(())
}
/// Bound the glyph FFI ABI and preserve source painting order. Each glyph is
/// four vertices of 16 floats, indexed as two triangles by the graphics owner.
pub fn validate_text_mesh(
    glyphs: &[f32],
    draws: &[u32],
    geometry_vertices: usize,
) -> Result<(), String> {
    if !glyphs.len().is_multiple_of(64)
        || glyphs.len() / 16 > 65536
        || !draws.len().is_multiple_of(3)
        || draws.len() > 12288
    {
        return Err("Invalid glyph quad or draw-plan dimensions".into());
    }
    for vertex in glyphs.chunks_exact(16) {
        if vertex.iter().any(|v| !v.is_finite() || v.abs() > 100000.)
            || vertex[4..8].iter().any(|v| !(0. ..=1.).contains(v))
            || vertex[12] < 0.
            || vertex[12] > 65535.
            || vertex[13] != 7.
            || vertex[14] != 7.
            || vertex[15] != 0.
        {
            return Err("Invalid glyph vertex".into());
        }
    }
    for draw in draws.chunks_exact(3) {
        let (kind, first, count) = (draw[0], draw[1] as usize, draw[2] as usize);
        let limit = if kind == 0 {
            geometry_vertices
        } else {
            glyphs.len() / 16
        };
        let alignment = if kind == 0 { 3 } else { 4 };
        if kind > 1
            || first > limit
            || count > limit - first
            || !first.is_multiple_of(alignment)
            || !count.is_multiple_of(alignment)
        {
            return Err("Invalid ordered draw range".into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn glyph_ffi_rejects_partial_quads_and_invalid_ordered_ranges() {
        let mut glyphs = vec![0.; 64];
        for vertex in glyphs.chunks_exact_mut(16) {
            vertex[7] = 1.;
            vertex[13] = 7.;
            vertex[14] = 7.;
        }
        assert!(validate_text_mesh(&glyphs, &[0, 0, 3, 1, 0, 4], 3).is_ok());
        assert!(validate_text_mesh(&glyphs[..63], &[1, 0, 4], 0).is_err());
        for draws in [
            &[1, 1, 3][..],
            &[1, 0, 8][..],
            &[0, 0, 4][..],
            &[2, 0, 0][..],
            &[1, 0][..],
        ] {
            assert!(validate_text_mesh(&glyphs, draws, 3).is_err());
        }
        glyphs[12] = f32::NAN;
        assert!(validate_text_mesh(&glyphs, &[1, 0, 4], 0).is_err());
    }
    #[test]
    fn rejects_incomplete_nonfinite_and_oversized_geometry() {
        let triangle = [
            0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0., 1., 0., 1., 0., 0., 1., 1.,
        ];
        assert!(validate_mesh(&triangle).is_ok());
        assert!(validate_mesh(&triangle[..17]).is_err());
        for bad in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            let mut v = triangle;
            v[5] = bad;
            assert!(validate_mesh(&v).is_err());
        }
        assert!(validate_mesh(&vec![0.; (MAX_VERTICES / 3 + 1) * 18]).is_err());
    }
}
