//! Request/response DTOs for the Local server's trading, account, market-data, history,
//! chart-lifecycle, drawing-object, and indicator surface.
//!
//! Every response DTO carries `#[serde(flatten)] pub extra: JsonObject` so a field this
//! crate doesn't yet model (or a future server build adds) survives decoding instead of
//! being silently dropped: inspect `extra` for anything not exposed as a named field.
//! Request DTOs deliberately do **not** have this catch-all: only fields this crate
//! declares are ever serialized and sent, which is the schema-fields-only pre-flight
//! gate (`self-healing-playbook.md` §1.5) enforced structurally rather than by a runtime
//! check.

use rmcp::model::JsonObject;
use serde::{Deserialize, Serialize};

use crate::common::{Period, TradeSide};

// ---------------------------------------------------------------------------------
// Connection & diagnostics
// ---------------------------------------------------------------------------------

/// Response shape for `get_server_time`. The exact field name for the timestamp itself
/// is not pinned down by the skill's reference docs (only that the tool exists and
/// should be preferred over the agent's local clock: `Q-L8`); this DTO captures every
/// field via `extra` and [`crate::local::client::LocalClient::get_server_time`] extracts
/// the timestamp defensively (`serverTime` or `time`, whichever the live build uses).
#[derive(Debug, Clone, Deserialize)]
pub struct ServerTimeResponse {
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Accounts & balance
// ---------------------------------------------------------------------------------

/// One entry from `get_accounts_list`. Per `Q-L15`, the currently active account may be
/// **absent** from this list: resolve the active account via
/// [`BalanceResponse::trader_id`] instead.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountSummary {
    #[serde(alias = "traderId")]
    pub trader_id: Option<i64>,
    #[serde(alias = "accountNumber")]
    pub account_number: Option<i64>,
    pub broker: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_accounts_list`.
#[derive(Debug, Clone, Deserialize)]
pub struct GetAccountsListResponse {
    #[serde(default)]
    pub accounts: Vec<AccountSummary>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_balance`.
#[derive(Debug, Clone, Deserialize)]
pub struct BalanceResponse {
    /// The authoritative active-account identifier (`Q-L15`).
    #[serde(alias = "traderId")]
    pub trader_id: Option<i64>,
    pub balance: Option<f64>,
    pub equity: Option<f64>,
    pub margin: Option<f64>,
    #[serde(alias = "freeMargin")]
    pub free_margin: Option<f64>,
    /// `None` when the account has zero open positions: this is a **normal** sentinel,
    /// not a missing-data error (`Q-L18`). Callers must not treat it as a stop-out.
    #[serde(alias = "marginLevel")]
    pub margin_level: Option<f64>,
    /// Look for `"Hedged"` to detect a hedging account per
    /// `SKILL.md` "Hedging vs netting accounts".
    #[serde(alias = "accountType")]
    pub account_type: Option<String>,
    /// The live server calls this `depositAsset`.
    #[serde(alias = "depositAsset")]
    pub currency: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_account_statistics`. Per `Q-L12`, `available` may be `false`
/// instead of populated statistics: check it before consuming any other field.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountStatisticsResponse {
    pub available: bool,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Symbols & market data
// ---------------------------------------------------------------------------------

/// Query parameters for `get_symbols`.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GetSymbolsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

/// One entry in `get_symbols`' result set.
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolSummary {
    /// The live server calls this `name`.
    #[serde(alias = "symbolName", alias = "name")]
    pub symbol_name: String,
    pub description: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetSymbolsResponse {
    pub symbols: Vec<SymbolSummary>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Query parameters for `get_symbol_details`.
#[derive(Debug, Clone, Serialize)]
pub struct GetSymbolDetailsParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
}

/// Response shape for `get_symbol_details`: the authoritative per-symbol precision the
/// `assets/symbol_precision_table.json` baseline exists to approximate. Always prefer
/// this live response; the baseline is a fallback only (`Q-L1`).
#[derive(Debug, Clone, Deserialize)]
pub struct SymbolDetails {
    /// The live server calls this `name`.
    #[serde(alias = "symbolName", alias = "name")]
    pub symbol_name: Option<String>,
    /// Base-asset units per lot. **Broker-dependent**: do not assume `100_000` (`Q-L1`).
    #[serde(alias = "lotSize")]
    pub lot_size: Option<f64>,
    #[serde(alias = "minVolume")]
    pub min_volume: Option<f64>,
    #[serde(alias = "volumeStep")]
    pub volume_step: Option<f64>,
    /// Decimal places in the display price.
    pub digits: Option<u32>,
    /// Price increment per pip: do not assume `0.0001` (`references/local-http-server.md`
    /// "Price encoding on Local").
    #[serde(alias = "pipSize")]
    pub pip_size: Option<f64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Query parameters for `get_symbol_sessions`.
#[derive(Debug, Clone, Serialize)]
pub struct GetSymbolSessionsParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SymbolSessionsResponse {
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Query parameters for `get_spot_prices` (Local takes one symbol per call: see
/// `SKILL.md` "Currency conversion" for the Local-vs-Remote batching contrast).
#[derive(Debug, Clone, Serialize)]
pub struct GetSpotPricesParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
}

/// Response shape for `get_spot_prices`: a live quote in **display** prices (Local
/// never uses pipettes).
#[derive(Debug, Clone, Deserialize)]
pub struct SpotPriceResponse {
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub timestamp: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Query parameters for `get_trendbars`.
#[derive(Debug, Clone, Serialize)]
pub struct GetTrendbarsParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
    pub period: Period,
    /// ISO 8601 with mandatory `Z`: see [`crate::time::to_local_iso8601_z`] (`Q-L8`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// Capped at 1000 per request; a request for more is silently truncated with
    /// `truncated: true` on the response (`Q-L4`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trendbar {
    pub timestamp: Option<String>,
    pub open: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub close: Option<f64>,
    pub volume: Option<f64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_trendbars`. When `truncated` is `true`, more bars exist in
/// the requested window than were returned: loop with an advanced `from`/`to` window
/// per `Q-L4`.
#[derive(Debug, Clone, Deserialize)]
pub struct TrendbarsResponse {
    #[serde(default)]
    pub bars: Vec<Trendbar>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Trading: positions
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Position {
    /// Response field name for the position identifier (`Q-L6`: NOT `positionId`, even
    /// though every request-side field that targets a position is named `positionId`).
    pub id: Option<i64>,
    #[serde(alias = "symbolName")]
    pub symbol_name: Option<String>,
    #[serde(alias = "tradeSide")]
    pub trade_side: Option<String>,
    pub volume: Option<f64>,
    #[serde(alias = "entryPrice")]
    pub entry_price: Option<f64>,
    #[serde(alias = "stopLoss")]
    pub stop_loss: Option<f64>,
    #[serde(alias = "takeProfit")]
    pub take_profit: Option<f64>,
    pub swap: Option<f64>,
    pub commission: Option<f64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetPositionsResponse {
    #[serde(default)]
    pub positions: Vec<Position>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// How the server reads a `volume` argument. Every Local tool that takes a volume requires
/// this next to it (live server, 2026-09): `units` is the base-asset count the symbol's
/// `minVolume`, `maxVolume` and `volumeStep` are also expressed in, `lots` is multiplied by
/// the symbol's `lotSize` server side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VolumeType {
    /// The volume is a number of lots.
    Lots,
    /// The volume is a number of base-asset units.
    #[default]
    Units,
}

/// Parameters for `place_market_order`. `stopLossPips`/`takeProfitPips` are **pip
/// distances**, not absolute prices: see `references/local-http-server.md` "Stop loss
/// and take profit semantics on Local" (`Q-L2`).
#[derive(Debug, Clone, Serialize)]
pub struct PlaceMarketOrderParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
    /// Accepted case-insensitively by the server (`Q-L3`); this crate always sends
    /// [`TradeSide::as_local_input`]'s lowercase form.
    pub side: String,
    pub volume: f64,
    /// Required by the server; [`VolumeType::Units`] unless set otherwise.
    #[serde(rename = "volumeType")]
    pub volume_type: VolumeType,
    #[serde(rename = "stopLossPips", skip_serializing_if = "Option::is_none")]
    pub stop_loss_pips: Option<i64>,
    #[serde(rename = "takeProfitPips", skip_serializing_if = "Option::is_none")]
    pub take_profit_pips: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl PlaceMarketOrderParams {
    pub fn new(symbol_name: impl Into<String>, side: TradeSide, volume: f64) -> Self {
        Self {
            symbol_name: symbol_name.into(),
            side: side.as_local_input().to_owned(),
            volume,
            volume_type: VolumeType::default(),
            stop_loss_pips: None,
            take_profit_pips: None,
            label: None,
            comment: None,
        }
    }
}

/// Response shape for every `place_*_order` tool. Per `Q-L5`, this is genuinely all the
/// server echoes: no volume, price, or SL/TP confirmation. ALWAYS re-read via
/// `get_positions`/`get_pending_orders` after placement (see
/// `self-healing-playbook.md` §2.3).
#[derive(Debug, Clone, Deserialize)]
pub struct PlaceOrderResponse {
    #[serde(alias = "orderId")]
    pub order_id: Option<i64>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Parameters for `amend_position` (an OPEN position). Unlike Remote's `amend_position`,
/// Local does not omit-remove an unset leg (`Q-R10` is Remote-only), but this crate
/// still recommends always passing both legs (read current values first) for symmetry
/// and to catch any future behavior change immediately via the post-flight re-read.
#[derive(Debug, Clone, Serialize)]
pub struct AmendPositionParams {
    #[serde(rename = "positionId")]
    pub position_id: i64,
    #[serde(rename = "stopLoss", skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,
    #[serde(rename = "takeProfit", skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClosePositionParams {
    #[serde(rename = "positionId")]
    pub position_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClosePositionPartialParams {
    #[serde(rename = "positionId")]
    pub position_id: i64,
    pub volume: f64,
    /// Required by the server; [`VolumeType::Units`] unless set otherwise.
    #[serde(rename = "volumeType")]
    pub volume_type: VolumeType,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CloseAllPositionsParams {
    #[serde(rename = "symbolName", skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
}

/// Generic acknowledgement shape used by several mutation tools that don't echo
/// meaningful content beyond a status/message.
#[derive(Debug, Clone, Deserialize)]
pub struct Acknowledgement {
    pub status: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Trading: pending orders
// ---------------------------------------------------------------------------------

/// One entry from `get_pending_orders`.
///
/// **Response-shape asymmetry (`Q-L2`, CRITICAL):** `stop_loss` is an ABSOLUTE PRICE,
/// but `take_profit` is a RAW PIP DISTANCE from `entry_price`: these two fields are NOT
/// symmetric with each other despite the naming. Use
/// [`crate::quirks::normalize_pending_order_take_profit`] before comparing or displaying
/// `take_profit` as a price.
#[derive(Debug, Clone, Deserialize)]
pub struct PendingOrder {
    /// Response field name for the order identifier (`Q-L6`: `id`, not `orderId`).
    pub id: Option<i64>,
    #[serde(alias = "symbolName")]
    pub symbol_name: Option<String>,
    /// PascalCase on the wire (`"Buy"`/`"Sell"`) per `Q-L3`.
    #[serde(alias = "tradeSide")]
    pub trade_side: Option<String>,
    /// PascalCase on the wire (`"Limit"`/`"Stop"`/`"StopLimit"`) per `Q-L3`.
    #[serde(alias = "orderType")]
    pub order_type: Option<String>,
    pub volume: Option<f64>,
    #[serde(alias = "entryPrice")]
    pub entry_price: Option<f64>,
    /// Overloads `limitPrice` (LIMIT) or `stopPrice` (STOP_LIMIT) from the placement
    /// input, per `Q-L6`'s input/response field-name map.
    #[serde(alias = "targetPrice")]
    pub target_price: Option<f64>,
    /// ABSOLUTE PRICE (see the struct-level `Q-L2` note: asymmetric with `take_profit`).
    #[serde(alias = "stopLoss")]
    pub stop_loss: Option<f64>,
    /// RAW PIP DISTANCE from `entry_price` (see the struct-level `Q-L2` note).
    #[serde(alias = "takeProfit")]
    pub take_profit: Option<f64>,
    /// Response field name for the expiry (`Q-L6`: `expiration`, not `expiresAt`).
    pub expiration: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetPendingOrdersResponse {
    #[serde(default)]
    pub orders: Vec<PendingOrder>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Parameters shared by `place_limit_order`, `place_stop_order`, and
/// `place_stop_limit_order`. Set `limit_price` for LIMIT, `stop_price` for STOP, and
/// both for STOP_LIMIT: the caller-facing helpers on [`crate::local::LocalClient`]
/// enforce which combination each order type needs.
#[derive(Debug, Clone, Serialize)]
pub struct PlacePendingOrderParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
    pub side: String,
    pub volume: f64,
    /// Required by the server; [`VolumeType::Units`] unless set otherwise.
    #[serde(rename = "volumeType")]
    pub volume_type: VolumeType,
    #[serde(rename = "limitPrice", skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    #[serde(rename = "stopPrice", skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    #[serde(rename = "stopLossPips", skip_serializing_if = "Option::is_none")]
    pub stop_loss_pips: Option<i64>,
    #[serde(rename = "takeProfitPips", skip_serializing_if = "Option::is_none")]
    pub take_profit_pips: Option<i64>,
    /// `"trade"` (default) or `"opposite"`: only meaningful on `place_stop_order` /
    /// `place_stop_limit_order`. See `references/local-http-server.md` "Stop-order
    /// `triggerMethod`".
    #[serde(rename = "triggerMethod", skip_serializing_if = "Option::is_none")]
    pub trigger_method: Option<String>,
    /// ISO 8601 with mandatory `Z` (`Q-L8`).
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl PlacePendingOrderParams {
    pub fn new(symbol_name: impl Into<String>, side: TradeSide, volume: f64) -> Self {
        Self {
            symbol_name: symbol_name.into(),
            side: side.as_local_input().to_owned(),
            volume,
            volume_type: VolumeType::default(),
            limit_price: None,
            stop_price: None,
            stop_loss_pips: None,
            take_profit_pips: None,
            trigger_method: None,
            expires_at: None,
            label: None,
            comment: None,
        }
    }
}

/// `trigger_method` values for `place_stop_order` / `place_stop_limit_order`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMethod {
    /// Trigger on the actual trade-side quote (default).
    Trade,
    /// Trigger on the opposite-side quote (ask for sells, bid for buys): reduces
    /// premature triggers from spread spikes during news or low-liquidity sessions.
    Opposite,
}

impl TriggerMethod {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::Opposite => "opposite",
        }
    }
}

/// Parameters for `amend_order` (a PENDING order): pip-distance SL/TP, per
/// `references/local-http-server.md`'s pip/absolute split table.
#[derive(Debug, Clone, Serialize)]
pub struct AmendOrderParams {
    #[serde(rename = "orderId")]
    pub order_id: i64,
    #[serde(rename = "limitPrice", skip_serializing_if = "Option::is_none")]
    pub limit_price: Option<f64>,
    #[serde(rename = "stopPrice", skip_serializing_if = "Option::is_none")]
    pub stop_price: Option<f64>,
    #[serde(rename = "stopLossPips", skip_serializing_if = "Option::is_none")]
    pub stop_loss_pips: Option<i64>,
    #[serde(rename = "takeProfitPips", skip_serializing_if = "Option::is_none")]
    pub take_profit_pips: Option<i64>,
    #[serde(rename = "expiresAt", skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CancelOrderParams {
    #[serde(rename = "orderId")]
    pub order_id: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CancelAllPendingOrdersParams {
    #[serde(rename = "symbolName", skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
}

// ---------------------------------------------------------------------------------
// Trade history
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Trade {
    #[serde(alias = "dealId")]
    pub deal_id: Option<i64>,
    #[serde(alias = "orderId")]
    pub order_id: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

/// Response shape for `get_order_history`. Per `Q-L7`, executed trades are under the
/// `trades` key, NOT `orders`: this struct is named to match the wire key directly so
/// there's no chance of reaching for a nonexistent `orders` field.
#[derive(Debug, Clone, Deserialize)]
pub struct GetOrderHistoryResponse {
    #[serde(default)]
    pub trades: Vec<Trade>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GetDealsParams {
    /// Capped at 200 per request: loop with a timestamp-advanced window and dedupe by
    /// `dealId` for larger spans (`Q-L4`-adjacent pagination note in
    /// `references/local-http-server.md`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(rename = "symbolName", skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Deal {
    #[serde(alias = "dealId")]
    pub deal_id: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetDealsResponse {
    #[serde(default)]
    pub deals: Vec<Deal>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// Charts: lifecycle & navigation
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ChartSummary {
    #[serde(alias = "chartId")]
    pub chart_id: Option<i64>,
    #[serde(alias = "symbolName")]
    pub symbol_name: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListChartsResponse {
    #[serde(default)]
    pub charts: Vec<ChartSummary>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct FocusChartParams {
    #[serde(rename = "chartId")]
    pub chart_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeChartSymbolParams {
    #[serde(rename = "symbolName")]
    pub symbol_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChangeChartTimeframeParams {
    pub period: Period,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChartViewport {
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScrollChartParams {
    /// Bars to scroll by (positive = forward in time, negative = backward). The exact
    /// parameter name is not pinned down by the skill's reference docs beyond the
    /// capability description; verify against the live schema.
    pub bars: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZoomChartParams {
    /// Positive to zoom in, negative to zoom out. The exact parameter name is not
    /// pinned down by the skill's reference docs beyond the capability description;
    /// verify against the live schema.
    pub steps: i64,
}

// ---------------------------------------------------------------------------------
// Charts: drawing objects
// ---------------------------------------------------------------------------------

/// The `object_type` values `add_chart_object` accepts, grouped by the anchor pattern
/// `references/local-http-server.md` "Drawing object anchor requirements" (`Q-L10`)
/// documents for each. Passing the wrong combination of price/time anchors for a given
/// type silently produces an ill-positioned (not rejected) object: always pass every
/// anchor the type's row calls for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartObjectType {
    // One-point (price1 + time1).
    HorizontalLine,
    Text,
    StaticText,
    UpArrow,
    DownArrow,
    Circle,
    Square,
    Diamond,
    Star,
    UpTriangle,
    DownTriangle,
    // Time-only (time1).
    VerticalLine,
    // Two-point (price1+time1, price2+time2), supports `extend`.
    TrendLine,
    Ray,
    ArrowLine,
    // Two-point geometry (price1+time1, price2+time2), some support `fill`.
    EquidistantChannel,
    Rectangle,
    Ellipse,
    FibonacciRetracement,
    FibonacciFan,
    FibonacciArcs,
    FibonacciTimezones,
    GannFan,
    GannBox,
    GannSquare,
    GannSquareFixed,
    // Three-point (p1+t1, p2+t2, p3+t3).
    Triangle,
    FibonacciExpansion,
    AndrewsPitchfork,
    // Trade visualization: side, p1=entry, p2=SL, p3=TP, t1=block_start, t2=block_end.
    RiskReward,
}

/// Parameters for `add_chart_object`. See [`ChartObjectType`] and
/// `references/local-http-server.md`'s anchor-requirement table (`Q-L10`) for which
/// combination of `price1`/`time1`/`price2`/`time2`/`price3`/`time3` a given
/// `object_type` needs: this crate does not validate the combination client-side
/// (the server accepts an incomplete anchor set silently and mis-positions the object,
/// per `Q-L10`, so client-side validation could not catch this anyway; always re-read
/// via `get_chart_objects` after placement to verify the visual position).
#[derive(Debug, Clone, Serialize)]
pub struct AddChartObjectParams {
    /// Wire field name is literally `object_type` (snake_case) per the skill's `Q-L10`
    /// example: this is the one documented exception to Local's otherwise-camelCase
    /// parameter naming convention.
    #[serde(rename = "object_type")]
    pub object_type: ChartObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price1: Option<f64>,
    /// ISO 8601 with mandatory `Z` (`Q-L8`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price2: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time2: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price3: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time3: Option<String>,
    /// `trend_line` / `ray` / `arrow_line` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extend: Option<bool>,
    /// `rectangle` / `ellipse` / `triangle` only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<bool>,
    /// `risk_reward` only: `"buy"` or `"sell"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
}

impl AddChartObjectParams {
    pub fn new(object_type: ChartObjectType) -> Self {
        Self {
            object_type,
            price1: None,
            time1: None,
            price2: None,
            time2: None,
            price3: None,
            time3: None,
            extend: None,
            fill: None,
            side: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChartObject {
    #[serde(alias = "objectId")]
    pub object_id: Option<i64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GetChartObjectsResponse {
    #[serde(default)]
    pub objects: Vec<ChartObject>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteChartObjectParams {
    #[serde(rename = "objectId")]
    pub object_id: i64,
}

// ---------------------------------------------------------------------------------
// Charts: indicators
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct IndicatorSummary {
    #[serde(alias = "indicatorId")]
    pub indicator_id: Option<i64>,
    pub name: Option<String>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListChartIndicatorsResponse {
    #[serde(default)]
    pub indicators: Vec<IndicatorSummary>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddChartIndicatorParams {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonObject>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveChartIndicatorParams {
    #[serde(rename = "indicatorId")]
    pub indicator_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateIndicatorParametersParams {
    #[serde(rename = "indicatorId")]
    pub indicator_id: i64,
    pub parameters: JsonObject,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetIndicatorValuesParams {
    #[serde(rename = "indicatorId")]
    pub indicator_id: i64,
    #[serde(rename = "outputIndex", skip_serializing_if = "Option::is_none")]
    pub output_index: Option<u32>,
    /// Capped at 1000 values per request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// Response shape for `getIndicatorValues`. Per `Q-L9`, `values` is OLDEST-first:
/// apply [`crate::quirks::local_oldest_first`] (**P-LOCAL-OLDEST-FIRST**) before
/// charting or signal generation.
#[derive(Debug, Clone, Deserialize)]
pub struct GetIndicatorValuesResponse {
    #[serde(default)]
    pub values: Vec<f64>,
    #[serde(flatten)]
    pub extra: JsonObject,
}

// ---------------------------------------------------------------------------------
// UI: notifications
// ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ShowNotificationParams {
    pub message: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn volume_orders_always_carry_a_volume_type() {
        let market = PlaceMarketOrderParams::new("BTCUSD", TradeSide::Buy, 0.01);
        let value = serde_json::to_value(&market).unwrap();
        assert_eq!(value["volumeType"], "units");
        assert_eq!(value["volume"], 0.01);

        let mut pending = PlacePendingOrderParams::new("EURUSD", TradeSide::Sell, 1.0);
        pending.volume_type = VolumeType::Lots;
        assert_eq!(
            serde_json::to_value(&pending).unwrap()["volumeType"],
            "lots"
        );

        let partial = ClosePositionPartialParams {
            position_id: 7,
            volume: 1000.0,
            volume_type: VolumeType::default(),
        };
        assert_eq!(
            serde_json::to_value(&partial).unwrap()["volumeType"],
            "units"
        );
    }

    /// A `get_balance` answer as the live Local server sent it (2026-09, values altered).
    #[test]
    fn balance_reads_the_live_shape() {
        let balance: BalanceResponse = serde_json::from_value(json!({
            "accountName": null,
            "accountType": "Hedged",
            "balance": 958.06,
            "brokerName": "Spotware",
            "connectionState": "Authenticated",
            "depositAsset": "EUR",
            "equity": 958.06,
            "freeMargin": 958.06,
            "grossProfit": 0,
            "isSwapFree": false,
            "leverage": 100,
            "margin": 0,
            "marginLevel": null,
            "netProfit": 0,
            "traderId": 3_382_707
        }))
        .unwrap();
        assert_eq!(balance.currency.as_deref(), Some("EUR"));
        assert_eq!(balance.trader_id, Some(3_382_707));
        assert_eq!(balance.margin_level, None);
    }
}
