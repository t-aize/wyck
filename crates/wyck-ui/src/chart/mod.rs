//! The price chart of Wyck and the screen that arranges several of them.

#[expect(
    clippy::module_inception,
    reason = "the single chart keeps its name from when this area was a crate with a `chart` module"
)]
pub mod chart;
pub mod multichart;
