#![cfg(test)]

mod formats;
mod presets;
mod sources;

use super::choices::*;
use super::columns::*;
use super::options::*;
use super::presets::*;
use super::render::*;
use super::source::*;
use super::table::*;
use super::*;
use crate::domain::chart::data::Series;
use crate::domain::chart::display::Display;
use crate::domain::chart::settings::{ChartKind, ChartSettings};
use crate::domain::chart::transform::settings::BoxSize;
use crate::domain::chart::zone::Zone;
use crate::domain::indicators::config::StudyConfig;
use crate::domain::indicators::kind::StudyKind;
use crate::domain::market::{Bar, Tick};

/// Monday 2026-01-05 00:00 UTC.
const MONDAY: i64 = 1_767_571_200_000;

const MIN: i64 = 60_000;

fn bars(n: usize) -> Vec<Bar> {
    (0..n)
        .map(|i| {
            let base = 100_000 + (i as i64 % 40) * 10;
            Bar {
                time_ms: MONDAY + i as i64 * 5 * MIN,
                open: base,
                high: base + 30,
                low: base - 20,
                close: base + 10,
                volume: 1 + i as i64 % 7,
            }
        })
        .collect()
}

struct Fixture {
    raw: Series,
    display: Display,
    settings: ChartSettings,
}

impl Fixture {
    fn new(raw: Series, settings: ChartSettings) -> Self {
        let display = Display::build(&raw, &settings, 1);
        Self {
            raw,
            display,
            settings,
        }
    }

    fn bars(n: usize) -> Self {
        Self::new(Series::Bars(bars(n)), ChartSettings::default())
    }

    fn source(&self) -> Source<'_> {
        Source {
            symbol: "EURUSD",
            timeframe: "M5",
            bar_ms: Some(5 * MIN),
            kind: self.settings.kind,
            digits: 5,
            zone: Zone::Utc,
            raw: &self.raw,
            display: &self.display,
            settings: &self.settings,
            visible: (0, self.display.shown(&self.raw).len()),
            now_ms: MONDAY + 10_000 * MIN,
        }
    }
}

fn options(edit: impl FnOnce(&mut ExportOptions)) -> ExportOptions {
    let mut o = ExportOptions {
        range: RangeKind::Loaded,
        ..ExportOptions::default()
    };
    edit(&mut o);
    o.normalized()
}

fn out(f: &Fixture, o: &ExportOptions) -> String {
    let source = f.source();
    render(&table(&source, o, None), o, source.digits, source.zone)
}

fn lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

#[test]
fn a_plain_export_has_a_header_and_a_line_for_each_bar() {
    let f = Fixture::bars(3);
    let text = out(&f, &options(|_| {}));
    let lines = lines(&text);
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], "Time,Open,High,Low,Close,Volume");
    assert_eq!(
        lines[1],
        "2026-01-05T00:00:00Z,1.00000,1.00030,0.99980,1.00010,1"
    );
}

#[test]
fn the_columns_come_in_the_order_asked_and_can_be_renamed() {
    let f = Fixture::bars(2);
    let o = options(|o| {
        o.columns = vec![ColumnKey::Close, ColumnKey::Time];
        o.renames.insert("close".to_owned(), "Last".to_owned());
        o.header_case = HeaderCase::Upper;
    });
    let text = out(&f, &o);
    assert_eq!(lines(&text)[0], "Last,TIME");
    assert!(lines(&text)[1].starts_with("1.00010,2026-"));
}

#[test]
fn the_delimiter_the_decimal_and_the_line_ending_are_the_users() {
    let f = Fixture::bars(2);
    let o = options(|o| {
        o.delimiter = Delimiter::Semicolon;
        o.decimal = Decimal::Comma;
        o.line_ending = LineEnding::CrLf;
        o.bom = true;
    });
    let text = out(&f, &o);
    assert!(text.starts_with('\u{feff}'));
    assert!(text.contains("\r\n"));
    assert!(text.contains(";1,00000;"), "{text}");
    let custom = options(|o| {
        o.delimiter = Delimiter::Custom;
        o.custom_delimiter = "~".to_owned();
    });
    assert!(out(&f, &custom).contains("~1.00000~"));
    let tab = options(|o| o.delimiter = Delimiter::Tab);
    assert!(out(&f, &tab).contains('\t'));
    assert_eq!(tab.extension(), "tsv");
    assert_eq!(options(|_| {}).extension(), "csv");
    assert_eq!(custom.extension(), "txt");
}

#[test]
fn a_field_that_would_be_misread_is_quoted() {
    assert_eq!(field("a,b", ',', Quote::WhenNeeded), "\"a,b\"");
    assert_eq!(
        field("say \"hi\"", ',', Quote::WhenNeeded),
        "\"say \"\"hi\"\"\""
    );
    assert_eq!(field("plain", ',', Quote::WhenNeeded), "plain");
    assert_eq!(field("plain", ',', Quote::Always), "\"plain\"");
    assert_eq!(field("a,b", ',', Quote::Never), "a,b");
    assert_eq!(field(" edge", ',', Quote::WhenNeeded), "\" edge\"");
    // A decimal comma with a comma delimiter is quoted, so it still reads.
    let f = Fixture::bars(1);
    let o = options(|o| o.decimal = Decimal::Comma);
    assert!(out(&f, &o).contains("\"1,00000\""));
    assert!(!clashes(&o));
    assert!(clashes(&options(|o| {
        o.decimal = Decimal::Comma;
        o.quote = Quote::Never;
    })));
}

#[test]
fn prices_can_be_raw_integers_or_have_their_own_decimals() {
    let f = Fixture::bars(1);
    let raw = options(|o| o.price_digits = PriceDigits::Raw);
    assert!(out(&f, &raw).contains(",100000,100030,99980,100010,"));
    let fixed = options(|o| {
        o.price_digits = PriceDigits::Fixed;
        o.fixed_digits = 2;
    });
    assert!(out(&f, &fixed).contains(",1.00,1.00,1.00,1.00,"));
    let trimmed = options(|o| o.trim_zeros = true);
    assert!(out(&f, &trimmed).contains(",1,1.0003,0.9998,1.0001,"));
}

#[test]
fn a_time_is_written_in_the_format_and_zone_asked() {
    // 09:30:00.123 UTC.
    let ms = MONDAY + 9 * 60 * MIN + 30 * MIN + 123;
    let base = ExportOptions::default();
    let utc = |o: &ExportOptions| time_text(ms, o, Zone::Utc).text;
    assert_eq!(utc(&base), "2026-01-05T09:30:00Z");
    let mut o = base.clone();
    o.millis = true;
    assert_eq!(utc(&o), "2026-01-05T09:30:00.123Z");
    o.time_format = TimeFormat::DateTime;
    assert_eq!(utc(&o), "2026-01-05 09:30:00.123");
    o.millis = false;
    assert_eq!(utc(&o), "2026-01-05 09:30:00");
    o.time_format = TimeFormat::UnixSeconds;
    assert_eq!(utc(&o), "1767605400");
    assert!(time_text(ms, &o, Zone::Utc).numeric);
    o.time_format = TimeFormat::UnixMillis;
    assert_eq!(utc(&o), "1767605400123");
    o.time_format = TimeFormat::Custom;
    o.time_pattern = "%d/%m/%Y %H:%M".to_owned();
    assert_eq!(utc(&o), "05/01/2026 09:30");
    // 2026-01-05 is 46027 days after 1899-12-30, and 09:30 is 0.395835 of a day.
    o.time_format = TimeFormat::Spreadsheet;
    assert_eq!(utc(&o), "46027.395835");
    // New York in January is five hours behind, and ISO says so.
    let york = ExportOptions::default();
    let text = time_text(ms, &york, Zone::Named(chrono_tz::Tz::America__New_York)).text;
    assert_eq!(text, "2026-01-05T04:30:00-05:00");
    let forced = ExportOptions {
        zone: ExportZone::Utc,
        ..ExportOptions::default()
    };
    assert_eq!(
        time_text(ms, &forced, Zone::Named(chrono_tz::Tz::America__New_York)).text,
        "2026-01-05T09:30:00Z"
    );
}

#[test]
fn the_date_and_the_clock_can_be_columns_of_their_own() {
    let f = Fixture::bars(2);
    let o = options(|o| {
        o.columns = vec![ColumnKey::Date, ColumnKey::Clock, ColumnKey::Close];
        o.date_pattern = "%Y.%m.%d".to_owned();
        o.clock_pattern = "%H:%M".to_owned();
    });
    let text = out(&f, &o);
    assert_eq!(lines(&text)[1], "2026.01.05,00:00,1.00010");
    assert_eq!(lines(&text)[2], "2026.01.05,00:05,1.00020");
}

#[test]
fn the_range_can_be_what_is_on_screen_dates_or_the_newest_rows() {
    let f = Fixture::bars(100);
    let mut source = f.source();
    source.visible = (10, 20);
    let o = options(|o| o.range = RangeKind::Visible);
    assert_eq!(table(&source, &o, None).rows.len(), 10);
    let last = options(|o| {
        o.range = RangeKind::Last;
        o.last = 7;
    });
    let t = table(&f.source(), &last, None);
    assert_eq!(t.rows.len(), 7);
    assert_eq!(t.rows[6][0], Cell::Time(MONDAY + 99 * 5 * MIN));
    let dates = options(|o| {
        o.range = RangeKind::Dates;
        o.from = "2026-01-05 01:00".to_owned();
        o.to = "2026-01-05 02:00".to_owned();
    });
    // From 01:00 to 02:00 inclusive, every five minutes: 13 bars.
    assert_eq!(table(&f.source(), &dates, None).rows.len(), 13);
    let day = options(|o| {
        o.range = RangeKind::Dates;
        o.from = "2026-01-05".to_owned();
        o.to = "2026-01-05".to_owned();
    });
    assert_eq!(
        table(&f.source(), &day, None).rows.len(),
        100,
        "a date takes the whole day"
    );
    let open = options(|o| {
        o.range = RangeKind::Dates;
        o.from = "not a date".to_owned();
    });
    assert_eq!(
        table(&f.source(), &open, None).rows.len(),
        100,
        "an open range"
    );
}

#[test]
fn dates_are_read_in_the_zone_of_the_export() {
    let f = Fixture::bars(300);
    let mut source = f.source();
    source.zone = Zone::Named(chrono_tz::Tz::America__New_York);
    // Midnight in New York on the 5th is 05:00 UTC, the 60th bar.
    let o = options(|o| {
        o.range = RangeKind::Dates;
        o.from = "2026-01-05 00:00".to_owned();
        o.to = "2026-01-05 00:00".to_owned();
    });
    let t = table(&source, &o, None);
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.rows[0][0], Cell::Time(MONDAY + 300 * MIN));
    // 19:00 on the 4th in New York is the first bar, at 00:00 UTC.
    let o = options(|o| {
        o.range = RangeKind::Dates;
        o.from = "2026-01-04 19:00".to_owned();
        o.to = "2026-01-04 19:00".to_owned();
    });
    let t = table(&source, &o, None);
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.rows[0][0], Cell::Time(MONDAY));
    // The same dates read as UTC ask for other bars.
    let utc = options(|o| {
        o.zone = ExportZone::Utc;
        o.range = RangeKind::Dates;
        o.from = "2026-01-05 00:00".to_owned();
        o.to = "2026-01-05 00:00".to_owned();
    });
    assert_eq!(table(&source, &utc, None).rows[0][0], Cell::Time(MONDAY));
}

#[test]
fn the_rows_can_be_thinned_ordered_limited_and_the_forming_one_left_out() {
    let f = Fixture::bars(10);
    let every = options(|o| o.every = 3);
    assert_eq!(table(&f.source(), &every, None).rows.len(), 4);
    let newest = options(|o| o.order = Order::NewestFirst);
    let t = table(&f.source(), &newest, None);
    assert_eq!(t.rows[0][0], Cell::Time(MONDAY + 9 * 5 * MIN));
    let limited = options(|o| {
        o.order = Order::NewestFirst;
        o.limit = 2;
    });
    assert_eq!(table(&f.source(), &limited, None).rows.len(), 2);
    // The last bar opened 45 minutes in and lasts five: a minute later it is still forming.
    let mut source = f.source();
    source.now_ms = MONDAY + 9 * 5 * MIN + MIN;
    let skip = options(|o| o.skip_forming = true);
    assert_eq!(table(&source, &skip, None).rows.len(), 9);
    source.now_ms = MONDAY + 10_000 * MIN;
    assert_eq!(table(&source, &skip, None).rows.len(), 10);
}
