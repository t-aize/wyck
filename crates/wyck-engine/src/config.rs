//! Engine configuration.
//!
//! Every field has a safe default, so `EngineConfig::default()` is a valid production
//! configuration. [`EngineConfig::validate`] is called by [`crate::Engine::start`]; it
//! rejects values that would make the engine misbehave (a zero refresh interval that
//! would spin, a reconnect ceiling below its floor) instead of silently clamping them.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::EngineError;

/// How the session supervisor refreshes state and recovers from failures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    /// Interval between account and position refreshes while `Ready`. Default 5 s.
    #[serde(with = "duration_ms")]
    pub refresh_interval: Duration,
    /// Interval between quote refreshes for watched symbols. Default 1 s.
    #[serde(with = "duration_ms")]
    pub quote_interval: Duration,
    /// Liveness ping interval. Default 15 s.
    #[serde(with = "duration_ms")]
    pub ping_interval: Duration,
    /// Consecutive refresh failures that turn into a reconnect. Default 3.
    pub max_refresh_failures: u32,
    /// First reconnect delay; doubles per attempt. Default 1 s.
    #[serde(with = "duration_ms")]
    pub reconnect_initial: Duration,
    /// Ceiling for the reconnect delay. Default 30 s.
    #[serde(with = "duration_ms")]
    pub reconnect_max: Duration,
    /// Reconnect attempts before the session is declared `Failed`. Default 8.
    pub reconnect_attempts: u32,
    /// Ceiling for one broker round trip made by the engine itself. Default 15 s.
    #[serde(with = "duration_ms")]
    pub request_timeout: Duration,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(5),
            quote_interval: Duration::from_secs(1),
            ping_interval: Duration::from_secs(15),
            max_refresh_failures: 3,
            reconnect_initial: Duration::from_secs(1),
            reconnect_max: Duration::from_secs(30),
            reconnect_attempts: 8,
            request_timeout: Duration::from_secs(15),
        }
    }
}

/// Order pipeline limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TradingConfig {
    /// Minimum time between two order submissions for the same symbol. Protects against a
    /// held-down or double-tapped hotkey. Default 750 ms.
    #[serde(with = "duration_ms")]
    pub min_order_interval: Duration,
    /// How long to wait for a broker answer to a mutating call before reporting the
    /// outcome as unknown and reconciling. Default 10 s.
    #[serde(with = "duration_ms")]
    pub order_timeout: Duration,
    /// Delay before re-reading positions to confirm a submitted order. Default 300 ms.
    #[serde(with = "duration_ms")]
    pub confirm_delay: Duration,
    /// Confirmation reads before an order is reported `Unknown`. Default 5.
    pub confirm_attempts: u32,
    /// Slippage tolerance for market orders, in points. `None` leaves the broker default.
    pub market_slippage_points: Option<i64>,
    /// How long a flatten preview stays valid. Default 30 s.
    #[serde(with = "duration_ms")]
    pub flatten_preview_ttl: Duration,
    /// How long an order plan stays submittable. A plan is priced from a live quote, so an
    /// old one must be re-planned rather than sent. Default 15 s.
    #[serde(with = "duration_ms")]
    pub plan_ttl: Duration,
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            min_order_interval: Duration::from_millis(750),
            order_timeout: Duration::from_secs(10),
            confirm_delay: Duration::from_millis(300),
            confirm_attempts: 5,
            market_slippage_points: None,
            flatten_preview_ttl: Duration::from_secs(30),
            plan_ttl: Duration::from_secs(15),
        }
    }
}

/// Thresholds for the non-blocking guardrails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GuardrailConfig {
    /// Warn when an order's own risk exceeds this percentage of balance. Default 2.0.
    pub max_risk_percent_per_trade: f64,
    /// Warn when open plus new risk exceeds this percentage of balance. Default 5.0.
    pub max_total_risk_percent: f64,
    /// Warn when account data is older than this. Default 30 s.
    #[serde(with = "duration_ms")]
    pub stale_account_after: Duration,
}

impl Default for GuardrailConfig {
    fn default() -> Self {
        Self {
            max_risk_percent_per_trade: 2.0,
            max_total_risk_percent: 5.0,
            stale_account_after: Duration::from_secs(30),
        }
    }
}

/// Volume rules assumed when the broker does not publish them.
///
/// The Remote server exposes no lot size, volume step or minimum volume per symbol, so the
/// engine has to assume them. Anything built from the global values is flagged
/// [`SpecsSource::Assumed`](crate::domain::SpecsSource) and planning says so. Rules the
/// user entered for a symbol in [`AssumedSpecs::symbols`] are flagged
/// [`SpecsSource::Configured`](crate::domain::SpecsSource) and do not raise that warning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AssumedSpecs {
    /// Units per lot. Default 100_000 (standard forex lot).
    pub lot_size: f64,
    /// Smallest tradable volume in units. Default 1_000 (0.01 lot).
    pub min_volume_units: i64,
    /// Volume increment in units. Default 1_000 (0.01 lot).
    pub volume_step_units: i64,
    /// Per-symbol rules, keyed by symbol name (case-insensitive). They win over the three
    /// global values above. The live servers show why they are needed: EURUSD trades in
    /// steps of 1000 units, XAUUSD in steps of 1 unit, BTCUSD in steps of 0.01 unit.
    pub symbols: BTreeMap<String, SymbolVolumeRules>,
}

/// Volume rules for one symbol, all in base-asset units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SymbolVolumeRules {
    /// Units per lot.
    pub lot_size: f64,
    /// Smallest tradable volume, in units (may be fractional, 0.01 for BTCUSD).
    pub min_volume: f64,
    /// Volume increment, in units.
    pub volume_step: f64,
    /// Largest tradable volume per order, in units, when known.
    #[serde(default)]
    pub max_volume: Option<f64>,
}

impl SymbolVolumeRules {
    fn is_valid(&self) -> bool {
        [self.lot_size, self.min_volume, self.volume_step]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
            && self
                .max_volume
                .is_none_or(|m| m.is_finite() && m >= self.min_volume)
    }
}

impl AssumedSpecs {
    /// The configured rules for `symbol`, when there are any.
    #[must_use]
    pub fn rules_for(&self, symbol: &str) -> Option<&SymbolVolumeRules> {
        self.symbols
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(symbol))
            .map(|(_, rules)| rules)
    }
}

impl Default for AssumedSpecs {
    fn default() -> Self {
        Self {
            lot_size: 100_000.0,
            min_volume_units: 1_000,
            volume_step_units: 1_000,
            symbols: BTreeMap::new(),
        }
    }
}

/// Top-level engine configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    /// Session supervision.
    pub session: SessionConfig,
    /// Order pipeline.
    pub trading: TradingConfig,
    /// Guardrail thresholds.
    pub guardrails: GuardrailConfig,
    /// Volume rules used when the broker publishes none.
    pub assumed_specs: AssumedSpecs,
    /// Capacity of the event broadcast channel. Slow subscribers that fall further behind
    /// than this get a `Lagged` error and must resynchronize from the state snapshot.
    /// Default 256.
    pub event_buffer: usize,
    /// How many recent events the activity log keeps. Default 200.
    pub activity_log_len: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            session: SessionConfig::default(),
            trading: TradingConfig::default(),
            guardrails: GuardrailConfig::default(),
            assumed_specs: AssumedSpecs::default(),
            event_buffer: 256,
            activity_log_len: 200,
        }
    }
}

impl EngineConfig {
    /// Checks the configuration for values the engine cannot work with.
    ///
    /// # Errors
    ///
    /// [`EngineError::Config`] naming the offending field.
    pub fn validate(&self) -> Result<(), EngineError> {
        let s = &self.session;
        let t = &self.trading;
        let g = &self.guardrails;
        let a = &self.assumed_specs;

        let positive: [(&str, Duration); 8] = [
            ("session.refresh_interval", s.refresh_interval),
            ("session.quote_interval", s.quote_interval),
            ("session.ping_interval", s.ping_interval),
            ("session.reconnect_initial", s.reconnect_initial),
            ("session.request_timeout", s.request_timeout),
            ("trading.order_timeout", t.order_timeout),
            ("trading.flatten_preview_ttl", t.flatten_preview_ttl),
            ("trading.plan_ttl", t.plan_ttl),
        ];
        for (name, value) in positive {
            if value.is_zero() {
                return Err(EngineError::Config(format!(
                    "{name} must be greater than zero"
                )));
            }
        }
        if s.reconnect_max < s.reconnect_initial {
            return Err(EngineError::Config(
                "session.reconnect_max must not be below session.reconnect_initial".into(),
            ));
        }
        if s.max_refresh_failures == 0 {
            return Err(EngineError::Config(
                "session.max_refresh_failures must be at least 1".into(),
            ));
        }
        if t.confirm_attempts == 0 {
            return Err(EngineError::Config(
                "trading.confirm_attempts must be at least 1".into(),
            ));
        }
        for (name, value) in [
            (
                "guardrails.max_risk_percent_per_trade",
                g.max_risk_percent_per_trade,
            ),
            (
                "guardrails.max_total_risk_percent",
                g.max_total_risk_percent,
            ),
        ] {
            if !(value.is_finite() && value > 0.0) {
                return Err(EngineError::Config(format!(
                    "{name} must be a positive number"
                )));
            }
        }
        if !(a.lot_size.is_finite() && a.lot_size > 0.0) {
            return Err(EngineError::Config(
                "assumed_specs.lot_size must be a positive number".into(),
            ));
        }
        if a.min_volume_units <= 0 || a.volume_step_units <= 0 {
            return Err(EngineError::Config(
                "assumed_specs volumes must be positive".into(),
            ));
        }
        if let Some((name, _)) = a.symbols.iter().find(|(_, rules)| !rules.is_valid()) {
            return Err(EngineError::Config(format!(
                "assumed_specs.symbols.{name} needs a positive lot size, minimum and step, and a maximum not below the minimum"
            )));
        }
        if self.event_buffer == 0 {
            return Err(EngineError::Config(
                "event_buffer must be at least 1".into(),
            ));
        }
        Ok(())
    }
}

/// Serde adapter storing a [`Duration`] as whole milliseconds, which is what a person
/// editing a config file expects to write.
mod duration_ms {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(value: &Duration, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(u64::try_from(value.as_millis()).unwrap_or(u64::MAX))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        Ok(Duration::from_millis(u64::deserialize(d)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        EngineConfig::default().validate().unwrap();
    }

    #[test]
    fn rejects_unusable_values() {
        let mut c = EngineConfig::default();
        c.session.refresh_interval = Duration::ZERO;
        assert!(
            matches!(c.validate(), Err(EngineError::Config(m)) if m.contains("refresh_interval"))
        );

        let mut c = EngineConfig::default();
        c.session.reconnect_max = Duration::from_millis(1);
        assert!(c.validate().is_err());

        let mut c = EngineConfig::default();
        c.guardrails.max_total_risk_percent = f64::NAN;
        assert!(c.validate().is_err());

        let mut c = EngineConfig::default();
        c.assumed_specs.volume_step_units = 0;
        assert!(c.validate().is_err());

        let c = EngineConfig {
            event_buffer: 0,
            ..EngineConfig::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn durations_are_milliseconds_and_missing_fields_take_defaults() {
        let json = serde_json::to_value(EngineConfig::default()).unwrap();
        assert_eq!(json["session"]["refresh_interval"], 5000);
        let parsed: EngineConfig =
            serde_json::from_str(r#"{"session":{"refresh_interval":250}}"#).unwrap();
        assert_eq!(parsed.session.refresh_interval, Duration::from_millis(250));
        assert_eq!(parsed.trading, TradingConfig::default());
        assert_eq!(parsed.event_buffer, 256);
    }
}
