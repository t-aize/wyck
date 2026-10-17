//! Currencies: the flag of a currency, and the two currencies of a pair such as `EURUSD`.

use super::Mark;
use super::paint::hashed_color;

/// The flag of the country whose currency this is.
pub(super) fn currency_flag(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_uppercase().as_str() {
        "EUR" => "eu",
        "USD" => "us",
        "GBP" => "gb",
        "JPY" => "jp",
        "CHF" => "ch",
        "AUD" => "au",
        "NZD" => "nz",
        "CAD" => "ca",
        "CNH" | "CNY" => "cn",
        "HKD" => "hk",
        "SGD" => "sg",
        "SEK" => "se",
        "NOK" => "no",
        "DKK" => "dk",
        "PLN" => "pl",
        "CZK" => "cz",
        "HUF" => "hu",
        "TRY" => "tr",
        "ZAR" => "za",
        "MXN" => "mx",
        "ILS" => "il",
        "THB" => "th",
        "INR" => "in",
        "KRW" => "kr",
        "BRL" => "br",
        "RUB" => "ru",
        _ => return None,
    })
}

/// A currency as a mark: its flag, or its letters when it has none.
pub(super) fn currency_mark(code: &str) -> Mark {
    currency_flag(code).map_or_else(
        || Mark::Letters {
            text: code
                .chars()
                .take(3)
                .collect::<String>()
                .to_ascii_uppercase(),
            color: hashed_color(code),
        },
        Mark::Flag,
    )
}

/// The two currencies of a six-letter ticker such as `EURUSD`.
pub(super) fn split_pair(ticker: &str) -> Option<(String, String)> {
    let ticker = ticker.trim();
    if ticker.len() != 6 || !ticker.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let (base, quote) = ticker.split_at_checked(3)?;
    Some((base.to_ascii_uppercase(), quote.to_ascii_uppercase()))
}
