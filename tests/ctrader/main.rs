//! Integration tests of the cTrader client against a scripted local server, plus the
//! opt-in suite for a real demo account (`live`, ignored by default).
//!
//! One test binary for all of them: each test binary links the whole library.

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
