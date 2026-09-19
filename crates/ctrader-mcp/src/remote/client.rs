//! [`RemoteClient`] — the typed wrapper around every documented `ctrader-remote-mcp`
//! tool.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::ConnectionConfig;
use crate::error::CTraderError;
use crate::rate_limit::RateLimiter;
use crate::remote::dto::*;
use crate::transport::McpSession;

/// Requests per second Remote's `rest-proxy` documents as its rate limit, in particular
/// on the historical-data endpoints (`get_trendbars`/`get_order_history`/`get_deals`) —
/// see `references/remote-http-server.md` and [`crate::workflows::backfill`]'s doc
/// comment. Applied to every call this client makes, not just the historical ones,
/// since the server does not document the cap as endpoint-specific.
const REMOTE_RATE_LIMIT_PER_SECOND: f64 = 5.0;

/// A connected session against a Remote server (`ctrader-remote-mcp`, the `rest-proxy`
/// headless REST proxy).
pub struct RemoteClient {
    session: McpSession,
    /// Gates every call this client makes to [`REMOTE_RATE_LIMIT_PER_SECOND`]. Held
    /// separately from [`McpSession`] (rather than built into it) because the 5 req/s cap
    /// is a Remote-specific server constraint, not a general MCP transport concern —
    /// [`crate::local::LocalClient`] shares the same session type but has no such limit.
    rate_limiter: RateLimiter,
}

impl RemoteClient {
    /// Wraps an already-connected [`McpSession`].
    pub fn new(session: McpSession) -> Self {
        Self {
            session,
            rate_limiter: RateLimiter::new(
                REMOTE_RATE_LIMIT_PER_SECOND,
                REMOTE_RATE_LIMIT_PER_SECOND as u32,
            ),
        }
    }

    /// Opens a new session against the Remote server at `config.uri` and wraps it.
    pub async fn connect(config: &ConnectionConfig) -> Result<Self, CTraderError> {
        Ok(Self::new(McpSession::connect(config).await?))
    }

    /// Rate-limited counterpart to [`McpSession::call`] — every method below that
    /// mutates account state uses this (not [`Self::call_idempotent`], since retrying a
    /// mutating call risks double-submitting it — see [`crate::retry`]'s doc comment).
    ///
    /// The rate limiter is only acquired once per logical call, not once per retry
    /// attempt — irrelevant here since this path never retries, and for
    /// [`Self::call_idempotent`] a bounded handful of retries spread over at most a few
    /// seconds does not meaningfully risk violating a 5 req/s server-side cap even
    /// without re-throttling each attempt individually.
    async fn call<P, R>(&self, tool: &'static str, params: P) -> Result<R, CTraderError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        self.rate_limiter.acquire().await;
        self.session.call(tool, params).await
    }

    /// Rate-limited AND retried counterpart to [`McpSession::call_idempotent`]. Only
    /// ever used by this client's read-only getters — see that method's doc comment.
    async fn call_idempotent<P, R>(&self, tool: &'static str, params: P) -> Result<R, CTraderError>
    where
        P: Serialize,
        R: DeserializeOwned,
    {
        self.rate_limiter.acquire().await;
        self.session.call_idempotent(tool, params).await
    }

    /// Rate-limited AND retried counterpart to [`McpSession::call_no_args_idempotent`].
    async fn call_no_args_idempotent<R: DeserializeOwned>(
        &self,
        tool: &'static str,
    ) -> Result<R, CTraderError> {
        self.rate_limiter.acquire().await;
        self.session.call_no_args_idempotent(tool).await
    }

    /// The underlying MCP session, for direct protocol access not covered by a typed
    /// method.
    pub fn session(&self) -> &McpSession {
        &self.session
    }

    /// Gracefully shuts down the underlying MCP session.
    pub async fn shutdown(self) -> Result<(), CTraderError> {
        self.session.shutdown().await
    }

    /// Inspects the live `tools/list` to determine whether this connection has the
    /// `trading` profile (mutating tools bound) or only `data` (read-only). Per
    /// `references/remote-http-server.md` "Profile distinction", surface this to the
    /// user before attempting any workflow that needs to mutate.
    pub async fn has_trading_profile(&self) -> Result<bool, CTraderError> {
        let tools = self.session.list_tool_names().await?;
        Ok(tools.iter().any(|name| name == "create_order"))
    }

    // -----------------------------------------------------------------------------
    // Version & diagnostics
    // -----------------------------------------------------------------------------

    /// Build identification — cache `version`/`build_time`/`service` once per session
    /// (W0) and compare `version` against this crate's documented minimum build
    /// (`rest-proxy 1.0.18`) before applying any `known-quirks.md` workaround.
    pub async fn get_version(&self) -> Result<VersionResponse, CTraderError> {
        self.call_no_args_idempotent("get_version").await
    }

    /// The server's wall-clock time, used by W0 to compute the agent's local-clock
    /// offset.
    pub async fn get_server_time(&self) -> Result<serde_json::Value, CTraderError> {
        self.call_no_args_idempotent("get_server_time").await
    }

    /// Confirms round-trip liveness with a native MCP protocol ping — see
    /// [`McpSession::ping`] for why this is not (and, per a live probe, never was
    /// successfully) a `tools/call`.
    pub async fn ping(&self) -> Result<(), CTraderError> {
        self.rate_limiter.acquire().await;
        self.session.ping().await
    }

    // -----------------------------------------------------------------------------
    // Account state
    // -----------------------------------------------------------------------------

    /// The active account's balance/equity/margin snapshot, in Remote's raw
    /// `10^money_digits`-scaled encoding — see [`RemoteBalanceResponse`]'s
    /// `display_*` helpers.
    pub async fn get_balance(&self) -> Result<RemoteBalanceResponse, CTraderError> {
        self.call_no_args_idempotent("get_balance").await
    }

    /// Resolves `assetId` values (`depositAssetId`, `baseAssetId`, `quoteAssetId`) to
    /// currency names. Stable for the session — cache it (`references/remote-http-
    /// server.md` "Symbol cache discipline" applies equally to this endpoint).
    pub async fn get_assets(&self) -> Result<GetAssetsResponse, CTraderError> {
        self.call_no_args_idempotent("get_assets").await
    }

    // -----------------------------------------------------------------------------
    // Symbols & static metadata
    // -----------------------------------------------------------------------------

    /// The full symbol universe (`symbolId` <-> `symbolName` plus static metadata).
    /// Stable for the session — cache it, do not re-fetch per tool call
    /// (`references/remote-http-server.md` "Symbol cache discipline").
    pub async fn get_symbols(&self) -> Result<GetSymbolsResponse, CTraderError> {
        self.call_no_args_idempotent("get_symbols").await
    }

    // -----------------------------------------------------------------------------
    // Live and historical market data
    // -----------------------------------------------------------------------------

    /// Batched live quotes for the given `symbol_ids`. Per `Q-R8`, validate every id
    /// against the cached [`Self::get_symbols`] map first — a single unknown id
    /// silently empties the ENTIRE `prices` array, with no per-symbol error to localize
    /// the bad id.
    pub async fn get_spot_prices(
        &self,
        symbol_ids: Vec<i64>,
    ) -> Result<GetSpotPricesResponse, CTraderError> {
        self.call_idempotent(
            "get_spot_prices",
            GetSpotPricesParams {
                symbol_id: symbol_ids,
            },
        )
        .await
    }

    /// Historical OHLCV bars for one symbol over `[from_timestamp, to_timestamp)`. Per
    /// `Q-R1`, `period` is one of exactly 9 values (enforced at the type level by
    /// [`crate::common::Period`]). Per `Q-R7`, a window spanning more than 720 hours is
    /// rejected outright (not paginated) — use
    /// [`crate::quirks::remote_history_windows`] (**P-REMOTE-HISTORY-CHUNK**) to split a
    /// wider request into compliant windows first.
    pub async fn get_trendbars(
        &self,
        params: GetTrendbarsParams,
    ) -> Result<GetTrendbarsResponse, CTraderError> {
        self.call_idempotent("get_trendbars", params).await
    }

    // -----------------------------------------------------------------------------
    // Positions, orders, deals
    // -----------------------------------------------------------------------------

    /// Every open position AND every pending order in one call (contrast with Local,
    /// which splits these across `get_positions`/`get_pending_orders`).
    pub async fn get_positions(&self) -> Result<GetPositionsResponse, CTraderError> {
        self.call_no_args_idempotent("get_positions").await
    }

    /// A single position's full detail: the position itself, its related orders, and
    /// its related deals.
    pub async fn get_position_details(
        &self,
        position_id: i64,
    ) -> Result<GetPositionDetailsResponse, CTraderError> {
        self.call_idempotent(
            "get_position_details",
            GetPositionDetailsParams { position_id },
        )
        .await
    }

    /// Every working (not yet filled) order.
    pub async fn get_pending_orders(&self) -> Result<GetPendingOrdersResponse, CTraderError> {
        self.call_no_args_idempotent("get_pending_orders").await
    }

    /// Order history over `[from_timestamp, to_timestamp)`. Per `Q-R7`, a window
    /// spanning more than 720 hours is rejected — chunk first (see
    /// [`crate::quirks::remote_history_windows`]). Per `Q-R11`, a just-created/-closed
    /// order may not appear here immediately — prefer the object embedded in the
    /// mutation response for immediate post-mutation verification.
    pub async fn get_order_history(
        &self,
        params: GetOrderHistoryParams,
    ) -> Result<GetOrderHistoryResponse, CTraderError> {
        self.call_idempotent("get_order_history", params).await
    }

    /// Deal (execution) history over `[from_timestamp, to_timestamp)`, page-capped by
    /// `max_rows` (default 50). Same `Q-R7`/`Q-R11` caveats as
    /// [`Self::get_order_history`].
    pub async fn get_deals(
        &self,
        params: GetDealsParams,
    ) -> Result<GetDealsResponse, CTraderError> {
        self.call_idempotent("get_deals", params).await
    }

    // -----------------------------------------------------------------------------
    // Trading mutations (requires the `trading` profile — see
    // [`Self::has_trading_profile`])
    // -----------------------------------------------------------------------------

    /// Places an order. Runs [`CreateOrderParams::validate`] client-side before sending
    /// — see that method's doc comment for exactly which `self-healing-playbook.md` §1
    /// gates it structurally enforces (`Q-R4`, required conditional fields, label/
    /// comment length limits). Per `Q-R11`, the returned `order`/`position`/`deal`
    /// objects are the authoritative immediate confirmation — prefer them over an
    /// immediate follow-up `get_deals`/`get_order_history` call.
    ///
    /// # Errors
    ///
    /// Returns [`CTraderError::PreFlightRejected`] if `params.validate()` fails, without
    /// making a network call.
    pub async fn create_order(
        &self,
        params: CreateOrderParams,
    ) -> Result<CreateOrderResponse, CTraderError> {
        params.validate()?;
        self.call("create_order", params).await
    }

    /// Amends a pending order's price/SL/TP/expiry.
    pub async fn amend_order(
        &self,
        params: AmendOrderParams,
    ) -> Result<serde_json::Value, CTraderError> {
        self.call("amend_order", params).await
    }

    /// Cancels a single working order.
    pub async fn cancel_order(&self, order_id: i64) -> Result<serde_json::Value, CTraderError> {
        self.call("cancel_order", CancelOrderParams { order_id })
            .await
    }

    /// Amends an OPEN position's SL/TP (and optionally trailing SL). `params` must be
    /// built via [`AmendPositionParams::new`], which requires both legs — see that
    /// type's doc comment for why (`Q-R10`, **P-AMEND-SAFE**). ALWAYS re-read via
    /// [`Self::get_position_details`] afterward and confirm both legs survived
    /// (`self-healing-playbook.md` §2.2) — the omit-removes quirk is silent at the wire
    /// level.
    pub async fn amend_position(
        &self,
        params: AmendPositionParams,
    ) -> Result<AmendPositionResponse, CTraderError> {
        self.call("amend_position", params).await
    }

    /// Closes a position, fully or partially. `params.volume` (cents) is REQUIRED —
    /// pass the position's current open volume (read via [`Self::get_positions`] first)
    /// to close entirely; Remote has no "close all without volume" form (contrast with
    /// Local's `close_position`, which takes no volume and always closes the full
    /// remainder).
    pub async fn close_position(
        &self,
        params: ClosePositionParams,
    ) -> Result<ClosePositionResponse, CTraderError> {
        self.call("close_position", params).await
    }
}
