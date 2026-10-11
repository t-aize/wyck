//! Writing a table as text: delimited text, JSON, Markdown or SQL.

use super::choices::{Decimal, Format, PriceDigits, Quote, TimeFormat};
use super::columns::ColumnKey;
use super::options::{ExportOptions, identifier, pattern_is_valid};
use super::source::{Cell, Source, Table};
use super::{RAW_DECIMALS, SQL_BATCH};

use crate::domain::chart::zone::Zone;
use chrono::{DateTime, FixedOffset};

pub(super) fn zone_name(zone: Zone, at: i64) -> String {
    zone.label(at)
}

/// A time as the calendar of `zone` reads it, by a `strftime` pattern.
pub(super) fn local_text(time_ms: i64, zone: Zone, pattern: &str) -> String {
    let local = time_ms.saturating_add(zone.offset_ms(time_ms));
    match DateTime::from_timestamp_millis(local) {
        Some(d) if pattern_is_valid(pattern) => d.naive_utc().format(pattern).to_string(),
        _ => String::new(),
    }
}

/// A time in ISO 8601: with `Z` for UTC and the offset of the zone otherwise.
pub(super) fn iso_text(time_ms: i64, zone: Zone, millis: bool) -> String {
    let offset_s = i32::try_from(zone.offset_ms(time_ms) / 1_000).unwrap_or(0);
    let Some(offset) = FixedOffset::east_opt(offset_s) else {
        return String::new();
    };
    let Some(utc) = DateTime::from_timestamp_millis(time_ms) else {
        return String::new();
    };
    let at = utc.with_timezone(&offset);
    let stamp = if millis {
        at.format("%Y-%m-%dT%H:%M:%S%.3f")
    } else {
        at.format("%Y-%m-%dT%H:%M:%S")
    };
    if zone == Zone::Utc || offset_s == 0 {
        format!("{stamp}Z")
    } else {
        format!("{stamp}{}", at.format("%:z"))
    }
}

/// How a time is written, and whether that is a number (for JSON).
pub(super) struct Written {
    pub(super) text: String,
    pub(super) numeric: bool,
}

pub(super) fn time_text(time_ms: i64, options: &ExportOptions, chart_zone: Zone) -> Written {
    let zone = options.zone(chart_zone);
    let (text, numeric) = match options.time_format {
        TimeFormat::Iso8601 => (iso_text(time_ms, zone, options.millis), false),
        TimeFormat::DateTime => (
            local_text(
                time_ms,
                zone,
                if options.millis {
                    "%Y-%m-%d %H:%M:%S%.3f"
                } else {
                    "%Y-%m-%d %H:%M:%S"
                },
            ),
            false,
        ),
        TimeFormat::UnixSeconds => ((time_ms.div_euclid(1_000)).to_string(), true),
        TimeFormat::UnixMillis => (time_ms.to_string(), true),
        TimeFormat::Spreadsheet => {
            let local = time_ms.saturating_add(zone.offset_ms(time_ms));
            // Days from 1899-12-30, which is where a spreadsheet counts from.
            (
                format!("{:.6}", local as f64 / 86_400_000.0 + 25_569.0),
                true,
            )
        }
        TimeFormat::Custom => (local_text(time_ms, zone, &options.time_pattern), false),
    };
    Written { text, numeric }
}

/// A price of the server's raw units, written exactly with `digits` decimals (rounded half away
/// from zero), or as the integer itself.
pub fn price_text(raw: i64, digits: u32, decimal: char, trim: bool) -> String {
    let negative = raw < 0;
    let magnitude = i128::from(raw).unsigned_abs();
    // Bring the raw units to the wanted decimals.
    let wanted = digits.min(20);
    let scaled = if wanted >= RAW_DECIMALS {
        magnitude * 10u128.pow(wanted - RAW_DECIMALS)
    } else {
        let step = 10u128.pow(RAW_DECIMALS - wanted);
        (magnitude + step / 2) / step
    };
    let unit = 10u128.pow(wanted);
    let (whole, part) = (scaled / unit, scaled % unit);
    let mut text = if wanted == 0 {
        whole.to_string()
    } else {
        format!("{whole}{decimal}{part:0width$}", width = wanted as usize)
    };
    if trim && wanted > 0 {
        text = text
            .trim_end_matches('0')
            .trim_end_matches(decimal)
            .to_owned();
    }
    // "-0" is just "0".
    let zero = text.chars().all(|c| c == '0' || c == decimal);
    if negative && !zero {
        format!("-{text}")
    } else {
        text
    }
}

pub(super) fn value_text(value: f64, digits: u32, decimal: char, trim: bool) -> String {
    let mut text = if trim {
        crate::domain::chart::format::trim(value, digits as usize)
    } else {
        format!("{value:.*}", digits as usize)
    };
    if text == "-0" {
        text = "0".to_owned();
    }
    if decimal != '.' {
        text = text.replace('.', &decimal.to_string());
    }
    text
}

/// How many decimals a price is written with.
pub(super) fn price_digits(options: &ExportOptions, symbol_digits: u32) -> Option<u32> {
    match options.price_digits {
        PriceDigits::Symbol => Some(symbol_digits),
        PriceDigits::Fixed => Some(options.fixed_digits),
        PriceDigits::Raw => None,
    }
}

/// A cell as text, and whether JSON writes it as a number. `None` is a value that is not there.
pub(super) struct Writer<'a> {
    pub(super) options: &'a ExportOptions,
    pub(super) digits: u32,
    pub(super) zone: Zone,
    /// The decimal to write: a point in the formats that need one.
    pub(super) decimal: char,
}

pub(super) fn cell_text(cell: &Cell, cx: &Writer<'_>) -> Option<Written> {
    let o = cx.options;
    let plain = |text: String, numeric: bool| Some(Written { text, numeric });
    match cell {
        Cell::Empty => None,
        Cell::Text(text) => plain(text.clone(), false),
        Cell::Int(n) => plain(n.to_string(), true),
        Cell::Price(raw) => match price_digits(o, cx.digits) {
            Some(d) => plain(price_text(*raw, d, cx.decimal, o.trim_zeros), true),
            None => plain(raw.to_string(), true),
        },
        Cell::Value(v) if v.is_finite() => plain(
            value_text(*v, o.value_digits, cx.decimal, o.trim_zeros),
            true,
        ),
        Cell::Value(_) => None,
        Cell::Time(t) => Some(time_text(*t, o, cx.zone)),
        Cell::Date(t) => plain(local_text(*t, o.zone(cx.zone), &o.date_pattern), false),
        Cell::Clock(t) => plain(local_text(*t, o.zone(cx.zone), &o.clock_pattern), false),
    }
}

pub(super) fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

pub(super) fn json_value(cell: &Cell, cx: &Writer<'_>) -> String {
    match cell_text(cell, cx) {
        None => "null".to_owned(),
        Some(w) if w.numeric => w.text,
        Some(w) => json_string(&w.text),
    }
}

/// A field of delimited text, quoted as the options say.
pub(super) fn field(text: &str, delimiter: char, quote: Quote) -> String {
    let needs = text.contains(delimiter)
        || text.contains('"')
        || text.contains('\n')
        || text.contains('\r')
        || text.starts_with(' ')
        || text.ends_with(' ');
    match quote {
        Quote::Always => format!("\"{}\"", text.replace('"', "\"\"")),
        Quote::WhenNeeded if needs => format!("\"{}\"", text.replace('"', "\"\"")),
        _ => text.to_owned(),
    }
}

pub(super) fn sql_literal(cell: &Cell, cx: &Writer<'_>) -> String {
    match cell_text(cell, cx) {
        None => "NULL".to_owned(),
        Some(w) if w.numeric => w.text,
        Some(w) => format!("'{}'", w.text.replace('\'', "''")),
    }
}

/// The table written as text, by the options.
pub fn render(table: &Table, options: &ExportOptions, digits: u32, chart_zone: Zone) -> String {
    let eol = options.line_ending.text();
    // JSON and SQL always use a point; the other formats say which decimal to use.
    let text_decimal = options.decimal.text();
    let cx = |decimal| Writer {
        options,
        digits,
        zone: chart_zone,
        decimal,
    };
    let mut out = String::new();
    if options.bom {
        out.push('\u{feff}');
    }
    let empty = options.empty.text();
    match options.format {
        Format::Delimited => {
            let delimiter = options.delimiter_char();
            let cx = cx(text_decimal);
            if options.notes {
                for (name, value) in &table.notes {
                    out.push_str(&format!("# {name}: {value}{eol}"));
                }
            }
            if options.header {
                let names: Vec<String> = table
                    .columns
                    .iter()
                    .map(|c| field(&c.name, delimiter, options.quote))
                    .collect();
                out.push_str(&names.join(&delimiter.to_string()));
                out.push_str(eol);
            }
            for row in &table.rows {
                let fields: Vec<String> = row
                    .iter()
                    .map(|c| match cell_text(c, &cx) {
                        Some(w) => field(&w.text, delimiter, options.quote),
                        None => empty.to_owned(),
                    })
                    .collect();
                out.push_str(&fields.join(&delimiter.to_string()));
                out.push_str(eol);
            }
        }
        Format::Json | Format::JsonLines => {
            let cx = cx('.');
            let (nl, pad, sep) = if options.json_pretty && options.format == Format::Json {
                (eol, "  ", ",")
            } else {
                ("", "", ",")
            };
            let object = |row: &Vec<Cell>| -> String {
                let members: Vec<String> = table
                    .columns
                    .iter()
                    .zip(row)
                    .map(|(c, cell)| format!("{}:{}", json_string(&c.name), json_value(cell, &cx)))
                    .collect();
                format!("{{{}}}", members.join(sep))
            };
            if options.format == Format::JsonLines {
                for row in &table.rows {
                    out.push_str(&object(row));
                    out.push_str(eol);
                }
            } else {
                let rows: Vec<String> = table.rows.iter().map(object).collect();
                let body = if rows.is_empty() {
                    "[]".to_owned()
                } else {
                    format!("[{nl}{pad}{}{nl}]", rows.join(&format!("{sep}{nl}{pad}")))
                };
                if options.notes {
                    let meta: Vec<String> = table
                        .notes
                        .iter()
                        .map(|(k, v)| format!("{}:{}", json_string(k), json_string(v)))
                        .collect();
                    out.push_str(&format!(
                        "{{{nl}{pad}\"meta\":{{{}}},{nl}{pad}\"data\":{body}{nl}}}",
                        meta.join(sep)
                    ));
                } else {
                    out.push_str(&body);
                }
                out.push_str(eol);
            }
        }
        Format::Markdown => {
            let cx = cx(text_decimal);
            let clean = |s: &str| s.replace('|', "\\|").replace(['\n', '\r'], " ");
            let names: Vec<String> = table.columns.iter().map(|c| clean(&c.name)).collect();
            out.push_str(&format!("| {} |{eol}", names.join(" | ")));
            let aligns: Vec<&str> = table
                .columns
                .iter()
                .map(|c| {
                    if c.key.is_text()
                        || matches!(c.key, ColumnKey::Time | ColumnKey::Date | ColumnKey::Clock)
                    {
                        ":--"
                    } else {
                        "--:"
                    }
                })
                .collect();
            out.push_str(&format!("| {} |{eol}", aligns.join(" | ")));
            for row in &table.rows {
                let cells: Vec<String> = row
                    .iter()
                    .map(|c| cell_text(c, &cx).map_or_else(|| empty.to_owned(), |w| clean(&w.text)))
                    .collect();
                out.push_str(&format!("| {} |{eol}", cells.join(" | ")));
            }
        }
        Format::Sql => {
            let cx = cx('.');
            let name = identifier(&options.table_name, "bars");
            let names: Vec<String> = table
                .columns
                .iter()
                .map(|c| {
                    // The name the user gave, or the code of the column (it has no spaces).
                    let asked = options
                        .renames
                        .get(&c.key.code())
                        .cloned()
                        .unwrap_or_else(|| c.key.code());
                    identifier(&asked, "col")
                })
                .collect();
            if options.notes {
                for (k, v) in &table.notes {
                    out.push_str(&format!("-- {k}: {v}{eol}"));
                }
            }
            for batch in table.rows.chunks(SQL_BATCH) {
                out.push_str(&format!(
                    "INSERT INTO {name} ({}) VALUES{eol}",
                    names.join(", ")
                ));
                let last = batch.len() - 1;
                for (n, row) in batch.iter().enumerate() {
                    let values: Vec<String> = row.iter().map(|c| sql_literal(c, &cx)).collect();
                    out.push_str(&format!(
                        "  ({}){}{eol}",
                        values.join(", "),
                        if n == last { ";" } else { "," }
                    ));
                }
            }
        }
    }
    out
}

/// The name of the file, from the pattern of the options.
pub fn file_name(source: &Source<'_>, options: &ExportOptions, table: &Table) -> String {
    let note = |key: &str| {
        table
            .notes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let day = |text: String| text.split(' ').next().unwrap_or("").replace('-', "");
    let zone = options.zone(source.zone);
    let text = options
        .file_name
        .replace("{symbol}", source.symbol)
        .replace("{timeframe}", source.timeframe)
        .replace("{type}", &note("type").to_lowercase().replace(' ', "-"))
        .replace("{from}", &day(note("from")))
        .replace("{to}", &day(note("to")))
        .replace("{date}", &local_text(source.now_ms, zone, "%Y%m%d"));
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_owned();
    let base = if cleaned.is_empty() {
        "export".to_owned()
    } else {
        cleaned
    };
    format!("{base}.{}", options.extension())
}

/// Whether a row of the export is likely to be misread: a comma as the decimal and as the
/// delimiter of the same file with nothing to quote it.
pub fn clashes(options: &ExportOptions) -> bool {
    options.format == Format::Delimited
        && options.decimal == Decimal::Comma
        && options.delimiter_char() == ','
        && options.quote == Quote::Never
}

#[cfg(test)]
mod tests {

    use super::super::render::*;

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
}
