//! The preferences document: what the user set and wants back, shared by every account.
//!
//! This file holds the shape of the document and the repair that runs when it is read. Each
//! domain of settings keeps its own rules in a module of its own:
//!
//! - `charts`: the layout, what each chart keeps, the links between the charts;
//! - `timeframes`: the favorite timeframes and the ones the user added;
//! - `tools`: the bar of favorite drawing tools;
//! - `limits`: how many objects the user lets a chart, a symbol and the alerts hold;
//! - `watchlists`: the document kept per account, next to this one.

mod charts;
mod limits;
mod timeframes;
mod tools;
pub(crate) mod watchlists;

use self::charts::default_charts;
pub(crate) use self::charts::{ChartPref, LayoutPref};
pub use self::charts::{ChartState, LinksPref, NEW_CHART_TIMEFRAMES};
pub use self::limits::UsageLimits;
use self::timeframes::default_favorite_timeframes;
pub use self::tools::MAX_FAVORITE_TOOLS;
use self::tools::default_favorite_tools;
use crate::prefs::panel::PanelPrefs;
use crate::prefs::ticket::TicketPrefs;
use crate::shortcuts::ShortcutPrefs;
use serde::{Deserialize, Serialize};
use wyck_domain::chart::ChartLook;
use wyck_domain::trading::guard::RiskPrefs;

/// The layout of the preferences document. Version 2: the account panel keeps only its open tab.
/// Version 3: how the charts look is saved once (`chart`), not in each chart.
/// Version 4: the bar of favorite tools can be moved (`favorites_place`).
pub(crate) const SCHEMA_VERSION: u32 = 4;

#[expect(
    clippy::struct_excessive_bools,
    reason = "saved document: each switch of the preferences is its own named key in the file"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "schema_version")]
    pub(crate) schema_version: u32,
    #[serde(default)]
    pub limits: UsageLimits,
    /// How every chart looks: the grid, the crosshair, the colors, the legend, the time zone.
    #[serde(default)]
    pub chart: ChartLook,
    /// The timeframes shown as buttons in the header (all of them stay in the menu).
    #[serde(default = "default_favorite_timeframes")]
    pub favorite_timeframes: Vec<String>,
    /// Timeframes the user added to the menu (`M7`, `H2`...), by code.
    #[serde(default)]
    pub(crate) custom_timeframes: Vec<String>,
    #[serde(default)]
    pub(crate) layout: LayoutPref,
    #[serde(default)]
    pub links: LinksPref,
    #[serde(default = "default_charts")]
    pub(crate) charts: Vec<ChartPref>,
    #[serde(default)]
    pub active_chart: usize,
    /// Whether drawings snap to the open, high, low and close of the nearest bar.
    #[serde(default)]
    pub magnet: bool,
    /// Whether a drawing tool stays picked once a drawing is finished.
    #[serde(default = "wyck_domain::base::defaults::yes")]
    pub keep_drawing: bool,
    /// How the lines between the charts were dragged, per layout (`"count-variant"`): one list
    /// of weights per split of the layout.
    #[serde(default)]
    pub splits: std::collections::BTreeMap<String, Vec<Vec<f32>>>,
    /// Whether the account panel under the charts is open, and its height.
    #[serde(default = "wyck_domain::base::defaults::yes")]
    pub panel_open: bool,
    #[serde(default = "default_panel_height")]
    pub panel_height: f32,
    /// Whether the order ticket is shown beside the charts.
    #[serde(default)]
    pub ticket_open: bool,
    /// Whether orders are sent without asking for a confirmation first.
    #[serde(default)]
    pub one_click: bool,
    /// How the order ticket sizes orders.
    #[serde(default)]
    pub ticket: TicketPrefs,
    /// How the account panel under the charts is arranged.
    #[serde(default)]
    pub account_panel: PanelPrefs,
    /// The limits that keep the account safe: size, daily loss, kill switch.
    #[serde(default)]
    pub risk: RiskPrefs,
    /// The sound and the desktop notification of an alert that fires.
    #[serde(default)]
    pub alert_output: crate::alerts::sound::Output,
    /// The drawing tools pinned in the bar of favorites, by code, in the order they show.
    #[serde(default = "default_favorite_tools")]
    pub(crate) favorite_tools: Vec<String>,
    /// Whether the bar of favorite tools shows over the charts.
    #[serde(default = "wyck_domain::base::defaults::yes")]
    pub favorites_bar: bool,
    /// Whether the bar writes the name of each tool beside its icon.
    #[serde(default)]
    pub favorites_labels: bool,
    /// How far the bar of favorite tools was moved from its place, right and down, in pixels.
    #[serde(default)]
    pub favorites_place: [f32; 2],
    /// The colors the user saved in the color panel, as `0xRRGGBB`.
    #[serde(default)]
    pub saved_colors: Vec<u32>,
    /// The keyboard shortcuts the user changed (see `wyck_ui::shell::keymap` for the defaults).
    #[serde(default)]
    pub shortcuts: ShortcutPrefs,
}

fn default_panel_height() -> f32 {
    240.0
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            limits: UsageLimits::default(),
            chart: ChartLook::default(),
            favorite_timeframes: default_favorite_timeframes(),
            custom_timeframes: Vec::new(),
            layout: LayoutPref::default(),
            links: LinksPref::default(),
            charts: default_charts(),
            active_chart: 0,
            magnet: false,
            keep_drawing: true,
            splits: std::collections::BTreeMap::new(),
            panel_open: true,
            panel_height: default_panel_height(),
            ticket_open: false,
            one_click: false,
            ticket: TicketPrefs::default(),
            account_panel: PanelPrefs::default(),
            risk: RiskPrefs::default(),
            alert_output: crate::alerts::sound::Output::default(),
            favorite_tools: default_favorite_tools(),
            favorites_bar: true,
            favorites_labels: false,
            favorites_place: [0.0, 0.0],
            saved_colors: Vec::new(),
            shortcuts: ShortcutPrefs::default(),
        }
    }
}

impl Preferences {
    /// The preferences repaired: unknown or repeated values dropped or replaced, the charts
    /// matched to the layout, the active chart inside the layout.
    #[must_use]
    pub(crate) fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        self.limits = self.limits.normalized();
        self.shortcuts = std::mem::take(&mut self.shortcuts).normalized();
        self.normalize_timeframes();
        // What a chart shows is repaired with the look as it is read, then the look itself.
        self.normalize_charts();
        self.chart = std::mem::take(&mut self.chart).normalized();
        for offset in &mut self.favorites_place {
            if !offset.is_finite() {
                *offset = 0.0;
            }
        }
        if !(self.panel_height.is_finite() && self.panel_height >= 80.0) {
            self.panel_height = default_panel_height();
        }
        self.panel_height = self.panel_height.min(2_000.0);
        self.ticket = self.ticket.normalized();
        self.risk = self.risk.normalized();
        self.alert_output = self.alert_output.normalized();
        self.normalize_favorite_tools();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_gives_the_defaults() {
        let prefs: Preferences = toml::from_str("").unwrap();
        assert_eq!(prefs.normalized(), Preferences::default().normalized());
    }

    #[test]
    fn safety_limits_and_exits_load_from_old_files_and_round_trip() {
        let old: Preferences = toml::from_str("magnet = true").unwrap();
        assert!(!old.ticket.exits.on);
        assert_eq!(old.normalized().risk, RiskPrefs::default());
        let mut prefs = Preferences::default();
        prefs.risk.max_daily_loss = 250.0;
        prefs.risk.kill_switch = true;
        prefs.ticket.exits.on = true;
        prefs.ticket.exits.oco_pips = 12.5;
        let text = toml::to_string(&prefs).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        let back = back.normalized();
        assert_eq!(back.risk, prefs.risk);
        assert_eq!(back.ticket.exits, prefs.ticket.exits);
    }

    #[test]
    fn a_file_from_before_replay_was_removed_still_loads() {
        let old: Preferences =
            toml::from_str("magnet = true\n\n[replay]\ndefault_speed = 2.0\n").unwrap();
        assert!(old.magnet);
    }
}
