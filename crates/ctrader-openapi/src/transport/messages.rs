//! The messages of the connection itself: signing the application and an account in, listing
//! accounts, refreshing tokens, the version, the cTrader ID profile, and the notices that end a
//! connection or an account's place on it.
//!
//! One struct per message, with the names and meaning of the official `.proto` files
//! (`OpenApiMessages.proto`, `OpenApiModelMessages.proto`), in camelCase as JSON wants. Only the
//! fields the client uses are kept; the server may send more and they are ignored.
//!
//! The requests that carry a secret do not implement `Debug`, so a stray `{:?}` cannot log one.

use serde::{Deserialize, Serialize};

use super::wire::flex;

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

/// `ProtoOAAccountAuthRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAuthRes {
    /// The account that was authorized.
    #[serde(deserialize_with = "flex::int")]
    pub ctid_trader_account_id: i64,
}

/// `ProtoOAGetAccountListByAccessTokenReq`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetAccountsByAccessTokenReq {
    /// The access token.
    pub access_token: String,
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

/// `ProtoOARefreshTokenReq`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenReq {
    /// The refresh token.
    pub refresh_token: String,
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

/// `ProtoOAVersionRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRes {
    /// The proxy's version.
    pub version: String,
}

/// `ProtoOAGetCtidProfileByTokenReq`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CtidProfileReq {
    /// The access token.
    pub access_token: String,
}

/// The profile of a cTrader ID (`ProtoOACtidProfile`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtidProfile {
    /// The user id.
    #[serde(deserialize_with = "flex::int")]
    pub user_id: i64,
}

/// `ProtoOAGetCtidProfileByTokenRes`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CtidProfileRes {
    /// The profile.
    pub profile: CtidProfile,
}

/// A request that only names the account: shared by several calls across the domains (the account
/// itself, the reference catalogs, the margin call list, logging out) that take nothing else.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountReq {
    /// The trading account id.
    pub ctid_trader_account_id: i64,
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
    fn unknown_fields_are_ignored() {
        let e: VersionRes =
            serde_json::from_value(json!({"version": "1.2", "somethingNew": [1, 2]})).unwrap();
        assert_eq!(e.version, "1.2");
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
    fn an_error_answer_keeps_the_retry_advice() {
        let e: ErrorRes = serde_json::from_value(json!({
            "errorCode": "REQUEST_FREQUENCY_EXCEEDED", "description": "slow down", "retryAfter": 2
        }))
        .unwrap();
        assert_eq!(e.retry_after, Some(2));
    }
}
