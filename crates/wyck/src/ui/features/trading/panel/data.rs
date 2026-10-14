//! What the tables of the account panel show: every row of every tab worked out from the account
//! and the alerts, as cells with a text and a tone, then totalled.
//!
//! Each tab has fixed columns (see [`columns`]), and every row has one cell for each of them, in
//! the same order.

pub mod alerts;
pub mod exposure;
pub mod orders;
pub mod positions;

use crate::app::account::Account;
use crate::app::alerts::Alerts;
use crate::app::prefs::panel::Tab;
use crate::ui::kit::tokens;
use wyck_chart::zone::Zone;
use wyck_trading::math;
use wyck_trading::math::{Contract, format_amount};

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

/// How the sum of a column is written in the totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Lots,
    Money,
}

#[derive(Debug, Clone)]
pub struct Cell {
    pub text: String,
    pub tone: Tone,
    /// What the column adds up, when it does.
    pub sum: Option<f64>,
    pub unit: Unit,
}

impl Cell {
    fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Plain,
            sum: None,
            unit: Unit::None,
        }
    }

    fn dash() -> Self {
        Self::text("-").tone(Tone::Muted)
    }

    /// A number that may be missing.
    fn opt(value: Option<f64>, text: impl FnOnce(f64) -> String) -> Self {
        value.map_or_else(Self::dash, |v| Self::text(text(v)))
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
    Alert {
        id: u64,
    },
    /// A closed trade, a symbol's exposure or a time an alert fired: nothing to do to the row.
    Plain,
}

#[derive(Debug, Clone)]
pub struct Row {
    /// A key that stays the same for the same row, for element ids.
    pub key: String,
    pub symbol: Option<i64>,
    pub kind: RowKind,
    /// One cell for each column of the table.
    pub cells: Vec<Cell>,
}

/// A column of a table.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub label: &'static str,
    /// How wide it is at least, in pixels, at the scale of the interface now.
    pub width: f32,
    /// Whether its text sits at the right, as numbers do.
    pub right: bool,
}

/// A table ready to draw.
#[derive(Debug, Clone)]
pub struct Table {
    pub tab: Tab,
    pub columns: Vec<Column>,
    pub rows: Vec<Row>,
    /// One total for each column; its text is empty where a column adds up to nothing.
    pub totals: Vec<(String, Tone)>,
}

/// What the tables are built from.
pub struct Ctx<'a> {
    pub account: &'a Account,
    pub alerts: &'a Alerts,
}

impl Ctx<'_> {
    fn currency(&self) -> &str {
        &self.account.book.currency
    }

    fn time(&self, ms: i64) -> String {
        super::stats::format_time(Zone::Local.shift(ms))
    }
}

/// A money amount of a table: the symbol of the currency, a `+` on a gain.
fn money_text(amount: f64, currency: &str) -> String {
    format_amount(amount, currency, true)
}

fn side_cell(buy: bool) -> Cell {
    Cell::text(if buy { "Buy" } else { "Sell" }).tone(if buy { Tone::Up } else { Tone::Down })
}

fn price_cell(contract: &Contract, price: Option<f64>) -> Cell {
    Cell::opt(price, |p| contract.format_price(p))
}

/// The columns of a tab, as wide as the sizes every screen shares.
fn columns(tab: Tab) -> Vec<Column> {
    let (short, number, wide, text) = (
        tokens::field::narrow(),
        tokens::field::number(),
        tokens::field::wide(),
        tokens::field::text(),
    );
    let col = |label, width, right| Column {
        label,
        width,
        right,
    };
    match tab {
        Tab::Positions => vec![
            col("Symbol", number, false),
            col("Side", short, false),
            col("Volume", short, true),
            col("Entry", number, true),
            col("Price", number, true),
            col("SL", number, true),
            col("TP", number, true),
            col("Profit", wide, true),
        ],
        Tab::Orders => vec![
            col("Symbol", number, false),
            col("Side", short, false),
            col("Type", number, false),
            col("Volume", short, true),
            col("Price", number, true),
            col("SL", number, true),
            col("TP", number, true),
        ],
        Tab::History => vec![
            col("Closed", wide, false),
            col("Symbol", number, false),
            col("Side", short, false),
            col("Volume", short, true),
            col("Entry", number, true),
            col("Exit", number, true),
            col("Profit", wide, true),
        ],
        Tab::Exposure => vec![
            col("Symbol", number, false),
            col("Net", short, true),
            col("Long", short, true),
            col("Short", short, true),
            col("Positions", short, true),
            col("Average entry", number, true),
            col("Profit", wide, true),
        ],
        Tab::Alerts => vec![
            col("Symbol", number, false),
            col("Condition", wide, false),
            col("Level", number, true),
            col("State", wide, false),
            col("Message", text, false),
        ],
        Tab::AlertLog => vec![
            col("Time", wide, false),
            col("Symbol", number, false),
            col("Watched", number, false),
            col("Condition", wide, false),
            col("Value", number, true),
            col("Message", text, false),
        ],
    }
}

/// What a column adds up to, written for its unit.
fn total_text(sum: f64, unit: Unit, currency: &str) -> String {
    match unit {
        Unit::None => String::new(),
        Unit::Lots => math::format_lots(sum),
        Unit::Money => money_text(sum, currency),
    }
}

/// Totals the rows of a table and wraps them.
fn finish(ctx: &Ctx, tab: Tab, rows: Vec<Row>) -> Table {
    let columns = columns(tab);
    let mut totals: Vec<(String, Tone)> = (0..columns.len())
        .map(|index| {
            let unit = rows.first().map_or(Unit::None, |r| r.cells[index].unit);
            let sums: Vec<f64> = rows.iter().filter_map(|r| r.cells[index].sum).collect();
            if sums.is_empty() {
                return (String::new(), Tone::Muted);
            }
            let sum: f64 = sums.iter().sum();
            let tone = if unit == Unit::Money {
                Tone::of(sum)
            } else {
                Tone::Muted
            };
            (total_text(sum, unit, ctx.currency()), tone)
        })
        .collect();
    // The first column says what the line is.
    if let Some(first) = totals.first_mut()
        && first.0.is_empty()
        && !rows.is_empty()
    {
        *first = ("Total".to_owned(), Tone::Muted);
    }
    Table {
        tab,
        columns,
        rows,
        totals,
    }
}

/// The table of a tab.
pub fn build(ctx: &Ctx, tab: Tab) -> Table {
    match tab {
        Tab::Positions => positions::positions(ctx),
        Tab::Orders => orders::orders(ctx),
        Tab::History => orders::history(ctx),
        Tab::Exposure => exposure::exposure(ctx),
        Tab::Alerts => alerts::alerts(ctx),
        Tab::AlertLog => alerts::alert_log(ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_are_written_for_their_unit() {
        assert_eq!(total_text(1.5, Unit::Lots, "USD"), "1.5");
        assert_eq!(total_text(-3.0, Unit::Money, "USD"), "-$3.00");
        assert_eq!(total_text(127.6, Unit::Money, "USD"), "+$127.60");
        assert_eq!(total_text(2.0, Unit::None, "USD"), "");
    }

    #[test]
    fn tones_follow_the_sign() {
        assert_eq!(Tone::of(1.0), Tone::Up);
        assert_eq!(Tone::of(-1.0), Tone::Down);
        assert_eq!(Tone::of(0.0), Tone::Plain);
    }

    #[test]
    fn every_tab_has_columns_and_a_first_one_for_the_symbol_or_time() {
        for tab in Tab::ALL {
            let columns = columns(tab);
            assert!(columns.len() >= 5, "{tab:?}");
            assert!(matches!(columns[0].label, "Symbol" | "Closed" | "Time"));
        }
    }
}
