//! Money written for people: `1,234.56 USD`, `$1,234.56`.

use wyck_core::math::f64_to_i64;

/// An amount of money with its currency: `1,234.56 USD`, `-12.30 EUR`.
pub fn format_money(amount: f64, currency: &str) -> String {
    let negative = amount < 0.0;
    let cents = f64_to_i64((amount.abs() * 100.0).round());
    let (whole, rest) = (cents / 100, cents % 100);
    let grouped = wyck_core::format::grouped(whole);
    let sign = if negative && cents > 0 { "-" } else { "" };
    if currency.is_empty() {
        format!("{sign}{grouped}.{rest:02}")
    } else {
        format!("{sign}{grouped}.{rest:02} {currency}")
    }
}

/// An amount in the short form of a screen of figures: the symbol of the currency when it has one
/// (`$1,234.56`, `\u{20AC}12.30`), else the code after it (`1,234.56 CHF`). With `signed`, a gain
/// carries a `+` (`+$114.00`); a loss always carries its `-`.
pub fn format_amount(amount: f64, currency: &str, signed: bool) -> String {
    let cents = f64_to_i64((amount.abs() * 100.0).round());
    let sign = if amount < 0.0 && cents > 0 {
        "-"
    } else if signed && cents > 0 {
        "+"
    } else {
        ""
    };
    let figures = format!(
        "{}.{:02}",
        wyck_core::format::grouped(cents / 100),
        cents % 100
    );
    match currency {
        "USD" => format!("{sign}${figures}"),
        "EUR" => format!("{sign}\u{20AC}{figures}"),
        "GBP" => format!("{sign}\u{A3}{figures}"),
        "" => format!("{sign}{figures}"),
        code => format!("{sign}{figures} {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_amount_is_written_with_the_symbol_of_its_currency() {
        assert_eq!(format_amount(1_234.5, "USD", false), "$1,234.50");
        assert_eq!(format_amount(114.0, "USD", true), "+$114.00");
        assert_eq!(format_amount(-12.3, "EUR", true), "-\u{20AC}12.30");
        assert_eq!(format_amount(5.0, "GBP", false), "\u{A3}5.00");
        assert_eq!(format_amount(7.5, "CHF", true), "+7.50 CHF");
        assert_eq!(format_amount(0.001, "USD", true), "$0.00");
        assert_eq!(format_amount(-0.001, "USD", true), "$0.00");
        assert_eq!(format_amount(3.0, "", true), "+3.00");
    }

    #[test]
    fn money_is_grouped_and_signed() {
        assert_eq!(format_money(1_234.5, "USD"), "1,234.50 USD");
        assert_eq!(format_money(-12.3, "EUR"), "-12.30 EUR");
        assert_eq!(format_money(-0.001, ""), "0.00");
        assert_eq!(format_money(1_000_000.0, ""), "1,000,000.00");
    }
}
