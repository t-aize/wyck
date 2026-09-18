//! **Pre-trade briefing** (`SKILL.md` recipe 2) — a symbol's static metadata, live
//! quote, and recent price history in one call, for "should I trade X" / "what does X
//! look like right now" style requests.

use crate::common::Period;
use crate::error::CTraderError;
use crate::remote::RemoteClient;
use crate::remote::dto::{GetTrendbarsParams, RemoteSymbol, RemoteTrendbar, SpotPrice};
use crate::time::RemoteTimestamp;
use crate::workflows::bootstrap::RemoteSessionContext;

/// A symbol's static metadata, live quote, and recent OHLCV bars, gathered in one
/// composite call.
#[derive(Debug, Clone)]
pub struct PreTradeBriefing {
    pub symbol: RemoteSymbol,
    pub spot: SpotPrice,
    pub recent_bars: Vec<RemoteTrendbar>,
}

/// Assembles a [`PreTradeBriefing`] for `symbol_name`, resolving it against `session`'s
/// cached symbol map (see [`RemoteSessionContext::find_symbol`] — populated by
/// [`crate::workflows::bootstrap_remote`]).
///
/// # Errors
///
/// [`CTraderError::Invariant`] if `symbol_name` isn't in the cached symbol map, or if
/// `get_spot_prices` returns no quote for it (per `Q-R8`, this can also mean the id is
/// unknown to the broker even though it appeared in `get_symbols` — e.g. a disabled or
/// stale entry).
pub async fn pre_trade_briefing(
    client: &RemoteClient,
    session: &RemoteSessionContext,
    symbol_name: &str,
    period: Period,
    bar_count: u32,
) -> Result<PreTradeBriefing, CTraderError> {
    let symbol = session.find_symbol(symbol_name).cloned().ok_or_else(|| {
        CTraderError::Invariant(format!(
            "symbol `{symbol_name}` not found in the cached get_symbols map"
        ))
    })?;

    let prices = client.get_spot_prices(vec![symbol.symbol_id]).await?;
    let spot = prices.prices.into_iter().next().ok_or_else(|| {
        CTraderError::Invariant(format!(
            "get_spot_prices returned no quote for `{symbol_name}` (symbolId {}) — see Q-R8",
            symbol.symbol_id
        ))
    })?;

    let bars = client
        .get_trendbars(GetTrendbarsParams {
            symbol_id: symbol.symbol_id,
            period,
            from_timestamp: None,
            to_timestamp: Some(RemoteTimestamp::epoch_millis(
                crate::time::now_epoch_millis(),
            )),
            count: Some(bar_count),
        })
        .await?;

    Ok(PreTradeBriefing {
        symbol,
        spot,
        recent_bars: bars.trendbars,
    })
}
