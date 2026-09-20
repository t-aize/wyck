//! The list of symbols an account can trade, ready for a picker: classified, iconified, searchable
//! and grouped, with a details sheet for one symbol. No UI toolkit in it.
//!
//! [`Catalog`] is built once from what the broker lists ([`SymbolInfo`]). It classifies every
//! entry into an [`AssetClass`], picks its icon ([`SymbolIcon`]: two round flags for a forex pair,
//! a flag for an index or a share when the country is known, an asset icon otherwise), and answers
//! searches the way a command palette does: every word of the query must match somewhere, and
//! the best matches come first ([`Catalog::query`]). [`describe`] turns one entry, plus the
//! instrument details and a quote when they have been loaded, into the rows of a details sheet.
//!
//! Nothing here invents data. A field the broker did not give is left out of the sheet, and the
//! asset class of a symbol whose broker does not say is inferred only from what the ticker and the
//! currencies make certain (a metal, a crypto, a pair of known currencies), else it is
//! [`AssetClass::Other`].

use std::collections::BTreeMap;

use wyck_engine::domain::{Instrument, Quote, SpecsSource, SymbolInfo};

use crate::presentation::{format_price, format_volume};

/// The country codes of the round flags that ship with the application (`assets/flags`).
pub const FLAG_CODES: [&str; 36] = [
    "at", "au", "be", "br", "ca", "ch", "cn", "cz", "de", "dk", "es", "eu", "fi", "fr", "gb", "hk",
    "hu", "ie", "il", "in", "it", "jp", "kr", "mx", "nl", "no", "nz", "pl", "pt", "ru", "se", "sg",
    "th", "tr", "us", "za",
];

/// The kind of instrument, as a picker files it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AssetClass {
    /// Currency pairs.
    Forex,
    /// Gold, silver and the like.
    Metals,
    /// Oil and gas.
    Energies,
    /// Stock indices.
    Indices,
    /// Cryptocurrencies.
    Crypto,
    /// Shares of a company.
    Shares,
    /// Agricultural and other commodities.
    Commodities,
    /// Bonds.
    Bonds,
    /// Anything the broker does not say and the ticker does not settle.
    Other,
}

impl AssetClass {
    /// Every class, in the order a picker lists them.
    pub const ALL: [Self; 9] = [
        Self::Forex,
        Self::Metals,
        Self::Energies,
        Self::Indices,
        Self::Crypto,
        Self::Shares,
        Self::Commodities,
        Self::Bonds,
        Self::Other,
    ];

    /// The name shown on a filter and a heading.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Forex => "Forex",
            Self::Metals => "Metals",
            Self::Energies => "Energies",
            Self::Indices => "Indices",
            Self::Crypto => "Crypto",
            Self::Shares => "Shares",
            Self::Commodities => "Commodities",
            Self::Bonds => "Bonds",
            Self::Other => "Other",
        }
    }

    /// The class a broker's own wording stands for (`US Shares`, `Cryptocurrency`, `Energies`).
    #[must_use]
    pub fn from_broker(text: &str) -> Self {
        let text = text.to_ascii_lowercase();
        let has = |needle: &str| text.contains(needle);
        if has("forex") || text == "fx" {
            Self::Forex
        } else if has("metal") {
            Self::Metals
        } else if has("energ") {
            Self::Energies
        } else if has("indic") || has("index") {
            Self::Indices
        } else if has("crypto") {
            Self::Crypto
        } else if has("share") || has("stock") || has("equit") {
            Self::Shares
        } else if has("commod") {
            Self::Commodities
        } else if has("bond") {
            Self::Bonds
        } else {
            Self::Other
        }
    }
}

const FIAT: [&str; 26] = [
    "EUR", "USD", "GBP", "JPY", "CHF", "AUD", "NZD", "CAD", "CNH", "CNY", "HKD", "SGD", "SEK",
    "NOK", "DKK", "PLN", "CZK", "HUF", "TRY", "ZAR", "MXN", "ILS", "THB", "INR", "KRW", "BRL",
];
const METALS: [&str; 4] = ["XAU", "XAG", "XPT", "XPD"];
const CRYPTO: [&str; 12] = [
    "BTC", "ETH", "LTC", "XRP", "BCH", "ADA", "DOT", "SOL", "DOGE", "BNB", "LINK", "XLM",
];

/// The class of an entry: the broker's word when it gave one, else what the ticker settles.
#[must_use]
pub fn classify(info: &SymbolInfo) -> AssetClass {
    if let Some(class) = info.asset_class.as_deref().filter(|c| !c.trim().is_empty()) {
        return AssetClass::from_broker(class);
    }
    let base = info.base_currency.as_deref().map(str::to_ascii_uppercase);
    let quote = info.quote_currency.as_deref().map(str::to_ascii_uppercase);
    let base = base.or_else(|| pair_of(&info.symbol).map(|(b, _)| b));
    let quote = quote.or_else(|| pair_of(&info.symbol).map(|(_, q)| q));
    match (base.as_deref(), quote.as_deref()) {
        (Some(b), _) if METALS.contains(&b) => AssetClass::Metals,
        (Some(b), _) if CRYPTO.contains(&b) => AssetClass::Crypto,
        (Some(b), Some(q)) if FIAT.contains(&b) && FIAT.contains(&q) => AssetClass::Forex,
        _ => AssetClass::Other,
    }
}

/// The two halves of a six-letter ticker such as `EURUSD`, upper-cased.
fn pair_of(symbol: &str) -> Option<(String, String)> {
    let symbol = symbol.trim();
    (symbol.len() == 6 && symbol.chars().all(|c| c.is_ascii_alphabetic())).then(|| {
        (
            symbol[..3].to_ascii_uppercase(),
            symbol[3..].to_ascii_uppercase(),
        )
    })
}

/// The country of a currency, as a flag code.
#[must_use]
pub fn currency_flag(code: &str) -> Option<&'static str> {
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

/// Phrases that name a country, longest first so `south africa` is not read as `africa`.
const COUNTRIES: [(&str, &str); 38] = [
    ("united kingdom", "gb"),
    ("great britain", "gb"),
    ("united states", "us"),
    ("south africa", "za"),
    ("south korea", "kr"),
    ("hong kong", "hk"),
    ("netherlands", "nl"),
    ("switzerland", "ch"),
    ("singapore", "sg"),
    ("australia", "au"),
    ("germany", "de"),
    ("portugal", "pt"),
    ("finland", "fi"),
    ("ireland", "ie"),
    ("belgium", "be"),
    ("denmark", "dk"),
    ("sweden", "se"),
    ("norway", "no"),
    ("austria", "at"),
    ("poland", "pl"),
    ("canada", "ca"),
    ("brazil", "br"),
    ("mexico", "mx"),
    ("israel", "il"),
    ("turkey", "tr"),
    ("europe", "eu"),
    ("france", "fr"),
    ("russia", "ru"),
    ("japan", "jp"),
    ("china", "cn"),
    ("italy", "it"),
    ("spain", "es"),
    ("india", "in"),
    ("korea", "kr"),
    ("usa", "us"),
    ("uk", "gb"),
    ("us", "us"),
    ("eu", "eu"),
];

/// The flag of the first country a text names as whole words (`HONG KONG 50`, `Japan`).
#[must_use]
pub fn country_flag(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    COUNTRIES.iter().find_map(|(phrase, code)| {
        let needle: Vec<&str> = phrase.split(' ').collect();
        words
            .windows(needle.len())
            .any(|w| w == needle.as_slice())
            .then_some(*code)
    })
}

/// One half of an icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// A round flag, by country code (one of [`FLAG_CODES`]).
    Flag(&'static str),
    /// The icon of an asset class.
    Class(AssetClass),
    /// Letters, for a currency that has no flag.
    Letters(String),
}

/// The icon of a symbol: one mark, or two overlapped for a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolIcon {
    /// The main mark (the base currency of a pair).
    pub primary: Mark,
    /// The second mark (the quote currency of a pair), when there is one.
    pub secondary: Option<Mark>,
}

fn currency_mark(code: &str) -> Mark {
    currency_flag(code).map_or_else(
        || {
            Mark::Letters(
                code.chars()
                    .take(3)
                    .collect::<String>()
                    .to_ascii_uppercase(),
            )
        },
        Mark::Flag,
    )
}

/// The icon of `info`, filed under `class`.
#[must_use]
pub fn icon_for(info: &SymbolInfo, class: AssetClass) -> SymbolIcon {
    let pair = pair_of(&info.symbol);
    let base = info
        .base_currency
        .clone()
        .or_else(|| pair.as_ref().map(|(b, _)| b.clone()));
    let quote = info.quote_currency.clone().or_else(|| pair.map(|(_, q)| q));
    let single = |mark| SymbolIcon {
        primary: mark,
        secondary: None,
    };
    match class {
        AssetClass::Forex => match (base, quote) {
            (Some(b), Some(q)) => SymbolIcon {
                primary: currency_mark(&b),
                secondary: Some(currency_mark(&q)),
            },
            _ => single(Mark::Class(class)),
        },
        AssetClass::Metals | AssetClass::Crypto => SymbolIcon {
            primary: Mark::Class(class),
            secondary: quote.as_deref().and_then(currency_flag).map(Mark::Flag),
        },
        AssetClass::Indices => single(
            country_flag(&info.symbol)
                .or_else(|| info.description.as_deref().and_then(country_flag))
                .map_or(Mark::Class(class), Mark::Flag),
        ),
        AssetClass::Shares => single(
            info.category
                .as_deref()
                .and_then(country_flag)
                .or_else(|| info.asset_class.as_deref().and_then(country_flag))
                .map_or(Mark::Class(class), Mark::Flag),
        ),
        _ => single(Mark::Class(class)),
    }
}

/// One symbol of the catalog with what a picker needs to draw and search it.
#[derive(Debug, Clone)]
pub struct Entry {
    /// What the broker lists.
    pub info: SymbolInfo,
    /// Its class.
    pub class: AssetClass,
    /// Its icon.
    pub icon: SymbolIcon,
    /// The ticker reduced to letters and digits, lower case.
    key: String,
    /// The words of the description and the category, lower case.
    words: Vec<String>,
    /// The first word of the description, lower case: `gold` for `Gold vs US Dollar`.
    lead: Option<String>,
}

/// A line of the picker's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A group heading: the class and how many symbols the group has in this list.
    Header(AssetClass, usize),
    /// A symbol, by index into the catalog.
    Symbol(usize),
}

/// Forex pairs listed first, in this order, before the rest by name.
const MAJORS: [&str; 10] = [
    "EURUSD", "GBPUSD", "USDJPY", "USDCHF", "AUDUSD", "USDCAD", "NZDUSD", "EURGBP", "EURJPY",
    "GBPJPY",
];

fn alnum_lower(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn words_of(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// The symbols an account can trade. See the module docs.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: Vec<Entry>,
}

impl Catalog {
    /// Builds the catalog, ordered by class, forex majors first, then by name.
    #[must_use]
    pub fn new(list: &[SymbolInfo]) -> Self {
        let mut entries: Vec<Entry> = list
            .iter()
            .map(|info| {
                let class = classify(info);
                let mut words: Vec<String> = info
                    .description
                    .iter()
                    .chain(info.category.iter())
                    .flat_map(|t| words_of(t))
                    .collect();
                words.sort();
                words.dedup();
                Entry {
                    icon: icon_for(info, class),
                    key: alnum_lower(&info.symbol),
                    lead: info.description.as_deref().and_then(|d| words_of(d).next()),
                    words,
                    class,
                    info: info.clone(),
                }
            })
            .collect();
        entries.sort_by(|a, b| {
            let major = |e: &Entry| {
                MAJORS
                    .iter()
                    .position(|m| m.eq_ignore_ascii_case(&e.info.symbol))
                    .unwrap_or(MAJORS.len())
            };
            (a.class, major(a), a.info.symbol.to_ascii_lowercase()).cmp(&(
                b.class,
                major(b),
                b.info.symbol.to_ascii_lowercase(),
            ))
        });
        Self { entries }
    }

    /// How many symbols there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the account offers no symbol at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry at `index`.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// The index of a symbol, ignoring case.
    #[must_use]
    pub fn find(&self, symbol: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.info.symbol.eq_ignore_ascii_case(symbol.trim()))
    }

    /// The classes present and how many symbols each has, in listing order.
    #[must_use]
    pub fn counts(&self) -> Vec<(AssetClass, usize)> {
        let mut counts: BTreeMap<AssetClass, usize> = BTreeMap::new();
        for entry in &self.entries {
            *counts.entry(entry.class).or_default() += 1;
        }
        counts.into_iter().collect()
    }

    /// The symbols that match `text` (every word of it must match somewhere), best first, limited
    /// to `class` when one is given. An empty text matches everything, in catalog order.
    #[must_use]
    pub fn query(&self, text: &str, class: Option<AssetClass>) -> Vec<usize> {
        let tokens: Vec<String> = words_of(text).collect();
        let joined = alnum_lower(text);
        let mut scored: Vec<(u32, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| class.is_none_or(|c| e.class == c))
            .filter_map(|(i, e)| score(e, &tokens, &joined).map(|s| (s, i)))
            .collect();
        if !tokens.is_empty() {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        }
        scored.into_iter().map(|(_, i)| i).collect()
    }

    /// Turns the result of [`query`](Self::query) into list lines, with a heading before each
    /// group when `grouped` (only sensible for an unsearched list, where the order is by class).
    #[must_use]
    pub fn rows(&self, matches: &[usize], grouped: bool) -> Vec<Row> {
        if !grouped {
            return matches.iter().map(|&i| Row::Symbol(i)).collect();
        }
        let mut rows = Vec::with_capacity(matches.len() + AssetClass::ALL.len());
        let mut current: Option<AssetClass> = None;
        for &index in matches {
            let class = self.entries[index].class;
            if current != Some(class) {
                let count = matches
                    .iter()
                    .filter(|&&i| self.entries[i].class == class)
                    .count();
                rows.push(Row::Header(class, count));
                current = Some(class);
            }
            rows.push(Row::Symbol(index));
        }
        rows
    }
}

/// How well `entry` matches the words of a search, or `None` when a word matches nowhere.
fn score(entry: &Entry, tokens: &[String], joined: &str) -> Option<u32> {
    if tokens.is_empty() {
        return Some(0);
    }
    let mut total = 0;
    for token in tokens {
        total += token_score(entry, token)?;
    }
    // The whole query typed as one ticker, with or without separators (`eur/usd`).
    if !joined.is_empty() && entry.key == joined {
        total += 1200;
    }
    Some(total)
}

fn token_score(entry: &Entry, token: &str) -> Option<u32> {
    let token = alnum_lower(token);
    if token.is_empty() {
        return Some(0);
    }
    let key = entry.key.as_str();
    let mut best = 0;
    if key == token {
        best = 1000;
    } else if key.starts_with(&token) {
        best = 800;
    } else if key.contains(&token) {
        best = 500;
    }
    let base = entry.info.base_currency.as_deref();
    let quote = entry.info.quote_currency.as_deref();
    for currency in [base, quote].into_iter().flatten() {
        if currency.to_ascii_lowercase().starts_with(&token) {
            best = best.max(200);
        }
    }
    // `gold` is what `Gold vs US Dollar` is about, more than what `GOLDMAN SACHS` is called.
    if entry.lead.as_deref() == Some(token.as_str()) {
        best = best.max(900);
    }
    for word in &entry.words {
        if *word == token {
            best = best.max(450);
        } else if word.starts_with(&token) {
            best = best.max(400);
        } else if word.contains(&token) {
            best = best.max(250);
        }
    }
    if entry.class.label().to_ascii_lowercase().starts_with(&token) {
        best = best.max(150);
    }
    if best == 0 && token.len() >= 3 && is_subsequence(&token, key) {
        best = 60;
    }
    (best > 0).then_some(best)
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut hay = haystack.chars();
    needle.chars().all(|c| hay.any(|h| h == c))
}

/// One line of a details sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    /// What it is.
    pub label: &'static str,
    /// Its value.
    pub value: String,
}

fn detail(label: &'static str, value: impl Into<String>) -> Detail {
    Detail {
        label,
        value: value.into(),
    }
}

/// The details sheet of one symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetailsView {
    /// The ticker.
    pub symbol: String,
    /// The long name, when the broker gives one.
    pub description: Option<String>,
    /// The class label.
    pub class: &'static str,
    /// The finer category, when there is one.
    pub category: Option<String>,
    /// Whether the broker allows trading it right now.
    pub enabled: bool,
    /// What the broker says about the instrument. Empty until its details are loaded.
    pub rows: Vec<Detail>,
    /// The current bid, ask and spread. Empty until a quote is loaded.
    pub live: Vec<Detail>,
}

/// Builds the details sheet of `entry`, with the instrument's details and a quote when known.
#[must_use]
pub fn describe(
    entry: &Entry,
    instrument: Option<&Instrument>,
    quote: Option<&Quote>,
) -> DetailsView {
    let info = &entry.info;
    let mut rows = vec![detail("Asset class", entry.class.label())];
    if let Some(category) = &info.category {
        rows.push(detail("Category", category.clone()));
    }
    let base = instrument
        .and_then(|i| i.base_currency.as_ref())
        .or(info.base_currency.as_ref());
    let quote_currency = instrument
        .and_then(|i| i.quote_currency.as_ref())
        .or(info.quote_currency.as_ref());
    if let (Some(base), Some(quote_currency)) = (base, quote_currency) {
        rows.push(detail("Pair", format!("{base} / {quote_currency}")));
    }
    let mut live = Vec::new();
    if let Some(instrument) = instrument {
        rows.push(detail("Price digits", instrument.price_digits.to_string()));
        rows.push(detail("Pip size", pip_text(instrument.pip_size)));
        let specs = &instrument.volume;
        rows.push(detail("Lot size", units_text(specs.lot_size)));
        let lot = Some(specs.lot_size);
        rows.push(detail("Minimum volume", volume_text(specs.min, lot)));
        rows.push(detail("Volume step", volume_text(specs.step, lot)));
        rows.push(detail(
            "Maximum volume",
            specs
                .max
                .map_or_else(|| "No limit".to_owned(), |max| volume_text(max, lot)),
        ));
        rows.push(detail(
            "Volume rules",
            match instrument.specs_source {
                SpecsSource::Broker => "From the broker",
                _ => "Assumed by Wyck",
            },
        ));
        if let Some(quote) = quote {
            let digits = instrument.price_digits;
            live.push(detail("Bid", format_price(quote.bid, digits)));
            live.push(detail("Ask", format_price(quote.ask, digits)));
            if instrument.pip_size > 0.0 {
                live.push(detail(
                    "Spread",
                    format!(
                        "{:.1} pips",
                        instrument.distance_to_pips(quote.ask - quote.bid)
                    ),
                ));
            }
        }
    }
    DetailsView {
        symbol: info.symbol.clone(),
        description: info.description.clone(),
        class: entry.class.label(),
        category: info.category.clone(),
        enabled: instrument.map_or(info.enabled, |i| i.enabled),
        rows,
        live,
    }
}

/// A volume as lots and units, or as units alone when a lot is one unit (shares): `100 lot` would
/// read as a lot of a hundred.
fn volume_text(volume: wyck_engine::domain::Volume, lot: Option<f64>) -> String {
    match lot {
        Some(size) if size > 1.0 => format_volume(volume, lot),
        _ => format!("{} units", units_text(volume.as_units())),
    }
}

/// A pip size without trailing zeros: `0.0001`, `0.01`, `1`.
fn pip_text(pip: f64) -> String {
    let text = format!("{pip:.8}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A number of units with thousands separated: `100,000`.
fn units_text(units: f64) -> String {
    let whole = format!("{units:.0}");
    let mut out = String::new();
    for (i, c) in whole.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 && c != '-' {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use wyck_engine::domain::{Volume, VolumeSpecs};

    use super::*;

    fn info(symbol: &str, description: &str, class: &str, category: &str) -> SymbolInfo {
        let mut info = SymbolInfo::named(symbol);
        info.description = (!description.is_empty()).then(|| description.to_owned());
        info.asset_class = (!class.is_empty()).then(|| class.to_owned());
        info.category = (!category.is_empty()).then(|| category.to_owned());
        info
    }

    fn sample() -> Catalog {
        Catalog::new(&[
            info("CANON", "CANON INC", "Asia/Pacific Shares", "Japan"),
            info("XAUUSD", "Gold vs US Dollar", "Metals", ""),
            info("USDJPY", "US Dollar vs Japanese Yen", "Forex", ""),
            info(
                "AUDCAD",
                "Australian Dollar vs Canadian Dollar",
                "Forex",
                "",
            ),
            info("EURUSD", "Euro vs US Dollar", "Forex", ""),
            info("GBPUSD", "Great Britain Pound vs US Dollar", "Forex", ""),
            info("BTCUSD", "Bitcoin vs USD", "Cryptocurrency", ""),
            info("HONG KONG 50", "HONG KONG 50", "Indices", ""),
            info("XBRUSD", "Brent Crude Oil", "Energies", ""),
            info("APPLE", "APPLE INC", "US Shares", ""),
        ])
    }

    fn names(catalog: &Catalog, hits: &[usize]) -> Vec<String> {
        hits.iter()
            .map(|&i| catalog.entry(i).unwrap().info.symbol.clone())
            .collect()
    }

    #[test]
    fn the_brokers_words_map_to_classes() {
        for (text, class) in [
            ("Forex", AssetClass::Forex),
            ("Cryptocurrency", AssetClass::Crypto),
            ("US Shares", AssetClass::Shares),
            ("Asia/Pacific Shares", AssetClass::Shares),
            ("Energies", AssetClass::Energies),
            ("Indices", AssetClass::Indices),
            ("Metals", AssetClass::Metals),
            ("Something else", AssetClass::Other),
        ] {
            assert_eq!(AssetClass::from_broker(text), class, "{text}");
        }
    }

    #[test]
    fn without_the_brokers_word_only_certain_things_are_classified() {
        let mut gold = SymbolInfo::named("XAUUSD");
        assert_eq!(classify(&gold), AssetClass::Metals);
        gold.symbol = "BTCEUR".to_owned();
        assert_eq!(classify(&gold), AssetClass::Crypto);
        assert_eq!(classify(&SymbolInfo::named("EURUSD")), AssetClass::Forex);
        assert_eq!(classify(&SymbolInfo::named("TOYOTA")), AssetClass::Other);
        assert_eq!(classify(&SymbolInfo::named("US30")), AssetClass::Other);
    }

    #[test]
    fn the_catalog_lists_by_class_with_the_majors_first() {
        let catalog = sample();
        let all = catalog.query("", None);
        let listed = names(&catalog, &all);
        assert_eq!(&listed[..4], ["EURUSD", "GBPUSD", "USDJPY", "AUDCAD"]);
        let position = |name: &str| listed.iter().position(|n| n == name).unwrap();
        assert!(
            position("XAUUSD") > position("AUDCAD"),
            "metals after forex"
        );
        assert!(
            position("XBRUSD") > position("XAUUSD"),
            "energies after metals"
        );
        assert!(
            position("CANON") > position("BTCUSD"),
            "shares after crypto"
        );
    }

    #[test]
    fn a_search_finds_by_ticker_description_and_category() {
        let catalog = sample();
        assert_eq!(names(&catalog, &catalog.query("eurusd", None)), ["EURUSD"]);
        assert_eq!(names(&catalog, &catalog.query("gold", None))[0], "XAUUSD");
        assert_eq!(names(&catalog, &catalog.query("brent", None)), ["XBRUSD"]);
        assert_eq!(
            names(&catalog, &catalog.query("japan", None)),
            ["CANON", "USDJPY"]
        );
        assert_eq!(
            names(&catalog, &catalog.query("hong", None)),
            ["HONG KONG 50"]
        );
    }

    #[test]
    fn every_word_must_match_and_separators_do_not_matter() {
        let catalog = sample();
        assert_eq!(names(&catalog, &catalog.query("eur/usd", None)), ["EURUSD"]);
        assert_eq!(names(&catalog, &catalog.query("eur usd", None)), ["EURUSD"]);
        assert_eq!(
            names(&catalog, &catalog.query("bitcoin eur", None)),
            Vec::<String>::new()
        );
        assert!(catalog.query("zzzz", None).is_empty());
    }

    #[test]
    fn a_ticker_match_beats_a_description_match() {
        let catalog = Catalog::new(&[
            info("GOLDMAN", "GOLDMAN SACHS", "US Shares", ""),
            info("XAUUSD", "Gold vs US Dollar", "Metals", ""),
            info("GOLD", "Gold Futures", "Commodities", ""),
        ]);
        let hits = names(&catalog, &catalog.query("gold", None));
        assert_eq!(
            hits,
            ["GOLD", "XAUUSD", "GOLDMAN"],
            "the ticker, then what the name is about, then a longer ticker that starts with it"
        );
    }

    #[test]
    fn a_class_filter_narrows_the_list_and_counts_follow_the_data() {
        let catalog = sample();
        let forex = catalog.query("usd", Some(AssetClass::Forex));
        assert_eq!(
            names(&catalog, &forex),
            ["USDJPY", "EURUSD", "GBPUSD"],
            "a ticker that starts with the word comes before one that contains it"
        );
        let counts = catalog.counts();
        assert_eq!(counts[0], (AssetClass::Forex, 4));
        assert!(
            counts.iter().all(|(_, n)| *n > 0),
            "only classes that exist"
        );
        assert_eq!(counts.iter().map(|(_, n)| n).sum::<usize>(), catalog.len());
    }

    #[test]
    fn an_unsearched_list_gets_a_heading_per_class() {
        let catalog = sample();
        let all = catalog.query("", None);
        let rows = catalog.rows(&all, true);
        assert_eq!(rows[0], Row::Header(AssetClass::Forex, 4));
        let headings = rows.iter().filter(|r| matches!(r, Row::Header(..))).count();
        assert_eq!(headings, catalog.counts().len());
        assert_eq!(rows.len(), all.len() + headings);
        let plain = catalog.rows(&all, false);
        assert!(plain.iter().all(|r| matches!(r, Row::Symbol(_))));
    }

    #[test]
    fn a_pair_is_two_flags_and_a_metal_is_an_icon_and_a_flag() {
        let catalog = sample();
        let icon = |name: &str| {
            catalog
                .entry(catalog.find(name).unwrap())
                .unwrap()
                .icon
                .clone()
        };
        assert_eq!(
            icon("EURUSD"),
            SymbolIcon {
                primary: Mark::Flag("eu"),
                secondary: Some(Mark::Flag("us"))
            }
        );
        assert_eq!(
            icon("XAUUSD"),
            SymbolIcon {
                primary: Mark::Class(AssetClass::Metals),
                secondary: Some(Mark::Flag("us"))
            }
        );
        assert_eq!(icon("HONG KONG 50").primary, Mark::Flag("hk"));
        assert_eq!(icon("CANON").primary, Mark::Flag("jp"));
        assert_eq!(icon("APPLE").primary, Mark::Flag("us"), "from `US Shares`");
        assert_eq!(icon("XBRUSD").primary, Mark::Class(AssetClass::Energies));
    }

    #[test]
    fn a_currency_without_a_flag_shows_its_letters() {
        let mut thb = SymbolInfo::named("EURXYZ");
        thb.asset_class = Some("Forex".to_owned());
        let icon = icon_for(&thb, AssetClass::Forex);
        assert_eq!(icon.primary, Mark::Flag("eu"));
        assert_eq!(icon.secondary, Some(Mark::Letters("XYZ".to_owned())));
    }

    #[test]
    fn countries_are_read_as_whole_words() {
        assert_eq!(country_flag("US 500"), Some("us"));
        assert_eq!(country_flag("AUSTRALIA 200"), Some("au"));
        assert_eq!(country_flag("SOUTH AFRICA 40"), Some("za"));
        assert_eq!(country_flag("Hong Kong"), Some("hk"));
        assert_eq!(
            country_flag("BUSINESS"),
            None,
            "`us` inside a word is not the US"
        );
        assert_eq!(country_flag(""), None);
    }

    #[test]
    fn every_flag_the_module_can_name_is_shipped() {
        let mut named: Vec<&str> = COUNTRIES.iter().map(|(_, code)| *code).collect();
        named.extend(FIAT.iter().filter_map(|c| currency_flag(c)));
        named.push(currency_flag("RUB").unwrap());
        for code in named {
            assert!(FLAG_CODES.contains(&code), "no flag file for `{code}`");
        }
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

    #[test]
    fn a_sheet_without_details_only_has_what_the_list_knows() {
        let catalog = sample();
        let entry = catalog.entry(catalog.find("EURUSD").unwrap()).unwrap();
        let sheet = describe(entry, None, None);
        assert_eq!(sheet.symbol, "EURUSD");
        assert_eq!(sheet.description.as_deref(), Some("Euro vs US Dollar"));
        assert_eq!(sheet.rows, [detail("Asset class", "Forex")]);
        assert!(sheet.live.is_empty());
    }

    #[test]
    fn a_sheet_with_details_and_a_quote_is_complete() {
        let catalog = sample();
        let entry = catalog.entry(catalog.find("EURUSD").unwrap()).unwrap();
        let quote = Quote {
            symbol: "EURUSD".to_owned(),
            bid: 1.08423,
            ask: 1.08431,
            timestamp: None,
        };
        let sheet = describe(entry, Some(&eurusd()), Some(&quote));
        let value = |label: &str| {
            sheet
                .rows
                .iter()
                .chain(&sheet.live)
                .find(|d| d.label == label)
                .map(|d| d.value.clone())
        };
        assert_eq!(value("Pair").as_deref(), Some("EUR / USD"));
        assert_eq!(value("Price digits").as_deref(), Some("5"));
        assert_eq!(value("Pip size").as_deref(), Some("0.0001"));
        assert_eq!(value("Lot size").as_deref(), Some("100,000"));
        assert_eq!(value("Maximum volume").as_deref(), Some("No limit"));
        assert_eq!(value("Volume rules").as_deref(), Some("From the broker"));
        assert_eq!(value("Bid").as_deref(), Some("1.08423"));
        assert_eq!(value("Ask").as_deref(), Some("1.08431"));
        assert_eq!(value("Spread").as_deref(), Some("0.8 pips"));
        assert!(sheet.enabled);
    }

    #[test]
    fn a_share_is_measured_in_units_not_in_lots_of_one() {
        let catalog = sample();
        let entry = catalog.entry(catalog.find("APPLE").unwrap()).unwrap();
        let mut share = eurusd();
        share.symbol = "APPLE".to_owned();
        share.volume.lot_size = 1.0;
        share.volume.min = Volume::from_units(100);
        share.volume.step = Volume::from_units(100);
        let sheet = describe(entry, Some(&share), None);
        let value = |label: &str| {
            sheet
                .rows
                .iter()
                .find(|d| d.label == label)
                .map(|d| d.value.clone())
        };
        assert_eq!(value("Minimum volume").as_deref(), Some("100 units"));
        assert_eq!(value("Volume step").as_deref(), Some("100 units"));
    }

    #[test]
    fn a_name_that_is_about_gold_beats_a_ticker_that_merely_starts_like_it() {
        let catalog = sample();
        let hits = names(&catalog, &catalog.query("gold", None));
        assert_eq!(hits[0], "XAUUSD");
    }

    #[test]
    fn assumed_volume_rules_say_so() {
        let catalog = sample();
        let entry = catalog.entry(catalog.find("EURUSD").unwrap()).unwrap();
        let mut assumed = eurusd();
        assumed.specs_source = SpecsSource::Assumed;
        let sheet = describe(entry, Some(&assumed), None);
        assert!(
            sheet
                .rows
                .iter()
                .any(|d| d.label == "Volume rules" && d.value == "Assumed by Wyck")
        );
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(pip_text(0.0001), "0.0001");
        assert_eq!(pip_text(0.01), "0.01");
        assert_eq!(pip_text(1.0), "1");
        assert_eq!(units_text(100_000.0), "100,000");
        assert_eq!(units_text(1_000_000.0), "1,000,000");
        assert_eq!(units_text(1.0), "1");
    }
}
