//! # ctrader-mcp
//!
//! A Rust client for cTrader's official Model Context Protocol (MCP) servers, built on
//! [`rmcp`]. cTrader exposes trading and market-data capabilities through **two** distinct
//! MCP server families, and this crate provides a typed, documented, quirk-aware wrapper
//! around both of them:
//!
//! - **Local** (`ctrader-local-mcp`, see [`local`]): bound to the cTrader Desktop
//!   application over local HTTP. Owns charts, drawings, indicators, watchlists,
//!   workspaces, alerts, cBot plugins, and multi-account desktop context. Volumes are in
//!   broker-defined units, prices are display floats, and symbols are identified by
//!   string ticker.
//! - **Remote** (`ctrader-remote-mcp`, see [`remote`]): a headless REST proxy
//!   (`rest-proxy`) fronting cTrader's Open API. Owns trailing stop loss, `MARKET_RANGE`
//!   orders, batched quote fetches, and granular timeframe history. Volumes are in
//!   integer cents, prices are integer pipettes, and symbols are identified by numeric
//!   `symbolId`.
//!
//! Mixing values between the two servers without conversion silently produces wrong
//! sizes, wrong prices, or schema rejections: see [`math`] for the conversion routines
//! (pip/price math, lot/cents/units encoding, tiered margin, currency-chain conversion,
//! risk-based position sizing) and [`quirks`] for the documented runtime-behavior
//! divergences and their recovery patterns.
//!
//! ## Module map
//!
//! - [`config`]: connection configuration (endpoint URI, bearer/auth header, timeouts).
//! - [`transport`]: the underlying [`rmcp`] streamable-HTTP+SSE session and the generic
//!   typed `tools/call` helper shared by both server clients.
//! - [`error`]: the crate's error type, including the self-healing error-classification
//!   matrix that distinguishes caller schema mismatches, server rejections, upstream
//!   broker failures, and local-server faults.
//! - [`common`]: types shared by both servers (trade side, timestamps, money helpers).
//! - [`math`]: pure, unit-tested conversion and sizing routines (no network I/O).
//! - [`local`]: the [`local::LocalClient`] wrapping every documented `ctrader-local-mcp`
//!   capability.
//! - [`remote`]: the [`remote::RemoteClient`] wrapping every documented
//!   `ctrader-remote-mcp` capability.
//! - [`quirks`]: the named recovery patterns (P-AMEND-SAFE, P-REMOTE-MARKET-RELATIVE,
//!   P-REMOTE-HISTORY-CHUNK, P-LOCAL-OLDEST-FIRST, ...) implemented as reusable functions.
//! - [`retry`], [`retry::RetryPolicy`] and the backoff loop wrapping session
//!   establishment and read-only calls (never mutating ones, see that module's doc
//!   comment for why).
//! - [`workflows`]: end-to-end trader workflows (session bootstrap, entry orders, modify,
//!   close, read, risk sizing, history) composed from the two clients plus [`math`] and
//!   [`quirks`].
//!
//! ## Provenance
//!
//! The behavioral documentation embedded in this crate's doc comments (quirk IDs like
//! `Q-R10`, pattern IDs like `P-AMEND-SAFE`, workflow IDs like `W1`) mirrors the
//! `ctrader-mcp-servers` skill, last audited against `rest-proxy 1.0.18` (Remote) and a
//! local build observed on 2026-05-14. The MCP JSON-Schema advertised by each live server
//! is always the source of truth on wire *shape*; this crate is the source of truth on
//! *how to drive that shape correctly*, including the workarounds the servers currently
//! require.

pub mod common;
pub mod config;
pub mod error;
pub mod local;
pub mod math;
pub mod quirks;
mod rate_limit;
pub mod remote;
pub mod retry;
pub mod time;
pub mod transport;
pub mod workflows;

pub use config::ConnectionConfig;
pub use error::CTraderError;
pub use local::LocalClient;
pub use remote::RemoteClient;
pub use retry::RetryPolicy;
pub use transport::McpSession;
