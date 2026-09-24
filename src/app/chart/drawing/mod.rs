//! Drawing on the chart: trend lines, Fibonacci levels, shapes, a measuring tool, long and short
//! positions, text.
//!
//! - [`model`]: what a drawing is, and the saved form of it.
//! - [`figures`]: the shapes of the tools made of many lines (pitchforks, Gann, patterns, waves).
//! - [`extras`]: the shapes of the other tools (ranges, cycles, circles and arcs, notes, volume).
//! - [`geometry`]: turning a drawing into shapes on the screen, and finding what is under the
//!   pointer. It sees the chart only through the [`geometry::Projection`] trait.
//! - [`book`]: the drawings of every symbol and the rules for making and changing them with the
//!   pointer, plus undo. No window in it, so it is tested on its own.
//!
//! A drawing is anchored to times and prices, not to screen positions, so it follows the chart
//! when it is scrolled or zoomed, and shows on every timeframe of its symbol.

pub mod book;
mod decor;
pub mod extras;
pub mod figures;
pub mod geometry;
pub mod look;
pub mod model;
pub mod position;
mod state;

pub use state::Drawings;
