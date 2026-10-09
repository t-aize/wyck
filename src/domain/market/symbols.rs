//! Symbols and the reference catalogs around them: the wire types for the symbol list and its
//! details, and the catalogs a symbol's ids point into (assets, asset classes, symbol categories).

use serde::{Deserialize, Serialize};

use crate::domain::flex;

/// A symbol in the list of an account (`ProtoOALightSymbol`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct LightSymbol {
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
#[non_exhaustive]
pub struct Symbol {
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
#[non_exhaustive]
pub struct Asset {
    #[serde(deserialize_with = "flex::int")]
    pub asset_id: i64,
    /// The short name (`EUR`).
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// Decimals of an amount of the asset.
    #[serde(default, deserialize_with = "flex::opt")]
    pub digits: Option<i64>,
}

/// A group of symbols by market (`ProtoOAAssetClass`): forex, indices, metals...
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
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
#[non_exhaustive]
pub struct SymbolsForConversionRes {
    /// The chain, in order.
    #[serde(default)]
    pub symbol: Vec<LightSymbol>,
}

/// `ProtoOAAssetListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AssetListRes {
    #[serde(default)]
    pub asset: Vec<Asset>,
}

/// `ProtoOAAssetClassListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AssetClassListRes {
    #[serde(default)]
    pub asset_class: Vec<AssetClass>,
}

/// `ProtoOASymbolCategoryListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SymbolCategoryListRes {
    #[serde(default)]
    pub symbol_category: Vec<SymbolCategory>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
