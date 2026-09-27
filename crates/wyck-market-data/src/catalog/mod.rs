//! The local historical data catalog: a [`manifest::Manifest`] (SQLite) recording which
//! `(symbol, series, month)` chunks are sealed, and [`chunks`] flat binary files holding
//! the bars/ticks themselves. See the crate-level docs for why this split (and not
//! Arrow/Parquet) was chosen.

pub mod chunks;
pub mod manifest;
pub mod record;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use wyck_openapi_model::market::{Bar, Period, Tick};

use self::manifest::{Manifest, TICKS};
use crate::error::{MarketDataError, Result};

/// A handle to one catalog on disk: a manifest plus a tree of bar/tick chunk files
/// rooted at the same directory.
pub struct Catalog {
    root: PathBuf,
    manifest: Manifest,
    write_lock: Mutex<()>,
}

impl Catalog {
    /// Opens (creating if needed) the catalog rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|source| MarketDataError::CreateDir {
            path: root.clone(),
            source,
        })?;
        let manifest = Manifest::open(&root.join("catalog.sqlite3"))?;
        Ok(Self {
            root,
            manifest,
            write_lock: Mutex::new(()),
        })
    }

    /// The directory this catalog is rooted at.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Appends `bars` to the catalog for `symbol_id`/`period`. `bars` may span several
    /// calendar months; each month's records are appended to its own chunk file. Does
    /// not itself seal any month as complete: call [`Self::mark_bars_month_complete`]
    /// once a caller (typically the backfill scheduler) knows a month has no more gaps.
    pub fn store_bars(&self, symbol_id: i64, period: Period, bars: &[Bar]) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for (year, month, group) in group_by_month(bars, |bar| bar.time_ms) {
            let path = chunks::bar_chunk_path(&self.root, symbol_id, period, year, month);
            chunks::append_bars(&path, &group)?;
        }
        Ok(())
    }

    /// Appends `ticks` to the catalog for `symbol_id`, grouped by calendar month exactly
    /// as [`Self::store_bars`] does for bars.
    pub fn store_ticks(&self, symbol_id: i64, ticks: &[Tick]) -> Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for (year, month, group) in group_by_month(ticks, |tick| tick.time_ms) {
            let path = chunks::tick_chunk_path(&self.root, symbol_id, year, month);
            chunks::append_ticks(&path, &group)?;
        }
        Ok(())
    }

    /// Every locally-cached bar for `symbol_id`/`period` in `[from_ms, to_ms)`, sorted
    /// ascending by time. Does not fetch anything from the network: this only reads what
    /// is already on disk, missing months are simply absent from the result.
    pub fn load_bars(
        &self,
        symbol_id: i64,
        period: Period,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<Bar>> {
        let mut out = Vec::new();
        for (year, month) in chunks::months_between(from_ms, to_ms) {
            let path = chunks::bar_chunk_path(&self.root, symbol_id, period, year, month);
            out.extend(chunks::read_bars(&path)?);
        }
        out.retain(|bar| bar.time_ms >= from_ms && bar.time_ms < to_ms);
        out.sort_by_key(|bar| bar.time_ms);
        Ok(out)
    }

    /// Every locally-cached tick for `symbol_id` in `[from_ms, to_ms)`, sorted ascending
    /// by time.
    pub fn load_ticks(&self, symbol_id: i64, from_ms: i64, to_ms: i64) -> Result<Vec<Tick>> {
        let mut out = Vec::new();
        for (year, month) in chunks::months_between(from_ms, to_ms) {
            let path = chunks::tick_chunk_path(&self.root, symbol_id, year, month);
            out.extend(chunks::read_ticks(&path)?);
        }
        out.retain(|tick| tick.time_ms >= from_ms && tick.time_ms < to_ms);
        out.sort_by_key(|tick| tick.time_ms);
        Ok(out)
    }

    /// Marks the bar chunk for `symbol_id`/`period`'s month (identified by any
    /// timestamp within it) as sealed: no gap backfill will ever be attempted for it
    /// again.
    pub fn mark_bars_month_complete(
        &self,
        symbol_id: i64,
        period: Period,
        time_in_month_ms: i64,
    ) -> Result<()> {
        let (year, month) = chunks::month_of(time_in_month_ms);
        self.manifest.mark_month_complete(
            symbol_id,
            period.label(),
            chunks::month_start_ms(year, month),
        )
    }

    /// Whether the bar chunk for `symbol_id`/`period`'s month is sealed.
    pub fn bars_month_complete(
        &self,
        symbol_id: i64,
        period: Period,
        time_in_month_ms: i64,
    ) -> Result<bool> {
        let (year, month) = chunks::month_of(time_in_month_ms);
        self.manifest.is_month_complete(
            symbol_id,
            period.label(),
            chunks::month_start_ms(year, month),
        )
    }

    /// Marks the tick chunk for `symbol_id`'s month as sealed, see
    /// [`Self::mark_bars_month_complete`].
    pub fn mark_ticks_month_complete(&self, symbol_id: i64, time_in_month_ms: i64) -> Result<()> {
        let (year, month) = chunks::month_of(time_in_month_ms);
        self.manifest
            .mark_month_complete(symbol_id, TICKS, chunks::month_start_ms(year, month))
    }

    /// Whether the tick chunk for `symbol_id`'s month is sealed.
    pub fn ticks_month_complete(&self, symbol_id: i64, time_in_month_ms: i64) -> Result<bool> {
        let (year, month) = chunks::month_of(time_in_month_ms);
        self.manifest
            .is_month_complete(symbol_id, TICKS, chunks::month_start_ms(year, month))
    }

    /// A reference to the underlying manifest, for callers (the backfill scheduler) that
    /// need the lower-level coverage/probe bookkeeping directly.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
}

/// Splits `items` into contiguous per-calendar-month groups, in the order given (callers
/// are expected to pass time-ordered data; this does not sort).
fn group_by_month<T: Copy>(items: &[T], time_of: impl Fn(T) -> i64) -> Vec<(i32, u8, Vec<T>)> {
    let mut groups: Vec<(i32, u8, Vec<T>)> = Vec::new();
    for &item in items {
        let (year, month) = chunks::month_of(time_of(item));
        match groups.last_mut() {
            Some((y, m, group)) if *y == year && *m == month => group.push(item),
            _ => groups.push((year, month, vec![item])),
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(time_ms: i64) -> Bar {
        Bar {
            time_ms,
            open: 1,
            high: 2,
            low: 0,
            close: 1,
            volume: 1,
        }
    }

    #[test]
    fn bars_stored_and_loaded_round_trip_across_a_month_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let feb = chunks::month_start_ms(2024, 2) + 60_000;
        let mar = chunks::month_start_ms(2024, 3) + 60_000;
        let bars = vec![bar(feb), bar(mar)];
        catalog.store_bars(7, Period::M1, &bars).unwrap();

        let loaded = catalog.load_bars(7, Period::M1, feb - 1, mar + 1).unwrap();
        assert_eq!(loaded, bars);
    }

    #[test]
    fn loading_an_uncached_range_is_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        assert_eq!(
            catalog
                .load_bars(1, Period::D1, 0, 1_000_000_000_000)
                .unwrap(),
            Vec::new()
        );
    }

    #[test]
    fn a_month_is_incomplete_until_marked() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let now = chunks::month_start_ms(2024, 5) + 1;
        assert!(!catalog.bars_month_complete(1, Period::H1, now).unwrap());
        catalog
            .mark_bars_month_complete(1, Period::H1, now)
            .unwrap();
        assert!(catalog.bars_month_complete(1, Period::H1, now).unwrap());
    }

    #[test]
    fn ticks_round_trip_and_filter_to_the_requested_range() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(dir.path()).unwrap();
        let base = chunks::month_start_ms(2024, 6) + 1_000;
        let ticks = vec![
            Tick {
                time_ms: base,
                price: 100,
            },
            Tick {
                time_ms: base + 1,
                price: 101,
            },
            Tick {
                time_ms: base + 2,
                price: 102,
            },
        ];
        catalog.store_ticks(9, &ticks).unwrap();
        let loaded = catalog.load_ticks(9, base, base + 2).unwrap();
        assert_eq!(loaded, vec![ticks[0], ticks[1]]);
    }
}
