//! The time zone a chart shows times in: UTC, the zone of the computer, or a named zone such as
//! New York or Tokyo. Each chart has its own.
//!
//! Bars and ticks always carry Unix milliseconds. A zone only decides how they are written on
//! the axis and in the crosshair, and where a "day" begins for the labels: shifting a time by the
//! zone's offset makes its UTC calendar fields read as local ones, so everything else (the
//! label steps, the month names) works on shifted times and stays the same code for every zone.
//!
//! The offset is asked per time, not once, because it changes with daylight saving time and a
//! chart can span a change. The answers are cached by quarter of an hour, since a chart asks
//! about neighbouring times one after the other and offsets only change on such boundaries.
//!
//! A zone is saved as `"utc"`, `"local"` or the IANA name (`"America/New_York"`).

use std::cell::Cell;

use chrono::{Local, Offset, TimeZone};
use chrono_tz::Tz;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zone {
    Utc,
    #[default]
    Local,
    Named(Tz),
}

/// The zones the menu offers, with the name shown for each: the market centres a trader follows,
/// west to east.
pub const NAMED: [(Tz, &str); 22] = [
    (Tz::Pacific__Honolulu, "Honolulu"),
    (Tz::America__Los_Angeles, "Los Angeles"),
    (Tz::America__Denver, "Denver"),
    (Tz::America__Chicago, "Chicago"),
    (Tz::America__New_York, "New York"),
    (Tz::America__Toronto, "Toronto"),
    (Tz::America__Sao_Paulo, "Sao Paulo"),
    (Tz::Europe__London, "London"),
    (Tz::Europe__Paris, "Paris"),
    (Tz::Europe__Berlin, "Berlin"),
    (Tz::Europe__Zurich, "Zurich"),
    (Tz::Europe__Athens, "Athens"),
    (Tz::Europe__Moscow, "Moscow"),
    (Tz::Asia__Dubai, "Dubai"),
    (Tz::Asia__Kolkata, "Kolkata"),
    (Tz::Asia__Bangkok, "Bangkok"),
    (Tz::Asia__Singapore, "Singapore"),
    (Tz::Asia__Hong_Kong, "Hong Kong"),
    (Tz::Asia__Shanghai, "Shanghai"),
    (Tz::Asia__Tokyo, "Tokyo"),
    (Tz::Australia__Sydney, "Sydney"),
    (Tz::Pacific__Auckland, "Auckland"),
];

/// How long one cached answer covers.
const CACHE_SPAN_MS: i64 = 15 * 60 * 1_000;

thread_local! {
    /// The last offset asked for: the zone, which quarter of an hour it was for, and the offset.
    static LAST: Cell<Option<(Zone, i64, i64)>> = const { Cell::new(None) };
}

impl Zone {
    /// Every zone of the menu: UTC, the computer's, then the named ones.
    pub fn menu() -> Vec<Self> {
        let mut zones = vec![Self::Utc, Self::Local];
        zones.extend(NAMED.iter().map(|(tz, _)| Self::Named(*tz)));
        zones
    }

    /// The offset from UTC at `time_ms`, in milliseconds.
    pub fn offset_ms(self, time_ms: i64) -> i64 {
        if self == Self::Utc {
            return 0;
        }
        let slot = time_ms.div_euclid(CACHE_SPAN_MS);
        if let Some((zone, cached, offset)) = LAST.get()
            && zone == self
            && cached == slot
        {
            return offset;
        }
        let at = slot * CACHE_SPAN_MS;
        let offset = match self {
            Self::Utc => 0,
            Self::Local => local_offset_ms(at),
            Self::Named(tz) => named_offset_ms(tz, at),
        };
        LAST.set(Some((self, slot, offset)));
        offset
    }

    /// `time_ms` moved so that its UTC calendar fields read as the time in this zone.
    pub fn shift(self, time_ms: i64) -> i64 {
        time_ms.saturating_add(self.offset_ms(time_ms))
    }

    /// The day `time_ms` is in, counted from the epoch, in this zone.
    pub fn day(self, time_ms: i64) -> i64 {
        self.shift(time_ms).div_euclid(86_400_000)
    }

    /// The label for the corner of the chart: `UTC`, or the offset such as `UTC+2` or
    /// `UTC-3:30`, as it is at `time_ms`.
    pub fn label(self, time_ms: i64) -> String {
        match self {
            Self::Utc => "UTC".to_owned(),
            _ => offset_label(self.offset_ms(time_ms)),
        }
    }

    /// The name for the menu: `UTC`, `Local (UTC+2)`, `New York (UTC-4)`.
    pub fn name(self, now_ms: i64) -> String {
        match self {
            Self::Utc => "UTC".to_owned(),
            Self::Local => format!("Local ({})", offset_label(self.offset_ms(now_ms))),
            Self::Named(tz) => {
                let city = NAMED
                    .iter()
                    .find(|(named, _)| *named == tz)
                    .map_or_else(|| tz.name().to_owned(), |(_, name)| (*name).to_owned());
                format!("{city} ({})", offset_label(self.offset_ms(now_ms)))
            }
        }
    }

    /// The text written in saved files.
    pub fn code(self) -> String {
        match self {
            Self::Utc => "utc".to_owned(),
            Self::Local => "local".to_owned(),
            Self::Named(tz) => tz.name().to_owned(),
        }
    }

    /// The zone a saved text names; the computer's zone for anything unknown.
    pub fn from_code(code: &str) -> Self {
        match code {
            "utc" | "UTC" => Self::Utc,
            "local" => Self::Local,
            name => name.parse::<Tz>().map_or(Self::Local, Self::Named),
        }
    }
}

impl Serialize for Zone {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.code())
    }
}

impl<'de> Deserialize<'de> for Zone {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Ok(Self::from_code(&code))
    }
}

fn local_offset_ms(time_ms: i64) -> i64 {
    match Local.timestamp_millis_opt(time_ms).single() {
        Some(local) => i64::from(local.offset().fix().local_minus_utc()) * 1_000,
        None => 0,
    }
}

fn named_offset_ms(tz: Tz, time_ms: i64) -> i64 {
    match tz.timestamp_millis_opt(time_ms).single() {
        Some(at) => i64::from(at.offset().fix().local_minus_utc()) * 1_000,
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

    /// 2026-01-05 00:00 UTC, a winter Monday.
    const WINTER: i64 = 1_767_571_200_000;
    /// 2026-07-06 00:00 UTC, a summer Monday.
    const SUMMER: i64 = WINTER + 182 * 86_400_000;

    #[test]
    fn utc_never_shifts() {
        for time in [-1, 0, WINTER, i64::MAX - 1] {
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
    fn a_named_zone_follows_its_daylight_saving_time() {
        let new_york = Zone::Named(Tz::America__New_York);
        assert_eq!(new_york.offset_ms(WINTER), -5 * 3_600_000);
        assert_eq!(new_york.offset_ms(SUMMER), -4 * 3_600_000);
        let tokyo = Zone::Named(Tz::Asia__Tokyo);
        assert_eq!(tokyo.offset_ms(WINTER), 9 * 3_600_000);
        assert_eq!(tokyo.offset_ms(SUMMER), 9 * 3_600_000);
        let kolkata = Zone::Named(Tz::Asia__Kolkata);
        assert_eq!(kolkata.label(WINTER), "UTC+5:30");
    }

    #[test]
    fn the_cache_never_mixes_two_zones() {
        let london = Zone::Named(Tz::Europe__London);
        let sydney = Zone::Named(Tz::Australia__Sydney);
        assert_eq!(london.offset_ms(SUMMER), 3_600_000);
        assert_eq!(sydney.offset_ms(SUMMER), 10 * 3_600_000);
        assert_eq!(london.offset_ms(SUMMER + 1), 3_600_000);
    }

    #[test]
    fn days_begin_at_midnight_in_the_zone() {
        let new_york = Zone::Named(Tz::America__New_York);
        // 03:00 UTC on 5 Jan is still 4 Jan in New York.
        assert_eq!(
            new_york.day(WINTER + 3 * 3_600_000),
            Zone::Utc.day(WINTER) - 1
        );
        assert_eq!(new_york.day(WINTER + 6 * 3_600_000), Zone::Utc.day(WINTER));
    }

    #[test]
    fn the_local_zone_is_a_plausible_offset_and_stable_across_neighbours() {
        let a = Zone::Local.offset_ms(WINTER);
        let b = Zone::Local.offset_ms(WINTER + 60_000);
        assert_eq!(a, b);
        assert!((-12 * 3_600_000..=14 * 3_600_000).contains(&a), "{a}");
        assert_eq!(Zone::Local.shift(WINTER), WINTER + a);
    }

    #[test]
    fn zones_are_saved_by_name_and_unknown_names_fall_back() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            zone: Zone,
        }
        for zone in Zone::menu() {
            let text = toml::to_string(&Holder { zone }).unwrap();
            let back: Holder = toml::from_str(&text).unwrap();
            assert_eq!(back.zone, zone);
        }
        let text = toml::to_string(&Holder { zone: Zone::Utc }).unwrap();
        assert_eq!(text.trim(), "zone = \"utc\"");
        let named: Holder = toml::from_str("zone = \"Asia/Tokyo\"").unwrap();
        assert_eq!(named.zone, Zone::Named(Tz::Asia__Tokyo));
        let unknown: Holder = toml::from_str("zone = \"Mars/Olympus\"").unwrap();
        assert_eq!(unknown.zone, Zone::Local);
    }

    #[test]
    fn menu_names_say_the_city_and_the_offset() {
        assert_eq!(Zone::Utc.name(WINTER), "UTC");
        assert_eq!(
            Zone::Named(Tz::America__New_York).name(WINTER),
            "New York (UTC-5)"
        );
        assert!(Zone::Local.name(WINTER).starts_with("Local (UTC"));
    }
}
