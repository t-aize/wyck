//! The rules that decide whether an alert fires, as plain functions of numbers: no window, no
//! broker, no clock. The engine ([`super::Alerts`]) gathers the numbers (a price, the plot of an
//! indicator, the level of a drawing) and asks these.

use wyck_chart::drawing::model::{Point, Tool};

use super::model::{Condition, Trigger};

/// What an alert saw at one moment: the value it watches, the level it is compared with, and
/// the other side of the zone for a zone condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub value: f64,
    pub level: f64,
    pub upper: Option<f64>,
}

impl Reading {
    /// The zone, lowest side first.
    fn zone(&self) -> Option<(f64, f64)> {
        self.upper
            .map(|upper| (self.level.min(upper), self.level.max(upper)))
    }

    fn inside(&self) -> Option<bool> {
        self.zone()
            .map(|(low, high)| self.value >= low && self.value <= high)
    }

    fn is_finite(&self) -> bool {
        self.value.is_finite() && self.level.is_finite() && self.upper.is_none_or(f64::is_finite)
    }
}

/// Whether going from the reading `before` to the reading `now` fires a condition that compares a
/// value with a level or a zone. The level may have moved too (a trend line, an average), so each
/// reading brings its own. A condition that needs more than two readings (a move, a change of
/// direction) is judged by [`moved`] and [`direction_changed`], and never fires here.
pub fn fired(condition: Condition, before: &Reading, now: &Reading) -> bool {
    if !before.is_finite() || !now.is_finite() {
        return false;
    }
    let (d_before, d_now) = (before.value - before.level, now.value - now.level);
    match condition {
        Condition::CrossingUp => d_before < 0.0 && d_now >= 0.0,
        Condition::CrossingDown => d_before > 0.0 && d_now <= 0.0,
        Condition::Crossing => (d_before < 0.0 && d_now >= 0.0) || (d_before > 0.0 && d_now <= 0.0),
        Condition::Above => d_before <= 0.0 && d_now > 0.0,
        Condition::Below => d_before >= 0.0 && d_now < 0.0,
        Condition::EntersZone => before.inside() == Some(false) && now.inside() == Some(true),
        Condition::ExitsZone => before.inside() == Some(true) && now.inside() == Some(false),
        Condition::MovesBy | Condition::ChangesDirection => false,
    }
}

/// The spread between a bid and an ask, in pips of `pip` (the size of one in price). An ask under
/// the bid (a stale side of the quote) counts as no spread.
pub fn spread_pips(bid: f64, ask: f64, pip: f64) -> f64 {
    if pip > 0.0 && bid.is_finite() && ask.is_finite() {
        ((ask - bid) / pip).max(0.0)
    } else {
        0.0
    }
}

/// Whether the value moved by `pct` percent of what it was, within `window_ms`. `history` holds
/// the values seen, as (time, value), oldest first, the newest being `now`.
pub fn moved(history: &[(i64, f64)], window_ms: i64, pct: f64) -> bool {
    let Some(&(now_at, now)) = history.last() else {
        return false;
    };
    if pct <= 0.0 || !now.is_finite() {
        return false;
    }
    history
        .iter()
        .filter(|(at, _)| now_at - at <= window_ms)
        .any(|&(_, then)| {
            then.is_finite() && then != 0.0 && ((now - then) / then).abs() * 100.0 >= pct
        })
}

/// Whether a series that went from `a` to `b` and now stands at `c` turned around: it was rising
/// and now falls, or the other way. A flat step is no direction.
pub fn direction_changed(a: f64, b: f64, c: f64) -> bool {
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        return false;
    }
    let (first, second) = (b - a, c - b);
    (first > 0.0 && second < 0.0) || (first < 0.0 && second > 0.0)
}

/// What judging an alert on its bars found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Judged {
    /// The condition came true and the trigger lets it fire.
    pub fired: bool,
    /// The bar (its start time) the reading is from: what a trigger by bar remembers.
    pub bar: i64,
    /// The reading of that bar, for the next judgment of a bar that is still forming.
    pub reading: Reading,
}

/// Judges an alert on the bars of its timeframe, oldest first: `times` are the start times of the
/// bars, `value` and `level` what the alert watches and what it is compared with at each of them.
///
/// A condition that is judged on closed bars (a bar close, a change of direction) looks at the
/// last bar that has closed and the one before it, and fires only for a bar that closed after the
/// alert was made and that it has not fired on. Otherwise it looks at the bar still forming, against
/// `last`, the reading of the judgment before (so nothing fires the first time it looks).
#[allow(clippy::too_many_arguments)]
pub fn judge_bars(
    condition: Condition,
    trigger: Trigger,
    times: &[i64],
    value: &[f64],
    level: &[f64],
    upper: Option<f64>,
    created_at: i64,
    bar_key: Option<i64>,
    last: Option<Reading>,
) -> Option<Judged> {
    let n = times.len();
    if n < 4 || value.len() != n || level.len() != n {
        return None;
    }
    let closed = trigger == Trigger::OncePerBarClose || condition == Condition::ChangesDirection;
    let at = if closed { n - 2 } else { n - 1 };
    let reading = |i: usize| Reading {
        value: value[i],
        level: level[i],
        upper,
    };
    let now = reading(at);
    let bar = times[at];
    let mut hit = if condition == Condition::ChangesDirection {
        direction_changed(value[at - 2], value[at - 1], value[at])
    } else {
        let before = if closed { Some(reading(at - 1)) } else { last };
        before.is_some_and(|before| fired(condition, &before, &now))
    };
    if closed {
        // The bar closed when the next one opened.
        hit &= bar_key != Some(bar) && times[at + 1] >= created_at;
    } else if trigger == Trigger::OncePerBar {
        hit &= bar_key != Some(bar);
    }
    Some(Judged {
        fired: hit,
        bar,
        reading: now,
    })
}

/// Where a drawing is at a time: the price of a line, or the two prices of a zone (lowest side
/// first). `None` when the drawing has nothing at that time (a segment that ended) or is a tool
/// with no level to watch.
pub fn drawing_level(tool: Tool, points: &[Point], time: i64) -> Option<(f64, Option<f64>)> {
    let (a, b) = (points.first()?, points.get(1));
    match tool {
        Tool::HorizontalLine => Some((a.p, None)),
        Tool::HorizontalRay => (time >= a.t).then_some((a.p, None)),
        Tool::TrendLine | Tool::Ray | Tool::ExtendedLine => {
            let b = b?;
            if a.t == b.t {
                return None;
            }
            let (first, second) = if a.t <= b.t { (a, b) } else { (b, a) };
            let inside = match tool {
                Tool::TrendLine => time >= first.t && time <= second.t,
                Tool::Ray => time >= a.t,
                _ => true,
            };
            let slope = (second.p - first.p) / (second.t - first.t) as f64;
            inside.then(|| (first.p + slope * (time - first.t) as f64, None))
        }
        Tool::Rectangle => {
            let b = b?;
            let (start, end) = (a.t.min(b.t), a.t.max(b.t));
            (time >= start && time <= end).then(|| (a.p.min(b.p), Some(a.p.max(b.p))))
        }
        _ => None,
    }
}

/// Whether a tool is a zone (two prices) rather than a line.
pub fn is_zone_tool(tool: Tool) -> bool {
    matches!(tool, Tool::Rectangle)
}

/// Whether an alert can be made on this tool.
pub fn is_alert_tool(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::HorizontalLine
            | Tool::HorizontalRay
            | Tool::TrendLine
            | Tool::Ray
            | Tool::ExtendedLine
            | Tool::Rectangle
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spread_is_counted_in_pips_and_never_negative() {
        assert!((spread_pips(1.10000, 1.10015, 0.0001) - 1.5).abs() < 1e-9);
        assert!((spread_pips(151.200, 151.213, 0.01) - 1.3).abs() < 1e-9);
        assert_eq!(spread_pips(1.1002, 1.1000, 0.0001), 0.0);
        assert_eq!(spread_pips(1.1, 1.2, 0.0), 0.0);
        assert_eq!(spread_pips(f64::NAN, 1.2, 0.0001), 0.0);
    }

    #[test]
    fn a_spread_that_widens_or_a_loss_that_deepens_fires_once() {
        // The same rules as a price: 1.8 pips to 2.6 pips crosses a level of 2 going up.
        assert!(fired(Condition::Above, &r(1.8, 2.0), &r(2.6, 2.0)));
        assert!(!fired(Condition::Above, &r(2.6, 2.0), &r(2.9, 2.0)));
        // A profit going from -40 to -120 falls below a level of -100.
        assert!(fired(
            Condition::Below,
            &r(-40.0, -100.0),
            &r(-120.0, -100.0)
        ));
        // And a level of 0 is the break of a gain into a loss.
        assert!(fired(Condition::CrossingDown, &r(3.0, 0.0), &r(-1.0, 0.0)));
    }

    fn r(value: f64, level: f64) -> Reading {
        Reading {
            value,
            level,
            upper: None,
        }
    }

    fn zone(value: f64) -> Reading {
        Reading {
            value,
            level: 1.0,
            upper: Some(2.0),
        }
    }

    #[test]
    fn crossings_follow_a_level_that_moves() {
        assert!(fired(Condition::CrossingUp, &r(1.0, 1.5), &r(2.0, 1.5)));
        assert!(!fired(Condition::CrossingUp, &r(2.0, 1.5), &r(1.0, 1.5)));
        assert!(fired(Condition::Crossing, &r(2.0, 1.5), &r(1.0, 1.5)));
        // The price did not move, the level did: an average that rose over the price.
        assert!(fired(Condition::CrossingDown, &r(1.5, 1.4), &r(1.5, 1.6)));
        // Touching counts as a crossing, leaving does not.
        assert!(fired(Condition::Crossing, &r(1.0, 1.5), &r(1.5, 1.5)));
        assert!(!fired(Condition::Crossing, &r(1.5, 1.5), &r(1.6, 1.5)));
    }

    #[test]
    fn above_and_below_fire_once_when_the_state_starts() {
        assert!(fired(Condition::Above, &r(1.0, 1.5), &r(1.6, 1.5)));
        assert!(!fired(Condition::Above, &r(1.6, 1.5), &r(1.7, 1.5)));
        assert!(fired(Condition::Below, &r(1.6, 1.5), &r(1.4, 1.5)));
        assert!(!fired(Condition::Below, &r(1.4, 1.5), &r(1.3, 1.5)));
    }

    #[test]
    fn a_zone_is_entered_and_left() {
        assert!(fired(Condition::EntersZone, &zone(0.5), &zone(1.5)));
        assert!(!fired(Condition::EntersZone, &zone(1.5), &zone(1.6)));
        assert!(fired(Condition::ExitsZone, &zone(1.5), &zone(2.5)));
        assert!(fired(Condition::ExitsZone, &zone(1.5), &zone(0.5)));
        // A gap across the whole zone is neither.
        assert!(!fired(Condition::EntersZone, &zone(0.5), &zone(2.5)));
        // The sides can be given either way round.
        let flipped = |value| Reading {
            value,
            level: 2.0,
            upper: Some(1.0),
        };
        assert!(fired(Condition::EntersZone, &flipped(0.5), &flipped(1.5)));
        // No zone, no zone condition.
        assert!(!fired(Condition::EntersZone, &r(0.5, 1.0), &r(1.5, 1.0)));
    }

    #[test]
    fn values_that_are_not_numbers_never_fire() {
        assert!(!fired(Condition::Crossing, &r(f64::NAN, 1.0), &r(2.0, 1.0)));
        assert!(!fired(Condition::Above, &r(0.0, 1.0), &r(2.0, f64::NAN)));
    }

    #[test]
    fn a_move_is_a_share_of_the_value_within_the_window() {
        let minute = 60_000;
        let history = [(0, 100.0), (5 * minute, 100.2), (10 * minute, 100.6)];
        assert!(moved(&history, 15 * minute, 0.5));
        assert!(!moved(&history, 15 * minute, 0.7));
        // The old value fell out of the window: only 0.4 of a move is left.
        assert!(!moved(&history, 5 * minute, 0.5));
        // Falling counts as well.
        assert!(moved(&[(0, 100.0), (minute, 99.0)], 5 * minute, 1.0));
        assert!(!moved(&[], minute, 1.0));
    }

    #[test]
    fn a_turn_needs_a_rise_then_a_fall_or_the_reverse() {
        assert!(direction_changed(1.0, 2.0, 1.5));
        assert!(direction_changed(2.0, 1.0, 1.5));
        assert!(!direction_changed(1.0, 2.0, 3.0));
        assert!(
            !direction_changed(1.0, 1.0, 2.0),
            "a flat step has no direction"
        );
        assert!(!direction_changed(1.0, f64::NAN, 2.0));
    }

    fn p(t: i64, price: f64) -> Point {
        Point { t, p: price }
    }

    #[test]
    fn a_line_has_a_level_at_a_time_as_far_as_it_reaches() {
        let points = [p(0, 1.0), p(100, 2.0)];
        let level = |tool, t| drawing_level(tool, &points, t).map(|(l, _)| l);
        assert_eq!(level(Tool::TrendLine, 50), Some(1.5));
        assert_eq!(level(Tool::TrendLine, 150), None, "a segment ends");
        assert_eq!(level(Tool::Ray, 200), Some(3.0));
        assert_eq!(level(Tool::Ray, -10), None, "a ray starts");
        assert_eq!(level(Tool::ExtendedLine, -100), Some(0.0));
        assert_eq!(level(Tool::HorizontalLine, 5_000), Some(1.0));
        assert_eq!(level(Tool::HorizontalRay, -1), None);
        assert_eq!(level(Tool::Text, 5), None);
        // Two points at one time make no line.
        assert_eq!(
            drawing_level(Tool::TrendLine, &[p(5, 1.0), p(5, 2.0)], 5),
            None
        );
    }

    #[test]
    fn a_rectangle_is_a_zone_while_it_lasts() {
        let points = [p(0, 2.0), p(100, 1.0)];
        assert_eq!(
            drawing_level(Tool::Rectangle, &points, 50),
            Some((1.0, Some(2.0)))
        );
        assert_eq!(drawing_level(Tool::Rectangle, &points, 101), None);
        assert!(is_zone_tool(Tool::Rectangle) && !is_zone_tool(Tool::TrendLine));
        assert!(is_alert_tool(Tool::Ray) && !is_alert_tool(Tool::Brush));
    }

    #[test]
    fn a_bar_close_is_judged_on_closed_bars_once_and_only_after_the_alert_was_made() {
        let times = [0, 60, 120, 180, 240];
        // The price crosses the level of 10 on the bar that opened at 120 and closed at 180.
        let value = [8.0, 9.0, 11.0, 12.0, 12.5];
        let level = [10.0; 5];
        let judge = |created, key| {
            judge_bars(
                Condition::CrossingUp,
                Trigger::OncePerBarClose,
                &times,
                &value,
                &level,
                None,
                created,
                key,
                None,
            )
            .unwrap()
        };
        // The last closed bar is the one at 180 (value 12), the one before is 11: no crossing.
        assert!(!judge(0, None).fired);
        // Look one bar earlier: the same series cut short.
        let cut = judge_bars(
            Condition::CrossingUp,
            Trigger::OncePerBarClose,
            &times[..4],
            &value[..4],
            &level[..4],
            None,
            0,
            None,
            None,
        )
        .unwrap();
        assert!(cut.fired && cut.bar == 120);
        // Already fired on that bar, or made after the bar closed: it stays quiet.
        let again = judge_bars(
            Condition::CrossingUp,
            Trigger::OncePerBarClose,
            &times[..4],
            &value[..4],
            &level[..4],
            None,
            0,
            Some(120),
            None,
        )
        .unwrap();
        assert!(!again.fired);
        let late = judge_bars(
            Condition::CrossingUp,
            Trigger::OncePerBarClose,
            &times[..4],
            &value[..4],
            &level[..4],
            None,
            500,
            None,
            None,
        )
        .unwrap();
        assert!(!late.fired);
    }

    #[test]
    fn a_forming_bar_is_judged_against_the_last_reading() {
        let times = [0, 60, 120, 180];
        let level = [10.0; 4];
        let judge = |value: [f64; 4], last, trigger, key| {
            judge_bars(
                Condition::CrossingUp,
                trigger,
                &times,
                &value,
                &level,
                None,
                0,
                key,
                last,
            )
            .unwrap()
        };
        let quiet = judge([8.0, 8.0, 9.0, 9.5], None, Trigger::EveryTime, None);
        assert!(!quiet.fired, "the first look only seeds");
        let up = judge(
            [8.0, 8.0, 9.0, 10.5],
            Some(quiet.reading),
            Trigger::EveryTime,
            None,
        );
        assert!(up.fired);
        // Once per bar: not again on a bar it fired on.
        let same_bar = judge(
            [8.0, 8.0, 9.0, 10.5],
            Some(quiet.reading),
            Trigger::OncePerBar,
            Some(180),
        );
        assert!(!same_bar.fired);
        // Too few bars to judge.
        assert!(
            judge_bars(
                Condition::Crossing,
                Trigger::Once,
                &times[..3],
                &[1.0; 3],
                &[1.0; 3],
                None,
                0,
                None,
                None
            )
            .is_none()
        );
    }

    #[test]
    fn a_change_of_direction_is_read_on_closed_bars() {
        let times = [0, 60, 120, 180, 240];
        // Closed bars 60, 120, 180: 2, 3, 2.5 turns down.
        let value = [1.0, 2.0, 3.0, 2.5, 2.6];
        let judged = judge_bars(
            Condition::ChangesDirection,
            Trigger::Once,
            &times,
            &value,
            &[0.0; 5],
            None,
            0,
            None,
            None,
        )
        .unwrap();
        assert!(judged.fired && judged.bar == 180);
    }
}
