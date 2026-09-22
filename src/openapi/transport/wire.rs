//! The message envelope, the payload type numbers, and reading integers the server sends loosely
//! typed.
//!
//! Every message, in both directions, is wrapped in the same envelope. In JSON it looks like
//!
//! ```text
//! {"clientMsgId": "...", "payloadType": 2100, "payload": {...}}
//! ```
//!
//! `payloadType` says what the payload is (the numbers are in [`payload`], from the official
//! `ProtoOAPayloadType` enum). `clientMsgId` is chosen by the client on a request and echoed on
//! its answer, which is how answers are matched to requests. Messages the server sends by itself
//! (events) carry no `clientMsgId`.
//!
//! Numbers in JSON may be written as numbers or as strings (a 64 bit id or a price is often sent
//! as text by protobuf's JSON mapping), so every integer field across this crate's messages
//! accepts both (see [`flex`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The payload type numbers this crate uses.
pub mod payload {
    /// `ProtoErrorRes`: an error from the proxy itself.
    pub const PROXY_ERROR_RES: u32 = 50;
    /// `ProtoHeartbeatEvent`: sent both ways to show the connection is alive.
    pub const HEARTBEAT_EVENT: u32 = 51;

    // ---- Auth ----

    /// `ProtoOAApplicationAuthReq`.
    pub const APPLICATION_AUTH_REQ: u32 = 2100;
    /// `ProtoOAApplicationAuthRes`.
    pub const APPLICATION_AUTH_RES: u32 = 2101;
    /// `ProtoOAAccountAuthReq`.
    pub const ACCOUNT_AUTH_REQ: u32 = 2102;
    /// `ProtoOAAccountAuthRes`.
    pub const ACCOUNT_AUTH_RES: u32 = 2103;
    /// `ProtoOAVersionReq`.
    pub const VERSION_REQ: u32 = 2104;
    /// `ProtoOAVersionRes`.
    pub const VERSION_RES: u32 = 2105;

    // ---- Trading ----

    /// `ProtoOANewOrderReq`.
    pub const NEW_ORDER_REQ: u32 = 2106;
    /// `ProtoOATrailingSLChangedEvent`.
    pub const TRAILING_SL_CHANGED_EVENT: u32 = 2107;
    /// `ProtoOACancelOrderReq`.
    pub const CANCEL_ORDER_REQ: u32 = 2108;
    /// `ProtoOAAmendOrderReq`.
    pub const AMEND_ORDER_REQ: u32 = 2109;
    /// `ProtoOAAmendPositionSLTPReq`.
    pub const AMEND_POSITION_SLTP_REQ: u32 = 2110;
    /// `ProtoOAClosePositionReq`.
    pub const CLOSE_POSITION_REQ: u32 = 2111;
    /// `ProtoOAOrderErrorEvent`.
    pub const ORDER_ERROR_EVENT: u32 = 2132;

    // ---- Market ----

    /// `ProtoOASymbolsListReq`.
    pub const SYMBOLS_LIST_REQ: u32 = 2114;
    /// `ProtoOASymbolsListRes`.
    pub const SYMBOLS_LIST_RES: u32 = 2115;
    /// `ProtoOASymbolByIdReq`.
    pub const SYMBOL_BY_ID_REQ: u32 = 2116;
    /// `ProtoOASymbolByIdRes`.
    pub const SYMBOL_BY_ID_RES: u32 = 2117;
    /// `ProtoOASymbolsForConversionReq`.
    pub const SYMBOLS_FOR_CONVERSION_REQ: u32 = 2118;
    /// `ProtoOASymbolsForConversionRes`.
    pub const SYMBOLS_FOR_CONVERSION_RES: u32 = 2119;
    /// `ProtoOASymbolChangedEvent`.
    pub const SYMBOL_CHANGED_EVENT: u32 = 2120;
    /// `ProtoOASubscribeSpotsReq`.
    pub const SUBSCRIBE_SPOTS_REQ: u32 = 2127;
    /// `ProtoOASubscribeSpotsRes`.
    pub const SUBSCRIBE_SPOTS_RES: u32 = 2128;
    /// `ProtoOAUnsubscribeSpotsReq`.
    pub const UNSUBSCRIBE_SPOTS_REQ: u32 = 2129;
    /// `ProtoOAUnsubscribeSpotsRes`.
    pub const UNSUBSCRIBE_SPOTS_RES: u32 = 2130;
    /// `ProtoOASpotEvent`.
    pub const SPOT_EVENT: u32 = 2131;
    /// `ProtoOASubscribeLiveTrendbarReq`.
    pub const SUBSCRIBE_LIVE_TRENDBAR_REQ: u32 = 2135;
    /// `ProtoOAUnsubscribeLiveTrendbarReq`.
    pub const UNSUBSCRIBE_LIVE_TRENDBAR_REQ: u32 = 2136;
    /// `ProtoOAGetTrendbarsReq`.
    pub const GET_TRENDBARS_REQ: u32 = 2137;
    /// `ProtoOAGetTrendbarsRes`.
    pub const GET_TRENDBARS_RES: u32 = 2138;
    /// `ProtoOAErrorRes`.
    pub const ERROR_RES: u32 = 2142;
    /// `ProtoOAGetTickDataReq`.
    pub const GET_TICK_DATA_REQ: u32 = 2145;
    /// `ProtoOAGetTickDataRes`.
    pub const GET_TICK_DATA_RES: u32 = 2146;
    /// `ProtoOAAccountsTokenInvalidatedEvent`.
    pub const ACCOUNTS_TOKEN_INVALIDATED_EVENT: u32 = 2147;
    /// `ProtoOAClientDisconnectEvent`.
    pub const CLIENT_DISCONNECT_EVENT: u32 = 2148;
    /// `ProtoOAGetAccountListByAccessTokenReq`.
    pub const GET_ACCOUNTS_BY_ACCESS_TOKEN_REQ: u32 = 2149;
    /// `ProtoOAGetAccountListByAccessTokenRes`.
    pub const GET_ACCOUNTS_BY_ACCESS_TOKEN_RES: u32 = 2150;
    /// `ProtoOADepthEvent`.
    pub const DEPTH_EVENT: u32 = 2155;
    /// `ProtoOASubscribeDepthQuotesReq`.
    pub const SUBSCRIBE_DEPTH_QUOTES_REQ: u32 = 2156;
    /// `ProtoOASubscribeDepthQuotesRes`.
    pub const SUBSCRIBE_DEPTH_QUOTES_RES: u32 = 2157;
    /// `ProtoOAUnsubscribeDepthQuotesReq`.
    pub const UNSUBSCRIBE_DEPTH_QUOTES_REQ: u32 = 2158;
    /// `ProtoOAUnsubscribeDepthQuotesRes`.
    pub const UNSUBSCRIBE_DEPTH_QUOTES_RES: u32 = 2159;
    /// `ProtoOAAccountDisconnectEvent`.
    pub const ACCOUNT_DISCONNECT_EVENT: u32 = 2164;
    /// `ProtoOASubscribeLiveTrendbarRes`.
    pub const SUBSCRIBE_LIVE_TRENDBAR_RES: u32 = 2165;
    /// `ProtoOAUnsubscribeLiveTrendbarRes`.
    pub const UNSUBSCRIBE_LIVE_TRENDBAR_RES: u32 = 2166;
    /// `ProtoOAAssetListReq`.
    pub const ASSET_LIST_REQ: u32 = 2112;
    /// `ProtoOAAssetListRes`.
    pub const ASSET_LIST_RES: u32 = 2113;
    /// `ProtoOATraderReq`.
    pub const TRADER_REQ: u32 = 2121;
    /// `ProtoOATraderRes`.
    pub const TRADER_RES: u32 = 2122;
    /// `ProtoOATraderUpdatedEvent`.
    pub const TRADER_UPDATE_EVENT: u32 = 2123;
    /// `ProtoOAReconcileReq`.
    pub const RECONCILE_REQ: u32 = 2124;
    /// `ProtoOAReconcileRes`.
    pub const RECONCILE_RES: u32 = 2125;
    /// `ProtoOAExecutionEvent`.
    pub const EXECUTION_EVENT: u32 = 2126;
    /// `ProtoOADealListReq`.
    pub const DEAL_LIST_REQ: u32 = 2133;
    /// `ProtoOADealListRes`.
    pub const DEAL_LIST_RES: u32 = 2134;
    /// `ProtoOAGetCtidProfileByTokenReq`.
    pub const GET_CTID_PROFILE_BY_TOKEN_REQ: u32 = 2151;
    /// `ProtoOAGetCtidProfileByTokenRes`.
    pub const GET_CTID_PROFILE_BY_TOKEN_RES: u32 = 2152;
    /// `ProtoOAAssetClassListReq`.
    pub const ASSET_CLASS_LIST_REQ: u32 = 2153;
    /// `ProtoOAAssetClassListRes`.
    pub const ASSET_CLASS_LIST_RES: u32 = 2154;
    /// `ProtoOASymbolCategoryListReq`.
    pub const SYMBOL_CATEGORY_REQ: u32 = 2160;
    /// `ProtoOASymbolCategoryListRes`.
    pub const SYMBOL_CATEGORY_RES: u32 = 2161;
    /// `ProtoOAAccountLogoutReq`.
    pub const ACCOUNT_LOGOUT_REQ: u32 = 2162;
    /// `ProtoOAAccountLogoutRes`.
    pub const ACCOUNT_LOGOUT_RES: u32 = 2163;
    /// `ProtoOARefreshTokenReq`.
    pub const REFRESH_TOKEN_REQ: u32 = 2173;
    /// `ProtoOARefreshTokenRes`.
    pub const REFRESH_TOKEN_RES: u32 = 2174;
    /// `ProtoOAOrderListReq`.
    pub const ORDER_LIST_REQ: u32 = 2175;
    /// `ProtoOAOrderListRes`.
    pub const ORDER_LIST_RES: u32 = 2176;
    /// `ProtoOADealListByPositionIdReq`.
    pub const DEAL_LIST_BY_POSITION_ID_REQ: u32 = 2179;
    /// `ProtoOADealListByPositionIdRes`.
    pub const DEAL_LIST_BY_POSITION_ID_RES: u32 = 2180;
    /// `ProtoOAOrderDetailsReq`.
    pub const ORDER_DETAILS_REQ: u32 = 2181;
    /// `ProtoOAOrderDetailsRes`.
    pub const ORDER_DETAILS_RES: u32 = 2182;
    /// `ProtoOAOrderListByPositionIdReq`.
    pub const ORDER_LIST_BY_POSITION_ID_REQ: u32 = 2183;
    /// `ProtoOAOrderListByPositionIdRes`.
    pub const ORDER_LIST_BY_POSITION_ID_RES: u32 = 2184;
    /// `ProtoOADealOffsetListReq`.
    pub const DEAL_OFFSET_LIST_REQ: u32 = 2185;
    /// `ProtoOADealOffsetListRes`.
    pub const DEAL_OFFSET_LIST_RES: u32 = 2186;
    /// `ProtoOAGetPositionUnrealizedPnLReq`.
    pub const GET_POSITION_UNREALIZED_PNL_REQ: u32 = 2187;
    /// `ProtoOAGetPositionUnrealizedPnLRes`.
    pub const GET_POSITION_UNREALIZED_PNL_RES: u32 = 2188;
    /// `ProtoOACashFlowHistoryListReq`.
    pub const CASH_FLOW_HISTORY_LIST_REQ: u32 = 2143;
    /// `ProtoOACashFlowHistoryListRes`.
    pub const CASH_FLOW_HISTORY_LIST_RES: u32 = 2144;

    // ---- Margin ----

    /// `ProtoOAExpectedMarginReq`.
    pub const EXPECTED_MARGIN_REQ: u32 = 2139;
    /// `ProtoOAExpectedMarginRes`.
    pub const EXPECTED_MARGIN_RES: u32 = 2140;
    /// `ProtoOAMarginChangedEvent`.
    pub const MARGIN_CHANGED_EVENT: u32 = 2141;
    /// `ProtoOAMarginCallListReq`.
    pub const MARGIN_CALL_LIST_REQ: u32 = 2167;
    /// `ProtoOAMarginCallListRes`.
    pub const MARGIN_CALL_LIST_RES: u32 = 2168;
    /// `ProtoOAMarginCallUpdateReq`.
    pub const MARGIN_CALL_UPDATE_REQ: u32 = 2169;
    /// `ProtoOAMarginCallUpdateRes`.
    pub const MARGIN_CALL_UPDATE_RES: u32 = 2170;
    /// `ProtoOAMarginCallUpdateEvent`.
    pub const MARGIN_CALL_UPDATE_EVENT: u32 = 2171;
    /// `ProtoOAMarginCallTriggerEvent`.
    pub const MARGIN_CALL_TRIGGER_EVENT: u32 = 2172;
    /// `ProtoOAGetDynamicLeverageByIDReq`. The enum constant itself is named without "ById"
    /// (`PROTO_OA_GET_DYNAMIC_LEVERAGE_REQ`); the message it carries is `ProtoOAGetDynamicLeverageByIDReq`.
    pub const GET_DYNAMIC_LEVERAGE_REQ: u32 = 2177;
    /// `ProtoOAGetDynamicLeverageByIDRes`.
    pub const GET_DYNAMIC_LEVERAGE_RES: u32 = 2178;
}

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

/// One message on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Chosen by the client on a request; echoed on the answer; absent on events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_msg_id: Option<String>,
    /// What the payload is: a value from [`payload`].
    pub payload_type: u32,
    /// The message itself. An empty object when the message has no fields.
    #[serde(default = "empty_object")]
    pub payload: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

impl Envelope {
    /// A request of `payload_type` with `payload`, tagged `id`.
    ///
    /// # Errors
    ///
    /// [`crate::openapi::OpenApiError::Protocol`] when the payload cannot be turned into JSON.
    pub fn request<T: Serialize>(
        payload_type: u32,
        id: impl Into<String>,
        payload: &T,
    ) -> crate::openapi::Result<Self> {
        Ok(Self {
            client_msg_id: Some(id.into()),
            payload_type,
            payload: serde_json::to_value(payload).map_err(|e| {
                crate::openapi::OpenApiError::Protocol(format!("cannot encode a request: {e}"))
            })?,
        })
    }

    /// The heartbeat message.
    #[must_use]
    pub fn heartbeat() -> Self {
        Self {
            client_msg_id: None,
            payload_type: payload::HEARTBEAT_EVENT,
            payload: empty_object(),
        }
    }

    /// Reads the payload as `T`.
    ///
    /// # Errors
    ///
    /// [`crate::openapi::OpenApiError::Protocol`] when the payload does not have the shape of `T`.
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> crate::openapi::Result<T> {
        serde_json::from_value(self.payload.clone()).map_err(|e| {
            crate::openapi::OpenApiError::Protocol(format!(
                "cannot read message {} as {}: {e}",
                self.payload_type,
                std::any::type_name::<T>()
                    .rsplit("::")
                    .next()
                    .unwrap_or("?")
            ))
        })
    }

    /// The text to send.
    ///
    /// # Errors
    ///
    /// [`crate::openapi::OpenApiError::Protocol`] when it cannot be serialized.
    pub fn to_text(&self) -> crate::openapi::Result<String> {
        serde_json::to_string(self).map_err(|e| {
            crate::openapi::OpenApiError::Protocol(format!("cannot encode a message: {e}"))
        })
    }

    /// Reads a text frame.
    ///
    /// # Errors
    ///
    /// [`crate::openapi::OpenApiError::Protocol`] when it is not a valid envelope.
    pub fn from_text(text: &str) -> crate::openapi::Result<Self> {
        serde_json::from_str(text).map_err(|e| {
            crate::openapi::OpenApiError::Protocol(format!("not a valid message: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_request_serializes_with_camel_case_keys() {
        let envelope = Envelope::request(payload::VERSION_REQ, "abc", &json!({})).unwrap();
        let value: Value = serde_json::from_str(&envelope.to_text().unwrap()).unwrap();
        assert_eq!(
            value,
            json!({"clientMsgId": "abc", "payloadType": 2104, "payload": {}})
        );
    }

    #[test]
    fn an_event_has_no_client_message_id() {
        let text = r#"{"payloadType":2131,"payload":{"symbolId":1}}"#;
        let envelope = Envelope::from_text(text).unwrap();
        assert_eq!(envelope.client_msg_id, None);
        assert_eq!(envelope.payload_type, payload::SPOT_EVENT);
        // And a heartbeat without any payload at all is fine too.
        let beat = Envelope::from_text(r#"{"payloadType":51}"#).unwrap();
        assert_eq!(beat.payload, json!({}));
    }

    #[test]
    fn the_heartbeat_is_the_documented_message() {
        let text = Envelope::heartbeat().to_text().unwrap();
        assert_eq!(text, r#"{"payloadType":51,"payload":{}}"#);
    }

    #[test]
    fn garbage_is_a_protocol_error_not_a_panic() {
        assert!(Envelope::from_text("not json").is_err());
        assert!(Envelope::from_text(r#"{"payload":{}}"#).is_err(), "no type");
        let e = Envelope::from_text("[]").unwrap_err();
        assert_eq!(e.kind(), crate::openapi::ErrorKind::Protocol);
    }

    #[test]
    fn payload_numbers_match_the_official_enum() {
        assert_eq!(payload::TRADER_REQ, 2121);
        assert_eq!(payload::RECONCILE_RES, 2125);
        assert_eq!(payload::DEAL_LIST_REQ, 2133);
        assert_eq!(payload::ORDER_LIST_RES, 2176);
        assert_eq!(payload::ACCOUNT_LOGOUT_RES, 2163);
        // A few spot checks against ProtoOAPayloadType, so a typo cannot slip in.
        assert_eq!(payload::APPLICATION_AUTH_REQ, 2100);
        assert_eq!(payload::SUBSCRIBE_SPOTS_REQ, 2127);
        assert_eq!(payload::GET_TRENDBARS_REQ, 2137);
        assert_eq!(payload::ERROR_RES, 2142);
        assert_eq!(payload::GET_TICK_DATA_REQ, 2145);
        assert_eq!(payload::REFRESH_TOKEN_RES, 2174);
        assert_eq!(payload::SUBSCRIBE_LIVE_TRENDBAR_RES, 2165);
    }

    #[test]
    fn trading_account_and_margin_payload_numbers_match_the_official_enum() {
        // Trading.
        assert_eq!(payload::NEW_ORDER_REQ, 2106);
        assert_eq!(payload::TRAILING_SL_CHANGED_EVENT, 2107);
        assert_eq!(payload::CANCEL_ORDER_REQ, 2108);
        assert_eq!(payload::AMEND_ORDER_REQ, 2109);
        assert_eq!(payload::AMEND_POSITION_SLTP_REQ, 2110);
        assert_eq!(payload::CLOSE_POSITION_REQ, 2111);
        assert_eq!(payload::ORDER_ERROR_EVENT, 2132);
        // Market.
        assert_eq!(payload::SYMBOLS_FOR_CONVERSION_REQ, 2118);
        assert_eq!(payload::SYMBOLS_FOR_CONVERSION_RES, 2119);
        assert_eq!(payload::SYMBOL_CHANGED_EVENT, 2120);
        // Account.
        assert_eq!(payload::DEAL_LIST_BY_POSITION_ID_REQ, 2179);
        assert_eq!(payload::DEAL_LIST_BY_POSITION_ID_RES, 2180);
        assert_eq!(payload::ORDER_DETAILS_REQ, 2181);
        assert_eq!(payload::ORDER_DETAILS_RES, 2182);
        assert_eq!(payload::ORDER_LIST_BY_POSITION_ID_REQ, 2183);
        assert_eq!(payload::ORDER_LIST_BY_POSITION_ID_RES, 2184);
        assert_eq!(payload::DEAL_OFFSET_LIST_REQ, 2185);
        assert_eq!(payload::DEAL_OFFSET_LIST_RES, 2186);
        assert_eq!(payload::GET_POSITION_UNREALIZED_PNL_REQ, 2187);
        assert_eq!(payload::GET_POSITION_UNREALIZED_PNL_RES, 2188);
        assert_eq!(payload::CASH_FLOW_HISTORY_LIST_REQ, 2143);
        assert_eq!(payload::CASH_FLOW_HISTORY_LIST_RES, 2144);
        // Margin.
        assert_eq!(payload::EXPECTED_MARGIN_REQ, 2139);
        assert_eq!(payload::EXPECTED_MARGIN_RES, 2140);
        assert_eq!(payload::MARGIN_CHANGED_EVENT, 2141);
        assert_eq!(payload::MARGIN_CALL_LIST_REQ, 2167);
        assert_eq!(payload::MARGIN_CALL_LIST_RES, 2168);
        assert_eq!(payload::MARGIN_CALL_UPDATE_REQ, 2169);
        assert_eq!(payload::MARGIN_CALL_UPDATE_RES, 2170);
        assert_eq!(payload::MARGIN_CALL_UPDATE_EVENT, 2171);
        assert_eq!(payload::MARGIN_CALL_TRIGGER_EVENT, 2172);
        assert_eq!(payload::GET_DYNAMIC_LEVERAGE_REQ, 2177);
        assert_eq!(payload::GET_DYNAMIC_LEVERAGE_RES, 2178);
    }

    #[test]
    fn integers_are_read_from_numbers_and_from_text() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct One {
            #[serde(deserialize_with = "flex::int")]
            n: i64,
        }
        let a: One = serde_json::from_value(json!({"n": 5})).unwrap();
        let b: One = serde_json::from_value(json!({"n": "5"})).unwrap();
        assert_eq!(a, b);
        let c: One = serde_json::from_value(json!({"n": -3.0})).unwrap();
        assert_eq!(c.n, -3);
        assert!(serde_json::from_value::<One>(json!({"n": "x"})).is_err());
        assert!(serde_json::from_value::<One>(json!({"n": 1.5})).is_err());
    }
}
