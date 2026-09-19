//! The Local MCP server (`ctrader-local-mcp`) client.
//!
//! Bound to the cTrader Desktop application over local HTTP. Owns charts, drawings,
//! indicators, watchlists, workspaces, price alerts, and cBot plugins, in addition to
//! the trading/account/market-data surface it shares conceptually (but not
//! bit-for-bit, see `references/local-http-server.md`) with [`crate::remote`].
//!
//! ## Typing depth by category
//!
//! The skill's `references/local-http-server.md` documents every category's *behavior*
//! (units, casing, pagination caps, quirks) but only names every wire tool exactly for
//! the trading/account/market-data/history/chart-lifecycle/drawing/indicator surface:
//! the categories this crate models with full request/response DTOs (see [`dto`]). For
//! the watchlist, workspace, chart-template, and price-alert categories, the skill names
//! the *capability* precisely but not always the exact wire tool name or field names;
//! those methods on [`LocalClient`] say so explicitly in their own doc comments and go
//! through [`crate::transport::McpSession::call_raw`], returning [`serde_json::Value`]
//! rather than a crate-owned struct. Verify the live `tools/list` schema before
//! depending on a specific shape there in production; the schema-fields-only pre-flight
//! gate (`self-healing-playbook.md` §1.5) still applies; this crate only ever sends the
//! fields it was explicitly given.

mod client;
pub mod dto;

pub use client::LocalClient;
