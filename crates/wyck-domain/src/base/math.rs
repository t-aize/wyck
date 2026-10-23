//! The conversions between the integer and float types the app mixes, each written down here
//! instead of an unmarked `as` cast: what rounds, what saturates, and where a value is exact.

/// A count (a period, a series length) as an `f64`.
#[expect(
    clippy::cast_precision_loss,
    reason = "a period or a series length is far below 2^53, so the conversion is exact"
)]
#[must_use]
pub const fn count_f64(count: usize) -> f64 {
    count as f64
}

/// An `i64` as an `f64`, rounded to the nearest representable value exactly as `value as f64`
/// does, but without the cast: the two halves of the integer are each exact in an `f64`, so their
/// sum is rounded once, by the hardware, the same way. Prices and volumes go through here, so that
/// the conversion is written down in one place instead of being an unmarked `as`.
#[must_use]
pub fn i64_to_f64(value: i64) -> f64 {
    let [h3, h2, h1, h0, l3, l2, l1, l0] = value.to_be_bytes();
    let high = i32::from_be_bytes([h3, h2, h1, h0]);
    let low = u32::from_be_bytes([l3, l2, l1, l0]);
    f64::from(high) * TWO_POW_32 + f64::from(low)
}

/// A `u64` as an `f64`, rounded like `value as f64`; see [`i64_to_f64`].
#[must_use]
pub fn u64_to_f64(value: u64) -> f64 {
    let [h3, h2, h1, h0, l3, l2, l1, l0] = value.to_be_bytes();
    let high = u32::from_be_bytes([h3, h2, h1, h0]);
    let low = u32::from_be_bytes([l3, l2, l1, l0]);
    f64::from(high) * TWO_POW_32 + f64::from(low)
}

const TWO_POW_32: f64 = 4_294_967_296.0;

/// A coordinate, a size or a factor worked out in `f64` as the `f32` the screen uses. The rounding
/// is far below one pixel. For display only: money stays in `f64` and goes through [`f64_to_i64`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "screen coordinates are f32 and the rounding of the conversion is far below a pixel"
)]
#[must_use]
pub const fn f32_of(value: f64) -> f32 {
    value as f32
}

/// An `f64` as a `usize` with the semantics of `as`: the fraction is cut, `NaN` and negative
/// values give 0 and a value out of range clamps. The caller rounds or floors first.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the saturating cast is the wanted behaviour: NaN and negatives give 0, too large clamps"
)]
#[must_use]
pub const fn f64_to_usize(value: f64) -> usize {
    value as usize
}

/// An `f32` as a `usize` with the semantics of `as`; see [`f64_to_usize`].
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the saturating cast is the wanted behaviour: NaN and negatives give 0, too large clamps"
)]
#[must_use]
pub const fn f32_to_usize(value: f32) -> usize {
    value as usize
}

/// An `f64` as a `u64` with the semantics of `as`: the fraction is cut, `NaN` and negative values
/// give 0 and a value out of range clamps. The caller rounds or floors first.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the saturating cast is the wanted behaviour: NaN and negatives give 0, too large clamps"
)]
#[must_use]
pub const fn f64_to_u64(value: f64) -> u64 {
    value as u64
}

/// An `f32` as an `i32` with the semantics of `as`: the fraction is cut, `NaN` gives 0 and a value
/// out of range clamps. The caller rounds or floors first.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the saturating cast is the wanted behaviour: NaN gives 0 and out of range clamps"
)]
#[must_use]
pub const fn f32_to_i32(value: f32) -> i32 {
    value as i32
}

/// An `f32` as an `i64`; see [`f32_to_i32`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "the saturating cast is the wanted behaviour: NaN gives 0 and out of range clamps"
)]
#[must_use]
pub const fn f32_to_i64(value: f32) -> i64 {
    value as i64
}

/// An `f32` as a `u32` with the semantics of `as`; see [`f64_to_u32`].
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the saturating cast is the wanted behaviour: NaN and negatives give 0, too large clamps"
)]
#[must_use]
pub const fn f32_to_u32(value: f32) -> u32 {
    value as u32
}

/// A `u64` (a volume, a count of trades) as an `f32` for a size on the screen, rounded exactly as
/// `value as f32` rounds it.
#[expect(
    clippy::cast_precision_loss,
    reason = "the value is only drawn: a bar width or a ratio, where the f32 precision is plenty"
)]
#[must_use]
pub const fn u64_to_f32(value: u64) -> f32 {
    value as f32
}

/// An `i64` (a time or a count) as an `f32` for a position on the screen, rounded exactly as
/// `value as f32` rounds it.
#[expect(
    clippy::cast_precision_loss,
    reason = "the value is only drawn: a position or a size, where the f32 precision is plenty"
)]
#[must_use]
pub const fn i64_to_f32(value: i64) -> f32 {
    value as f32
}

/// A `u32` (a cell of a grid, a pixel count) as an `f32` for a position or a size on the screen.
#[expect(
    clippy::cast_precision_loss,
    reason = "a cell or a pixel count is far below 2^24, so the conversion is exact"
)]
#[must_use]
pub const fn u32_to_f32(value: u32) -> f32 {
    value as f32
}

/// A count as an `i64`. A length never comes near `i64::MAX`; past it the result clamps.
#[must_use]
pub fn count_i64(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

/// A count (an index, a number of bars) as an `f32` for a coordinate or a factor.
#[expect(
    clippy::cast_precision_loss,
    reason = "a count of bars or points is far below 2^24, so the conversion is exact"
)]
#[must_use]
pub const fn count_f32(count: usize) -> f32 {
    count as f32
}

/// An `f64` as an `i64` with the semantics of `as`: the fraction is cut, `NaN` gives 0 and a value
/// out of range clamps to `i64::MIN` or `i64::MAX`. The caller rounds or floors first. This is the
/// one place where prices, volumes and amounts of money leave the floating point world.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the saturating cast is the wanted behaviour: NaN gives 0 and out of range clamps"
)]
#[must_use]
pub const fn f64_to_i64(value: f64) -> i64 {
    value as i64
}

/// An `f64` as a `u32` with the semantics of `as`: the fraction is cut, `NaN` and negative values
/// give 0 and a value out of range clamps to `u32::MAX`. The caller rounds or floors first.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the saturating cast is the wanted behaviour: NaN and negatives give 0, too large clamps"
)]
#[must_use]
pub const fn f64_to_u32(value: f64) -> u32 {
    value as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_leave_the_float_world_by_saturating() {
        assert_eq!(f64_to_i64(1.9), 1);
        assert_eq!(f64_to_i64(-1.9), -1);
        assert_eq!(f64_to_i64(1e30), i64::MAX);
        assert_eq!(f64_to_i64(-1e30), i64::MIN);
        assert_eq!(f64_to_i64(f64::INFINITY), i64::MAX);
        assert_eq!(f64_to_i64(f64::NAN), 0);
        assert_eq!(f64_to_u32(1.9), 1);
        assert_eq!(f64_to_u32(-1.9), 0);
        assert_eq!(f64_to_u32(4_294_967_296.0), u32::MAX);
        assert_eq!(f64_to_u32(f64::NAN), 0);
        assert_eq!(f64_to_usize(2.9), 2);
        assert_eq!(f64_to_usize(-2.9), 0);
        assert_eq!(f64_to_usize(f64::NAN), 0);
        assert_eq!(f32_to_usize(7.9), 7);
        assert_eq!(f32_to_usize(-1.0), 0);
        assert_eq!(count_i64(5), 5);
        assert_eq!(f64_to_u64(3.7), 3);
        assert_eq!(f64_to_u64(-3.7), 0);
        assert_eq!(f32_to_i32(-2.5), -2);
        assert_eq!(f32_to_i32(f32::NAN), 0);
        assert_eq!(f32_to_i64(1e30), i64::MAX);
        assert_eq!(f32_to_u32(-1.0), 0);
        assert_eq!(f32_to_u32(9.9), 9);
        assert_eq!(u64_to_f32(3), 3.0);
        assert_eq!(i64_to_f32(-3), -3.0);
        assert_eq!(u32_to_f32(7), 7.0);
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "the test compares the helpers with the cast they replace"
    )]
    fn integers_convert_to_floats_like_the_cast_does() {
        let edges = [
            0i64,
            1,
            -1,
            (1 << 53) - 1,
            1 << 53,
            (1 << 53) + 1,
            (1 << 53) + 3,
            -(1 << 53) - 1,
            -(1 << 53) - 3,
            i64::MAX,
            i64::MAX - 1,
            i64::MIN,
            i64::MIN + 1,
            108_501,
            2_965_960,
        ];
        for v in edges {
            assert_eq!(i64_to_f64(v).to_bits(), (v as f64).to_bits(), "{v}");
            assert_eq!(
                u64_to_f64(v.unsigned_abs()).to_bits(),
                (v.unsigned_abs() as f64).to_bits(),
                "{v}"
            );
        }
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let signed = i64::from_be_bytes(state.to_be_bytes());
            assert_eq!(i64_to_f64(signed).to_bits(), (signed as f64).to_bits());
            assert_eq!(u64_to_f64(state).to_bits(), (state as f64).to_bits());
            let shifted = signed >> (state % 63);
            assert_eq!(i64_to_f64(shifted).to_bits(), (shifted as f64).to_bits());
        }
    }
}
