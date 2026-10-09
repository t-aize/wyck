//! Drawing on the chart: trend lines, Fibonacci levels, shapes, a measuring tool, long and short
//! positions, text.
//!
//! What a drawing is, its shapes, how the pointer finds one and the book of every symbol's drawings
//! with its undo live in [`wyck_chart::drawing`], with no window in them. This module holds
//! [`Drawings`], the live copy every chart reads and edits.
//!
//! A drawing is anchored to times and prices, not to screen positions, so it follows the chart
//! when it is scrolled or zoomed, and shows on every timeframe of its symbol.

mod state;

pub use state::Drawings;
