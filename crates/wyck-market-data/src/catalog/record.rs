//! Fixed-size on-disk record layouts for bars and ticks, byte for byte the same fields as
//! [`wyck_openapi_model::market::Bar`] and [`wyck_openapi_model::market::Tick`] so encoding is a
//! plain reinterpretation, not a transformation.

use wyck_openapi_model::market::{Bar, Tick};

/// The size in bytes of one encoded [`Bar`]: six `i64` fields, little-endian.
pub const BAR_RECORD_LEN: usize = 48;

/// The size in bytes of one encoded [`Tick`]: two `i64` fields, little-endian.
pub const TICK_RECORD_LEN: usize = 16;

/// Appends `bar`'s encoded bytes to `out`.
pub fn encode_bar(bar: &Bar, out: &mut Vec<u8>) {
    out.extend_from_slice(&bar.time_ms.to_le_bytes());
    out.extend_from_slice(&bar.open.to_le_bytes());
    out.extend_from_slice(&bar.high.to_le_bytes());
    out.extend_from_slice(&bar.low.to_le_bytes());
    out.extend_from_slice(&bar.close.to_le_bytes());
    out.extend_from_slice(&bar.volume.to_le_bytes());
}

/// Decodes one [`Bar`] from exactly [`BAR_RECORD_LEN`] bytes.
///
/// # Panics
///
/// Panics if `bytes.len() != BAR_RECORD_LEN`. Callers only ever hand this a chunk already
/// validated to be a whole multiple of the record length (see
/// [`super::chunks::read_bars`]).
pub fn decode_bar(bytes: &[u8]) -> Bar {
    debug_assert_eq!(bytes.len(), BAR_RECORD_LEN);
    let field =
        |range: std::ops::Range<usize>| i64::from_le_bytes(bytes[range].try_into().unwrap());
    Bar {
        time_ms: field(0..8),
        open: field(8..16),
        high: field(16..24),
        low: field(24..32),
        close: field(32..40),
        volume: field(40..48),
    }
}

/// Appends `tick`'s encoded bytes to `out`.
pub fn encode_tick(tick: &Tick, out: &mut Vec<u8>) {
    out.extend_from_slice(&tick.time_ms.to_le_bytes());
    out.extend_from_slice(&tick.price.to_le_bytes());
}

/// Decodes one [`Tick`] from exactly [`TICK_RECORD_LEN`] bytes.
///
/// # Panics
///
/// Panics if `bytes.len() != TICK_RECORD_LEN`, see [`decode_bar`].
pub fn decode_tick(bytes: &[u8]) -> Tick {
    debug_assert_eq!(bytes.len(), TICK_RECORD_LEN);
    Tick {
        time_ms: i64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        price: i64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_round_trips_through_encode_decode() {
        let bar = Bar {
            time_ms: 1_700_000_000_000,
            open: 109_500,
            high: 109_800,
            low: 109_200,
            close: 109_650,
            volume: 42,
        };
        let mut bytes = Vec::new();
        encode_bar(&bar, &mut bytes);
        assert_eq!(bytes.len(), BAR_RECORD_LEN);
        assert_eq!(decode_bar(&bytes), bar);
    }

    #[test]
    fn a_tick_round_trips_through_encode_decode() {
        let tick = Tick {
            time_ms: 1_700_000_000_123,
            price: 109_567,
        };
        let mut bytes = Vec::new();
        encode_tick(&tick, &mut bytes);
        assert_eq!(bytes.len(), TICK_RECORD_LEN);
        assert_eq!(decode_tick(&bytes), tick);
    }
}
