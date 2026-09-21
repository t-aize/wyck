//! **Multi-window historical backfill** (`SKILL.md` recipe 5 / `references/trader-
//! workflows.md` W6): pulls more trendbars than a single `get_trendbars` call can
//! return, applying **P-REMOTE-HISTORY-CHUNK** (`Q-R7`'s 720-hour window cap) and
//! pagination, deduped by bar-open timestamp. `hasMore` is not trusted: checked live (2026-09), the
//! server cuts a window to its newest 100 bars and still answers `hasMore: false`, so a full page is
//! followed by another request further back (see [`crate::quirks::REMOTE_TRENDBARS_PAGE_CAP`]).

use std::collections::HashSet;

use crate::common::Period;
use crate::error::CTraderError;
use crate::quirks::{remote_history_windows, remote_trendbars_may_continue};
use crate::remote::RemoteClient;
use crate::remote::dto::{GetTrendbarsParams, RemoteTrendbar};
use crate::time::RemoteTimestamp;

/// Backfills every trendbar for `symbol_id`/`period` across `[from_epoch_ms,
/// to_epoch_ms)`, transparently splitting the request into <=720-hour windows (`Q-R7`)
/// and following `has_more` within each window, deduped by bar-open timestamp.
///
/// Returned bars are sorted ascending by timestamp (oldest first). Windows are issued
/// sequentially here, not in parallel, to stay conservative against Remote's 5 req/s
/// historical-endpoint rate limit: a caller backfilling a very wide span and willing to
/// manage its own concurrency budget can instead call
/// [`crate::quirks::remote_history_windows`] directly and fan the resulting windows out
/// itself (the 1.0.18 rejection hint confirms per-window calls may run in parallel).
///
/// # Errors
///
/// Propagates any [`CTraderError`] from the underlying `get_trendbars` calls.
pub async fn backfill_trendbars(
    client: &RemoteClient,
    symbol_id: i64,
    period: Period,
    from_epoch_ms: i64,
    to_epoch_ms: i64,
) -> Result<Vec<RemoteTrendbar>, CTraderError> {
    let mut all_bars = Vec::new();
    let mut seen_timestamps: HashSet<i64> = HashSet::new();

    for (window_from, window_to) in remote_history_windows(from_epoch_ms, to_epoch_ms) {
        // A window with more bars than a page comes back as its newest bars (see
        // `REMOTE_TRENDBARS_PAGE_CAP`), so the rest is fetched by moving the upper bound back to
        // just before the oldest bar received, until a page is not full.
        let mut upper = window_to;
        loop {
            let response = client
                .get_trendbars(GetTrendbarsParams {
                    symbol_id,
                    period,
                    from_timestamp: Some(RemoteTimestamp::epoch_millis(window_from)),
                    to_timestamp: Some(RemoteTimestamp::epoch_millis(upper)),
                    count: None,
                })
                .await?;

            let page_len = response.trendbars.len();
            let mut oldest_new_timestamp = None;
            for bar in response.trendbars {
                if let Some(timestamp) = bar.timestamp
                    && seen_timestamps.insert(timestamp)
                {
                    oldest_new_timestamp =
                        Some(oldest_new_timestamp.map_or(timestamp, |t: i64| t.min(timestamp)));
                    all_bars.push(bar);
                }
            }

            if !remote_trendbars_may_continue(response.has_more, page_len) {
                break;
            }
            // A full page with no new timestamp cannot prove that the range is complete.
            let Some(oldest) = oldest_new_timestamp else {
                return Err(CTraderError::Invariant(format!(
                    "trendbar history for symbol {symbol_id} is incomplete in \
                     [{window_from}, {upper}]: a full page made no progress"
                )));
            };
            if oldest <= window_from {
                break;
            }
            upper = oldest - 1;
        }
    }

    all_bars.sort_by_key(|bar| bar.timestamp);
    Ok(all_bars)
}
