//! [`RemoteClient`] — the typed wrapper around every documented `ctrader-remote-mcp`
//! tool.

use crate::config::ConnectionConfig;
use crate::error::CTraderError;
use crate::remote::dto::*;
use crate::transport::McpSession;

/// A connected session against a Remote server (`ctrader-remote-mcp`, the `rest-proxy`
/// headless REST proxy).
pub struct RemoteClient {
    session: McpSession,
}

impl RemoteClient {
    /// Wraps an already-connected [`McpSession`].
    pub fn new(session: McpSession) -> Self {
        Self { session }
    }

    /// Opens a new session against the Remote server at `config.uri` and wraps it.
    pub async fn connect(config: &ConnectionConfig) -> Result<Self, CTraderError> {
        Ok(Self::new(McpSession::connect(config).await?))
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
        self.session.call_no_args("get_version").await
    }

    /// The server's wall-clock time, used by W0 to compute the agent's local-clock
    /// offset.
    pub async fn get_server_time(&self) -> Result<serde_json::Value, CTraderError> {
        self.session.call_no_args("get_server_time").await
    }

    /// Confirms round-trip liveness, if exposed by the live build.
    pub async fn ping(&self) -> Result<serde_json::Value, CTraderError> {
        self.session.call_no_args("ping").await
    }

    // -----------------------------------------------------------------------------
    // Account state
    // -----------------------------------------------------------------------------

    /// The active account's balance/equity/margin snapshot, in Remote's raw
    /// `10^money_digits`-scaled encoding — see [`RemoteBalanceResponse`]'s
    /// `display_*` helpers.
    pub async fn get_balance(&self) -> Result<RemoteBalanceResponse, CTraderError> {
        self.session.call_no_args("get_balance").await
    }

    /// Resolves `assetId` values (`depositAssetId`, `baseAssetId`, `quoteAssetId`) to
    /// currency names. Stable for the session — cache it (`references/remote-http-
    /// server.md` "Symbol cache discipline" applies equally to this endpoint).
    pub async fn get_assets(&self) -> Result<GetAssetsResponse, CTraderError> {
        self.session.call_no_args("get_assets").await
    }

    // -----------------------------------------------------------------------------
    // Symbols & static metadata
    // -----------------------------------------------------------------------------

    /// The full symbol universe (`symbolId` <-> `symbolName` plus static metadata).
    /// Stable for the session — cache it, do not re-fetch per tool call
    /// (`references/remote-http-server.md` "Symbol cache discipline").
    pub async fn get_symbols(&self) -> Result<GetSymbolsResponse, CTraderError> {
        self.session.call_no_args("get_symbols").await
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
        self.session
            .call(
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
        self.session.call("get_trendbars", params).await
    }

    // -----------------------------------------------------------------------------
    // Positions, orders, deals
    // -----------------------------------------------------------------------------

    /// Every open position AND every pending order in one call (contrast with Local,
    /// which splits these across `get_positions`/`get_pending_orders`).
    pub async fn get_positions(&self) -> Result<GetPositionsResponse, CTraderError> {
        self.session.call_no_args("get_positions").await
    }

    /// A single position's full detail: the position itself, its related orders, and
    /// its related deals.
    pub async fn get_position_details(
        &self,
        position_id: i64,
    ) -> Result<GetPositionDetailsResponse, CTraderError> {
        self.session
            .call(
                "get_position_details",
                GetPositionDetailsParams { position_id },
            )
            .await
    }

    /// Every working (not yet filled) order.
    pub async fn get_pending_orders(&self) -> Result<GetPendingOrdersResponse, CTraderError> {
        self.session.call_no_args("get_pending_orders").await
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
        self.session.call("get_order_history", params).await
    }

    /// Deal (execution) history over `[from_timestamp, to_timestamp)`, page-capped by
    /// `max_rows` (default 50). Same `Q-R7`/`Q-R11` caveats as
    /// [`Self::get_order_history`].
    pub async fn get_deals(
        &self,
        params: GetDealsParams,
    ) -> Result<GetDealsResponse, CTraderError> {
        self.session.call("get_deals", params).await
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
        self.session.call("create_order", params).await
    }

    /// Amends a pending order's price/SL/TP/expiry.
    pub async fn amend_order(
        &self,
        params: AmendOrderParams,
    ) -> Result<serde_json::Value, CTraderError> {
        self.session.call("amend_order", params).await
    }

    /// Cancels a single working order.
    pub async fn cancel_order(&self, order_id: i64) -> Result<serde_json::Value, CTraderError> {
        self.session
            .call("cancel_order", CancelOrderParams { order_id })
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
        self.session.call("amend_position", params).await
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
        self.session.call("close_position", params).await
    }
}
