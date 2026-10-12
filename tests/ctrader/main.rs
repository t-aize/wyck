//! Integration tests of the cTrader client against a scripted local server, plus the
//! opt-in suite for a real demo account (`live`, ignored by default).
//!
//! One test binary for all of them: each test binary links the whole library.
// A failed `unwrap` or `expect` is how a test fails. clippy.toml covers only `#[test]` functions
// and `#[cfg(test)]` modules, not the helpers of an integration test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

mod account;
mod auth;
mod client;
mod handle;
mod live;
mod margin;
mod market;
mod properties;
mod robustness;
mod session;
mod trading;
