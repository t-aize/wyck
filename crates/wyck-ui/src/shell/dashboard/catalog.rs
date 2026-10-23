//! The list of symbols an account can trade, ready for a picker: classified, searchable and
//! ordered. No UI in it.
//!
//! cTrader files every symbol under a category, and every category under an asset class (Forex,
//! Indices, Metals...). [`Catalog::build`] follows those links. A symbol the broker leaves
//! uncategorized is classified from known currency pairs and product names. Unknown tickers
//! remain [`Class::Other`].

use std::collections::HashMap;

use wyck_domain::market::{Asset, AssetClass, LightSymbol, SymbolCategory};

use super::marks::{self, Icon};

pub(crate) use super::class::Class;

const FIAT: [&str; 26] = [
    "EUR", "USD", "GBP", "JPY", "CHF", "AUD", "NZD", "CAD", "CNH", "CNY", "HKD", "SGD", "SEK",
    "NOK", "DKK", "PLN", "CZK", "HUF", "TRY", "ZAR", "MXN", "ILS", "THB", "INR", "KRW", "BRL",
];
const CRYPTO: [&str; 12] = [
    "BTC", "ETH", "LTC", "XRP", "BCH", "ADA", "DOT", "SOL", "DOGE", "BNB", "LINK", "XLM",
];

/// The class of a ticker the broker did not categorize, when the ticker settles it.
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the ticker is upper-cased above and these are the broker's own suffixes, not file extensions"
)]
fn class_from_ticker(name: &str) -> Class {
    let name = name.trim().to_ascii_uppercase();
    let is_pair = name.len() == 6 && name.chars().all(|c| c.is_ascii_alphabetic());
    let (base, quote) = if is_pair {
        name.split_at_checked(3).unwrap_or((name.as_str(), ""))
    } else {
        (name.as_str(), "")
    };
    // The broker's own suffixes: `.cash` for a cash CFD (an index, or oil), `.c` for a
    // commodity future.
    let root = name.split('.').next().unwrap_or(&name);
    if let Some(class) = marks::known_physical_class(&name) {
        class
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
pub(crate) struct Entry {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) class: Class,
    /// The asset the symbol is bought in, and the one it is priced in, when the broker says.
    pub(crate) base: Option<String>,
    pub(crate) quote: Option<String>,
    /// The id of the asset it is priced in.
    pub(crate) quote_asset: Option<i64>,
    /// Its picture: flags, a logo or a glyph.
    pub(crate) icon: Icon,
    /// The broker's finer grouping inside the class (`Major Pairs`, `US Shares`...).
    pub(crate) category: Option<String>,
    /// Lower-cased name and description, what a search looks through.
    search: String,
}

#[derive(Debug, Default)]
pub(crate) struct Catalog {
    entries: Vec<Entry>,
}

impl Catalog {
    /// Builds the catalog from what the broker lists, keeping only the symbols enabled for the
    /// account, ordered by class then ticker.
    pub(crate) fn build(
        symbols: Vec<LightSymbol>,
        categories: &[SymbolCategory],
        classes: &[AssetClass],
        assets: &[Asset],
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
                    quote_asset: s.quote_asset_id,
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

    pub(crate) fn total(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    pub(crate) fn by_id(&self, id: i64) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub(crate) fn by_name(&self, name: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The classes that have at least one symbol, in display order.
    pub(crate) fn classes(&self) -> Vec<Class> {
        Class::ALL
            .into_iter()
            .filter(|class| self.entries.iter().any(|e| e.class == *class))
            .collect()
    }

    /// The entries matching `text` (every word must appear in the ticker or the description),
    /// best first, limited to `class` when one is given. With no text: everything in order.
    pub(crate) fn query(&self, text: &str, class: Option<Class>) -> Vec<usize> {
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
    use serde_json::json;

    fn from_json<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).unwrap()
    }

    fn light(id: i64, name: &str, description: &str, category: Option<i64>) -> LightSymbol {
        from_json(json!({
            "symbolId": id,
            "symbolName": name,
            "enabled": true,
            "description": description,
            "symbolCategoryId": category,
        }))
    }

    fn catalog() -> Catalog {
        Catalog::build(
            vec![
                {
                    let mut eurusd = light(1, "EURUSD", "Euro vs US Dollar", None);
                    eurusd.base_asset_id = Some(1);
                    eurusd.quote_asset_id = Some(2);
                    eurusd
                },
                light(2, "US 30", "Dow Jones", Some(10)),
                light(3, "XAUUSD", "Gold vs US Dollar", None),
                light(4, "USDJPY", "US Dollar vs Japanese Yen", None),
                {
                    let mut hidden = light(5, "HIDDEN", "off", None);
                    hidden.enabled = Some(false);
                    hidden
                },
            ],
            &[from_json(
                json!({ "id": 10, "assetClassId": 1, "name": "US Indices" }),
            )],
            &[from_json(json!({ "id": 1, "name": "Indices" }))],
            &[
                from_json(json!({ "assetId": 1, "name": "EUR" })),
                from_json(json!({ "assetId": 2, "name": "USD" })),
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
        assert_eq!(class_from_ticker("BRENT.cash"), Class::Energies);
        assert_eq!(class_from_ticker("GASOLINE.c"), Class::Energies);
        assert_eq!(class_from_ticker("COFFEE.c"), Class::Commodities);
        assert_eq!(class_from_ticker("LUMBER.c"), Class::Commodities);
        assert_eq!(class_from_ticker("IRONORE.c"), Class::Metals);
        assert_eq!(class_from_ticker("XCUUSD"), Class::Metals);
        assert_eq!(class_from_ticker("CL"), Class::Other);
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
        assert_eq!(names("nothing like this"), Vec::<&str>::new());
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
