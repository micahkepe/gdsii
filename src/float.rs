//! GDSII floating-point type definitions.
//!
//! > **NOTE**: GDSII predates [IEEE 754] Standard for Floating-Point Arithmetic and instead uses a
//! > custom floating point definition that requires specialized parsing.
//!
//! ## Structure
//!
//! Both the 4 and 8-byte "real" floats share the same encoding structure:
//!
//! ```text
//! Byte    0:      SEEE EEEE
//! Bytes   1-N:    MMMM MMMM ...
//! ```
//!
//! * `S`: sign bit (1 = negative)
//! * `E`: 7-bit exponent in excess-64 format (actual = field - 64)
//! * `M`: mantissa bits
//!
//! ```text
//! Value = ((-1) ^ S) x (M / 2 ^ N_MATINSSA_BITS) x 16 ^ (E - 64)
//! ```
//!
//! * True zero is represented with all zero bits.
//!
//! [IEEE 754]: <https://en.wikipedia.org/wiki/IEEE_754>

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

/// Unrepresentable GDSII "real" type error.
#[derive(Debug, thiserror::Error)]
#[error(
    "value is not representable as a GDSII real (NaN, Inf, or out of range)"
)]
pub struct NotRepresentable;

/// GDSII 4-byte "real" float.
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    FromBytes,
    IntoBytes,
    KnownLayout,
    Immutable,
    Unaligned,
)]
pub struct GdsFourByteReal([u8; 4]);

impl GdsFourByteReal {
    #[must_use]
    pub const fn raw(&self) -> [u8; 4] {
        self.0
    }
}

impl From<GdsFourByteReal> for f64 {
    fn from(value: GdsFourByteReal) -> Self {
        let bytes = value.raw();
        if bytes == [0u8; 4] {
            return 0.0;
        }
        let sign: Self = if bytes[0] & 0x80 != 0 { -1.0 } else { 1.0 };
        let exp = i32::from(bytes[0] & 0x7F) - 64;
        let mantissa_int = u32::from(bytes[1]) << 16
            | u32::from(bytes[2]) << 8
            | u32::from(bytes[3]);
        sign * Self::from(mantissa_int) * 2f64.powi(4 * exp - 24)
    }
}

impl TryFrom<f64> for GdsFourByteReal {
    type Error = NotRepresentable;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !value.is_finite() {
            return Err(NotRepresentable);
        }
        if value == 0.0 {
            return Ok(Self([0u8; 4]));
        }

        let GdsRealComponents { sign, exponent, mantissa } =
            encode_real_components(value, 24)?;

        #[expect(
            clippy::cast_possible_truncation,
            reason = "mantissa is in [2^20, 2^24), fits in u32"
        )]
        let m = mantissa as u32;
        Ok(Self([
            sign | exponent,
            ((m >> 16) & 0xFF) as u8,
            ((m >> 8) & 0xFF) as u8,
            (m & 0xFF) as u8,
        ]))
    }
}

/// GDSII 8-byte "real" float.
#[repr(transparent)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    FromBytes,
    IntoBytes,
    KnownLayout,
    Immutable,
    Unaligned,
)]
pub struct GdsEightByteReal([u8; 8]);

impl GdsEightByteReal {
    #[must_use]
    pub const fn raw(&self) -> [u8; 8] {
        self.0
    }
}

impl From<GdsEightByteReal> for f64 {
    fn from(value: GdsEightByteReal) -> Self {
        let bytes = value.raw();
        if bytes == [0u8; 8] {
            return 0.0;
        }
        let sign: Self = if bytes[0] & 0x80 != 0 { -1.0 } else { 1.0 };
        let exp = i32::from(bytes[0] & 0x7F) - 64;
        let mantissa_int = u64::from(bytes[1]) << 48
            | u64::from(bytes[2]) << 40
            | u64::from(bytes[3]) << 32
            | u64::from(bytes[4]) << 24
            | u64::from(bytes[5]) << 16
            | u64::from(bytes[6]) << 8
            | u64::from(bytes[7]);
        #[expect(
            clippy::cast_precision_loss,
            reason = "56-bit mantissa -> f64's 53-bit significand loses at most 3 bits. \
                      For values that originated from f64 encoding, the bottom bits are \
                      zero padding so this cast is exact."
        )]
        let mantissa = mantissa_int as Self;
        sign * mantissa * 2f64.powi(4 * exp - 56)
    }
}

impl TryFrom<f64> for GdsEightByteReal {
    type Error = NotRepresentable;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if !value.is_finite() {
            return Err(NotRepresentable);
        }
        if value == 0.0 {
            return Ok(Self([0u8; 8]));
        }

        let GdsRealComponents { sign, exponent, mantissa } =
            encode_real_components(value, 56)?;

        Ok(Self([
            sign | exponent,
            ((mantissa >> 48) & 0xFF) as u8,
            ((mantissa >> 40) & 0xFF) as u8,
            ((mantissa >> 32) & 0xFF) as u8,
            ((mantissa >> 24) & 0xFF) as u8,
            ((mantissa >> 16) & 0xFF) as u8,
            ((mantissa >> 8) & 0xFF) as u8,
            (mantissa & 0xFF) as u8,
        ]))
    }
}

// ==============================================================================
// Encoding via IEEE 754 bit extraction
// ==============================================================================

#[derive(Debug)]
struct GdsRealComponents {
    sign: u8,
    exponent: u8,
    mantissa: u64,
}

/// Encode a non-zero finite `f64` into GDS real components using exact integer arithmetic.
///
/// Operates directly on the IEEE 754 bit representation to avoid all floating-point rounding
/// in the conversion. For 8-byte (`mantissa_bits` = 56), the encoding is lossless: GDS has
/// 56 mantissa bits vs f64's 53-bit significand, so every f64 value maps to a unique GDS
/// encoding. For 4-byte (`mantissa_bits` = 24), the 53-bit significand is rounded to 24 bits
/// using round-to-nearest-even.
fn encode_real_components(
    value: f64,
    mantissa_bits: u32,
) -> Result<GdsRealComponents, NotRepresentable> {
    debug_assert!(value.is_finite() && value != 0.0);
    debug_assert!(mantissa_bits == 24 || mantissa_bits == 56);

    let bits = value.to_bits();
    let sign = if bits >> 63 != 0 { 0x80u8 } else { 0x00u8 };
    let biased_exp = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & 0x000F_FFFF_FFFF_FFFF;

    // Extract the integer significand and its true binary exponent such that
    // |value| = sig × 2^e2 exactly.
    let (sig, e2) = if biased_exp == 0 {
        // Subnormal: no implicit leading 1.
        (frac, -1074_i32)
    } else {
        ((1u64 << 52) | frac, biased_exp - 1023 - 52)
    };

    let mb = mantissa_bits.cast_signed();
    let sb = (64 - sig.leading_zeros()).cast_signed();

    // We need a GDS mantissa M in [2^(mb-4), 2^mb) and integer exponent E such that
    //   M × 2^(4E - mb) = sig × 2^e2
    // i.e. M = sig × 2^shift where shift = e2 + mb - 4E.
    //
    // For sig's MSB at bit (sb-1), after shifting the MSB lands at (sb-1+shift).
    // We need that in [mb-4, mb-1], giving shift ∈ [mb-sb-3, mb-sb].
    // Exactly one value in that 4-element range makes (e2 + mb - shift) divisible by 4.
    let min_shift = mb - sb - 3;
    let base = e2 + sb + 3;
    let extra = base.rem_euclid(4);
    let shift = min_shift + extra;
    let mut gds_exp = (e2 + mb - shift) / 4;

    let mut m = if shift >= 0 {
        sig << shift.cast_unsigned()
    } else {
        let right_shift = (-shift).cast_unsigned();
        let truncated = sig >> right_shift;
        let remainder = sig & ((1u64 << right_shift) - 1);
        let half = 1u64 << (right_shift - 1);
        if remainder > half || (remainder == half && truncated & 1 != 0) {
            truncated + 1
        } else {
            truncated
        }
    };

    // Rounding can push M to exactly 2^mb, requiring renormalization.
    if m >= (1u64 << mantissa_bits) {
        m >>= 4;
        gds_exp += 1;
    }

    if !(-64..=63).contains(&gds_exp) {
        return Err(NotRepresentable);
    }

    Ok(GdsRealComponents {
        sign,
        exponent: u8::try_from(gds_exp + 64)
            .expect("gds_exp + 64 is in [0, 127]"),
        mantissa: m,
    })
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::*;

    #[test]
    fn eight_byte_real_zero() {
        assert!(
            (f64::from(GdsEightByteReal([0u8; 8])) - 0.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn four_byte_real_zero() {
        assert!(
            (f64::from(GdsFourByteReal([0u8; 4])) - 0.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn eight_byte_real_one() {
        // 1.0 = 0.0625 × 16^1 -> exponent byte = 64 + 1 = 0x41, mantissa = 0x10_0000_0000_0000
        let r =
            GdsEightByteReal([0x41, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let v = f64::from(r);
        assert!((v - 1.0).abs() < 1e-15, "expected 1.0, got {v}");
    }

    #[test]
    fn eight_byte_real_negative_one() {
        // -1.0: sign bit set -> 0xC1 first byte
        let r =
            GdsEightByteReal([0xC1, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let v = f64::from(r);
        assert!((v + 1.0).abs() < 1e-15, "expected -1.0, got {v}");
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "8-byte roundtrip is exact by construction"
    )]
    fn encode_decode_eight_byte_round_trip_manual() {
        for &x in &[1.0f64, -1.0, 0.5, 1.5, 0.1, 1e10, -1e-10, 42.0] {
            let encoded = GdsEightByteReal::try_from(x).expect("should encode");
            let decoded = f64::from(encoded);
            assert_eq!(decoded, x, "round-trip not exact for {x}");
        }
    }

    #[test]
    fn encode_decode_four_byte_round_trip_manual() {
        for &x in &[1.0f64, -1.0, 0.5, 1.5, 42.0] {
            let encoded = GdsFourByteReal::try_from(x).expect("should encode");
            let decoded = f64::from(encoded);
            // 4-byte real has only 24-bit mantissa; allow ~1e-6 relative error
            let rel_err = (decoded - x).abs() / x.abs();
            assert!(
                rel_err < 1e-6,
                "round-trip failed for {x}: got {decoded}, rel_err={rel_err}"
            );
        }
    }

    #[test]
    fn not_representable_nan_inf() {
        assert!(GdsEightByteReal::try_from(f64::NAN).is_err());
        assert!(GdsEightByteReal::try_from(f64::INFINITY).is_err());
        assert!(GdsFourByteReal::try_from(f64::NAN).is_err());
        assert!(GdsFourByteReal::try_from(f64::INFINITY).is_err());
    }

    #[expect(
        clippy::float_cmp,
        reason = "8-byte roundtrip is exact by construction"
    )]
    #[quickcheck]
    fn qc_eight_byte_round_trip(x: f64) -> bool {
        if !x.is_finite() {
            return GdsEightByteReal::try_from(x).is_err();
        }
        if x == 0.0 {
            let r = GdsEightByteReal::try_from(x).expect("zero encodes");
            return f64::from(r) == 0.0;
        }
        GdsEightByteReal::try_from(x).map_or(true, |r| f64::from(r) == x)
    }

    #[quickcheck]
    fn qc_four_byte_round_trip(x: f64) -> bool {
        if !x.is_finite() {
            return GdsFourByteReal::try_from(x).is_err();
        }
        if x == 0.0 {
            let r = GdsFourByteReal::try_from(x).expect("zero encodes");
            return f64::from(r) == 0.0;
        }
        GdsFourByteReal::try_from(x).map_or(true, |r| {
            let decoded = f64::from(r);
            (decoded - x).abs() / x.abs() < 1e-6
        })
    }
}
