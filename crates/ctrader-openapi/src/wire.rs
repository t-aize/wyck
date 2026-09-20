//! The message envelope and the payload type numbers.
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

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The payload type numbers this crate uses.
pub mod payload {
    /// `ProtoErrorRes`: an error from the proxy itself.
    pub const PROXY_ERROR_RES: u32 = 50;
    /// `ProtoHeartbeatEvent`: sent both ways to show the connection is alive.
    pub const HEARTBEAT_EVENT: u32 = 51;

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
    /// `ProtoOASymbolsListReq`.
    pub const SYMBOLS_LIST_REQ: u32 = 2114;
    /// `ProtoOASymbolsListRes`.
    pub const SYMBOLS_LIST_RES: u32 = 2115;
    /// `ProtoOASymbolByIdReq`.
    pub const SYMBOL_BY_ID_REQ: u32 = 2116;
    /// `ProtoOASymbolByIdRes`.
    pub const SYMBOL_BY_ID_RES: u32 = 2117;
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
    /// `ProtoOARefreshTokenReq`.
    pub const REFRESH_TOKEN_REQ: u32 = 2173;
    /// `ProtoOARefreshTokenRes`.
    pub const REFRESH_TOKEN_RES: u32 = 2174;
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
    /// [`crate::OpenApiError::Protocol`] when the payload cannot be turned into JSON.
    pub fn request<T: Serialize>(
        payload_type: u32,
        id: impl Into<String>,
        payload: &T,
    ) -> crate::Result<Self> {
        Ok(Self {
            client_msg_id: Some(id.into()),
            payload_type,
            payload: serde_json::to_value(payload).map_err(|e| {
                crate::OpenApiError::Protocol(format!("cannot encode a request: {e}"))
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
    /// [`crate::OpenApiError::Protocol`] when the payload does not have the shape of `T`.
    pub fn decode<T: serde::de::DeserializeOwned>(&self) -> crate::Result<T> {
        serde_json::from_value(self.payload.clone()).map_err(|e| {
            crate::OpenApiError::Protocol(format!(
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
    /// [`crate::OpenApiError::Protocol`] when it cannot be serialized.
    pub fn to_text(&self) -> crate::Result<String> {
        serde_json::to_string(self)
            .map_err(|e| crate::OpenApiError::Protocol(format!("cannot encode a message: {e}")))
    }

    /// Reads a text frame.
    ///
    /// # Errors
    ///
    /// [`crate::OpenApiError::Protocol`] when it is not a valid envelope.
    pub fn from_text(text: &str) -> crate::Result<Self> {
        serde_json::from_str(text)
            .map_err(|e| crate::OpenApiError::Protocol(format!("not a valid message: {e}")))
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
        assert_eq!(e.kind(), crate::ErrorKind::Protocol);
    }

    #[test]
    fn payload_numbers_match_the_official_enum() {
        // A few spot checks against ProtoOAPayloadType, so a typo cannot slip in.
        assert_eq!(payload::APPLICATION_AUTH_REQ, 2100);
        assert_eq!(payload::SUBSCRIBE_SPOTS_REQ, 2127);
        assert_eq!(payload::GET_TRENDBARS_REQ, 2137);
        assert_eq!(payload::ERROR_RES, 2142);
        assert_eq!(payload::GET_TICK_DATA_REQ, 2145);
        assert_eq!(payload::REFRESH_TOKEN_RES, 2174);
        assert_eq!(payload::SUBSCRIBE_LIVE_TRENDBAR_RES, 2165);
    }
}
