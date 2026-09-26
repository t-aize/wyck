//! What the user chooses about the account panel, kept between runs: which tabs show and in what
//! order, the columns of each table, the figures of the header, how the rows look, what is
//! filtered, what a click does, and which buttons a row carries.
//!
//! Everything has a default, and everything read from a file is put back in range by
//! [`PanelPrefs::normalized`].

use serde::{Deserialize, Serialize};

use super::columns::{AlertCol, DealCol, ExposureCol, OrderCol, PositionCol, TablePrefs};
use crate::ticket::prefs::{Placed, Slot, default_list, mend};

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
}

impl Slot for Tab {
    const ALL: &'static [Self] = &[
        Self::Positions,
        Self::Orders,
        Self::History,
        Self::Exposure,
        Self::Alerts,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Positions => "Positions",
            Self::Orders => "Orders",
            Self::History => "History",
            Self::Exposure => "Exposure",
            Self::Alerts => "Alerts",
        }
    }
}

/// A figure of the account, in the header of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
    Balance,
    Equity,
    Margin,
    FreeMargin,
    MarginLevel,
    Profit,
    ProfitPercent,
}

impl Slot for Stat {
    const ALL: &'static [Self] = &[
        Self::Balance,
        Self::Margin,
        Self::MarginLevel,
        Self::FreeMargin,
        Self::Equity,
        Self::Profit,
        Self::ProfitPercent,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Balance => "Balance",
            Self::Equity => "Equity",
            Self::Margin => "Margin",
            Self::FreeMargin => "Free margin",
            Self::MarginLevel => "Margin level",
            Self::Profit => "Open profit",
            Self::ProfitPercent => "Open profit in percent",
        }
    }

    fn shown_by_default(self) -> bool {
        self != Self::ProfitPercent
    }
}

/// A button a row of positions or orders can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    /// Change the stop loss and take profit (and the price of an order).
    Edit,
    /// Close half of the position.
    Half,
    /// Move the stop loss to the entry.
    BreakEven,
    /// Turn the trailing of the stop loss on or off.
    Trail,
    /// Close and open the other way.
    Reverse,
    /// Close the position, or cancel the order.
    Close,
}

impl Slot for RowAction {
    const ALL: &'static [Self] = &[
        Self::Edit,
        Self::Half,
        Self::BreakEven,
        Self::Trail,
        Self::Reverse,
        Self::Close,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Edit => "Modify (stop loss, take profit, price)",
            Self::Half => "Close half",
            Self::BreakEven => "Move the stop loss to the entry",
            Self::Trail => "Trail the stop loss",
            Self::Reverse => "Reverse",
            Self::Close => "Close or cancel",
        }
    }

    fn shown_by_default(self) -> bool {
        matches!(self, Self::Edit | Self::Reverse | Self::Close)
    }
}

/// How much room a row takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowDensity {
    Compact,
    #[default]
    Comfortable,
    Roomy,
}

impl RowDensity {
    pub const ALL: [Self; 3] = [Self::Compact, Self::Comfortable, Self::Roomy];

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Comfortable => "Comfortable",
            Self::Roomy => "Roomy",
        }
    }

    /// The height of a row, in pixels.
    pub fn height(self) -> f32 {
        match self {
            Self::Compact => 24.0,
            Self::Comfortable => 30.0,
            Self::Roomy => 38.0,
        }
    }

    /// The size of the text, in pixels.
    pub fn text(self) -> f32 {
        match self {
            Self::Compact => 11.0,
            Self::Comfortable | Self::Roomy => 12.0,
        }
    }
}

/// How the profit of a position is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfitUnit {
    /// The amount, and the pips it comes to.
    #[default]
    MoneyPips,
    Money,
    Pips,
    /// A share of the balance.
    Percent,
}

impl ProfitUnit {
    pub const ALL: [Self; 4] = [Self::MoneyPips, Self::Money, Self::Pips, Self::Percent];

    pub fn label(self) -> &'static str {
        match self {
            Self::MoneyPips => "Money and pips",
            Self::Money => "Money",
            Self::Pips => "Pips",
            Self::Percent => "Percent",
        }
    }
}

/// How a time is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeStyle {
    /// `Sep 25 16:35:12`.
    #[default]
    Short,
    /// `2026-09-25 16:35:12`.
    Iso,
    /// `16:35:12`.
    Clock,
}

impl TimeStyle {
    pub const ALL: [Self; 3] = [Self::Short, Self::Iso, Self::Clock];

    pub fn label(self) -> &'static str {
        match self {
            Self::Short => "Sep 25 16:35",
            Self::Iso => "2026-09-25 16:35",
            Self::Clock => "16:35:12",
        }
    }
}

/// Which side the rows are kept to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideFilter {
    #[default]
    All,
    Buy,
    Sell,
}

impl SideFilter {
    pub const ALL: [Self; 3] = [Self::All, Self::Buy, Self::Sell];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Buy => "Buy",
            Self::Sell => "Sell",
        }
    }

    pub fn keeps(self, buy: bool) -> bool {
        match self {
            Self::All => true,
            Self::Buy => buy,
            Self::Sell => !buy,
        }
    }
}

/// How far back the history reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRange {
    Today,
    Day,
    ThreeDays,
    #[default]
    Week,
}

impl HistoryRange {
    pub const ALL: [Self; 4] = [Self::Today, Self::Day, Self::ThreeDays, Self::Week];

    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Day => "24 hours",
            Self::ThreeDays => "3 days",
            Self::Week => "7 days",
        }
    }
}

/// Everything about the account panel that the user sets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelPrefs {
    /// The tab that was open.
    pub tab: Tab,
    pub tabs: Vec<Placed<Tab>>,
    pub stats: Vec<Placed<Stat>>,
    pub positions: TablePrefs<PositionCol>,
    pub orders: TablePrefs<OrderCol>,
    pub history: TablePrefs<DealCol>,
    pub exposure: TablePrefs<ExposureCol>,
    pub alerts: TablePrefs<AlertCol>,
    pub actions: Vec<Placed<RowAction>>,
    pub density: RowDensity,
    /// A tint on every other row.
    pub zebra: bool,
    /// A tint on a row that gains or loses.
    pub tint_rows: bool,
    /// Buy in the color of a rise and sell in the color of a fall.
    pub color_side: bool,
    pub profit_unit: ProfitUnit,
    pub time_style: TimeStyle,
    /// The line of totals under a table.
    pub totals: bool,
    /// The search field and the filters over a table.
    pub filters: bool,
    /// Only what is on the symbol of the active chart.
    pub only_symbol: bool,
    pub side: SideFilter,
    /// A click on a row shows its symbol on the active chart.
    pub click_shows_symbol: bool,
    /// Ask before closing a position, reversing it or closing many.
    pub confirm_close: bool,
    pub history_range: HistoryRange,
    /// The deals that opened a position, beside those that closed one.
    pub history_opening: bool,
    /// The strip of figures over the history: wins, losses, profit factor.
    pub history_stats: bool,
}

impl Default for PanelPrefs {
    fn default() -> Self {
        Self {
            tab: Tab::Positions,
            tabs: default_list(),
            stats: default_list(),
            positions: TablePrefs::default(),
            orders: TablePrefs::default(),
            history: TablePrefs::default(),
            exposure: TablePrefs::default(),
            alerts: TablePrefs::default(),
            actions: default_list(),
            density: RowDensity::Comfortable,
            zebra: false,
            tint_rows: false,
            color_side: true,
            profit_unit: ProfitUnit::MoneyPips,
            time_style: TimeStyle::Short,
            totals: true,
            filters: true,
            only_symbol: false,
            side: SideFilter::All,
            click_shows_symbol: true,
            confirm_close: true,
            history_range: HistoryRange::Week,
            history_opening: true,
            history_stats: true,
        }
    }
}

impl PanelPrefs {
    /// The prefs with every list mended and at least one tab showing.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        mend(&mut self.tabs);
        mend(&mut self.stats);
        mend(&mut self.actions);
        if !self.tabs.iter().any(|t| t.shown)
            && let Some(first) = self.tabs.first_mut()
        {
            first.shown = true;
        }
        // The tab that was open is one that shows.
        if !self.tabs.iter().any(|t| t.shown && t.item == self.tab)
            && let Some(first) = self.tabs.iter().find(|t| t.shown)
        {
            self.tab = first.item;
        }
        self.positions = self.positions.normalized();
        self.orders = self.orders.normalized();
        self.history = self.history.normalized();
        self.exposure = self.exposure.normalized();
        self.alerts = self.alerts.normalized();
        self
    }

    /// The tabs that show, in order.
    pub fn visible_tabs(&self) -> impl Iterator<Item = Tab> + '_ {
        self.tabs.iter().filter(|t| t.shown).map(|t| t.item)
    }

    pub fn visible_stats(&self) -> impl Iterator<Item = Stat> + '_ {
        self.stats.iter().filter(|s| s.shown).map(|s| s.item)
    }

    pub fn shows_action(&self, action: RowAction) -> bool {
        self.actions.iter().any(|a| a.item == action && a.shown)
    }

    /// Sorts a table by the column at `slot`, or turns the sort round.
    pub fn cycle_sort(&mut self, tab: Tab, slot: usize) {
        macro_rules! sort {
            ($table:expr) => {{
                if let Some(col) = $table.columns.get(slot).copied() {
                    $table.cycle_sort(col.item);
                }
            }};
        }
        match tab {
            Tab::Positions => sort!(self.positions),
            Tab::Orders => sort!(self.orders),
            Tab::History => sort!(self.history),
            Tab::Exposure => sort!(self.exposure),
            Tab::Alerts => sort!(self.alerts),
        }
    }

    /// Gives the column at `slot` a width.
    pub fn set_width(&mut self, tab: Tab, slot: usize, width: f32) {
        match tab {
            Tab::Positions => self.positions.set_width(slot, width),
            Tab::Orders => self.orders.set_width(slot, width),
            Tab::History => self.history.set_width(slot, width),
            Tab::Exposure => self.exposure.set_width(slot, width),
            Tab::Alerts => self.alerts.set_width(slot, width),
        }
    }

    /// Shows or hides the column at `slot`.
    pub fn toggle_column(&mut self, tab: Tab, slot: usize) {
        match tab {
            Tab::Positions => self.positions.toggle(slot),
            Tab::Orders => self.orders.toggle(slot),
            Tab::History => self.history.toggle(slot),
            Tab::Exposure => self.exposure.toggle(slot),
            Tab::Alerts => self.alerts.toggle(slot),
        }
    }

    /// Moves the column at `slot` along its table.
    pub fn move_column(&mut self, tab: Tab, slot: usize, delta: isize) {
        match tab {
            Tab::Positions => self.positions.shift(slot, delta),
            Tab::Orders => self.orders.shift(slot, delta),
            Tab::History => self.history.shift(slot, delta),
            Tab::Exposure => self.exposure.shift(slot, delta),
            Tab::Alerts => self.alerts.shift(slot, delta),
        }
    }

    /// Puts the columns of a table back as they were first.
    pub fn reset_columns(&mut self, tab: Tab) {
        match tab {
            Tab::Positions => self.positions = TablePrefs::default(),
            Tab::Orders => self.orders = TablePrefs::default(),
            Tab::History => self.history = TablePrefs::default(),
            Tab::Exposure => self.exposure = TablePrefs::default(),
            Tab::Alerts => self.alerts = TablePrefs::default(),
        }
    }

    /// The columns of a table for a list in a settings panel: name, whether it shows, and
    /// whether the table is sorted by it.
    pub fn column_list(&self, tab: Tab) -> Vec<(&'static str, bool)> {
        fn list<C: super::columns::Column>(t: &TablePrefs<C>) -> Vec<(&'static str, bool)> {
            t.columns
                .iter()
                .map(|c| (c.item.label(), c.shown))
                .collect()
        }
        match tab {
            Tab::Positions => list(&self.positions),
            Tab::Orders => list(&self.orders),
            Tab::History => list(&self.history),
            Tab::Exposure => list(&self.exposure),
            Tab::Alerts => list(&self.alerts),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_a_fixed_point_of_normalizing() {
        let prefs = PanelPrefs::default();
        assert_eq!(prefs, prefs.clone().normalized());
        assert_eq!(prefs.visible_tabs().count(), Tab::ALL.len());
        assert!(prefs.shows_action(RowAction::Close));
        assert!(!prefs.shows_action(RowAction::Half));
    }

    #[test]
    fn the_open_tab_is_always_one_that_shows() {
        let mut prefs = PanelPrefs {
            tab: Tab::Alerts,
            ..PanelPrefs::default()
        };
        for placed in &mut prefs.tabs {
            placed.shown = placed.item == Tab::Orders;
        }
        assert_eq!(prefs.normalized().tab, Tab::Orders);

        let mut none = PanelPrefs::default();
        for placed in &mut none.tabs {
            placed.shown = false;
        }
        let none = none.normalized();
        assert_eq!(none.visible_tabs().count(), 1);
        assert_eq!(none.tab, Tab::Positions);
    }

    #[test]
    fn prefs_from_an_older_file_load_with_the_defaults() {
        let old: PanelPrefs = toml::from_str("zebra = true\ndensity = \"compact\"\n").unwrap();
        assert!(old.zebra);
        assert_eq!(old.density, RowDensity::Compact);
        assert_eq!(old.positions, TablePrefs::default());
        assert!(old.confirm_close);
    }

    #[test]
    fn the_prefs_survive_a_round_trip() {
        let mut prefs = PanelPrefs {
            tab: Tab::History,
            history_range: HistoryRange::Today,
            ..PanelPrefs::default()
        };
        prefs.positions.cycle_sort(PositionCol::Profit);
        prefs.tabs.swap(0, 2);
        prefs.actions[1].shown = true;
        let text = toml::to_string(&prefs).unwrap();
        let back: PanelPrefs = toml::from_str(&text).unwrap();
        assert_eq!(back, prefs);
    }

    #[test]
    fn a_side_filter_keeps_its_side() {
        assert!(SideFilter::All.keeps(true) && SideFilter::All.keeps(false));
        assert!(SideFilter::Buy.keeps(true) && !SideFilter::Buy.keeps(false));
        assert!(!SideFilter::Sell.keeps(true) && SideFilter::Sell.keeps(false));
    }
}
