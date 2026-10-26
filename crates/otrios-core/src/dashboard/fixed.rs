// SPDX-License-Identifier: LGPL-3.0-only
//! Checked Q48.16 layout arithmetic. Conversion is explicit at float contracts.
use super::Result;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed(i64);
/// Higher fractional precision for ratios and expression intermediates. A small
/// ratio must not be rounded to the coordinate grid before multiplying a width.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scalar(i64);
// Decode the exact binary f32 value rather than calling roundf and a soft i64
// float conversion on ARMv7. Admission rounds once, nearest with ties away.
#[inline]
fn admit_raw(value: f32, fractional: i32) -> Result<i64> {
    let bits = value.to_bits();
    let exponent = ((bits >> 23) & 255) as i32;
    if exponent == 255 {
        return Err("Nonfinite fixed point input".into());
    }
    let mantissa = u64::from((bits & 0x7fffff) | if exponent == 0 { 0 } else { 1 << 23 });
    let shift = if exponent == 0 { -149 } else { exponent - 150 } + fractional;
    let magnitude = if shift >= 0 {
        if shift > 40 {
            return Err("Fixed point admission overflow".into());
        }
        mantissa << shift
    } else {
        let right = (-shift) as u32;
        if right > 24 {
            0
        } else {
            (mantissa + (1 << (right - 1))) >> right
        }
    };
    signed_magnitude(magnitude, bits >> 31 != 0)
}
#[inline]
fn signed_magnitude(magnitude: u64, negative: bool) -> Result<i64> {
    if negative && magnitude == 1 << 63 {
        Ok(i64::MIN)
    } else {
        let value = i64::try_from(magnitude).map_err(|_| "Fixed point arithmetic overflow")?;
        Ok(if negative { -value } else { value })
    }
}
impl Scalar {
    pub const ZERO: Self = Self(0);
    pub const fn raw(self) -> i64 {
        self.0
    }
    #[inline]
    pub fn from_f32(value: f32) -> Result<Self> {
        let raw = admit_raw(value, 32)?;
        if raw == 0 && value != 0. {
            return Err("Fixed scalar admission underflow".into());
        }
        Ok(Self(raw))
    }
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / 4294967296.
    }
    #[inline]
    pub fn checked_add(self, rhs: Self) -> Result<Self> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or_else(|| "Fixed scalar addition overflow".into())
    }
    #[inline]
    pub fn checked_sub(self, rhs: Self) -> Result<Self> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or_else(|| "Fixed scalar subtraction overflow".into())
    }
    #[inline]
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
        let normalized = values.map(|value| {
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
        });
        // Ratios are invariant under a common exact power of two. Strip shared
        // zero bits so ordinary dyadic layout sizes fit native integer lanes.
        let shift = normalized
            .iter()
            .map(|v| v.0.unsigned_abs().trailing_zeros())
            .min()
            .unwrap_or(0);
        Ok(normalized.map(|v| Self(v.0 >> shift)))
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
    #[inline]
    pub fn from_f32(value: f32) -> Result<Self> {
        admit_raw(value, 16).map(Self)
    }
    #[inline]
    pub fn to_f32(self) -> f32 {
        if let Ok(local) = i32::try_from(self.0) {
            local as f32 / 65536.
        } else {
            self.0 as f32 / 65536.
        }
    }
    #[inline]
    pub fn checked_add(self, other: Self) -> Result<Self> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(|| "Fixed point addition overflow".into())
    }
    #[inline]
    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| "Fixed point subtraction overflow".into())
    }
    #[inline]
    pub fn checked_mul(self, other: Self) -> Result<Self> {
        rounded_ratio(i128::from(self.0) * i128::from(other.0), 65536).map(Self)
    }
    #[inline]
    pub fn checked_div(self, other: Self) -> Result<Self> {
        rounded_ratio(i128::from(self.0) * 65536, i128::from(other.0)).map(Self)
    }
    /// Multiply before dividing so a rounded proportional share cannot accumulate
    /// a layout-size-dependent error. All three operands have the same Q format.
    #[inline]
    pub fn checked_mul_ratio(self, numerator: Self, denominator: Self) -> Result<Self> {
        rounded_ratio(
            i128::from(self.0) * i128::from(numerator.0),
            i128::from(denominator.0),
        )
        .map(Self)
    }
    #[inline]
    pub fn checked_scale_ratio(self, numerator: Scalar, denominator: Scalar) -> Result<Self> {
        if let (Ok(a), Ok(b), Ok(c)) = (
            i32::try_from(self.0),
            i32::try_from(numerator.0),
            i32::try_from(denominator.0),
        ) {
            if c == 0 {
                return Err("Fixed point division by zero".into());
            }
            let product = i64::from(a) * i64::from(b);
            return rounded_magnitudes(
                product.unsigned_abs(),
                i64::from(c).unsigned_abs(),
                (product < 0) ^ (c < 0),
            )
            .map(Self);
        }
        rounded_ratio(
            i128::from(self.0) * i128::from(numerator.0),
            i128::from(denominator.0),
        )
        .map(Self)
    }
    #[inline]
    pub fn checked_div_int(self, denominator: i64) -> Result<Self> {
        rounded_ratio(i128::from(self.0), i128::from(denominator)).map(Self)
    }
}
#[inline]
fn rounded_magnitudes(n: u64, d: u64, negative: bool) -> Result<i64> {
    let (q, r) = if let (Ok(a), Ok(b)) = (u32::try_from(n), u32::try_from(d)) {
        (u64::from(a / b), u64::from(a % b))
    } else {
        (n / d, n % d)
    };
    let rounded = q
        .checked_add(u64::from(r >= d - r))
        .ok_or("Fixed point arithmetic overflow")?;
    signed_magnitude(rounded, negative)
}
#[inline]
pub(super) fn rounded_ratio(n: i128, d: i128) -> Result<i64> {
    if d == 0 {
        return Err("Fixed point division by zero".into());
    }
    let negative = (n < 0) ^ (d < 0);
    if let (Ok(n), Ok(d)) = (
        u64::try_from(n.unsigned_abs()),
        u64::try_from(d.unsigned_abs()),
    ) {
        // Most bounded layout products fit 64 bits. Avoid a software 128-bit
        // division, and use Cortex-A7's integer divide when both fit 32 bits.
        return rounded_magnitudes(n, d, negative);
    }
    // Even i128::MIN plus half an i128 denominator stays below 2^128.
    let quotient = (n.unsigned_abs() + d.unsigned_abs() / 2) / d.unsigned_abs();
    let magnitude = u64::try_from(quotient).map_err(|_| "Fixed point arithmetic overflow")?;
    signed_magnitude(magnitude, negative)
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
    fn binary_admission_matches_wide_float_reference_over_exponents_and_rounding_boundaries() {
        let mut bits = 0x12345678_u32;
        for _ in 0..65536 {
            bits ^= bits << 13;
            bits ^= bits >> 17;
            bits ^= bits << 5;
            let value = f32::from_bits(bits);
            for fractional in [16, 32] {
                let reference = (f64::from(value) * (1_u64 << fractional) as f64).round();
                let admitted = admit_raw(value, fractional);
                if reference.is_finite()
                    && reference >= i64::MIN as f64
                    && reference < -(i64::MIN as f64)
                {
                    assert_eq!(
                        admitted.unwrap(),
                        reference as i64,
                        "{bits:08x} Q{fractional}"
                    );
                } else {
                    assert!(admitted.is_err(), "{bits:08x} Q{fractional}");
                }
            }
        }
        for value in [-32768., -0.5 / 65536., 0., 0.5 / 65536., 32768., 100000.] {
            assert_eq!(
                Fixed::from_f32(value).unwrap().raw(),
                (f64::from(value) * 65536.).round() as i64
            );
        }
    }
    #[test]
    fn narrow_division_preserves_wide_signed_rounding_and_overflow() {
        assert!(rounded_ratio(i128::MIN, 1).is_err());
        assert!(rounded_ratio(i128::MIN, -1).is_err());
        assert_eq!(rounded_ratio(i128::MIN, i128::MIN).unwrap(), 1);
        for a in [
            i64::MIN,
            -4294967296,
            -65537,
            -1,
            0,
            1,
            65537,
            4294967296,
            i64::MAX,
        ] {
            for b in [-65536_i64, -5, -2, -1, 1, 2, 5, 65536, i64::MAX] {
                for n in [i128::from(a), i128::from(a) * i128::from(b)] {
                    let d = i128::from(b);
                    let magnitude = (n.unsigned_abs() + d.unsigned_abs() / 2) / d.unsigned_abs();
                    let reference = if (n < 0) ^ (d < 0) {
                        -(magnitude as i128)
                    } else {
                        magnitude as i128
                    };
                    match i64::try_from(reference) {
                        Ok(expected) => assert_eq!(rounded_ratio(n, d).unwrap(), expected),
                        Err(_) => assert!(rounded_ratio(n, d).is_err()),
                    }
                }
            }
        }
    }
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
