//! The Remote MCP server (`ctrader-remote-mcp`) client.
//!
//! A headless REST proxy (`rest-proxy`) fronting cTrader's Open API. Every tool this
//! server exposes is named explicitly in `references/remote-http-server.md`, so unlike
//! [`crate::local`], every method on [`RemoteClient`] is backed by a fully-typed
//! request/response DTO (see [`dto`]) — there is no "best-effort inferred name"
//! category here.
//!
//! Two profiles exist on the live server: `data` (every read-only tool) and `trading`
//! (`data` plus the five mutating tools: `create_order`, `amend_order`, `cancel_order`,
//! `amend_position`, `close_position`). [`RemoteClient::has_trading_profile`] inspects
//! the live `tools/list` to tell a caller which profile the bound connection has before
//! a workflow attempts a mutation (`references/remote-http-server.md` "Profile
//! distinction").

mod client;
pub mod dto;

pub use client::RemoteClient;
