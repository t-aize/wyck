//! Keeping the stop loss of a pending order on the ATR while it waits.
//!
//! A pending order with an ATR stop gets its stop from the ATR the moment it is sent. The ATR keeps
//! moving while the order waits, so the stop is stale by the time the order fills. An order that
//! carries this rule (in its label, see [`super::plan::Label`]) has its stop, and its take profit
//! when that is a multiple of the risk, worked out again from the ATR of the moment. The broker
//! knows nothing of it: the app does it while it runs, and the order stays protected by the last
//! stop sent when the app is closed.
//!
//! This file is plain data and arithmetic. Where the ATR comes from, and the amending, are in
//! `app::account::follow`.

use crate::atr_stop::{AtrStop, MAX_LENGTH, MAX_MULTIPLIER, Smoothing};
use wyck_market::Timeframe;

/// The largest multiplier the label can hold, in hundredths.
const MAX_MULTIPLIER_HUNDREDTHS: u32 = (MAX_MULTIPLIER * 100.0) as u32;

/// The rule an order carries.
#[derive(Debug, Clone, PartialEq)]
pub struct AtrFollow {
    /// How the ATR is made. Its timeframe is the one the ATR is read on: an order that follows the
    /// ATR of "the chart" cannot be followed once the chart is gone, so the ticket writes the real
    /// one.
    pub atr: AtrStop,
    /// The take profit as a multiple of the risk, when the order has one that is.
    pub rr: Option<f64>,
}

/// Where the protection of the order should be now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Levels {
    pub stop: f64,
    pub target: Option<f64>,
}

fn letter(smoothing: Smoothing) -> char {
    match smoothing {
        Smoothing::Rma => 'r',
        Smoothing::Sma => 's',
        Smoothing::Ema => 'e',
        Smoothing::Wma => 'w',
    }
}

fn from_letter(letter: &str) -> Option<Smoothing> {
    match letter {
        "r" => Some(Smoothing::Rma),
        "s" => Some(Smoothing::Sma),
        "e" => Some(Smoothing::Ema),
        "w" => Some(Smoothing::Wma),
        _ => None,
    }
}

impl AtrFollow {
    /// The rule as a part of a label, after the `a`: `14,r,200,H1,0` and `,r150` when the take
    /// profit is a multiple of the risk. Numbers are written in hundredths so no sign but a comma
    /// is used.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut text = format!(
            "{},{},{},{},{}",
            self.atr.length,
            letter(self.atr.smoothing),
            (self.atr.multiplier * 100.0).round() as u32,
            self.atr.timeframe.as_deref().unwrap_or("-"),
            u8::from(self.atr.current_bar),
        );
        if let Some(rr) = self.rr {
            text.push_str(&format!(",r{}", (rr * 100.0).round() as u32));
        }
        text
    }

    /// Reads what [`Self::encode`] wrote; `None` for anything else.
    #[must_use]
    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.split(',');
        let length: usize = parts.next()?.parse().ok()?;
        let smoothing = from_letter(parts.next()?)?;
        let hundredths: u32 = parts.next()?.parse().ok()?;
        let timeframe = match parts.next()? {
            "-" => None,
            code => Some(Timeframe::from_code(code)?.code()),
        };
        let current_bar = match parts.next()? {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        let rr = match parts.next() {
            None => None,
            Some(rule) => {
                let hundredths: u32 = rule.strip_prefix('r')?.parse().ok()?;
                if hundredths == 0 {
                    return None;
                }
                Some(f64::from(hundredths) / 100.0)
            }
        };
        if parts.next().is_some()
            || !(1..=MAX_LENGTH).contains(&length)
            || !(1..=MAX_MULTIPLIER_HUNDREDTHS).contains(&hundredths)
        {
            return None;
        }
        Some(Self {
            atr: AtrStop {
                length,
                smoothing,
                multiplier: f64::from(hundredths) / 100.0,
                timeframe,
                current_bar,
            },
            rr,
        })
    }

    /// The stop loss, and the take profit when it follows the risk, for an order that enters at
    /// `entry` when the ATR is `atr`. `None` when the numbers make no sense.
    #[must_use]
    pub fn levels(&self, entry: f64, buy: bool, atr: f64) -> Option<Levels> {
        let stop = self.atr.stop(entry, buy, atr)?;
        let risk = (entry - stop).abs();
        let target = self
            .rr
            .map(|rr| entry + if buy { rr * risk } else { -rr * risk });
        Some(Levels { stop, target })
    }
}

/// Whether a level of the order is far enough from where it should be to be amended: an amend
/// for a hair of a pip would only fill the broker's log.
#[must_use]
pub(crate) fn needs_move(current: Option<f64>, wanted: f64, tolerance: f64) -> bool {
    current.is_none_or(|c| (c - wanted).abs() >= tolerance)
}

/// Whether the stop on the order is where this rule last put it. If it is not, the user moved it
/// by hand and the rule leaves it alone from then on.
#[must_use]
pub(crate) fn is_untouched(current: Option<f64>, last_set: f64, tolerance: f64) -> bool {
    current.is_some_and(|c| (c - last_set).abs() <= tolerance)
}

/// A working order a rule is kept on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tracked {
    pub buy: bool,
    /// Where it enters: its limit price, or its stop price for a stop or stop limit order.
    pub entry: f64,
    pub stop: f64,
    pub target: Option<f64>,
}

/// What to do with an order at this moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Nothing: the levels are where they should be, or the ATR cannot be used.
    Keep,
    /// The user moved the stop: stop following the order.
    Release,
    /// Amend the order to these levels.
    Move { stop: f64, target: Option<f64> },
}

/// Decides what the rule does with `order` when the ATR is `atr`. `last_set` is the stop the rule
/// put on the order last, if it did, `tolerance` how far a level may be from where it should be
/// and `round` rounds a price the way the symbol quotes it.
#[must_use]
pub fn next_step(
    rule: &AtrFollow,
    order: &Tracked,
    atr: f64,
    last_set: Option<f64>,
    tolerance: f64,
    round: impl Fn(f64) -> f64,
) -> Step {
    if let Some(set) = last_set
        && !is_untouched(Some(order.stop), set, tolerance)
    {
        return Step::Release;
    }
    let Some(levels) = rule.levels(order.entry, order.buy, atr) else {
        return Step::Keep;
    };
    let (stop, target) = (round(levels.stop), levels.target.map(&round));
    let moves = needs_move(Some(order.stop), stop, tolerance)
        || target.is_some_and(|t| needs_move(order.target, t, tolerance));
    if moves {
        Step::Move {
            stop,
            target: target.or(order.target),
        }
    } else {
        Step::Keep
    }
}

/// Whether bars loaded `age_ms` ago, when the last bar started at `loaded_ms`, are recent enough
/// to work the ATR out of at `now_ms`. An ATR that reads the bar being formed changes all the time;
/// one that reads closed bars changes when a bar closes.
#[must_use]
pub fn bars_are_fresh(
    timeframe: Timeframe,
    current_bar: bool,
    loaded_ms: i64,
    now_ms: i64,
) -> bool {
    let age = now_ms.saturating_sub(loaded_ms);
    if current_bar {
        age < 30_000
    } else {
        timeframe.bar_key(loaded_ms) == timeframe.bar_key(now_ms) && age < 300_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(rr: Option<f64>) -> AtrFollow {
        AtrFollow {
            atr: AtrStop {
                length: 14,
                smoothing: Smoothing::Ema,
                multiplier: 2.5,
                timeframe: Some("H1".to_owned()),
                current_bar: true,
            },
            rr,
        }
    }

    #[test]
    fn a_rule_reads_back_as_it_was_written() {
        for rr in [None, Some(1.5), Some(3.0)] {
            let rule = rule(rr);
            assert_eq!(AtrFollow::decode(&rule.encode()), Some(rule));
        }
        assert_eq!(rule(None).encode(), "14,e,250,H1,1");
        assert_eq!(rule(Some(1.5)).encode(), "14,e,250,H1,1,r150");
    }

    #[test]
    fn a_rule_without_a_timeframe_keeps_none() {
        let mut open = rule(None);
        open.atr.timeframe = None;
        assert_eq!(open.encode(), "14,e,250,-,1");
        assert_eq!(AtrFollow::decode("14,e,250,-,1"), Some(open));
    }

    #[test]
    fn what_is_not_a_rule_is_refused() {
        for bad in [
            "",
            "14,e,250,H1",
            "0,e,250,H1,1",
            "1001,e,250,H1,1",
            "14,x,250,H1,1",
            "14,e,0,H1,1",
            "14,e,250,nonsense,1",
            "14,e,250,H1,2",
            "14,e,250,H1,1,150",
            "14,e,250,H1,1,r0",
            "14,e,250,H1,1,r150,extra",
        ] {
            assert_eq!(AtrFollow::decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_stop_sits_the_atr_times_the_multiplier_away_on_the_losing_side() {
        let rule = rule(None);
        let buy = rule.levels(1.1000, true, 0.0010).unwrap();
        assert!((buy.stop - 1.0975).abs() < 1e-9);
        assert_eq!(buy.target, None);
        let sell = rule.levels(1.1000, false, 0.0010).unwrap();
        assert!((sell.stop - 1.1025).abs() < 1e-9);
    }

    #[test]
    fn a_take_profit_in_multiples_of_the_risk_moves_with_the_stop() {
        let rule = rule(Some(2.0));
        let buy = rule.levels(1.1000, true, 0.0010).unwrap();
        assert!((buy.target.unwrap() - 1.1050).abs() < 1e-9);
        let wider = rule.levels(1.1000, true, 0.0020).unwrap();
        assert!((wider.target.unwrap() - 1.1100).abs() < 1e-9);
        let sell = rule.levels(1.1000, false, 0.0010).unwrap();
        assert!((sell.target.unwrap() - 1.0950).abs() < 1e-9);
    }

    #[test]
    fn no_atr_or_a_zero_one_gives_no_levels() {
        assert_eq!(rule(None).levels(1.1, true, 0.0), None);
        assert_eq!(rule(None).levels(1.1, true, f64::NAN), None);
        assert_eq!(rule(None).levels(f64::NAN, true, 0.001), None);
    }

    #[test]
    fn a_level_moves_only_when_it_is_off_by_the_tolerance_or_more() {
        assert!(needs_move(None, 1.0, 0.0001));
        assert!(!needs_move(Some(1.00005), 1.0, 0.0001));
        assert!(needs_move(Some(1.0002), 1.0, 0.0001));
    }

    #[test]
    fn a_stop_the_user_moved_is_not_the_one_the_rule_set() {
        assert!(is_untouched(Some(1.0), 1.0, 0.0001));
        assert!(is_untouched(Some(1.00005), 1.0, 0.0001));
        assert!(!is_untouched(Some(1.01), 1.0, 0.0001));
        assert!(!is_untouched(None, 1.0, 0.0001));
    }

    fn tracked(stop: f64) -> Tracked {
        Tracked {
            buy: true,
            entry: 1.1000,
            stop,
            target: Some(1.1050),
        }
    }

    #[test]
    fn an_order_whose_stop_is_where_the_atr_puts_it_is_left_alone() {
        let step = next_step(&rule(None), &tracked(1.0975), 0.0010, None, 0.00005, |p| p);
        assert_eq!(step, Step::Keep);
    }

    #[test]
    fn a_stale_stop_is_moved_and_the_take_profit_stays_when_it_is_not_a_multiple_of_the_risk() {
        let step = next_step(&rule(None), &tracked(1.0975), 0.0020, None, 0.00005, |p| p);
        match step {
            Step::Move { stop, target } => {
                assert!((stop - 1.0950).abs() < 1e-9);
                assert_eq!(target, Some(1.1050));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_take_profit_in_multiples_of_the_risk_moves_with_the_stop_and_is_rounded() {
        let step = next_step(
            &rule(Some(2.0)),
            &tracked(1.0975),
            0.0020,
            None,
            0.00005,
            |p| (p * 1e5).round() / 1e5,
        );
        assert_eq!(
            step,
            Step::Move {
                stop: 1.0950,
                target: Some(1.1100)
            }
        );
    }

    #[test]
    fn a_stop_the_user_moved_since_the_rule_set_it_releases_the_order() {
        let step = next_step(
            &rule(None),
            &tracked(1.0900),
            0.0020,
            Some(1.0950),
            0.00005,
            |p| p,
        );
        assert_eq!(step, Step::Release);
        let still = next_step(
            &rule(None),
            &tracked(1.0950),
            0.0030,
            Some(1.0950),
            0.00005,
            |p| p,
        );
        assert!(matches!(still, Step::Move { .. }));
    }

    #[test]
    fn an_atr_that_cannot_be_used_changes_nothing() {
        let step = next_step(
            &rule(None),
            &tracked(1.0975),
            f64::NAN,
            None,
            0.00005,
            |p| p,
        );
        assert_eq!(step, Step::Keep);
    }

    #[test]
    fn bars_are_fresh_for_half_a_minute_when_they_include_the_open_bar_and_for_the_bar_otherwise() {
        let h1 = Timeframe::from_code("H1").unwrap();
        assert!(bars_are_fresh(h1, true, 1_000_000, 1_020_000));
        assert!(!bars_are_fresh(h1, true, 1_000_000, 1_040_000));
        // Closed bars do not change until the next one closes, so a few minutes are fine.
        assert!(bars_are_fresh(h1, false, 1_000_000, 1_200_000));
        assert!(!bars_are_fresh(h1, false, 1_000_000, 1_400_000));
    }
}
