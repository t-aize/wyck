//! Calendar arithmetic for the indicators: the period a day belongs to (day, week, month, year),
//! the fields of a time, and the trading sessions of a day.
//!
//! A day is counted from the epoch in the time zone of the chart (see `chart::Zone::day`), and a
//! time is the Unix milliseconds moved by the offset of that zone, so that its UTC calendar fields
//! read as the local ones. Weeks begin on Monday.

use chrono::{DateTime, Datelike, NaiveDate, Timelike};

/// The period a series is grouped by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Day,
    Week,
    Month,
    Year,
}

impl Anchor {
    /// The anchor a name gives: `day`, `week`, `month` or `year`, in any case.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "day" | "d" | "1d" => Some(Self::Day),
            "week" | "w" | "1w" => Some(Self::Week),
            "month" | "m" | "1m" => Some(Self::Month),
            "year" | "y" | "12m" => Some(Self::Year),
            _ => None,
        }
    }

    /// The anchor of a choice input: 0 day, 1 week, 2 month, 3 year.
    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Week,
            2 => Self::Month,
            3 => Self::Year,
            _ => Self::Day,
        }
    }

    /// A number that is the same for every day of a period and differs between two periods that
    /// follow each other. `day` counts from the epoch.
    pub fn key(self, day: i64) -> i64 {
        let date = || {
            i32::try_from(day + 719_163)
                .ok()
                .and_then(NaiveDate::from_num_days_from_ce_opt)
        };
        match self {
            Self::Day => day,
            // Day 0 is a Thursday: shifting by 3 makes a week begin on Monday.
            Self::Week => (day + 3).div_euclid(7),
            Self::Month => date().map_or(day, |d| i64::from(d.year()) * 12 + i64::from(d.month())),
            Self::Year => date().map_or(day, |d| i64::from(d.year())),
        }
    }
}

/// The calendar fields of a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fields {
    pub year: i32,
    pub month: u32,
    pub day_of_month: u32,
    /// Sunday is 1 and Saturday is 7.
    pub day_of_week: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// The week of the year, from 1 to 53, by the ISO rule (a week begins on Monday).
    pub week_of_year: u32,
}

/// The fields of `shifted_ms`, Unix milliseconds already moved by the offset of the zone. `None`
/// for a time too far from today to be a date.
pub fn fields(shifted_ms: i64) -> Option<Fields> {
    let time = DateTime::from_timestamp_millis(shifted_ms)?.naive_utc();
    Some(Fields {
        year: time.year(),
        month: time.month(),
        day_of_month: time.day(),
        day_of_week: time.weekday().num_days_from_sunday() + 1,
        hour: time.hour(),
        minute: time.minute(),
        second: time.second(),
        week_of_year: time.iso_week().week(),
    })
}

/// A part of the day, such as `0930-1600`. One that ends before it begins (`2200-0600`) goes over
/// midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    from: u32,
    to: u32,
}

impl Session {
    /// The session a text names; `None` for anything that is not `HHMM-HHMM`. An empty text is
    /// the whole day.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Some(Self { from: 0, to: 0 });
        }
        let (from, to) = text.split_once('-')?;
        let minutes = |part: &str| -> Option<u32> {
            let ok = part.len() == 4 && part.bytes().all(|b| b.is_ascii_digit());
            if !ok {
                return None;
            }
            let hour: u32 = part.get(..2)?.parse().ok()?;
            let minute: u32 = part.get(2..)?.parse().ok()?;
            (hour < 24 && minute < 60).then_some(hour * 60 + minute)
        };
        Some(Self {
            from: minutes(from)?,
            to: minutes(to)?,
        })
    }

    /// Whether the minute of the day (0 to 1439) is in the session. The end is not in it: a
    /// session `0930-1600` stops at 15:59.
    pub fn contains(self, minute_of_day: u32) -> bool {
        match self.from.cmp(&self.to) {
            std::cmp::Ordering::Equal => true,
            std::cmp::Ordering::Less => (self.from..self.to).contains(&minute_of_day),
            std::cmp::Ordering::Greater => minute_of_day >= self.from || minute_of_day < self.to,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1970-01-01 was a Thursday.
    #[test]
    fn a_week_begins_on_monday_and_a_month_and_a_year_on_their_first_day() {
        // Day 4 is Monday 1970-01-05; day 3 the Sunday before.
        assert_eq!(Anchor::Week.key(3), Anchor::Week.key(0));
        assert_ne!(Anchor::Week.key(3), Anchor::Week.key(4));
        assert_eq!(Anchor::Week.key(4), Anchor::Week.key(10));
        assert_ne!(Anchor::Week.key(10), Anchor::Week.key(11));
        // 1970-01-31 is day 30 and 1970-02-01 is day 31.
        assert_eq!(Anchor::Month.key(0), Anchor::Month.key(30));
        assert_ne!(Anchor::Month.key(30), Anchor::Month.key(31));
        // 1970-12-31 is day 364 and 1971-01-01 is day 365.
        assert_eq!(Anchor::Year.key(0), Anchor::Year.key(364));
        assert_ne!(Anchor::Year.key(364), Anchor::Year.key(365));
        assert_eq!(Anchor::Day.key(7), 7);
    }

    #[test]
    fn anchors_are_read_from_a_name_or_a_choice() {
        assert_eq!(Anchor::parse(" Week "), Some(Anchor::Week));
        assert_eq!(Anchor::parse("1D"), Some(Anchor::Day));
        assert_eq!(Anchor::parse("quarter"), None);
        assert_eq!(Anchor::from_index(3), Anchor::Year);
        assert_eq!(Anchor::from_index(99), Anchor::Day);
    }

    #[test]
    fn the_fields_of_a_time_are_those_of_the_calendar() {
        // 2026-01-05 (a Monday) 09:30:15 UTC.
        let ms = 1_767_605_415_000;
        let f = fields(ms).unwrap();
        assert_eq!((f.year, f.month, f.day_of_month), (2026, 1, 5));
        assert_eq!((f.hour, f.minute, f.second), (9, 30, 15));
        assert_eq!(f.day_of_week, 2, "Sunday is 1, so Monday is 2");
        assert_eq!(f.week_of_year, 2);
        assert!(fields(i64::MAX).is_none());
    }

    #[test]
    fn a_session_is_a_part_of_the_day_and_may_cross_midnight() {
        let day = Session::parse("0930-1600").unwrap();
        assert!(day.contains(9 * 60 + 30) && day.contains(15 * 60 + 59));
        assert!(!day.contains(16 * 60) && !day.contains(9 * 60 + 29));
        let night = Session::parse("2200-0600").unwrap();
        assert!(night.contains(23 * 60) && night.contains(5 * 60 + 59));
        assert!(!night.contains(12 * 60));
        assert!(Session::parse("").unwrap().contains(0));
        assert_eq!(Session::parse("0930"), None);
        assert_eq!(Session::parse("2460-0100"), None);
    }
}
