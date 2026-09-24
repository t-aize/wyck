//! What the user arranges and wants back at the next start: the layout of the charts and their
//! timeframes and types, the timeframes they favor, the time zone, and (for the account in use)
//! favorite symbols and watchlists.
//!
//! The data lives in TOML documents managed by [`wyck::config::DocumentStore`]; this module owns
//! their shape. Two rules keep old files working: every field has a default, so a file from an
//! older version (or a hand-edited one) still loads, and everything read is checked and repaired
//! by [`Preferences::normalized`], so a value this version does not know (a timeframe that was
//! removed, a layout that no longer exists) becomes a sane default instead of an error.
//!
//! - `preferences` is global, shared by every account.
//! - `watchlists` is per account, because symbols are named by the broker behind it.
//!
//! [`Workspace`] is the live copy, a gpui entity: views read it, change it through
//! [`Workspace::edit_preferences`] and [`Workspace::edit_watchlists`], and are told when it
//! changed. Each change is saved a moment later (see [`saver`]) and once more on exit.

mod saver;

use serde::{Deserialize, Serialize};
use wyck::config::DocumentStore;

pub use self::saver::Saver;
use super::chart::drawing::model::Tool;
use super::chart::{ChartKind, ChartSettings, QUICK, Timeframe, Zone};
use super::multichart::layouts::{self, LayoutKey};

const SCHEMA_VERSION: u32 = 1;
const PREFERENCES: &str = "preferences";
const WATCHLISTS: &str = "watchlists";

/// The most characters of a watchlist name.
pub const MAX_LIST_NAME: usize = 40;

/// The two places documents are kept.
#[derive(Clone)]
pub struct Documents {
    /// Shared by every account.
    pub global: DocumentStore,
    /// Private to the account in use.
    pub account: DocumentStore,
}

// ---- preferences ----

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
    #[serde(default = "yes")]
    pub symbol: bool,
    #[serde(default)]
    pub interval: bool,
    #[serde(default = "yes")]
    pub crosshair: bool,
    #[serde(default = "yes")]
    pub time: bool,
    #[serde(default)]
    pub range: bool,
}

fn yes() -> bool {
    true
}

impl Default for LinksPref {
    fn default() -> Self {
        Self {
            symbol: true,
            interval: false,
            crosshair: true,
            time: true,
            range: false,
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
    #[serde(default = "yes")]
    pub keep_drawing: bool,
    /// How the lines between the charts were dragged, per layout (`"count-variant"`): one list
    /// of weights per split of the layout.
    #[serde(default)]
    pub splits: std::collections::BTreeMap<String, Vec<Vec<f32>>>,
    /// Whether the account panel under the charts is open, and its height.
    #[serde(default = "yes")]
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
    pub ticket: crate::app::trading::ticket::TicketPrefs,
    /// The drawing tools pinned in the bar of favorites, by code, in the order they show.
    #[serde(default = "default_favorite_tools")]
    pub favorite_tools: Vec<String>,
    /// Whether the bar of favorite tools shows over the charts.
    #[serde(default = "yes")]
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
            ticket: crate::app::trading::ticket::TicketPrefs::default(),
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

// ---- watchlists ----

/// A named list of symbols, by the names the broker gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchlist {
    pub name: String,
    #[serde(default)]
    pub symbols: Vec<String>,
}

/// Why a watchlist name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    Taken,
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Give the list a name"),
            Self::TooLong => write!(f, "A name has at most {MAX_LIST_NAME} characters"),
            Self::Taken => f.write_str("A list with this name already exists"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchlists {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub lists: Vec<Watchlist>,
}

impl Default for Watchlists {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            favorites: Vec::new(),
            lists: Vec::new(),
        }
    }
}

impl Watchlists {
    /// The lists repaired: repeated symbols and lists with a repeated or empty name dropped.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        dedup_keep_order(&mut self.favorites);
        let mut seen: Vec<String> = Vec::new();
        self.lists.retain_mut(|list| {
            list.name = list.name.trim().to_owned();
            let key = list.name.to_lowercase();
            if list.name.is_empty() || seen.contains(&key) {
                return false;
            }
            seen.push(key);
            dedup_keep_order(&mut list.symbols);
            true
        });
        self
    }

    pub fn is_favorite(&self, symbol: &str) -> bool {
        self.favorites.iter().any(|s| s == symbol)
    }

    pub fn toggle_favorite(&mut self, symbol: &str) {
        if let Some(at) = self.favorites.iter().position(|s| s == symbol) {
            self.favorites.remove(at);
        } else {
            self.favorites.push(symbol.to_owned());
        }
    }

    fn check_name(&self, name: &str, except: Option<usize>) -> Result<String, NameError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if name.chars().count() > MAX_LIST_NAME {
            return Err(NameError::TooLong);
        }
        let key = name.to_lowercase();
        let taken = self
            .lists
            .iter()
            .enumerate()
            .any(|(i, list)| Some(i) != except && list.name.to_lowercase() == key);
        if taken {
            return Err(NameError::Taken);
        }
        Ok(name.to_owned())
    }

    /// Adds an empty list and returns its position.
    pub fn create_list(&mut self, name: &str) -> Result<usize, NameError> {
        let name = self.check_name(name, None)?;
        self.lists.push(Watchlist {
            name,
            symbols: Vec::new(),
        });
        Ok(self.lists.len() - 1)
    }

    pub fn rename_list(&mut self, index: usize, name: &str) -> Result<(), NameError> {
        let name = self.check_name(name, Some(index))?;
        if let Some(list) = self.lists.get_mut(index) {
            list.name = name;
        }
        Ok(())
    }

    pub fn delete_list(&mut self, index: usize) {
        if index < self.lists.len() {
            self.lists.remove(index);
        }
    }

    pub fn in_list(&self, index: usize, symbol: &str) -> bool {
        self.lists
            .get(index)
            .is_some_and(|list| list.symbols.iter().any(|s| s == symbol))
    }

    /// Puts the symbol in the list, or takes it out if it is already there.
    pub fn toggle_in_list(&mut self, index: usize, symbol: &str) {
        let Some(list) = self.lists.get_mut(index) else {
            return;
        };
        if let Some(at) = list.symbols.iter().position(|s| s == symbol) {
            list.symbols.remove(at);
        } else {
            list.symbols.push(symbol.to_owned());
        }
    }
}

fn dedup_keep_order(items: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

// ---- the live copy ----

pub struct Workspace {
    preferences: Preferences,
    watchlists: Watchlists,
    preferences_saver: Saver<Preferences>,
    watchlists_saver: Saver<Watchlists>,
}

impl Workspace {
    /// Reads both documents (repairing them), and arranges for the last changes to be written
    /// when the app quits.
    pub fn new(documents: &Documents, cx: &mut gpui::Context<Self>) -> Self {
        let preferences = documents
            .global
            .load_or_default::<Preferences>(PREFERENCES)
            .normalized();
        let watchlists = documents
            .account
            .load_or_default::<Watchlists>(WATCHLISTS)
            .normalized();
        cx.on_app_quit(|this, _cx| {
            this.preferences_saver.flush();
            this.watchlists_saver.flush();
            async {}
        })
        .detach();
        Self {
            preferences,
            watchlists,
            preferences_saver: Saver::new(documents.global.clone(), PREFERENCES),
            watchlists_saver: Saver::new(documents.account.clone(), WATCHLISTS),
        }
    }

    /// Writes what waits to be saved now, for what is about to read the files (a backup).
    pub fn flush(&self) {
        self.preferences_saver.flush();
        self.watchlists_saver.flush();
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn watchlists(&self) -> &Watchlists {
        &self.watchlists
    }

    /// Changes the preferences, tells the views, and saves. Nothing happens if `change` leaves
    /// them as they were.
    pub fn edit_preferences<R>(
        &mut self,
        cx: &mut gpui::Context<Self>,
        change: impl FnOnce(&mut Preferences) -> R,
    ) -> R {
        let before = self.preferences.clone();
        let result = change(&mut self.preferences);
        if self.preferences != before {
            self.preferences_saver.schedule(self.preferences.clone());
            cx.notify();
        }
        result
    }

    /// Changes the watchlists, tells the views, and saves.
    pub fn edit_watchlists<R>(
        &mut self,
        cx: &mut gpui::Context<Self>,
        change: impl FnOnce(&mut Watchlists) -> R,
    ) -> R {
        let before = self.watchlists.clone();
        let result = change(&mut self.watchlists);
        if self.watchlists != before {
            self.watchlists_saver.schedule(self.watchlists.clone());
            cx.notify();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck::openapi::market::Period;

    #[test]
    fn an_empty_file_gives_the_defaults() {
        let prefs: Preferences = toml::from_str("").unwrap();
        assert_eq!(prefs.normalized(), Preferences::default().normalized());
    }

    #[test]
    fn a_saved_arrangement_is_read_back_exactly() {
        let mut prefs = Preferences {
            zone: Zone::Utc,
            magnet: true,
            ..Preferences::default()
        };
        prefs.links.interval = true;
        let state = |timeframe, kind, symbol: Option<&str>| ChartState {
            timeframe,
            settings: ChartSettings {
                kind,
                ..ChartSettings::default()
            },
            symbol: symbol.map(str::to_owned),
        };
        prefs.set_arrangement(
            LayoutKey {
                count: 4,
                variant: 0,
            },
            &[
                state(Timeframe::Bars(Period::M5), ChartKind::Candles, None),
                state(
                    Timeframe::Seconds(15),
                    ChartKind::HeikinAshi,
                    Some("XAUUSD"),
                ),
                state(Timeframe::Ticks, ChartKind::Line, None),
                state(Timeframe::Bars(Period::D1), ChartKind::Renko, None),
            ],
            2,
        );
        let text = toml::to_string_pretty(&prefs).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        assert_eq!(back.clone().normalized(), prefs);
        let second = back.chart(1);
        assert_eq!(second.timeframe, Timeframe::Seconds(15));
        assert_eq!(second.settings.kind, ChartKind::HeikinAshi);
        assert_eq!(second.symbol.as_deref(), Some("XAUUSD"));
        assert_eq!(back.chart(3).settings.kind, ChartKind::Renko);
        assert_eq!(back.active_chart, 2);
    }

    #[test]
    fn unknown_values_are_repaired_not_refused() {
        let text = r#"
            favorite_timeframes = ["M5", "M5", "X9", "H1"]
            active_chart = 40
            [layout]
            count = 3
            variant = 99
            [[charts]]
            timeframe = "M9999"
            kind = "spiral"
        "#;
        let prefs: Preferences = toml::from_str(text).unwrap();
        let prefs = prefs.normalized();
        assert_eq!(prefs.favorite_timeframes, vec!["M5", "H1"]);
        assert_eq!(prefs.layout, LayoutPref::default(), "a layout that is gone");
        assert_eq!(prefs.charts.len(), 1);
        assert_eq!(prefs.chart(0).timeframe, Timeframe::DEFAULT);
        assert_eq!(prefs.chart(0).settings.kind, ChartKind::Candles);
        assert_eq!(prefs.active_chart, 0);
    }

    #[test]
    fn a_file_from_before_chart_settings_keeps_its_types_and_links() {
        let text = r#"
            [links]
            interval = true
            crosshair = false
            time = false
            range = true
            [[charts]]
            timeframe = "H1"
            kind = "hollow"
        "#;
        let prefs: Preferences = toml::from_str(text).unwrap();
        let prefs = prefs.normalized();
        assert!(prefs.links.symbol, "the symbol link is new and on");
        assert!(prefs.links.interval && prefs.links.range && !prefs.links.time);
        assert_eq!(prefs.chart(0).settings.kind, ChartKind::Hollow);
        assert!(prefs.panel_open);
    }

    #[test]
    fn the_charts_always_match_the_layout() {
        let prefs = Preferences {
            layout: LayoutPref {
                count: 6,
                variant: 0,
            },
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(prefs.charts.len(), 6);
        // Added charts do not all start on the same timeframe.
        assert_ne!(prefs.charts[1].timeframe, prefs.charts[2].timeframe);

        let mut prefs = prefs;
        prefs.layout = LayoutPref {
            count: 2,
            variant: 0,
        };
        prefs.active_chart = 5;
        let prefs = prefs.normalized();
        assert_eq!(prefs.charts.len(), 2);
        assert_eq!(prefs.active_chart, 1);
    }

    #[test]
    fn favorite_timeframes_keep_menu_order_and_never_run_out() {
        let mut prefs = Preferences {
            favorite_timeframes: vec!["D1".into(), "M1".into()],
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(prefs.favorite_timeframes, vec!["M1", "D1"]);

        prefs.toggle_favorite_timeframe(Timeframe::Seconds(5));
        assert_eq!(prefs.favorite_timeframes, vec!["S5", "M1", "D1"]);
        prefs.toggle_favorite_timeframe(Timeframe::Bars(Period::M1));
        prefs.toggle_favorite_timeframe(Timeframe::Seconds(5));
        assert_eq!(prefs.favorite_timeframes, vec!["D1"]);
        prefs.toggle_favorite_timeframe(Timeframe::Bars(Period::D1));
        assert_eq!(
            prefs.favorite_timeframes,
            vec!["D1"],
            "the last one stays, so the header keeps a button"
        );
        assert!(prefs.is_favorite_timeframe(Timeframe::Bars(Period::D1)));
    }

    #[test]
    fn custom_timeframes_are_kept_once_and_can_be_starred() {
        let mut prefs = Preferences::default().normalized();
        let h2 = Timeframe::from_code("H2").unwrap();
        let m7 = Timeframe::from_code("M7").unwrap();
        prefs.add_custom_timeframe(m7);
        prefs.add_custom_timeframe(Timeframe::from_code("120m").unwrap());
        prefs.add_custom_timeframe(h2);
        // H2 is offered by the menu already; M7 is the one added.
        assert_eq!(prefs.custom_timeframes, vec!["M7"]);
        prefs.add_custom_timeframe(Timeframe::from_code("M9").unwrap());
        assert_eq!(prefs.custom_timeframes, vec!["M7", "M9"]);
        prefs.toggle_favorite_timeframe(m7);
        assert!(prefs.favorite_timeframes.contains(&"M7".to_owned()));
        // Saved and read back, it keeps its place in the header and the menu.
        let text = toml::to_string(&prefs).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        let back = back.normalized();
        assert_eq!(
            back.customs(),
            vec![m7, Timeframe::from_code("M9").unwrap()]
        );
        assert!(back.favorites().contains(&m7));
        // Taking it out of the menu takes it off the header too.
        prefs.remove_custom_timeframe(m7);
        assert_eq!(prefs.custom_timeframes, vec!["M9"]);
        assert!(!prefs.is_favorite_timeframe(m7));
        // A favorite from nowhere (a file written by hand) is added to the menu.
        let prefs = Preferences {
            favorite_timeframes: vec!["M1".into(), "M13".into()],
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(prefs.custom_timeframes, vec!["M13"]);
    }

    #[test]
    fn favorites_of_nothing_fall_back_to_the_defaults() {
        let prefs = Preferences {
            favorite_timeframes: vec![],
            ..Preferences::default()
        };
        assert_eq!(prefs.normalized().favorites(), QUICK.to_vec());
    }

    #[test]
    fn watchlist_names_are_checked() {
        let mut lists = Watchlists::default();
        assert_eq!(lists.create_list("  Majors "), Ok(0));
        assert_eq!(lists.lists[0].name, "Majors");
        assert_eq!(lists.create_list("majors"), Err(NameError::Taken));
        assert_eq!(lists.create_list("   "), Err(NameError::Empty));
        assert_eq!(
            lists.create_list(&"x".repeat(MAX_LIST_NAME + 1)),
            Err(NameError::TooLong)
        );
        assert_eq!(lists.create_list("Metals"), Ok(1));
        assert_eq!(lists.rename_list(1, "Majors"), Err(NameError::Taken));
        assert_eq!(
            lists.rename_list(1, "METALS"),
            Ok(()),
            "its own name is fine"
        );
        assert_eq!(lists.rename_list(1, "Gold and silver"), Ok(()));
    }

    #[test]
    fn symbols_move_in_and_out_of_lists_and_favorites() {
        let mut lists = Watchlists::default();
        lists.create_list("A").unwrap();
        lists.toggle_in_list(0, "EURUSD");
        lists.toggle_in_list(0, "XAUUSD");
        assert!(lists.in_list(0, "EURUSD"));
        lists.toggle_in_list(0, "EURUSD");
        assert!(!lists.in_list(0, "EURUSD"));
        assert_eq!(lists.lists[0].symbols, vec!["XAUUSD"]);
        lists.toggle_in_list(7, "X");
        assert!(!lists.in_list(7, "X"));

        assert!(!lists.is_favorite("US100.cash"));
        lists.toggle_favorite("US100.cash");
        assert!(lists.is_favorite("US100.cash"));
        lists.toggle_favorite("US100.cash");
        assert!(!lists.is_favorite("US100.cash"));

        lists.delete_list(0);
        lists.delete_list(0);
        assert!(lists.lists.is_empty());
    }

    #[test]
    fn a_hand_edited_watchlist_file_is_cleaned_up() {
        let text = r#"
            favorites = ["A", "B", "A"]
            [[lists]]
            name = "One"
            symbols = ["X", "X", "Y"]
            [[lists]]
            name = " one "
            [[lists]]
            name = ""
        "#;
        let lists: Watchlists = toml::from_str(text).unwrap();
        let lists = lists.normalized();
        assert_eq!(lists.favorites, vec!["A", "B"]);
        assert_eq!(lists.lists.len(), 1);
        assert_eq!(lists.lists[0].symbols, vec!["X", "Y"]);
    }

    #[test]
    fn watchlists_survive_awkward_symbol_names_through_toml() {
        let mut lists = Watchlists::default();
        lists.create_list("Odd \"names\" & more").unwrap();
        lists.toggle_in_list(0, "US100.cash");
        lists.toggle_in_list(0, "EUR/USD #1");
        let text = toml::to_string_pretty(&lists).unwrap();
        let back: Watchlists = toml::from_str(&text).unwrap();
        assert_eq!(back, lists);
    }

    #[test]
    fn the_favorite_tools_start_with_the_usual_ones_and_old_files_get_them() {
        let fresh = Preferences::default();
        let list = fresh.favorite_tool_list();
        assert_eq!(list[0], Tool::TrendLine);
        assert!(list.contains(&Tool::LongPosition));
        assert!(fresh.favorites_bar && !fresh.favorites_labels);

        // A file from before the bar has none of these keys.
        let old: Preferences = toml::from_str(
            "magnet = true
",
        )
        .unwrap();
        assert_eq!(old.favorite_tool_list(), list);
        assert!(old.favorites_bar);
    }

    #[test]
    fn a_tool_is_pinned_and_unpinned_by_the_same_call() {
        let mut prefs = Preferences::default();
        assert!(
            !prefs.toggle_favorite_tool(Tool::TrendLine),
            "it was pinned: now it is not"
        );
        assert!(!prefs.favorite_tool_list().contains(&Tool::TrendLine));
        assert!(prefs.toggle_favorite_tool(Tool::Ellipse));
        assert_eq!(prefs.favorite_tool_list().last(), Some(&Tool::Ellipse));
        assert!(!prefs.toggle_favorite_tool(Tool::Ellipse));
    }

    #[test]
    fn the_bar_holds_a_limited_number_and_an_empty_bar_stays_empty() {
        let mut prefs = Preferences::default();
        prefs.favorite_tools.clear();
        for tool in Tool::ALL.into_iter().filter(|t| *t != Tool::Unknown) {
            prefs.toggle_favorite_tool(tool);
        }
        assert_eq!(prefs.favorite_tools.len(), MAX_FAVORITE_TOOLS);
        let extra = Tool::ALL
            .into_iter()
            .find(|t| !prefs.favorite_tool_list().contains(t))
            .unwrap();
        assert!(
            !prefs.toggle_favorite_tool(extra),
            "a full bar takes no more"
        );
        assert!(!prefs.favorite_tool_list().contains(&extra));

        prefs.favorite_tools.clear();
        let prefs = prefs.normalized();
        assert!(
            prefs.favorite_tools.is_empty(),
            "the choice of none is kept"
        );
    }

    #[test]
    fn normalizing_drops_unknown_and_repeated_favorites_and_keeps_the_order() {
        let prefs = Preferences {
            favorite_tools: vec![
                Tool::Rectangle.code(),
                "no_such_tool".to_owned(),
                Tool::TrendLine.code(),
                Tool::Rectangle.code(),
            ],
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(
            prefs.favorite_tool_list(),
            vec![Tool::Rectangle, Tool::TrendLine]
        );
    }

    #[test]
    fn favorites_move_along_the_bar_and_stop_at_its_ends() {
        let mut prefs = Preferences {
            favorite_tools: vec![Tool::Ray.code(), Tool::Arrow.code(), Tool::Text.code()],
            ..Preferences::default()
        };
        prefs.move_favorite_tool(2, -1);
        assert_eq!(
            prefs.favorite_tool_list(),
            vec![Tool::Ray, Tool::Text, Tool::Arrow]
        );
        prefs.move_favorite_tool(0, -1);
        prefs.move_favorite_tool(2, 1);
        prefs.move_favorite_tool(9, 1);
        assert_eq!(
            prefs.favorite_tool_list(),
            vec![Tool::Ray, Tool::Text, Tool::Arrow]
        );
        prefs.move_favorite_tool(0, 5);
        assert_eq!(prefs.favorite_tool_list().last(), Some(&Tool::Ray));
    }
}
