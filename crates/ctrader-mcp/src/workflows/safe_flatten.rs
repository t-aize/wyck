//! **Safe flatten** (`SKILL.md` recipe 6): closes every open position and cancels
//! every pending order, optionally scoped to one symbol. Read-then-mutate, and never
//! silently swallows a per-item failure: every close/cancel attempt is accounted for
//! in the returned [`FlattenReport`].

use crate::error::CTraderError;
use crate::remote::RemoteClient;
use crate::remote::dto::ClosePositionParams;

/// The outcome of a [`safe_flatten`] call. `errors` is populated per-item (one entry per
/// failed close/cancel) rather than aborting the whole flatten on the first failure, so
/// a single stuck position doesn't prevent flattening everything else.
#[derive(Debug, Clone, Default)]
pub struct FlattenReport {
    pub closed_positions: Vec<i64>,
    pub cancelled_orders: Vec<i64>,
    pub errors: Vec<String>,
}

impl FlattenReport {
    /// `true` iff every close/cancel attempt succeeded.
    pub fn fully_flattened(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Closes every open position and cancels every pending order on the account,
/// optionally restricted to `symbol_id`. **Destructive: irreversible.** Callers should
/// present the affected positions/orders to the user for confirmation before invoking
/// this (per `references/local-http-server.md` "Destructive operations", which applies
/// equally to this Remote-side equivalent of `close_all_positions` +
/// `cancel_all_pending_orders`).
///
/// Reads the current book once via `get_positions` (which returns both positions and
/// orders on Remote), then issues one `close_position`/`cancel_order` call per item: it
/// does not re-read between items, so a position closed by an SL/TP hit concurrently
/// with this call surfaces as a per-item error in the report rather than aborting the
/// whole flatten.
pub async fn safe_flatten(
    client: &RemoteClient,
    symbol_id: Option<i64>,
) -> Result<FlattenReport, CTraderError> {
    let snapshot = client.get_positions().await?;
    let mut report = FlattenReport::default();

    for position in snapshot
        .positions
        .iter()
        .filter(|position| symbol_id.is_none_or(|id| position.symbol_id == Some(id)))
    {
        let (Some(position_id), Some(volume)) = (position.position_id, position.volume) else {
            continue;
        };
        match client
            .close_position(ClosePositionParams {
                position_id,
                volume,
            })
            .await
        {
            Ok(_) => report.closed_positions.push(position_id),
            Err(source) => report
                .errors
                .push(format!("failed to close position {position_id}: {source}")),
        }
    }

    for order in snapshot
        .orders
        .iter()
        .filter(|order| symbol_id.is_none_or(|id| order.symbol_id == Some(id)))
    {
        let Some(order_id) = order.order_id else {
            continue;
        };
        match client.cancel_order(order_id).await {
            Ok(_) => report.cancelled_orders.push(order_id),
            Err(source) => report
                .errors
                .push(format!("failed to cancel order {order_id}: {source}")),
        }
    }

    Ok(report)
}
