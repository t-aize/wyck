//! **W0: Session bootstrap.** Identifies the live build, resolves the active account,
//! and caches the symbol/asset maps every other workflow assumes are already loaded.
//! Run this exactly once per session, before any other workflow.

use std::collections::HashMap;

use crate::error::CTraderError;
use crate::remote::RemoteClient;
use crate::remote::dto::{Asset, RemoteSymbol};

/// The cached state W0 produces and every later workflow call in the session reuses,
/// per `references/trader-workflows.md` W0's "Invariants": "W0's output (cached state)
/// is the precondition every subsequent workflow assumes."
///
/// `symbols`/`assets` stay in server-returned order for iteration/display; symbol- and
/// asset-lookups go through an index built once in [`bootstrap_remote`], so
/// [`Self::find_symbol`]/[`Self::find_symbol_by_id`]/[`Self::find_asset_name`] are O(1)
/// instead of scanning the full universe on every call. That matters here specifically
/// because `wyck`'s whole premise is hotkey-driven, no-LLM-in-the-execution-path
/// trading (see the crate root docs): a symbol lookup sits directly on that hotkey path
/// for every order placement, so it should not cost a linear scan over a symbol universe
/// that can run into the thousands.
#[derive(Debug, Clone)]
pub struct RemoteSessionContext {
    /// `rest-proxy` build identifier: compare against this crate's documented minimum
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
    /// The full symbol universe, in server-returned order. Prefer
    /// [`Self::find_symbol`]/[`Self::find_symbol_by_id`] over scanning this directly.
    pub symbols: Vec<RemoteSymbol>,
    /// The full asset universe, in server-returned order. Prefer
    /// [`Self::find_asset_name`] over scanning this directly.
    pub assets: Vec<Asset>,
    /// Whether this connection has the `trading` profile bound (mutating tools
    /// available) or only `data` (read-only): surface to the user before attempting a
    /// mutating workflow if this is `false`.
    pub has_trading_profile: bool,
    /// A session-scoped prefix (`"sess-<8 hex chars>"`): include it in every mutating
    /// call's `label`/`comment` field for the rest of the session so a transient-failure
    /// retry can detect (and skip) a duplicate rather than placing a second order.
    pub idempotency_prefix: String,

    /// Uppercased ticker -> index into `symbols`. Not `pub`: an implementation detail
    /// of the O(1) lookup, not part of the public contract (the index encoding could
    /// change without breaking callers who only ever go through
    /// [`Self::find_symbol`]).
    symbol_index_by_name: HashMap<String, usize>,
    /// `symbolId` -> index into `symbols`.
    symbol_index_by_id: HashMap<i64, usize>,
    /// `assetId` -> index into `assets`.
    asset_index_by_id: HashMap<i64, usize>,
}

impl RemoteSessionContext {
    /// Looks up a cached symbol by its human ticker (e.g. `"EURUSD"`), case-insensitive.
    /// O(1) via the index built in [`bootstrap_remote`].
    pub fn find_symbol(&self, symbol_name: &str) -> Option<&RemoteSymbol> {
        let key = symbol_name.to_ascii_uppercase();
        self.symbol_index_by_name
            .get(&key)
            .map(|&index| &self.symbols[index])
    }

    /// Looks up a cached symbol by its `symbolId`. O(1).
    pub fn find_symbol_by_id(&self, symbol_id: i64) -> Option<&RemoteSymbol> {
        self.symbol_index_by_id
            .get(&symbol_id)
            .map(|&index| &self.symbols[index])
    }

    /// Resolves an `assetId` (as seen on `depositAssetId`/`baseAssetId`/`quoteAssetId`)
    /// to its currency name. O(1).
    pub fn find_asset_name(&self, asset_id: i64) -> Option<&str> {
        let index = *self.asset_index_by_id.get(&asset_id)?;
        self.assets[index].name.as_deref()
    }
}

/// Runs W0 against a Remote connection: probes `get_version`, resolves the active
/// account via `get_balance`, and caches `get_assets`/`get_symbols` (indexed for O(1)
/// lookup: see [`RemoteSessionContext`]'s doc comment).
///
/// Does **not** run the optional Q-R4-RANGE `MARKET_RANGE` probe from W0 step 7 (it
/// places a real order): that remains an explicit, separate opt-in a caller makes only
/// on a demo account; see `self-healing-playbook.md` §5.3 (**P-REMOTE-MARKET-RANGE**)
/// and the skill's W0 step 7 for the exact probe sequence if a caller wants to implement
/// it.
///
/// # `get_version` is best-effort
///
/// The skill's reference docs (audited against `rest-proxy 1.0.18`) document
/// `get_version` as part of the Remote surface, but it has been observed absent
/// (`MCP -32602: Tool get_version not found`) on at least one live deployment: the
/// build-identification step this tool feeds is a diagnostic nicety (it only gates
/// *which* `known-quirks.md` workarounds a caller trusts), not something the rest of
/// bootstrap: resolving the account, caching symbols: depends on. A missing
/// `get_version` is therefore logged and swallowed rather than aborting the whole
/// session: `version`/`build_time` on the returned [`RemoteSessionContext`] are simply
/// `None`, and a caller relying on a specific quirk build-gate should treat `None` the
/// same way W0 step 3 treats an unparseable version: decline to assume the workaround
/// still applies rather than guessing.
///
/// # Errors
///
/// Propagates any [`CTraderError`] from the underlying `get_balance`/`get_assets`/
/// `get_symbols`/`tools/list` calls (but not from `get_version`: see above).
pub async fn bootstrap_remote(client: &RemoteClient) -> Result<RemoteSessionContext, CTraderError> {
    // Diagnostic-only, logged unconditionally before anything else: if a later step in
    // this function fails with a "tool not found" style error, the full advertised
    // catalog is what actually explains why, instead of guessing tool-by-tool against
    // the skill's documented names (which are audited against a specific rest-proxy
    // build and may not match every live deployment).
    match client.session().list_tool_names().await {
        Ok(tools) => tracing::info!(?tools, "remote server advertised tool catalog"),
        Err(source) => {
            tracing::warn!(error = %source, "failed to list the remote server's tool catalog")
        }
    }

    let (version, build_time) = match client.get_version().await {
        Ok(response) => (response.version, response.build_time),
        Err(source) => {
            tracing::warn!(
                error = %source,
                "get_version failed; continuing session bootstrap without a build identifier"
            );
            (None, None)
        }
    };
    let balance = client.get_balance().await?;
    let assets = client.get_assets().await?;
    let symbols = client.get_symbols().await?;
    let has_trading_profile = client.has_trading_profile().await?;

    let symbol_index_by_name: HashMap<String, usize> = symbols
        .symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| (symbol.symbol_name.to_ascii_uppercase(), index))
        .collect();
    let symbol_index_by_id: HashMap<i64, usize> = symbols
        .symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| (symbol.symbol_id, index))
        .collect();
    let asset_index_by_id: HashMap<i64, usize> = assets
        .assets
        .iter()
        .enumerate()
        .filter_map(|(index, asset)| asset.asset_id.map(|id| (id, index)))
        .collect();

    let account_currency = balance
        .deposit_asset_id
        .and_then(|id| asset_index_by_id.get(&id))
        .and_then(|&index| assets.assets[index].name.clone());

    let idempotency_prefix = format!("sess-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    Ok(RemoteSessionContext {
        version,
        build_time,
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
        symbol_index_by_name,
        symbol_index_by_id,
        asset_index_by_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::dto::Asset;
    use rmcp::model::JsonObject;

    fn symbol(id: i64, name: &str) -> RemoteSymbol {
        RemoteSymbol {
            symbol_id: id,
            symbol_name: name.to_owned(),
            enabled: Some(true),
            base_asset_id: None,
            quote_asset_id: None,
            symbol_category_id: None,
            description: None,
            pip_digits: Some(5),
            extra: JsonObject::new(),
        }
    }

    fn context_with_symbols(symbols: Vec<RemoteSymbol>) -> RemoteSessionContext {
        let symbol_index_by_name = symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| (symbol.symbol_name.to_ascii_uppercase(), index))
            .collect();
        let symbol_index_by_id = symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| (symbol.symbol_id, index))
            .collect();
        let assets: Vec<Asset> = vec![Asset {
            asset_id: Some(1),
            name: Some("USD".to_owned()),
            extra: JsonObject::new(),
        }];
        let asset_index_by_id = assets
            .iter()
            .enumerate()
            .filter_map(|(index, asset)| asset.asset_id.map(|id| (id, index)))
            .collect();

        RemoteSessionContext {
            version: None,
            build_time: None,
            trader_id: None,
            account_currency: None,
            money_digits: None,
            balance_display: None,
            equity_display: None,
            free_margin_display: None,
            symbols,
            assets,
            has_trading_profile: true,
            idempotency_prefix: "sess-test".to_owned(),
            symbol_index_by_name,
            symbol_index_by_id,
            asset_index_by_id,
        }
    }

    #[test]
    fn find_symbol_is_case_insensitive() {
        let context = context_with_symbols(vec![symbol(1, "EURUSD")]);
        assert_eq!(context.find_symbol("eurusd").map(|s| s.symbol_id), Some(1));
        assert_eq!(context.find_symbol("EURUSD").map(|s| s.symbol_id), Some(1));
        assert_eq!(context.find_symbol("EurUsd").map(|s| s.symbol_id), Some(1));
        assert!(context.find_symbol("GBPUSD").is_none());
    }

    #[test]
    fn find_symbol_by_id_and_find_asset_name() {
        let context = context_with_symbols(vec![symbol(1, "EURUSD"), symbol(2, "GBPUSD")]);
        assert_eq!(
            context.find_symbol_by_id(2).map(|s| s.symbol_name.as_str()),
            Some("GBPUSD")
        );
        assert!(context.find_symbol_by_id(999).is_none());
        assert_eq!(context.find_asset_name(1), Some("USD"));
        assert_eq!(context.find_asset_name(999), None);
    }
}
