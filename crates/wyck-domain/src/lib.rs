//! The domain of Wyck, one module per area, each naming only the ones before it in this list:
//! `base` (shared vocabulary), `market`, `trading`, `drawings`, `indicators`, `chart`. No screen,
//! no file, no network: `cargo xtask deps` checks the modules and the crates it may use.

pub mod base;
pub mod chart;
pub mod drawings;
pub mod indicators;
pub mod market;
pub mod trading;
