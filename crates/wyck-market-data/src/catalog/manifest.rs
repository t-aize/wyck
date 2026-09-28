//! The catalog's SQLite manifest: which `(symbol, series, month)` chunks are sealed and
//! complete, and the resumable state of in-flight backfill jobs. Small and relational, so
//! this is the one place the catalog needs atomic commits; the bulk numeric data lives in
//! the flat chunk files from [`super::chunks`], never in SQLite.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};

use crate::error::{MarketDataError, Result};

/// A series identifier within the manifest: either a bar period's label (e.g. `"M1"`) or
/// the literal `"ticks"`. Kept as a plain string key rather than an enum so the manifest
/// schema does not need to change if `wyck_openapi_model::market::Period` grows variants.
pub type Series<'a> = &'a str;

/// The series key for ticks, as stored in the manifest.
pub const TICKS: Series<'static> = "ticks";

/// The SQLite-backed manifest of catalog coverage and backfill job state.
///
/// A `rusqlite::Connection` is `Send` but not `Sync` (its interior mutability is not safe
/// to touch from two threads at once), while a [`super::Catalog`] is shared behind a
/// plain `&self` across concurrent async tasks (multiple charts backfilling at once).
/// The `Mutex` here is that synchronization, not a performance optimization: catalog
/// access is not a hot path.
pub struct Manifest {
    conn: Mutex<Connection>,
}

impl Manifest {
    /// Opens (creating if needed) the manifest database at `path`, and applies the
    /// schema if it is not already present.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|source| MarketDataError::Manifest {
            path: path.to_path_buf(),
            source,
        })?;
        let manifest = Self {
            conn: Mutex::new(conn),
        };
        manifest.migrate(path)?;
        Ok(manifest)
    }

    /// Opens an in-memory manifest, for tests that don't need the database to survive
    /// the process.
    #[must_use]
    pub fn open_in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory sqlite connection");
        let manifest = Self {
            conn: Mutex::new(conn),
        };
        manifest
            .migrate(Path::new(":memory:"))
            .expect("in-memory schema migration");
        manifest
    }

    fn migrate(&self, path: &Path) -> Result<()> {
        self.conn()
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS coverage (
                    symbol_id INTEGER NOT NULL,
                    series TEXT NOT NULL,
                    month_start_ms INTEGER NOT NULL,
                    complete INTEGER NOT NULL DEFAULT 0,
                    request_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (symbol_id, series, month_start_ms)
                );
                CREATE TABLE IF NOT EXISTS backfill_jobs (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    symbol_id INTEGER NOT NULL,
                    series TEXT NOT NULL,
                    from_ms INTEGER NOT NULL,
                    to_ms INTEGER NOT NULL,
                    cursor_ms INTEGER NOT NULL,
                    status TEXT NOT NULL
                );
                ",
            )
            .map_err(|source| MarketDataError::Manifest {
                path: path.to_path_buf(),
                source,
            })
    }

    /// Locks the connection, recovering it if a previous holder panicked while holding
    /// the lock rather than poisoning every later access: a manifest query is never
    /// half-applied (SQLite statements are atomic), so there is nothing to roll back.
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Marks the chunk for `symbol_id`/`series`/`month_start_ms` as sealed and complete.
    /// Called only after the chunk file itself has been written and flushed
    /// successfully, so a crash between the two never leaves a month marked complete
    /// with data missing on disk.
    pub fn mark_month_complete(
        &self,
        symbol_id: i64,
        series: Series<'_>,
        month_start_ms: i64,
    ) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO coverage (symbol_id, series, month_start_ms, complete)
                 VALUES (?1, ?2, ?3, 1)
                 ON CONFLICT (symbol_id, series, month_start_ms)
                 DO UPDATE SET complete = 1",
                params![symbol_id, series, month_start_ms],
            )
            .map(|_| ())
            .map_err(MarketDataError::ManifestQuery)
    }

    /// Whether the chunk for `symbol_id`/`series`/`month_start_ms` is sealed and
    /// complete.
    pub fn is_month_complete(
        &self,
        symbol_id: i64,
        series: Series<'_>,
        month_start_ms: i64,
    ) -> Result<bool> {
        self.conn()
            .query_row(
                "SELECT complete FROM coverage
                 WHERE symbol_id = ?1 AND series = ?2 AND month_start_ms = ?3",
                params![symbol_id, series, month_start_ms],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|value| value == Some(1))
            .map_err(MarketDataError::ManifestQuery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_month_starts_incomplete() {
        let manifest = Manifest::open_in_memory();
        assert!(!manifest.is_month_complete(1, "M1", 0).unwrap());
    }

    #[test]
    fn marking_a_month_complete_persists() {
        let manifest = Manifest::open_in_memory();
        manifest
            .mark_month_complete(1, "M1", 1_700_000_000_000)
            .unwrap();
        assert!(
            manifest
                .is_month_complete(1, "M1", 1_700_000_000_000)
                .unwrap()
        );
        // A different symbol/series/month is unaffected.
        assert!(
            !manifest
                .is_month_complete(2, "M1", 1_700_000_000_000)
                .unwrap()
        );
        assert!(
            !manifest
                .is_month_complete(1, TICKS, 1_700_000_000_000)
                .unwrap()
        );
    }

    #[test]
    fn marking_complete_twice_is_idempotent() {
        let manifest = Manifest::open_in_memory();
        manifest.mark_month_complete(1, "M1", 0).unwrap();
        manifest.mark_month_complete(1, "M1", 0).unwrap();
        assert!(manifest.is_month_complete(1, "M1", 0).unwrap());
    }
}
