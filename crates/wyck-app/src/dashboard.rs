//! What the dashboard header shows, computed from the engine's state and free of any UI toolkit.
//!
//! The header names the traded symbol and its price. Everything in it comes from the engine: the
//! currencies from the instrument (or, failing that, from the six letters of a forex ticker), the
//! price and the spread from the latest quote. **Nothing is invented**: a symbol whose quote has
//! not arrived yet has no price, and the header says so instead of showing a stale or made-up
//! number. The daily change of the price is not here because the engine does not have it yet: it
//! needs candles.
//!
//! [`Timeframe`] is the chart's time frame selector, and knows the [`Period`] each one asks the
//! servers for.

use wyck_engine::EngineState;
use wyck_engine::domain::{Period, SymbolInfo};

use crate::presentation::quote_line;
use crate::symbols::{SymbolIcon, classify, icon_for};

/// The time frame of the chart. Each one is a period the servers serve natively, so its bars come
/// from the server as the broker cuts them (sessions and time zones included) and are never
/// rebuilt from smaller ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Timeframe {
    /// One minute.
    M1,
    /// Five minutes.
    M5,
    /// Fifteen minutes.
    M15,
    /// Thirty minutes.
    M30,
    /// One hour.
    #[default]
    H1,
    /// Four hours.
    H4,
    /// One day.
    D1,
    /// One week.
    W1,
    /// One month.
    MN1,
}

impl Timeframe {
    /// Every time frame, shortest first, in the order the selector shows them.
    pub const ALL: [Self; 9] = [
        Self::M1,
        Self::M5,
        Self::M15,
        Self::M30,
        Self::H1,
        Self::H4,
        Self::D1,
        Self::W1,
        Self::MN1,
    ];

    /// The label on the selector.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::M30 => "30m",
            Self::H1 => "1H",
            Self::H4 => "4H",
            Self::D1 => "1D",
            Self::W1 => "1W",
            Self::MN1 => "1M",
        }
    }

    /// The period the servers know it by.
    #[must_use]
    pub fn period(self) -> Period {
        match self {
            Self::M1 => Period::M1,
            Self::M5 => Period::M5,
            Self::M15 => Period::M15,
            Self::M30 => Period::M30,
            Self::H1 => Period::H1,
            Self::H4 => Period::H4,
            Self::D1 => Period::D1,
            Self::W1 => Period::W1,
            Self::MN1 => Period::MN1,
        }
    }
}

/// Which way the price moved at its last change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// It rose.
    Up,
    /// It fell.
    Down,
}

/// The direction from `previous` to `now`, or `None` when the price did not change (or one of
/// them is not a number).
#[must_use]
pub fn tick(previous: f64, now: f64) -> Option<Tick> {
    if previous.is_nan() || now.is_nan() {
        None
    } else if now > previous {
        Some(Tick::Up)
    } else if now < previous {
        Some(Tick::Down)
    } else {
        None
    }
}

/// The left side of the dashboard header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolHeader {
    /// The ticker, as it is traded.
    pub symbol: String,
    /// The mark in the tile: the symbol of the base currency (`$`, an euro sign) or its first letter.
    pub mark: String,
    /// The long name: the broker's description when the symbol list is loaded, else `Euro / US
    /// Dollar` read from the currencies, else the ticker.
    pub name: String,
    /// The icon: two flags for a pair, a flag or an asset icon for the rest.
    pub icon: SymbolIcon,
    /// The latest bid with the instrument's digits, once a quote has arrived.
    pub price: Option<String>,
    /// The spread in pips, once the quote and the instrument are known.
    pub spread_pips: Option<String>,
}

/// Builds the header of `symbol` from the engine state, and from the broker's entry for it once the
/// list of symbols has been loaded.
#[must_use]
pub fn symbol_header(
    state: &EngineState,
    symbol: &str,
    listed: Option<&SymbolInfo>,
) -> SymbolHeader {
    let instrument = state.instruments.get(symbol);
    let (base, quote) = match instrument {
        Some(i) => (i.base_currency.clone(), i.quote_currency.clone()),
        None => (None, None),
    };
    let (base, quote) = match (base, quote) {
        (Some(base), Some(quote)) => (Some(base), Some(quote)),
        _ => forex_pair(symbol).map_or((None, None), |(b, q)| (Some(b), Some(q))),
    };

    let mark = base
        .as_deref()
        .map_or_else(|| first_letter(symbol), currency_mark);
    let described = listed
        .and_then(|l| l.description.clone())
        .filter(|d| !d.trim().is_empty());
    let name = described.unwrap_or_else(|| match (&base, &quote) {
        (Some(base), Some(quote)) => format!("{} / {}", currency_name(base), currency_name(quote)),
        _ => symbol.to_owned(),
    });
    let mut info = listed.cloned().unwrap_or_else(|| SymbolInfo::named(symbol));
    info.base_currency = info.base_currency.or_else(|| base.clone());
    info.quote_currency = info.quote_currency.or_else(|| quote.clone());
    let class = classify(&info);
    let icon = icon_for(&info, class);
    let line = quote_line(state, symbol);
    SymbolHeader {
        symbol: symbol.to_owned(),
        mark,
        name,
        icon,
        price: line.as_ref().map(|l| l.bid.clone()),
        spread_pips: line.and_then(|l| l.spread_pips),
    }
}

/// The two currencies of a six-letter forex ticker such as `EURUSD`, upper-cased.
fn forex_pair(symbol: &str) -> Option<(String, String)> {
    let symbol = symbol.trim();
    if symbol.len() == 6 && symbol.chars().all(|c| c.is_ascii_alphabetic()) {
        let upper = symbol.to_ascii_uppercase();
        Some((upper[..3].to_owned(), upper[3..].to_owned()))
    } else {
        None
    }
}

fn first_letter(text: &str) -> String {
    text.chars()
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().collect())
}

/// The name of a currency, or its code when it is not one of the common ones.
#[must_use]
pub fn currency_name(code: &str) -> String {
    let name = match code.to_ascii_uppercase().as_str() {
        "EUR" => "Euro",
        "USD" => "US Dollar",
        "GBP" => "British Pound",
        "JPY" => "Japanese Yen",
        "CHF" => "Swiss Franc",
        "AUD" => "Australian Dollar",
        "NZD" => "New Zealand Dollar",
        "CAD" => "Canadian Dollar",
        "CNH" | "CNY" => "Chinese Yuan",
        "HKD" => "Hong Kong Dollar",
        "SGD" => "Singapore Dollar",
        "SEK" => "Swedish Krona",
        "NOK" => "Norwegian Krone",
        "DKK" => "Danish Krone",
        "PLN" => "Polish Zloty",
        "CZK" => "Czech Koruna",
        "HUF" => "Hungarian Forint",
        "TRY" => "Turkish Lira",
        "ZAR" => "South African Rand",
        "MXN" => "Mexican Peso",
        _ => return code.to_ascii_uppercase(),
    };
    name.to_owned()
}

/// The mark of a currency for the tile: its sign when it has a well known one, else its first
/// letter. Non-ASCII signs are written as escapes so this file stays ASCII.
#[must_use]
pub fn currency_mark(code: &str) -> String {
    match code.to_ascii_uppercase().as_str() {
        "EUR" => "\u{20AC}".to_owned(),
        "GBP" => "\u{A3}".to_owned(),
        "JPY" | "CNH" | "CNY" => "\u{A5}".to_owned(),
        "USD" | "AUD" | "NZD" | "CAD" | "HKD" | "SGD" | "MXN" => "$".to_owned(),
        "CHF" => "Fr".to_owned(),
        "TRY" => "\u{20BA}".to_owned(),
        _ => first_letter(code),
    }
}

#[cfg(test)]
mod tests {
    use wyck_engine::EngineConfig;
    use wyck_engine::domain::{Instrument, Quote, SpecsSource, Volume, VolumeSpecs};

    use super::*;

    fn blank_state() -> EngineState {
        let config = EngineConfig {
            ..EngineConfig::default()
        };
        let engine = wyck_engine::Engine::start(config).unwrap();
        let state = (*engine.handle().state()).clone();
        drop(engine);
        state
    }

    fn eurusd() -> Instrument {
        Instrument {
            symbol: "EURUSD".to_owned(),
            symbol_id: Some(1),
            price_digits: 5,
            pip_size: 0.0001,
            base_currency: Some("EUR".to_owned()),
            quote_currency: Some("USD".to_owned()),
            enabled: true,
            volume: VolumeSpecs {
                lot_size: 100_000.0,
                min: Volume::from_units(1_000),
                step: Volume::from_units(1_000),
                max: None,
            },
            specs_source: SpecsSource::Broker,
        }
    }

    fn with_quote(bid: f64, ask: f64) -> EngineState {
        let mut state = blank_state();
        state.instruments.insert("EURUSD".to_owned(), eurusd());
        state.quotes.insert(
            "EURUSD".to_owned(),
            Quote {
                symbol: "EURUSD".to_owned(),
                bid,
                ask,
                timestamp: None,
            },
        );
        state
    }

    #[test]
    fn the_default_time_frame_is_one_hour_and_all_are_listed_shortest_first() {
        assert_eq!(Timeframe::default(), Timeframe::H1);
        let labels: Vec<_> = Timeframe::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(
            labels,
            ["1m", "5m", "15m", "30m", "1H", "4H", "1D", "1W", "1M"]
        );
        assert!(Timeframe::ALL.contains(&Timeframe::default()));
    }

    #[test]
    fn a_price_that_moves_has_a_direction_and_one_that_does_not_has_none() {
        assert_eq!(tick(1.0842, 1.0843), Some(Tick::Up));
        assert_eq!(tick(1.0843, 1.0842), Some(Tick::Down));
        assert_eq!(tick(1.0842, 1.0842), None);
        assert_eq!(tick(f64::NAN, 1.0), None);
    }

    #[test]
    fn a_known_instrument_gives_a_named_pair_a_price_and_a_spread() {
        let header = symbol_header(&with_quote(1.08423, 1.08431), "EURUSD", None);
        assert_eq!(header.symbol, "EURUSD");
        assert_eq!(header.mark, "\u{20AC}");
        assert_eq!(header.name, "Euro / US Dollar");
        assert_eq!(header.price.as_deref(), Some("1.08423"));
        assert_eq!(header.spread_pips.as_deref(), Some("0.8"));
    }

    #[test]
    fn before_the_first_quote_there_is_no_price() {
        let mut state = blank_state();
        state.instruments.insert("EURUSD".to_owned(), eurusd());
        let header = symbol_header(&state, "EURUSD", None);
        assert_eq!(header.price, None, "no made-up number");
        assert_eq!(header.spread_pips, None);
        assert_eq!(header.name, "Euro / US Dollar");
    }

    #[test]
    fn an_unknown_forex_ticker_is_read_from_its_letters() {
        let header = symbol_header(&blank_state(), "gbpjpy", None);
        assert_eq!(header.mark, "\u{A3}");
        assert_eq!(header.name, "British Pound / Japanese Yen");
    }

    #[test]
    fn something_that_is_not_a_pair_is_named_by_its_ticker() {
        let header = symbol_header(&blank_state(), "XAUUSD.x", None);
        assert_eq!(header.name, "XAUUSD.x");
        assert_eq!(header.mark, "X");
        let header = symbol_header(&blank_state(), "", None);
        assert_eq!((header.mark.as_str(), header.name.as_str()), ("", ""));
    }

    #[test]
    fn the_brokers_description_and_class_win_once_the_list_is_loaded() {
        let mut listed = SymbolInfo::named("CANON");
        listed.description = Some("CANON INC".to_owned());
        listed.asset_class = Some("Asia/Pacific Shares".to_owned());
        listed.category = Some("Japan".to_owned());
        let header = symbol_header(&blank_state(), "CANON", Some(&listed));
        assert_eq!(header.name, "CANON INC");
        assert_eq!(header.icon.primary, crate::symbols::Mark::Flag("jp"));
        assert_eq!(header.icon.secondary, None);
    }

    #[test]
    fn a_pair_gets_two_flags_even_before_the_list_is_loaded() {
        let header = symbol_header(&with_quote(1.0, 1.0001), "EURUSD", None);
        assert_eq!(header.icon.primary, crate::symbols::Mark::Flag("eu"));
        assert_eq!(
            header.icon.secondary,
            Some(crate::symbols::Mark::Flag("us"))
        );
    }

    #[test]
    fn a_currency_without_a_name_is_shown_by_its_code() {
        assert_eq!(currency_name("thb"), "THB");
        assert_eq!(currency_mark("thb"), "T");
        assert_eq!(currency_name("usd"), "US Dollar");
    }
}
