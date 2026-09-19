//! Shared test-only infrastructure for `ctrader-mcp`'s integration tests. The
//! implementation lives in the library (`ctrader_mcp::test_support`, behind the
//! `test-support` feature) so other crates can reuse it; this module re-exports it under
//! the path the tests already use.
//!
//! Named `support/mod.rs` (not `support.rs`) so `cargo test` does not treat it as its own
//! test binary. Each `tests/*.rs` file that does `mod support;` compiles its own copy and
//! typically uses only a subset, hence the `unused_imports` allowance.
#![allow(unused_imports)]

pub use ctrader_mcp::test_support::*;
