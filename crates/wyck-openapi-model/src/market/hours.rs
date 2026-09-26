//! When a symbol trades: its weekly sessions, its holidays and its trading mode, read from the
//! symbol's details, and whether the market is open at a given moment.
//!
//! The Open API gives each session as seconds from Sunday 00:00 in the schedule's time zone
//! (start included, end excluded), and each holiday as a day (days since 1970-01-01), optionally
//! limited to part of it by seconds from the start of that day.

use chrono::{Datelike, TimeZone, Timelike};
use chrono_tz::Tz;
use serde::Deserialize;

use crate::transport::wire::flex;

use super::Symbol;

const WEEK: i64 = 7 * 86_400;

/// One trading session of the week (`ProtoOAInterval`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interval {
    /// Seconds from Sunday 00:00, included.
    #[serde(deserialize_with = "flex::int")]
    pub start_second: i64,
    /// Seconds from Sunday 00:00, excluded.
    #[serde(deserialize_with = "flex::int")]
    pub end_second: i64,
}

/// A day the symbol does not trade, or trades less (`ProtoOAHoliday`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holiday {
    /// What the holiday is called.
    #[serde(default)]
    pub name: Option<String>,
    /// Days since 1970-01-01.
    #[serde(deserialize_with = "flex::int")]
    pub holiday_date: i64,
    /// Whether it comes back every year on the same day.
    #[serde(default)]
    pub is_recurring: bool,
    /// Seconds from the start of the day; the whole day when absent.
    #[serde(default, deserialize_with = "flex::opt")]
    pub start_second: Option<i64>,
    /// Seconds from the start of the day where it ends; the end of the day when absent.
    #[serde(default, deserialize_with = "flex::opt")]
    pub end_second: Option<i64>,
}

/// Where a market stands at a moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketStatus {
    /// Trading, until `closes_at` (Unix milliseconds) when a session end is known.
    Open {
        /// When the session ends, in Unix milliseconds.
        closes_at: Option<i64>,
    },
    /// Not trading, until `opens_at` when the next session is known; `holiday` names the reason.
    Closed {
        /// When the next session starts, in Unix milliseconds.
        opens_at: Option<i64>,
        /// The holiday the market is closed for, if that is why.
        holiday: Option<String>,
    },
    /// Positions can only be closed.
    CloseOnly,
    /// The broker disabled trading on the symbol.
    Disabled,
}

impl MarketStatus {
    /// Whether the market trades now.
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }
}

/// A symbol's trading hours.
#[derive(Debug, Clone, PartialEq)]
pub struct TradingHours {
    zone: Tz,
    sessions: Vec<(i64, i64)>,
    holidays: Vec<Holiday>,
    mode: i64,
}

impl TradingHours {
    /// The hours in a symbol's details. A symbol with no schedule trades all week.
    pub fn from_symbol(symbol: &Symbol) -> Self {
        let zone = symbol
            .schedule_time_zone
            .as_deref()
            .and_then(|name| name.parse().ok())
            .unwrap_or(Tz::UTC);
        let mut sessions: Vec<(i64, i64)> = symbol
            .schedule
            .iter()
            .map(|i| (i.start_second.clamp(0, WEEK), i.end_second.clamp(0, WEEK)))
            .filter(|(a, b)| a < b)
            .collect();
        if sessions.is_empty() {
            sessions.push((0, WEEK));
        }
        sessions.sort_unstable();
        Self {
            zone,
            sessions,
            holidays: symbol.holiday.clone(),
            mode: symbol.trading_mode.unwrap_or(0),
        }
    }

    /// Where the market stands at `now_ms`.
    pub fn status_at(&self, now_ms: i64) -> MarketStatus {
        match self.mode {
            1 | 2 => return MarketStatus::Disabled,
            3 => return MarketStatus::CloseOnly,
            _ => {}
        }
        let Some(local) = self.zone.timestamp_millis_opt(now_ms).single() else {
            return MarketStatus::Open { closes_at: None };
        };
        let second_of_day = i64::from(local.num_seconds_from_midnight());
        let week_second =
            i64::from(local.weekday().num_days_from_sunday()) * 86_400 + second_of_day;
        let at = |target_week_second: i64| {
            // Seconds ahead, wrapping round the week.
            let ahead = (target_week_second - week_second).rem_euclid(WEEK);
            now_ms + ahead * 1_000
        };
        if let Some(holiday) = self.holiday_on(local.date_naive(), second_of_day) {
            let end_of_day = holiday.end_second.unwrap_or(86_400);
            let resume = week_second - second_of_day + end_of_day;
            let opens = self.next_open_from(resume.rem_euclid(WEEK)).map(at);
            return MarketStatus::Closed {
                opens_at: opens,
                holiday: holiday.name.clone(),
            };
        }
        if let Some((_, end)) = self
            .sessions
            .iter()
            .find(|(a, b)| (*a..*b).contains(&week_second))
        {
            // Back to back sessions (one ending as the next starts, round the week too) are one
            // stretch of trading.
            let mut end = *end;
            for _ in 0..self.sessions.len() {
                let Some((a, b)) = self
                    .sessions
                    .iter()
                    .find(|(a, _)| *a == end.rem_euclid(WEEK))
                else {
                    break;
                };
                end += b - a;
            }
            let left = end - week_second;
            let closes_at = (left < WEEK).then(|| now_ms + left * 1_000);
            return MarketStatus::Open { closes_at };
        }
        MarketStatus::Closed {
            opens_at: self.next_open_from(week_second).map(at),
            holiday: None,
        }
    }

    /// The next session start at or after `week_second`, wrapping round the week.
    fn next_open_from(&self, week_second: i64) -> Option<i64> {
        if self
            .sessions
            .iter()
            .any(|(a, b)| (*a..*b).contains(&week_second))
        {
            return Some(week_second);
        }
        self.sessions
            .iter()
            .map(|(a, _)| *a)
            .min_by_key(|a| (a - week_second).rem_euclid(WEEK))
    }

    fn holiday_on(&self, date: chrono::NaiveDate, second_of_day: i64) -> Option<&Holiday> {
        let days = date
            .signed_duration_since(chrono::NaiveDate::from_ymd_opt(1970, 1, 1)?)
            .num_days();
        self.holidays.iter().find(|h| {
            let same_day = if h.is_recurring {
                chrono::NaiveDate::from_ymd_opt(1970, 1, 1)
                    .and_then(|epoch| {
                        epoch.checked_add_signed(chrono::Duration::days(h.holiday_date))
                    })
                    .is_some_and(|d| d.month() == date.month() && d.day() == date.day())
            } else {
                h.holiday_date == days
            };
            let start = h.start_second.unwrap_or(0);
            let end = h.end_second.unwrap_or(86_400);
            same_day && (start..end).contains(&second_of_day)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(schedule: &[(i64, i64)], zone: &str) -> Symbol {
        serde_json::from_value(serde_json::json!({
            "symbolId": 1,
            "digits": 5,
            "pipPosition": 4,
            "scheduleTimeZone": zone,
            "schedule": schedule
                .iter()
                .map(|(a, b)| serde_json::json!({ "startSecond": a, "endSecond": b }))
                .collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    const DAY: i64 = 86_400;
    // 2024-01-03 was a Wednesday.
    const WEDNESDAY_NOON_MS: i64 = 1_704_283_200_000;

    #[test]
    fn a_forex_week_is_open_midweek_and_closed_at_the_weekend() {
        // Sunday 22:00 to Friday 22:00, UTC.
        let hours =
            TradingHours::from_symbol(&symbol(&[(22 * 3_600, 5 * DAY + 22 * 3_600)], "UTC"));
        let status = hours.status_at(WEDNESDAY_NOON_MS);
        // Friday 22:00 is two days and ten hours ahead.
        assert_eq!(
            status,
            MarketStatus::Open {
                closes_at: Some(WEDNESDAY_NOON_MS + (2 * DAY + 10 * 3_600) * 1_000)
            }
        );
        // Saturday noon: closed until Sunday 22:00, 34 hours later.
        let saturday = WEDNESDAY_NOON_MS + 3 * DAY * 1_000;
        assert_eq!(
            hours.status_at(saturday),
            MarketStatus::Closed {
                opens_at: Some(saturday + 34 * 3_600 * 1_000),
                holiday: None
            }
        );
    }

    #[test]
    fn a_daily_break_closes_the_market_for_its_hour() {
        // Every weekday from 23:00 to 22:00 the next day, in New York time.
        let sessions: Vec<(i64, i64)> = (0..5)
            .map(|d| (d * DAY + 23 * 3_600, (d + 1) * DAY + 22 * 3_600))
            .collect();
        let hours = TradingHours::from_symbol(&symbol(&sessions, "America/New_York"));
        // Wednesday 22:30 in New York (03:30 UTC Thursday in winter).
        let break_ms = WEDNESDAY_NOON_MS + (15 * 3_600 + 1_800) * 1_000;
        assert!(
            matches!(hours.status_at(break_ms), MarketStatus::Closed { opens_at: Some(t), .. } if t == break_ms + 1_800 * 1_000)
        );
        assert!(hours.status_at(WEDNESDAY_NOON_MS).is_open());
    }

    #[test]
    fn no_schedule_trades_all_week_and_modes_win() {
        let mut s = symbol(&[], "UTC");
        let hours = TradingHours::from_symbol(&s);
        assert_eq!(
            hours.status_at(WEDNESDAY_NOON_MS),
            MarketStatus::Open { closes_at: None }
        );
        s.trading_mode = Some(3);
        assert_eq!(
            TradingHours::from_symbol(&s).status_at(WEDNESDAY_NOON_MS),
            MarketStatus::CloseOnly
        );
    }

    #[test]
    fn a_holiday_closes_its_day() {
        let mut s = symbol(&[(0, 7 * DAY)], "UTC");
        s.holiday = vec![Holiday {
            name: Some("New Year".into()),
            holiday_date: 19_725, // 2024-01-03
            is_recurring: false,
            start_second: None,
            end_second: None,
        }];
        let hours = TradingHours::from_symbol(&s);
        assert_eq!(
            hours.status_at(WEDNESDAY_NOON_MS),
            MarketStatus::Closed {
                opens_at: Some(WEDNESDAY_NOON_MS + 12 * 3_600 * 1_000),
                holiday: Some("New Year".into())
            }
        );
    }
}
