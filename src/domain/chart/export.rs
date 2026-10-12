//! Exporting what a chart holds: its prices (bars or ticks), what its chart type draws (the
//! bricks, columns, lines and profiles of a non-time chart), and the values of its indicators.
//!
//! The work is in two steps, both plain data in and out so they are tested without a window:
//!
//! 1. [`table`] picks the rows (what is on screen, everything loaded, a range of dates, the last
//!    few), the columns (in the order asked, with the names asked) and turns them into a
//!    [`Table`](source::Table) of typed [`Cell`](source::Cell)s.
//! 2. [`render`] writes the table as text, by [`ExportOptions`](options::ExportOptions): delimited text with any
//!    delimiter, JSON, JSON lines, a Markdown table or SQL inserts, with times and numbers written
//!    the way the options say.
//!
//! Prices are the server's raw integers until the very end, and are written by integer
//! arithmetic, so a price never picks up the error of a float.

pub mod choices;
pub mod columns;
pub mod options;
pub mod presets;
pub mod render;
pub mod source;
pub mod table;

/// How many rows a preview shows.
pub const PREVIEW_ROWS: usize = 12;
/// The decimals of the raw price scale.
const RAW_DECIMALS: u32 = 5;
/// Rows of one SQL insert.
const SQL_BATCH: usize = 500;

#[cfg(test)]
mod tests;
