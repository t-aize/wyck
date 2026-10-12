//! The setups of the ticket saved under a name, and the time stop they carry.

use serde::{Deserialize, Serialize};

use super::Span;
use crate::domain::trading::math::{Offset, SizeMode};
use crate::domain::trading::plan::{ExitPlan, MAX_TIME_STOP_MINUTES, Only, TimeStop};

/// Closing a position once it has been open for a while, as set on the ticket.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeStopPrefs {
    pub on: bool,
    /// How long, in the unit of `span`.
    pub amount: f64,
    pub span: Span,
    pub only: Only,
}

impl Default for TimeStopPrefs {
    fn default() -> Self {
        Self {
            on: false,
            amount: 4.0,
            span: Span::Hours,
            only: Only::Always,
        }
    }
}

impl TimeStopPrefs {
    /// The time in minutes, at least one and at most a year.
    pub fn minutes(&self) -> u32 {
        let minutes = (self.amount * self.span.millis() as f64 / 60_000.0).round();
        minutes.clamp(1.0, f64::from(MAX_TIME_STOP_MINUTES)) as u32
    }

    /// The rule the label of an order carries, when the time stop is on.
    pub fn rule(&self) -> Option<TimeStop> {
        self.on.then(|| TimeStop {
            minutes: self.minutes(),
            only: self.only,
        })
    }

    #[must_use]
    pub fn normalized(mut self) -> Self {
        if !(self.amount.is_finite() && self.amount > 0.0) {
            self.amount = Self::default().amount;
        }
        // Whole numbers of the span: the field takes no decimals.
        let most =
            (f64::from(MAX_TIME_STOP_MINUTES) * 60_000.0 / self.span.millis() as f64).floor();
        self.amount = self.amount.round().clamp(1.0, most.max(1.0));
        self
    }
}

/// The most setups that can be saved under a name.
pub const MAX_PLANS: usize = 30;
/// The longest a name can be, in characters.
pub const MAX_PLAN_NAME: usize = 40;

/// A protection as a plan keeps it: a distance in a unit. A price is not kept, it only holds for
/// one moment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Distance {
    pub unit: Offset,
    pub value: f64,
}

/// A setup of the ticket saved under a name: how the volume is sized, where the stop loss and the
/// take profit go, the exits and the time stop. Applying it fills the ticket; the symbol, the side
/// and the price are still the ones of the moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanTemplate {
    pub name: String,
    pub size_mode: SizeMode,
    pub size: f64,
    /// `None` is off.
    pub stop: Option<Distance>,
    pub target: Option<Distance>,
    pub exits: ExitPlan,
    pub time_stop: TimeStopPrefs,
}

impl Default for PlanTemplate {
    fn default() -> Self {
        Self {
            name: String::new(),
            size_mode: SizeMode::Lots,
            size: 0.1,
            stop: None,
            target: None,
            exits: ExitPlan::default(),
            time_stop: TimeStopPrefs::default(),
        }
    }
}

impl PlanTemplate {
    /// The template put back in range.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.name = clean_name(&self.name);
        if !(self.size.is_finite() && self.size > 0.0) {
            self.size = Self::default().size;
        }
        for distance in [&mut self.stop, &mut self.target] {
            // A price means nothing without the entry it was typed for.
            if distance
                .as_ref()
                .is_some_and(|d| d.unit == Offset::Price || !(d.value.is_finite() && d.value > 0.0))
            {
                *distance = None;
            }
        }
        // A volume sized by the risk cannot depend on a stop loss that depends on the volume.
        if self.size_mode.is_risk()
            && let Some(stop) = &mut self.stop
            && stop.unit.needs_volume()
        {
            stop.unit = Offset::Pips;
        }
        if let Some(stop) = &mut self.stop
            && stop.unit == Offset::Ratio
        {
            stop.unit = Offset::Pips;
        }
        self.exits = self.exits.normalized();
        self.time_stop = self.time_stop.normalized();
        self
    }

    /// The template in a line: `1% risk, stop 20 pips, target 2R, 3 exits, close after 4 h`.
    pub fn summary(&self) -> String {
        let size = match self.size_mode {
            SizeMode::Lots => format!("{} lots", trim(self.size)),
            SizeMode::Units => format!("{} units", trim(self.size)),
            SizeMode::RiskBalance | SizeMode::RiskEquity => format!("{}% risk", trim(self.size)),
            SizeMode::RiskMoney => format!("{} risked", trim(self.size)),
            SizeMode::FreeMargin => format!("{}% of the free margin", trim(self.size)),
        };
        let mut parts = vec![size];
        let unit = |d: &Distance| match d.unit {
            Offset::Pips => format!("{} pips", trim(d.value)),
            Offset::Money => format!("{} in money", trim(d.value)),
            Offset::Percent => format!("{}%", trim(d.value)),
            Offset::Ratio => format!("{}R", trim(d.value)),
            Offset::Price => trim(d.value),
        };
        if let Some(stop) = &self.stop {
            parts.push(format!("stop {}", unit(stop)));
        }
        if let Some(target) = &self.target {
            parts.push(format!("target {}", unit(target)));
        }
        if self.exits.on {
            parts.push(format!("{} exits", self.exits.legs.len()));
        }
        if let Some(rule) = self.time_stop.rule() {
            parts.push(format!("close {}", rule.describe()));
        }
        parts.join(", ")
    }
}

/// A number without trailing zeros: `20`, `1.5`.
fn trim(value: f64) -> String {
    let text = format!("{value:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A name trimmed, on one line and no longer than [`MAX_PLAN_NAME`] characters.
pub fn clean_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_PLAN_NAME)
        .collect::<String>()
        .trim()
        .to_owned()
}
