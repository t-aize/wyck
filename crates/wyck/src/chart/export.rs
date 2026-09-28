//! Exporting what a chart holds: its prices (bars or ticks), what its chart type draws (the
//! bricks, columns, lines and profiles of a non-time chart), and the values of its indicators.
//!
//! The work is in two steps, both plain data in and out so they are tested without a window:
//!
//! 1. [`table`] picks the rows (what is on screen, everything loaded, a range of dates, the last
//!    few), the columns (in the order asked, with the names asked) and turns them into a
//!    [`Table`] of typed [`Cell`]s.
//! 2. [`render`] writes the table as text, by [`ExportOptions`]: delimited text with any
//!    delimiter, JSON, JSON lines, a Markdown table or SQL inserts, with times and numbers written
//!    the way the options say.
//!
//! Prices are the server's raw integers until the very end, and are written by integer
//! arithmetic, so a price never picks up the error of a float.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset, NaiveDateTime, format::Item, format::StrftimeItems};
use serde::{Deserialize, Serialize};

use super::data::Series;
use super::display::Display;
use super::settings::{ChartKind, ChartSettings};
use super::study::ValueFormat;
use super::zone::Zone;

/// How many rows a preview shows.
pub const PREVIEW_ROWS: usize = 12;
/// The decimals of the raw price scale.
const RAW_DECIMALS: u32 = 5;
/// Rows of one SQL insert.
const SQL_BATCH: usize = 500;

// ---- the options ----

/// What is exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Content {
    /// The prices as the chart holds them: bars, or ticks.
    #[default]
    Prices,
    /// What the chart type draws: the bricks of a Renko chart, the columns of point and figure,
    /// the profiles of a TPO chart. The same as the prices for a chart with a bar for each period.
    Shown,
}

impl Content {
    pub const ALL: [Self; 2] = [Self::Prices, Self::Shown];
}

/// Which rows are exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangeKind {
    /// What is on screen.
    #[default]
    Visible,
    /// Everything the chart has loaded.
    Loaded,
    /// From one date to another.
    Dates,
    /// The newest rows.
    Last,
}

impl RangeKind {
    pub const ALL: [Self; 4] = [Self::Visible, Self::Loaded, Self::Dates, Self::Last];

    pub fn label(self) -> &'static str {
        match self {
            Self::Visible => "On screen",
            Self::Loaded => "Everything loaded",
            Self::Dates => "Dates",
            Self::Last => "Newest rows",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    #[default]
    OldestFirst,
    NewestFirst,
}

impl Order {
    pub const ALL: [Self; 2] = [Self::OldestFirst, Self::NewestFirst];

    pub fn label(self) -> &'static str {
        match self {
            Self::OldestFirst => "Oldest first",
            Self::NewestFirst => "Newest first",
        }
    }
}

/// How the file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    /// Text with a delimiter between the fields: CSV and its relatives.
    #[default]
    Delimited,
    Json,
    /// One JSON object a line.
    JsonLines,
    Markdown,
    /// `INSERT` statements.
    Sql,
}

impl Format {
    pub const ALL: [Self; 5] = [
        Self::Delimited,
        Self::Json,
        Self::JsonLines,
        Self::Markdown,
        Self::Sql,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Delimited => "CSV / text",
            Self::Json => "JSON",
            Self::JsonLines => "JSON lines",
            Self::Markdown => "Markdown",
            Self::Sql => "SQL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delimiter {
    #[default]
    Comma,
    Semicolon,
    Tab,
    Pipe,
    Space,
    /// The first character of [`ExportOptions::custom_delimiter`].
    Custom,
}

impl Delimiter {
    pub const ALL: [Self; 6] = [
        Self::Comma,
        Self::Semicolon,
        Self::Tab,
        Self::Pipe,
        Self::Space,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Comma => "Comma",
            Self::Semicolon => "Semicolon",
            Self::Tab => "Tab",
            Self::Pipe => "Pipe",
            Self::Space => "Space",
            Self::Custom => "Other",
        }
    }
}

/// When a field is put in quotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quote {
    /// Only what would be misread otherwise: a field with the delimiter, a quote or a line break.
    #[default]
    WhenNeeded,
    Always,
    /// Never: the reader must cope.
    Never,
}

impl Quote {
    pub const ALL: [Self; 3] = [Self::WhenNeeded, Self::Always, Self::Never];

    pub fn label(self) -> &'static str {
        match self {
            Self::WhenNeeded => "When needed",
            Self::Always => "Always",
            Self::Never => "Never",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    pub const ALL: [Self; 2] = [Self::Lf, Self::CrLf];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "Unix (LF)",
            Self::CrLf => "Windows (CRLF)",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// How a time is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeFormat {
    /// `2026-01-05T09:30:00Z`, or with the offset of the zone.
    #[default]
    Iso8601,
    /// `2026-01-05 09:30:00`.
    DateTime,
    UnixSeconds,
    UnixMillis,
    /// Days since 1900 as a number, which a spreadsheet reads as a date.
    Spreadsheet,
    /// The pattern of [`ExportOptions::time_pattern`], with the codes of `strftime`.
    Custom,
}

impl TimeFormat {
    pub const ALL: [Self; 6] = [
        Self::Iso8601,
        Self::DateTime,
        Self::UnixSeconds,
        Self::UnixMillis,
        Self::Spreadsheet,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Iso8601 => "ISO 8601",
            Self::DateTime => "Date and time",
            Self::UnixSeconds => "Unix seconds",
            Self::UnixMillis => "Unix milliseconds",
            Self::Spreadsheet => "Spreadsheet serial",
            Self::Custom => "Pattern",
        }
    }
}

/// The time zone times are written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportZone {
    /// The zone of the chart.
    #[default]
    Chart,
    Utc,
    /// The zone of the computer.
    Computer,
}

impl ExportZone {
    pub const ALL: [Self; 3] = [Self::Chart, Self::Utc, Self::Computer];

    pub fn label(self) -> &'static str {
        match self {
            Self::Chart => "Chart's zone",
            Self::Utc => "UTC",
            Self::Computer => "Computer's zone",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decimal {
    #[default]
    Point,
    Comma,
}

impl Decimal {
    pub const ALL: [Self; 2] = [Self::Point, Self::Comma];

    pub fn label(self) -> &'static str {
        match self {
            Self::Point => "Point (1.2345)",
            Self::Comma => "Comma (1,2345)",
        }
    }

    fn text(self) -> char {
        match self {
            Self::Point => '.',
            Self::Comma => ',',
        }
    }
}

/// How prices are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceDigits {
    /// The decimals of the symbol.
    #[default]
    Symbol,
    /// [`ExportOptions::fixed_digits`] decimals.
    Fixed,
    /// The server's own integers, without a decimal point.
    Raw,
}

impl PriceDigits {
    pub const ALL: [Self; 3] = [Self::Symbol, Self::Fixed, Self::Raw];

    pub fn label(self) -> &'static str {
        match self {
            Self::Symbol => "Symbol's decimals",
            Self::Fixed => "Fixed decimals",
            Self::Raw => "Raw integers",
        }
    }
}

/// What is written for a value that is not there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Empty {
    #[default]
    Blank,
    Nan,
    Null,
    Zero,
}

impl Empty {
    pub const ALL: [Self; 4] = [Self::Blank, Self::Nan, Self::Null, Self::Zero];

    pub fn label(self) -> &'static str {
        match self {
            Self::Blank => "Nothing",
            Self::Nan => "NaN",
            Self::Null => "null",
            Self::Zero => "0",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Blank => "",
            Self::Nan => "NaN",
            Self::Null => "null",
            Self::Zero => "0",
        }
    }
}

/// The case of the names in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderCase {
    #[default]
    Title,
    Lower,
    Upper,
}

impl HeaderCase {
    pub const ALL: [Self; 3] = [Self::Title, Self::Lower, Self::Upper];

    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Open",
            Self::Lower => "open",
            Self::Upper => "OPEN",
        }
    }

    fn apply(self, name: &str) -> String {
        match self {
            Self::Title => name.to_owned(),
            Self::Lower => name.to_lowercase(),
            Self::Upper => name.to_uppercase(),
        }
    }
}

/// Every option of an export. Saved as a preset, so every field has a default and everything
/// read is repaired by [`ExportOptions::normalized`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default = "yes")]
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
    #[serde(default = "yes")]
    pub json_pretty: bool,
    /// The table of SQL inserts.
    #[serde(default = "default_table")]
    pub table_name: String,
    /// The name of the file, with `{symbol}`, `{timeframe}`, `{type}`, `{from}`, `{to}` and
    /// `{date}`.
    #[serde(default = "default_file_name")]
    pub file_name: String,
}

fn yes() -> bool {
    true
}

fn default_last() -> u32 {
    1_000
}

fn default_every() -> u32 {
    1
}

fn default_time_pattern() -> String {
    "%Y-%m-%d %H:%M:%S".to_owned()
}

fn default_date_pattern() -> String {
    "%Y-%m-%d".to_owned()
}

fn default_clock_pattern() -> String {
    "%H:%M:%S".to_owned()
}

fn default_fixed() -> u32 {
    5
}

fn default_value_digits() -> u32 {
    6
}

fn default_table() -> String {
    "bars".to_owned()
}

fn default_file_name() -> String {
    "{symbol}_{timeframe}_{from}_{to}".to_owned()
}

fn default_columns() -> Vec<ColumnKey> {
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
fn identifier(text: &str, fallback: &str) -> String {
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
    fn delimiter_char(&self) -> char {
        match self.delimiter {
            Delimiter::Comma => ',',
            Delimiter::Semicolon => ';',
            Delimiter::Tab => '\t',
            Delimiter::Pipe => '|',
            Delimiter::Space => ' ',
            Delimiter::Custom => self.custom_delimiter.chars().next().unwrap_or(','),
        }
    }

    /// The extension a file of these options gets.
    pub fn extension(&self) -> &'static str {
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
    fn zone(&self, chart: Zone) -> Zone {
        match self.zone {
            ExportZone::Chart => chart,
            ExportZone::Utc => Zone::Utc,
            ExportZone::Computer => Zone::Local,
        }
    }
}

// ---- the columns ----

/// A column that can be exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKey {
    Time,
    /// The date alone, by the date pattern.
    Date,
    /// The time of day alone, by the clock pattern.
    Clock,
    Open,
    High,
    Low,
    Close,
    /// The price of a tick.
    Price,
    Volume,
    /// The close against the close of the row before, in price.
    Change,
    ChangePercent,
    /// High minus low.
    Range,
    /// Close minus open.
    Body,
    /// The average of the high, low and close.
    Typical,
    /// The average of the high and low.
    Median,
    /// `up`, `down` or `flat`, by the close against the open.
    Direction,
    /// The number of the row, from 1.
    Index,
    Symbol,
    Timeframe,
    ChartType,
    /// The size of a box, reversal, range or row of the chart type, in price.
    BoxSize,
    // The levels of a TPO profile.
    Poc,
    ValueAreaHigh,
    ValueAreaLow,
    InitialBalanceHigh,
    InitialBalanceLow,
    Marks,
    Periods,
    PoorHigh,
    PoorLow,
    /// `yang` or `yin`, for the line of a Kagi chart.
    Thickness,
    /// `X` or `O`, for the column of a point and figure chart.
    Glyph,
    /// How many boxes a point and figure column has.
    Boxes,
    /// One line of an indicator: the position of the indicator in the chart, then of the line.
    Study {
        study: usize,
        plot: usize,
    },
}

impl ColumnKey {
    /// The name written in saved presets.
    pub fn code(self) -> String {
        match self {
            Self::Time => "time".into(),
            Self::Date => "date".into(),
            Self::Clock => "clock".into(),
            Self::Open => "open".into(),
            Self::High => "high".into(),
            Self::Low => "low".into(),
            Self::Close => "close".into(),
            Self::Price => "price".into(),
            Self::Volume => "volume".into(),
            Self::Change => "change".into(),
            Self::ChangePercent => "change_percent".into(),
            Self::Range => "range".into(),
            Self::Body => "body".into(),
            Self::Typical => "typical".into(),
            Self::Median => "median".into(),
            Self::Direction => "direction".into(),
            Self::Index => "index".into(),
            Self::Symbol => "symbol".into(),
            Self::Timeframe => "timeframe".into(),
            Self::ChartType => "chart_type".into(),
            Self::BoxSize => "box_size".into(),
            Self::Poc => "poc".into(),
            Self::ValueAreaHigh => "value_area_high".into(),
            Self::ValueAreaLow => "value_area_low".into(),
            Self::InitialBalanceHigh => "initial_balance_high".into(),
            Self::InitialBalanceLow => "initial_balance_low".into(),
            Self::Marks => "marks".into(),
            Self::Periods => "periods".into(),
            Self::PoorHigh => "poor_high".into(),
            Self::PoorLow => "poor_low".into(),
            Self::Thickness => "thickness".into(),
            Self::Glyph => "glyph".into(),
            Self::Boxes => "boxes".into(),
            Self::Study { study, plot } => format!("study.{study}.{plot}"),
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        const FIXED: [ColumnKey; 33] = [
            ColumnKey::Time,
            ColumnKey::Date,
            ColumnKey::Clock,
            ColumnKey::Open,
            ColumnKey::High,
            ColumnKey::Low,
            ColumnKey::Close,
            ColumnKey::Price,
            ColumnKey::Volume,
            ColumnKey::Change,
            ColumnKey::ChangePercent,
            ColumnKey::Range,
            ColumnKey::Body,
            ColumnKey::Typical,
            ColumnKey::Median,
            ColumnKey::Direction,
            ColumnKey::Index,
            ColumnKey::Symbol,
            ColumnKey::Timeframe,
            ColumnKey::ChartType,
            ColumnKey::BoxSize,
            ColumnKey::Poc,
            ColumnKey::ValueAreaHigh,
            ColumnKey::ValueAreaLow,
            ColumnKey::InitialBalanceHigh,
            ColumnKey::InitialBalanceLow,
            ColumnKey::Marks,
            ColumnKey::Periods,
            ColumnKey::PoorHigh,
            ColumnKey::PoorLow,
            ColumnKey::Thickness,
            ColumnKey::Glyph,
            ColumnKey::Boxes,
        ];
        if let Some(rest) = code.strip_prefix("study.") {
            let (study, plot) = rest.split_once('.')?;
            return Some(Self::Study {
                study: study.parse().ok()?,
                plot: plot.parse().ok()?,
            });
        }
        FIXED.into_iter().find(|key| key.code() == code)
    }

    /// The name of the column when the user has not chosen one.
    pub fn default_name(self) -> String {
        match self {
            Self::Time => "Time".into(),
            Self::Date => "Date".into(),
            Self::Clock => "Clock".into(),
            Self::Open => "Open".into(),
            Self::High => "High".into(),
            Self::Low => "Low".into(),
            Self::Close => "Close".into(),
            Self::Price => "Price".into(),
            Self::Volume => "Volume".into(),
            Self::Change => "Change".into(),
            Self::ChangePercent => "Change %".into(),
            Self::Range => "Range".into(),
            Self::Body => "Body".into(),
            Self::Typical => "Typical".into(),
            Self::Median => "Median".into(),
            Self::Direction => "Direction".into(),
            Self::Index => "N".into(),
            Self::Symbol => "Symbol".into(),
            Self::Timeframe => "Timeframe".into(),
            Self::ChartType => "Type".into(),
            Self::BoxSize => "Box size".into(),
            Self::Poc => "POC".into(),
            Self::ValueAreaHigh => "VAH".into(),
            Self::ValueAreaLow => "VAL".into(),
            Self::InitialBalanceHigh => "IB high".into(),
            Self::InitialBalanceLow => "IB low".into(),
            Self::Marks => "Marks".into(),
            Self::Periods => "Periods".into(),
            Self::PoorHigh => "Poor high".into(),
            Self::PoorLow => "Poor low".into(),
            Self::Thickness => "Thickness".into(),
            Self::Glyph => "Glyph".into(),
            Self::Boxes => "Boxes".into(),
            Self::Study { study, plot } => format!("Study {} line {}", study + 1, plot + 1),
        }
    }

    /// What kind of value the column holds, for the formats that tell them apart.
    fn is_text(self) -> bool {
        matches!(
            self,
            Self::Direction
                | Self::Symbol
                | Self::Timeframe
                | Self::ChartType
                | Self::Thickness
                | Self::Glyph
        )
    }
}

impl Serialize for ColumnKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.code())
    }
}

impl<'de> Deserialize<'de> for ColumnKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        // A column an older or newer version does not know is read as the time, and dropped
        // as a repeat by [`ExportOptions::normalized`] when the time is already there.
        Ok(Self::from_code(&code).unwrap_or(Self::Time))
    }
}

// ---- the source ----

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
struct Point {
    time: i64,
    open: i64,
    high: i64,
    low: i64,
    close: i64,
    volume: i64,
}

impl Source<'_> {
    fn series(&self, content: Content) -> &Series {
        match content {
            Content::Prices => self.raw,
            Content::Shown => self.display.shown(self.raw),
        }
    }

    fn items(&self, content: Content) -> Vec<Point> {
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

    fn is_ticks(&self, content: Content) -> bool {
        matches!(self.series(content), Series::Ticks(_))
    }

    /// Whether the rows exported are elements the chart type laid out, not the prices.
    fn is_derived(&self, content: Content) -> bool {
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

// ---- the table ----

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

/// A column of a table.
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

/// The first and last time of a range of dates, in Unix milliseconds, or `None` for a side that is
/// open (or a text that is not a date).
/// A date as typed: `2026-01-05`, `2026-01-05 09:30` or with seconds, and whether it was a date
/// alone (a whole day).
fn parse_local(text: &str) -> Option<(NaiveDateTime, bool)> {
    let text = text.trim().replace('T', " ");
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"]
        .iter()
        .find_map(|pattern| NaiveDateTime::parse_from_str(&text, pattern).ok())
        .map(|d| (d, false))
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(&text, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| (d, true))
        })
}

/// Whether a date field holds a date, or nothing (an open side of the range).
pub fn date_is_valid(text: &str) -> bool {
    text.trim().is_empty() || parse_local(text).is_some()
}

fn parse_bound(text: &str, zone: Zone, end: bool) -> Option<i64> {
    let (naive, whole_day) = parse_local(text)?;
    let local = naive.and_utc().timestamp_millis();
    // The zone's offset at that moment: guessed from the local time, then corrected.
    let guess = local - zone.offset_ms(local);
    let utc = local - zone.offset_ms(guess);
    Some(if end && whole_day {
        utc + 86_400_000 - 1
    } else {
        utc
    })
}

/// The rows of `items` that the range asks for.
fn select(items: &[Point], source: &Source<'_>, options: &ExportOptions) -> Vec<usize> {
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
            all.filter(|&i| (from..=to).contains(&items[i].time))
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
                all.filter(|&i| (start..=end).contains(&items[i].time))
                    .collect()
            }
        }
    };
    if options.skip_forming && !picked.is_empty() && picked.last() == Some(&(items.len() - 1)) {
        let forming = source.is_derived(options.content)
            || source
                .bar_ms
                .is_some_and(|ms| items[items.len() - 1].time + ms > source.now_ms);
        if forming {
            picked.pop();
        }
    }
    if options.every > 1 {
        picked = picked.into_iter().step_by(options.every as usize).collect();
    }
    picked
}

fn box_size_of(source: &Source<'_>) -> Option<i64> {
    let size = source.display.box_size;
    (size > 0).then_some(size)
}

/// The cell of a column for row `i` of `items` (which is row `n` of the table, from 1).
fn cell(
    key: ColumnKey,
    items: &[Point],
    i: usize,
    n: usize,
    source: &Source<'_>,
    options: &ExportOptions,
) -> Cell {
    let it = items[i];
    let previous = i.checked_sub(1).map(|p| items[p].close);
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
            Some(p) if p != 0 => Cell::Value((it.close - p) as f64 / p as f64 * 100.0),
            _ => Cell::Empty,
        },
        ColumnKey::Range => Cell::Price(it.high - it.low),
        ColumnKey::Body => Cell::Price(it.close - it.open),
        ColumnKey::Typical => Cell::Price((it.high + it.low + it.close) / 3),
        ColumnKey::Median => Cell::Price((it.high + it.low) / 2),
        ColumnKey::Direction => Cell::Text(
            match it.close.cmp(&it.open) {
                std::cmp::Ordering::Greater => "up",
                std::cmp::Ordering::Less => "down",
                std::cmp::Ordering::Equal => "flat",
            }
            .to_owned(),
        ),
        ColumnKey::Index => Cell::Int(n as i64),
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
            Cell::Price((p.row_bottom(p.poc) + p.row_top(p.poc)) / 2)
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
        ColumnKey::Marks => profile().map_or(Cell::Empty, |p| Cell::Int(p.marks as i64)),
        ColumnKey::Periods => profile().map_or(Cell::Empty, |p| Cell::Int(p.periods as i64)),
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
                        Cell::Price(v.round() as i64)
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
    let first = picked.first().map(|&i| items[i].time);
    let last = picked.last().map(|&i| items[i].time);
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

// ---- writing values ----

fn zone_name(zone: Zone, at: i64) -> String {
    zone.label(at)
}

/// A time as the calendar of `zone` reads it, by a `strftime` pattern.
fn local_text(time_ms: i64, zone: Zone, pattern: &str) -> String {
    let local = time_ms.saturating_add(zone.offset_ms(time_ms));
    match DateTime::from_timestamp_millis(local) {
        Some(d) if pattern_is_valid(pattern) => d.naive_utc().format(pattern).to_string(),
        _ => String::new(),
    }
}

/// A time in ISO 8601: with `Z` for UTC and the offset of the zone otherwise.
fn iso_text(time_ms: i64, zone: Zone, millis: bool) -> String {
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
struct Written {
    text: String,
    numeric: bool,
}

fn time_text(time_ms: i64, options: &ExportOptions, chart_zone: Zone) -> Written {
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

fn value_text(value: f64, digits: u32, decimal: char, trim: bool) -> String {
    let mut text = format!("{value:.*}", digits as usize);
    if trim && text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    if decimal != '.' {
        text = text.replace('.', &decimal.to_string());
    }
    text
}

/// How many decimals a price is written with.
fn price_digits(options: &ExportOptions, symbol_digits: u32) -> Option<u32> {
    match options.price_digits {
        PriceDigits::Symbol => Some(symbol_digits),
        PriceDigits::Fixed => Some(options.fixed_digits),
        PriceDigits::Raw => None,
    }
}

/// A cell as text, and whether JSON writes it as a number. `None` is a value that is not there.
struct Writer<'a> {
    options: &'a ExportOptions,
    digits: u32,
    zone: Zone,
    /// The decimal to write: a point in the formats that need one.
    decimal: char,
}

fn cell_text(cell: &Cell, cx: &Writer<'_>) -> Option<Written> {
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

// ---- rendering ----

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

fn json_value(cell: &Cell, cx: &Writer<'_>) -> String {
    match cell_text(cell, cx) {
        None => "null".to_owned(),
        Some(w) if w.numeric => w.text,
        Some(w) => json_string(&w.text),
    }
}

/// A field of delimited text, quoted as the options say.
fn field(text: &str, delimiter: char, quote: Quote) -> String {
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

fn sql_literal(cell: &Cell, cx: &Writer<'_>) -> String {
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

// ---- presets ----

/// A named set of options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub options: ExportOptions,
}

/// The presets that come with the app.
pub fn builtin_presets() -> Vec<Preset> {
    use ColumnKey as K;
    let ohlcv = vec![K::Open, K::High, K::Low, K::Close, K::Volume];
    let with = |first: Vec<ColumnKey>, rest: &[ColumnKey]| -> Vec<ColumnKey> {
        first.into_iter().chain(rest.iter().copied()).collect()
    };
    let base = ExportOptions::default();
    vec![
        Preset {
            name: "Standard CSV".to_owned(),
            options: ExportOptions {
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "Spreadsheet".to_owned(),
            options: ExportOptions {
                delimiter: Delimiter::Semicolon,
                decimal: Decimal::Comma,
                bom: true,
                line_ending: LineEnding::CrLf,
                time_format: TimeFormat::DateTime,
                ..base.clone()
            },
        },
        Preset {
            name: "NinjaTrader".to_owned(),
            options: ExportOptions {
                columns: with(vec![K::Time], &ohlcv),
                header: false,
                delimiter: Delimiter::Semicolon,
                time_format: TimeFormat::Custom,
                time_pattern: "%Y%m%d %H%M%S".to_owned(),
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "MetaTrader".to_owned(),
            options: ExportOptions {
                columns: with(vec![K::Date, K::Clock], &ohlcv),
                header: false,
                date_pattern: "%Y.%m.%d".to_owned(),
                clock_pattern: "%H:%M".to_owned(),
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "Unix time".to_owned(),
            options: ExportOptions {
                time_format: TimeFormat::UnixSeconds,
                header_case: HeaderCase::Lower,
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "JSON".to_owned(),
            options: ExportOptions {
                format: Format::Json,
                notes: true,
                header_case: HeaderCase::Lower,
                ..base.clone()
            },
        },
        Preset {
            name: "Everything".to_owned(),
            options: ExportOptions {
                columns: vec![
                    K::Time,
                    K::Open,
                    K::High,
                    K::Low,
                    K::Close,
                    K::Volume,
                    K::Change,
                    K::ChangePercent,
                    K::Range,
                    K::Body,
                    K::Direction,
                    K::Symbol,
                    K::Timeframe,
                ],
                notes: true,
                range: RangeKind::Loaded,
                ..base
            },
        },
    ]
}

pub mod store;

#[cfg(test)]
mod tests;
