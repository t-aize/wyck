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

/// Why a set of exits cannot be sent: codes, the sentence belongs to the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ExitsProblem {
    /// An exit takes no share of the volume.
    EmptyShare,
    /// The shares add up to this many percent instead of 100.
    SharesTotal(f64),
    /// A take profit is not further than the one before it.
    TargetsNotRising,
}

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
    pub fn problem(&self) -> Option<ExitsProblem> {
        if self.legs.iter().any(|l| l.share <= 0.0) {
            return Some(ExitsProblem::EmptyShare);
        }
        let total: f64 = self.legs.iter().map(|l| l.share).sum();
        if (total - 100.0).abs() > 0.5 {
            return Some(ExitsProblem::SharesTotal(total));
        }
        let mut last = 0.0;
        for leg in self.legs.iter().filter(|l| l.target_r > 0.0) {
            if leg.target_r <= last {
                return Some(ExitsProblem::TargetsNotRising);
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
    /// Close the position after a time (see [`TimeStop`]).
    pub time_stop: Option<TimeStop>,
}

/// Closing a position once it has been open for a while.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeStop {
    /// How long the position may stay open, in minutes, counted from when it opened.
    pub minutes: u32,
    pub only: Only,
}

/// Which positions a time stop closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Only {
    /// Whatever the position makes.
    #[default]
    Always,
    /// Only one that is in profit; a losing one is left to its stop loss.
    Winning,
    /// Only one that is losing; a winning one is left to run.
    Losing,
}

impl Only {
    pub const ALL: [Self; 3] = [Self::Always, Self::Winning, Self::Losing];

    pub fn label(self) -> &'static str {
        match self {
            Self::Always => "Always",
            Self::Winning => "If in profit",
            Self::Losing => "If in loss",
        }
    }

    /// The letter the label carries for it.
    fn letter(self) -> &'static str {
        match self {
            Self::Always => "",
            Self::Winning => "w",
            Self::Losing => "l",
        }
    }
}

/// The longest a time stop can be: a year.
pub const MAX_TIME_STOP_MINUTES: u32 = 525_600;

impl TimeStop {
    /// Whether the position must be closed now. `opened` is when it opened and `profit` what it
    /// makes now, after costs, when that is known. A rule that depends on the profit waits for it.
    pub fn due(self, opened_ms: i64, now_ms: i64, profit: Option<f64>) -> bool {
        if now_ms < opened_ms.saturating_add(i64::from(self.minutes) * 60_000) {
            return false;
        }
        match (self.only, profit) {
            (Only::Always, _) => true,
            (Only::Winning, Some(p)) => p > 0.0,
            (Only::Losing, Some(p)) => p < 0.0,
            (_, None) => false,
        }
    }

    /// The rule in words: `after 90 min`, `after 4 h if in profit`.
    pub fn describe(self) -> String {
        let after = match self.minutes {
            m if m % 1_440 == 0 => format!("{} d", m / 1_440),
            m if m % 60 == 0 => format!("{} h", m / 60),
            m => format!("{m} min"),
        };
        match self.only {
            Only::Always => format!("after {after}"),
            only => format!("after {after} {}", only.label().to_lowercase()),
        }
    }
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
        if let Some(stop) = self.time_stop {
            text.push_str(&format!(":t{}{}", stop.minutes, stop.only.letter()));
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
            time_stop: None,
        };
        for part in parts {
            if part == "oco" {
                label.oco = true;
            } else if let Some(rule) = part.strip_prefix('t') {
                let (digits, only) = match rule.char_indices().last() {
                    Some((at, 'w')) => (&rule[..at], Only::Winning),
                    Some((at, 'l')) => (&rule[..at], Only::Losing),
                    _ => (rule, Only::Always),
                };
                let minutes: u32 = digits.parse().ok()?;
                if minutes == 0 || minutes > MAX_TIME_STOP_MINUTES {
                    return None;
                }
                label.time_stop = Some(TimeStop { minutes, only });
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
mod tests;
