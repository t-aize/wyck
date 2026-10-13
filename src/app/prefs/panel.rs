//! What the user chooses about the account panel, kept between runs: the tab that was open.
//!
//! The panel has no other setting. Its tables always show the same columns, in the same order.

use serde::{Deserialize, Serialize};

/// A tab of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tab {
    #[default]
    Positions,
    Orders,
    History,
    Exposure,
    Alerts,
    /// The times alerts fired (`History` is the closed trades).
    AlertLog,
}

impl Tab {
    /// Every tab, in the order they show.
    pub const ALL: [Self; 6] = [
        Self::Positions,
        Self::Orders,
        Self::History,
        Self::Exposure,
        Self::Alerts,
        Self::AlertLog,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Positions => "Positions",
            Self::Orders => "Orders",
            Self::History => "History",
            Self::Exposure => "Exposure",
            Self::Alerts => "Alerts",
            Self::AlertLog => "Alert log",
        }
    }
}

/// Everything about the account panel that is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelPrefs {
    /// The tab that was open.
    pub tab: Tab,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_from_before_the_panel_lost_its_settings_still_loads() {
        let old: PanelPrefs =
            toml::from_str("tab = \"history\"\nzebra = true\ndensity = \"compact\"\n").unwrap();
        assert_eq!(old.tab, Tab::History);
        assert_eq!(
            toml::from_str::<PanelPrefs>("").unwrap(),
            PanelPrefs::default()
        );
    }

    #[test]
    fn the_open_tab_survives_a_round_trip() {
        let prefs = PanelPrefs { tab: Tab::Alerts };
        let text = toml::to_string(&prefs).unwrap();
        assert_eq!(toml::from_str::<PanelPrefs>(&text).unwrap(), prefs);
    }
}
