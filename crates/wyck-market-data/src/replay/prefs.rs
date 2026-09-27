//! Persisted defaults for a new Replay session.

use serde::{Deserialize, Serialize};

/// The speed presets a Replay control bar offers; also the valid range for
/// [`ReplayPrefs::default_speed`].
pub const SPEED_PRESETS: [f64; 6] = [0.25, 0.5, 1.0, 2.0, 5.0, 10.0];

/// Replay defaults, saved with the rest of the app's preferences.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReplayPrefs {
    /// The speed a new replay starts at.
    #[serde(default = "default_speed")]
    pub default_speed: f64,
}

fn default_speed() -> f64 {
    1.0
}

impl Default for ReplayPrefs {
    fn default() -> Self {
        Self {
            default_speed: default_speed(),
        }
    }
}

impl ReplayPrefs {
    /// The preferences repaired: an unrecognized speed (e.g. from an older or newer
    /// version of the app) falls back to the default rather than being rejected outright.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if !SPEED_PRESETS
            .iter()
            .any(|preset| (*preset - self.default_speed).abs() < 0.001)
        {
            self.default_speed = default_speed();
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_speed_is_one_of_the_presets() {
        assert!(SPEED_PRESETS.contains(&ReplayPrefs::default().default_speed));
    }

    #[test]
    fn an_unrecognized_speed_is_repaired_to_the_default() {
        let prefs = ReplayPrefs { default_speed: 3.7 }.normalized();
        assert_eq!(prefs.default_speed, default_speed());
    }

    #[test]
    fn a_recognized_speed_survives_normalization() {
        let prefs = ReplayPrefs { default_speed: 5.0 }.normalized();
        assert_eq!(prefs.default_speed, 5.0);
    }
}
