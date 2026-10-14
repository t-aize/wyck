//! The choices an export offers: what to export, the format, the delimiter, the times and the numbers.

use serde::{Deserialize, Serialize};

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
    /// The first character of [`super::options::ExportOptions::custom_delimiter`].
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

    pub(super) fn text(self) -> &'static str {
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
    /// The pattern of [`super::options::ExportOptions::time_pattern`], with the codes of `strftime`.
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

    pub(super) fn text(self) -> char {
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
    /// [`super::options::ExportOptions::fixed_digits`] decimals.
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

    pub(super) fn text(self) -> &'static str {
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

    pub(super) fn apply(self, name: &str) -> String {
        match self {
            Self::Title => name.to_owned(),
            Self::Lower => name.to_lowercase(),
            Self::Upper => name.to_uppercase(),
        }
    }
}
