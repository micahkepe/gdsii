//! GDSII floating-point type definitions.
//!
//! NOTE: GDSII predates [IEEE 754] Standard for Floating-Point Arithmetic because it's super old.
//! It uses a custom floating point definition that requires specialized parsing.
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

#[derive(Debug, thiserror::Error)]
#[error("value is not representable as a GDSII real (NaN, Inf, or out of range)")]
pub struct NotRepresentable;

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
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
        let sign = if bytes[0] & 0x80 != 0 {
            -1.0f64
        } else {
            1.0f64
        };
        let exp = i32::from(bytes[0] & 0x7F) - 64;
        let mantissa_int =
            u32::from(bytes[1]) << 16 | u32::from(bytes[2]) << 8 | u32::from(bytes[3]);
        let mantissa = Self::from(mantissa_int) / Self::from(1u32 << 24);
        sign * mantissa * 16f64.powi(exp)
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

        let ParsedRealComponents {
            sign,
            exponent,
            mantissa,
        } = encode_components(value)?;

        // mantissa \in [1/16, 1) so mantissa × 2^24 \in [2^20, 2^24) -> fits in u32, non-negative.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "mantissa × 2^24 is in [2^20, 2^24), fits in u32"
        )]
        #[expect(clippy::cast_sign_loss, reason = "mantissa is always positive")]
        let m = (mantissa * f64::from(1u32 << 24)) as u32;
        Ok(Self([
            sign | exponent,
            ((m >> 16) & 0xFF) as u8,
            ((m >> 8) & 0xFF) as u8,
            (m & 0xFF) as u8,
        ]))
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
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
        let sign = if bytes[0] & 0x80 != 0 {
            -1.0f64
        } else {
            1.0f64
        };
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
            reason = "Precision loss here is bounded: 56-bit mantissa into f64's 52-bit mantissa loses at most 4 bits, giving relative error < 2^-53 < f64::EPSILON."
        )]
        let mantissa = mantissa_int as Self / (1u64 << 56) as Self;
        sign * mantissa * 16f64.powi(exp)
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

        let ParsedRealComponents {
            sign,
            exponent,
            mantissa,
        } = encode_components(value)?;

        // mantissa \in [1/16, 1) so mantissa × 2^56 \in [2^52, 2^56) -> fits in u64, non-negative.
        #[expect(
            clippy::cast_precision_loss,
            reason = "1u64 << 56 is a power of 2, exactly representable as f64"
        )]
        #[expect(
            clippy::cast_possible_truncation,
            reason = "mantissa × 2^56 is in [2^52, 2^56), fits in u64"
        )]
        #[expect(clippy::cast_sign_loss, reason = "mantissa is always positive")]
        let m = (mantissa * (1u64 << 56) as f64) as u64;
        Ok(Self([
            sign | exponent,
            ((m >> 48) & 0xFF) as u8,
            ((m >> 40) & 0xFF) as u8,
            ((m >> 32) & 0xFF) as u8,
            ((m >> 24) & 0xFF) as u8,
            ((m >> 16) & 0xFF) as u8,
            ((m >> 8) & 0xFF) as u8,
            (m & 0xFF) as u8,
        ]))
    }
}

#[derive(Debug)]
struct ParsedRealComponents {
    sign: u8,
    exponent: u8,
    mantissa: f64,
}

/// Compute GDSII excess-64 encoding components for a non-zero finite `f64`.
///
/// Caller must ensure `value` is non-zero and finite.
fn encode_components(value: f64) -> Result<ParsedRealComponents, NotRepresentable> {
    debug_assert!(value.is_finite() && value != 0.0);

    let sign = if value.is_sign_negative() { 0x80 } else { 0x00 };
    let abs = value.abs();

    // Find the base-16 exponent: smallest exp such that abs / 16^exp \in [1/16, 1).
    // log2(abs) / 4 gives log16(abs); floor + 1 gives the ceiling.
    // For finite non-zero f64, log2/4 is in ~[-255, 255], well within i32.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "log16 of finite non-zero f64 is in [-255, 255], always fits in i32"
    )]
    let Some(mut exp) = ((abs.log2() / 4.0).floor() as i32).checked_add(1) else {
        return Err(NotRepresentable);
    };

    let mut mantissa = abs / 16_f64.powi(exp);

    // Correct for floating-point imprecision at normalization boundaries.
    // Bounded: at most 1 iteration each (log16 estimate is off by at most 1 ULP).
    #[expect(
        clippy::while_float,
        reason = "bounded normalization loop, at most 1 iteration"
    )]
    while mantissa >= 1.0 {
        exp += 1;
        mantissa /= 16.0;
    }
    while mantissa > 0.0 && mantissa < 1.0 / 16.0 {
        exp -= 1;
        mantissa *= 16.0;
    }

    // 7-bit excess-64 field encodes actual exponents in [-64, 63].
    if !(-64..=63).contains(&exp) {
        return Err(NotRepresentable);
    }

    Ok(ParsedRealComponents {
        sign,
        // exp + 64 is in [0, 127] by the bounds check above.
        exponent: u8::try_from(exp + 64).expect("exp + 64 is in [0, 127]"),
        mantissa,
    })
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::*;

    #[test]
    fn eight_byte_real_zero() {
        assert!((f64::from(GdsEightByteReal([0u8; 8])) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn four_byte_real_zero() {
        assert!((f64::from(GdsFourByteReal([0u8; 4])) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn eight_byte_real_one() {
        // 1.0 = 0.0625 × 16^1 -> exponent byte = 64 + 1 = 0x41, mantissa = 0x10_0000_0000_0000
        let r = GdsEightByteReal([0x41, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let v = f64::from(r);
        assert!((v - 1.0).abs() < 1e-15, "expected 1.0, got {v}");
    }

    #[test]
    fn eight_byte_real_negative_one() {
        // -1.0: sign bit set -> 0xC1 first byte
        let r = GdsEightByteReal([0xC1, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        let v = f64::from(r);
        assert!((v + 1.0).abs() < 1e-15, "expected -1.0, got {v}");
    }

    #[test]
    fn encode_decode_eight_byte_round_trip_manual() {
        for &x in &[1.0f64, -1.0, 0.5, 1.5, 0.1, 1e10, -1e-10, 42.0] {
            let encoded = GdsEightByteReal::try_from(x).expect("should encode");
            let decoded = f64::from(encoded);
            let rel_err = (decoded - x).abs() / x.abs();
            assert!(
                rel_err < 1e-14,
                "round-trip failed for {x}: got {decoded}, rel_err={rel_err}"
            );
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

    #[quickcheck]
    fn qc_eight_byte_round_trip(x: f64) -> bool {
        if !x.is_finite() {
            return GdsEightByteReal::try_from(x).is_err();
        }
        if x == 0.0 {
            let r = GdsEightByteReal::try_from(x).expect("zero encodes");
            return f64::from(r) == 0.0;
        }
        GdsEightByteReal::try_from(x).map_or(true, |r| {
            let decoded = f64::from(r);
            (decoded - x).abs() / x.abs() < 1e-13
        })
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
