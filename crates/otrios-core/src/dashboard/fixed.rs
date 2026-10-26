// SPDX-License-Identifier: LGPL-3.0-only
//! Checked Q48.16 layout arithmetic. Conversion is explicit at float contracts.
use super::Result;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed(i64);
/// Higher fractional precision for ratios and expression intermediates. A small
/// ratio must not be rounded to the coordinate grid before multiplying a width.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scalar(i64);
impl Scalar {
    pub const ZERO: Self = Self(0);
    pub const fn raw(self) -> i64 {
        self.0
    }
    pub fn from_f32(value: f32) -> Result<Self> {
        let scaled = value * 4294967296.;
        if !scaled.is_finite() || scaled < i64::MIN as f32 || scaled >= -(i64::MIN as f32) {
            return Err("Fixed scalar admission overflow".into());
        }
        let raw = scaled.round() as i64;
        if raw == 0 && value != 0. {
            return Err("Fixed scalar admission underflow".into());
        }
        Ok(Self(raw))
    }
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 4294967296.
    }
    pub fn checked_add(self, rhs: Self) -> Result<Self> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or_else(|| "Fixed scalar addition overflow".into())
    }
    pub fn checked_sub(self, rhs: Self) -> Result<Self> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or_else(|| "Fixed scalar subtraction overflow".into())
    }
    pub fn checked_mul(self, rhs: Self) -> Result<Self> {
        let product = i128::from(self.0) * i128::from(rhs.0);
        let raw = rounded_ratio(product, 4294967296)?;
        if raw == 0 && product != 0 {
            return Err("Fixed scalar multiplication underflow".into());
        }
        Ok(Self(raw))
    }
    /// Put proportional inputs on one exact power-of-two scale before Q32
    /// admission. This keeps tiny absolute spring segments accurate as ratios.
    pub fn normalize_ratios<const N: usize>(values: [f32; N]) -> Result<[Self; N]> {
        let mut maximum = 0_u32;
        for value in values {
            let bits = value.to_bits() & 0x7fff_ffff;
            if bits >= 0x7f80_0000 {
                return Err("Nonfinite ratio".into());
            }
            maximum = maximum.max(bits);
        }
        if maximum == 0 {
            return Ok([Self::ZERO; N]);
        }
        let exponent = (maximum >> 23) as i32;
        let common = if exponent == 0 {
            (maximum & 0x7fffff).ilog2() as i32 - 148
        } else {
            exponent - 126
        };
        Ok(values.map(|value| {
            let bits = value.to_bits();
            let exp = ((bits >> 23) & 255) as i32;
            let mantissa = u64::from((bits & 0x7fffff) | if exp == 0 { 0 } else { 1 << 23 });
            let shift = if exp == 0 { -149 } else { exp - 150 } - common + 32;
            let raw = if shift >= 0 {
                mantissa << shift
            } else {
                let right = (-shift) as u32;
                if right > 24 {
                    0
                } else {
                    (mantissa + (1 << (right - 1))) >> right
                }
            };
            Self(if bits >> 31 != 0 {
                -(raw as i64)
            } else {
                raw as i64
            })
        }))
    }
}
impl Fixed {
    pub const FRACTION_BITS: u32 = 16;
    pub const ONE: Self = Self(1 << 16);
    pub const ZERO: Self = Self(0);
    pub const fn from_raw(raw: i64) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> i64 {
        self.0
    }
    pub fn from_f32(value: f32) -> Result<Self> {
        // Multiplication by a power of two is exact for admitted normal f32s.
        // The upper bound is exclusive: i64::MAX rounds to 2^63 as f32.
        let scaled = value * 65536.;
        if !scaled.is_finite() || scaled < i64::MIN as f32 || scaled >= -(i64::MIN as f32) {
            return Err("Fixed point admission overflow".into());
        }
        Ok(Self(scaled.round() as i64))
    }
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 65536.
    }
    pub fn checked_add(self, other: Self) -> Result<Self> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(|| "Fixed point addition overflow".into())
    }
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| "Fixed point subtraction overflow".into())
    }
    pub fn checked_mul(self, other: Self) -> Result<Self> {
        rounded_ratio(i128::from(self.0) * i128::from(other.0), 65536).map(Self)
    }
    pub fn checked_div(self, other: Self) -> Result<Self> {
        rounded_ratio(i128::from(self.0) * 65536, i128::from(other.0)).map(Self)
    }
    /// Multiply before dividing so a rounded proportional share cannot accumulate
    /// a layout-size-dependent error. All three operands have the same Q format.
    pub fn checked_mul_ratio(self, numerator: Self, denominator: Self) -> Result<Self> {
        rounded_ratio(
            i128::from(self.0) * i128::from(numerator.0),
            i128::from(denominator.0),
        )
        .map(Self)
    }
    pub fn checked_scale_ratio(self, numerator: Scalar, denominator: Scalar) -> Result<Self> {
        rounded_ratio(
            i128::from(self.0) * i128::from(numerator.0),
            i128::from(denominator.0),
        )
        .map(Self)
    }
    pub fn checked_div_int(self, denominator: i64) -> Result<Self> {
        rounded_ratio(i128::from(self.0), i128::from(denominator)).map(Self)
    }
}
pub(super) fn rounded_ratio(n: i128, d: i128) -> Result<i64> {
    if d == 0 {
        return Err("Fixed point division by zero".into());
    }
    // Callers use products of i64 values or an i64 shifted by 16, so this
    // unsigned rounding sum stays below 2^128, including negative extrema.
    let quotient = (n.unsigned_abs() + d.unsigned_abs() / 2) / d.unsigned_abs();
    let signed = if (n < 0) ^ (d < 0) {
        -(quotient as i128)
    } else {
        quotient as i128
    };
    i64::try_from(signed).map_err(|_| "Fixed point arithmetic overflow".into())
}

/// Four independent bounded coordinate additions. The explicit integer overflow
/// reduction permits compiler SIMD without unsafe intrinsics or relaxed FP math.
#[inline]
pub fn add4(a: [i32; 4], b: [i32; 4]) -> Option<[i32; 4]> {
    let sum = std::array::from_fn(|i| a[i].wrapping_add(b[i]));
    let overflow = (0..4).fold(0, |bits, i| bits | ((sum[i] ^ a[i]) & (sum[i] ^ b[i])));
    if overflow < 0 { None } else { Some(sum) }
}

/// Immutable inputs and translation are admitted once, before the hot loop.
/// The borrow prevents mutation of admitted coordinates while this proof lives.
pub struct Translation<'a> {
    input: &'a [[i32; 4]],
    delta: [i32; 4],
}
impl<'a> Translation<'a> {
    pub fn admit(input: &'a [[i32; 4]], delta: [i32; 4]) -> Result<Self> {
        if input.iter().any(|a| add4(*a, delta).is_none()) {
            return Err("Fixed translation overflow".into());
        }
        Ok(Self { input, delta })
    }
    pub fn write(&self, out: &mut [[i32; 4]]) -> Result<()> {
        if out.len() != self.input.len() {
            return Err("Fixed translation dimensions".into());
        }
        for (a, out) in self.input.iter().zip(out) {
            *out = add4_admitted(*a, self.delta);
        }
        Ok(())
    }
}
// Only callers that have proved their complete input range may use this kernel.
// Wrapping operations make the scalar and vector instructions' semantics exact;
// range admission guarantees the mathematical results themselves do not wrap.
#[inline]
pub(super) fn add4_admitted(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    std::array::from_fn(|i| a[i].wrapping_add(b[i]))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_steps_survive_large_coordinates_until_presentation() {
        let large = Fixed::from_f32(100_000.).unwrap();
        let step = Fixed::from_raw(1);
        assert_eq!(
            large.checked_add(step).unwrap().checked_sub(large).unwrap(),
            step
        );
        assert_eq!(100_000_f32 + step.to_f32() - 100_000_f32, 0.);
        for raw in [-1, 0, 1, 65536, 65537, -65537] {
            assert_eq!(
                Fixed::from_f32(Fixed::from_raw(raw).to_f32())
                    .unwrap()
                    .raw(),
                raw
            );
        }
    }
    #[test]
    fn ratios_keep_extra_precision_until_large_dimensions_have_been_applied() {
        let ratio = Scalar::from_f32(0.0001).unwrap();
        let width = Scalar::from_f32(8192.).unwrap();
        let result = ratio.checked_mul(width).unwrap().to_f32();
        assert!((result - 0.0001 * 8192.).abs() < 0.000002);
        let low = Fixed::from_f32(0.0001)
            .unwrap()
            .checked_mul(Fixed::from_f32(8192.).unwrap())
            .unwrap()
            .to_f32();
        assert!(
            (low - result).abs() > 0.01,
            "Coordinate precision is unsuitable for an intermediate ratio"
        );
        assert!(Scalar::from_f32(1e20).is_err());
        assert!(Scalar::from_f32(1e-30).is_err());
        assert!(
            Scalar::from_f32(0.000001)
                .unwrap()
                .checked_mul(Scalar::from_f32(0.000001).unwrap())
                .is_err()
        );
        for exponent in 0..255 {
            let a = f32::from_bits(exponent << 23 | 0x200000);
            let b = f32::from_bits(exponent << 23 | 0x400000);
            let normalized = Scalar::normalize_ratios([a, b]).unwrap();
            let actual = Fixed::from_f32(8192.)
                .unwrap()
                .checked_scale_ratio(
                    normalized[0],
                    normalized[0].checked_add(normalized[1]).unwrap(),
                )
                .unwrap()
                .to_f32();
            let expected = (f64::from(a) / (f64::from(a) + f64::from(b)) * 8192.) as f32;
            assert!(
                (actual - expected).abs() < 0.001,
                "Exponent {exponent}: {actual} vs {expected}"
            );
        }
    }
    #[test]
    fn signed_rounding_wide_intermediates_and_overflow_are_explicit() {
        assert_eq!(Fixed::from_f32(0.5 / 65536.).unwrap().raw(), 1);
        assert_eq!(Fixed::from_f32(-0.5 / 65536.).unwrap().raw(), -1);
        let max = Fixed::from_raw(i64::MAX);
        assert_eq!(max.checked_mul(Fixed::ONE).unwrap(), max);
        assert_eq!(max.checked_mul_ratio(max, max).unwrap(), max);
        assert!(max.checked_add(Fixed::from_raw(1)).is_err());
        assert!(
            Fixed::from_raw(i64::MIN)
                .checked_sub(Fixed::from_raw(1))
                .is_err()
        );
        assert!(Fixed::ONE.checked_div(Fixed::ZERO).is_err());
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
            assert!(Fixed::from_f32(value).is_err());
        }
        assert_eq!(Fixed::from_raw(-5).checked_div_int(2).unwrap().raw(), -3);
    }
    #[test]
    fn coordinate_lanes_match_checked_scalar_at_every_overflow_boundary() {
        for a in [
            i32::MIN,
            i32::MIN + 1,
            -65536,
            -1,
            0,
            1,
            65536,
            i32::MAX - 1,
            i32::MAX,
        ] {
            for b in [i32::MIN, -65536, -1, 0, 1, 65536, i32::MAX] {
                let left = [a, 1, -1, a];
                let right = [b, 2, 1, b];
                let scalar: Option<Vec<_>> = left
                    .iter()
                    .zip(right)
                    .map(|(a, b)| a.checked_add(b))
                    .collect();
                assert_eq!(add4(left, right).map(Vec::from), scalar);
            }
        }
    }
    #[test]
    fn admitted_batches_preserve_checked_results_and_reject_invalid_dimensions() {
        let input = [[-65536, 0, 65536, 131072], [1, 2, 3, 4]];
        let delta = [5, -6, 7, -8];
        let admitted = Translation::admit(&input, delta).unwrap();
        let mut out = [[0; 4]; 2];
        admitted.write(&mut out).unwrap();
        for (a, b) in input.into_iter().zip(out) {
            assert_eq!(Some(b), add4(a, delta));
        }
        assert!(admitted.write(&mut [[0; 4]; 1]).is_err());
        assert!(Translation::admit(&[[i32::MAX, 0, 0, 0]], [1, 0, 0, 0]).is_err());
        assert!(Translation::admit(&[[i32::MIN, 0, 0, 0]], [-1, 0, 0, 0]).is_err());
    }
}
