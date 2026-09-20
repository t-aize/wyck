//! What the server tells the client without being asked.
//!
//! A connection carries two kinds of traffic: answers to requests (matched by `clientMsgId`, see
//! [`crate::Client`]) and **events**, which the server sends on its own: a new price, a change of
//! the order book, an account logged out, tokens invalidated. Events are broadcast to every reader
//! of [`crate::Client::events`], already decoded.
//!
//! The end of the connection is also an event ([`Event::Disconnected`]), so a task that only reads
//! events learns about it without polling the client.

use serde_json::Value;

use crate::account::TraderUpdatedEvent;
use crate::error::OpenApiError;
use crate::model::{
    AccountDisconnectEvent, AccountsTokenInvalidatedEvent, ClientDisconnectEvent, DepthEvent,
    ErrorRes, SpotEvent,
};
use crate::wire::{Envelope, payload};

/// Why a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    /// [`crate::Client::close`] was called.
    ClosedByClient,
    /// The server closed the WebSocket.
    ClosedByServer,
    /// The server announced it was ending the connection (`ProtoOAClientDisconnectEvent`).
    ServerAnnounced(Option<String>),
    /// The connection failed: the text says how.
    Failed(String),
}

/// Something the server sent by itself, or the end of the connection.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Event {
    /// A new price, or the first one after a subscription. It may also carry live bars.
    Spot(SpotEvent),
    /// A change of the order book.
    Depth(DepthEvent),
    /// The account changed (a balance moved, for example).
    TraderUpdated(TraderUpdatedEvent),
    /// Tokens stopped working: refresh them, or sign in again.
    TokensInvalidated(AccountsTokenInvalidatedEvent),
    /// An account was logged out of this connection: authorize it again to keep using it.
    AccountDisconnected(AccountDisconnectEvent),
    /// The server says it is ending the connection.
    ServerDisconnecting(ClientDisconnectEvent),
    /// An error with no request to attach it to.
    ServerError(OpenApiError),
    /// A message this client does not know. Its type and raw payload are kept.
    Other {
        /// The payload type number.
        payload_type: u32,
        /// The payload, as sent.
        payload: Value,
    },
    /// The connection ended. This is always the last event.
    Disconnected(DisconnectReason),
}

/// Turns an unsolicited message into an [`Event`]. Heartbeats give `None`: they are only for the
/// connection's own upkeep.
#[must_use]
pub fn event_from(envelope: &Envelope) -> Option<Event> {
    let decode_failed = |e: OpenApiError| Event::ServerError(e);
    Some(match envelope.payload_type {
        payload::HEARTBEAT_EVENT => return None,
        payload::SPOT_EVENT => envelope
            .decode()
            .map(Event::Spot)
            .unwrap_or_else(decode_failed),
        payload::DEPTH_EVENT => envelope
            .decode()
            .map(Event::Depth)
            .unwrap_or_else(decode_failed),
        payload::TRADER_UPDATE_EVENT => envelope
            .decode()
            .map(Event::TraderUpdated)
            .unwrap_or_else(decode_failed),
        payload::ACCOUNTS_TOKEN_INVALIDATED_EVENT => envelope
            .decode()
            .map(Event::TokensInvalidated)
            .unwrap_or_else(decode_failed),
        payload::ACCOUNT_DISCONNECT_EVENT => envelope
            .decode()
            .map(Event::AccountDisconnected)
            .unwrap_or_else(decode_failed),
        payload::CLIENT_DISCONNECT_EVENT => envelope
            .decode()
            .map(Event::ServerDisconnecting)
            .unwrap_or_else(decode_failed),
        payload::ERROR_RES | payload::PROXY_ERROR_RES => Event::ServerError(error_of(envelope)),
        other => Event::Other {
            payload_type: other,
            payload: envelope.payload.clone(),
        },
    })
}

/// The error an error message describes: an [`OpenApiError::Server`] with its code and advice, or
/// an [`OpenApiError::Protocol`] when the message cannot be read.
#[must_use]
pub fn error_of(envelope: &Envelope) -> OpenApiError {
    match envelope.decode::<ErrorRes>() {
        Ok(res) => OpenApiError::server(
            res.error_code,
            res.description,
            res.retry_after,
            res.maintenance_end_timestamp,
        ),
        Err(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env(payload_type: u32, payload: Value) -> Envelope {
        Envelope {
            client_msg_id: None,
            payload_type,
            payload,
        }
    }

    #[test]
    fn a_heartbeat_is_not_an_event() {
        assert!(event_from(&Envelope::heartbeat()).is_none());
    }

    #[test]
    fn a_spot_event_is_decoded() {
        let event = event_from(&env(
            payload::SPOT_EVENT,
            json!({"ctidTraderAccountId": 1, "symbolId": 7, "bid": 108499, "ask": 108501}),
        ))
        .unwrap();
        match event {
            Event::Spot(spot) => {
                assert_eq!(
                    (spot.symbol_id, spot.bid, spot.ask),
                    (7, Some(108_499), Some(108_501))
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn account_and_token_events_are_decoded() {
        let invalidated = event_from(&env(
            payload::ACCOUNTS_TOKEN_INVALIDATED_EVENT,
            json!({"ctidTraderAccountIds": [1, 2], "reason": "revoked"}),
        ))
        .unwrap();
        assert!(
            matches!(invalidated, Event::TokensInvalidated(e) if e.ctid_trader_account_ids == [1, 2])
        );

        let disconnected = event_from(&env(
            payload::ACCOUNT_DISCONNECT_EVENT,
            json!({"ctidTraderAccountId": 9}),
        ))
        .unwrap();
        assert!(
            matches!(disconnected, Event::AccountDisconnected(e) if e.ctid_trader_account_id == 9)
        );

        let ending = event_from(&env(
            payload::CLIENT_DISCONNECT_EVENT,
            json!({"reason": "bye"}),
        ))
        .unwrap();
        assert!(
            matches!(ending, Event::ServerDisconnecting(e) if e.reason.as_deref() == Some("bye"))
        );
    }

    #[test]
    fn an_error_without_a_request_is_a_server_error_event() {
        let event = event_from(&env(
            payload::ERROR_RES,
            json!({"errorCode": "SERVER_IS_UNDER_MAINTENANCE", "maintenanceEndTimestamp": 5}),
        ))
        .unwrap();
        match event {
            Event::ServerError(OpenApiError::Server {
                code,
                maintenance_end,
                ..
            }) => {
                assert_eq!(code, "SERVER_IS_UNDER_MAINTENANCE");
                assert_eq!(maintenance_end, Some(5));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_broken_event_payload_is_reported_not_dropped() {
        let event = event_from(&env(payload::SPOT_EVENT, json!({"bid": "not a number"}))).unwrap();
        assert!(matches!(
            event,
            Event::ServerError(OpenApiError::Protocol(_))
        ));
    }

    #[test]
    fn an_unknown_message_is_kept_raw() {
        let event = event_from(&env(9999, json!({"x": 1}))).unwrap();
        assert_eq!(
            event,
            Event::Other {
                payload_type: 9999,
                payload: json!({"x": 1})
            }
        );
    }
}
