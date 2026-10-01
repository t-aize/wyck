//! How the app writes a number: a fixed most of decimals, no trailing zeros.

/// `value` with at most `decimals` decimals and no trailing zeros. `-0` is written `0`.
pub fn trim(value: f64, decimals: usize) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    text
}

/// A whole number with its thousands apart: `1234567` as `1,234,567`.
pub fn grouped(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

/// [`trim`] with a `+` in front of a value above zero.
pub fn signed(value: f64, decimals: usize) -> String {
    let text = trim(value, decimals);
    if value > 0.0 && text != "0" {
        format!("+{text}")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_zeros_go_and_whole_numbers_stay() {
        assert_eq!(trim(1.5000, 4), "1.5");
        assert_eq!(trim(2.0, 2), "2");
        assert_eq!(trim(100.0, 0), "100");
        assert_eq!(trim(0.0004, 3), "0");
        assert_eq!(trim(-0.0004, 3), "0");
        assert_eq!(trim(-1.26, 1), "-1.3");
    }

    #[test]
    fn numbers_are_grouped_by_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(10_000_000), "10,000,000");
        assert_eq!(grouped(-1_234_567), "-1,234,567");
    }

    #[test]
    fn a_sign_shows_above_zero_only() {
        assert_eq!(signed(1.5, 2), "+1.5");
        assert_eq!(signed(-1.5, 2), "-1.5");
        assert_eq!(signed(0.0001, 2), "0");
    }
}
