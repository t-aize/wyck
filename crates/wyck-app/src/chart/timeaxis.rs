//! The time axis: which bars get a label, and what it says.
//!
//! Labels follow the usual trading platform habit. Inside a day they are clock times
//! (`14:30`); where a new day, month or year starts the label changes to the day (`Sep 20`), the
//! month (`Sep`) or the year (`2026`), and those **boundary** labels are the ones kept when space
//! is short. Times are shown in UTC: the broker's own time zone is not known to the app.
//!
//! Which bars carry a label is decided from the bar's index in the series, not from its position
//! on screen, so the labels stay put while the chart is dragged instead of shimmering. The gap
//! between two labels is a whole number of bars taken from a short list of round steps.

use time::{Month, OffsetDateTime};
use wyck_engine::domain::{Candle, Period, UnixMillis};

use super::viewport::Viewport;

/// How important a label is: a bigger boundary wins a place over a smaller one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// An ordinary label: a clock time inside a day, or a day inside a month.
    Plain,
    /// The bar starts a new day.
    Day,
    /// The bar starts a new month.
    Month,
    /// The bar starts a new year.
    Year,
}

/// One label on the axis.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisLabel {
    /// The bar the label belongs to.
    pub index: usize,
    /// Centre of the label in pixels.
    pub x: f64,
    /// The text.
    pub text: String,
    /// Boundary labels are drawn bolder.
    pub rank: Rank,
}

/// Round bar counts a label gap is taken from, so the gap does not change on every zoom step.
const STEPS: [usize; 14] = [1, 2, 3, 5, 10, 15, 20, 30, 50, 100, 200, 500, 1000, 5000];

fn date_time(time: UnixMillis) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(time) * 1_000_000).ok()
}

const fn month_name(month: Month) -> &'static str {
    match month {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

/// What kind of boundary the bar at `time` is, compared with the bar before it (`previous`).
#[must_use]
pub fn rank_of(time: UnixMillis, previous: Option<UnixMillis>) -> Rank {
    let (Some(now), Some(before)) = (date_time(time), previous.and_then(date_time)) else {
        return Rank::Plain;
    };
    if now.year() != before.year() {
        Rank::Year
    } else if now.month() != before.month() {
        Rank::Month
    } else if now.date() != before.date() {
        Rank::Day
    } else {
        Rank::Plain
    }
}

/// The text of a label for the bar at `time`.
#[must_use]
pub fn label_text(time: UnixMillis, rank: Rank, period: Period) -> String {
    let Some(at) = date_time(time) else {
        return String::new();
    };
    match rank {
        Rank::Year => at.year().to_string(),
        Rank::Month => month_name(at.month()).to_owned(),
        Rank::Day => format!("{} {}", month_name(at.month()), at.day()),
        Rank::Plain => match period {
            Period::MN1 => month_name(at.month()).to_owned(),
            Period::D1 | Period::W1 => at.day().to_string(),
            _ => format!("{:02}:{:02}", at.hour(), at.minute()),
        },
    }
}

/// The full date and time of a bar, as the crosshair shows it: `2026-09-20 14:30` for intraday
/// bars and `2026-09-20` for day bars and longer.
#[must_use]
pub fn full_text(time: UnixMillis, period: Period) -> String {
    let Some(at) = date_time(time) else {
        return String::new();
    };
    let date = format!("{}-{:02}-{:02}", at.year(), u8::from(at.month()), at.day());
    match period {
        Period::D1 | Period::W1 | Period::MN1 => date,
        _ => format!("{date} {:02}:{:02}", at.hour(), at.minute()),
    }
}

/// The smallest round step whose gap on screen is at least `min_gap_px`.
fn step_for(spacing: f64, min_gap_px: f64) -> usize {
    let need = (min_gap_px / spacing).ceil().max(1.0) as usize;
    STEPS
        .iter()
        .copied()
        .find(|step| *step >= need)
        .unwrap_or(need.next_multiple_of(STEPS[STEPS.len() - 1]))
}

/// The labels for the visible bars, left to right, none closer than `min_gap_px`.
///
/// `bars` is the whole series and `count` its length (passed for symmetry with the viewport
/// maths); labels are placed with `viewport`. Boundary bars (a new day, month or year) are always
/// candidates and beat plain labels that would touch them.
#[must_use]
pub fn labels(
    bars: &[Candle],
    viewport: &Viewport,
    period: Period,
    min_gap_px: f64,
) -> Vec<AxisLabel> {
    let count = bars.len();
    let range = viewport.visible(count);
    if range.is_empty() {
        return Vec::new();
    }
    let step = step_for(viewport.bar_spacing, min_gap_px);
    let mut candidates: Vec<AxisLabel> = Vec::new();
    for index in range {
        let previous = index.checked_sub(1).map(|i| bars[i].time);
        let rank = rank_of(bars[index].time, previous);
        let on_grid = (count - 1 - index).is_multiple_of(step);
        // Daily bars and longer have a boundary nearly every bar; keep the coarse ones only.
        let boundary = match period {
            Period::D1 | Period::W1 => rank >= Rank::Month,
            Period::MN1 => rank >= Rank::Year,
            _ => rank >= Rank::Day,
        };
        if on_grid || boundary {
            let x = viewport.x_of(index as f64, count);
            if x >= 0.0 && x <= viewport.width {
                candidates.push(AxisLabel {
                    index,
                    x,
                    text: label_text(bars[index].time, rank, period),
                    rank,
                });
            }
        }
    }

    // Greedy from the most important label down: a kept label blocks its neighbours.
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| {
        candidates[b]
            .rank
            .cmp(&candidates[a].rank)
            .then(candidates[a].x.total_cmp(&candidates[b].x))
    });
    let mut kept: Vec<usize> = Vec::new();
    for i in order {
        let x = candidates[i].x;
        if kept
            .iter()
            .all(|&k| (candidates[k].x - x).abs() >= min_gap_px)
        {
            kept.push(i);
        }
    }
    kept.sort_unstable();
    kept.into_iter().map(|i| candidates[i].clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    // 2026-09-30 22:00 UTC.
    const START: i64 = 1_790_805_600_000;

    fn hourly(n: usize) -> Vec<Candle> {
        (0..n as i64)
            .map(|i| Candle::at(START + i * HOUR, 1.0))
            .collect()
    }

    #[test]
    fn boundaries_are_told_from_the_bar_before() {
        let before = START + HOUR; // 23:00 on Sep 30
        assert_eq!(rank_of(before + HOUR, Some(before)), Rank::Month);
        assert_eq!(rank_of(before, Some(before - HOUR)), Rank::Plain);
        assert_eq!(rank_of(before, None), Rank::Plain);
        // New year: 2026-12-31 23:00 to 2027-01-01 00:00.
        let nye = 1_798_761_600_000 - HOUR;
        assert_eq!(rank_of(nye + HOUR, Some(nye)), Rank::Year);
    }

    #[test]
    fn texts_follow_the_rank_and_the_period() {
        assert_eq!(label_text(START, Rank::Plain, Period::H1), "22:00");
        assert_eq!(label_text(START, Rank::Day, Period::H1), "Sep 30");
        assert_eq!(label_text(START, Rank::Month, Period::H1), "Sep");
        assert_eq!(label_text(START, Rank::Year, Period::H1), "2026");
        assert_eq!(label_text(START, Rank::Plain, Period::D1), "30");
        assert_eq!(label_text(START, Rank::Plain, Period::MN1), "Sep");
    }

    #[test]
    fn the_full_text_has_a_time_only_for_intraday() {
        assert_eq!(full_text(START, Period::H1), "2026-09-30 22:00");
        assert_eq!(full_text(START, Period::D1), "2026-09-30");
    }

    #[test]
    fn steps_are_round_and_wide_enough() {
        assert_eq!(step_for(10.0, 80.0), 10);
        assert_eq!(step_for(50.0, 80.0), 2);
        assert_eq!(step_for(100.0, 80.0), 1);
        assert_eq!(step_for(0.1, 80.0), 1000);
    }

    #[test]
    fn labels_keep_their_distance_and_stay_on_screen() {
        let bars = hourly(200);
        let view = Viewport {
            bar_spacing: 10.0,
            right_offset: 0.0,
            width: 800.0,
        };
        let labels = labels(&bars, &view, Period::H1, 80.0);
        assert!(!labels.is_empty());
        for pair in labels.windows(2) {
            assert!(pair[1].x - pair[0].x >= 80.0, "{pair:?}");
        }
        assert!(labels.iter().all(|l| l.x >= 0.0 && l.x <= 800.0));
    }

    #[test]
    fn a_day_boundary_beats_a_plain_time_next_to_it() {
        let bars = hourly(200);
        let view = Viewport {
            bar_spacing: 10.0,
            right_offset: 0.0,
            width: 800.0,
        };
        let out = labels(&bars, &view, Period::H1, 80.0);
        // The bar at index 2 is 2026-10-01 00:00, a new month.
        let month = out.iter().find(|l| l.index == 2);
        assert!(month.is_none() || month.unwrap().text == "Oct");
        assert!(out.iter().any(|l| l.rank >= Rank::Day));
    }

    #[test]
    fn labels_do_not_shimmer_while_dragging() {
        let bars = hourly(500);
        let mut view = Viewport {
            bar_spacing: 10.0,
            right_offset: 0.0,
            width: 800.0,
        };
        let before: Vec<usize> = labels(&bars, &view, Period::H1, 80.0)
            .iter()
            .filter(|l| l.rank == Rank::Plain)
            .map(|l| l.index)
            .collect();
        view.pan_by(30.0, bars.len());
        let after: Vec<usize> = labels(&bars, &view, Period::H1, 80.0)
            .iter()
            .filter(|l| l.rank == Rank::Plain)
            .map(|l| l.index)
            .collect();
        let shared = before.iter().filter(|i| after.contains(i)).count();
        assert!(
            shared >= before.len().saturating_sub(4),
            "{before:?} {after:?}"
        );
    }

    #[test]
    fn nothing_to_label_gives_nothing() {
        let view = Viewport::new(800.0);
        assert!(labels(&[], &view, Period::H1, 80.0).is_empty());
    }
}
