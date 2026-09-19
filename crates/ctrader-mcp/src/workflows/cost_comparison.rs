//! **Cost-of-trading comparison** (`SKILL.md` recipe 3): ranks a set of symbols by
//! current spread, for "which is cheaper to trade, A or B" style requests.

use crate::error::CTraderError;
use crate::remote::RemoteClient;
use crate::workflows::bootstrap::RemoteSessionContext;

/// One symbol's cost snapshot.
#[derive(Debug, Clone)]
pub struct SymbolCost {
    pub symbol_name: String,
    pub symbol_id: i64,
    /// Raw pipette spread (`ask - bid`); `None` if a quote wasn't returned for this
    /// symbol (see `Q-R8`: a single unknown id can empty the whole batch response, so
    /// a missing entry here may mean a DIFFERENT symbol in the batch was invalid, not
    /// necessarily this one).
    pub spread_pipettes: Option<i64>,
    /// `spread_pipettes` decoded to a display value using the symbol's cached
    /// `pip_digits`.
    pub spread_display: Option<f64>,
}

/// Fetches one batched quote for every symbol in `symbol_names` and ranks them
/// cheapest-spread-first.
///
/// # Errors
///
/// [`CTraderError::Invariant`] if any name in `symbol_names` isn't in `session`'s cached
/// symbol map (populated by [`crate::workflows::bootstrap_remote`]).
pub async fn compare_trading_costs(
    client: &RemoteClient,
    session: &RemoteSessionContext,
    symbol_names: &[&str],
) -> Result<Vec<SymbolCost>, CTraderError> {
    let mut resolved = Vec::with_capacity(symbol_names.len());
    for name in symbol_names {
        let symbol = session.find_symbol(name).ok_or_else(|| {
            CTraderError::Invariant(format!(
                "symbol `{name}` not found in the cached get_symbols map"
            ))
        })?;
        resolved.push(symbol.clone());
    }

    let symbol_ids = resolved.iter().map(|s| s.symbol_id).collect();
    let prices = client.get_spot_prices(symbol_ids).await?;

    let mut costs: Vec<SymbolCost> = resolved
        .iter()
        .map(|symbol| {
            let quote = prices
                .prices
                .iter()
                .find(|p| p.symbol_id == Some(symbol.symbol_id));
            let spread_pipettes = quote.and_then(|q| Some(q.ask? - q.bid?));
            let spread_display = spread_pipettes
                .zip(symbol.pip_digits)
                .map(|(spread, digits)| crate::common::price_from_pipettes(spread, digits));
            SymbolCost {
                symbol_name: symbol.symbol_name.clone(),
                symbol_id: symbol.symbol_id,
                spread_pipettes,
                spread_display,
            }
        })
        .collect();

    costs.sort_by(|a, b| {
        a.spread_display
            .partial_cmp(&b.spread_display)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(costs)
}
