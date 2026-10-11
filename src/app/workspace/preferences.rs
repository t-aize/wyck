//! Saved preferences and watchlists.

use super::layouts::{self, LayoutKey};
use crate::app::prefs::panel::PanelPrefs;
use crate::app::prefs::ticket::TicketPrefs;
use crate::domain::chart::settings::{DEFAULT_STUDIES_LIMIT, MAX_STUDIES};
use crate::domain::chart::{ChartKind, ChartSettings, QUICK, Timeframe, Zone};
use crate::domain::drawings::model::{DEFAULT_DRAWINGS_PER_SYMBOL, MAX_DRAWINGS_PER_SYMBOL, Tool};
use crate::domain::trading::guard::RiskPrefs;
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

/// Maximum length of a watchlist name.
pub const MAX_LIST_NAME: usize = 40;
pub const MAX_SAVED_ALERTS: usize = 2_000;
pub const DEFAULT_SAVED_ALERTS: usize = 500;

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
    pub fn normalized(mut self) -> Self {
        self.studies_per_chart = self.studies_per_chart.clamp(1, MAX_STUDIES);
        self.alerts = self.alerts.clamp(1, MAX_SAVED_ALERTS);
        self.drawings_per_symbol = self.drawings_per_symbol.clamp(1, MAX_DRAWINGS_PER_SYMBOL);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPref {
    pub count: usize,
    pub variant: usize,
}

impl Default for LayoutPref {
    fn default() -> Self {
        Self {
            count: 1,
            variant: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinksPref {
    /// Every chart shows the same symbol.
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub symbol: bool,
    #[serde(default)]
    pub interval: bool,
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub crosshair: bool,
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub time: bool,
    #[serde(default)]
    pub range: bool,
    #[serde(default)]
    pub studies: bool,
}

impl Default for LinksPref {
    fn default() -> Self {
        Self {
            symbol: true,
            interval: false,
            crosshair: true,
            time: true,
            range: false,
            studies: false,
        }
    }
}

/// One chart of the layout, by the stable codes written in the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPref {
    #[serde(default = "default_timeframe_code")]
    pub timeframe: String,
    /// The chart type, as older files wrote it. [`ChartPref::settings`] wins when present.
    #[serde(default = "default_kind_code")]
    pub kind: String,
    /// The symbol the chart shows when the symbol is not linked, by the broker's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Everything else about the chart: type, scale, zone, indicators.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<ChartSettings>,
}

fn default_timeframe_code() -> String {
    Timeframe::DEFAULT.code()
}

fn default_kind_code() -> String {
    ChartKind::Candles.code().to_owned()
}

impl Default for ChartPref {
    fn default() -> Self {
        Self {
            timeframe: default_timeframe_code(),
            kind: default_kind_code(),
            symbol: None,
            settings: None,
        }
    }
}

impl ChartPref {
    /// The settings of the chart: the saved ones, or the defaults with the saved type.
    pub fn chart_settings(&self) -> ChartSettings {
        match &self.settings {
            Some(settings) => settings.clone().normalized(),
            None => ChartSettings {
                kind: ChartKind::from_code(&self.kind).unwrap_or(ChartKind::Candles),
                ..ChartSettings::default()
            },
        }
    }
}

/// What one chart of a layout keeps: its timeframe, its settings and its own symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartState {
    pub timeframe: Timeframe,
    pub settings: ChartSettings,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub limits: UsageLimits,
    #[serde(default)]
    pub zone: Zone,
    /// The timeframes shown as buttons in the header (all of them stay in the menu).
    #[serde(default = "default_favorite_timeframes")]
    pub favorite_timeframes: Vec<String>,
    /// Timeframes the user added to the menu (`M7`, `H2`...), by code.
    #[serde(default)]
    pub custom_timeframes: Vec<String>,
    #[serde(default)]
    pub layout: LayoutPref,
    #[serde(default)]
    pub links: LinksPref,
    #[serde(default = "default_charts")]
    pub charts: Vec<ChartPref>,
    #[serde(default)]
    pub active_chart: usize,
    /// Whether drawings snap to the open, high, low and close of the nearest bar.
    #[serde(default)]
    pub magnet: bool,
    /// Whether a drawing tool stays picked once a drawing is finished.
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub keep_drawing: bool,
    /// How the lines between the charts were dragged, per layout (`"count-variant"`): one list
    /// of weights per split of the layout.
    #[serde(default)]
    pub splits: std::collections::BTreeMap<String, Vec<Vec<f32>>>,
    /// Whether the account panel under the charts is open, and its height.
    #[serde(default = "crate::domain::chart::defaults::yes")]
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
    pub alert_output: crate::app::alerts::sound::Output,
    /// The drawing tools pinned in the bar of favorites, by code, in the order they show.
    #[serde(default = "default_favorite_tools")]
    pub favorite_tools: Vec<String>,
    /// Whether the bar of favorite tools shows over the charts.
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub favorites_bar: bool,
    /// Whether the bar writes the name of each tool beside its icon.
    #[serde(default)]
    pub favorites_labels: bool,
    /// The colors the user saved in the color panel, as `0xRRGGBB`.
    #[serde(default)]
    pub saved_colors: Vec<u32>,
}

fn default_panel_height() -> f32 {
    240.0
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_favorite_timeframes() -> Vec<String> {
    QUICK.iter().map(|timeframe| timeframe.code()).collect()
}

/// The most tools the bar of favorites holds.
pub const MAX_FAVORITE_TOOLS: usize = 16;

/// The tools pinned the first time: the ones drawn most.
fn default_favorite_tools() -> Vec<String> {
    [
        Tool::TrendLine,
        Tool::HorizontalLine,
        Tool::Rectangle,
        Tool::FibRetracement,
        Tool::LongPosition,
        Tool::ShortPosition,
        Tool::Text,
    ]
    .into_iter()
    .map(Tool::code)
    .collect()
}

fn default_charts() -> Vec<ChartPref> {
    vec![ChartPref::default()]
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            limits: UsageLimits::default(),
            zone: Zone::default(),
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
            alert_output: crate::app::alerts::sound::Output::default(),
            favorite_tools: default_favorite_tools(),
            favorites_bar: true,
            favorites_labels: false,
            saved_colors: Vec::new(),
        }
    }
}

/// Timeframes given to the charts a layout adds, when nothing is saved for them. They differ, so
/// a new chart shows something else than the first one.
pub const NEW_CHART_TIMEFRAMES: [&str; 8] = ["M5", "M15", "H1", "H4", "D1", "M1", "M30", "W1"];

impl Preferences {
    /// The preferences repaired: unknown or repeated values dropped or replaced, the charts
    /// matched to the layout, the active chart inside the layout.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        self.limits = self.limits.normalized();

        // The custom timeframes, each once under its own code, shortest first. A favorite
        // that is neither offered nor added becomes an added one, so its star has a home.
        let mut favorites: Vec<Timeframe> = self
            .favorite_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect();
        let mut customs: Vec<Timeframe> = self
            .custom_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .chain(favorites.iter().copied())
            .filter(|timeframe| !timeframe.is_builtin())
            .collect();
        for list in [&mut favorites, &mut customs] {
            list.sort_by_key(|timeframe| timeframe.sort_key());
            list.dedup();
        }
        if favorites.is_empty() {
            favorites = QUICK.to_vec();
        }
        self.favorite_timeframes = favorites.iter().map(|t| t.code()).collect();
        self.custom_timeframes = customs.iter().map(|t| t.code()).collect();

        // A layout that does not exist becomes the single chart.
        let key = LayoutKey {
            count: self.layout.count,
            variant: self.layout.variant,
        };
        let count = layouts::layout(key).count();
        if count != self.layout.count || !layouts::exists(key) {
            self.layout = LayoutPref::default();
        }
        let count = layouts::layout(self.layout_key()).count();

        // One valid entry per chart of the layout.
        self.charts.truncate(count);
        for chart in &mut self.charts {
            if Timeframe::from_code(&chart.timeframe).is_none() {
                chart.timeframe = default_timeframe_code();
            }
            if ChartKind::from_code(&chart.kind).is_none() {
                chart.kind = default_kind_code();
            }
        }
        while self.charts.len() < count {
            let timeframe = NEW_CHART_TIMEFRAMES[self.charts.len() % NEW_CHART_TIMEFRAMES.len()];
            self.charts.push(ChartPref {
                timeframe: timeframe.to_owned(),
                ..ChartPref::default()
            });
        }
        for chart in &mut self.charts {
            if let Some(settings) = chart.settings.take() {
                chart.kind = settings.kind.code().to_owned();
                chart.settings = Some(settings.normalized());
            }
        }
        self.active_chart = self.active_chart.min(count - 1);
        if !(self.panel_height.is_finite() && self.panel_height >= 80.0) {
            self.panel_height = default_panel_height();
        }
        self.panel_height = self.panel_height.min(2_000.0);
        self.ticket = self.ticket.normalized();
        self.account_panel = self.account_panel.normalized();
        self.risk = self.risk.normalized();
        self.alert_output = self.alert_output.normalized();

        // The favorite tools: known ones, each once, in the order saved, no more than the bar
        // holds. An empty list is kept: it is what the user chose, not a reason for the defaults.
        let mut tools: Vec<Tool> = Vec::new();
        for code in &self.favorite_tools {
            if let Some(tool) = Tool::from_code(code)
                && !tools.contains(&tool)
                && tools.len() < MAX_FAVORITE_TOOLS
            {
                tools.push(tool);
            }
        }
        self.favorite_tools = tools.into_iter().map(Tool::code).collect();
        self
    }

    /// The favorite tools, in the order the bar shows them.
    pub fn favorite_tool_list(&self) -> Vec<Tool> {
        self.favorite_tools
            .iter()
            .filter_map(|code| Tool::from_code(code))
            .collect()
    }

    /// Pins the tool, or unpins it when it is pinned. Returns whether it is a favorite now. A full
    /// bar takes no more, and the tool is not pinned.
    pub fn toggle_favorite_tool(&mut self, tool: Tool) -> bool {
        let code = tool.code();
        if let Some(at) = self.favorite_tools.iter().position(|c| *c == code) {
            self.favorite_tools.remove(at);
            return false;
        }
        if self.favorite_tools.len() >= MAX_FAVORITE_TOOLS {
            return false;
        }
        self.favorite_tools.push(code);
        true
    }

    /// Moves the favorite at `index` by `delta` places (negative is towards the start), stopping
    /// at the ends.
    pub fn move_favorite_tool(&mut self, index: usize, delta: isize) {
        let len = self.favorite_tools.len();
        if index >= len {
            return;
        }
        let target = index
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        if target != index {
            let code = self.favorite_tools.remove(index);
            self.favorite_tools.insert(target, code);
        }
    }

    pub fn layout_key(&self) -> LayoutKey {
        LayoutKey {
            count: self.layout.count,
            variant: self.layout.variant,
        }
    }

    /// The favorite timeframes, in menu order.
    pub fn favorites(&self) -> Vec<Timeframe> {
        self.favorite_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect()
    }

    pub fn is_favorite_timeframe(&self, timeframe: Timeframe) -> bool {
        self.favorite_timeframes.contains(&timeframe.code())
    }

    /// Adds the timeframe to the favorites, or removes it. The last favorite cannot be removed:
    /// the header would be left with no buttons at all.
    pub fn toggle_favorite_timeframe(&mut self, timeframe: Timeframe) {
        let code = timeframe.code();
        if let Some(at) = self.favorite_timeframes.iter().position(|c| *c == code) {
            if self.favorite_timeframes.len() > 1 {
                self.favorite_timeframes.remove(at);
            }
        } else {
            self.favorite_timeframes.push(code);
            *self = std::mem::take(self).normalized();
        }
    }

    /// The timeframes the user added, shortest first.
    pub fn customs(&self) -> Vec<Timeframe> {
        self.custom_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect()
    }

    /// Adds a timeframe to the menu, unless it is there already.
    pub fn add_custom_timeframe(&mut self, timeframe: Timeframe) {
        if timeframe.is_builtin() || self.customs().contains(&timeframe) {
            return;
        }
        self.custom_timeframes.push(timeframe.code());
        *self = std::mem::take(self).normalized();
    }

    /// Takes an added timeframe out of the menu, and off the header. The last favorite stays,
    /// as with [`Self::toggle_favorite_timeframe`].
    pub fn remove_custom_timeframe(&mut self, timeframe: Timeframe) {
        let code = timeframe.code();
        if self.is_favorite_timeframe(timeframe) {
            if self.favorite_timeframes.len() == 1 {
                return;
            }
            self.favorite_timeframes.retain(|c| *c != code);
        }
        self.custom_timeframes.retain(|c| *c != code);
    }

    /// What chart `index` keeps, as the file says (repaired on load).
    pub fn chart(&self, index: usize) -> ChartState {
        let pref = self.charts.get(index).cloned().unwrap_or_default();
        let mut settings = pref.chart_settings();
        if pref.settings.is_none() {
            // Files from before per-chart zones had one zone for every chart.
            settings.zone = self.zone;
        }
        ChartState {
            timeframe: Timeframe::from_code(&pref.timeframe).unwrap_or(Timeframe::DEFAULT),
            settings,
            symbol: pref.symbol.clone(),
        }
    }

    /// Records the layout and what each chart keeps, and which chart is active.
    pub fn set_arrangement(&mut self, key: LayoutKey, charts: &[ChartState], active: usize) {
        self.layout = LayoutPref {
            count: key.count,
            variant: key.variant,
        };
        self.charts = charts
            .iter()
            .map(|chart| ChartPref {
                timeframe: chart.timeframe.code(),
                kind: chart.settings.kind.code().to_owned(),
                symbol: chart.symbol.clone(),
                settings: Some(chart.settings.clone()),
            })
            .collect();
        self.active_chart = active;
        *self = std::mem::take(self).normalized();
    }

    /// The key a layout's split weights are saved under.
    pub fn split_key(key: LayoutKey) -> String {
        format!("{}-{}", key.count, key.variant)
    }
}

mod watchlists;

pub use watchlists::{NameError, Watchlist, Watchlists};

#[cfg(test)]
#[path = "../../../tests/unit/app/workspace/preferences.rs"]
mod tests;
