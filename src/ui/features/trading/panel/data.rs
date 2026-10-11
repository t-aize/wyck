//! What the tables of the account panel show: every row of every tab worked out from the account
//! and the alerts, as cells with a text, a tone and a value to sort by, then filtered, sorted and
//! totalled as the user asked.
//!
//! Rows are built with a cell for every column the table has, so showing or moving a column never
//! asks the account again.

use std::cmp::Ordering;

use super::stats::{self, HistoryStats};
use crate::app::account::Account;
use crate::app::alerts::Alerts;
use crate::app::prefs::columns::{Sort, TablePrefs};
use crate::app::prefs::panel::{PanelPrefs, Tab};
use crate::domain::chart::zone::Zone;
use crate::domain::trading::math;
use crate::domain::trading::math::{Contract, format_money};

/// The color a cell is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Muted,
    Up,
    Down,
    Accent,
}

impl Tone {
    /// The tone of an amount: gains up, losses down.
    pub fn of(value: f64) -> Self {
        if value > 0.0 {
            Self::Up
        } else if value < 0.0 {
            Self::Down
        } else {
            Self::Plain
        }
    }
}

/// What a cell is sorted by.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    None,
    Num(f64),
    Text(String),
}

/// How the sum of a column is written in the totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Lots,
    Money,
    Percent,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
    pub key: Key,
    /// What the column adds up, when it does.
    pub sum: Option<f64>,
    pub unit: Unit,
}

impl Cell {
    fn text(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            key: Key::Text(text.to_lowercase()),
            text,
            tone: Tone::Plain,
            sum: None,
            unit: Unit::None,
        }
    }

    fn empty() -> Self {
        Self {
            text: String::new(),
            tone: Tone::Muted,
            key: Key::None,
            sum: None,
            unit: Unit::None,
        }
    }

    fn dash() -> Self {
        Self {
            text: "-".to_owned(),
            ..Self::empty()
        }
    }

    fn num(text: impl Into<String>, value: f64) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Plain,
            key: Key::Num(value),
            sum: None,
            unit: Unit::None,
        }
    }

    /// A number that may be missing.
    fn opt(value: Option<f64>, text: impl FnOnce(f64) -> String) -> Self {
        value.map_or_else(Self::dash, |v| Self::num(text(v), v))
    }

    fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    fn adds(mut self, value: f64, unit: Unit) -> Self {
        self.sum = Some(value);
        self.unit = unit;
        self
    }
}

/// What can be done to a position from its row.
#[derive(Debug, Clone)]
pub struct PositionRow {
    pub id: i64,
    /// Half of the volume, stepped, when that is a volume the broker takes.
    pub half: Option<i64>,
    pub entry: Option<f64>,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub trailing: bool,
    pub busy: bool,
    /// `Buy 0.1 EURUSD`, for what a confirmation says.
    pub describe: String,
}

#[derive(Debug, Clone)]
pub enum RowKind {
    Position(PositionRow),
    Order {
        id: i64,
        busy: bool,
        describe: String,
    },
    Deal,
    Alert {
        id: u64,
        active: bool,
    },
    Exposure,
    /// A time an alert fired; `alert` is the alert, which may be gone.
    Firing {
        alert: u64,
    },
}

#[derive(Debug, Clone)]
pub struct Row {
    /// A key that stays the same for the same row, for element ids.
    pub key: String,
    pub symbol: Option<i64>,
    /// What the row gains or loses, for its tint.
    pub profit: Option<f64>,
    pub kind: RowKind,
    /// One cell for each column the table can have, in the order of the column type.
    pub cells: Vec<Cell>,
}

/// A column that shows.
#[derive(Debug, Clone)]
pub struct ColView {
    /// Its place among all the columns of the table.
    pub index: usize,
    /// Its place among the columns the user arranged.
    pub slot: usize,
    pub label: &'static str,
    pub width: f32,
    pub right: bool,
    /// `Some(descending)` when the table is sorted by it.
    pub sorted: Option<bool>,
}

/// A table ready to draw.
#[derive(Debug, Clone)]
pub struct Table {
    pub tab: Tab,
    pub columns: Vec<ColView>,
    pub rows: Vec<Row>,
    /// One text for each column that shows; empty where a column adds up to nothing.
    pub totals: Vec<String>,
    /// How many rows there are before the filters.
    pub unfiltered: usize,
    pub stats: Option<HistoryStats>,
}

/// What the tables are built from.
pub struct Ctx<'a> {
    pub account: &'a Account,
    pub alerts: &'a Alerts,
    pub prefs: &'a PanelPrefs,
    /// The symbol of the active chart.
    pub symbol: Option<i64>,
    pub query: &'a str,
}

impl Ctx<'_> {
    /// Whether a row passes the filters and the search.
    fn keeps(&self, symbol: i64, buy: Option<bool>, texts: &[&str]) -> bool {
        if self.prefs.only_symbol && self.symbol.is_some_and(|s| s != symbol) {
            return false;
        }
        if let Some(buy) = buy
            && !self.prefs.side.keeps(buy)
        {
            return false;
        }
        stats::matches(self.query, texts)
    }

    fn currency(&self) -> &str {
        &self.account.book.currency
    }

    fn time(&self, ms: i64) -> String {
        stats::format_time(Zone::Local.shift(ms), self.prefs.time_style)
    }
}

fn side_cell(buy: bool, colored: bool) -> Cell {
    let cell = Cell::text(if buy { "Buy" } else { "Sell" });
    if colored {
        cell.tone(if buy { Tone::Up } else { Tone::Down })
    } else {
        cell
    }
}

fn price_cell(contract: &Contract, price: Option<f64>) -> Cell {
    Cell::opt(price, |p| contract.format_price(p))
}

fn pips_cell(contract: &Contract, distance: Option<f64>) -> Cell {
    Cell::opt(distance, |d| format!("{:+.1}", contract.pips(d)))
}

/// The value a sort key compares: numbers before text, missing last.
fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Num(x), Key::Num(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Key::Text(x), Key::Text(y)) => x.cmp(y),
        (Key::None, Key::None) => Ordering::Equal,
        (Key::None, _) => Ordering::Greater,
        (_, Key::None) => Ordering::Less,
        (Key::Num(_), Key::Text(_)) => Ordering::Less,
        (Key::Text(_), Key::Num(_)) => Ordering::Greater,
    }
}

/// Puts the rows in the order of a sort. A missing value stays last either way.
fn sort_rows(rows: &mut [Row], index: usize, descending: bool) {
    rows.sort_by(|a, b| {
        let (x, y) = (&a.cells[index].key, &b.cells[index].key);
        match (x, y) {
            (Key::None, _) | (_, Key::None) => compare(x, y),
            _ if descending => compare(y, x),
            _ => compare(x, y),
        }
    });
}

/// The columns that show, with which one the table is sorted by.
fn views<C: crate::app::prefs::columns::Column>(prefs: &TablePrefs<C>) -> Vec<ColView> {
    prefs
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.shown)
        .map(|(slot, c)| ColView {
            index: C::ALL.iter().position(|x| *x == c.item).unwrap_or(0),
            slot,
            label: c.item.label(),
            width: prefs.width_at(slot),
            right: c.item.right(),
            sorted: prefs
                .sort
                .filter(|s| s.column == c.item)
                .map(|s| s.descending),
        })
        .collect()
}

/// What a column adds up to, written for its unit.
fn total_text(sum: f64, unit: Unit, currency: &str, balance: f64) -> String {
    match unit {
        Unit::None => String::new(),
        Unit::Lots => math::format_lots(sum),
        Unit::Money => format_money(sum, currency),
        Unit::Percent if balance > 0.0 => format!("{sum:+.2}%"),
        Unit::Percent => String::new(),
    }
}

/// Sorts, totals and wraps the rows of a table.
fn finish<C: crate::app::prefs::columns::Column>(
    ctx: &Ctx,
    tab: Tab,
    prefs: &TablePrefs<C>,
    mut rows: Vec<Row>,
    unfiltered: usize,
    stats: Option<HistoryStats>,
) -> Table {
    let columns = views(prefs);
    if let Some(Sort { column, descending }) = prefs.sort
        && let Some(index) = C::ALL.iter().position(|c| *c == column)
    {
        sort_rows(&mut rows, index, descending);
    }
    let balance = ctx.account.summary().balance;
    let mut totals: Vec<String> = columns
        .iter()
        .map(|col| {
            let cells = rows.iter().map(|r| &r.cells[col.index]);
            let unit = rows.first().map_or(Unit::None, |r| r.cells[col.index].unit);
            let sums: Vec<f64> = cells.filter_map(|c| c.sum).collect();
            if sums.is_empty() {
                String::new()
            } else {
                total_text(sums.iter().sum(), unit, ctx.currency(), balance)
            }
        })
        .collect();
    // The first column says what the line is, when it adds up to nothing itself.
    if let Some(first) = totals.first_mut()
        && first.is_empty()
        && !rows.is_empty()
    {
        *first = "Total".to_owned();
    }
    Table {
        tab,
        columns,
        rows,
        totals,
        unfiltered,
        stats,
    }
}

/// The table of a tab.
pub fn build(ctx: &Ctx, tab: Tab) -> Table {
    match tab {
        Tab::Positions => positions(ctx),
        Tab::Orders => orders(ctx),
        Tab::History => history(ctx),
        Tab::Exposure => exposure(ctx),
        Tab::Alerts => alerts(ctx),
        Tab::AlertLog => alert_log(ctx),
    }
}

/// The table as CSV: the columns that show, then a line for each row.
pub fn to_csv(table: &Table) -> String {
    let mut lines = vec![stats::csv_line(table.columns.iter().map(|c| c.label))];
    for row in &table.rows {
        lines.push(stats::csv_line(
            table
                .columns
                .iter()
                .map(|c| row.cells[c.index].text.as_str()),
        ));
    }
    lines.join("\n")
}

mod alerts;
mod exposure;
mod orders;
mod positions;

use alerts::{alert_log, alerts, capitalized};
use exposure::exposure;
use orders::{history, orders};
use positions::positions;

#[cfg(test)]
#[path = "../../../../../tests/unit/ui/features/trading/panel/data.rs"]
mod tests;
