use wyck_openapi::market::{Bar, Tick};

use super::*;
use crate::app::chart::study::{StudyConfig, StudyKind};
use crate::app::chart::transform::BoxSize;

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
fn prices_are_written_exactly_at_any_number_of_decimals() {
    assert_eq!(price_text(123_456, 5, '.', false), "1.23456");
    assert_eq!(price_text(123_456, 3, '.', false), "1.235");
    assert_eq!(price_text(123_449, 3, '.', false), "1.234");
    assert_eq!(price_text(123_456, 0, '.', false), "1");
    assert_eq!(price_text(150_000, 0, '.', false), "2");
    assert_eq!(price_text(123_456, 7, '.', false), "1.2345600");
    assert_eq!(price_text(100_000, 5, '.', true), "1");
    assert_eq!(price_text(123_400, 5, '.', true), "1.234");
    assert_eq!(price_text(123_456, 5, ',', false), "1,23456");
    assert_eq!(price_text(5, 5, '.', false), "0.00005");
    assert_eq!(price_text(-10, 5, '.', false), "-0.00010");
    // A price that rounds to nothing has no sign.
    assert_eq!(price_text(-1, 3, '.', false), "0.000");
    assert_eq!(price_text(i64::MAX, 5, '.', false), "92233720368547.75807");
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
fn a_pattern_that_cannot_be_written_is_replaced() {
    assert!(pattern_is_valid("%Y-%m-%d"));
    assert!(!pattern_is_valid("%Q"));
    assert!(!pattern_is_valid(""));
    let o = ExportOptions {
        time_format: TimeFormat::Custom,
        time_pattern: "%Q%".to_owned(),
        date_pattern: String::new(),
        ..ExportOptions::default()
    }
    .normalized();
    assert_eq!(o.time_pattern, "%Y-%m-%d %H:%M:%S");
    assert_eq!(o.date_pattern, "%Y-%m-%d");
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

#[test]
fn a_preview_builds_few_rows_and_says_how_many_there_are() {
    let f = Fixture::bars(500);
    let t = table(&f.source(), &options(|_| {}), Some(PREVIEW_ROWS));
    assert_eq!(t.rows.len(), PREVIEW_ROWS);
    assert_eq!(t.total_rows, 500);
}

#[test]
fn the_columns_derived_from_the_prices_are_worked_out() {
    let f = Fixture::bars(3);
    let o = options(|o| {
        o.columns = vec![
            ColumnKey::Index,
            ColumnKey::Change,
            ColumnKey::ChangePercent,
            ColumnKey::Range,
            ColumnKey::Body,
            ColumnKey::Typical,
            ColumnKey::Median,
            ColumnKey::Direction,
            ColumnKey::Symbol,
            ColumnKey::Timeframe,
        ];
        o.value_digits = 4;
    });
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(
        l[0],
        "N,Change,Change %,Range,Body,Typical,Median,Direction,Symbol,Timeframe"
    );
    // The first row has nothing before it. Its typical price is 300020 / 3 in whole raw units.
    assert_eq!(l[1], "1,,,0.00050,0.00010,1.00006,1.00005,up,EURUSD,M5");
    // 10 raw units on a close of 100010 is 0.01 percent.
    assert_eq!(
        l[2],
        "2,0.00010,0.0100,0.00050,0.00010,1.00016,1.00015,up,EURUSD,M5"
    );
}

#[test]
fn ticks_export_a_price_and_no_volume() {
    let ticks: Vec<Tick> = (0..5)
        .map(|i| Tick {
            time_ms: MONDAY + i * 1_000,
            price: 100_000 + i * 5,
        })
        .collect();
    let f = Fixture::new(Series::Ticks(ticks), ChartSettings::default());
    let source = f.source();
    let a = source.available(Content::Prices);
    assert!(a.contains(&ColumnKey::Price));
    assert!(!a.contains(&ColumnKey::Volume) && !a.contains(&ColumnKey::Open));
    let o = options(|o| {
        o.columns = vec![ColumnKey::Time, ColumnKey::Price];
        o.millis = true;
    });
    let text = out(&f, &o);
    assert_eq!(lines(&text)[1], "2026-01-05T00:00:00.000Z,1.00000");
    assert_eq!(lines(&text)[3], "2026-01-05T00:00:02.000Z,1.00010");
}

#[test]
fn indicator_lines_are_columns_written_like_the_prices_they_follow() {
    let mut settings = ChartSettings::default();
    settings.studies.push(StudyConfig::new(StudyKind::Sma));
    settings.studies.push(StudyConfig::new(StudyKind::Rsi));
    let f = Fixture::new(Series::Bars(bars(60)), settings);
    let source = f.source();
    let available = source.available(Content::Prices);
    let studies: Vec<ColumnKey> = available
        .iter()
        .copied()
        .filter(|k| matches!(k, ColumnKey::Study { .. }))
        .collect();
    assert!(studies.len() >= 2, "{studies:?}");
    let sma = studies[0];
    assert!(
        source.column_name(sma).starts_with("SMA"),
        "{}",
        source.column_name(sma)
    );
    let o = options(|o| {
        o.columns = vec![ColumnKey::Index, sma, studies[1]];
        o.value_digits = 2;
        o.empty = Empty::Nan;
    });
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(l[1], "1,NaN,NaN", "no average yet on the first bar");
    let last: Vec<&str> = l.last().unwrap().split(',').collect();
    // The moving average is a price and reads like one; the RSI is a plain number.
    let average: f64 = last[1].parse().unwrap();
    assert!((0.99..1.01).contains(&average), "{last:?}");
    assert!(
        last[1].split('.').nth(1).is_some_and(|d| d.len() == 5),
        "{last:?}"
    );
    let rsi: f64 = last[2].parse().unwrap();
    assert!((0.0..=100.0).contains(&rsi), "{last:?}");
    assert!(
        last[2].split('.').nth(1).is_some_and(|d| d.len() == 2),
        "{last:?}"
    );
}

#[test]
fn the_codes_of_the_columns_are_a_file_format() {
    for code in [
        "time",
        "date",
        "clock",
        "open",
        "high",
        "low",
        "close",
        "price",
        "volume",
        "change",
        "change_percent",
        "range",
        "body",
        "typical",
        "median",
        "direction",
        "index",
        "symbol",
        "timeframe",
        "chart_type",
        "box_size",
        "poc",
        "value_area_high",
        "value_area_low",
        "initial_balance_high",
        "initial_balance_low",
        "marks",
        "periods",
        "poor_high",
        "poor_low",
        "thickness",
        "glyph",
        "boxes",
    ] {
        let key = ColumnKey::from_code(code).unwrap_or_else(|| panic!("{code}"));
        assert_eq!(key.code(), code);
    }
    assert_eq!(
        ColumnKey::from_code("study.2.1"),
        Some(ColumnKey::Study { study: 2, plot: 1 })
    );
    assert_eq!(ColumnKey::from_code("study.x.1"), None);
    assert_eq!(ColumnKey::from_code("nonsense"), None);
    assert_eq!(ColumnKey::Study { study: 0, plot: 3 }.code(), "study.0.3");
}

#[test]
fn a_renko_chart_can_export_its_bricks_with_their_size_and_direction() {
    let mut settings = ChartSettings {
        kind: ChartKind::Renko,
        ..ChartSettings::default()
    };
    settings.transform.renko_box = BoxSize::Fixed { price: 0.0002 };
    let f = Fixture::new(Series::Bars(bars(300)), settings);
    let source = f.source();
    let shown = source.available(Content::Shown);
    assert!(shown.contains(&ColumnKey::BoxSize));
    assert!(
        !source
            .available(Content::Prices)
            .contains(&ColumnKey::BoxSize)
    );
    let bricks = f.display.shown(&f.raw).len();
    let o = options(|o| {
        o.content = Content::Shown;
        o.columns = vec![ColumnKey::Time, ColumnKey::Direction, ColumnKey::BoxSize];
    });
    let text = out(&f, &o);
    assert_eq!(lines(&text).len(), bricks + 1);
    assert!(lines(&text)[1].ends_with(",0.00020"), "{}", lines(&text)[1]);
    assert!(text.contains(",up,") && text.contains(",down,"));
    // The prices are still the bars.
    assert_eq!(table(&source, &options(|_| {}), None).rows.len(), 300);
}

#[test]
fn a_tpo_chart_exports_the_levels_of_each_session() {
    let mut settings = ChartSettings {
        kind: ChartKind::Tpo,
        zone: Zone::Utc,
        ..ChartSettings::default()
    };
    settings.tpo.row_units = 10;
    let f = Fixture::new(Series::Bars(bars(600)), settings);
    let source = f.source();
    assert!(source.available(Content::Shown).contains(&ColumnKey::Poc));
    let o = options(|o| {
        o.content = Content::Shown;
        o.columns = vec![
            ColumnKey::Time,
            ColumnKey::Poc,
            ColumnKey::ValueAreaHigh,
            ColumnKey::ValueAreaLow,
            ColumnKey::InitialBalanceHigh,
            ColumnKey::Marks,
            ColumnKey::Periods,
            ColumnKey::PoorHigh,
            ColumnKey::BoxSize,
        ];
    });
    let t = table(&source, &o, None);
    assert_eq!(t.rows.len(), f.display.tpo.len());
    assert!(!t.rows.is_empty());
    for row in &t.rows {
        assert!(row.iter().all(|c| *c != Cell::Empty), "{row:?}");
        let (Cell::Price(poc), Cell::Price(vah), Cell::Price(val)) = (&row[1], &row[2], &row[3])
        else {
            panic!("{row:?}");
        };
        assert!(val <= poc && poc <= vah, "{val} {poc} {vah}");
    }
}

#[test]
fn the_json_is_valid_and_typed() {
    let f = Fixture::bars(3);
    let o = options(|o| {
        o.format = Format::Json;
        o.header_case = HeaderCase::Lower;
        o.columns = vec![
            ColumnKey::Time,
            ColumnKey::Close,
            ColumnKey::Volume,
            ColumnKey::Direction,
            ColumnKey::Change,
        ];
    });
    let text = out(&f, &o);
    let value: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    let rows = value.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["time"], "2026-01-05T00:00:00Z");
    assert_eq!(rows[0]["close"], 1.0001);
    assert_eq!(rows[0]["volume"], 1);
    assert_eq!(rows[0]["direction"], "up");
    assert!(rows[0]["change"].is_null(), "nothing before the first bar");
    let unix = options(|o| {
        o.format = Format::Json;
        o.time_format = TimeFormat::UnixSeconds;
        o.decimal = Decimal::Comma;
        o.notes = true;
    });
    let value: serde_json::Value = serde_json::from_str(&out(&f, &unix)).unwrap();
    assert_eq!(value["meta"]["symbol"], "EURUSD");
    assert_eq!(value["data"][0]["Time"], 1_767_571_200);
    assert_eq!(
        value["data"][0]["Close"], 1.0001,
        "JSON always uses a point"
    );
    let compact = options(|o| {
        o.format = Format::Json;
        o.json_pretty = false;
    });
    assert_eq!(lines(&out(&f, &compact)).len(), 1);
}

#[test]
fn json_lines_are_one_object_a_line() {
    let f = Fixture::bars(4);
    let o = options(|o| o.format = Format::JsonLines);
    let text = out(&f, &o);
    assert_eq!(lines(&text).len(), 4);
    for line in lines(&text) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(v.is_object());
    }
    assert_eq!(o.extension(), "jsonl");
    let empty = options(|o| o.format = Format::Json);
    let none = Fixture::new(Series::Bars(Vec::new()), ChartSettings::default());
    assert_eq!(out(&none, &empty).trim(), "[]");
}

#[test]
fn the_markdown_table_has_a_header_an_alignment_row_and_escaped_pipes() {
    let f = Fixture::bars(2);
    let o = options(|o| {
        o.format = Format::Markdown;
        o.columns = vec![ColumnKey::Time, ColumnKey::Close, ColumnKey::Direction];
        o.renames
            .insert("close".to_owned(), "Last | price".to_owned());
    });
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(l[0], "| Time | Last \\| price | Direction |");
    assert_eq!(l[1], "| :-- | --: | :-- |");
    assert!(l[2].starts_with("| 2026-01-05T00:00:00Z | 1.00010 | up |"));
    assert_eq!(o.extension(), "md");
}

#[test]
fn the_sql_is_batched_quoted_and_uses_the_table_asked() {
    let f = Fixture::bars(SQL_BATCH + 3);
    let o = options(|o| {
        o.format = Format::Sql;
        o.table_name = "my bars".to_owned();
        o.columns = vec![
            ColumnKey::Time,
            ColumnKey::Close,
            ColumnKey::Change,
            ColumnKey::Symbol,
        ];
        o.renames.insert("symbol".to_owned(), "sym".to_owned());
        o.notes = true;
    });
    let text = out(&f, &o);
    assert!(text.contains("-- symbol: EURUSD"));
    assert_eq!(
        text.matches("INSERT INTO my_bars (time, close, change, sym) VALUES")
            .count(),
        2
    );
    assert!(text.contains("('2026-01-05T00:00:00Z', 1.00010, NULL, 'EURUSD'),"));
    assert_eq!(
        text.matches("EURUSD'),\n").count() + text.matches("EURUSD');\n").count(),
        SQL_BATCH + 3
    );
    assert_eq!(text.matches("');\n").count(), 2, "each statement ends once");
    assert_eq!(identifier("9lives", "x"), "x");
    assert_eq!(identifier("a b-c", "x"), "a_b_c");
    // A quote in a value is doubled.
    let quoted = Cell::Text("O'Brien".to_owned());
    let writer = Writer {
        options: &o,
        digits: 5,
        zone: Zone::Utc,
        decimal: '.',
    };
    assert_eq!(sql_literal(&quoted, &writer), "'O''Brien'");
}

#[test]
fn notes_open_a_delimited_file_as_comments() {
    let f = Fixture::bars(3);
    let o = options(|o| o.notes = true);
    let text = out(&f, &o);
    let l = lines(&text);
    assert_eq!(l[0], "# symbol: EURUSD");
    assert!(l.contains(&"# timeframe: M5"));
    assert!(l.contains(&"# rows: 3"));
    assert!(l.contains(&"# from: 2026-01-05 00:00:00"));
    assert!(l.iter().any(|x| x.starts_with("# zone: UTC")));
}

#[test]
fn what_is_not_there_is_written_as_asked() {
    let f = Fixture::bars(2);
    for (empty, expected) in [
        (Empty::Blank, "1,,"),
        (Empty::Nan, "1,NaN,NaN"),
        (Empty::Null, "1,null,null"),
        (Empty::Zero, "1,0,0"),
    ] {
        let o = options(|o| {
            o.columns = vec![
                ColumnKey::Index,
                ColumnKey::Change,
                ColumnKey::ChangePercent,
            ];
            o.empty = empty;
            o.header = false;
        });
        assert_eq!(lines(&out(&f, &o))[0], expected, "{empty:?}");
    }
}

#[test]
fn file_names_are_built_from_the_pattern_and_kept_safe() {
    let f = Fixture::bars(10);
    let source = f.source();
    let o = options(|o| o.file_name = "{symbol}-{timeframe}_{type}_{from}_{to}".to_owned());
    let t = table(&source, &o, None);
    assert_eq!(
        file_name(&source, &o, &t),
        "EURUSD-M5_prices_20260105_20260105.csv"
    );
    let dated = options(|o| {
        o.file_name = "{date}/..\\x:y".to_owned();
        o.format = Format::Json;
    });
    let t = table(&source, &dated, None);
    let name = file_name(&source, &dated, &t);
    assert!(
        !name.contains('/') && !name.contains('\\') && !name.contains(':'),
        "{name}"
    );
    assert!(name.ends_with(".json"));
    let empty = ExportOptions {
        file_name: "  ".to_owned(),
        ..ExportOptions::default()
    }
    .normalized();
    assert_eq!(empty.file_name, "{symbol}_{timeframe}_{from}_{to}");
}

#[test]
fn every_built_in_preset_writes_something_and_survives_being_saved() {
    let f = Fixture::bars(20);
    let presets = builtin_presets();
    assert!(presets.len() >= 6);
    let mut names: Vec<_> = presets.iter().map(|p| p.name.clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), presets.len());
    for preset in &presets {
        let o = preset.options.clone().normalized();
        let text = out(&f, &o);
        assert!(!text.trim().is_empty(), "{}", preset.name);
        let saved = toml::to_string(preset).unwrap();
        let back: Preset = toml::from_str(&saved).unwrap();
        assert_eq!(back.options.normalized(), o, "{}", preset.name);
    }
}

#[test]
fn the_trading_platform_presets_write_what_those_platforms_read() {
    let f = Fixture::bars(2);
    let find = |name: &str| {
        builtin_presets()
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
            .options
            .normalized()
    };
    let nt = out(&f, &find("NinjaTrader"));
    assert_eq!(
        lines(&nt)[0],
        "20260105 000000;1.00000;1.00030;0.99980;1.00010;1"
    );
    let mt = out(&f, &find("MetaTrader"));
    assert_eq!(
        lines(&mt)[0],
        "2026.01.05,00:00,1.00000,1.00030,0.99980,1.00010,1"
    );
    let sheet = out(&f, &find("Spreadsheet"));
    assert!(sheet.starts_with('\u{feff}'));
    assert!(
        lines(&sheet)[1].starts_with("2026-01-05 00:00:00;1,00000;"),
        "{sheet}"
    );
}

#[test]
fn options_from_an_old_or_broken_file_are_repaired() {
    let empty: ExportOptions = toml::from_str("").unwrap();
    assert_eq!(empty, ExportOptions::default());
    let broken: ExportOptions = toml::from_str(
        "every = 0\nlast = 0\nfixed_digits = 99\ncustom_delimiter = \"abc\"\ncolumns = [\"open\", \"open\", \"wat\", \"study.1.0\"]\n[renames]\nopen = \" \"\n",
    )
    .unwrap();
    let broken = broken.normalized();
    assert_eq!(broken.every, 1);
    assert_eq!(broken.last, 1);
    assert_eq!(broken.fixed_digits, 12);
    assert_eq!(broken.custom_delimiter, "a");
    // The repeat of open is gone; the unknown one reads as the time.
    assert_eq!(
        broken.columns,
        vec![
            ColumnKey::Open,
            ColumnKey::Time,
            ColumnKey::Study { study: 1, plot: 0 }
        ]
    );
    assert!(broken.renames.is_empty());
}

#[test]
fn nothing_loaded_exports_only_the_header() {
    let f = Fixture::new(Series::Bars(Vec::new()), ChartSettings::default());
    let text = out(&f, &options(|_| {}));
    assert_eq!(lines(&text), vec!["Time,Open,High,Low,Close,Volume"]);
    let sql = out(&f, &options(|o| o.format = Format::Sql));
    assert!(sql.is_empty());
}

#[test]
fn the_notes_of_a_preview_describe_every_row_not_only_the_ones_built() {
    let f = Fixture::bars(100);
    let o = options(|_| {});
    let full = table(&f.source(), &o, None);
    let preview = table(&f.source(), &o, Some(3));
    assert_eq!(preview.rows.len(), 3);
    assert_eq!(preview.notes, full.notes);
    let name = |t: &Table| file_name(&f.source(), &o, t);
    assert_eq!(name(&preview), name(&full));
}

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
