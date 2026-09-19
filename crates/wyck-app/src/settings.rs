//! Application settings: which account to connect to, what a hotkey order looks like, and
//! which keys to bind.
//!
//! Until the settings screen and the persisted engine configuration exist (`TODO.md` 4.4),
//! everything is read from environment variables, with defaults that are safe. The parsing
//! takes a lookup function instead of reading the process environment directly, so it is
//! tested without touching global state.
//!
//! | Variable | Meaning | Default |
//! |---|---|---|
//! | `WYCK_SERVICE` | `remote` or `local`. Setting any of the three connection variables selects the environment instead of the active profile | active profile |
//! | `WYCK_ENDPOINT` | Server URL | the service's default |
//! | `WYCK_TOKEN` | Remote bearer token | none |
//! | `WYCK_SYMBOL` | The symbol the hotkeys trade | `EURUSD` |
//! | `WYCK_WATCH` | Comma separated extra symbols to quote | none |
//! | `WYCK_RISK_PERCENT` | Risk per hotkey order, percent of balance | `0.25` |
//! | `WYCK_STOP_PIPS` | Stop distance of a hotkey order, in pips | `20` |
//! | `WYCK_REWARD_RISK` | Take profit as a multiple of the stop | `2` |
//! | `WYCK_HOTKEY_BUY`, `WYCK_HOTKEY_SELL`, `WYCK_HOTKEY_PANEL` | Global shortcuts | `ctrl+alt+b`, `ctrl+alt+s`, `ctrl+alt+p` |
//! | `WYCK_LOG` | `tracing` filter directive | `warn,wyck_app=info,wyck_engine=info` |
//! | `WYCK_NEWS` | `on` or `off`: host the economic calendar | `on` |

use secrecy::SecretString;
use wyck_engine::broker::ServiceKind;

/// A setting that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    /// A variable holds a value the application cannot use.
    #[error("{name}: {reason}")]
    Invalid {
        /// The variable.
        name: &'static str,
        /// What is wrong with it.
        reason: String,
    },
}

/// Where the connection comes from.
#[derive(Debug)]
pub enum ConnectionChoice {
    /// The profile marked active in `wyck-config`, with its token from the secret store.
    ActiveProfile,
    /// Given directly by environment variables. Meant for development and for demo
    /// accounts: a token in the environment is visible to every process of the user.
    Environment {
        /// Which server family.
        service: ServiceKind,
        /// The server URL.
        endpoint: String,
        /// The bearer token, if the service needs one. Never printed.
        token: Option<SecretString>,
    },
}

/// What a hotkey order looks like. The engine turns it into an exact volume from the account
/// balance and the live quote.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrderDefaults {
    /// Risk per order, percent of balance, greater than 0 and below 100.
    pub risk_percent: f64,
    /// Stop loss distance in pips, greater than 0.
    pub stop_pips: f64,
    /// Take profit as a multiple of the stop distance, greater than 0.
    pub reward_risk: f64,
}

impl Default for OrderDefaults {
    fn default() -> Self {
        Self {
            risk_percent: 0.25,
            stop_pips: 20.0,
            reward_risk: 2.0,
        }
    }
}

/// The global shortcuts, as text such as `ctrl+alt+b`. They are parsed and checked when they
/// are registered (see [`crate::hotkeys`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyText {
    /// Plan a buy (dry run).
    pub buy: String,
    /// Plan a sell (dry run).
    pub sell: String,
    /// Show or hide the floating panel.
    pub panel: String,
}

impl Default for HotkeyText {
    fn default() -> Self {
        Self {
            buy: "ctrl+alt+b".to_owned(),
            sell: "ctrl+alt+s".to_owned(),
            panel: "ctrl+alt+p".to_owned(),
        }
    }
}

/// Everything the application is configured with.
#[derive(Debug)]
pub struct AppSettings {
    /// Where the connection comes from.
    pub connection: ConnectionChoice,
    /// The symbol the hotkeys trade.
    pub symbol: String,
    /// Extra symbols to keep quotes for. The traded symbol is always watched.
    pub watch: Vec<String>,
    /// What a hotkey order looks like.
    pub order: OrderDefaults,
    /// The global shortcuts.
    pub hotkeys: HotkeyText,
    /// The `tracing` filter directive.
    pub log_filter: String,
    /// Whether the engine hosts the economic calendar (one request to a public feed).
    pub news_enabled: bool,
}

impl AppSettings {
    /// Reads the settings from the process environment.
    ///
    /// # Errors
    ///
    /// [`SettingsError::Invalid`] naming the variable that cannot be used.
    pub fn from_env() -> Result<Self, SettingsError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Reads the settings through `lookup`, which returns the value of a variable if it is set.
    /// Blank values count as unset.
    ///
    /// # Errors
    ///
    /// [`SettingsError::Invalid`] naming the variable that cannot be used.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, SettingsError> {
        let get = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };

        let connection = match (get("WYCK_SERVICE"), get("WYCK_ENDPOINT"), get("WYCK_TOKEN")) {
            (None, None, None) => ConnectionChoice::ActiveProfile,
            (service, endpoint, token) => {
                let service = match service.as_deref().map(str::to_ascii_lowercase).as_deref() {
                    None | Some("remote") => ServiceKind::CtraderRemote,
                    Some("local") => ServiceKind::CtraderLocal,
                    Some(other) => {
                        return Err(invalid(
                            "WYCK_SERVICE",
                            format!("expected `remote` or `local`, got `{other}`"),
                        ));
                    }
                };
                ConnectionChoice::Environment {
                    endpoint: endpoint.unwrap_or_else(|| service.default_endpoint().to_owned()),
                    token: token.map(SecretString::from),
                    service,
                }
            }
        };

        let symbol = match get("WYCK_SYMBOL") {
            Some(text) => parse_symbol("WYCK_SYMBOL", &text)?,
            None => "EURUSD".to_owned(),
        };
        let watch = match get("WYCK_WATCH") {
            Some(text) => text
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| parse_symbol("WYCK_WATCH", s))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        let defaults = OrderDefaults::default();
        let order = OrderDefaults {
            risk_percent: parse_number(
                "WYCK_RISK_PERCENT",
                get("WYCK_RISK_PERCENT"),
                defaults.risk_percent,
            )?,
            stop_pips: parse_number("WYCK_STOP_PIPS", get("WYCK_STOP_PIPS"), defaults.stop_pips)?,
            reward_risk: parse_number(
                "WYCK_REWARD_RISK",
                get("WYCK_REWARD_RISK"),
                defaults.reward_risk,
            )?,
        };
        if order.risk_percent <= 0.0 || order.risk_percent >= 100.0 {
            return Err(invalid(
                "WYCK_RISK_PERCENT",
                format!("must be above 0 and below 100, got {}", order.risk_percent),
            ));
        }
        for (name, value) in [
            ("WYCK_STOP_PIPS", order.stop_pips),
            ("WYCK_REWARD_RISK", order.reward_risk),
        ] {
            if value <= 0.0 {
                return Err(invalid(name, format!("must be above 0, got {value}")));
            }
        }

        let hotkey_defaults = HotkeyText::default();
        let hotkeys = HotkeyText {
            buy: get("WYCK_HOTKEY_BUY").unwrap_or(hotkey_defaults.buy),
            sell: get("WYCK_HOTKEY_SELL").unwrap_or(hotkey_defaults.sell),
            panel: get("WYCK_HOTKEY_PANEL").unwrap_or(hotkey_defaults.panel),
        };

        Ok(Self {
            connection,
            symbol,
            watch,
            order,
            hotkeys,
            log_filter: get("WYCK_LOG")
                .unwrap_or_else(|| "warn,wyck_app=info,wyck_engine=info".to_owned()),
            news_enabled: parse_switch("WYCK_NEWS", get("WYCK_NEWS"), true)?,
        })
    }

    /// Every symbol to keep quotes for: the traded one first, then the extras, without
    /// duplicates.
    #[must_use]
    pub fn watched_symbols(&self) -> Vec<String> {
        let mut out = vec![self.symbol.clone()];
        for symbol in &self.watch {
            if !out.contains(symbol) {
                out.push(symbol.clone());
            }
        }
        out
    }
}

fn invalid(name: &'static str, reason: String) -> SettingsError {
    SettingsError::Invalid { name, reason }
}

/// A ticker: letters, digits, spaces and a few separators, upper-cased.
fn parse_symbol(name: &'static str, text: &str) -> Result<String, SettingsError> {
    let ok = !text.is_empty()
        && text.len() <= 24
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-' | '#'));
    if ok {
        Ok(text.to_ascii_uppercase())
    } else {
        Err(invalid(
            name,
            format!("`{text}` is not a valid symbol name"),
        ))
    }
}

fn parse_switch(
    name: &'static str,
    text: Option<String>,
    default: bool,
) -> Result<bool, SettingsError> {
    match text.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None => Ok(default),
        Some("on" | "true" | "1" | "yes") => Ok(true),
        Some("off" | "false" | "0" | "no") => Ok(false),
        Some(other) => Err(invalid(
            name,
            format!("expected `on` or `off`, got `{other}`"),
        )),
    }
}

fn parse_number(
    name: &'static str,
    text: Option<String>,
    default: f64,
) -> Result<f64, SettingsError> {
    let Some(text) = text else {
        return Ok(default);
    };
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(invalid(name, format!("`{text}` is not a finite number"))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use secrecy::ExposeSecret;

    use super::*;

    fn settings(pairs: &[(&str, &str)]) -> Result<AppSettings, SettingsError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        AppSettings::from_lookup(|name| map.get(name).cloned())
    }

    #[test]
    fn nothing_set_means_the_active_profile_and_safe_defaults() {
        let s = settings(&[]).unwrap();
        assert!(matches!(s.connection, ConnectionChoice::ActiveProfile));
        assert_eq!(s.symbol, "EURUSD");
        assert_eq!(s.order, OrderDefaults::default());
        assert_eq!(s.hotkeys, HotkeyText::default());
        assert_eq!(s.log_filter, "warn,wyck_app=info,wyck_engine=info");
        assert!(s.news_enabled);
        assert_eq!(s.watched_symbols(), ["EURUSD"]);
    }

    #[test]
    fn any_connection_variable_selects_the_environment() {
        let s = settings(&[("WYCK_TOKEN", "abc")]).unwrap();
        let ConnectionChoice::Environment {
            service,
            endpoint,
            token,
        } = s.connection
        else {
            panic!("expected the environment");
        };
        assert_eq!(service, ServiceKind::CtraderRemote);
        assert_eq!(endpoint, ServiceKind::CtraderRemote.default_endpoint());
        assert_eq!(token.unwrap().expose_secret(), "abc");

        let s = settings(&[
            ("WYCK_SERVICE", "LOCAL"),
            ("WYCK_ENDPOINT", "http://127.0.0.1:1/mcp/"),
        ])
        .unwrap();
        let ConnectionChoice::Environment {
            service,
            endpoint,
            token,
        } = s.connection
        else {
            panic!("expected the environment");
        };
        assert_eq!(service, ServiceKind::CtraderLocal);
        assert_eq!(endpoint, "http://127.0.0.1:1/mcp/");
        assert!(token.is_none());
    }

    #[test]
    fn the_token_never_shows_in_debug_output() {
        let s = settings(&[("WYCK_TOKEN", "super-secret-token")]).unwrap();
        assert!(!format!("{s:?}").contains("super-secret-token"));
    }

    #[test]
    fn blank_values_count_as_unset() {
        let s = settings(&[("WYCK_SERVICE", "  "), ("WYCK_SYMBOL", "")]).unwrap();
        assert!(matches!(s.connection, ConnectionChoice::ActiveProfile));
        assert_eq!(s.symbol, "EURUSD");
    }

    #[test]
    fn symbols_are_validated_and_upper_cased() {
        let s = settings(&[
            ("WYCK_SYMBOL", "us 500"),
            ("WYCK_WATCH", "gbpusd, xauusd,,EURUSD, us 500"),
        ])
        .unwrap();
        assert_eq!(s.symbol, "US 500");
        assert_eq!(
            s.watched_symbols(),
            ["US 500", "GBPUSD", "XAUUSD", "EURUSD"]
        );
        assert!(settings(&[("WYCK_SYMBOL", "EUR/USD")]).is_err());
        assert!(settings(&[("WYCK_WATCH", "GBPUSD,;DROP")]).is_err());
    }

    #[test]
    fn order_defaults_are_range_checked() {
        for (name, value) in [
            ("WYCK_RISK_PERCENT", "0"),
            ("WYCK_RISK_PERCENT", "100"),
            ("WYCK_RISK_PERCENT", "-1"),
            ("WYCK_RISK_PERCENT", "nan"),
            ("WYCK_RISK_PERCENT", "abc"),
            ("WYCK_STOP_PIPS", "0"),
            ("WYCK_REWARD_RISK", "-2"),
        ] {
            let error = settings(&[(name, value)]).unwrap_err();
            assert!(
                error.to_string().starts_with(name),
                "{name}={value}: {error}"
            );
        }
        let s = settings(&[("WYCK_RISK_PERCENT", "1.5"), ("WYCK_STOP_PIPS", "12.5")]).unwrap();
        assert!((s.order.risk_percent - 1.5).abs() < f64::EPSILON);
        assert!((s.order.stop_pips - 12.5).abs() < f64::EPSILON);
    }

    #[test]
    fn the_calendar_can_be_switched_off() {
        assert!(!settings(&[("WYCK_NEWS", "off")]).unwrap().news_enabled);
        assert!(settings(&[("WYCK_NEWS", "sometimes")]).is_err());
    }

    #[test]
    fn an_unknown_service_is_refused() {
        let error = settings(&[("WYCK_SERVICE", "ftx")]).unwrap_err();
        assert!(error.to_string().contains("remote"));
    }
}
