//! The named recovery patterns from `self-healing-playbook.md` §5, implemented as
//! reusable functions so [`crate::workflows`] (and any caller) never has to restate
//! them inline.
//!
//! Every function here names the quirk it works around and the pattern ID from the
//! playbook in its doc comment, per the skill's own DRY principle: "Patterns are named
//! so workflows and reference docs can refer to them without restating the body."

use crate::common::Period;
use crate::error::CTraderError;
use crate::local::dto::PendingOrder;
use crate::remote::RemoteClient;
use crate::remote::dto::{AmendPositionParams, AmendPositionResponse, CreateOrderParams};

// ---------------------------------------------------------------------------------
// P-AMEND-SAFE (§5.1): read-then-amend with BOTH SL+TP always present
// ---------------------------------------------------------------------------------

/// Amends a Remote position's SL and/or TP while preserving whichever leg the caller
/// doesn't specify, by reading the position's current legs first.
///
/// Implements **P-AMEND-SAFE** end to end: [`AmendPositionParams::new`] already forces
/// both legs to be populated in the request; this function is the "read current value
/// to preserve the untouched leg" half of the pattern, plus the post-flight check that
/// both legs survived the round-trip (`Q-R10` is silent at the wire level: only a
/// re-read catches a violation).
///
/// Pass `None` for whichever leg the caller wants left unchanged; pass `Some(price)`
/// (an absolute display price, not pipettes) for the leg being changed.
///
/// # Errors
///
/// - [`CTraderError::Invariant`] if the position doesn't exist, or if neither a new
///   value nor a preservable current value is available for a leg (e.g. the position
///   currently has no SL and the caller didn't request one), or if the post-flight
///   re-read shows either leg missing (a `Q-R10` violation slipping through).
/// - Any error [`RemoteClient::get_position_details`] or [`RemoteClient::amend_position`]
///   can return.
pub async fn amend_position_preserving_legs(
    client: &RemoteClient,
    position_id: i64,
    new_stop_loss: Option<f64>,
    new_take_profit: Option<f64>,
) -> Result<AmendPositionResponse, CTraderError> {
    let details = client.get_position_details(position_id).await?;
    let position = details
        .position
        .ok_or_else(|| CTraderError::Invariant(format!("position {position_id} not found")))?;

    let stop_loss = new_stop_loss.or(position.stop_loss).ok_or_else(|| {
        CTraderError::Invariant(format!(
            "position {position_id} has no current stopLoss to preserve, and none was provided"
        ))
    })?;
    let take_profit = new_take_profit.or(position.take_profit).ok_or_else(|| {
        CTraderError::Invariant(format!(
            "position {position_id} has no current takeProfit to preserve, and none was provided"
        ))
    })?;

    let result = client
        .amend_position(AmendPositionParams::new(
            position_id,
            stop_loss,
            take_profit,
        ))
        .await?;

    if let Some(amended) = &result.position
        && (amended.stop_loss.is_none() || amended.take_profit.is_none())
    {
        return Err(CTraderError::Invariant(format!(
            "P-AMEND-SAFE violation: amend_position on position {position_id} returned with a \
             missing leg (stop_loss={:?}, take_profit={:?}) despite both being sent: re-issue \
             the amend or escalate per the unknown-quirk decision tree",
            amended.stop_loss, amended.take_profit
        )));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------------
// P-REMOTE-MARKET-2STEP (§5.2): MARKET without SL/TP, then amend_position
// ---------------------------------------------------------------------------------

/// Opens a Remote `MARKET` position with absolute SL/TP applied via a follow-up
/// `amend_position`, per `Q-R4`'s two-step fallback pattern.
///
/// Use this ONLY when the user stated SL/TP as absolute prices that can't be cleanly
/// converted to point offsets: prefer [`market_with_relative_sl_tp`] (**P-REMOTE-
/// MARKET-RELATIVE**) whenever the SL/TP is expressible as a pip/point distance, since
/// that pattern lands both legs atomically with no unprotected window. Between this
/// function's fill and its amend call, the position is genuinely unprotected: for
/// high-volatility instruments or large size, prefer the relative pattern instead.
///
/// # Errors
///
/// [`CTraderError::Invariant`] if `create_order`'s response doesn't include a
/// `position_id` to amend (unexpected: normally means the order didn't fill
/// synchronously; re-read `get_positions` to locate it before retrying this function).
pub async fn market_two_step_open(
    client: &RemoteClient,
    symbol_id: i64,
    trade_side: crate::common::TradeSide,
    volume_cents: i64,
    stop_loss: f64,
    take_profit: f64,
    label: Option<String>,
) -> Result<AmendPositionResponse, CTraderError> {
    let mut params = CreateOrderParams::market(symbol_id, trade_side, volume_cents);
    if let Some(label) = label {
        params = params.with_label(label);
    }
    let created = client.create_order(params).await?;
    let position_id = created
        .position
        .as_ref()
        .and_then(|p| p.position_id)
        .ok_or_else(|| {
            CTraderError::Invariant(
            "P-REMOTE-MARKET-2STEP step 1: create_order response had no position_id to amend in \
             step 2: poll get_positions to locate the fill before retrying"
                .to_owned(),
        )
        })?;

    amend_position_preserving_legs(client, position_id, Some(stop_loss), Some(take_profit)).await
}

// ---------------------------------------------------------------------------------
// P-REMOTE-MARKET-RELATIVE (§5.6): single-call MARKET with relative SL/TP (preferred)
// ---------------------------------------------------------------------------------

/// Builds a `MARKET` order with SL/TP expressed as pip distances, converted to Remote's
/// integer POINTS encoding and passed as `relativeStopLoss`/`relativeTakeProfit`: the
/// preferred, atomic, single-call replacement for [`market_two_step_open`] (`Q-R4`).
///
/// `pip_digits` is the symbol's pipette precision from `get_symbols` (NOT the pip size
/// itself): see [`crate::math::pip::pips_to_points`].
pub fn market_with_relative_sl_tp(
    symbol_id: i64,
    trade_side: crate::common::TradeSide,
    volume_cents: i64,
    sl_pips: i64,
    tp_pips: i64,
    pip_digits: u32,
) -> CreateOrderParams {
    CreateOrderParams::market_with_relative_sl_tp(
        symbol_id,
        trade_side,
        volume_cents,
        crate::math::pip::pips_to_points(sl_pips, pip_digits),
        crate::math::pip::pips_to_points(tp_pips, pip_digits),
    )
}

// ---------------------------------------------------------------------------------
// P-LOCAL-OLDEST-FIRST (§5.4): reverse getIndicatorValues
// ---------------------------------------------------------------------------------

/// Reverses a Local `getIndicatorValues` result so index `0` is the most recent bar,
/// per `Q-L9` (the server returns the array oldest-first).
pub fn local_oldest_first(mut values: Vec<f64>) -> Vec<f64> {
    values.reverse();
    values
}

// ---------------------------------------------------------------------------------
// Q-L2: normalize the asymmetric SL/TP shape on Local's get_pending_orders
// ---------------------------------------------------------------------------------

/// Decodes a Local [`PendingOrder`]'s asymmetric SL/TP shape (`Q-L2`: `stop_loss` is an
/// absolute price, but `take_profit` is a RAW PIP DISTANCE from `entry_price`) into two
/// absolute prices, so both legs compare/display uniformly.
///
/// `pip_size` and `trade_side` (`"Buy"`/`"Sell"`, as echoed by the response: see
/// `Q-L3`) determine the sign applied to `take_profit`'s pip distance. Returns `None`
/// for a leg the order doesn't have.
pub fn normalize_pending_order_take_profit(order: &PendingOrder, pip_size: f64) -> Option<f64> {
    let entry = order.entry_price?;
    let take_profit_pips = order.take_profit?;
    let side = crate::common::TradeSide::parse_any_case(order.trade_side.as_deref()?)?;
    Some(crate::math::pip::pips_to_price(
        entry,
        take_profit_pips.round() as i64,
        pip_size,
        side,
        crate::math::pip::PriceLeg::TakeProfit,
        // `digits` only affects rounding precision of the output; 8 is generous enough
        // not to lose information for any realistic symbol's pip_size.
        8,
    ))
}

// ---------------------------------------------------------------------------------
// P-REMOTE-HISTORY-CHUNK (§5.5): 720h windowed loop
// ---------------------------------------------------------------------------------

const REMOTE_HISTORY_WINDOW_MS: i64 = 720 * 3_600 * 1_000;

/// How many bars one `get_trendbars` call returns at most. Checked against a live server
/// (2026-09): a window holding more bars than this comes back cut to its newest 100 bars with
/// `hasMore: false`, so the flag cannot be trusted and a full page must be followed by another
/// request for the part before its oldest bar.
pub const REMOTE_TRENDBARS_PAGE_CAP: usize = 100;

/// Whether a `get_trendbars` answer may have left bars out: the server says so, or the page is
/// as full as a page can be (see [`REMOTE_TRENDBARS_PAGE_CAP`]).
pub fn remote_trendbars_may_continue(has_more: bool, returned: usize) -> bool {
    has_more || returned >= REMOTE_TRENDBARS_PAGE_CAP
}

/// Splits `[from_epoch_ms, to_epoch_ms)` into windows no wider than Remote's 720-hour
/// history cap (`Q-R7`), in chronological order. The 1.0.18 rejection hint explicitly
/// states the resulting per-window calls can be issued in parallel; this function only
/// computes the windows: pacing/parallelism is the caller's choice (mind the Remote
/// historical-endpoint rate limit of 5 req/s if issuing them concurrently).
///
/// Returns a single `(from, to)` window unchanged if the span already fits.
pub fn remote_history_windows(from_epoch_ms: i64, to_epoch_ms: i64) -> Vec<(i64, i64)> {
    if to_epoch_ms <= from_epoch_ms {
        return Vec::new();
    }
    let mut windows = Vec::new();
    let mut cursor = from_epoch_ms;
    while cursor < to_epoch_ms {
        let window_end = (cursor + REMOTE_HISTORY_WINDOW_MS).min(to_epoch_ms);
        windows.push((cursor, window_end));
        cursor = window_end;
    }
    windows
}

// ---------------------------------------------------------------------------------
// Q-L4: Local get_trendbars pagination windows
// ---------------------------------------------------------------------------------

/// Splits `[from_epoch_ms, to_epoch_ms)` into windows sized so each one requests at most
/// 1000 bars of `period` granularity (`Q-L4`: Local silently truncates a wider request
/// and sets `truncated: true` rather than rejecting it, so unlike
/// [`remote_history_windows`] this is a proactive sizing choice, not a hard requirement,
/// but sizing windows this way means the caller never has to inspect `truncated` and
/// retry).
pub fn local_trendbar_windows(
    from_epoch_ms: i64,
    to_epoch_ms: i64,
    period: Period,
) -> Vec<(i64, i64)> {
    if to_epoch_ms <= from_epoch_ms {
        return Vec::new();
    }
    let window_ms = 1000 * period.approx_minutes() * 60_000;
    let mut windows = Vec::new();
    let mut cursor = from_epoch_ms;
    while cursor < to_epoch_ms {
        let window_end = (cursor + window_ms).min(to_epoch_ms);
        windows.push((cursor, window_end));
        cursor = window_end;
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_oldest_first_reverses() {
        assert_eq!(local_oldest_first(vec![1.0, 2.0, 3.0]), vec![3.0, 2.0, 1.0]);
    }

    #[test]
    fn a_full_page_may_continue_even_when_the_server_says_no_more() {
        assert!(remote_trendbars_may_continue(
            false,
            REMOTE_TRENDBARS_PAGE_CAP
        ));
        assert!(remote_trendbars_may_continue(true, 3));
        assert!(!remote_trendbars_may_continue(
            false,
            REMOTE_TRENDBARS_PAGE_CAP - 1
        ));
    }

    #[test]
    fn remote_history_windows_single_window_when_span_fits() {
        let windows = remote_history_windows(0, REMOTE_HISTORY_WINDOW_MS - 1);
        assert_eq!(windows, vec![(0, REMOTE_HISTORY_WINDOW_MS - 1)]);
    }

    #[test]
    fn remote_history_windows_splits_wider_span() {
        let span = REMOTE_HISTORY_WINDOW_MS * 2 + 1000;
        let windows = remote_history_windows(0, span);
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0], (0, REMOTE_HISTORY_WINDOW_MS));
        assert_eq!(
            windows[1],
            (REMOTE_HISTORY_WINDOW_MS, REMOTE_HISTORY_WINDOW_MS * 2)
        );
        assert_eq!(windows[2], (REMOTE_HISTORY_WINDOW_MS * 2, span));
    }

    #[test]
    fn local_trendbar_windows_sizes_by_1000_bars() {
        let windows = local_trendbar_windows(0, 1000 * 60_000 * 3, Period::M1);
        // 1000 M1 bars = 1000 minutes per window; a 3000-minute span needs 3 windows.
        assert_eq!(windows.len(), 3);
    }

    #[test]
    fn empty_or_inverted_span_yields_no_windows() {
        assert!(remote_history_windows(100, 100).is_empty());
        assert!(remote_history_windows(200, 100).is_empty());
    }
}
