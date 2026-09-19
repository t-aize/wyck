//! Request/response DTOs for every documented `ctrader-remote-mcp` tool.
//!
//! Every response DTO carries `#[serde(flatten)] pub extra: JsonObject` so an
//! undocumented or future field survives decoding: see the equivalent note on
//! [`crate::local::dto`]. Every money field on a response DTO is Remote's raw
//! `10^moneyDigits`-scaled integer encoding; every price field is raw integer pipettes
//! (`Q-K19`): decode with [`crate::common::money_from_raw`] /
//! [`crate::common::price_from_pipettes`] before displaying, never before re-submitting
//! (mutation DTOs re-encode from display values themselves).

use rmcp::model::JsonObject;
use serde::{Deserialize, Serialize};

use crate::common::TradeSide;
use crate::error::CTraderError;
use crate::time::RemoteTimestamp;

// ---------------------------------------------------------------------------------
// Version & diagnostics
// ---------------------------------------------------------------------------------

/// Response shape for `get_version`: the build identifier workflow W0 compares against
/// this crate's documented minimum build (`rest-proxy 1.0.18` as of the last audit) to
/// decide whether the `known-quirks.md` workarounds still apply.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionResponse {
    pub version: Option<String>,
    #[serde(alias = "buildTime")]
    pub build_time: Option<String>,
    pub service: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Account state
// ---------------------------------------------------------------------------------

/// Response shape for `get_balance`. Every money field is raw
/// `10^money_digits`-scaled: decode with [`Self::display_balance`] /
/// [`Self::display_equity`] / [`Self::display_free_margin`].
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteBalanceResponse {
    #[serde(alias = "traderId")]
    pub trader_id: Option<i64>,
    pub balance: Option<i64>,
    pub equity: Option<i64>,
    #[serde(alias = "freeMargin")]
    pub free_margin: Option<i64>,
    /// Number of fractional digits every money field on this response (and every other
    /// Remote response, per `references/remote-http-server.md` "Money encoding on
    /// Remote") is scaled by. Cache this once per session (W0): do not re-fetch it per
    /// money field.
    #[serde(alias = "moneyDigits")]
    pub money_digits: Option<u32>,
    /// Resolve to a currency name via [`crate::remote::RemoteClient::get_assets`]'s
    /// cached map.
    #[serde(alias = "depositAssetId")]
    pub deposit_asset_id: Option<i64>,
    /// Monotonic counter incremented on every balance-affecting event: compare two
    /// snapshots to detect that account state changed between them.
    #[serde(alias = "balanceVersion")]
    pub balance_version: Option<i64>,
    /// Look for `"Hedged"` to detect a hedging account.
    #[serde(alias = "accountType")]
    pub account_type: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

impl RemoteBalanceResponse {
    /// Decodes `balance` to a display value using this response's own `money_digits`.
    /// Returns `None` if either field is absent.
    pub fn display_balance(&self) -> Option<f64> {
        Some(crate::common::money_from_raw(
            self.balance?,
            self.money_digits?,
        ))
    }

    /// Decodes `equity` to a display value using this response's own `money_digits`.
    pub fn display_equity(&self) -> Option<f64> {
        Some(crate::common::money_from_raw(
            self.equity?,
            self.money_digits?,
        ))
    }

    /// Decodes `free_margin` to a display value using this response's own
    /// `money_digits`.
    pub fn display_free_margin(&self) -> Option<f64> {
        Some(crate::common::money_from_raw(
            self.free_margin?,
            self.money_digits?,
        ))
    }
}

/// One entry from `get_assets`: resolves `assetId` (as seen on `depositAssetId`,
/// `baseAssetId`, `quoteAssetId`) to a currency name (e.g. `"USD"`).
#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    #[serde(alias = "assetId")]
    pub asset_id: Option<i64>,
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetAssetsResponse {
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Symbols & static metadata
// ---------------------------------------------------------------------------------

/// One entry from `get_symbols`. This response is stable for the session: cache it
/// (`references/remote-http-server.md` "Symbol cache discipline") rather than re-fetching
/// per tool call.
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteSymbol {
    #[serde(alias = "symbolId")]
    pub symbol_id: i64,
    #[serde(alias = "symbolName")]
    pub symbol_name: String,
    pub enabled: Option<bool>,
    #[serde(alias = "baseAssetId")]
    pub base_asset_id: Option<i64>,
    #[serde(alias = "quoteAssetId")]
    pub quote_asset_id: Option<i64>,
    #[serde(alias = "symbolCategoryId")]
    pub symbol_category_id: Option<i64>,
    pub description: Option<String>,
    /// Decimal places for this symbol's pipette-encoded price fields: required to
    /// decode/encode every price this symbol appears in (`Q-K19`).
    #[serde(alias = "pipDigits")]
    pub pip_digits: Option<u32>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetSymbolsResponse {
    #[serde(default)]
    pub symbols: Vec<RemoteSymbol>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Live and historical market data
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GetSpotPricesParams {
    #[serde(rename = "symbolId")]
    pub symbol_id: Vec<i64>,
}

/// One quote from `get_spot_prices`. `bid`/`ask` are raw pipettes (`Q-K19`): decode
/// with [`crate::common::price_from_pipettes`] using the symbol's cached `pip_digits`.
#[derive(Debug, Clone, Deserialize)]
pub struct SpotPrice {
    #[serde(alias = "symbolId")]
    pub symbol_id: Option<i64>,
    pub bid: Option<i64>,
    pub ask: Option<i64>,
    /// Epoch milliseconds.
    pub timestamp: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_spot_prices`. Per `Q-R8`, a SINGLE unknown `symbolId` in the
/// request batch causes the ENTIRE `prices` array to come back empty: validate every
/// requested id against the cached `get_symbols` map before calling this, never send a
/// mixed batch and assume partial success.
#[derive(Debug, Clone, Deserialize)]
pub struct GetSpotPricesResponse {
    #[serde(default)]
    pub prices: Vec<SpotPrice>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetTrendbarsParams {
    #[serde(rename = "symbolId")]
    pub symbol_id: i64,
    pub period: crate::common::Period,
    /// Accepts either epoch milliseconds or an ISO 8601 string on this endpoint.
    #[serde(rename = "fromTimestamp", skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<RemoteTimestamp>,
    #[serde(rename = "toTimestamp", skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<RemoteTimestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// One bar from `get_trendbars`. Price fields are raw pipettes (`Q-K19`).
#[derive(Debug, Clone, Deserialize)]
pub struct RemoteTrendbar {
    /// Epoch milliseconds.
    pub timestamp: Option<i64>,
    pub open: Option<i64>,
    pub high: Option<i64>,
    pub low: Option<i64>,
    pub close: Option<i64>,
    pub volume: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_trendbars`. `has_more: true` means the window spans more
/// data than one page returned: advance the window per **P-REMOTE-HISTORY-CHUNK**
/// (`Q-R7`).
#[derive(Debug, Clone, Deserialize)]
pub struct GetTrendbarsResponse {
    #[serde(default)]
    pub trendbars: Vec<RemoteTrendbar>,
    #[serde(alias = "hasMore", default)]
    pub has_more: bool,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Positions, orders, deals
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct RemotePosition {
    #[serde(alias = "positionId")]
    pub position_id: Option<i64>,
    #[serde(alias = "symbolId")]
    pub symbol_id: Option<i64>,
    #[serde(alias = "tradeSide")]
    pub trade_side: Option<String>,
    /// Cents.
    pub volume: Option<i64>,
    /// Raw pipettes.
    #[serde(alias = "entryPrice")]
    pub entry_price: Option<i64>,
    /// Raw pipettes.
    #[serde(alias = "stopLoss")]
    pub stop_loss: Option<i64>,
    /// Raw pipettes.
    #[serde(alias = "takeProfit")]
    pub take_profit: Option<i64>,
    #[serde(alias = "trailingStopLoss")]
    pub trailing_stop_loss: Option<bool>,
    /// Raw `10^moneyDigits`-scaled.
    pub swap: Option<i64>,
    /// Raw `10^moneyDigits`-scaled.
    pub commission: Option<i64>,
    /// Raw `10^moneyDigits`-scaled.
    #[serde(alias = "unrealizedPnl")]
    pub unrealized_pnl: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteOrder {
    #[serde(alias = "orderId")]
    pub order_id: Option<i64>,
    #[serde(alias = "symbolId")]
    pub symbol_id: Option<i64>,
    #[serde(alias = "orderType")]
    pub order_type: Option<String>,
    #[serde(alias = "tradeSide")]
    pub trade_side: Option<String>,
    pub volume: Option<i64>,
    #[serde(alias = "limitPrice")]
    pub limit_price: Option<i64>,
    #[serde(alias = "stopPrice")]
    pub stop_price: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_positions`. Unlike Local, Remote returns BOTH positions AND
/// pending orders in one call (`references/trader-workflows.md` W4).
#[derive(Debug, Clone, Deserialize)]
pub struct GetPositionsResponse {
    #[serde(default)]
    pub positions: Vec<RemotePosition>,
    #[serde(default)]
    pub orders: Vec<RemoteOrder>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetPositionDetailsParams {
    #[serde(rename = "positionId")]
    pub position_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetPositionDetailsResponse {
    pub position: Option<RemotePosition>,
    #[serde(default)]
    pub orders: Vec<RemoteOrder>,
    #[serde(default)]
    pub deals: Vec<Deal>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetPendingOrdersResponse {
    #[serde(default)]
    pub orders: Vec<RemoteOrder>,
    #[serde(alias = "hasMore", default)]
    pub has_more: bool,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GetOrderHistoryParams {
    #[serde(rename = "fromTimestamp", skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<RemoteTimestamp>,
    #[serde(rename = "toTimestamp", skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<RemoteTimestamp>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetOrderHistoryResponse {
    #[serde(default)]
    pub orders: Vec<RemoteOrder>,
    #[serde(alias = "hasMore", default)]
    pub has_more: bool,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// `dealStatus` values on [`Deal`]. See `references/remote-http-server.md` "`dealStatus`
/// enum" for the full follow-up-action table this crate's doc comments mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DealStatus {
    /// Fully filled: proceed.
    Filled,
    /// Partially filled; `volume` vs `filled_volume` differ: surface the gap.
    PartiallyFilled,
    /// Broker rejected before any fill: do not retry blindly.
    Rejected,
    /// Server-side rejection before reaching the broker: treat like `Rejected`.
    InternallyRejected,
    /// Execution attempt errored out: treat like `Rejected`; re-read
    /// `get_position_details` before further action, the position state may or may not
    /// have moved.
    Error,
    /// The execution opportunity was missed (e.g. a gap past a stop-limit window). No
    /// fill occurred.
    Missed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Deal {
    #[serde(alias = "dealId")]
    pub deal_id: Option<i64>,
    #[serde(alias = "positionId")]
    pub position_id: Option<i64>,
    #[serde(alias = "tradeSide")]
    pub trade_side: Option<String>,
    pub volume: Option<i64>,
    #[serde(alias = "filledVolume")]
    pub filled_volume: Option<i64>,
    #[serde(alias = "executionPrice")]
    pub execution_price: Option<i64>,
    #[serde(alias = "executionTimestamp")]
    pub execution_timestamp: Option<i64>,
    #[serde(alias = "dealStatus")]
    pub deal_status: Option<DealStatus>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetDealsParams {
    #[serde(rename = "fromTimestamp", skip_serializing_if = "Option::is_none")]
    pub from_timestamp: Option<RemoteTimestamp>,
    #[serde(rename = "toTimestamp", skip_serializing_if = "Option::is_none")]
    pub to_timestamp: Option<RemoteTimestamp>,
    /// Default `50` if unset. Caps the page size: loop with an advanced
    /// `from_timestamp` while `has_more` is `true`.
    #[serde(rename = "maxRows", skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetDealsResponse {
    #[serde(default)]
    pub deals: Vec<Deal>,
    #[serde(alias = "hasMore", default)]
    pub has_more: bool,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Trading mutations
// ---------------------------------------------------------------------------------

/// `orderType` on `create_order`. See `references/remote-http-server.md` "Order type
/// enum" for the conditional-required price fields each variant needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemoteOrderType {
    /// Fills immediately at the current bid/ask. **Rejects absolute `stopLoss`/
    /// `takeProfit`** (`Q-R4`): use `relativeStopLoss`/`relativeTakeProfit` instead
    /// (see [`CreateOrderParams::market_with_relative_sl_tp`]), or the two-step
    /// **P-REMOTE-MARKET-2STEP** pattern via [`crate::quirks::market_two_step_open`].
    Market,
    /// Fills only at `limitPrice` or better. Requires `limitPrice`.
    Limit,
    /// Triggers as a market order when price crosses `stopPrice`. Requires `stopPrice`.
    Stop,
    /// Fills at market within an acceptable slippage band. Requires
    /// `slippageInPoints`; absolute SL/TP acceptance is build-dependent (`Q-R4-RANGE`):
    /// probe before relying on it, or use the relative form.
    MarketRange,
    /// Triggers at `stopPrice`, then submits a limit at `limitPrice`. Requires both.
    StopLimit,
}

/// `timeInForce` on `create_order`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TimeInForce {
    GoodTillCancel,
    /// Requires `expirationTimestamp`.
    GoodTillDate,
    /// Per `Q-R5`, behaves like a working LIMIT rather than cancelling the unfilled
    /// remainder: do not rely on this for cancel-remainder semantics; post-flight
    /// `cancel_order` any residual if strict IOC behavior is required.
    ImmediateOrCancel,
}

/// Parameters for `create_order`.
///
/// This struct structurally prevents the two most consequential Remote quirks:
///
/// - **`Q-R3`** (`trailingStopLoss` silently dropped by `create_order`): there is no
///   `trailing_stop_loss` field here at all: trailing SL is only ever exposed on
///   [`AmendPositionParams`], which is the only tool that honors it.
/// - **`Q-R4`** (MARKET rejects absolute SL/TP): [`Self::validate`] rejects a `MARKET`
///   or not-yet-probed `MARKET_RANGE` order that sets `stop_loss`/`take_profit` instead
///   of `relative_stop_loss`/`relative_take_profit` *before* the request ever reaches
///   the network.
///
/// Construct via [`Self::market`], [`Self::limit`], or [`Self::stop`] and the `with_*`
/// builders, or via [`Self::market_with_relative_sl_tp`] for the common "MARKET entry
/// with SL/TP as a point offset" case (**P-REMOTE-MARKET-RELATIVE**, the preferred
/// single-call pattern).
#[derive(Debug, Clone, Serialize)]
pub struct CreateOrderParams {
    #[serde(rename = "symbolId")]
    pub symbol_id: i64,
    #[serde(rename = "orderType")]
    pub order_type: RemoteOrderType,
    #[serde(rename = "tradeSide")]
    pub trade_side: TradeSide,
    /// Cents.
    pub volume: i64,
    #[serde(rename = "limitPrice", skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<i64>,
    #[serde(rename = "stopPrice", skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<i64>,
    #[serde(rename = "stopLoss", skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<i64>,
    #[serde(rename = "takeProfit", skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<i64>,
    /// Positive integer POINTS offset from fill price (mutually exclusive with
    /// `stop_loss`). Only honored on `MARKET`/`MARKET_RANGE`: see `Q-R4`.
    #[serde(rename = "relativeStopLoss", skip_serializing_if = "Option::is_none")]
    pub relative_stop_loss: Option<i64>,
    #[serde(rename = "relativeTakeProfit", skip_serializing_if = "Option::is_none")]
    pub relative_take_profit: Option<i64>,
    #[serde(rename = "slippageInPoints", skip_serializing_if = "Option::is_none")]
    pub slippage_in_points: Option<i64>,
    #[serde(rename = "baseSlippagePrice", skip_serializing_if = "Option::is_none")]
    pub base_slippage_price: Option<i64>,
    #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    /// Integer epoch milliseconds ONLY (`Q-R2`): an ISO string here is rejected.
    #[serde(
        rename = "expirationTimestamp",
        skip_serializing_if = "Option::is_none"
    )]
    pub expiration_timestamp: Option<RemoteTimestamp>,
    /// Server-enforced `<= 100` characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Server-enforced `<= 256` characters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl CreateOrderParams {
    /// A minimal `MARKET` order with no SL/TP (`P-REMOTE-MARKET-2STEP` step 1): fill
    /// first, then apply SL/TP via [`AmendPositionParams`].
    pub fn market(symbol_id: i64, trade_side: TradeSide, volume_cents: i64) -> Self {
        Self {
            symbol_id,
            order_type: RemoteOrderType::Market,
            trade_side,
            volume: volume_cents,
            limit_price: None,
            stop_price: None,
            stop_loss: None,
            take_profit: None,
            relative_stop_loss: None,
            relative_take_profit: None,
            slippage_in_points: None,
            base_slippage_price: None,
            time_in_force: None,
            expiration_timestamp: None,
            label: None,
            comment: None,
        }
    }

    /// A `MARKET` order with SL/TP expressed as integer POINT offsets from the fill
    /// price: the preferred single-call **P-REMOTE-MARKET-RELATIVE** pattern for
    /// `Q-R4` (no race window, unlike the two-step fallback). Use
    /// [`crate::math::pip::pips_to_points`] to convert a user-stated pip distance.
    pub fn market_with_relative_sl_tp(
        symbol_id: i64,
        trade_side: TradeSide,
        volume_cents: i64,
        relative_stop_loss_points: i64,
        relative_take_profit_points: i64,
    ) -> Self {
        Self {
            relative_stop_loss: Some(relative_stop_loss_points),
            relative_take_profit: Some(relative_take_profit_points),
            ..Self::market(symbol_id, trade_side, volume_cents)
        }
    }

    /// A `LIMIT` order with absolute SL/TP: Tier 1 (preferred) per
    /// `references/trader-workflows.md` W1; absolute SL/TP IS accepted at creation on
    /// this order type (`Q-R4` affects `MARKET` only).
    pub fn limit(
        symbol_id: i64,
        trade_side: TradeSide,
        volume_cents: i64,
        limit_price_pipettes: i64,
    ) -> Self {
        Self {
            order_type: RemoteOrderType::Limit,
            limit_price: Some(limit_price_pipettes),
            ..Self::market(symbol_id, trade_side, volume_cents)
        }
    }

    /// A `STOP` order with absolute SL/TP.
    pub fn stop(
        symbol_id: i64,
        trade_side: TradeSide,
        volume_cents: i64,
        stop_price_pipettes: i64,
    ) -> Self {
        Self {
            order_type: RemoteOrderType::Stop,
            stop_price: Some(stop_price_pipettes),
            ..Self::market(symbol_id, trade_side, volume_cents)
        }
    }

    #[must_use]
    pub fn with_absolute_stop_loss(mut self, price_pipettes: i64) -> Self {
        self.stop_loss = Some(price_pipettes);
        self
    }

    #[must_use]
    pub fn with_absolute_take_profit(mut self, price_pipettes: i64) -> Self {
        self.take_profit = Some(price_pipettes);
        self
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = Some(comment.into());
        self
    }

    #[must_use]
    pub fn with_expiration(mut self, timestamp: RemoteTimestamp) -> Self {
        self.time_in_force = Some(TimeInForce::GoodTillDate);
        self.expiration_timestamp = Some(timestamp);
        self
    }

    /// Client-side pre-flight validation implementing `self-healing-playbook.md` §1
    /// gates that are structurally checkable without a network round-trip:
    ///
    /// - **`Q-R4`**: `MARKET`/`MARKET_RANGE` orders may not combine absolute
    ///   `stop_loss`/`take_profit` with the relative fields, and (since this crate has
    ///   no way to know the live Q-R4-RANGE probe result from inside a pure DTO method)
    ///   flags absolute SL/TP on `MARKET` outright (always rejected) and leaves
    ///   `MARKET_RANGE` to the server (may be accepted, depending on build).
    /// - **§1.7 required runtime fields**: `LIMIT` requires `limit_price`; `STOP`
    ///   requires `stop_price`; `STOP_LIMIT` requires both; `GoodTillDate` requires
    ///   `expiration_timestamp`.
    /// - **§1.5 schema-fields-only**: `relative_*` and absolute `stop_loss`/
    ///   `take_profit` are mutually exclusive per field per the schema.
    /// - Remote's server-side length constraints: `label <= 100` chars, `comment <= 256`
    ///   chars.
    ///
    /// # Errors
    ///
    /// Returns [`CTraderError::PreFlightRejected`] describing the first violated
    /// invariant.
    pub fn validate(&self) -> Result<(), CTraderError> {
        let reject = |message: String| {
            Err(CTraderError::PreFlightRejected {
                tool: "create_order".into(),
                message,
            })
        };

        if self.order_type == RemoteOrderType::Market
            && (self.stop_loss.is_some() || self.take_profit.is_some())
        {
            return reject(
                "Q-R4: create_order with orderType=MARKET rejects absolute stopLoss/takeProfit; \
                 use relativeStopLoss/relativeTakeProfit (CreateOrderParams::market_with_relative_sl_tp) \
                 or the P-REMOTE-MARKET-2STEP two-step pattern instead."
                    .to_owned(),
            );
        }
        if (self.relative_stop_loss.is_some() && self.stop_loss.is_some())
            || (self.relative_take_profit.is_some() && self.take_profit.is_some())
        {
            return reject(
                "relative and absolute stopLoss/takeProfit are mutually exclusive per leg"
                    .to_owned(),
            );
        }
        match self.order_type {
            RemoteOrderType::Limit if self.limit_price.is_none() => {
                return reject("orderType=LIMIT requires limitPrice".to_owned());
            }
            RemoteOrderType::Stop if self.stop_price.is_none() => {
                return reject("orderType=STOP requires stopPrice".to_owned());
            }
            RemoteOrderType::StopLimit
                if self.stop_price.is_none() || self.limit_price.is_none() =>
            {
                return reject(
                    "orderType=STOP_LIMIT requires both stopPrice and limitPrice".to_owned(),
                );
            }
            _ => {}
        }
        if self.time_in_force == Some(TimeInForce::GoodTillDate)
            && self.expiration_timestamp.is_none()
        {
            return reject("timeInForce=GOOD_TILL_DATE requires expirationTimestamp".to_owned());
        }
        if let Some(label) = &self.label
            && label.chars().count() > 100
        {
            return reject("label must be <= 100 characters".to_owned());
        }
        if let Some(comment) = &self.comment
            && comment.chars().count() > 256
        {
            return reject("comment must be <= 256 characters".to_owned());
        }
        Ok(())
    }
}

/// Response shape for `create_order`. Per `Q-R11`, prefer these embedded objects over a
/// follow-up `get_deals`/`get_order_history` call for immediate post-mutation
/// verification: history endpoints can lag behind the mutation by seconds to minutes.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateOrderResponse {
    pub order: Option<RemoteOrder>,
    pub position: Option<RemotePosition>,
    pub deal: Option<Deal>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmendOrderParams {
    #[serde(rename = "orderId")]
    pub order_id: i64,
    #[serde(rename = "limitPrice", skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<i64>,
    #[serde(rename = "stopPrice", skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<i64>,
    #[serde(rename = "stopLoss", skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<i64>,
    #[serde(rename = "takeProfit", skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<i64>,
    /// Integer epoch milliseconds ONLY (`Q-R2`).
    #[serde(
        rename = "expirationTimestamp",
        skip_serializing_if = "Option::is_none"
    )]
    pub expiration_timestamp: Option<RemoteTimestamp>,
    #[serde(rename = "slippageInPoints", skip_serializing_if = "Option::is_none")]
    pub slippage_in_points: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CancelOrderParams {
    #[serde(rename = "orderId")]
    pub order_id: i64,
}

/// Parameters for `amend_position`.
///
/// **Always construct via [`Self::new`]**, which requires both `stop_loss` and
/// `take_profit` up front: this structurally enforces **P-AMEND-SAFE**
/// (`self-healing-playbook.md` §5.1): per `Q-R10`, *omitting* either field REMOVES that
/// leg (the opposite of "leaves unchanged"), and passing `null` is rejected outright.
/// There is no builder path that allows constructing this struct with only one leg set.
/// To preserve a leg, read its current value via `get_positions`/`get_position_details`
/// first and re-pass it unchanged: see [`crate::quirks::amend_position_preserving_legs`].
#[derive(Debug, Clone, Serialize)]
pub struct AmendPositionParams {
    #[serde(rename = "positionId")]
    pub position_id: i64,
    #[serde(rename = "stopLoss")]
    pub stop_loss: i64,
    #[serde(rename = "takeProfit")]
    pub take_profit: i64,
    /// Only honored here, never on `create_order`/`amend_order` (`Q-R3`). Requires
    /// `stop_loss` to be present as the trail anchor (always true on this struct).
    #[serde(rename = "trailingStopLoss", skip_serializing_if = "Option::is_none")]
    pub trailing_stop_loss: Option<bool>,
}

impl AmendPositionParams {
    /// Constructs an amend payload with BOTH legs populated, per **P-AMEND-SAFE**.
    pub fn new(position_id: i64, stop_loss_pipettes: i64, take_profit_pipettes: i64) -> Self {
        Self {
            position_id,
            stop_loss: stop_loss_pipettes,
            take_profit: take_profit_pipettes,
            trailing_stop_loss: None,
        }
    }

    /// Enables trailing SL, anchored on this payload's `stop_loss` (`Q-R3`).
    #[must_use]
    pub fn with_trailing_stop_loss(mut self, enabled: bool) -> Self {
        self.trailing_stop_loss = Some(enabled);
        self
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AmendPositionResponse {
    pub position: Option<RemotePosition>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Parameters for `close_position`. `volume` (cents) is REQUIRED on Remote: pass the
/// position's full open volume to close entirely (there is no "close all without
/// volume" form, unlike Local).
#[derive(Debug, Clone, Serialize)]
pub struct ClosePositionParams {
    #[serde(rename = "positionId")]
    pub position_id: i64,
    pub volume: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClosePositionResponse {
    pub deal: Option<Deal>,
    pub position: Option<RemotePosition>,
    #[serde(flatten)]
    pub extra: JsonObject,
}
