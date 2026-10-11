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

/// How many rows a preview shows.
pub const PREVIEW_ROWS: usize = 12;
/// The decimals of the raw price scale.
const RAW_DECIMALS: u32 = 5;
/// Rows of one SQL insert.
const SQL_BATCH: usize = 500;

mod choices;
mod columns;
mod options;
mod presets;
mod render;
mod source;
mod table;

pub use choices::{
    Content, Decimal, Delimiter, Empty, ExportZone, Format, HeaderCase, LineEnding, Order,
    PriceDigits, Quote, RangeKind, TimeFormat,
};
pub use columns::ColumnKey;
use options::identifier;
pub use options::{ExportOptions, pattern_is_valid};
pub use presets::{Preset, builtin_presets};
pub use render::{clashes, file_name, price_text, render};
use render::{local_text, zone_name};
use source::Point;
pub use source::{Cell, Source, Table, TableColumn};
pub use table::{date_is_valid, table};

#[cfg(test)]
#[path = "../../../tests/unit/domain/chart/export.rs"]
mod tests;
