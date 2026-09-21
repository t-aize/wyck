//! End-to-end trader workflows composed from [`crate::local`], [`crate::remote`],
//! [`crate::math`], and [`crate::quirks`].
//!
//! Implements W0 bootstrap, W5 risk sizing, W6 history, pre-trade briefing,
//! cost comparison, and safe flatten. Entry and amendment patterns from W1 and W2
//! are available directly in [`crate::quirks`].
//!
//! Every workflow function here is written against [`crate::remote::RemoteClient`],
//! since Remote's tool surface is the one the skill documents with exact field names
//! for every tool ([`crate::local`]'s doc comments explain which categories are
//! best-effort). A Local-server equivalent of any workflow below composes the same way
//! from [`crate::local::LocalClient`]'s 1:1-equivalent methods (`get_positions`,
//! `get_pending_orders`, `place_market_order`, `close_position`, ...): the *shape* of
//! the workflow (read -> compute -> pre-flight gate -> mutate -> re-read) does not
//! change between servers, only the DTOs and unit encodings do.

pub mod backfill;
pub mod bootstrap;
pub mod briefing;
pub mod cost_comparison;
pub mod safe_flatten;
pub mod sizing;

pub use backfill::backfill_trendbars;
pub use bootstrap::{RemoteSessionContext, bootstrap_remote};
pub use briefing::{PreTradeBriefing, pre_trade_briefing};
pub use cost_comparison::{SymbolCost, compare_trading_costs};
pub use safe_flatten::{FlattenReport, safe_flatten};
pub use sizing::{RiskSizingDecision, size_position_by_risk};
