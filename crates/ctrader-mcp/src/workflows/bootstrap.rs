//! **W0 — Session bootstrap.** Identifies the live build, resolves the active account,
//! and caches the symbol/asset maps every other workflow assumes are already loaded.
//! Run this exactly once per session, before any other workflow.

use crate::error::CTraderError;
use crate::remote::RemoteClient;
use crate::remote::dto::{Asset, RemoteSymbol};

/// The cached state W0 produces and every later workflow call in the session reuses,
/// per `references/trader-workflows.md` W0's "Invariants": "W0's output (cached state)
/// is the precondition every subsequent workflow assumes."
#[derive(Debug, Clone)]
pub struct RemoteSessionContext {
    /// `rest-proxy` build identifier — compare against this crate's documented minimum
    /// build (`rest-proxy 1.0.18`) before trusting `known-quirks.md` workarounds still
    /// apply verbatim (W0 step 3: older builds may not have the fields these workarounds
    /// assume; newer builds may have already fixed some of them).
    pub version: Option<String>,
    pub build_time: Option<String>,
    /// The authoritative active-account identifier.
    pub trader_id: Option<i64>,
    /// Resolved from `deposit_asset_id` via the cached `get_assets` map.
    pub account_currency: Option<String>,
    pub money_digits: Option<u32>,
    pub balance_display: Option<f64>,
    pub equity_display: Option<f64>,
    pub free_margin_display: Option<f64>,
    /// The full symbol universe — cache and reuse via [`Self::find_symbol`] rather than
    /// re-fetching per lookup (`references/remote-http-server.md` "Symbol cache
    /// discipline").
    pub symbols: Vec<RemoteSymbol>,
    pub assets: Vec<Asset>,
    /// Whether this connection has the `trading` profile bound (mutating tools
    /// available) or only `data` (read-only) — surface to the user before attempting a
    /// mutating workflow if this is `false`.
    pub has_trading_profile: bool,
    /// A session-scoped prefix (`"sess-<8 hex chars>"`) — include it in every mutating
    /// call's `label`/`comment` field for the rest of the session so a transient-failure
    /// retry can detect (and skip) a duplicate rather than placing a second order.
    pub idempotency_prefix: String,
}

impl RemoteSessionContext {
    /// Looks up a cached symbol by its human ticker (e.g. `"EURUSD"`).
    pub fn find_symbol(&self, symbol_name: &str) -> Option<&RemoteSymbol> {
        self.symbols
            .iter()
            .find(|s| s.symbol_name.eq_ignore_ascii_case(symbol_name))
    }

    /// Looks up a cached symbol by its `symbolId`.
    pub fn find_symbol_by_id(&self, symbol_id: i64) -> Option<&RemoteSymbol> {
        self.symbols.iter().find(|s| s.symbol_id == symbol_id)
    }

    /// Resolves an `assetId` (as seen on `depositAssetId`/`baseAssetId`/`quoteAssetId`)
    /// to its currency name.
    pub fn find_asset_name(&self, asset_id: i64) -> Option<&str> {
        self.assets
            .iter()
            .find(|a| a.asset_id == Some(asset_id))
            .and_then(|a| a.name.as_deref())
    }
}

/// Runs W0 against a Remote connection: probes `get_version`, resolves the active
/// account via `get_balance`, and caches `get_assets`/`get_symbols`.
///
/// Does **not** run the optional Q-R4-RANGE `MARKET_RANGE` probe from W0 step 7 (it
/// places a real order) — that remains an explicit, separate opt-in a caller makes only
/// on a demo account; see `self-healing-playbook.md` §5.3 (**P-REMOTE-MARKET-RANGE**)
/// and the skill's W0 step 7 for the exact probe sequence if a caller wants to implement
/// it.
///
/// # Errors
///
/// Propagates any [`CTraderError`] from the underlying `get_version`/`get_balance`/
/// `get_assets`/`get_symbols`/`tools/list` calls.
pub async fn bootstrap_remote(client: &RemoteClient) -> Result<RemoteSessionContext, CTraderError> {
    let version = client.get_version().await?;
    let balance = client.get_balance().await?;
    let assets = client.get_assets().await?;
    let symbols = client.get_symbols().await?;
    let has_trading_profile = client.has_trading_profile().await?;

    let account_currency = balance
        .deposit_asset_id
        .and_then(|id| assets.assets.iter().find(|a| a.asset_id == Some(id)))
        .and_then(|asset| asset.name.clone());

    let idempotency_prefix = format!("sess-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    Ok(RemoteSessionContext {
        version: version.version,
        build_time: version.build_time,
        trader_id: balance.trader_id,
        account_currency,
        money_digits: balance.money_digits,
        balance_display: balance.display_balance(),
        equity_display: balance.display_equity(),
        free_margin_display: balance.display_free_margin(),
        symbols: symbols.symbols,
        assets: assets.assets,
        has_trading_profile,
        idempotency_prefix,
    })
}
