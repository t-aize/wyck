//! Month-partitioned flat binary chunk files: one file per `(symbol, series, calendar
//! month)`, holding fixed-size records back to back, sorted by time. Bars have one
//! record per time; ticks can share a millisecond.
//! Only the current (still-open) month is ever written to more than once; sealed months
//! are immutable (see [`super::manifest::Manifest`] for the "which months are sealed"
//! bookkeeping) and are read as a single sequential file read.
//!
//! Storing is a read-merge-rewrite, not a blind append: the backfill scheduler may
//! ask for the same still-open month more than once as new data arrives, and a live
//! write-behind cache would too. Rewriting the whole (small, one month of one instrument)
//! file keeps that safe without a separate "have I already stored this record" index.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime};
use wyck_openapi_model::market::{Bar, Period, Tick};

use super::record::{
    BAR_RECORD_LEN, TICK_RECORD_LEN, decode_bar, decode_tick, encode_bar, encode_tick,
};
use crate::error::{MarketDataError, Result};

/// The UTC calendar month a Unix-millisecond timestamp falls in, as `(year, month 1-12)`.
#[must_use]
pub fn month_of(time_ms: i64) -> (i32, u8) {
    let dt = DateTime::from_timestamp(time_ms.div_euclid(1000), 0).unwrap_or(DateTime::UNIX_EPOCH);
    (dt.year(), dt.month() as u8)
}

/// The Unix-millisecond timestamp of the first instant of the UTC calendar month
/// `(year, month)`.
#[must_use]
pub fn month_start_ms(year: i32, month: u8) -> i64 {
    let month = if (1..=12).contains(&month) { month } else { 1 };
    let date = NaiveDate::from_ymd_opt(year, u32::from(month), 1).unwrap_or(NaiveDate::MIN);
    date.and_time(NaiveTime::MIN).and_utc().timestamp() * 1000
}

/// The Unix-millisecond timestamp of the first instant of the UTC calendar month right
/// after `(year, month)`: the exclusive end of that month's range.
#[must_use]
pub fn next_month_start_ms(year: i32, month: u8) -> i64 {
    if month == 12 {
        month_start_ms(year + 1, 1)
    } else {
        month_start_ms(year, month + 1)
    }
}

/// Every calendar month whose range overlaps `[from_ms, to_ms)`, oldest first. Empty if
/// the range is empty or inverted.
#[must_use]
pub fn months_between(from_ms: i64, to_ms: i64) -> Vec<(i32, u8)> {
    if to_ms <= from_ms {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut year, mut month) = month_of(from_ms);
    while month_start_ms(year, month) < to_ms {
        out.push((year, month));
        (year, month) = if month == 12 {
            (year + 1, 1)
        } else {
            (year, month + 1)
        };
    }
    out
}

/// The path of the bar chunk file for `symbol_id`/`period`'s `(year, month)`, rooted at
/// the catalog directory.
#[must_use]
pub fn bar_chunk_path(
    root: &Path,
    symbol_id: i64,
    period: Period,
    year: i32,
    month: u8,
) -> PathBuf {
    root.join("bars")
        .join(symbol_id.to_string())
        .join(period.label())
        .join(format!("{year:04}-{month:02}.bin"))
}

/// The path of the tick chunk file for `symbol_id`'s `(year, month)`, rooted at the
/// catalog directory.
#[must_use]
pub fn tick_chunk_path(root: &Path, symbol_id: i64, year: i32, month: u8) -> PathBuf {
    root.join("ticks")
        .join(symbol_id.to_string())
        .join(format!("{year:04}-{month:02}.bin"))
}

/// Writes `bytes` to `path` atomically (see [`wyck_config::write_atomically`]), so a crash
/// mid-write never leaves a half-written chunk where a reader could see it.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    wyck_config::write_atomically(path, bytes).map_err(|source| MarketDataError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Merges `bars` into whatever is already stored at `path` (if anything), keeping the
/// newest record for any duplicate timestamp, and rewrites the file sorted by time. Safe
/// to call more than once with overlapping data, which the backfill scheduler and a live
/// write-behind cache both need for a still-open month.
pub fn append_bars(path: &Path, bars: &[Bar]) -> Result<()> {
    if bars.is_empty() {
        return Ok(());
    }
    // New records are placed first so that, after the stable sort below, they win over
    // an existing record at the same timestamp: `dedup_by_key` keeps the first of each
    // run of equal keys.
    let mut all = bars.to_vec();
    all.extend(read_bars(path)?);
    all.sort_by_key(|bar| bar.time_ms);
    all.dedup_by_key(|bar| bar.time_ms);
    let mut buf = Vec::with_capacity(all.len() * BAR_RECORD_LEN);
    for bar in &all {
        encode_bar(bar, &mut buf);
    }
    write_atomic(path, &buf)
}

/// Merges `ticks` into whatever is already stored at `path`, see [`append_bars`].
pub fn append_ticks(path: &Path, ticks: &[Tick]) -> Result<()> {
    if ticks.is_empty() {
        return Ok(());
    }
    let times: std::collections::HashSet<i64> = ticks.iter().map(|tick| tick.time_ms).collect();
    let mut all = read_ticks(path)?;
    all.retain(|tick| !times.contains(&tick.time_ms));
    all.extend_from_slice(ticks);
    all.sort_by_key(|tick| tick.time_ms);
    let mut buf = Vec::with_capacity(all.len() * TICK_RECORD_LEN);
    for tick in &all {
        encode_tick(tick, &mut buf);
    }
    write_atomic(path, &buf)
}

/// Reads every bar in the chunk file at `path`, or an empty vec if it does not exist.
pub fn read_bars(path: &Path) -> Result<Vec<Bar>> {
    read_records(path, BAR_RECORD_LEN, decode_bar)
}

/// Reads every tick in the chunk file at `path`, or an empty vec if it does not exist.
pub fn read_ticks(path: &Path) -> Result<Vec<Tick>> {
    read_records(path, TICK_RECORD_LEN, decode_tick)
}

fn read_records<T>(path: &Path, record_len: usize, decode: impl Fn(&[u8]) -> T) -> Result<Vec<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(MarketDataError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    if bytes.len() % record_len != 0 {
        return Err(MarketDataError::TruncatedChunk {
            path: path.to_path_buf(),
            len: bytes.len(),
            record_len,
        });
    }
    Ok(bytes.chunks_exact(record_len).map(decode).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_of_resolves_the_utc_calendar_month() {
        // 2024-03-15T12:00:00Z
        assert_eq!(month_of(1_710_504_000_000), (2024, 3));
    }

    #[test]
    fn month_start_ms_is_the_first_instant_of_the_month() {
        let start = month_start_ms(2024, 3);
        assert_eq!(month_of(start), (2024, 3));
        assert_eq!(month_of(start - 1), (2024, 2));
    }

    #[test]
    fn next_month_start_ms_rolls_over_the_year() {
        assert_eq!(next_month_start_ms(2024, 3), month_start_ms(2024, 4));
        assert_eq!(next_month_start_ms(2024, 12), month_start_ms(2025, 1));
    }

    #[test]
    fn months_between_lists_every_overlapping_month() {
        let from = month_start_ms(2024, 2) + 1;
        let to = month_start_ms(2024, 4) + 1;
        assert_eq!(
            months_between(from, to),
            vec![(2024, 2), (2024, 3), (2024, 4)]
        );
    }

    #[test]
    fn months_between_is_empty_for_an_inverted_or_empty_range() {
        assert_eq!(months_between(100, 100), Vec::new());
        assert_eq!(months_between(100, 0), Vec::new());
    }

    #[test]
    fn bars_round_trip_through_append_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = bar_chunk_path(dir.path(), 1, Period::M1, 2024, 3);
        let bars = vec![
            Bar {
                time_ms: 1_710_504_000_000,
                open: 100,
                high: 110,
                low: 90,
                close: 105,
                volume: 5,
            },
            Bar {
                time_ms: 1_710_504_060_000,
                open: 105,
                high: 108,
                low: 104,
                close: 107,
                volume: 3,
            },
        ];
        append_bars(&path, &bars).unwrap();
        assert_eq!(read_bars(&path).unwrap(), bars);
    }

    #[test]
    fn appending_twice_accumulates_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = tick_chunk_path(dir.path(), 1, 2024, 3);
        let first = vec![Tick {
            time_ms: 1,
            price: 100,
        }];
        let second = vec![Tick {
            time_ms: 2,
            price: 101,
        }];
        append_ticks(&path, &first).unwrap();
        append_ticks(&path, &second).unwrap();
        assert_eq!(read_ticks(&path).unwrap(), vec![first[0], second[0]]);
    }

    #[test]
    fn ticks_at_the_same_millisecond_survive_repeated_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = tick_chunk_path(dir.path(), 1, 2024, 3);
        let ticks = vec![
            Tick {
                time_ms: 10,
                price: 100,
            },
            Tick {
                time_ms: 10,
                price: 101,
            },
            Tick {
                time_ms: 10,
                price: 101,
            },
            Tick {
                time_ms: 11,
                price: 102,
            },
        ];
        append_ticks(&path, &ticks).unwrap();
        append_ticks(&path, &ticks).unwrap();
        assert_eq!(read_ticks(&path).unwrap(), ticks);
    }

    #[test]
    fn storing_the_same_timestamp_again_replaces_it_and_keeps_it_sorted() {
        let dir = tempfile::tempdir().unwrap();
        let path = bar_chunk_path(dir.path(), 1, Period::M1, 2024, 3);
        let original = Bar {
            time_ms: 10,
            open: 1,
            high: 1,
            low: 1,
            close: 1,
            volume: 1,
        };
        let earlier = Bar {
            time_ms: 5,
            ..original
        };
        let corrected = Bar {
            close: 999,
            ..original
        };
        append_bars(&path, &[original]).unwrap();
        append_bars(&path, &[earlier, corrected]).unwrap();
        assert_eq!(read_bars(&path).unwrap(), vec![earlier, corrected]);
    }

    #[test]
    fn reading_a_missing_chunk_is_an_empty_vec() {
        let dir = tempfile::tempdir().unwrap();
        let path = bar_chunk_path(dir.path(), 1, Period::H1, 2024, 3);
        assert_eq!(read_bars(&path).unwrap(), Vec::new());
    }

    #[test]
    fn a_truncated_chunk_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.bin");
        fs::write(&path, [0u8; 10]).unwrap();
        assert!(matches!(
            read_bars(&path),
            Err(MarketDataError::TruncatedChunk { .. })
        ));
    }
}
