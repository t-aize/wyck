//! What an export reads: the data of the chart, and the table it is cut into.

use super::choices::Content;
use super::columns::ColumnKey;

use crate::domain::chart::data::Series;
use crate::domain::chart::display::Display;
use crate::domain::chart::settings::{ChartKind, ChartSettings};
use crate::domain::chart::zone::Zone;

/// What an export reads: the chart's data and how it stands.
pub struct Source<'a> {
    pub symbol: &'a str,
    /// The timeframe as written, such as `M5`.
    pub timeframe: &'a str,
    /// How long a bar lasts, when it is a period of time.
    pub bar_ms: Option<i64>,
    pub kind: ChartKind,
    /// The decimals of the symbol.
    pub digits: u32,
    /// The zone of the chart.
    pub zone: Zone,
    pub raw: &'a Series,
    pub display: &'a Display,
    pub settings: &'a ChartSettings,
    /// The points on screen of the series the chart draws.
    pub visible: (usize, usize),
    pub now_ms: i64,
}

/// One row of the series read: a bar, or a tick as a bar of one price.
#[derive(Debug, Clone, Copy)]
pub(super) struct Point {
    pub(super) time: i64,
    pub(super) open: i64,
    pub(super) high: i64,
    pub(super) low: i64,
    pub(super) close: i64,
    pub(super) volume: i64,
}

impl Source<'_> {
    pub(super) fn series(&self, content: Content) -> &Series {
        match content {
            Content::Prices => self.raw,
            Content::Shown => self.display.shown(self.raw),
        }
    }

    pub(super) fn items(&self, content: Content) -> Vec<Point> {
        match self.series(content) {
            Series::Bars(bars) => bars
                .iter()
                .map(|b| Point {
                    time: b.time_ms,
                    open: b.open,
                    high: b.high,
                    low: b.low,
                    close: b.close,
                    volume: b.volume,
                })
                .collect(),
            Series::Ticks(ticks) => ticks
                .iter()
                .map(|t| Point {
                    time: t.time_ms,
                    open: t.price,
                    high: t.price,
                    low: t.price,
                    close: t.price,
                    volume: 1,
                })
                .collect(),
        }
    }

    pub(super) fn is_ticks(&self, content: Content) -> bool {
        matches!(self.series(content), Series::Ticks(_))
    }

    /// Whether the rows exported are elements the chart type laid out, not the prices.
    pub(super) fn is_derived(&self, content: Content) -> bool {
        content == Content::Shown && self.display.is_derived()
    }

    /// The columns that make sense for `content`, in the order a list should show them.
    pub fn available(&self, content: Content) -> Vec<ColumnKey> {
        use ColumnKey as K;
        let mut out = vec![K::Time, K::Date, K::Clock];
        if self.is_ticks(content) {
            out.push(K::Price);
        } else {
            out.extend([K::Open, K::High, K::Low, K::Close, K::Volume]);
        }
        out.extend([K::Change, K::ChangePercent]);
        if !self.is_ticks(content) {
            out.extend([K::Range, K::Body, K::Typical, K::Median, K::Direction]);
        }
        out.extend([K::Index, K::Symbol, K::Timeframe, K::ChartType]);
        if self.is_derived(content) {
            match self.kind {
                ChartKind::Renko
                | ChartKind::LineBreak
                | ChartKind::Range
                | ChartKind::Kagi
                | ChartKind::PointFigure => out.push(K::BoxSize),
                _ => {}
            }
            match self.kind {
                ChartKind::Kagi => out.push(K::Thickness),
                ChartKind::PointFigure => out.extend([K::Glyph, K::Boxes]),
                ChartKind::Tpo => out.extend([
                    K::Poc,
                    K::ValueAreaHigh,
                    K::ValueAreaLow,
                    K::InitialBalanceHigh,
                    K::InitialBalanceLow,
                    K::Marks,
                    K::Periods,
                    K::PoorHigh,
                    K::PoorLow,
                ]),
                _ => {}
            }
        }
        // The indicators were computed on what the chart draws, so their rows only line up with
        // that (or with the prices, when the chart draws them as they are).
        if content == Content::Shown || !self.display.is_derived() {
            for (study, output) in self.display.studies.iter().enumerate() {
                if let Some(output) = output {
                    out.extend((0..output.plots.len()).map(|plot| K::Study { study, plot }));
                }
            }
        }
        out
    }

    /// The name of a column, with the name of its indicator for a line of one.
    pub fn column_name(&self, key: ColumnKey) -> String {
        match key {
            ColumnKey::Study { study, plot } => {
                let title = self
                    .settings
                    .studies
                    .get(study)
                    .map_or_else(|| format!("Study {}", study + 1), |c| c.title());
                let line = self
                    .display
                    .studies
                    .get(study)
                    .and_then(Option::as_ref)
                    .and_then(|o| o.plots.get(plot))
                    .map(|p| p.key);
                match line {
                    Some(line) => format!("{title} {line}"),
                    None => title,
                }
            }
            other => other.default_name(),
        }
    }
}

/// One value of a table.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Int(i64),
    /// A price in the server's raw units.
    Price(i64),
    /// A number that is not a price: an indicator, a percent.
    Value(f64),
    /// A time in Unix milliseconds.
    Time(i64),
    /// A time as a date alone, or as a time of day alone.
    Date(i64),
    Clock(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableColumn {
    pub key: ColumnKey,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub columns: Vec<TableColumn>,
    pub rows: Vec<Vec<Cell>>,
    /// Where the data comes from, for the notes.
    pub notes: Vec<(String, String)>,
    /// How many rows the options selected, before the limit of a preview cut them.
    pub total_rows: usize,
}
