//! Bridges the local historical catalog (`wyck-market-data`) into the chart's existing
//! [`History`] trait, backfilling any missing range from the broker on demand before
//! reading back from disk. Live charts, Replay and Backtesting all read history through
//! [`CatalogClient`], so they see one consistent, persistent store instead of each
//! re-fetching from the rate-limited broker endpoint every session.

use std::future::Future;
use std::sync::Arc;

use wyck_market_data::Catalog;
use wyck_market_data::backfill::{self, BackfillRange, RateLimiter, Upstream};
use wyck_openapi::market::{Bar, MarketClient, Period, QuoteType, Tick};
use wyck_openapi::{OpenApiError, Result};

use super::load::History;
use super::now_ms;

/// Adapts [`MarketClient`] to [`Upstream`]. Neither type is defined in this crate, so a
/// direct `impl Upstream for MarketClient` would violate the orphan rule; this thin local
/// wrapper is the standard way around that.
struct MarketUpstream<'a>(&'a MarketClient);

impl Upstream for MarketUpstream<'_> {
    type Error = OpenApiError;

    fn bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = std::result::Result<Vec<Bar>, Self::Error>> + Send {
        self.0.bars(symbol_id, period, from_ms, to_ms)
    }

    fn ticks(
        &self,
        symbol_id: i64,
        from_ms: i64,
        to_ms: i64,
    ) -> impl Future<Output = std::result::Result<Vec<Tick>, Self::Error>> + Send {
        self.0.ticks(symbol_id, QuoteType::Bid, from_ms, to_ms)
    }
}

fn store_err(err: wyck_market_data::MarketDataError) -> OpenApiError {
    OpenApiError::Protocol(err.to_string())
}

/// A [`History`] backed by the local catalog. `bars`/`ticks` here take the same
/// inclusive `[from_ms, to_ms]` range [`History`] already does; the catalog underneath
/// treats its own range as half-open, so every call to it below adjusts by one
/// millisecond at the edge.
pub struct CatalogClient {
    catalog: Arc<Catalog>,
    market: MarketClient,
    limiter: RateLimiter,
}

impl CatalogClient {
    /// A catalog client backfilling from `market`, rate-limited to what cTrader's
    /// historical endpoints allow. `catalog` is the app-wide shared catalog (see
    /// `crate::app::market_data::catalog`), cheap to clone into one client per chart.
    pub fn new(catalog: Arc<Catalog>, market: MarketClient) -> Self {
        Self {
            catalog,
            market,
            limiter: RateLimiter::for_ctrader_history(),
        }
    }
}

impl History for CatalogClient {
    async fn bars(&self, symbol_id: i64, period: Period, from_ms: i64, to_ms: i64) -> Result<Vec<Bar>> {
        backfill::backfill_bars(
            &self.catalog,
            &MarketUpstream(&self.market),
            &self.limiter,
            symbol_id,
            period,
            BackfillRange {
                from_ms,
                to_ms: to_ms + 1,
                now_ms: now_ms(),
            },
        )
        .await
        .map_err(store_err)?;
        self.catalog
            .load_bars(symbol_id, period, from_ms, to_ms + 1)
            .map_err(store_err)
    }

    async fn ticks(&self, symbol_id: i64, from_ms: i64, to_ms: i64) -> Result<Vec<Tick>> {
        backfill::backfill_ticks(
            &self.catalog,
            &MarketUpstream(&self.market),
            &self.limiter,
            symbol_id,
            BackfillRange {
                from_ms,
                to_ms: to_ms + 1,
                now_ms: now_ms(),
            },
        )
        .await
        .map_err(store_err)?;
        self.catalog
            .load_ticks(symbol_id, from_ms, to_ms + 1)
            .map_err(store_err)
    }
}
