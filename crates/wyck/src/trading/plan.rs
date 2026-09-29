//! Trade plans: an order cut into several exits, with a break-even and an OCO pair.
//!
//! The broker has no such orders, so the app builds them from plain ones. A plan with three exits
//! sends three orders, each with its own stop loss and take profit, which the broker keeps and
//! runs on its own: if the app is closed, every leg stays protected. What the app adds while it
//! runs is what the broker cannot do:
//!
//! - **Break-even**: when the leg named in the plan closes in profit, the stop loss of the legs
//!   still open moves to their entry price (plus or minus a few pips).
//! - **OCO**: two pending orders of one pair; when one fills, the other is cancelled.
//!
//! Nothing here is kept in a file. Every order carries a label that says which plan it belongs to,
//! which leg it is and what the break-even rule is (see [`Label`]), so after a restart the account
//! finds its plans again by reading the labels of what is open.
//!
//! This module is plain data and arithmetic, tested without a window or a broker.

use serde::{Deserialize, Serialize};

/// The most exits one plan has.
pub const MAX_LEGS: usize = 3;

/// The start of every label the app writes.
const PREFIX: &str = "wyck:";

/// One exit of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Leg {
    /// The share of the volume, in percent.
    pub share: f64,
    /// The take profit, in multiples of the risk (R). 0 leaves the leg with no take profit: it
    /// runs until the stop loss, the trailing stop or the user closes it.
    pub target_r: f64,
}

/// Moving the stop loss to the entry once a leg has closed in profit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BreakEven {
    pub on: bool,
    /// The leg (counted from 1) whose closing in profit triggers it.
    pub after_leg: u8,
    /// How far past the entry the stop goes, in pips: a few pips lock in a small gain.
    pub offset_pips: f64,
}

/// An order cut into exits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExitPlan {
    /// Whether the ticket sends its order as a plan.
    pub on: bool,
    pub legs: Vec<Leg>,
    pub break_even: BreakEven,
    /// The last leg has a trailing stop loss (the broker moves it with the price).
    pub trail_last: bool,
    /// Also place the opposite side of a pending order, this many pips from its price, as an OCO
    /// pair: 0 is off.
    pub oco_pips: f64,
}

impl Default for ExitPlan {
    fn default() -> Self {
        Self {
            on: false,
            legs: vec![
                Leg {
                    share: 50.0,
                    target_r: 1.0,
                },
                Leg {
                    share: 30.0,
                    target_r: 2.0,
                },
                Leg {
                    share: 20.0,
                    target_r: 0.0,
                },
            ],
            break_even: BreakEven {
                on: true,
                after_leg: 1,
                offset_pips: 0.0,
            },
            trail_last: false,
            oco_pips: 0.0,
        }
    }
}

impl ExitPlan {
    /// The plan put back in range: one to [`MAX_LEGS`] legs with positive shares, a break-even
    /// that names a leg that exists.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let clean = |v: f64, most: f64| {
            if v.is_finite() {
                v.clamp(0.0, most)
            } else {
                0.0
            }
        };
        self.legs.truncate(MAX_LEGS);
        for leg in &mut self.legs {
            leg.share = clean(leg.share, 100.0);
            leg.target_r = clean(leg.target_r, 1_000.0);
        }
        if self.legs.is_empty() {
            self.legs = Self::default().legs;
        }
        let last_but_one = self.legs.len().saturating_sub(1).max(1) as u8;
        self.break_even.after_leg = self.break_even.after_leg.clamp(1, last_but_one);
        self.break_even.offset_pips = clean(self.break_even.offset_pips, 10_000.0);
        self.oco_pips = clean(self.oco_pips, 100_000.0);
        self
    }

    /// Why the plan cannot be sent, if it cannot.
    pub fn problem(&self) -> Option<String> {
        if self.legs.iter().any(|l| l.share <= 0.0) {
            return Some("Every exit needs a share above 0".to_owned());
        }
        let total: f64 = self.legs.iter().map(|l| l.share).sum();
        if (total - 100.0).abs() > 0.5 {
            return Some(format!(
                "The shares of the exits add up to {total:.0}%, not 100%"
            ));
        }
        let mut last = 0.0;
        for leg in self.legs.iter().filter(|l| l.target_r > 0.0) {
            if leg.target_r <= last {
                return Some("Each take profit must be further than the one before".to_owned());
            }
            last = leg.target_r;
        }
        None
    }
}

/// Cuts a volume between legs by their shares. The volumes are multiples of `step`; the part that
/// does not divide goes to the first leg. A leg under `min` is dropped and its share goes to a
/// neighbor, so the answer can have fewer legs than shares. It gives the volumes and, for each,
/// the index of the leg (in `shares`) it stands for. The volumes add up to the volume given, cut to
/// a multiple of `step`.
pub fn split_volume(total: i64, shares: &[f64], min: i64, step: i64) -> (Vec<i64>, Vec<usize>) {
    let step = step.max(1);
    let total = total - total % step;
    let mut shares: Vec<f64> = shares.to_vec();
    let mut kept: Vec<usize> = (0..shares.len()).collect();
    loop {
        let sum: f64 = shares.iter().sum();
        if shares.is_empty() || sum <= 0.0 || total <= 0 {
            return (Vec::new(), Vec::new());
        }
        let mut volumes: Vec<i64> = shares
            .iter()
            .map(|s| ((total as f64 * s / sum / step as f64).floor() as i64) * step)
            .collect();
        let rest = total - volumes.iter().sum::<i64>();
        volumes[0] += rest;
        match volumes.iter().position(|v| *v < min) {
            Some(at) if shares.len() > 1 => {
                // The share of the small leg goes to the leg before it (or after, for the first).
                let share = shares.remove(at);
                kept.remove(at);
                shares[at.saturating_sub(1)] += share;
            }
            _ => return (volumes, kept),
        }
    }
}

/// What a label of the app says about an order or a position.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    /// The plan, as a short id shared by its orders.
    pub group: String,
    /// The leg, counted from 1, and how many there are.
    pub leg: u8,
    pub of: u8,
    /// The break-even rule: the leg that triggers it and the offset in tenths of a pip.
    pub break_even: Option<(u8, u32)>,
    /// One order of an OCO pair.
    pub oco: bool,
}

impl Label {
    /// The label as it is sent to the broker.
    pub fn encode(&self) -> String {
        let mut text = format!("{PREFIX}{}:{}/{}", self.group, self.leg, self.of);
        if let Some((after, tenths)) = self.break_even {
            text.push_str(&format!(":b{after},{tenths}"));
        }
        if self.oco {
            text.push_str(":oco");
        }
        text
    }

    /// Reads a label; `None` for one the app did not write in this form (`wyck` alone, or another
    /// program's).
    pub fn decode(text: &str) -> Option<Self> {
        let rest = text.strip_prefix(PREFIX)?;
        let mut parts = rest.split(':');
        let group = parts.next().filter(|g| !g.is_empty())?.to_owned();
        let (leg, of) = parts.next()?.split_once('/')?;
        let (leg, of) = (leg.parse().ok()?, of.parse().ok()?);
        let mut label = Self {
            group,
            leg,
            of,
            break_even: None,
            oco: false,
        };
        for part in parts {
            if part == "oco" {
                label.oco = true;
            } else if let Some(rule) = part.strip_prefix('b') {
                let (after, tenths) = rule.split_once(',')?;
                label.break_even = Some((after.parse().ok()?, tenths.parse().ok()?));
            }
        }
        Some(label)
    }
}

/// A short id for a new plan, from the time: 8 letters and digits.
pub fn new_group(now_ms: i64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = now_ms.unsigned_abs();
    let mut out = Vec::new();
    while n > 0 && out.len() < 8 {
        out.push(DIGITS[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// An open position of a plan, as far as the break-even rule cares.
#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub position: i64,
    pub label: Label,
    pub buy: bool,
    pub entry: f64,
    pub stop_loss: Option<f64>,
}

/// The stop losses to move after the leg `closed` of a plan closed with `profit`: the legs of the
/// same plan that are still open go to their entry, plus `offset` in price on the side of the
/// trade. A stop loss already at or past that level is left alone (the rule only tightens).
/// Returns the position and its new stop loss.
pub fn break_even_moves(closed: &Label, profit: f64, open: &[Open], pip: f64) -> Vec<(i64, f64)> {
    let Some((after, tenths)) = closed.break_even else {
        return Vec::new();
    };
    if profit <= 0.0 || closed.leg != after {
        return Vec::new();
    }
    let offset = f64::from(tenths) / 10.0 * pip;
    open.iter()
        .filter(|o| o.label.group == closed.group && o.label.leg != closed.leg)
        .filter_map(|o| {
            let level = if o.buy {
                o.entry + offset
            } else {
                o.entry - offset
            };
            let tighter = match o.stop_loss {
                None => true,
                Some(sl) if o.buy => sl < level - pip / 100.0,
                Some(sl) => sl > level + pip / 100.0,
            };
            tighter.then_some((o.position, level))
        })
        .collect()
}

/// The working orders to cancel when an order of an OCO pair filled: the orders of the same group,
/// marked as OCO, on the other side. The legs of the side that filled stay: they are its exits.
/// `orders` are working orders as (id, label, is a buy).
pub fn oco_siblings(filled: &Label, filled_buy: bool, orders: &[(i64, Label, bool)]) -> Vec<i64> {
    if !filled.oco {
        return Vec::new();
    }
    orders
        .iter()
        .filter(|(_, l, buy)| l.oco && l.group == filled.group && *buy != filled_buy)
        .map(|(id, _, _)| *id)
        .collect()
}

/// An order of the account's recent history that carries a label of the app.
#[derive(Debug, Clone, PartialEq)]
pub struct Past {
    pub label: Label,
    pub buy: bool,
    /// The order was filled (an OCO order that only expired or was cancelled proves nothing).
    pub filled: bool,
    /// The position the fill opened.
    pub position: Option<i64>,
    /// The size of a pip of its symbol.
    pub pip: f64,
}

/// What the app must still do after being offline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Catch {
    /// Move the stop loss of an open position.
    Stop { position: i64, stop: f64 },
    /// Cancel a working order.
    Cancel { order: i64 },
}

/// The moves the app missed while it was closed or disconnected, from what is open now and the
/// recent orders. A leg of a plan that closed in profit moves the stops of its siblings (see
/// [`break_even_moves`]); a filled OCO order cancels the other side (see [`oco_siblings`]).
///
/// `profits` is what each closed position made, after costs. Doing it twice changes nothing: a
/// stop already at the entry, or an order already gone, gives no move. The one thing it cannot
/// know is a stop the user loosened by hand after a break-even that had run: it would tighten it
/// again once, at the next connection.
pub fn catch_up(
    past: &[Past],
    profits: &std::collections::HashMap<i64, f64>,
    open: &[Open],
    working: &[(i64, Label, bool)],
) -> Vec<Catch> {
    let mut moves: Vec<Catch> = Vec::new();
    for old in past.iter().filter(|p| p.filled) {
        if old.label.oco {
            for order in oco_siblings(&old.label, old.buy, working) {
                let cancel = Catch::Cancel { order };
                if !moves.contains(&cancel) {
                    moves.push(cancel);
                }
            }
        }
        let Some(position) = old.position else {
            continue;
        };
        // A position still open is not a leg that closed.
        if open.iter().any(|o| o.position == position) {
            continue;
        }
        let Some(&profit) = profits.get(&position) else {
            continue;
        };
        for (id, stop) in break_even_moves(&old.label, profit, open, old.pip) {
            let done = moves
                .iter()
                .any(|m| matches!(m, Catch::Stop { position, .. } if *position == id));
            if !done {
                moves.push(Catch::Stop { position: id, stop });
            }
        }
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_volume_is_cut_by_shares_in_whole_steps() {
        // 1 lot = 10_000_000 hundredths of a unit on a forex pair; min 0.01 lot, step 0.01 lot.
        let (min, step) = (100_000, 100_000);
        let (v, kept) = split_volume(10_000_000, &[50.0, 30.0, 20.0], min, step);
        assert_eq!(v, vec![5_000_000, 3_000_000, 2_000_000]);
        assert_eq!(kept, vec![0, 1, 2]);
        // A volume that does not divide: the rest goes to the first leg.
        let (v, _) = split_volume(1_000_000, &[50.0, 30.0, 20.0], min, step);
        assert_eq!(v.iter().sum::<i64>(), 1_000_000);
        assert!(v.iter().all(|x| x % step == 0));
        assert!(v[0] >= v[1]);
    }

    #[test]
    fn legs_too_small_to_trade_are_merged_into_the_one_before() {
        let (min, step) = (100_000, 100_000);
        // 0.02 lot cannot make three legs of 0.01: two legs remain.
        let (v, kept) = split_volume(200_000, &[50.0, 30.0, 20.0], min, step);
        assert_eq!(v.iter().sum::<i64>(), 200_000);
        assert!(v.iter().all(|x| *x >= min));
        assert_eq!(v.len(), kept.len());
        assert!(kept.len() < 3 && kept[0] == 0);
        // One minimum lot cannot be split at all.
        let (v, kept) = split_volume(100_000, &[50.0, 50.0], min, step);
        assert_eq!(v, vec![100_000]);
        assert_eq!(kept.len(), 1);
        // Nothing to split.
        assert!(split_volume(0, &[100.0], min, step).0.is_empty());
    }

    #[test]
    fn a_plan_needs_shares_that_add_up_and_targets_that_climb() {
        let plan = ExitPlan::default();
        assert_eq!(plan.problem(), None);
        let mut bad = plan.clone();
        bad.legs[2].share = 30.0;
        assert!(bad.problem().unwrap().contains("add up"));
        let mut bad = plan.clone();
        bad.legs[1].target_r = 0.5;
        assert!(bad.problem().unwrap().contains("further"));
        let mut zero = plan;
        zero.legs[0].share = 0.0;
        assert!(zero.problem().is_some());
    }

    #[test]
    fn a_wild_plan_is_put_back_in_range() {
        let plan = ExitPlan {
            legs: vec![
                Leg {
                    share: f64::NAN,
                    target_r: -1.0,
                };
                7
            ],
            break_even: BreakEven {
                on: true,
                after_leg: 9,
                offset_pips: -3.0,
            },
            oco_pips: f64::INFINITY,
            ..ExitPlan::default()
        }
        .normalized();
        assert_eq!(plan.legs.len(), MAX_LEGS);
        assert!(
            plan.legs
                .iter()
                .all(|l| l.share == 0.0 && l.target_r == 0.0)
        );
        assert!(plan.break_even.after_leg <= 2);
        assert_eq!(plan.break_even.offset_pips, 0.0);
        assert_eq!(plan.oco_pips, 0.0);
        let empty = ExitPlan {
            legs: Vec::new(),
            ..ExitPlan::default()
        }
        .normalized();
        assert_eq!(empty.legs.len(), 3);
    }

    #[test]
    fn a_label_reads_back_as_it_was_written() {
        let label = Label {
            group: "k3j9x2".into(),
            leg: 2,
            of: 3,
            break_even: Some((1, 25)),
            oco: false,
        };
        let text = label.encode();
        assert_eq!(text, "wyck:k3j9x2:2/3:b1,25");
        assert!(text.len() < 100);
        assert_eq!(Label::decode(&text), Some(label));
        let oco = Label::decode("wyck:ab:1/1:oco").unwrap();
        assert!(oco.oco && oco.break_even.is_none());
        assert_eq!(Label::decode("wyck"), None);
        assert_eq!(Label::decode("other:ab:1/1"), None);
        assert_eq!(Label::decode("wyck:ab:x/1"), None);
        assert_eq!(new_group(1_700_000_000_000).len(), 8);
    }

    fn open(position: i64, leg: u8, buy: bool, entry: f64, sl: Option<f64>) -> Open {
        Open {
            position,
            label: Label {
                group: "g".into(),
                leg,
                of: 3,
                break_even: Some((1, 0)),
                oco: false,
            },
            buy,
            entry,
            stop_loss: sl,
        }
    }

    #[test]
    fn the_first_target_moves_the_other_stops_to_the_entry() {
        let closed = open(1, 1, true, 1.1000, Some(1.0950)).label;
        let legs = [
            open(2, 2, true, 1.1000, Some(1.0950)),
            open(3, 3, true, 1.1000, Some(1.0950)),
            open(9, 2, true, 1.2, None),
        ];
        // A leg of another plan is left alone.
        let mut other = legs[2].clone();
        other.label.group = "elsewhere".into();
        let moves = break_even_moves(
            &closed,
            12.0,
            &[legs[0].clone(), legs[1].clone(), other],
            0.0001,
        );
        assert_eq!(moves, vec![(2, 1.1000), (3, 1.1000)]);
        // A loss, or another leg closing, moves nothing.
        assert!(break_even_moves(&closed, -1.0, &legs, 0.0001).is_empty());
        let mut second = closed.clone();
        second.leg = 2;
        assert!(break_even_moves(&second, 5.0, &legs, 0.0001).is_empty());
    }

    #[test]
    fn break_even_only_tightens_and_respects_the_offset_and_the_side() {
        let mut closed = open(1, 1, false, 1.1000, Some(1.1050)).label;
        closed.break_even = Some((1, 20));
        // A sell: the stop goes below the entry by the offset (2 pips), and only if it was above.
        let moves = break_even_moves(
            &closed,
            3.0,
            &[
                open(2, 2, false, 1.1000, Some(1.1050)),
                open(3, 3, false, 1.1000, Some(1.0990)),
                open(4, 3, false, 1.1000, None),
            ],
            0.0001,
        );
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].0, 2);
        assert!((moves[0].1 - 1.0998).abs() < 1e-9);
        assert_eq!(moves[1].0, 4, "a leg with no stop gets one");
    }

    #[test]
    fn a_filled_order_of_a_pair_cancels_its_sibling_only() {
        let label = |group: &str, oco: bool| Label {
            group: group.into(),
            leg: 1,
            of: 1,
            break_even: None,
            oco,
        };
        let working = [
            (10, label("a", true), false),
            (11, label("a", true), false),
            (12, label("b", true), false),
            (13, label("a", false), false),
            (14, label("a", true), true),
        ];
        // A buy of the pair filled: the sells of the pair go, its own legs and other plans stay.
        assert_eq!(
            oco_siblings(&label("a", true), true, &working),
            vec![10, 11]
        );
        assert!(oco_siblings(&label("a", false), true, &working).is_empty());
    }

    fn past(position: i64, leg: u8, oco: bool, buy: bool, filled: bool) -> Past {
        Past {
            label: Label {
                group: "g".into(),
                leg,
                of: 3,
                break_even: Some((1, 0)),
                oco,
            },
            buy,
            filled,
            position: Some(position),
            pip: 0.0001,
        }
    }

    #[test]
    fn a_break_even_missed_offline_is_caught_up_once() {
        let open = [
            open(2, 2, true, 1.1000, Some(1.0950)),
            open(3, 3, true, 1.1000, Some(1.0950)),
        ];
        let history = [past(1, 1, false, true, true)];
        let profits = std::collections::HashMap::from([(1, 12.0)]);
        let moves = catch_up(&history, &profits, &open, &[]);
        assert_eq!(
            moves,
            vec![
                Catch::Stop {
                    position: 2,
                    stop: 1.1000
                },
                Catch::Stop {
                    position: 3,
                    stop: 1.1000
                },
            ]
        );
        // Once the stops are at the entry, a second pass finds nothing to do.
        let done = [open_at(2, 2, Some(1.1000)), open_at(3, 3, Some(1.1000))];
        assert!(catch_up(&history, &profits, &done, &[]).is_empty());
    }

    fn open_at(position: i64, leg: u8, sl: Option<f64>) -> Open {
        open(position, leg, true, 1.1000, sl)
    }

    #[test]
    fn a_leg_that_lost_or_is_still_open_moves_nothing() {
        let open = [open_at(2, 2, Some(1.0950))];
        let history = [past(1, 1, false, true, true)];
        let loss = std::collections::HashMap::from([(1, -4.0)]);
        assert!(catch_up(&history, &loss, &open, &[]).is_empty());
        // No closing deal known for the position: nothing is assumed.
        assert!(catch_up(&history, &std::collections::HashMap::new(), &open, &[]).is_empty());
        // The leg is still open (only part of it closed): it is not a leg that closed.
        let still = [open_at(1, 1, Some(1.0950)), open_at(2, 2, Some(1.0950))];
        let profit = std::collections::HashMap::from([(1, 9.0)]);
        assert!(catch_up(&history, &profit, &still, &[]).is_empty());
        // An order that never filled proves nothing.
        let unfilled = [past(1, 1, false, true, false)];
        assert!(catch_up(&unfilled, &profit, &open, &[]).is_empty());
    }

    #[test]
    fn an_oco_fill_missed_offline_cancels_the_other_side() {
        let label = |oco: bool| Label {
            group: "g".into(),
            leg: 1,
            of: 1,
            break_even: None,
            oco,
        };
        let working = [(10, label(true), false), (11, label(true), true)];
        let mut filled = past(5, 1, true, true, true);
        filled.label = label(true);
        let moves = catch_up(
            &[filled.clone(), filled],
            &std::collections::HashMap::new(),
            &[],
            &working,
        );
        assert_eq!(
            moves,
            vec![Catch::Cancel { order: 10 }],
            "the same order is cancelled once"
        );
    }
}
