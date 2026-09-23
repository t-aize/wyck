//! The time zone the chart shows times in: UTC, or the zone of the computer.
//!
//! Bars and ticks always carry Unix milliseconds. A zone only decides how they are written on
//! the axis and in the crosshair, and where a "day" begins for the labels: shifting a time by the
//! zone's offset makes its UTC calendar fields read as local ones, so everything else (the
//! label steps, the month names) works on shifted times and stays the same code for both zones.
//!
//! The local offset is asked per time, not once, because it changes with daylight saving time
//! and a chart can span a change. The answers are cached by quarter of an hour, since a chart
//! asks about neighbouring times one after the other and offsets only change on such
//! boundaries.

use std::cell::Cell;

use chrono::{Local, Offset, TimeZone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Zone {
    Utc,
    #[default]
    Local,
}

/// How long one cached answer covers.
const CACHE_SPAN_MS: i64 = 15 * 60 * 1_000;

thread_local! {
    /// The last local offset asked for: which quarter of an hour it was for, and the offset.
    static LAST: Cell<Option<(i64, i64)>> = const { Cell::new(None) };
}

impl Zone {
    /// The other zone.
    pub fn toggled(self) -> Self {
        match self {
            Self::Utc => Self::Local,
            Self::Local => Self::Utc,
        }
    }

    /// The offset from UTC at `time_ms`, in milliseconds.
    pub fn offset_ms(self, time_ms: i64) -> i64 {
        match self {
            Self::Utc => 0,
            Self::Local => {
                let slot = time_ms.div_euclid(CACHE_SPAN_MS);
                if let Some((cached, offset)) = LAST.get()
                    && cached == slot
                {
                    return offset;
                }
                let offset = local_offset_ms(slot * CACHE_SPAN_MS);
                LAST.set(Some((slot, offset)));
                offset
            }
        }
    }

    /// `time_ms` moved so that its UTC calendar fields read as the time in this zone.
    pub fn shift(self, time_ms: i64) -> i64 {
        time_ms.saturating_add(self.offset_ms(time_ms))
    }

    /// The label for the corner of the chart: `UTC`, or the local offset such as `UTC+2` or
    /// `UTC-3:30`, as it is at `time_ms`.
    pub fn label(self, time_ms: i64) -> String {
        match self {
            Self::Utc => "UTC".to_owned(),
            Self::Local => offset_label(self.offset_ms(time_ms)),
        }
    }
}

fn local_offset_ms(time_ms: i64) -> i64 {
    match Local.timestamp_millis_opt(time_ms).single() {
        Some(local) => i64::from(local.offset().fix().local_minus_utc()) * 1_000,
        None => 0,
    }
}

/// `UTC`, `UTC+2`, `UTC-3:30`, for an offset in milliseconds.
pub fn offset_label(offset_ms: i64) -> String {
    let minutes = offset_ms / 60_000;
    if minutes == 0 {
        return "UTC".to_owned();
    }
    let sign = if minutes < 0 { '-' } else { '+' };
    let (hours, rest) = (minutes.abs() / 60, minutes.abs() % 60);
    if rest == 0 {
        format!("UTC{sign}{hours}")
    } else {
        format!("UTC{sign}{hours}:{rest:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_never_shifts() {
        for time in [-1, 0, 1_767_571_200_000, i64::MAX - 1] {
            assert_eq!(Zone::Utc.offset_ms(time), 0);
        }
        assert_eq!(Zone::Utc.shift(123), 123);
        assert_eq!(Zone::Utc.label(0), "UTC");
    }

    #[test]
    fn offsets_read_as_utc_plus_or_minus_hours_and_minutes() {
        assert_eq!(offset_label(0), "UTC");
        assert_eq!(offset_label(2 * 3_600_000), "UTC+2");
        assert_eq!(offset_label(-3 * 3_600_000), "UTC-3");
        assert_eq!(offset_label(-(3 * 3_600_000 + 30 * 60_000)), "UTC-3:30");
        assert_eq!(offset_label(5 * 3_600_000 + 45 * 60_000), "UTC+5:45");
    }

    #[test]
    fn the_local_zone_is_a_plausible_offset_and_stable_across_neighbours() {
        // Whatever zone the machine is in, the offset is within the real range, and two times a
        // minute apart agree (the cache must not change the answer).
        let time = 1_767_571_200_000;
        let a = Zone::Local.offset_ms(time);
        let b = Zone::Local.offset_ms(time + 60_000);
        assert_eq!(a, b);
        assert!((-12 * 3_600_000..=14 * 3_600_000).contains(&a), "{a}");
        assert_eq!(Zone::Local.shift(time), time + a);
    }

    #[test]
    fn the_cache_follows_a_jump_to_a_distant_time() {
        // Jan and Jul of one year: the answers must each be right for their own time, never the
        // cached one of the other.
        let winter = 1_767_571_200_000;
        let summer = winter + 181 * 86_400_000;
        let (w1, s1) = (Zone::Local.offset_ms(winter), Zone::Local.offset_ms(summer));
        let (w2, s2) = (Zone::Local.offset_ms(winter), Zone::Local.offset_ms(summer));
        assert_eq!((w1, s1), (w2, s2));
        assert_eq!(w1, local_offset_ms(winter));
        assert_eq!(s1, local_offset_ms(summer));
    }

    #[test]
    fn zones_toggle_and_are_saved_as_lowercase_words() {
        assert_eq!(Zone::Utc.toggled(), Zone::Local);
        assert_eq!(Zone::Local.toggled(), Zone::Utc);
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            zone: Zone,
        }
        let text = toml::to_string(&Holder { zone: Zone::Utc }).unwrap();
        assert_eq!(text.trim(), "zone = \"utc\"");
        let back: Holder = toml::from_str("zone = \"local\"").unwrap();
        assert_eq!(back.zone, Zone::Local);
    }
}
