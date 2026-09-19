//! Currency codes, and deriving "the currencies I actually trade" from symbol names.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The currencies the ForexFactory feed publishes events for.
const CALENDAR_CURRENCIES: [Currency; 9] = [
    Currency::AUD,
    Currency::CAD,
    Currency::CHF,
    Currency::CNY,
    Currency::EUR,
    Currency::GBP,
    Currency::JPY,
    Currency::NZD,
    Currency::USD,
];

/// A three-letter, upper-case ASCII currency code (`USD`, `EUR`, …).
///
/// `Copy`, hashable and ordered, so it is cheap to keep in a filter set. Any three ASCII
/// letters are accepted by [`FromStr`] (the feed could add a currency tomorrow);
/// [`Currency::is_calendar_currency`] says whether it is one the feed is known to cover.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Currency([u8; 3]);

impl Currency {
    /// Australian dollar.
    pub const AUD: Self = Self(*b"AUD");
    /// Canadian dollar.
    pub const CAD: Self = Self(*b"CAD");
    /// Swiss franc.
    pub const CHF: Self = Self(*b"CHF");
    /// Chinese yuan.
    pub const CNY: Self = Self(*b"CNY");
    /// Euro.
    pub const EUR: Self = Self(*b"EUR");
    /// British pound.
    pub const GBP: Self = Self(*b"GBP");
    /// Japanese yen.
    pub const JPY: Self = Self(*b"JPY");
    /// New Zealand dollar.
    pub const NZD: Self = Self(*b"NZD");
    /// US dollar.
    pub const USD: Self = Self(*b"USD");

    /// Every currency the feed is known to publish events for.
    #[must_use]
    pub fn calendar_currencies() -> &'static [Currency] {
        &CALENDAR_CURRENCIES
    }

    /// The code as a string slice, e.g. `"USD"`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // Invariant: the bytes are always ASCII letters (checked in `from_bytes`).
        std::str::from_utf8(&self.0).unwrap_or("???")
    }

    /// Whether the feed is known to publish events for this currency.
    #[must_use]
    pub fn is_calendar_currency(self) -> bool {
        CALENDAR_CURRENCIES.contains(&self)
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let code: [u8; 3] = bytes.try_into().ok()?;
        code.iter()
            .all(u8::is_ascii_alphabetic)
            .then(|| Self(code.map(|b| b.to_ascii_uppercase())))
    }
}

impl fmt::Debug for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Currency").field(&self.as_str()).finish()
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A string that is not exactly three ASCII letters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("`{0}` is not a three-letter currency code")]
pub struct ParseCurrencyError(String);

impl FromStr for Currency {
    type Err = ParseCurrencyError;

    /// Case-insensitive; surrounding whitespace is ignored.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_bytes(s.trim().as_bytes()).ok_or_else(|| ParseCurrencyError(s.to_owned()))
    }
}

impl TryFrom<String> for Currency {
    type Error = ParseCurrencyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Currency> for String {
    fn from(value: Currency) -> Self {
        value.as_str().to_owned()
    }
}

/// The calendar currencies making up a trading symbol.
///
/// Non-letters are ignored (`EUR/USD`, `EUR_USD` and `EURUSD.r` all work) and the first
/// six letters are read as `BASE` + `QUOTE`. Halves that are not a calendar currency are
/// dropped, so `XAUUSD` yields just `USD`. Symbols that do not follow the six-letter
/// pattern (indices such as `US30`, crypto tickers) yield nothing.
///
/// ```
/// use wyck_calendar::{Currency, currencies_from_symbol};
///
/// assert_eq!(currencies_from_symbol("GBP/JPY"), [Currency::GBP, Currency::JPY]);
/// assert_eq!(currencies_from_symbol("XAUUSD"), [Currency::USD]);
/// assert!(currencies_from_symbol("US30").is_empty());
/// ```
#[must_use]
pub fn currencies_from_symbol(symbol: &str) -> Vec<Currency> {
    let letters: Vec<u8> = symbol
        .bytes()
        .filter(u8::is_ascii_alphabetic)
        .map(|b| b.to_ascii_uppercase())
        .take(6)
        .collect();
    if letters.len() < 6 {
        return Vec::new();
    }
    letters
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|code| Currency::from_bytes(code))
        .filter(|c| c.is_calendar_currency())
        .collect()
}

/// The union of [`currencies_from_symbol`] over many symbols — the natural default for
/// [`crate::EventFilter::currencies`]: "the currencies I actually trade".
#[must_use]
pub fn currencies_from_symbols<I, S>(symbols: I) -> BTreeSet<Currency>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    symbols
        .into_iter()
        .flat_map(|s| currencies_from_symbol(s.as_ref()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_case_insensitively_and_trims() {
        assert_eq!(" usd ".parse::<Currency>(), Ok(Currency::USD));
        assert_eq!(
            "Eur".parse::<Currency>().map(|c| c.to_string()),
            Ok("EUR".to_owned())
        );
    }

    #[test]
    fn rejects_malformed_codes() {
        for bad in ["", "US", "USDX", "U5D", "€UR"] {
            assert!(bad.parse::<Currency>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn serde_round_trips_as_a_plain_string() {
        let json = serde_json::to_string(&Currency::GBP).unwrap();
        assert_eq!(json, "\"GBP\"");
        assert_eq!(
            serde_json::from_str::<Currency>("\"jpy\"").unwrap(),
            Currency::JPY
        );
        assert!(serde_json::from_str::<Currency>("\"JAPAN\"").is_err());
    }

    #[test]
    fn symbols_map_to_calendar_currencies() {
        assert_eq!(
            currencies_from_symbol("EURUSD"),
            [Currency::EUR, Currency::USD]
        );
        assert_eq!(
            currencies_from_symbol("eur_usd"),
            [Currency::EUR, Currency::USD]
        );
        assert_eq!(
            currencies_from_symbol("EURUSD.r"),
            [Currency::EUR, Currency::USD]
        );
        assert_eq!(currencies_from_symbol("XAUUSD"), [Currency::USD]);
        assert!(currencies_from_symbol("BTCETH").is_empty());
        assert!(currencies_from_symbol("US30").is_empty());
        assert!(currencies_from_symbol("").is_empty());
    }

    #[test]
    fn symbol_set_is_deduplicated_and_ordered() {
        let set = currencies_from_symbols(["EURUSD", "GBPUSD", "USDJPY"]);
        let codes: Vec<_> = set.iter().map(Currency::as_str).collect();
        assert_eq!(codes, ["EUR", "GBP", "JPY", "USD"]);
    }
}
