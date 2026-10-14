//! The request and response messages of the market calls.

use serde::{Deserialize, Serialize};
use wyck_market::{Asset, AssetClass, LightSymbol, Symbol, SymbolCategory, WireTick, WireTrendbar};

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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsForConversionRes {
    /// The chain, in order.
    #[serde(default)]
    pub symbol: Vec<LightSymbol>,
}

/// `ProtoOAAssetListRes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetListRes {
    #[serde(default)]
    pub asset: Vec<Asset>,
}

/// `ProtoOAAssetClassListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetClassListRes {
    #[serde(default)]
    pub asset_class: Vec<AssetClass>,
}

/// `ProtoOASymbolCategoryListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolCategoryListRes {
    #[serde(default)]
    pub symbol_category: Vec<SymbolCategory>,
}

/// `ProtoOASubscribeSpotsReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeSpotsReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbols to follow.
    pub symbol_id: Vec<i64>,
    /// Ask for the server time of each spot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe_to_spot_timestamp: Option<bool>,
}

/// `ProtoOASubscribeLiveTrendbarReq` and its unsubscribe twin.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTrendbarReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The bar period, as its number.
    pub period: i32,
    pub symbol_id: i64,
}

/// `ProtoOAGetTrendbarsReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTrendbarsReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
    /// The bar period, as its number.
    pub period: i32,
    pub symbol_id: i64,
    /// The most bars to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// `ProtoOAGetTrendbarsRes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTrendbarsRes {
    #[serde(default)]
    pub trendbar: Vec<WireTrendbar>,
    /// Whether more bars exist in the range than were returned.
    #[serde(default)]
    pub has_more: Option<bool>,
}

/// `ProtoOAGetTickDataReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTickDataReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    pub symbol_id: i64,
    /// Bid (1) or ask (2) ticks.
    pub r#type: i32,
    /// Start of the range, in Unix milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<i64>,
    /// End of the range, in Unix milliseconds. The range may span at most one week.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<i64>,
}

/// `ProtoOAGetTickDataRes`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTickDataRes {
    /// The ticks, newest first.
    #[serde(default)]
    pub tick_data: Vec<WireTick>,
    /// Whether more ticks exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_use_the_official_field_names_and_skip_missing_options() {
        let bars = GetTrendbarsReq {
            ctid_trader_account_id: 7,
            from_timestamp: Some(1),
            to_timestamp: None,
            period: 1,
            symbol_id: 3,
            count: None,
        };
        assert_eq!(
            serde_json::to_value(&bars).unwrap(),
            json!({"ctidTraderAccountId": 7, "fromTimestamp": 1, "period": 1, "symbolId": 3})
        );
        let ticks = GetTickDataReq {
            ctid_trader_account_id: 7,
            symbol_id: 3,
            r#type: 1,
            from_timestamp: Some(10),
            to_timestamp: Some(20),
        };
        assert_eq!(
            serde_json::to_value(&ticks).unwrap(),
            json!({"ctidTraderAccountId": 7, "symbolId": 3, "type": 1, "fromTimestamp": 10, "toTimestamp": 20})
        );
    }

    #[test]
    fn a_conversion_chain_is_read() {
        let chain: SymbolsForConversionRes = serde_json::from_value(json!({"symbol": [
            {"symbolId": 1, "symbolName": "EURUSD"},
            {"symbolId": 2, "symbolName": "USDJPY"}
        ]}))
        .unwrap();
        assert_eq!(chain.symbol.len(), 2);
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
