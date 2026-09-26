//! Symbols and the reference catalogs around them: [`SymbolTable`] for looking one up by id or by
//! name, the wire types for the symbol list and its details, and the catalogs a symbol's ids point
//! into (assets, asset classes, symbol categories).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::transport::wire::flex;

/// A symbol in the list of an account (`ProtoOALightSymbol`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightSymbol {
    /// The symbol id.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// The ticker the broker uses.
    #[serde(default)]
    pub symbol_name: Option<String>,
    /// Whether the symbol is enabled for the account.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// The broker's description.
    #[serde(default)]
    pub description: Option<String>,
    /// The asset the symbol is bought in.
    #[serde(default, deserialize_with = "flex::opt")]
    pub base_asset_id: Option<i64>,
    /// The asset the symbol is priced in.
    #[serde(default, deserialize_with = "flex::opt")]
    pub quote_asset_id: Option<i64>,
    /// The symbol's category.
    #[serde(default, deserialize_with = "flex::opt")]
    pub symbol_category_id: Option<i64>,
}

/// The details of a symbol (`ProtoOASymbol`, the fields that matter to a data client).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    /// The symbol id.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// Decimals the symbol is quoted with.
    #[serde(deserialize_with = "flex::int")]
    pub digits: i64,
    /// Where the pip sits: a pip is 10 to the power of minus this.
    #[serde(deserialize_with = "flex::int")]
    pub pip_position: i64,
    /// Contract size, in the base asset's smallest unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub lot_size: Option<i64>,
    /// Smallest volume, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub min_volume: Option<i64>,
    /// Largest volume, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub max_volume: Option<i64>,
    /// Volume step, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub step_volume: Option<i64>,
    /// Time zone of the trading schedule.
    #[serde(default)]
    pub schedule_time_zone: Option<String>,
    /// When the symbol trades each week (`schedule`).
    #[serde(default)]
    pub schedule: Vec<super::hours::Interval>,
    /// Days the symbol does not trade (`holiday`).
    #[serde(default)]
    pub holiday: Vec<super::hours::Holiday>,
    /// Whether trading is allowed (`ProtoOATradingMode`: 0 enabled, 1 and 2 disabled, 3 close
    /// only).
    #[serde(default, deserialize_with = "flex::opt")]
    pub trading_mode: Option<i64>,
}

/// An asset: a currency or other unit an account or symbol is denominated in (`ProtoOAAsset`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// The asset id.
    #[serde(deserialize_with = "flex::int")]
    pub asset_id: i64,
    /// The short name (`EUR`).
    pub name: String,
    /// The display name.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Decimals of an amount of the asset.
    #[serde(default, deserialize_with = "flex::opt")]
    pub digits: Option<i64>,
}

/// A group of symbols by market (`ProtoOAAssetClass`): forex, indices, metals...
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetClass {
    /// The class id.
    #[serde(default, deserialize_with = "flex::opt")]
    pub id: Option<i64>,
    /// The class name.
    #[serde(default)]
    pub name: Option<String>,
    /// Where it sorts in the platform.
    #[serde(default)]
    pub sorting_number: Option<f64>,
}

/// A group of symbols inside an asset class (`ProtoOASymbolCategory`): major pairs, cryptos...
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolCategory {
    /// The category id (what `LightSymbol::symbol_category_id` points to).
    #[serde(deserialize_with = "flex::int")]
    pub id: i64,
    /// The asset class it belongs to.
    #[serde(deserialize_with = "flex::int")]
    pub asset_class_id: i64,
    /// The category name.
    pub name: String,
    /// Where it sorts in the platform.
    #[serde(default)]
    pub sorting_number: Option<f64>,
}

/// `ProtoOASymbolChangedEvent`: the broker changed one or more symbols (trading hours, volume
/// rules, ...). Ask `MarketClient::symbol_details` again for the ones named here.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolChangedEvent {
    /// The account the changed symbols belong to.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The symbols that changed.
    #[serde(default, deserialize_with = "flex::list")]
    pub symbol_id: Vec<i64>,
}

/// `ProtoOASymbolsListReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsListReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Whether archived symbols are wanted too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_archived_symbols: Option<bool>,
}

/// `ProtoOASymbolsListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsListRes {
    /// The symbols of the account.
    #[serde(default)]
    pub symbol: Vec<LightSymbol>,
}

/// `ProtoOASymbolByIdReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolByIdReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbols asked about.
    pub symbol_id: Vec<i64>,
}

/// `ProtoOASymbolByIdRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolByIdRes {
    /// The details asked for.
    #[serde(default)]
    pub symbol: Vec<Symbol>,
}

/// `ProtoOASymbolsForConversionReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsForConversionReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The asset converted from.
    pub first_asset_id: i64,
    /// The asset converted to.
    pub last_asset_id: i64,
}

/// `ProtoOASymbolsForConversionRes`: a chain of symbols to convert one asset into another when no
/// direct quote exists (for example EUR/USD, USD/JPY for a EUR/JPY conversion).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsForConversionRes {
    /// The chain, in order.
    #[serde(default)]
    pub symbol: Vec<LightSymbol>,
}

/// `ProtoOAAssetListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetListRes {
    /// The assets.
    #[serde(default)]
    pub asset: Vec<Asset>,
}

/// `ProtoOAAssetClassListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetClassListRes {
    /// The classes.
    #[serde(default)]
    pub asset_class: Vec<AssetClass>,
}

/// `ProtoOASymbolCategoryListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolCategoryListRes {
    /// The categories.
    #[serde(default)]
    pub symbol_category: Vec<SymbolCategory>,
}

/// A symbol list indexed by id and by name.
///
/// Names are matched without regard to case (`eurusd` finds `EURUSD`). A name that appears twice
/// keeps the first symbol.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    symbols: Vec<LightSymbol>,
    by_id: HashMap<i64, usize>,
    by_name: HashMap<String, usize>,
}

impl SymbolTable {
    /// Indexes `symbols`.
    #[must_use]
    pub fn new(symbols: impl IntoIterator<Item = LightSymbol>) -> Self {
        let mut table = Self::default();
        for symbol in symbols {
            let index = table.symbols.len();
            table.by_id.entry(symbol.symbol_id).or_insert(index);
            if let Some(name) = &symbol.symbol_name {
                table
                    .by_name
                    .entry(name.to_ascii_uppercase())
                    .or_insert(index);
            }
            table.symbols.push(symbol);
        }
        table
    }

    /// How many symbols there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// The symbol with this id.
    #[must_use]
    pub fn get(&self, symbol_id: i64) -> Option<&LightSymbol> {
        self.by_id.get(&symbol_id).map(|&i| &self.symbols[i])
    }

    /// The symbol with this name, in any case.
    ///
    /// ```
    /// use wyck_openapi_model::market::{LightSymbol, SymbolTable};
    ///
    /// let symbol: LightSymbol = serde_json::from_str(r#"{"symbolId": 1, "symbolName": "EURUSD"}"#).unwrap();
    /// let table = SymbolTable::new([symbol]);
    /// assert_eq!(table.id_of("eurusd"), Some(1));
    /// ```
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&LightSymbol> {
        self.by_name
            .get(&name.to_ascii_uppercase())
            .map(|&i| &self.symbols[i])
    }

    /// The id of the symbol with this name.
    #[must_use]
    pub fn id_of(&self, name: &str) -> Option<i64> {
        self.find(name).map(|s| s.symbol_id)
    }

    /// The name of the symbol with this id.
    #[must_use]
    pub fn name_of(&self, symbol_id: i64) -> Option<&str> {
        self.get(symbol_id).and_then(|s| s.symbol_name.as_deref())
    }

    /// The symbols whose name starts with `prefix` (any case), in list order.
    #[must_use]
    pub fn starting_with(&self, prefix: &str) -> Vec<&LightSymbol> {
        let prefix = prefix.to_ascii_uppercase();
        self.symbols
            .iter()
            .filter(|s| {
                s.symbol_name
                    .as_deref()
                    .is_some_and(|n| n.to_ascii_uppercase().starts_with(&prefix))
            })
            .collect()
    }

    /// Every symbol, in list order.
    pub fn iter(&self) -> impl Iterator<Item = &LightSymbol> {
        self.symbols.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn symbol(id: i64, name: &str) -> LightSymbol {
        LightSymbol {
            symbol_id: id,
            symbol_name: Some(name.to_owned()),
            enabled: Some(true),
            description: None,
            base_asset_id: None,
            quote_asset_id: None,
            symbol_category_id: None,
        }
    }

    #[test]
    fn symbols_are_found_by_id_and_by_name_in_any_case() {
        let table = SymbolTable::new([
            symbol(1, "EURUSD"),
            symbol(2, "XAUUSD"),
            symbol(3, "EURGBP"),
        ]);
        assert_eq!(table.len(), 3);
        assert_eq!(table.id_of("eurusd"), Some(1));
        assert_eq!(table.id_of("XauUsd"), Some(2));
        assert_eq!(table.name_of(3), Some("EURGBP"));
        assert_eq!(table.id_of("NOPE"), None);
        assert_eq!(table.name_of(99), None);
    }

    #[test]
    fn a_prefix_search_keeps_list_order() {
        let table = SymbolTable::new([
            symbol(1, "EURUSD"),
            symbol(2, "XAUUSD"),
            symbol(3, "EURGBP"),
        ]);
        let names: Vec<_> = table
            .starting_with("eur")
            .iter()
            .filter_map(|s| s.symbol_name.as_deref())
            .collect();
        assert_eq!(names, vec!["EURUSD", "EURGBP"]);
        assert!(table.starting_with("zzz").is_empty());
    }

    #[test]
    fn a_repeated_name_keeps_the_first_and_a_nameless_symbol_is_still_listed() {
        let mut nameless = symbol(9, "X");
        nameless.symbol_name = None;
        let table = SymbolTable::new([symbol(1, "EURUSD"), symbol(2, "eurusd"), nameless]);
        assert_eq!(table.id_of("EURUSD"), Some(1));
        assert_eq!(table.len(), 3);
        assert!(table.get(9).is_some());
        assert_eq!(table.name_of(9), None);
        assert!(SymbolTable::default().is_empty());
    }

    #[test]
    fn a_conversion_chain_and_a_symbol_changed_event_are_read() {
        let chain: SymbolsForConversionRes = serde_json::from_value(json!({"symbol": [
            {"symbolId": 1, "symbolName": "EURUSD"},
            {"symbolId": 2, "symbolName": "USDJPY"}
        ]}))
        .unwrap();
        assert_eq!(chain.symbol.len(), 2);
        let changed: SymbolChangedEvent =
            serde_json::from_value(json!({"ctidTraderAccountId": 1, "symbolId": ["2", 3]}))
                .unwrap();
        assert_eq!(changed.symbol_id, vec![2, 3]);
    }

    #[test]
    fn the_catalogs_are_read() {
        let assets: AssetListRes = serde_json::from_value(json!({"asset": [
            {"assetId": 1, "name": "EUR", "displayName": "Euro", "digits": 2},
            {"assetId": "2", "name": "USD"}
        ]}))
        .unwrap();
        assert_eq!(assets.asset[1].asset_id, 2);
        assert_eq!(assets.asset[0].display_name.as_deref(), Some("Euro"));

        let categories: SymbolCategoryListRes = serde_json::from_value(json!({"symbolCategory": [
            {"id": 4, "assetClassId": 1, "name": "Majors", "sortingNumber": 1.0}
        ]}))
        .unwrap();
        assert_eq!(categories.symbol_category[0].asset_class_id, 1);

        let classes: AssetClassListRes =
            serde_json::from_value(json!({"assetClass": [{"id": 1, "name": "Forex"}]})).unwrap();
        assert_eq!(classes.asset_class[0].name.as_deref(), Some("Forex"));
    }
}
