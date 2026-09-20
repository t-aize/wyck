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
//! | [`coverage`] | Which time ranges were already fetched, empty ones included |
//! | [`history`] | What to ask the server for: the tail, older pages, what counts as fetched |
//! | [`store`] | The bars on disk, in one transactional file |
//! | [`lod`] | Level of detail and pixel snapped boxes for candles, lines and columns |
//! | [`interaction`] | The chart model: data, view, and what each wheel turn, drag and key does |
//!
//! The data flows one way. The engine hands over [`wyck_engine::domain::Candle`]s; the
//! [`store`] keeps the closed ones between launches; a [`series::Series`] holds them in order in
//! memory; the [`viewport::Viewport`] picks the visible slice; the [`scale::PriceScale`] fits the
//! vertical range to that slice; and the drawing code maps both to pixels.
//!
//! # Opening a chart
//!
//! 1. Read the newest bars from the [`store`] and draw them at once.
//! 2. Ask [`history::tail_span`] what is missing since the last covered time, fetch it, merge it
//!    into the series, and save the closed part with the span [`history::covered_for`] returns.
//! 3. From then on fold each live price into the last bar ([`series::Series::apply_price`]).
//! 4. When the user scrolls near the oldest bar, read [`history::older_span`] from the store and
//!    fetch only the [`coverage::Coverage::missing`] parts of it.

pub mod coverage;
pub mod history;
pub mod interaction;
pub mod lod;
pub mod scale;
pub mod series;
pub mod store;
pub mod timeaxis;
pub mod viewport;
