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
use super::chart::{ChartKind, GROUPS, QUICK, Timeframe, Zone};
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
    pub interval: bool,
    pub crosshair: bool,
    pub time: bool,
    pub range: bool,
}

impl Default for LinksPref {
    fn default() -> Self {
        Self {
            interval: false,
            crosshair: true,
            time: true,
            range: false,
        }
    }
}

/// One chart of the layout, by the stable codes written in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChartPref {
    #[serde(default = "default_timeframe_code")]
    pub timeframe: String,
    #[serde(default = "default_kind_code")]
    pub kind: String,
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
        }
    }
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
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_favorite_timeframes() -> Vec<String> {
    QUICK.iter().map(|timeframe| timeframe.code()).collect()
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
            layout: LayoutPref::default(),
            links: LinksPref::default(),
            charts: default_charts(),
            active_chart: 0,
            magnet: false,
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

        // The favorites, in the order of the menu, without repeats or unknown codes.
        let wanted: Vec<Timeframe> = self
            .favorite_timeframes
            .iter()
            .filter_map(|code| Timeframe::from_code(code))
            .collect();
        let mut favorites: Vec<Timeframe> = GROUPS
            .iter()
            .flat_map(|(_, items)| items.iter().copied())
            .filter(|timeframe| wanted.contains(timeframe))
            .collect();
        if favorites.is_empty() {
            favorites = QUICK.to_vec();
        }
        self.favorite_timeframes = favorites.iter().map(|t| t.code()).collect();

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
                kind: default_kind_code(),
            });
        }
        self.active_chart = self.active_chart.min(count - 1);
        self
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

    /// The timeframe and type of chart `index`, as the file says (repaired on load).
    pub fn chart(&self, index: usize) -> (Timeframe, ChartKind) {
        let pref = self.charts.get(index).cloned().unwrap_or_default();
        (
            Timeframe::from_code(&pref.timeframe).unwrap_or(Timeframe::DEFAULT),
            ChartKind::from_code(&pref.kind).unwrap_or(ChartKind::Candles),
        )
    }

    /// Records the layout and one `(timeframe, type)` per chart, and which chart is active.
    pub fn set_arrangement(
        &mut self,
        key: LayoutKey,
        charts: &[(Timeframe, ChartKind)],
        active: usize,
    ) {
        self.layout = LayoutPref {
            count: key.count,
            variant: key.variant,
        };
        self.charts = charts
            .iter()
            .map(|(timeframe, kind)| ChartPref {
                timeframe: timeframe.code(),
                kind: kind.code().to_owned(),
            })
            .collect();
        self.active_chart = active;
        *self = std::mem::take(self).normalized();
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
        prefs.set_arrangement(
            LayoutKey {
                count: 4,
                variant: 0,
            },
            &[
                (Timeframe::Bars(Period::M5), ChartKind::Candles),
                (Timeframe::Seconds(15), ChartKind::HeikinAshi),
                (Timeframe::Ticks, ChartKind::Line),
                (Timeframe::Bars(Period::D1), ChartKind::Hollow),
            ],
            2,
        );
        let text = toml::to_string_pretty(&prefs).unwrap();
        let back: Preferences = toml::from_str(&text).unwrap();
        assert_eq!(back.clone().normalized(), prefs);
        assert_eq!(
            back.chart(1),
            (Timeframe::Seconds(15), ChartKind::HeikinAshi)
        );
        assert_eq!(back.chart(2), (Timeframe::Ticks, ChartKind::Line));
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
            timeframe = "M99"
            kind = "renko"
        "#;
        let prefs: Preferences = toml::from_str(text).unwrap();
        let prefs = prefs.normalized();
        assert_eq!(prefs.favorite_timeframes, vec!["M5", "H1"]);
        assert_eq!(prefs.layout, LayoutPref::default(), "a layout that is gone");
        assert_eq!(prefs.charts.len(), 1);
        assert_eq!(prefs.chart(0), (Timeframe::DEFAULT, ChartKind::Candles));
        assert_eq!(prefs.active_chart, 0);
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
}
