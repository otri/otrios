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
#[cfg(test)]
mod tests {
    use super::*;
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
