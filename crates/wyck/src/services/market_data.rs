//! The local historical market data catalog (`wyck-market-data`), opened once at startup
//! and shared as a GPUI global so every chart reads and backfills history through the
//! same [`CatalogClient`](crate::chart::catalog_client::CatalogClient) instead of
//! each re-fetching from the broker every session.

use std::sync::Arc;

use gpui::{App, Global};
use wyck_config::AppPaths;
use wyck_market_data::Catalog;

struct Service {
    catalog: Arc<Catalog>,
}

impl Global for Service {}

/// Opens the catalog under `paths.market_data_dir()` and makes it available via
/// [`catalog`]. Does nothing (charts simply fall back to fetching from the broker every
/// time, as they did before this feature existed) if `paths` is `None` or the catalog
/// cannot be opened, which is not fatal to the rest of the app.
pub fn init(paths: Option<&AppPaths>, cx: &mut App) {
    let Some(paths) = paths else { return };
    match Catalog::open(paths.market_data_dir()) {
        Ok(catalog) => cx.set_global(Service {
            catalog: Arc::new(catalog),
        }),
        Err(error) => {
            tracing::warn!(%error, "could not open the local market data catalog");
        }
    }
}

/// The shared catalog, if it was opened successfully at startup.
pub fn catalog(cx: &App) -> Option<Arc<Catalog>> {
    cx.try_global::<Service>()
        .map(|service| service.catalog.clone())
}
