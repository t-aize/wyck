//! Everything the user set for an export, and the defaults.

use super::choices::{
    Content, Decimal, Delimiter, Empty, ExportZone, Format, HeaderCase, LineEnding, Order,
    PriceDigits, Quote, RangeKind, TimeFormat,
};
use super::columns::ColumnKey;

use crate::chart::zone::Zone;
use chrono::{format::Item, format::StrftimeItems};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every option of an export. Saved as a preset, so every field has a default and everything
/// read is repaired by [`ExportOptions::normalized`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is its own saved switch; grouping them would change the saved settings"
)]
pub struct ExportOptions {
    #[serde(default)]
    pub content: Content,
    #[serde(default)]
    pub range: RangeKind,
    /// The first date of [`RangeKind::Dates`], as `2026-01-05` or `2026-01-05 09:30`. Empty is
    /// the start.
    #[serde(default)]
    pub from: String,
    /// The last date. A date without a time includes that whole day. Empty is the end.
    #[serde(default)]
    pub to: String,
    /// How many rows [`RangeKind::Last`] takes.
    #[serde(default = "default_last")]
    pub last: u32,
    #[serde(default)]
    pub order: Order,
    /// Leave out the newest row when it is still forming.
    #[serde(default)]
    pub skip_forming: bool,
    /// Keep one row in this many.
    #[serde(default = "default_every")]
    pub every: u32,
    /// The most rows written; 0 is no limit.
    #[serde(default)]
    pub limit: u32,
    #[serde(default = "default_columns")]
    pub columns: Vec<ColumnKey>,
    /// The names written for columns, by their code, instead of the default ones.
    #[serde(default)]
    pub renames: BTreeMap<String, String>,
    #[serde(default = "crate::base::defaults::yes")]
    pub header: bool,
    #[serde(default)]
    pub header_case: HeaderCase,
    #[serde(default)]
    pub format: Format,
    #[serde(default)]
    pub delimiter: Delimiter,
    #[serde(default)]
    pub custom_delimiter: String,
    #[serde(default)]
    pub quote: Quote,
    #[serde(default)]
    pub line_ending: LineEnding,
    /// A byte order mark at the start, which some spreadsheets need to read accents.
    #[serde(default)]
    pub bom: bool,
    #[serde(default)]
    pub time_format: TimeFormat,
    #[serde(default = "default_time_pattern")]
    pub time_pattern: String,
    #[serde(default = "default_date_pattern")]
    pub date_pattern: String,
    #[serde(default = "default_clock_pattern")]
    pub clock_pattern: String,
    /// Milliseconds after the seconds, for the ISO and date and time formats.
    #[serde(default)]
    pub millis: bool,
    #[serde(default)]
    pub zone: ExportZone,
    #[serde(default)]
    pub decimal: Decimal,
    #[serde(default)]
    pub price_digits: PriceDigits,
    #[serde(default = "default_fixed")]
    pub fixed_digits: u32,
    /// The decimals of the values of indicators.
    #[serde(default = "default_value_digits")]
    pub value_digits: u32,
    /// Drop the zeros at the end of a decimal part.
    #[serde(default)]
    pub trim_zeros: bool,
    #[serde(default)]
    pub empty: Empty,
    /// Lines before the data that say where it comes from (as comments, or as the `meta` of a
    /// JSON file).
    #[serde(default)]
    pub notes: bool,
    #[serde(default = "crate::base::defaults::yes")]
    pub json_pretty: bool,
    /// The table of SQL inserts.
    #[serde(default = "default_table")]
    pub table_name: String,
    /// The name of the file, with `{symbol}`, `{timeframe}`, `{type}`, `{from}`, `{to}` and
    /// `{date}`.
    #[serde(default = "default_file_name")]
    pub file_name: String,
}

pub(super) fn default_last() -> u32 {
    1_000
}

pub(super) fn default_every() -> u32 {
    1
}

pub(super) fn default_time_pattern() -> String {
    "%Y-%m-%d %H:%M:%S".to_owned()
}

pub(super) fn default_date_pattern() -> String {
    "%Y-%m-%d".to_owned()
}

pub(super) fn default_clock_pattern() -> String {
    "%H:%M:%S".to_owned()
}

pub(super) fn default_fixed() -> u32 {
    5
}

pub(super) fn default_value_digits() -> u32 {
    6
}

pub(super) fn default_table() -> String {
    "bars".to_owned()
}

pub(super) fn default_file_name() -> String {
    "{symbol}_{timeframe}_{from}_{to}".to_owned()
}

pub(super) fn default_columns() -> Vec<ColumnKey> {
    vec![
        ColumnKey::Time,
        ColumnKey::Open,
        ColumnKey::High,
        ColumnKey::Low,
        ColumnKey::Close,
        ColumnKey::Volume,
    ]
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            content: Content::Prices,
            range: RangeKind::Visible,
            from: String::new(),
            to: String::new(),
            last: default_last(),
            order: Order::OldestFirst,
            skip_forming: false,
            every: 1,
            limit: 0,
            columns: default_columns(),
            renames: BTreeMap::new(),
            header: true,
            header_case: HeaderCase::Title,
            format: Format::Delimited,
            delimiter: Delimiter::Comma,
            custom_delimiter: String::new(),
            quote: Quote::WhenNeeded,
            line_ending: LineEnding::Lf,
            bom: false,
            time_format: TimeFormat::Iso8601,
            time_pattern: default_time_pattern(),
            date_pattern: default_date_pattern(),
            clock_pattern: default_clock_pattern(),
            millis: false,
            zone: ExportZone::Chart,
            decimal: Decimal::Point,
            price_digits: PriceDigits::Symbol,
            fixed_digits: default_fixed(),
            value_digits: default_value_digits(),
            trim_zeros: false,
            empty: Empty::Blank,
            notes: false,
            json_pretty: true,
            table_name: default_table(),
            file_name: default_file_name(),
        }
    }
}

/// Whether `pattern` only holds codes `strftime` knows, so writing with it cannot fail.
pub fn pattern_is_valid(pattern: &str) -> bool {
    !pattern.is_empty() && !StrftimeItems::new(pattern).any(|item| matches!(item, Item::Error))
}

/// A name for SQL or a file: letters, digits and underscores only.
pub(super) fn identifier(text: &str, fallback: &str) -> String {
    let cleaned: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        fallback.to_owned()
    } else {
        cleaned
    }
}

impl ExportOptions {
    /// The options repaired: numbers in range, patterns that work, columns not repeated.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.last = self.last.clamp(1, 10_000_000);
        self.every = self.every.clamp(1, 1_000_000);
        self.limit = self.limit.min(100_000_000);
        self.fixed_digits = self.fixed_digits.min(12);
        self.value_digits = self.value_digits.min(12);
        self.custom_delimiter = self.custom_delimiter.chars().take(1).collect();
        if self.time_format == TimeFormat::Custom && !pattern_is_valid(&self.time_pattern) {
            self.time_pattern = default_time_pattern();
        }
        if !pattern_is_valid(&self.date_pattern) {
            self.date_pattern = default_date_pattern();
        }
        if !pattern_is_valid(&self.clock_pattern) {
            self.clock_pattern = default_clock_pattern();
        }
        let mut seen = Vec::new();
        self.columns.retain(|c| {
            let fresh = !seen.contains(c);
            seen.push(*c);
            fresh
        });
        self.renames.retain(|_, name| !name.trim().is_empty());
        self.table_name = identifier(&self.table_name, "bars");
        if self.file_name.trim().is_empty() {
            self.file_name = default_file_name();
        }
        self
    }

    /// The character between fields.
    pub(super) fn delimiter_char(&self) -> char {
        match self.delimiter {
            Delimiter::Comma => ',',
            Delimiter::Semicolon => ';',
            Delimiter::Tab => '\t',
            Delimiter::Pipe => '|',
            Delimiter::Space => ' ',
            Delimiter::Custom => self.custom_delimiter.chars().next().unwrap_or(','),
        }
    }

    pub(crate) fn extension(&self) -> &'static str {
        match self.format {
            Format::Delimited => match self.delimiter {
                Delimiter::Tab => "tsv",
                Delimiter::Comma | Delimiter::Semicolon => "csv",
                _ => "txt",
            },
            Format::Json => "json",
            Format::JsonLines => "jsonl",
            Format::Markdown => "md",
            Format::Sql => "sql",
        }
    }

    /// The zone times are written in.
    pub(super) fn zone(&self, chart: Zone) -> Zone {
        match self.zone {
            ExportZone::Chart => chart,
            ExportZone::Utc => Zone::Utc,
            ExportZone::Computer => Zone::Local,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::export::choices::*;
    use crate::chart::export::columns::*;

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
}
