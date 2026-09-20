//! The messages, as plain data.
//!
//! One struct per message the client sends or reads, with the names and meaning of the official
//! `.proto` files (`OpenApiMessages.proto`, `OpenApiModelMessages.proto`), in camelCase as JSON
//! wants. Only the fields the client uses are kept; the server may send more and they are ignored.
//!
//! Numbers in JSON may be written as numbers or as strings (a 64 bit id or a price is often sent as
//! text by protobuf's JSON mapping), so every integer here accepts both (see [`flex`]).
//! Enumerations are sent as their numbers.
//!
//! Prices and volumes are the server's raw integers. [`crate::types`] turns bars and ticks into
//! friendlier values.
//!
//! The requests that carry a secret do not implement `Debug`, so a stray `{:?}` cannot log one.

use serde::{Deserialize, Serialize};

/// Reading integers that may arrive as a number or as text.
pub mod flex {
    use serde::Deserialize;
    use serde::de::{Deserializer, Error};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Num {
        Int(i64),
        Uint(u64),
        Float(f64),
        Text(String),
    }

    fn to_i64<E: Error>(value: Num) -> Result<i64, E> {
        match value {
            Num::Int(v) => Ok(v),
            Num::Uint(v) => i64::try_from(v).map_err(|_| E::custom("number out of range")),
            Num::Float(v) if v.fract() == 0.0 && v.abs() < 9.0e15 => Ok(v as i64),
            Num::Float(_) => Err(E::custom("not a whole number")),
            Num::Text(text) => text
                .trim()
                .parse::<i64>()
                .map_err(|_| E::custom("not a whole number")),
        }
    }

    /// A required integer.
    ///
    /// # Errors
    ///
    /// When the value is neither a whole number nor text holding one.
    pub fn int<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        to_i64(Num::deserialize(d)?)
    }

    /// An optional integer (absent or null gives `None`).
    ///
    /// # Errors
    ///
    /// When the value is present but is not a whole number.
    pub fn opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        Option::<Num>::deserialize(d)?.map(to_i64).transpose()
    }

    /// A list of integers.
    ///
    /// # Errors
    ///
    /// When an element is not a whole number.
    pub fn list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
        Vec::<Num>::deserialize(d)?
            .into_iter()
            .map(to_i64)
            .collect()
    }
}

// ---- requests ----

/// `ProtoOAApplicationAuthReq`: identifies the application.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationAuthReq {
    /// The client id.
    pub client_id: String,
    /// The client secret.
    pub client_secret: String,
}

/// `ProtoOAAccountAuthReq`: authorizes one trading account on the connection.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAuthReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// A valid access token that covers the account.
    pub access_token: String,
}

/// `ProtoOAGetAccountListByAccessTokenReq`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountsByAccessTokenReq {
    /// The access token.
    pub access_token: String,
}

/// `ProtoOARefreshTokenReq`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenReq {
    /// The refresh token.
    pub refresh_token: String,
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

/// `ProtoOASymbolByIdReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolByIdReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbols asked about.
    pub symbol_id: Vec<i64>,
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

/// `ProtoOAUnsubscribeSpotsReq`, and the request of the depth subscription calls: an account and
/// a list of symbols.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbols.
    pub symbol_id: Vec<i64>,
}

/// `ProtoOASubscribeLiveTrendbarReq` and its unsubscribe twin.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTrendbarReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The bar period, as its number (see [`crate::types::Period`]).
    pub period: i32,
    /// The symbol.
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
    /// The symbol.
    pub symbol_id: i64,
    /// The most bars to return.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// `ProtoOAGetTickDataReq`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTickDataReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
    /// The symbol.
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

// ---- responses ----

/// `ProtoOAAccountAuthRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAuthRes {
    /// The account that was authorized.
    #[serde(deserialize_with = "flex::int")]
    pub ctid_trader_account_id: i64,
}

/// `ProtoOAVersionRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRes {
    /// The proxy's version.
    pub version: String,
}

/// One trading account of a cTrader ID (`ProtoOACtidTraderAccount`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraderAccount {
    /// The id to pass as `ctidTraderAccountId`.
    #[serde(deserialize_with = "flex::int")]
    pub ctid_trader_account_id: i64,
    /// Live (true) or demo (false).
    #[serde(default)]
    pub is_live: Option<bool>,
    /// The login number shown in the platform. For display only.
    #[serde(default, deserialize_with = "flex::opt")]
    pub trader_login: Option<i64>,
    /// The broker's short name.
    #[serde(default)]
    pub broker_title_short: Option<String>,
}

/// `ProtoOAGetAccountListByAccessTokenRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsRes {
    /// The permission of the token: 0 view, 1 trade.
    #[serde(default, deserialize_with = "flex::opt")]
    pub permission_scope: Option<i64>,
    /// The accounts the token covers.
    #[serde(default)]
    pub ctid_trader_account: Vec<TraderAccount>,
}

/// `ProtoOARefreshTokenRes`.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenRes {
    /// The new access token.
    pub access_token: String,
    /// The token type (`bearer`).
    #[serde(default)]
    pub token_type: Option<String>,
    /// Seconds the access token stays valid.
    #[serde(default, deserialize_with = "flex::opt")]
    pub expires_in: Option<i64>,
    /// The new refresh token; the old one no longer works.
    pub refresh_token: String,
}

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

/// `ProtoOASymbolsListRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolsListRes {
    /// The symbols of the account.
    #[serde(default)]
    pub symbol: Vec<LightSymbol>,
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
}

/// `ProtoOASymbolByIdRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolByIdRes {
    /// The details asked for.
    #[serde(default)]
    pub symbol: Vec<Symbol>,
}

/// A bar as the server sends it (`ProtoOATrendbar`): the low is the base, the rest are offsets
/// from it. See [`crate::types::Bar`] for the readable form.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTrendbar {
    /// Volume in ticks: how many ticks the bar saw.
    #[serde(deserialize_with = "flex::int")]
    pub volume: i64,
    /// The bar period, as its number.
    #[serde(default, deserialize_with = "flex::opt")]
    pub period: Option<i64>,
    /// The low price.
    #[serde(default, deserialize_with = "flex::opt")]
    pub low: Option<i64>,
    /// `open - low`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub delta_open: Option<i64>,
    /// `close - low`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub delta_close: Option<i64>,
    /// `high - low`.
    #[serde(default, deserialize_with = "flex::opt")]
    pub delta_high: Option<i64>,
    /// The open time of the bar, in Unix minutes.
    #[serde(default, deserialize_with = "flex::opt")]
    pub utc_timestamp_in_minutes: Option<i64>,
}

/// `ProtoOAGetTrendbarsRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTrendbarsRes {
    /// The bars.
    #[serde(default)]
    pub trendbar: Vec<WireTrendbar>,
    /// Whether more bars exist in the range than were returned.
    #[serde(default)]
    pub has_more: Option<bool>,
}

/// A tick as the server sends it (`ProtoOATickData`). Read the list with
/// [`crate::types::decode_ticks`]: the times are not absolute.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTick {
    /// For the first tick a Unix time in milliseconds, for the others a difference in
    /// milliseconds from the tick before.
    #[serde(deserialize_with = "flex::int")]
    pub timestamp: i64,
    /// The tick price.
    #[serde(deserialize_with = "flex::int")]
    pub tick: i64,
}

/// `ProtoOAGetTickDataRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTickDataRes {
    /// The ticks, newest first.
    #[serde(default)]
    pub tick_data: Vec<WireTick>,
    /// Whether more ticks exist in the range than were returned.
    #[serde(default)]
    pub has_more: bool,
}

/// `ProtoOASpotEvent`: a new price, or the first one after a subscription.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotEvent {
    /// The trading account id.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The symbol.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// The new bid, when it changed.
    #[serde(default, deserialize_with = "flex::opt")]
    pub bid: Option<i64>,
    /// The new ask, when it changed.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ask: Option<i64>,
    /// Updated live bars, when a live bar subscription exists.
    #[serde(default)]
    pub trendbar: Vec<WireTrendbar>,
    /// The close price of the last session.
    #[serde(default, deserialize_with = "flex::opt")]
    pub session_close: Option<i64>,
    /// When the server made the event, in Unix milliseconds, if it was asked to say.
    #[serde(default, deserialize_with = "flex::opt")]
    pub timestamp: Option<i64>,
}

/// One order book entry (`ProtoOADepthQuote`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthQuote {
    /// The entry id.
    #[serde(default, deserialize_with = "flex::opt")]
    pub id: Option<i64>,
    /// The size, in hundredths of a unit.
    #[serde(default, deserialize_with = "flex::opt")]
    pub size: Option<i64>,
    /// The price, for a bid entry.
    #[serde(default, deserialize_with = "flex::opt")]
    pub bid: Option<i64>,
    /// The price, for an ask entry.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ask: Option<i64>,
}

/// `ProtoOADepthEvent`: changes of the order book.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthEvent {
    /// The symbol.
    #[serde(deserialize_with = "flex::int")]
    pub symbol_id: i64,
    /// Entries that were added or changed.
    #[serde(default)]
    pub new_quotes: Vec<DepthQuote>,
    /// Ids of the entries that were removed.
    #[serde(default, deserialize_with = "flex::list")]
    pub deleted_quotes: Vec<i64>,
}

/// `ProtoOAErrorRes` (and the proxy's own `ProtoErrorRes`, which has the same useful fields).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorRes {
    /// The account the error is about.
    #[serde(default, deserialize_with = "flex::opt")]
    pub ctid_trader_account_id: Option<i64>,
    /// The error code, for example `REQUEST_FREQUENCY_EXCEEDED`.
    pub error_code: String,
    /// The explanation.
    #[serde(default)]
    pub description: Option<String>,
    /// When maintenance ends, as a Unix time in seconds.
    #[serde(default, deserialize_with = "flex::opt")]
    pub maintenance_end_timestamp: Option<i64>,
    /// How long to wait before trying again, in seconds (with `BLOCKED_PAYLOAD_TYPE`, the time until
    /// that type of request is unblocked).
    #[serde(default, deserialize_with = "flex::opt")]
    pub retry_after: Option<i64>,
}

/// `ProtoOAAccountsTokenInvalidatedEvent`: tokens stopped working.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsTokenInvalidatedEvent {
    /// The accounts whose tokens were invalidated.
    #[serde(default, deserialize_with = "flex::list")]
    pub ctid_trader_account_ids: Vec<i64>,
    /// Why.
    #[serde(default)]
    pub reason: Option<String>,
}

/// `ProtoOAClientDisconnectEvent`: the server is ending the connection.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientDisconnectEvent {
    /// Why.
    #[serde(default)]
    pub reason: Option<String>,
}

/// `ProtoOAAccountDisconnectEvent`: an account was logged out of the connection.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountDisconnectEvent {
    /// The account.
    #[serde(deserialize_with = "flex::int")]
    pub ctid_trader_account_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integers_are_read_from_numbers_and_from_text() {
        let a: WireTick = serde_json::from_value(json!({"timestamp": 5, "tick": 108500})).unwrap();
        let b: WireTick =
            serde_json::from_value(json!({"timestamp": "5", "tick": "108500"})).unwrap();
        assert_eq!(a, b);
        let c: WireTick = serde_json::from_value(json!({"timestamp": -3.0, "tick": 1})).unwrap();
        assert_eq!(c.timestamp, -3);
        assert!(serde_json::from_value::<WireTick>(json!({"timestamp": "x", "tick": 1})).is_err());
        assert!(serde_json::from_value::<WireTick>(json!({"timestamp": 1.5, "tick": 1})).is_err());
    }

    #[test]
    fn a_spot_event_may_carry_only_one_side() {
        let e: SpotEvent =
            serde_json::from_value(json!({"symbolId": 1, "bid": 108499, "timestamp": 99})).unwrap();
        assert_eq!((e.bid, e.ask, e.timestamp), (Some(108_499), None, Some(99)));
        assert!(e.trendbar.is_empty());
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let e: VersionRes =
            serde_json::from_value(json!({"version": "1.2", "somethingNew": [1, 2]})).unwrap();
        assert_eq!(e.version, "1.2");
    }

    #[test]
    fn requests_use_the_official_field_names_and_skip_missing_options() {
        let req = GetTrendbarsReq {
            ctid_trader_account_id: 7,
            from_timestamp: Some(1),
            to_timestamp: None,
            period: 1,
            symbol_id: 3,
            count: None,
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
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
    fn an_account_list_reads_ids_sent_as_text() {
        let res: AccountsRes = serde_json::from_value(json!({
            "accessToken": "t",
            "permissionScope": 0,
            "ctidTraderAccount": [
                {"ctidTraderAccountId": "40212", "isLive": false, "traderLogin": 9001, "brokerTitleShort": "IC"}
            ]
        }))
        .unwrap();
        assert_eq!(res.ctid_trader_account[0].ctid_trader_account_id, 40_212);
        assert_eq!(res.ctid_trader_account[0].is_live, Some(false));
        assert_eq!(res.permission_scope, Some(0));
    }

    #[test]
    fn a_depth_event_reads_its_lists() {
        let e: DepthEvent = serde_json::from_value(json!({
            "symbolId": 1,
            "newQuotes": [{"id": 5, "size": 100, "bid": 108490}],
            "deletedQuotes": [1, "2"]
        }))
        .unwrap();
        assert_eq!(e.new_quotes[0].bid, Some(108_490));
        assert_eq!(e.deleted_quotes, vec![1, 2]);
    }

    #[test]
    fn an_error_answer_keeps_the_retry_advice() {
        let e: ErrorRes = serde_json::from_value(json!({
            "errorCode": "REQUEST_FREQUENCY_EXCEEDED", "description": "slow down", "retryAfter": 2
        }))
        .unwrap();
        assert_eq!(e.retry_after, Some(2));
    }
}
