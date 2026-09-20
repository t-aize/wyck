//! The bars on disk.
//!
//! Every bar downloaded is kept, so the second launch draws the chart straight away and only
//! asks the server for what is new. [`CandleStore`] is a single [redb] file: an embedded,
//! transactional database in pure Rust. A write is one transaction, so a crash or a killed
//! process leaves either the old state or the new one, never half a series.
//!
//! # Layout
//!
//! Two tables, both keyed by a text made of a **namespace**, the symbol and the period (see
//! [`SeriesKey`]):
//!
//! - `bars_v2`: key `(series, open time)`, value the five numbers of the bar (open, high, low,
//!   close, volume) as 40 bytes. The time is part of the key, so the bars of one series sit next
//!   to each other in time order and a range read is one scan.
//! - `coverage_v2`: key `series`, value the list of fetched ranges (see [`Coverage`]).
//!
//! The namespace separates data that must not be mixed: Remote and Local quote in different
//! units (Local's volume is a float, Remote's an integer) and may differ in history.
//!
//! # What is never stored
//!
//! The bar still forming changes until it closes. [`CandleStore::save`] refuses to write bars at
//! or after the end of the range it is told is complete, and that end is chosen by the caller to
//! stop at the forming bar. A cached series therefore only ever holds closed bars.
//!
//! # Failure
//!
//! The cache is a convenience, never the source of truth. Every error is reported to the caller,
//! which logs it and carries on with what the server gave; a file that cannot be opened or read
//! is deleted and started again by [`CandleStore::open_or_reset`].

use std::fmt;
use std::path::Path;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use wyck_engine::domain::{Candle, Period, UnixMillis};

use super::coverage::Coverage;

/// `(series, open time) -> open, high, low, close, volume`.
const BARS: TableDefinition<(&str, i64), [u8; 40]> = TableDefinition::new("bars_v2");

/// The tables of the first layout. Bars fetched then were cut to 100 per request by a Remote quirk
/// while their ranges were recorded as complete, so they hold holes that coverage hides: they are
/// dropped when the file is opened.
const LEGACY_BARS: TableDefinition<(&str, i64), [u8; 40]> = TableDefinition::new("bars");
const LEGACY_COVERAGE: TableDefinition<&str, &[u8]> = TableDefinition::new("coverage");

/// `series -> ranges fetched`.
const COVERAGE: TableDefinition<&str, &[u8]> = TableDefinition::new("coverage_v2");

/// What went wrong reading or writing the cache. The text is for the log.
#[derive(Debug)]
pub struct StoreError(String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bar cache: {}", self.0)
    }
}

impl std::error::Error for StoreError {}

fn err(error: impl fmt::Display) -> StoreError {
    StoreError(error.to_string())
}

/// Which series a bar belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeriesKey {
    /// Which server family the bars came from (`remote` or `local`).
    pub namespace: String,
    /// The symbol's ticker, as the account names it.
    pub symbol: String,
    /// The period of the bars.
    pub period: Period,
}

impl SeriesKey {
    /// A key for `symbol` at `period` on `namespace`. The symbol is stored upper case.
    #[must_use]
    pub fn new(namespace: &str, symbol: &str, period: Period) -> Self {
        Self {
            namespace: namespace.to_owned(),
            symbol: symbol.to_ascii_uppercase(),
            period,
        }
    }

    /// The text the tables are keyed by.
    #[must_use]
    pub fn text(&self) -> String {
        format!(
            "{}/{}/{}",
            self.namespace,
            self.symbol,
            self.period.as_wire_str()
        )
    }
}

fn encode(bar: &Candle) -> [u8; 40] {
    let mut out = [0u8; 40];
    let numbers = [bar.open, bar.high, bar.low, bar.close, bar.volume];
    for (slot, value) in out.as_chunks_mut::<8>().0.iter_mut().zip(numbers) {
        *slot = value.to_le_bytes();
    }
    out
}

fn decode(time: UnixMillis, raw: [u8; 40]) -> Option<Candle> {
    let [open, high, low, close, volume] = *raw.as_chunks::<8>().0 else {
        return None;
    };
    let bar = Candle {
        time,
        open: f64::from_le_bytes(open),
        high: f64::from_le_bytes(high),
        low: f64::from_le_bytes(low),
        close: f64::from_le_bytes(close),
        volume: f64::from_le_bytes(volume),
    };
    // A record that decodes to nonsense is treated as absent, not as a chart.
    bar.is_sane().then_some(bar)
}

/// The on-disk bar cache. See the [module docs](self).
pub struct CandleStore {
    db: Database,
}

impl fmt::Debug for CandleStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CandleStore").finish_non_exhaustive()
    }
}

impl CandleStore {
    /// Opens the cache at `path`, creating it when missing.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the file cannot be created or is not a valid database.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(err)?;
        }
        let db = Database::create(path).map_err(err)?;
        // Make sure both tables exist so a first read finds them.
        let txn = db.begin_write().map_err(err)?;
        // A missing table is not an error: these only exist in files from the first layout.
        let _ = txn.delete_table(LEGACY_BARS);
        let _ = txn.delete_table(LEGACY_COVERAGE);
        txn.open_table(BARS).map_err(err)?;
        txn.open_table(COVERAGE).map_err(err)?;
        txn.commit().map_err(err)?;
        Ok(Self { db })
    }

    /// Opens the cache, and when that fails deletes the file and starts a fresh one. A cache that
    /// cannot be read has nothing worth keeping: every bar in it can be fetched again.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when even a fresh file cannot be created.
    pub fn open_or_reset(path: &Path) -> Result<Self, StoreError> {
        Self::open(path).or_else(|first| {
            tracing::warn!(error = %first, "the bar cache is unreadable, starting a new one");
            let _ = std::fs::remove_file(path);
            Self::open(path)
        })
    }

    /// The bars of `key` opening in `[from, to)`, oldest first.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the read fails.
    pub fn load(
        &self,
        key: &SeriesKey,
        from: UnixMillis,
        to: UnixMillis,
    ) -> Result<Vec<Candle>, StoreError> {
        if to <= from {
            return Ok(Vec::new());
        }
        let text = key.text();
        let txn = self.db.begin_read().map_err(err)?;
        let table = txn.open_table(BARS).map_err(err)?;
        let mut bars = Vec::new();
        for row in table
            .range((text.as_str(), from)..(text.as_str(), to))
            .map_err(err)?
        {
            let (k, v) = row.map_err(err)?;
            if let Some(bar) = decode(k.value().1, v.value()) {
                bars.push(bar);
            }
        }
        Ok(bars)
    }

    /// The newest `limit` bars of `key` opening before `before`, oldest first. This is how a chart
    /// starts from cache: the last stretch of history, without knowing its start.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the read fails.
    pub fn load_latest(
        &self,
        key: &SeriesKey,
        before: UnixMillis,
        limit: usize,
    ) -> Result<Vec<Candle>, StoreError> {
        let text = key.text();
        let txn = self.db.begin_read().map_err(err)?;
        let table = txn.open_table(BARS).map_err(err)?;
        let mut bars = Vec::new();
        for row in table
            .range((text.as_str(), i64::MIN)..(text.as_str(), before))
            .map_err(err)?
            .rev()
            .take(limit)
        {
            let (k, v) = row.map_err(err)?;
            if let Some(bar) = decode(k.value().1, v.value()) {
                bars.push(bar);
            }
        }
        bars.reverse();
        Ok(bars)
    }

    /// The ranges of `key` already fetched.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the read fails. A stored list that is damaged reads as empty
    /// coverage, which only costs a refetch.
    pub fn coverage(&self, key: &SeriesKey) -> Result<Coverage, StoreError> {
        let text = key.text();
        let txn = self.db.begin_read().map_err(err)?;
        let table = txn.open_table(COVERAGE).map_err(err)?;
        Ok(table
            .get(text.as_str())
            .map_err(err)?
            .and_then(|guard| Coverage::from_bytes(guard.value()))
            .unwrap_or_default())
    }

    /// Stores `bars` and records `[from, to)` as fetched, in one transaction. Bars outside the
    /// range are ignored (see "What is never stored" in the module docs).
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the write fails; nothing is changed then.
    pub fn save(
        &self,
        key: &SeriesKey,
        bars: &[Candle],
        from: UnixMillis,
        to: UnixMillis,
    ) -> Result<(), StoreError> {
        if to <= from {
            return Ok(());
        }
        let text = key.text();
        let txn = self.db.begin_write().map_err(err)?;
        {
            let mut table = txn.open_table(BARS).map_err(err)?;
            for bar in bars
                .iter()
                .filter(|b| b.time >= from && b.time < to && b.is_sane())
            {
                table
                    .insert((text.as_str(), bar.time), encode(bar))
                    .map_err(err)?;
            }
            let mut coverage_table = txn.open_table(COVERAGE).map_err(err)?;
            let mut coverage = coverage_table
                .get(text.as_str())
                .map_err(err)?
                .and_then(|guard| Coverage::from_bytes(guard.value()))
                .unwrap_or_default();
            coverage.add(from, to);
            coverage_table
                .insert(text.as_str(), coverage.to_bytes().as_slice())
                .map_err(err)?;
        }
        txn.commit().map_err(err)
    }

    /// Forgets everything stored for `key`.
    ///
    /// # Errors
    ///
    /// A [`StoreError`] when the write fails.
    pub fn clear(&self, key: &SeriesKey) -> Result<(), StoreError> {
        let text = key.text();
        let txn = self.db.begin_write().map_err(err)?;
        {
            let mut table = txn.open_table(BARS).map_err(err)?;
            table
                .retain_in(
                    (text.as_str(), i64::MIN)..=(text.as_str(), i64::MAX),
                    |_, _| false,
                )
                .map_err(err)?;
            let mut coverage = txn.open_table(COVERAGE).map_err(err)?;
            coverage.remove(text.as_str()).map_err(err)?;
        }
        txn.commit().map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, CandleStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = CandleStore::open(&dir.path().join("candles.redb")).unwrap();
        (dir, store)
    }

    fn key() -> SeriesKey {
        SeriesKey::new("remote", "eurusd", Period::H1)
    }

    fn bar(time: i64, price: f64) -> Candle {
        Candle {
            time,
            open: price,
            high: price + 1.0,
            low: price - 1.0,
            close: price + 0.5,
            volume: 7.0,
        }
    }

    #[test]
    fn a_saved_series_reads_back_in_order() {
        let (_dir, store) = store();
        store
            .save(&key(), &[bar(30, 3.0), bar(10, 1.0), bar(20, 2.0)], 0, 40)
            .unwrap();
        let bars = store.load(&key(), 0, 40).unwrap();
        assert_eq!(
            bars.iter().map(|b| b.time).collect::<Vec<_>>(),
            [10, 20, 30]
        );
        assert_eq!(bars[0], bar(10, 1.0), "every number survives");
        assert_eq!(
            store.load(&key(), 15, 30).unwrap().len(),
            1,
            "half open range"
        );
    }

    #[test]
    fn the_latest_bars_are_read_newest_first_then_returned_oldest_first() {
        let (_dir, store) = store();
        let all: Vec<Candle> = (0..10).map(|i| bar(i * 10, i as f64)).collect();
        store.save(&key(), &all, 0, 100).unwrap();
        let recent = store.load_latest(&key(), 100, 3).unwrap();
        assert_eq!(
            recent.iter().map(|b| b.time).collect::<Vec<_>>(),
            [70, 80, 90]
        );
        let before = store.load_latest(&key(), 50, 2).unwrap();
        assert_eq!(before.iter().map(|b| b.time).collect::<Vec<_>>(), [30, 40]);
    }

    #[test]
    fn bars_outside_the_range_are_not_stored() {
        let (_dir, store) = store();
        // The forming bar, at 40, is beyond the range the caller says is complete.
        store
            .save(&key(), &[bar(10, 1.0), bar(40, 4.0)], 0, 40)
            .unwrap();
        assert_eq!(store.load(&key(), 0, 1000).unwrap().len(), 1);
    }

    #[test]
    fn coverage_accumulates_and_survives_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("candles.redb");
        {
            let store = CandleStore::open(&path).unwrap();
            store.save(&key(), &[], 0, 10).unwrap();
            store.save(&key(), &[], 10, 20).unwrap();
            store.save(&key(), &[], 30, 40).unwrap();
        }
        let store = CandleStore::open(&path).unwrap();
        assert_eq!(
            store.coverage(&key()).unwrap().spans(),
            &[(0, 20), (30, 40)]
        );
    }

    #[test]
    fn series_do_not_mix() {
        let (_dir, store) = store();
        let other_period = SeriesKey::new("remote", "EURUSD", Period::M5);
        let other_server = SeriesKey::new("local", "EURUSD", Period::H1);
        store.save(&key(), &[bar(10, 1.0)], 0, 20).unwrap();
        assert!(store.load(&other_period, 0, 20).unwrap().is_empty());
        assert!(store.load(&other_server, 0, 20).unwrap().is_empty());
        assert!(store.coverage(&other_period).unwrap().is_empty());
        assert_eq!(
            store.load(&key(), 0, 20).unwrap().len(),
            1,
            "case of the symbol is folded"
        );
    }

    #[test]
    fn clearing_removes_bars_and_coverage_of_one_series_only() {
        let (_dir, store) = store();
        let other = SeriesKey::new("remote", "GBPUSD", Period::H1);
        store.save(&key(), &[bar(10, 1.0)], 0, 20).unwrap();
        store.save(&other, &[bar(10, 1.0)], 0, 20).unwrap();
        store.clear(&key()).unwrap();
        assert!(store.load(&key(), 0, 20).unwrap().is_empty());
        assert!(store.coverage(&key()).unwrap().is_empty());
        assert_eq!(store.load(&other, 0, 20).unwrap().len(), 1);
    }

    #[test]
    fn tables_of_the_first_layout_are_dropped_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("candles.redb");
        {
            let db = Database::create(&path).unwrap();
            let txn = db.begin_write().unwrap();
            {
                let mut old = txn.open_table(LEGACY_BARS).unwrap();
                old.insert(("remote/EURUSD/H_1", 10), encode(&bar(10, 1.0)))
                    .unwrap();
                let mut cov = txn.open_table(LEGACY_COVERAGE).unwrap();
                cov.insert("remote/EURUSD/H_1", Coverage::new().to_bytes().as_slice())
                    .unwrap();
            }
            txn.commit().unwrap();
        }
        let store = CandleStore::open(&path).unwrap();
        assert!(store.load(&key(), 0, 100).unwrap().is_empty());
        drop(store);
        let db = Database::create(&path).unwrap();
        let txn = db.begin_read().unwrap();
        assert!(
            txn.open_table(LEGACY_BARS).is_err(),
            "the old table is gone"
        );
    }

    #[test]
    fn a_garbage_file_is_replaced_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("candles.redb");
        std::fs::write(&path, b"this is not a database at all, not even close").unwrap();
        assert!(CandleStore::open(&path).is_err());
        let store = CandleStore::open_or_reset(&path).unwrap();
        store.save(&key(), &[bar(10, 1.0)], 0, 20).unwrap();
        assert_eq!(store.load(&key(), 0, 20).unwrap().len(), 1);
    }
}
