//! Month-partitioned flat binary chunk files: one file per `(symbol, series, calendar
//! month)`, holding fixed-size records back to back. Only the current month is ever
//! appended to; sealed months are immutable (see [`super::manifest::Manifest`] for the
//! "which months are sealed" bookkeeping) and are read as a single sequential file read.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use time::OffsetDateTime;
use wyck_openapi_model::market::{Bar, Period, Tick};

use super::record::{
    BAR_RECORD_LEN, TICK_RECORD_LEN, decode_bar, decode_tick, encode_bar, encode_tick,
};
use crate::error::{MarketDataError, Result};

/// The UTC calendar month a Unix-millisecond timestamp falls in, as `(year, month 1-12)`.
#[must_use]
pub fn month_of(time_ms: i64) -> (i32, u8) {
    let dt = OffsetDateTime::from_unix_timestamp(time_ms.div_euclid(1000))
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    (dt.year(), u8::from(dt.month()))
}

/// The Unix-millisecond timestamp of the first instant of the UTC calendar month
/// `(year, month)`.
#[must_use]
pub fn month_start_ms(year: i32, month: u8) -> i64 {
    let month = time::Month::try_from(month).unwrap_or(time::Month::January);
    let date = time::Date::from_calendar_date(year, month, 1).unwrap_or(time::Date::MIN);
    date.midnight().assume_utc().unix_timestamp() * 1000
}

/// The path of the bar chunk file for `symbol_id`/`period`'s `(year, month)`, rooted at
/// the catalog directory.
#[must_use]
pub fn bar_chunk_path(root: &Path, symbol_id: i64, period: Period, year: i32, month: u8) -> PathBuf {
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

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| MarketDataError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    Ok(())
}

/// Appends `bars` (already sorted and belonging to the same chunk) to `path`, creating
/// the file and its parent directories if needed.
pub fn append_bars(path: &Path, bars: &[Bar]) -> Result<()> {
    if bars.is_empty() {
        return Ok(());
    }
    create_parent(path)?;
    let mut buf = Vec::with_capacity(bars.len() * BAR_RECORD_LEN);
    for bar in bars {
        encode_bar(bar, &mut buf);
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| MarketDataError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(&buf)
        .and_then(|()| file.sync_data())
        .map_err(|source| MarketDataError::Write {
            path: path.to_path_buf(),
            source,
        })
}

/// Appends `ticks` (already sorted and belonging to the same chunk) to `path`, creating
/// the file and its parent directories if needed.
pub fn append_ticks(path: &Path, ticks: &[Tick]) -> Result<()> {
    if ticks.is_empty() {
        return Ok(());
    }
    create_parent(path)?;
    let mut buf = Vec::with_capacity(ticks.len() * TICK_RECORD_LEN);
    for tick in ticks {
        encode_tick(tick, &mut buf);
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| MarketDataError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(&buf)
        .and_then(|()| file.sync_data())
        .map_err(|source| MarketDataError::Write {
            path: path.to_path_buf(),
            source,
        })
}

/// Reads every bar in the chunk file at `path`, or an empty vec if it does not exist.
pub fn read_bars(path: &Path) -> Result<Vec<Bar>> {
    read_records(path, BAR_RECORD_LEN, decode_bar)
}

/// Reads every tick in the chunk file at `path`, or an empty vec if it does not exist.
pub fn read_ticks(path: &Path) -> Result<Vec<Tick>> {
    read_records(path, TICK_RECORD_LEN, decode_tick)
}

fn read_records<T>(
    path: &Path,
    record_len: usize,
    decode: impl Fn(&[u8]) -> T,
) -> Result<Vec<T>> {
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
        assert_eq!(
            read_ticks(&path).unwrap(),
            vec![first[0], second[0]]
        );
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
