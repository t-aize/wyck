//! Symbols and the reference catalogs around them: a symbol of the list, its details, and the
//! catalogs its ids point into (assets, asset classes, symbol categories).

use serde::Deserialize;
use wyck_core::flex;

use crate::hours::{Holiday, Interval};

/// A symbol in the list of an account (`ProtoOALightSymbol`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    pub schedule: Vec<Interval>,
    /// Days the symbol does not trade (`holiday`).
    #[serde(default)]
    pub holiday: Vec<Holiday>,
    /// Whether trading is allowed (`ProtoOATradingMode`: 0 enabled, 1 and 2 disabled, 3 close
    /// only).
    #[serde(default, deserialize_with = "flex::opt")]
    pub trading_mode: Option<i64>,
}

/// An asset: a currency or other unit an account or symbol is denominated in (`ProtoOAAsset`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolChangedEvent {
    /// The account the changed symbols belong to.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The symbols that changed.
    #[serde(default, deserialize_with = "flex::list")]
    pub symbol_id: Vec<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_symbol_changed_event_is_read() {
        let changed: SymbolChangedEvent =
            serde_json::from_value(json!({"ctidTraderAccountId": 1, "symbolId": ["2", 3]}))
                .unwrap();
        assert_eq!(changed.symbol_id, vec![2, 3]);
    }
}
