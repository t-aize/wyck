//! Types shared by both the Local and Remote clients.
//!
//! Enum *casing* still differs per server at the wire level (Local accepts
//! case-insensitive input and echoes PascalCase, per `Q-L3`; Remote requires uppercase
//! input and always echoes uppercase) even though the underlying concept (buy vs. sell)
//! is identical: [`TradeSide`] centralizes that with server-specific serialization
//! helpers instead of duplicating the enum per server.

use serde::{Deserialize, Serialize};

/// The direction of a trade, order, or position.
///
/// - **Local** (`references/local-http-server.md` "Side enum casing on Local", `Q-L3`):
///   input is case-insensitive (`"buy"`/`"BUY"` both accepted); responses use PascalCase
///   (`"Buy"`/`"Sell"`). Use [`TradeSide::as_local_input`] / [`TradeSide::as_local_wire`]
///   for the two directions.
/// - **Remote** (`references/remote-http-server.md` "Side enum casing on Remote"):
///   input and output are always uppercase (`"BUY"`/`"SELL"`). This is the enum's
///   default `Serialize`/`Deserialize` mapping, so `TradeSide` can be used directly in
///   Remote DTOs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    /// The lowercase form accepted by every Local input field (`side` on
    /// `place_*_order`, the `risk_reward` drawing object's `side` field). Local accepts
    /// any casing per `Q-L3`; lowercase is used here purely as this crate's canonical
    /// choice.
    pub fn as_local_input(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }

    /// The PascalCase form Local echoes on `tradeSide` in responses (`Q-L3`).
    pub fn as_local_wire(self) -> &'static str {
        match self {
            Self::Buy => "Buy",
            Self::Sell => "Sell",
        }
    }

    /// Parses either casing of `"buy"`/`"sell"` (covers both Local's PascalCase
    /// responses and Remote's uppercase responses).
    pub fn parse_any_case(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "buy" => Some(Self::Buy),
            "sell" => Some(Self::Sell),
            _ => None,
        }
    }

    /// The opposite side: useful when computing the closing-deal side expected during
    /// post-flight verification (`self-healing-playbook.md` §2.1: "closing trade is
    /// recorded ... with side opposite to `tradeSide`").
    pub fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }
}

/// The 9-value trendbar granularity enum shared by both servers.
///
/// `references/remote-http-server.md` "`period` enum: 9 values" (`Q-R1`) documents that
/// the Remote `get_trendbars.period` field accepts exactly these 9 values, not the
/// 26-value superset earlier documentation claimed: `get_trendbars(period="M_2", ...)`
/// returns an MCP `-32602` schema-mismatch error. Local's `get_trendbars.period` uses an
/// equivalent 9-timeframe set. Centralizing the enum here means an unsupported
/// granularity (`M_2`, `M_3`, `H_3`, ...) is a compile error in caller code, not a
/// runtime rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Period {
    #[serde(rename = "M_1")]
    M1,
    #[serde(rename = "M_5")]
    M5,
    #[serde(rename = "M_15")]
    M15,
    #[serde(rename = "M_30")]
    M30,
    #[serde(rename = "H_1")]
    H1,
    #[serde(rename = "H_4")]
    H4,
    #[serde(rename = "D_1")]
    D1,
    #[serde(rename = "W_1")]
    W1,
    #[serde(rename = "MN_1")]
    MN1,
}

impl Period {
    /// The wire enum value, e.g. `"M_1"`.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::M1 => "M_1",
            Self::M5 => "M_5",
            Self::M15 => "M_15",
            Self::M30 => "M_30",
            Self::H1 => "H_1",
            Self::H4 => "H_4",
            Self::D1 => "D_1",
            Self::W1 => "W_1",
            Self::MN1 => "MN_1",
        }
    }

    /// The timeframe's duration in minutes, used to compute Local `get_trendbars`
    /// pagination window sizes (`Q-L4`: `window_minutes = 1000 * timeframe_minutes`).
    /// `MN1` (calendar month) has no fixed minute duration; this returns the average
    /// Gregorian month length (43,200 minutes / 30 days) as a conservative window-sizing
    /// approximation: callers doing exact calendar-month windowing should compute
    /// month boundaries directly instead of relying on this value.
    pub fn approx_minutes(self) -> i64 {
        match self {
            Self::M1 => 1,
            Self::M5 => 5,
            Self::M15 => 15,
            Self::M30 => 30,
            Self::H1 => 60,
            Self::H4 => 240,
            Self::D1 => 1_440,
            Self::W1 => 10_080,
            Self::MN1 => 43_200,
        }
    }

    /// If `label` is a granularity this enum does not support (e.g. the legacy claim's
    /// `"M_2"`, `"M_3"`, `"H_3"`), suggests the nearest supported alternative per the
    /// `Q-R1` workaround ("propose the nearest supported alternative"). Returns `None`
    /// when `label` already names a supported [`Period`] (use `label.parse()`, see the
    /// [`std::str::FromStr`] impl, for the exact match instead).
    pub fn suggest_alternative(label: &str) -> Option<Period> {
        match label {
            "M_2" | "M_3" | "M_4" => Some(Period::M5),
            "M_10" => Some(Period::M15),
            "M_20" => Some(Period::M15),
            "M_45" => Some(Period::M30),
            "H_2" | "H_3" => Some(Period::H1),
            "H_6" | "H_8" | "H_12" => Some(Period::H4),
            _ => None,
        }
    }
}

impl std::str::FromStr for Period {
    type Err = UnsupportedPeriod;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "M_1" => Ok(Self::M1),
            "M_5" => Ok(Self::M5),
            "M_15" => Ok(Self::M15),
            "M_30" => Ok(Self::M30),
            "H_1" => Ok(Self::H1),
            "H_4" => Ok(Self::H4),
            "D_1" => Ok(Self::D1),
            "W_1" => Ok(Self::W1),
            "MN_1" => Ok(Self::MN1),
            other => Err(UnsupportedPeriod {
                requested: other.to_owned(),
                suggestion: Period::suggest_alternative(other),
            }),
        }
    }
}

/// `period` did not match one of the 9 values `get_trendbars` actually supports
/// (`Q-R1`). Carries a suggested nearest alternative when this crate recognizes the
/// requested granularity as a legacy/unsupported claim.
#[derive(Debug, thiserror::Error)]
#[error(
    "unsupported trendbar period `{requested}` (only M_1, M_5, M_15, M_30, H_1, H_4, D_1, W_1, MN_1 are supported){}",
    suggestion.map(|p| format!(": did you mean {}?", p.as_wire_str())).unwrap_or_default()
)]
pub struct UnsupportedPeriod {
    pub requested: String,
    pub suggestion: Option<Period>,
}

/// Converts a raw `10^money_digits`-scaled integer (Remote's money encoding: see
/// `references/remote-http-server.md` "Money encoding on Remote") to its display value.
pub fn money_from_raw(raw: i64, money_digits: u32) -> f64 {
    raw as f64 / 10f64.powi(money_digits as i32)
}

/// Converts a display money value to Remote's raw `10^money_digits`-scaled integer
/// encoding. Rounds to the nearest integer rather than truncating, since money values
/// are expected to already be aligned to the currency's minor unit.
pub fn money_to_raw(display: f64, money_digits: u32) -> i64 {
    (display * 10f64.powi(money_digits as i32)).round() as i64
}

/// Converts a Remote integer-pipettes price to its display value (`display = pipettes /
/// 10^pip_digits`), per `references/remote-http-server.md` "Price encoding on Remote"
/// and the `Q-K19` pipettes-vs-display foot-gun this function exists specifically to
/// prevent.
pub fn price_from_pipettes(pipettes: i64, pip_digits: u32) -> f64 {
    pipettes as f64 / 10f64.powi(pip_digits as i32)
}

/// Converts a display price to Remote's integer-pipettes encoding.
pub fn price_to_pipettes(display: f64, pip_digits: u32) -> i64 {
    (display * 10f64.powi(pip_digits as i32)).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_side_roundtrips_any_case() {
        assert_eq!(TradeSide::parse_any_case("buy"), Some(TradeSide::Buy));
        assert_eq!(TradeSide::parse_any_case("BUY"), Some(TradeSide::Buy));
        assert_eq!(TradeSide::parse_any_case("Buy"), Some(TradeSide::Buy));
        assert_eq!(TradeSide::parse_any_case("sell"), Some(TradeSide::Sell));
        assert_eq!(TradeSide::parse_any_case("nope"), None);
    }

    #[test]
    fn trade_side_opposite() {
        assert_eq!(TradeSide::Buy.opposite(), TradeSide::Sell);
        assert_eq!(TradeSide::Sell.opposite(), TradeSide::Buy);
    }

    #[test]
    fn period_rejects_unsupported_granularity_with_suggestion() {
        let err = "M_2".parse::<Period>().unwrap_err();
        assert_eq!(err.suggestion, Some(Period::M5));
    }

    #[test]
    fn period_parses_all_nine_supported_values() {
        for wire in [
            "M_1", "M_5", "M_15", "M_30", "H_1", "H_4", "D_1", "W_1", "MN_1",
        ] {
            let parsed: Period = wire.parse().unwrap();
            assert_eq!(parsed.as_wire_str(), wire);
        }
    }

    #[test]
    fn money_and_price_conversions_round_trip() {
        assert!((money_from_raw(123_456, 2) - 1_234.56).abs() < 1e-9);
        assert_eq!(money_to_raw(1_234.56, 2), 123_456);
        assert!((price_from_pipettes(117_090, 5) - 1.1709).abs() < 1e-9);
        assert_eq!(price_to_pipettes(1.1709, 5), 117_090);
    }
}
