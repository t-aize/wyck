//! What the server tells the client without being asked.

use serde_json::Value;
use tracing::{debug, warn};

use crate::openapi::account::TraderUpdatedEvent;
use crate::openapi::error::Error;
use crate::openapi::margin::{MarginCallTriggerEvent, MarginCallUpdateEvent, MarginChangedEvent};
use crate::openapi::market::{DepthEvent, SpotEvent, SymbolChangedEvent};
use crate::openapi::trading::{ExecutionEvent, OrderErrorEvent, TrailingSlChangedEvent};
use crate::openapi::transport::messages::{
    AccountDisconnectEvent, AccountsTokenInvalidatedEvent, ClientDisconnectEvent, ErrorRes,
};
use crate::openapi::transport::wire::{Envelope, payload};

/// Why a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisconnectReason {
    ClosedByClient,
    ClosedByServer,
    ServerAnnounced(Option<String>),
    Failed(String),
}

/// Something the server sent by itself, or the end of the connection.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Event {
    Spot(SpotEvent),
    Depth(DepthEvent),
    TraderUpdated(TraderUpdatedEvent),
    TokensInvalidated(AccountsTokenInvalidatedEvent),
    AccountDisconnected(AccountDisconnectEvent),
    ServerDisconnecting(ClientDisconnectEvent),
    Execution(Box<ExecutionEvent>),
    OrderError(OrderErrorEvent),
    TrailingSlChanged(TrailingSlChangedEvent),
    MarginChanged(MarginChangedEvent),
    MarginCallTriggered(MarginCallTriggerEvent),
    MarginCallUpdated(MarginCallUpdateEvent),
    SymbolChanged(SymbolChangedEvent),
    ServerError(Error),
    /// A message this client does not know.
    Other {
        payload_type: u32,
        payload: Value,
    },
    Disconnected(DisconnectReason),
}

/// Turns an unsolicited message into an `Event`.
#[must_use]
#[doc(hidden)]
pub fn event_from(envelope: &Envelope) -> Option<Event> {
    let decode_failed = |e: Error| {
        warn!(
            payload_type = envelope.payload_type,
            %e,
            "could not decode a message of a known type"
        );
        Event::ServerError(e)
    };
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
        payload::EXECUTION_EVENT => envelope
            .decode()
            .map(|e: ExecutionEvent| Event::Execution(Box::new(e)))
            .unwrap_or_else(decode_failed),
        payload::ORDER_ERROR_EVENT => envelope
            .decode()
            .map(Event::OrderError)
            .unwrap_or_else(decode_failed),
        payload::TRAILING_SL_CHANGED_EVENT => envelope
            .decode()
            .map(Event::TrailingSlChanged)
            .unwrap_or_else(decode_failed),
        payload::MARGIN_CHANGED_EVENT => envelope
            .decode()
            .map(Event::MarginChanged)
            .unwrap_or_else(decode_failed),
        payload::MARGIN_CALL_TRIGGER_EVENT => envelope
            .decode()
            .map(Event::MarginCallTriggered)
            .unwrap_or_else(decode_failed),
        payload::MARGIN_CALL_UPDATE_EVENT => envelope
            .decode()
            .map(Event::MarginCallUpdated)
            .unwrap_or_else(decode_failed),
        payload::SYMBOL_CHANGED_EVENT => envelope
            .decode()
            .map(Event::SymbolChanged)
            .unwrap_or_else(decode_failed),
        payload::ERROR_RES | payload::PROXY_ERROR_RES => Event::ServerError(error_of(envelope)),
        other => {
            debug!(
                payload_type = other,
                "an unrecognized message was kept as Event::Other"
            );
            Event::Other {
                payload_type: other,
                payload: envelope.payload.clone(),
            }
        }
    })
}

/// The error an error message describes: an `Error::Server` with its code and advice, or an
/// `Error::Protocol` when the message cannot be read.
#[must_use]
pub(crate) fn error_of(envelope: &Envelope) -> Error {
    match envelope.decode::<ErrorRes>() {
        Ok(res) => Error::server(
            res.error_code,
            res.description,
            res.retry_after,
            res.maintenance_end_timestamp,
        ),
        Err(error) => error,
    }
}

/// The error a `ProtoOAOrderErrorEvent` describes, for a trading request the server refused this
/// way instead of with a `ProtoOAErrorRes`: an `Error::Server` with its code and advice, or an
/// `Error::Protocol` when the message cannot be read.
#[must_use]
pub(crate) fn order_error_of(envelope: &Envelope) -> Error {
    match envelope.decode::<OrderErrorEvent>() {
        Ok(event) => Error::server(event.error_code, event.description, None, None),
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
            Event::ServerError(Error::Server {
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
        assert!(matches!(event, Event::ServerError(Error::Protocol(_))));
    }

    #[test]
    fn trading_and_margin_events_are_decoded() {
        let execution = event_from(&env(
            payload::EXECUTION_EVENT,
            json!({"ctidTraderAccountId": 1, "executionType": 2}),
        ))
        .unwrap();
        assert!(matches!(execution, Event::Execution(e) if e.execution_type == 2));

        let order_error = event_from(&env(
            payload::ORDER_ERROR_EVENT,
            json!({"errorCode": "NOT_ENOUGH_MONEY", "orderId": 9}),
        ))
        .unwrap();
        assert!(matches!(order_error, Event::OrderError(e) if e.order_id == Some(9)));

        let trailing = event_from(&env(
            payload::TRAILING_SL_CHANGED_EVENT,
            json!({"positionId": 1, "orderId": 2, "stopPrice": 1.1, "utcLastUpdateTimestamp": 5}),
        ))
        .unwrap();
        assert!(matches!(trailing, Event::TrailingSlChanged(e) if e.position_id == 1));

        let margin_changed = event_from(&env(
            payload::MARGIN_CHANGED_EVENT,
            json!({"positionId": 1, "usedMargin": 500}),
        ))
        .unwrap();
        assert!(matches!(margin_changed, Event::MarginChanged(e) if e.used_margin == 500));

        let margin_triggered = event_from(&env(
            payload::MARGIN_CALL_TRIGGER_EVENT,
            json!({"marginCall": {"marginCallType": 61, "marginLevelThreshold": 50.0}}),
        ))
        .unwrap();
        assert!(matches!(margin_triggered, Event::MarginCallTriggered(_)));

        let margin_updated = event_from(&env(
            payload::MARGIN_CALL_UPDATE_EVENT,
            json!({"marginCall": {"marginCallType": 61, "marginLevelThreshold": 60.0}}),
        ))
        .unwrap();
        assert!(matches!(margin_updated, Event::MarginCallUpdated(_)));

        let symbol_changed = event_from(&env(
            payload::SYMBOL_CHANGED_EVENT,
            json!({"symbolId": [1, 2]}),
        ))
        .unwrap();
        assert!(matches!(symbol_changed, Event::SymbolChanged(e) if e.symbol_id == vec![1, 2]));
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
