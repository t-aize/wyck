//! Account snapshots.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use super::UnixMillis;
use crate::ids::AccountId;

/// Whether an account trades real money.
///
/// Best effort and informational only: it is read from the access token, never verified,
/// and never used to grant or deny anything. `Unknown` is a first-class answer and a front
/// end should show it as such rather than assume demo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AccountKind {
    /// A demo account.
    Demo,
    /// A live account.
    Live,
    /// Could not be determined.
    Unknown,
}

impl AccountKind {
    /// Reads the `environment` claim from an access token that is a JWT.
    ///
    /// The payload segment is base64url-decoded and parsed; nothing else about the token is
    /// inspected and the signature is **not** verified. Any token that is not a JWT, or has
    /// no recognizable claim, yields [`AccountKind::Unknown`]. The token itself is never
    /// logged or stored.
    #[must_use]
    pub fn from_token(token: &SecretString) -> Self {
        Self::from_token_str(token.expose_secret())
    }

    fn from_token_str(token: &str) -> Self {
        let Some(payload) = token.split('.').nth(1) else {
            return Self::Unknown;
        };
        let Ok(bytes) = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')) else {
            return Self::Unknown;
        };
        let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Self::Unknown;
        };
        match claims
            .get("environment")
            .and_then(serde_json::Value::as_str)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("demo") => Self::Demo,
            Some("live") => Self::Live,
            _ => Self::Unknown,
        }
    }
}

/// The account's headline figures at one instant, in display units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountSnapshot {
    /// Which account this is.
    pub account_id: AccountId,
    /// Demo, live or unknown.
    pub kind: AccountKind,
    /// Account currency code, when the broker reports one.
    pub currency: Option<String>,
    /// Balance in account currency.
    pub balance: Option<f64>,
    /// Equity in account currency.
    pub equity: Option<f64>,
    /// Free margin in account currency.
    pub free_margin: Option<f64>,
    /// Used margin in account currency, when reported.
    pub used_margin: Option<f64>,
    /// Margin level as a percentage, when reported.
    pub margin_level_pct: Option<f64>,
    /// The server build, when reported.
    pub server_version: Option<String>,
    /// When this snapshot was taken, in Unix milliseconds.
    pub captured_at: UnixMillis,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jwt(payload: &str) -> String {
        format!(
            "eyJhbGciOiJIUzI1NiJ9.{}.sig",
            URL_SAFE_NO_PAD.encode(payload)
        )
    }

    #[test]
    fn reads_the_environment_claim() {
        assert_eq!(
            AccountKind::from_token_str(&jwt(r#"{"environment":"demo"}"#)),
            AccountKind::Demo
        );
        assert_eq!(
            AccountKind::from_token_str(&jwt(r#"{"environment":"LIVE"}"#)),
            AccountKind::Live
        );
    }

    #[test]
    fn anything_else_is_unknown() {
        for token in [
            "",
            "not-a-jwt",
            "a.b.c",
            "a.!!!.c",
            &jwt(r#"{"sub":"x"}"#),
            &jwt(r#"{"environment":"staging"}"#),
            &jwt("not json"),
        ] {
            assert_eq!(
                AccountKind::from_token_str(token),
                AccountKind::Unknown,
                "{token}"
            );
        }
    }

    #[test]
    fn accepts_padded_payloads() {
        let padded = format!(
            "h.{}=.s",
            URL_SAFE_NO_PAD.encode(r#"{"environment":"demo"}"#)
        );
        assert_eq!(AccountKind::from_token_str(&padded), AccountKind::Demo);
    }
}
