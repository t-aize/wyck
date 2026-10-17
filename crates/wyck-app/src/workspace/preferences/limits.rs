//! How many objects the user lets a chart, a symbol and the alert list hold.

use crate::alerts::{DEFAULT_SAVED_ALERTS, MAX_SAVED_ALERTS};
use serde::{Deserialize, Serialize};
use wyck_chart::{DEFAULT_STUDIES_LIMIT, MAX_STUDIES};
use wyck_drawings::model::{DEFAULT_DRAWINGS_PER_SYMBOL, MAX_DRAWINGS_PER_SYMBOL};

/// User limits for adding new chart objects. Lowering one keeps objects already saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageLimits {
    #[serde(default = "default_studies_limit")]
    pub studies_per_chart: usize,
    #[serde(default = "default_alerts_limit")]
    pub alerts: usize,
    #[serde(default = "default_drawings_limit")]
    pub drawings_per_symbol: usize,
}

fn default_studies_limit() -> usize {
    DEFAULT_STUDIES_LIMIT
}
fn default_alerts_limit() -> usize {
    DEFAULT_SAVED_ALERTS
}
fn default_drawings_limit() -> usize {
    DEFAULT_DRAWINGS_PER_SYMBOL
}

impl Default for UsageLimits {
    fn default() -> Self {
        Self {
            studies_per_chart: default_studies_limit(),
            alerts: default_alerts_limit(),
            drawings_per_symbol: default_drawings_limit(),
        }
    }
}

impl UsageLimits {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.studies_per_chart = self.studies_per_chart.clamp(1, MAX_STUDIES);
        self.alerts = self.alerts.clamp(1, MAX_SAVED_ALERTS);
        self.drawings_per_symbol = self.drawings_per_symbol.clamp(1, MAX_DRAWINGS_PER_SYMBOL);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Preferences;

    #[test]
    fn usage_limits_load_from_old_files_and_stay_in_supported_ranges() {
        let old: Preferences = toml::from_str("magnet = true").unwrap();
        assert_eq!(old.normalized().limits, UsageLimits::default());
        let prefs = Preferences {
            limits: UsageLimits {
                studies_per_chart: 0,
                alerts: usize::MAX,
                drawings_per_symbol: 3_000,
            },
            ..Preferences::default()
        };
        let prefs = prefs.normalized();
        assert_eq!(prefs.limits.studies_per_chart, 1);
        assert_eq!(prefs.limits.alerts, MAX_SAVED_ALERTS);
        assert_eq!(prefs.limits.drawings_per_symbol, 3_000);
        let saved = toml::to_string(&prefs).unwrap();
        let loaded: Preferences = toml::from_str(&saved).unwrap();
        assert_eq!(loaded.normalized().limits, prefs.limits);
    }
}
