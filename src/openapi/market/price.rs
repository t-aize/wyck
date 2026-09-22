//! Converting between the server's integer prices and real numbers, and formatting them for
//! display.
//!
//! Prices travel as integers scaled by [`PRICE_SCALE`] (100 000): the price `1.08501` is `108501`
//! on the wire. Keep the integer for exact work and comparisons; the `f64` from [`to_price`] is for
//! display and arithmetic that can tolerate floating point error.

/// The factor between the server's integer prices and real prices.
pub const PRICE_SCALE: i64 = 100_000;

/// The number of raw price units in a price of 1.0 (an alias for [`PRICE_SCALE`], kept next to
/// [`format_price`] for callers that think in terms of formatting rather than decoding).
pub const UNITS_PER_PRICE: i64 = PRICE_SCALE;

/// The raw price `raw` as a real price.
///
/// ```
/// assert_eq!(wyck::openapi::market::to_price(108_501), 1.08501);
/// ```
#[must_use]
pub fn to_price(raw: i64) -> f64 {
    raw as f64 / PRICE_SCALE as f64
}

/// A real price as the server's integer, rounded to the nearest unit.
///
/// ```
/// assert_eq!(wyck::openapi::market::from_price(1.08501), 108_501);
/// ```
#[must_use]
pub fn from_price(price: f64) -> i64 {
    (price * PRICE_SCALE as f64).round() as i64
}

/// ```
/// use wyck::openapi::market::format_price;
///
/// assert_eq!(format_price(114_880, 5), "1.14880");
/// assert_eq!(format_price(114_886, 4), "1.1489"); // rounded to the decimals asked for
/// assert_eq!(format_price(15_676_800, 3), "156.768");
/// ```
///
/// A raw price as text with `digits` decimals, as the symbol quotes it (`114880` with 5 digits is
/// `1.14880`, with 3 digits `1.149`). The price is rounded to the decimals asked for.
///
/// `digits` above 5 is treated as 5: the server's integers only carry five decimals.
#[must_use]
pub fn format_price(raw: i64, digits: u32) -> String {
    let digits = digits.min(5);
    let drop = 10i128.pow(5 - digits);
    let scaled = i128::from(raw);
    // Round half away from zero at the last decimal kept.
    let rounded = if scaled >= 0 {
        (scaled + drop / 2) / drop
    } else {
        (scaled - drop / 2) / drop
    };
    let unit = 10i128.pow(digits);
    let sign = if rounded < 0 { "-" } else { "" };
    let magnitude = rounded.abs();
    if digits == 0 {
        return format!("{sign}{magnitude}");
    }
    format!(
        "{sign}{}.{:0width$}",
        magnitude / unit,
        magnitude % unit,
        width = digits as usize
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_convert_both_ways_without_drift() {
        assert_eq!(to_price(108_501), 1.08501);
        assert_eq!(from_price(1.08501), 108_501);
        assert_eq!(from_price(to_price(2_965_960)), 2_965_960);
        assert_eq!(from_price(-0.5), -50_000);
    }

    #[test]
    fn prices_are_formatted_with_the_symbols_decimals() {
        assert_eq!(format_price(114_880, 5), "1.14880");
        assert_eq!(format_price(114_880, 4), "1.1488");
        assert_eq!(format_price(114_886, 4), "1.1489");
        assert_eq!(format_price(15_676_800, 3), "156.768");
        assert_eq!(format_price(265_000_000, 2), "2650.00");
        assert_eq!(format_price(265_000_000, 0), "2650");
        assert_eq!(format_price(0, 5), "0.00000");
        assert_eq!(format_price(5, 5), "0.00005");
        assert_eq!(format_price(-114_880, 5), "-1.14880");
        assert_eq!(
            format_price(-50, 2),
            "0.00",
            "a value that rounds to zero has no sign"
        );
        assert_eq!(
            format_price(114_880, 9),
            "1.14880",
            "more than five decimals is five"
        );
    }
}
