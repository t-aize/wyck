//! The list of symbols an account can trade, ready for a picker: classified, searchable and
//! ordered. No UI in it.
//!
//! cTrader files every symbol under a category, and every category under an asset class (Forex,
//! Indices, Metals...). [`Catalog::build`] follows those links. A symbol the broker leaves
//! uncategorized is classified only from what its ticker makes certain (a pair of known
//! currencies, a metal, a crypto), otherwise it is [`Class::Other`]; nothing is guessed beyond
//! that.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use wyck::openapi::market::{Asset, AssetClass, LightSymbol, SymbolCategory};

use super::marks::{self, Icon};

/// The kind of instrument, as the picker files it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    Forex,
    Metals,
    Energies,
    Indices,
    Crypto,
    Shares,
    Commodities,
    Bonds,
    Other,
}

impl Class {
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

    pub fn icon(self) -> IconName {
        match self {
            Self::Forex => IconName::ArrowLeftRight,
            Self::Metals => IconName::Gem,
            Self::Energies => IconName::Fuel,
            Self::Indices => IconName::ChartLine,
            Self::Crypto => IconName::Bitcoin,
            Self::Shares => IconName::Building2,
            Self::Commodities => IconName::Wheat,
            Self::Bonds => IconName::Landmark,
            Self::Other => IconName::CircleDot,
        }
    }

    /// The class a broker's own wording stands for (`Forex`, `Crypto Currencies`, `Energies`).
    fn from_broker(text: &str) -> Self {
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

/// The class of a ticker the broker did not categorize, when the ticker settles it.
fn class_from_ticker(name: &str) -> Class {
    let name = name.trim().to_ascii_uppercase();
    let is_pair = name.len() == 6 && name.chars().all(|c| c.is_ascii_alphabetic());
    let (base, quote) = if is_pair {
        (&name[..3], &name[3..])
    } else {
        (name.as_str(), "")
    };
    // The broker's own suffixes: `.cash` for a cash CFD (an index, or oil), `.c` for a
    // commodity future.
    let root = name.split('.').next().unwrap_or(&name);
    if METALS.contains(&base) {
        Class::Metals
    } else if CRYPTO.contains(&base) {
        Class::Crypto
    } else if is_pair && FIAT.contains(&base) && FIAT.contains(&quote) {
        Class::Forex
    } else if root.ends_with("OIL") || root.ends_with("GAS") {
        Class::Energies
    } else if name.ends_with(".CASH") {
        Class::Indices
    } else if name.ends_with(".C") {
        Class::Commodities
    } else {
        Class::Other
    }
}

/// One symbol of the account.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub class: Class,
    /// The asset the symbol is bought in, and the one it is priced in, when the broker says.
    pub base: Option<String>,
    pub quote: Option<String>,
    /// Its picture: flags, a logo or a glyph.
    pub icon: Icon,
    /// The broker's finer grouping inside the class (`Major Pairs`, `US Shares`...).
    pub category: Option<String>,
    /// Lower-cased name and description, what a search looks through.
    search: String,
}

#[derive(Debug, Default)]
pub struct Catalog {
    entries: Vec<Entry>,
}

impl Catalog {
    /// Builds the catalog from what the broker lists, keeping only the symbols enabled for the
    /// account, ordered by class then ticker.
    pub fn build(
        symbols: Vec<LightSymbol>,
        categories: Vec<SymbolCategory>,
        classes: Vec<AssetClass>,
        assets: Vec<Asset>,
    ) -> Self {
        let asset_names: HashMap<i64, &str> = assets
            .iter()
            .map(|a| (a.asset_id, a.name.as_str()))
            .collect();
        let class_names: HashMap<i64, &str> = classes
            .iter()
            .filter_map(|c| Some((c.id?, c.name.as_deref()?)))
            .collect();
        let category_names: HashMap<i64, String> =
            categories.iter().map(|c| (c.id, c.name.clone())).collect();
        let category_class: HashMap<i64, Class> = categories
            .iter()
            .filter_map(|c| {
                let name = class_names.get(&c.asset_class_id)?;
                Some((c.id, Class::from_broker(name)))
            })
            .collect();

        let mut entries: Vec<Entry> = symbols
            .into_iter()
            .filter(|s| s.enabled != Some(false))
            .filter_map(|s| {
                let name = s.symbol_name?;
                let description = s.description.unwrap_or_default();
                let class = s
                    .symbol_category_id
                    .and_then(|id| category_class.get(&id).copied())
                    .filter(|class| *class != Class::Other)
                    .unwrap_or_else(|| class_from_ticker(&name));
                let search = format!("{} {}", name.to_lowercase(), description.to_lowercase());
                let base = s
                    .base_asset_id
                    .and_then(|id| asset_names.get(&id))
                    .map(|name| (*name).to_owned());
                let quote = s
                    .quote_asset_id
                    .and_then(|id| asset_names.get(&id))
                    .map(|name| (*name).to_owned());
                let icon = marks::icon_for(&name, class, base.as_deref(), quote.as_deref());
                let category = s
                    .symbol_category_id
                    .and_then(|id| category_names.get(&id))
                    .filter(|name| {
                        let name = name.trim();
                        // Brokers file everything under a placeholder when they have no finer grouping.
                        !name.is_empty() && !name.to_ascii_lowercase().starts_with("default")
                    })
                    .cloned();
                Some(Entry {
                    id: s.symbol_id,
                    name,
                    description,
                    class,
                    base,
                    quote,
                    icon,
                    category,
                    search,
                })
            })
            .collect();
        entries.sort_by(|a, b| (a.class, &a.name).cmp(&(b.class, &b.name)));
        Self { entries }
    }

    pub fn total(&self) -> usize {
        self.entries.len()
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub fn by_name(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The classes that have at least one symbol, in display order.
    pub fn classes(&self) -> Vec<Class> {
        Class::ALL
            .into_iter()
            .filter(|class| self.entries.iter().any(|e| e.class == *class))
            .collect()
    }

    /// The entries matching `text` (every word must appear in the ticker or the description),
    /// best first, limited to `class` when one is given. With no text: everything in order.
    pub fn query(&self, text: &str, class: Option<Class>) -> Vec<usize> {
        let text = text.trim().to_lowercase();
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut hits: Vec<(u8, usize)> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| class.is_none_or(|c| e.class == c))
            .filter(|(_, e)| words.iter().all(|w| e.search.contains(w)))
            .map(|(index, e)| (rank(e, &text, &words), index))
            .collect();
        // Entries are already ordered by class then ticker, and the sort is stable.
        hits.sort_by_key(|(rank, _)| *rank);
        hits.into_iter().map(|(_, index)| index).collect()
    }
}

/// How well an entry matches: the lower, the better.
fn rank(entry: &Entry, text: &str, words: &[&str]) -> u8 {
    if words.is_empty() {
        return 0;
    }
    let name = entry.name.to_lowercase();
    if name == text {
        0
    } else if name.starts_with(text) {
        1
    } else if words.iter().all(|w| name.contains(w)) {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light(id: i64, name: &str, description: &str, category: Option<i64>) -> LightSymbol {
        LightSymbol {
            symbol_id: id,
            symbol_name: Some(name.into()),
            enabled: Some(true),
            description: Some(description.into()),
            base_asset_id: None,
            quote_asset_id: None,
            symbol_category_id: category,
        }
    }

    fn catalog() -> Catalog {
        Catalog::build(
            vec![
                LightSymbol {
                    base_asset_id: Some(1),
                    quote_asset_id: Some(2),
                    ..light(1, "EURUSD", "Euro vs US Dollar", None)
                },
                light(2, "US 30", "Dow Jones", Some(10)),
                light(3, "XAUUSD", "Gold vs US Dollar", None),
                light(4, "USDJPY", "US Dollar vs Japanese Yen", None),
                LightSymbol {
                    enabled: Some(false),
                    ..light(5, "HIDDEN", "off", None)
                },
            ],
            vec![SymbolCategory {
                id: 10,
                asset_class_id: 1,
                name: "US Indices".into(),
                sorting_number: None,
            }],
            vec![AssetClass {
                id: Some(1),
                name: Some("Indices".into()),
                sorting_number: None,
            }],
            vec![
                Asset {
                    asset_id: 1,
                    name: "EUR".into(),
                    display_name: None,
                    digits: None,
                },
                Asset {
                    asset_id: 2,
                    name: "USD".into(),
                    display_name: None,
                    digits: None,
                },
            ],
        )
    }

    #[test]
    fn classifies_from_the_brokers_categories_then_the_ticker() {
        let catalog = catalog();
        assert_eq!(catalog.by_name("US 30").unwrap().class, Class::Indices);
        assert_eq!(catalog.by_name("EURUSD").unwrap().class, Class::Forex);
        assert_eq!(catalog.by_name("XAUUSD").unwrap().class, Class::Metals);
    }

    #[test]
    fn the_brokers_ticker_suffixes_settle_the_class() {
        assert_eq!(class_from_ticker("US100.cash"), Class::Indices);
        assert_eq!(class_from_ticker("UKOIL.cash"), Class::Energies);
        assert_eq!(class_from_ticker("HEATOIL.c"), Class::Energies);
        assert_eq!(class_from_ticker("NATGAS.cash"), Class::Energies);
        assert_eq!(class_from_ticker("COFFEE.c"), Class::Commodities);
        assert_eq!(class_from_ticker("AAPL"), Class::Other);
    }

    #[test]
    fn names_the_assets_a_symbol_is_bought_and_priced_in() {
        let catalog = catalog();
        let eurusd = catalog.by_name("EURUSD").unwrap();
        assert_eq!(eurusd.base.as_deref(), Some("EUR"));
        assert_eq!(eurusd.quote.as_deref(), Some("USD"));
        assert_eq!(catalog.by_name("US 30").unwrap().base, None);
    }

    #[test]
    fn leaves_out_disabled_symbols() {
        assert!(catalog().by_name("HIDDEN").is_none());
    }

    #[test]
    fn every_word_must_match_and_the_ticker_ranks_first() {
        let catalog = catalog();
        let names = |q: &str| -> Vec<String> {
            catalog
                .query(q, None)
                .into_iter()
                .map(|i| catalog.entry(i).unwrap().name.clone())
                .collect()
        };
        // USDJPY has the query as a ticker prefix, EURUSD only in the description.
        assert_eq!(names("usd"), vec!["USDJPY", "EURUSD", "XAUUSD"]);
        assert_eq!(names("dollar yen"), vec!["USDJPY"]);
        assert!(names("nothing like this").is_empty());
    }

    #[test]
    fn a_class_filter_narrows_the_list() {
        let catalog = catalog();
        assert_eq!(catalog.query("", Some(Class::Indices)).len(), 1);
        assert_eq!(
            catalog.classes(),
            vec![Class::Forex, Class::Metals, Class::Indices]
        );
    }
}
