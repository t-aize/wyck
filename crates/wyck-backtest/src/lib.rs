//! Rhai-scripted strategy engine, simulated order matching and portfolio analytics for
//! Wyck's Backtesting feature.
//!
//! This crate is scaffolding: it exists so the workspace has a UI-free, testable home for
//! the backtesting engine (strategy scripting API, matching engine, fill/slippage/
//! commission models, and the performance report) once it is built on top of
//! [`wyck_market_data`]'s historical catalog and Replay clock. See the "Replay &
//! Backtesting" plan for the full design.
