//! The chart's logic, free of any UI toolkit.
//!
//! Everything the price chart decides lives here so it can be tested without a window; the
//! drawing (`ui::chart`) only turns these results into shapes. The pieces:
//!
//! | Module | Role |
//! |---|---|
//! | [`series`] | Bars of one symbol and period: merging, the live bar, the memory cap |
//! | [`viewport`] | Which bars are on screen: zoom, pan, and the pixel/bar mapping |
//! | [`scale`] | The price scale: auto and manual modes, "nice" grid steps, labels |
//! | [`timeaxis`] | Labels for the time axis, with day, month and year boundaries |
//!
//! The data flows one way: the engine hands over [`wyck_engine::domain::Candle`]s, a
//! [`series::Series`] keeps them in order, the [`viewport::Viewport`] picks the visible slice,
//! the [`scale::PriceScale`] fits the vertical range to that slice, and the drawing code maps
//! both to pixels.

pub mod scale;
pub mod series;
pub mod timeaxis;
pub mod viewport;
