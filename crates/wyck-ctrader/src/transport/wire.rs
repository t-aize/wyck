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
//! accepts both (see [`wyck_core::flex`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The payload type numbers this crate uses.
pub(crate) mod payload {
    /// `ProtoErrorRes`: an error from the proxy itself.
    pub(crate) const PROXY_ERROR_RES: u32 = 50;
    /// `ProtoHeartbeatEvent`: sent both ways to show the connection is alive.
    pub(crate) const HEARTBEAT_EVENT: u32 = 51;

    /// `ProtoOAApplicationAuthReq`.
    pub(crate) const APPLICATION_AUTH_REQ: u32 = 2100;
    /// `ProtoOAApplicationAuthRes`.
    pub(crate) const APPLICATION_AUTH_RES: u32 = 2101;
    /// `ProtoOAAccountAuthReq`.
    pub(crate) const ACCOUNT_AUTH_REQ: u32 = 2102;
    /// `ProtoOAAccountAuthRes`.
    pub(crate) const ACCOUNT_AUTH_RES: u32 = 2103;
    /// `ProtoOANewOrderReq`.
    pub(crate) const NEW_ORDER_REQ: u32 = 2106;
    /// `ProtoOATrailingSLChangedEvent`.
    pub(crate) const TRAILING_SL_CHANGED_EVENT: u32 = 2107;
    /// `ProtoOACancelOrderReq`.
    pub(crate) const CANCEL_ORDER_REQ: u32 = 2108;
    /// `ProtoOAAmendOrderReq`.
    pub(crate) const AMEND_ORDER_REQ: u32 = 2109;
    /// `ProtoOAAmendPositionSLTPReq`.
    pub(crate) const AMEND_POSITION_SLTP_REQ: u32 = 2110;
    /// `ProtoOAClosePositionReq`.
    pub(crate) const CLOSE_POSITION_REQ: u32 = 2111;
    /// `ProtoOAOrderErrorEvent`.
    pub(crate) const ORDER_ERROR_EVENT: u32 = 2132;

    /// `ProtoOASymbolsListReq`.
    pub(crate) const SYMBOLS_LIST_REQ: u32 = 2114;
    /// `ProtoOASymbolsListRes`.
    pub(crate) const SYMBOLS_LIST_RES: u32 = 2115;
    /// `ProtoOASymbolByIdReq`.
    pub(crate) const SYMBOL_BY_ID_REQ: u32 = 2116;
    /// `ProtoOASymbolByIdRes`.
    pub(crate) const SYMBOL_BY_ID_RES: u32 = 2117;
    /// `ProtoOASymbolsForConversionReq`.
    pub(crate) const SYMBOLS_FOR_CONVERSION_REQ: u32 = 2118;
    /// `ProtoOASymbolsForConversionRes`.
    pub(crate) const SYMBOLS_FOR_CONVERSION_RES: u32 = 2119;
    /// `ProtoOASymbolChangedEvent`.
    pub(crate) const SYMBOL_CHANGED_EVENT: u32 = 2120;
    /// `ProtoOASubscribeSpotsReq`.
    pub(crate) const SUBSCRIBE_SPOTS_REQ: u32 = 2127;
    /// `ProtoOASubscribeSpotsRes`.
    pub(crate) const SUBSCRIBE_SPOTS_RES: u32 = 2128;
    /// `ProtoOAUnsubscribeSpotsReq`.
    pub(crate) const UNSUBSCRIBE_SPOTS_REQ: u32 = 2129;
    /// `ProtoOAUnsubscribeSpotsRes`.
    pub(crate) const UNSUBSCRIBE_SPOTS_RES: u32 = 2130;
    /// `ProtoOASpotEvent`.
    pub(crate) const SPOT_EVENT: u32 = 2131;
    /// `ProtoOASubscribeLiveTrendbarReq`.
    pub(crate) const SUBSCRIBE_LIVE_TRENDBAR_REQ: u32 = 2135;
    /// `ProtoOAUnsubscribeLiveTrendbarReq`.
    pub(crate) const UNSUBSCRIBE_LIVE_TRENDBAR_REQ: u32 = 2136;
    /// `ProtoOAGetTrendbarsReq`.
    pub(crate) const GET_TRENDBARS_REQ: u32 = 2137;
    /// `ProtoOAGetTrendbarsRes`.
    pub(crate) const GET_TRENDBARS_RES: u32 = 2138;
    /// `ProtoOAErrorRes`.
    pub(crate) const ERROR_RES: u32 = 2142;
    /// `ProtoOAGetTickDataReq`.
    pub(crate) const GET_TICK_DATA_REQ: u32 = 2145;
    /// `ProtoOAGetTickDataRes`.
    pub(crate) const GET_TICK_DATA_RES: u32 = 2146;
    /// `ProtoOAAccountsTokenInvalidatedEvent`.
    pub(crate) const ACCOUNTS_TOKEN_INVALIDATED_EVENT: u32 = 2147;
    /// `ProtoOAClientDisconnectEvent`.
    pub(crate) const CLIENT_DISCONNECT_EVENT: u32 = 2148;
    /// `ProtoOAGetAccountListByAccessTokenReq`.
    pub(crate) const GET_ACCOUNTS_BY_ACCESS_TOKEN_REQ: u32 = 2149;
    /// `ProtoOAGetAccountListByAccessTokenRes`.
    pub(crate) const GET_ACCOUNTS_BY_ACCESS_TOKEN_RES: u32 = 2150;
    /// `ProtoOADepthEvent`.
    pub(crate) const DEPTH_EVENT: u32 = 2155;
    /// `ProtoOASubscribeDepthQuotesReq`.
    pub(crate) const SUBSCRIBE_DEPTH_QUOTES_REQ: u32 = 2156;
    /// `ProtoOASubscribeDepthQuotesRes`.
    pub(crate) const SUBSCRIBE_DEPTH_QUOTES_RES: u32 = 2157;
    /// `ProtoOAAccountDisconnectEvent`.
    pub(crate) const ACCOUNT_DISCONNECT_EVENT: u32 = 2164;
    /// `ProtoOASubscribeLiveTrendbarRes`.
    pub(crate) const SUBSCRIBE_LIVE_TRENDBAR_RES: u32 = 2165;
    /// `ProtoOAUnsubscribeLiveTrendbarRes`.
    pub(crate) const UNSUBSCRIBE_LIVE_TRENDBAR_RES: u32 = 2166;
    /// `ProtoOAAssetListReq`.
    pub(crate) const ASSET_LIST_REQ: u32 = 2112;
    /// `ProtoOAAssetListRes`.
    pub(crate) const ASSET_LIST_RES: u32 = 2113;
    /// `ProtoOATraderReq`.
    pub(crate) const TRADER_REQ: u32 = 2121;
    /// `ProtoOATraderRes`.
    pub(crate) const TRADER_RES: u32 = 2122;
    /// `ProtoOATraderUpdatedEvent`.
    pub(crate) const TRADER_UPDATE_EVENT: u32 = 2123;
    /// `ProtoOAReconcileReq`.
    pub(crate) const RECONCILE_REQ: u32 = 2124;
    /// `ProtoOAReconcileRes`.
    pub(crate) const RECONCILE_RES: u32 = 2125;
    /// `ProtoOAExecutionEvent`.
    pub(crate) const EXECUTION_EVENT: u32 = 2126;
    /// `ProtoOADealListReq`.
    pub(crate) const DEAL_LIST_REQ: u32 = 2133;
    /// `ProtoOADealListRes`.
    pub(crate) const DEAL_LIST_RES: u32 = 2134;
    /// `ProtoOAAssetClassListReq`.
    pub(crate) const ASSET_CLASS_LIST_REQ: u32 = 2153;
    /// `ProtoOAAssetClassListRes`.
    pub(crate) const ASSET_CLASS_LIST_RES: u32 = 2154;
    /// `ProtoOASymbolCategoryListReq`.
    pub(crate) const SYMBOL_CATEGORY_REQ: u32 = 2160;
    /// `ProtoOASymbolCategoryListRes`.
    pub(crate) const SYMBOL_CATEGORY_RES: u32 = 2161;
    /// `ProtoOAOrderListReq`.
    pub(crate) const ORDER_LIST_REQ: u32 = 2175;
    /// `ProtoOAOrderListRes`.
    pub(crate) const ORDER_LIST_RES: u32 = 2176;
    /// `ProtoOAGetPositionUnrealizedPnLReq`.
    pub(crate) const GET_POSITION_UNREALIZED_PNL_REQ: u32 = 2187;
    /// `ProtoOAGetPositionUnrealizedPnLRes`.
    pub(crate) const GET_POSITION_UNREALIZED_PNL_RES: u32 = 2188;
    /// `ProtoOAExpectedMarginReq`.
    pub(crate) const EXPECTED_MARGIN_REQ: u32 = 2139;
    /// `ProtoOAExpectedMarginRes`.
    pub(crate) const EXPECTED_MARGIN_RES: u32 = 2140;
    /// `ProtoOAMarginChangedEvent`.
    pub(crate) const MARGIN_CHANGED_EVENT: u32 = 2141;
    /// `ProtoOAMarginCallUpdateEvent`.
    pub(crate) const MARGIN_CALL_UPDATE_EVENT: u32 = 2171;
    /// `ProtoOAMarginCallTriggerEvent`.
    pub(crate) const MARGIN_CALL_TRIGGER_EVENT: u32 = 2172;
}

/// One message on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Envelope {
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
    /// [`crate::Error::Protocol`] when the payload cannot be turned into JSON.
    pub(crate) fn request<T: Serialize>(
        payload_type: u32,
        id: impl Into<String>,
        payload: &T,
    ) -> crate::Result<Self> {
        Ok(Self {
            client_msg_id: Some(id.into()),
            payload_type,
            payload: serde_json::to_value(payload)
                .map_err(|e| crate::Error::Protocol(format!("cannot encode a request: {e}")))?,
        })
    }

    #[must_use]
    pub(crate) fn heartbeat() -> Self {
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
    /// [`crate::Error::Protocol`] when the payload does not have the shape of `T`.
    pub(crate) fn decode<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        serde_json::from_value(self.payload.clone()).map_err(|e| {
            crate::Error::Protocol(format!(
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
    /// [`crate::Error::Protocol`] when it cannot be serialized.
    pub(crate) fn to_text(&self) -> crate::Result<String> {
        serde_json::to_string(self)
            .map_err(|e| crate::Error::Protocol(format!("cannot encode a message: {e}")))
    }

    /// Reads a text frame.
    ///
    /// # Errors
    ///
    /// [`crate::Error::Protocol`] when it is not a valid envelope.
    pub(crate) fn from_text(text: &str) -> crate::Result<Self> {
        serde_json::from_str(text)
            .map_err(|e| crate::Error::Protocol(format!("not a valid message: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wyck_core::flex;

    #[test]
    fn a_request_serializes_with_camel_case_keys() {
        let envelope = Envelope::request(payload::APPLICATION_AUTH_REQ, "abc", &json!({})).unwrap();
        let value: Value = serde_json::from_str(&envelope.to_text().unwrap()).unwrap();
        assert_eq!(
            value,
            json!({"clientMsgId": "abc", "payloadType": 2100, "payload": {}})
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
        assert_eq!(e.kind(), crate::ErrorKind::Protocol);
    }

    #[test]
    fn payload_numbers_match_the_official_enum() {
        assert_eq!(payload::TRADER_REQ, 2121);
        assert_eq!(payload::RECONCILE_RES, 2125);
        assert_eq!(payload::DEAL_LIST_REQ, 2133);
        assert_eq!(payload::ORDER_LIST_RES, 2176);
        // A few spot checks against ProtoOAPayloadType, so a typo cannot slip in.
        assert_eq!(payload::APPLICATION_AUTH_REQ, 2100);
        assert_eq!(payload::SUBSCRIBE_SPOTS_REQ, 2127);
        assert_eq!(payload::GET_TRENDBARS_REQ, 2137);
        assert_eq!(payload::ERROR_RES, 2142);
        assert_eq!(payload::GET_TICK_DATA_REQ, 2145);
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
        assert_eq!(payload::GET_POSITION_UNREALIZED_PNL_REQ, 2187);
        assert_eq!(payload::GET_POSITION_UNREALIZED_PNL_RES, 2188);
        // Margin.
        assert_eq!(payload::EXPECTED_MARGIN_REQ, 2139);
        assert_eq!(payload::EXPECTED_MARGIN_RES, 2140);
        assert_eq!(payload::MARGIN_CHANGED_EVENT, 2141);
        assert_eq!(payload::MARGIN_CALL_UPDATE_EVENT, 2171);
        assert_eq!(payload::MARGIN_CALL_TRIGGER_EVENT, 2172);
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
