//! Picking the rows and columns of an export and turning them into a table of cells.

use super::choices::{Content, Order, RangeKind};
use super::columns::ColumnKey;
use super::options::ExportOptions;
use super::render::{local_text, zone_name};
use super::source::{Cell, Point, Source, Table, TableColumn};
use wyck_core::math::{count_i64, f64_to_i64, i64_to_f64};

use crate::settings::ChartKind;
use crate::zone::Zone;
use wyck_indicators::spec::ValueFormat;

/// Whether a date field holds a date, or nothing (an open side of the range).
pub fn date_is_valid(text: &str) -> bool {
    text.trim().is_empty() || crate::zone::parse_local(text).is_some()
}

/// A side of a range of dates, in Unix milliseconds: the time typed, or for the end of the range
/// the last instant of a day typed alone. `None` for a text that is not a date.
pub(super) fn parse_bound(text: &str, zone: Zone, end: bool) -> Option<i64> {
    let (_, whole_day) = crate::zone::parse_local(text)?;
    let utc = zone.parse(text)?;
    Some(if end && whole_day {
        utc + 86_400_000 - 1
    } else {
        utc
    })
}

/// The rows of `items` that the range asks for.
pub(super) fn select(items: &[Point], source: &Source<'_>, options: &ExportOptions) -> Vec<usize> {
    let all = 0..items.len();
    let zone = options.zone(source.zone);
    let mut picked: Vec<usize> = match options.range {
        RangeKind::Loaded => all.collect(),
        RangeKind::Last => {
            let n = options.last as usize;
            (items.len().saturating_sub(n)..items.len()).collect()
        }
        RangeKind::Dates => {
            let from = parse_bound(&options.from, zone, false).unwrap_or(i64::MIN);
            let to = parse_bound(&options.to, zone, true).unwrap_or(i64::MAX);
            all.filter(|&i| {
                items
                    .get(i)
                    .is_some_and(|item| (from..=to).contains(&item.time))
            })
            .collect()
        }
        RangeKind::Visible => {
            let series = source.display.shown(source.raw);
            let (first, last) = source.visible;
            let last = last.min(series.len());
            if first >= last {
                Vec::new()
            } else if options.content == Content::Shown {
                (first..last.min(items.len())).collect()
            } else {
                // The prices behind the points on screen: from the time of the first to the
                // time of the point after the last.
                let start = series.time_at(first).unwrap_or(i64::MIN);
                let end = series.time_at(last).map_or(i64::MAX, |t| t - 1);
                all.filter(|&i| {
                    items
                        .get(i)
                        .is_some_and(|item| (start..=end).contains(&item.time))
                })
                .collect()
            }
        }
    };
    if options.skip_forming
        && let Some(newest) = items.last()
        && picked.last() == Some(&(items.len() - 1))
    {
        let forming = source.is_derived(options.content)
            || source
                .bar_ms
                .is_some_and(|ms| newest.time + ms > source.now_ms);
        if forming {
            picked.pop();
        }
    }
    if options.every > 1 {
        picked = picked.into_iter().step_by(options.every as usize).collect();
    }
    picked
}

pub(super) fn box_size_of(source: &Source<'_>) -> Option<i64> {
    let size = source.display.box_size;
    (size > 0).then_some(size)
}

/// The cell of a column for row `i` of `items` (which is row `n` of the table, from 1).
pub(super) fn cell(
    key: ColumnKey,
    items: &[Point],
    i: usize,
    n: usize,
    source: &Source<'_>,
    options: &ExportOptions,
) -> Cell {
    let Some(&it) = items.get(i) else {
        return Cell::Empty;
    };
    let previous = i
        .checked_sub(1)
        .and_then(|p| items.get(p))
        .map(|item| item.close);
    let profile = || {
        (options.content == Content::Shown && source.kind == ChartKind::Tpo)
            .then(|| source.display.tpo.get(i))
            .flatten()
    };
    let derived = options.content == Content::Shown && source.display.is_derived();
    match key {
        ColumnKey::Time => Cell::Time(it.time),
        ColumnKey::Date => Cell::Date(it.time),
        ColumnKey::Clock => Cell::Clock(it.time),
        ColumnKey::Open => Cell::Price(it.open),
        ColumnKey::High => Cell::Price(it.high),
        ColumnKey::Low => Cell::Price(it.low),
        ColumnKey::Close | ColumnKey::Price => Cell::Price(it.close),
        ColumnKey::Volume => Cell::Int(it.volume),
        ColumnKey::Change => previous.map_or(Cell::Empty, |p| Cell::Price(it.close - p)),
        ColumnKey::ChangePercent => match previous {
            Some(p) if p != 0 => Cell::Value(i64_to_f64(it.close - p) / i64_to_f64(p) * 100.0),
            _ => Cell::Empty,
        },
        ColumnKey::Range => Cell::Price(it.high - it.low),
        ColumnKey::Body => Cell::Price(it.close - it.open),
        ColumnKey::Typical => Cell::Price((it.high + it.low + it.close) / 3),
        ColumnKey::Median => Cell::Price(i64::midpoint(it.high, it.low)),
        ColumnKey::Direction => Cell::Text(
            match it.close.cmp(&it.open) {
                std::cmp::Ordering::Greater => "up",
                std::cmp::Ordering::Less => "down",
                std::cmp::Ordering::Equal => "flat",
            }
            .to_owned(),
        ),
        ColumnKey::Index => Cell::Int(count_i64(n)),
        ColumnKey::Symbol => Cell::Text(source.symbol.to_owned()),
        ColumnKey::Timeframe => Cell::Text(source.timeframe.to_owned()),
        ColumnKey::ChartType => Cell::Text(
            if options.content == Content::Shown {
                source.kind.label()
            } else {
                "Prices"
            }
            .to_owned(),
        ),
        ColumnKey::BoxSize => {
            if source.kind == ChartKind::Tpo {
                profile().map_or(Cell::Empty, |p| Cell::Price(p.row_size))
            } else if derived {
                box_size_of(source).map_or(Cell::Empty, Cell::Price)
            } else {
                Cell::Empty
            }
        }
        ColumnKey::Poc => profile().map_or(Cell::Empty, |p| {
            Cell::Price(i64::midpoint(p.row_bottom(p.poc), p.row_top(p.poc)))
        }),
        ColumnKey::ValueAreaHigh => {
            profile().map_or(Cell::Empty, |p| Cell::Price(p.row_top(p.value_area.1)))
        }
        ColumnKey::ValueAreaLow => {
            profile().map_or(Cell::Empty, |p| Cell::Price(p.row_bottom(p.value_area.0)))
        }
        ColumnKey::InitialBalanceHigh => profile()
            .and_then(|p| p.initial_balance)
            .map_or(Cell::Empty, |(_, high)| Cell::Price(high)),
        ColumnKey::InitialBalanceLow => profile()
            .and_then(|p| p.initial_balance)
            .map_or(Cell::Empty, |(low, _)| Cell::Price(low)),
        ColumnKey::Marks => profile().map_or(Cell::Empty, |p| Cell::Int(count_i64(p.marks))),
        ColumnKey::Periods => profile().map_or(Cell::Empty, |p| Cell::Int(count_i64(p.periods))),
        ColumnKey::PoorHigh => {
            profile().map_or(Cell::Empty, |p| Cell::Text(p.poor_high.to_string()))
        }
        ColumnKey::PoorLow => profile().map_or(Cell::Empty, |p| Cell::Text(p.poor_low.to_string())),
        ColumnKey::Thickness => source
            .display
            .kagi
            .get(i)
            .filter(|_| options.content == Content::Shown && source.kind == ChartKind::Kagi)
            .map_or(Cell::Empty, |line| {
                Cell::Text(if line.yang { "yang" } else { "yin" }.to_owned())
            }),
        ColumnKey::Glyph | ColumnKey::Boxes => source
            .display
            .pnf
            .get(i)
            .filter(|_| options.content == Content::Shown && source.kind == ChartKind::PointFigure)
            .map_or(Cell::Empty, |c| match key {
                ColumnKey::Glyph => Cell::Text(if c.up { "X" } else { "O" }.to_owned()),
                _ => Cell::Int(c.top - c.bottom + 1),
            }),
        ColumnKey::Study { study, plot } => {
            let usable = options.content == Content::Shown || !source.display.is_derived();
            // An indicator in price units is written like a price, so it lines up with the
            // columns of the prices; any other is a plain number.
            let in_price = source
                .settings
                .studies
                .get(study)
                .is_some_and(|c| matches!(c.value_format(), ValueFormat::Price));
            usable
                .then(|| {
                    source
                        .display
                        .studies
                        .get(study)?
                        .as_ref()?
                        .plots
                        .get(plot)?
                        .values
                        .get(i)
                        .copied()
                })
                .flatten()
                .filter(|v| v.is_finite())
                .map_or(Cell::Empty, |v| {
                    if in_price {
                        Cell::Price(f64_to_i64(v.round()))
                    } else {
                        Cell::Value(v)
                    }
                })
        }
    }
}

/// The table the options ask for. `rows` caps how many rows are built (for a preview); the count
/// of all of them is kept in [`Table::total_rows`].
pub fn table(source: &Source<'_>, options: &ExportOptions, rows: Option<usize>) -> Table {
    let items = source.items(options.content);
    let mut picked = select(&items, source, options);
    if options.order == Order::NewestFirst {
        picked.reverse();
    }
    if options.limit > 0 {
        picked.truncate(options.limit as usize);
    }
    let total_rows = picked.len();
    // The dates of the notes are those of all the rows, not only of the ones a preview builds.
    let time_of = |&i: &usize| items.get(i).map(|item| item.time);
    let first = picked.first().and_then(time_of);
    let last = picked.last().and_then(time_of);
    if let Some(cap) = rows {
        picked.truncate(cap);
    }
    let columns: Vec<TableColumn> = options
        .columns
        .iter()
        .map(|&key| TableColumn {
            key,
            name: options
                .renames
                .get(&key.code())
                .cloned()
                .unwrap_or_else(|| options.header_case.apply(&source.column_name(key))),
        })
        .collect();
    let rows_out = picked
        .iter()
        .enumerate()
        .map(|(n, &i)| {
            options
                .columns
                .iter()
                .map(|&key| cell(key, &items, i, n + 1, source, options))
                .collect()
        })
        .collect();
    let zone = options.zone(source.zone);
    let show =
        |ms: Option<i64>| ms.map_or_else(String::new, |t| local_text(t, zone, "%Y-%m-%d %H:%M:%S"));
    let (from, to) = if options.order == Order::OldestFirst {
        (first, last)
    } else {
        (last, first)
    };
    Table {
        columns,
        rows: rows_out,
        notes: vec![
            ("symbol".to_owned(), source.symbol.to_owned()),
            ("timeframe".to_owned(), source.timeframe.to_owned()),
            (
                "type".to_owned(),
                if options.content == Content::Shown {
                    source.kind.label().to_owned()
                } else {
                    "Prices".to_owned()
                },
            ),
            (
                "zone".to_owned(),
                zone_name(zone, from.unwrap_or(source.now_ms)),
            ),
            ("from".to_owned(), show(from)),
            ("to".to_owned(), show(to)),
            ("rows".to_owned(), total_rows.to_string()),
        ],
        total_rows,
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn a_date_field_is_valid_when_empty_or_a_date() {
        assert!(date_is_valid(""));
        assert!(date_is_valid("  "));
        assert!(date_is_valid("2026-01-05"));
        assert!(date_is_valid("2026-01-05 09:30"));
        assert!(date_is_valid("2026-01-05T09:30:15"));
        assert!(!date_is_valid("5 January"));
        assert!(!date_is_valid("2026-13-45"));
    }
}
