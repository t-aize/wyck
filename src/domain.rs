//! The trading domain: market data, indicators, drawings and chart models.
//!
//! Pure code. Nothing here reads a file, opens a socket or draws a pixel: a layer above does.

pub mod chart;
pub mod drawings;
pub mod indicators;
