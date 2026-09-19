//! [`LocalClient`]: the typed wrapper around every documented `ctrader-local-mcp` tool.

use rmcp::model::JsonObject;
use serde_json::{Value, json};

use crate::common::TradeSide;
use crate::config::ConnectionConfig;
use crate::error::CTraderError;
use crate::local::dto::*;
use crate::transport::McpSession;

/// A connected session against a Local server (`ctrader-local-mcp`), bound to a running
/// cTrader Desktop application.
///
/// Construct via [`LocalClient::connect`], or wrap an already-open [`McpSession`] with
/// [`LocalClient::new`] if the session is shared with other server-family clients (not
/// applicable to Local/Remote, which are always distinct endpoints, but useful in tests).
pub struct LocalClient {
    session: McpSession,
}

fn as_object(value: Value) -> Option<JsonObject> {
    match value {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

impl LocalClient {
    /// Wraps an already-connected [`McpSession`].
    pub fn new(session: McpSession) -> Self {
        Self { session }
    }

    /// Opens a new session against the Local server at `config.uri` and wraps it.
    pub async fn connect(config: &ConnectionConfig) -> Result<Self, CTraderError> {
        Ok(Self::new(McpSession::connect(config).await?))
    }

    /// The underlying MCP session, for direct protocol access (`tools/list`, etc.) not
    /// covered by a typed method.
    pub fn session(&self) -> &McpSession {
        &self.session
    }

    /// Gracefully shuts down the underlying MCP session.
    pub async fn shutdown(self) -> Result<(), CTraderError> {
        self.session.shutdown().await
    }

    // -----------------------------------------------------------------------------
    // Connection & diagnostics
    // -----------------------------------------------------------------------------

    /// Confirms round-trip liveness with the Local server via a native MCP protocol
    /// ping: see [`McpSession::ping`] for why this is not (and, per a live probe,
    /// never was successfully) a `tools/call`.
    pub async fn ping(&self) -> Result<(), CTraderError> {
        self.session.ping().await
    }

    /// The server's wall-clock time: prefer this over the agent's local clock for
    /// every time-window computation (`Q-L8`).
    pub async fn get_server_time(&self) -> Result<ServerTimeResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("get_server_time")
            .await
    }

    // -----------------------------------------------------------------------------
    // Accounts & balance
    // -----------------------------------------------------------------------------

    /// Enumerates known accounts. Per `Q-L15`, the currently ACTIVE account may be
    /// absent from this list: resolve it via [`Self::get_balance`]'s `trader_id`
    /// instead of assuming it appears here.
    pub async fn get_accounts_list(&self) -> Result<GetAccountsListResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("get_accounts_list")
            .await
    }

    /// The active account's balance/equity/margin snapshot. `margin_level: None` is
    /// normal with zero open positions (`Q-L18`): do not treat it as an error.
    pub async fn get_balance(&self) -> Result<BalanceResponse, CTraderError> {
        self.session.call_no_args_idempotent("get_balance").await
    }

    /// Lifetime account statistics. Per `Q-L12`, may return `available: false` instead
    /// of populated data: check that flag before consuming any other field, and fall
    /// back to deriving the needed metric from in-session reads.
    pub async fn get_account_statistics(&self) -> Result<AccountStatisticsResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("get_account_statistics")
            .await
    }

    // -----------------------------------------------------------------------------
    // Symbols & market data
    // -----------------------------------------------------------------------------

    /// Enumerates the symbol universe, optionally narrowed by `filter` (e.g. `"XAU"` to
    /// find gold-related tickers when the user says "gold").
    pub async fn get_symbols(
        &self,
        filter: Option<&str>,
    ) -> Result<GetSymbolsResponse, CTraderError> {
        self.session
            .call_idempotent(
                "get_symbols",
                GetSymbolsParams {
                    filter: filter.map(str::to_owned),
                },
            )
            .await
    }

    /// The authoritative per-symbol precision (`lotSize`, `minVolume`, `volumeStep`,
    /// `digits`, `pipSize`): always call this before any volume- or price-bearing
    /// mutation on `symbol_name` (`Q-L1`); the `assets/symbol_precision_table.json`
    /// baseline this crate does not embed is a fallback only, never authoritative.
    pub async fn get_symbol_details(
        &self,
        symbol_name: &str,
    ) -> Result<SymbolDetails, CTraderError> {
        self.session
            .call_idempotent(
                "get_symbol_details",
                GetSymbolDetailsParams {
                    symbol_name: symbol_name.to_owned(),
                },
            )
            .await
    }

    /// The symbol's trading sessions: use this to detect a closed market before
    /// placing a pending order (`references/trader-workflows.md` W1 edge case).
    pub async fn get_symbol_sessions(
        &self,
        symbol_name: &str,
    ) -> Result<SymbolSessionsResponse, CTraderError> {
        self.session
            .call_idempotent(
                "get_symbol_sessions",
                GetSymbolSessionsParams {
                    symbol_name: symbol_name.to_owned(),
                },
            )
            .await
    }

    /// A live quote for one symbol, in display prices. Local takes one symbol per call
    /// (contrast with Remote's batched `get_spot_prices`).
    pub async fn get_spot_prices(
        &self,
        symbol_name: &str,
    ) -> Result<SpotPriceResponse, CTraderError> {
        self.session
            .call_idempotent(
                "get_spot_prices",
                GetSpotPricesParams {
                    symbol_name: symbol_name.to_owned(),
                },
            )
            .await
    }

    /// Historical OHLCV bars. Capped at 1000 bars per request; a wider request is
    /// silently truncated with `truncated: true` on the response (`Q-L4`): see
    /// [`crate::quirks::local_trendbar_windows`] for the pagination-window helper.
    pub async fn get_trendbars(
        &self,
        params: GetTrendbarsParams,
    ) -> Result<TrendbarsResponse, CTraderError> {
        self.session.call_idempotent("get_trendbars", params).await
    }

    // -----------------------------------------------------------------------------
    // Trading: positions
    // -----------------------------------------------------------------------------

    /// Every open position on the active account.
    pub async fn get_positions(&self) -> Result<GetPositionsResponse, CTraderError> {
        self.session.call_no_args_idempotent("get_positions").await
    }

    /// Places a market order. `params.stop_loss_pips`/`params.take_profit_pips` are pip
    /// distances, not absolute prices (`Q-L2`). Per `Q-L5`, the response is only
    /// `{orderId, status}`: ALWAYS re-read via [`Self::get_positions`] afterward to
    /// confirm the fill matched intent.
    pub async fn place_market_order(
        &self,
        params: PlaceMarketOrderParams,
    ) -> Result<PlaceOrderResponse, CTraderError> {
        self.session.call("place_market_order", params).await
    }

    /// Amends SL/TP on an OPEN position using ABSOLUTE prices (contrast with
    /// `place_*_order`'s pip-distance form). See `references/trader-workflows.md` W2 for
    /// the full read-then-amend-then-verify sequence this crate recommends even though
    /// Local (unlike Remote's `Q-R10`) does not omit-remove an unset leg.
    pub async fn amend_position(
        &self,
        params: AmendPositionParams,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session.call("amend_position", params).await
    }

    /// Fully closes a position (no volume parameter: Local always closes the entire
    /// remainder). For a partial close use [`Self::close_position_partial`].
    pub async fn close_position(&self, position_id: i64) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call("close_position", ClosePositionParams { position_id })
            .await
    }

    /// Partially closes a position by `volume` (base-asset units, rounded to the
    /// symbol's `volumeStep`: round DOWN toward smaller risk if the caller's requested
    /// volume doesn't divide evenly, per `self-healing-playbook.md` §1.4).
    pub async fn close_position_partial(
        &self,
        position_id: i64,
        volume: f64,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "close_position_partial",
                ClosePositionPartialParams {
                    position_id,
                    volume,
                    volume_type: VolumeType::default(),
                },
            )
            .await
    }

    /// Closes every open position, optionally scoped to one `symbol_name`. **Destructive:
    /// irreversible.** Always confirm the affected positions with the user first (see
    /// `references/local-http-server.md` "Destructive operations").
    pub async fn close_all_positions(
        &self,
        symbol_name: Option<&str>,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "close_all_positions",
                CloseAllPositionsParams {
                    symbol_name: symbol_name.map(str::to_owned),
                },
            )
            .await
    }

    // -----------------------------------------------------------------------------
    // Trading: pending orders
    // -----------------------------------------------------------------------------

    /// Every working (not yet filled) order. Per `Q-L2`, each entry's `stop_loss` is an
    /// absolute price but `take_profit` is a raw pip distance: see
    /// [`crate::quirks::normalize_pending_order_take_profit`] before comparing or
    /// displaying.
    pub async fn get_pending_orders(&self) -> Result<GetPendingOrdersResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("get_pending_orders")
            .await
    }

    /// Places a LIMIT order. `params.limit_price` must be set (buy-limit below current
    /// ask, sell-limit above current bid: pre-flight gate `self-healing-playbook.md`
    /// §1.2).
    pub async fn place_limit_order(
        &self,
        params: PlacePendingOrderParams,
    ) -> Result<PlaceOrderResponse, CTraderError> {
        self.session.call("place_limit_order", params).await
    }

    /// Places a STOP order. `params.stop_price` must be set (buy-stop above market,
    /// sell-stop below). Consider `trigger_method = "opposite"` during volatile/wide-
    /// spread sessions (`references/local-http-server.md` "Stop-order `triggerMethod`").
    pub async fn place_stop_order(
        &self,
        params: PlacePendingOrderParams,
    ) -> Result<PlaceOrderResponse, CTraderError> {
        self.session.call("place_stop_order", params).await
    }

    /// Places a STOP_LIMIT order. Both `params.stop_price` and `params.limit_price` must
    /// be set.
    pub async fn place_stop_limit_order(
        &self,
        params: PlacePendingOrderParams,
    ) -> Result<PlaceOrderResponse, CTraderError> {
        self.session.call("place_stop_limit_order", params).await
    }

    /// Amends a PENDING order (pip-distance SL/TP: contrast with
    /// [`Self::amend_position`]'s absolute-price form for OPEN positions).
    pub async fn amend_order(
        &self,
        params: AmendOrderParams,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session.call("amend_order", params).await
    }

    /// Cancels a single working order.
    pub async fn cancel_order(&self, order_id: i64) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call("cancel_order", CancelOrderParams { order_id })
            .await
    }

    /// Cancels every working order, optionally scoped to one `symbol_name`.
    /// **Destructive: irreversible.**
    pub async fn cancel_all_pending_orders(
        &self,
        symbol_name: Option<&str>,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "cancel_all_pending_orders",
                CancelAllPendingOrdersParams {
                    symbol_name: symbol_name.map(str::to_owned),
                },
            )
            .await
    }

    // -----------------------------------------------------------------------------
    // Trade history
    // -----------------------------------------------------------------------------

    /// Executed trade history CURRENTLY LOADED by the desktop client (the user may need
    /// to scroll the History tab in the cTrader UI to force older periods to load).
    /// Per `Q-L7`, results live under the `trades` key, not `orders`.
    pub async fn get_order_history(&self) -> Result<GetOrderHistoryResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("get_order_history")
            .await
    }

    /// Recent deals, capped at 200 per request: loop with a timestamp-advanced
    /// `symbol_name`-scoped window and dedupe by `deal_id` for a larger span.
    pub async fn get_deals(
        &self,
        params: GetDealsParams,
    ) -> Result<GetDealsResponse, CTraderError> {
        self.session.call_idempotent("get_deals", params).await
    }

    // -----------------------------------------------------------------------------
    // Charts: lifecycle & navigation
    // -----------------------------------------------------------------------------

    /// Enumerates open chart tabs.
    pub async fn list_charts(&self) -> Result<ListChartsResponse, CTraderError> {
        self.session.call_no_args_idempotent("list_charts").await
    }

    /// Switches chart focus. Every chart-mutating tool below acts on the FOCUSED chart,
    /// never on a `chartId` parameter: call this first, then re-confirm with
    /// [`Self::get_active_chart`] before issuing chart-mutating calls
    /// (`references/local-http-server.md` "Active chart focus model").
    pub async fn focus_chart(&self, chart_id: i64) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call("focus_chart", FocusChartParams { chart_id })
            .await
    }

    /// The currently focused chart: call this to re-confirm focus before any
    /// chart-mutating sequence.
    pub async fn get_active_chart(&self) -> Result<ChartSummary, CTraderError> {
        self.session
            .call_no_args_idempotent("get_active_chart")
            .await
    }

    /// Changes the FOCUSED chart's symbol.
    pub async fn change_chart_symbol(
        &self,
        symbol_name: &str,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "change_chart_symbol",
                ChangeChartSymbolParams {
                    symbol_name: symbol_name.to_owned(),
                },
            )
            .await
    }

    /// Changes the FOCUSED chart's timeframe.
    pub async fn change_chart_timeframe(
        &self,
        period: crate::common::Period,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "change_chart_timeframe",
                ChangeChartTimeframeParams { period },
            )
            .await
    }

    /// The FOCUSED chart's current viewport (visible time/price range).
    pub async fn get_chart_viewport(&self) -> Result<ChartViewport, CTraderError> {
        self.session
            .call_no_args_idempotent("get_chart_viewport")
            .await
    }

    /// Scrolls the FOCUSED chart by `bars` (positive = forward in time).
    pub async fn scroll_chart(&self, bars: i64) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call("scroll_chart", ScrollChartParams { bars })
            .await
    }

    /// Zooms the FOCUSED chart by `steps` (positive = zoom in).
    pub async fn zoom_chart(&self, steps: i64) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call("zoom_chart", ZoomChartParams { steps })
            .await
    }

    // -----------------------------------------------------------------------------
    // Charts: drawing objects
    // -----------------------------------------------------------------------------

    /// Adds a drawing object to the FOCUSED chart. See [`ChartObjectType`] and
    /// `references/local-http-server.md`'s anchor table (`Q-L10`) for which
    /// price/time anchors a given `object_type` needs. Always focus the target chart
    /// first (see [`Self::focus_chart`]) and re-read via [`Self::get_chart_objects`]
    /// after placement to confirm the visual position.
    pub async fn add_chart_object(
        &self,
        params: AddChartObjectParams,
    ) -> Result<ChartObject, CTraderError> {
        self.session.call("add_chart_object", params).await
    }

    /// Every drawing object currently on the FOCUSED chart.
    pub async fn get_chart_objects(&self) -> Result<GetChartObjectsResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("get_chart_objects")
            .await
    }

    /// Updates an existing drawing object on the FOCUSED chart. The parameter shape
    /// mirrors [`AddChartObjectParams`] but targets an existing `object_id`; exposed
    /// here via [`crate::transport::McpSession::call_raw`] because the skill's reference
    /// docs do not pin down which subset of fields an update call accepts versus a
    /// create call: pass only the fields you intend to change.
    pub async fn update_chart_object(
        &self,
        object_id: i64,
        fields: JsonObject,
    ) -> Result<Value, CTraderError> {
        let mut arguments = fields;
        arguments.insert("objectId".to_string(), json!(object_id));
        self.session
            .call_raw("update_chart_object", Some(arguments))
            .await
    }

    /// Deletes one drawing object from the FOCUSED chart.
    pub async fn delete_chart_object(
        &self,
        object_id: i64,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call("delete_chart_object", DeleteChartObjectParams { object_id })
            .await
    }

    /// Deletes EVERY drawing object on the FOCUSED chart. **Destructive:
    /// irreversible.**
    pub async fn clear_chart_objects(&self) -> Result<Acknowledgement, CTraderError> {
        self.session.call_no_args("clear_chart_objects").await
    }

    // -----------------------------------------------------------------------------
    // Charts: indicators
    // -----------------------------------------------------------------------------

    /// The indicator catalog and/or currently attached indicators on the FOCUSED chart
    /// (the skill's capability description covers both; the exact response shape is not
    /// pinned down further: inspect `extra` on each [`IndicatorSummary`]).
    pub async fn list_chart_indicators(&self) -> Result<ListChartIndicatorsResponse, CTraderError> {
        self.session
            .call_no_args_idempotent("listChartIndicators")
            .await
    }

    /// Attaches an indicator to the FOCUSED chart.
    pub async fn add_chart_indicator(
        &self,
        params: AddChartIndicatorParams,
    ) -> Result<IndicatorSummary, CTraderError> {
        self.session.call("addChartIndicator", params).await
    }

    /// Removes an indicator from the FOCUSED chart.
    pub async fn remove_chart_indicator(
        &self,
        indicator_id: i64,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "removeChartIndicator",
                RemoveChartIndicatorParams { indicator_id },
            )
            .await
    }

    /// Mutates an attached indicator's parameters.
    pub async fn update_indicator_parameters(
        &self,
        indicator_id: i64,
        parameters: JsonObject,
    ) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "update_indicator_parameters",
                UpdateIndicatorParametersParams {
                    indicator_id,
                    parameters,
                },
            )
            .await
    }

    /// Reads an attached indicator's output values, capped at 1000 per request. Per
    /// `Q-L9`, the result is OLDEST-first: apply
    /// [`crate::quirks::local_oldest_first`] (**P-LOCAL-OLDEST-FIRST**) before charting
    /// or signal generation.
    pub async fn get_indicator_values(
        &self,
        params: GetIndicatorValuesParams,
    ) -> Result<GetIndicatorValuesResponse, CTraderError> {
        self.session
            .call_idempotent("getIndicatorValues", params)
            .await
    }

    // -----------------------------------------------------------------------------
    // Chart templates: capability named by the skill; exact wire tool names are NOT
    // pinned down beyond "save / list / apply / delete templates" plus the explicitly
    // named `save_chart_template`, `apply_chart_template`, and `delete_chart_template`
    // (the last from the destructive-operations list). `list_chart_templates` is this
    // crate's best-effort inferred name for the missing fourth verb: verify against
    // the live `tools/list` schema before depending on it.
    // -----------------------------------------------------------------------------

    /// Saves the FOCUSED chart's styling/indicator setup as a named template.
    pub async fn save_chart_template(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("save_chart_template", as_object(json!({ "name": name })))
            .await
    }

    /// Lists saved chart templates. Best-effort inferred tool name: see this section's
    /// module-level note.
    pub async fn list_chart_templates(&self) -> Result<Value, CTraderError> {
        self.session.call_raw("list_chart_templates", None).await
    }

    /// Applies a saved template to the FOCUSED chart.
    pub async fn apply_chart_template(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("apply_chart_template", as_object(json!({ "name": name })))
            .await
    }

    /// Deletes a saved chart template. **Destructive: irreversible.**
    pub async fn delete_chart_template(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("delete_chart_template", as_object(json!({ "name": name })))
            .await
    }

    // -----------------------------------------------------------------------------
    // Workspaces: capability named by the skill ("Save / load / delete the full UI
    // layout snapshot") plus `delete_workspace` from the destructive-operations list;
    // `save_workspace`/`load_workspace`/`list_workspaces` are this crate's best-effort
    // inferred names: verify against the live `tools/list` schema.
    // -----------------------------------------------------------------------------

    /// Saves the current full UI layout as a named workspace.
    pub async fn save_workspace(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("save_workspace", as_object(json!({ "name": name })))
            .await
    }

    /// Loads a named workspace, replacing the current UI layout.
    pub async fn load_workspace(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("load_workspace", as_object(json!({ "name": name })))
            .await
    }

    /// Lists saved workspaces.
    pub async fn list_workspaces(&self) -> Result<Value, CTraderError> {
        self.session.call_raw("list_workspaces", None).await
    }

    /// Deletes a saved workspace. **Destructive: irreversible.**
    pub async fn delete_workspace(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("delete_workspace", as_object(json!({ "name": name })))
            .await
    }

    // -----------------------------------------------------------------------------
    // UI: notifications
    // -----------------------------------------------------------------------------

    /// Surfaces a native notification in the cTrader Desktop UI.
    pub async fn show_notification(&self, message: &str) -> Result<Acknowledgement, CTraderError> {
        self.session
            .call(
                "show_notification",
                ShowNotificationParams {
                    message: message.to_owned(),
                },
            )
            .await
    }

    // -----------------------------------------------------------------------------
    // Watchlists: capability named by the skill ("enumeration, creation, renaming,
    // deletion, and symbol membership editing") plus `delete_watchlist` and
    // `remove_symbol_from_watchlist` (from the state-verification table). The
    // remaining verbs are this crate's best-effort inferred names: verify against
    // the live `tools/list` schema.
    // -----------------------------------------------------------------------------

    /// Enumerates the user's watchlists.
    pub async fn get_watchlists(&self) -> Result<Value, CTraderError> {
        self.session.call_raw("get_watchlists", None).await
    }

    /// Creates a new watchlist.
    pub async fn create_watchlist(&self, name: &str) -> Result<Value, CTraderError> {
        self.session
            .call_raw("create_watchlist", as_object(json!({ "name": name })))
            .await
    }

    /// Renames an existing watchlist.
    pub async fn rename_watchlist(
        &self,
        watchlist_id: i64,
        new_name: &str,
    ) -> Result<Value, CTraderError> {
        self.session
            .call_raw(
                "rename_watchlist",
                as_object(json!({ "watchlistId": watchlist_id, "name": new_name })),
            )
            .await
    }

    /// Deletes a watchlist, including its symbol membership. **Destructive:
    /// irreversible.**
    pub async fn delete_watchlist(&self, watchlist_id: i64) -> Result<Value, CTraderError> {
        self.session
            .call_raw(
                "delete_watchlist",
                as_object(json!({ "watchlistId": watchlist_id })),
            )
            .await
    }

    /// Adds a symbol to a watchlist.
    pub async fn add_symbol_to_watchlist(
        &self,
        watchlist_id: i64,
        symbol_name: &str,
    ) -> Result<Value, CTraderError> {
        self.session
            .call_raw(
                "add_symbol_to_watchlist",
                as_object(json!({ "watchlistId": watchlist_id, "symbolName": symbol_name })),
            )
            .await
    }

    /// Removes a symbol from a watchlist.
    pub async fn remove_symbol_from_watchlist(
        &self,
        watchlist_id: i64,
        symbol_name: &str,
    ) -> Result<Value, CTraderError> {
        self.session
            .call_raw(
                "remove_symbol_from_watchlist",
                as_object(json!({ "watchlistId": watchlist_id, "symbolName": symbol_name })),
            )
            .await
    }

    // -----------------------------------------------------------------------------
    // Price alerts: capability named by the skill ("List / create / delete
    // price-trigger alerts (above/below, bid/ask)") plus `delete_price_alert` (from the
    // destructive-operations list). `get_price_alerts`/`create_price_alert` are this
    // crate's best-effort inferred names: verify against the live `tools/list` schema.
    // Per `Q-L13`, response enum value-names (e.g. `conditionType: "GreaterOrEqual"`)
    // do NOT match the input enum (`condition: "above"`): this crate does not attempt
    // to normalize that mapping automatically since the skill does not enumerate every
    // pair; inspect the raw response and cross-reference `Q-L13` when consuming it.
    // -----------------------------------------------------------------------------

    /// Lists price alerts.
    pub async fn get_price_alerts(&self) -> Result<Value, CTraderError> {
        self.session.call_raw("get_price_alerts", None).await
    }

    /// Creates a price alert. `condition` is `"above"` or `"below"`; `price_type` is
    /// `"bid"` or `"ask"`.
    pub async fn create_price_alert(
        &self,
        symbol_name: &str,
        condition: &str,
        price_type: &str,
        price: f64,
    ) -> Result<Value, CTraderError> {
        self.session
            .call_raw(
                "create_price_alert",
                as_object(json!({
                    "symbolName": symbol_name,
                    "condition": condition,
                    "priceType": price_type,
                    "price": price,
                })),
            )
            .await
    }

    /// Deletes a price alert by id. **Destructive: irreversible.**
    pub async fn delete_price_alert(&self, alert_id: i64) -> Result<Value, CTraderError> {
        self.session
            .call_raw(
                "delete_price_alert",
                as_object(json!({ "alertId": alert_id })),
            )
            .await
    }

    // -----------------------------------------------------------------------------
    // cBot plugins: `listPlugins`, `startPlugin`, `stopPlugin` are literally named by
    // the skill's surface map (camelCase, matching `listChartIndicators` et al.).
    // -----------------------------------------------------------------------------

    /// Enumerates available cBots/plugins.
    pub async fn list_plugins(&self) -> Result<Value, CTraderError> {
        self.session.call_raw("listPlugins", None).await
    }

    /// Starts a cBot/plugin by id.
    pub async fn start_plugin(&self, plugin_id: i64) -> Result<Value, CTraderError> {
        self.session
            .call_raw("startPlugin", as_object(json!({ "pluginId": plugin_id })))
            .await
    }

    /// Stops a running cBot/plugin by id. **Destructive** in the sense that it
    /// interrupts a live automated strategy mid-execution: confirm with the user first.
    pub async fn stop_plugin(&self, plugin_id: i64) -> Result<Value, CTraderError> {
        self.session
            .call_raw("stopPlugin", as_object(json!({ "pluginId": plugin_id })))
            .await
    }

    // -----------------------------------------------------------------------------
    // Convenience: strongly-typed entry helpers matching `references/trader-workflows.md`
    // W1's pip-distance/absolute-price conventions, sparing callers from re-deriving
    // TradeSide -> wire-string mapping at every call site.
    // -----------------------------------------------------------------------------

    /// Builds [`PlaceMarketOrderParams`] with `side` already mapped to Local's
    /// case-insensitive wire form.
    pub fn market_order(
        symbol_name: impl Into<String>,
        side: TradeSide,
        volume: f64,
    ) -> PlaceMarketOrderParams {
        PlaceMarketOrderParams::new(symbol_name, side, volume)
    }

    /// Builds [`PlacePendingOrderParams`] with `side` already mapped to Local's
    /// case-insensitive wire form.
    pub fn pending_order(
        symbol_name: impl Into<String>,
        side: TradeSide,
        volume: f64,
    ) -> PlacePendingOrderParams {
        PlacePendingOrderParams::new(symbol_name, side, volume)
    }
}
