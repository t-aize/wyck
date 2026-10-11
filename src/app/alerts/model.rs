//! What an alert is, as data: what it watches (a price, an indicator), what it compares that with
//! (a level, a drawing, another indicator), the condition that fires it, how often it may fire,
//! and when it stops. Saved per account, and old files (a price and a crossing) still load.

use crate::domain::indicators::StudyConfig;
use serde::{Deserialize, Serialize};

use crate::app::workspace::MAX_SAVED_ALERTS as MAX_ALERTS;

pub const SCHEMA_VERSION: u32 = 2;
/// The firings kept for the history.
pub const MAX_HISTORY: usize = 200;

/// Which price of the market a price alert follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceKind {
    #[default]
    Bid,
    Ask,
    Mid,
}

impl PriceKind {
    pub const ALL: [Self; 3] = [Self::Bid, Self::Ask, Self::Mid];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bid => "Bid",
            Self::Ask => "Ask",
            Self::Mid => "Mid",
        }
    }

    /// The price of this kind in a quote.
    pub fn of(self, bid: Option<f64>, ask: Option<f64>) -> Option<f64> {
        match self {
            Self::Bid => bid,
            Self::Ask => ask,
            Self::Mid => bid.zip(ask).map(|(b, a)| (b + a) / 2.0),
        }
    }
}

/// Something that has a value at every moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// The price of the symbol.
    Price {
        #[serde(default)]
        price: PriceKind,
    },
    /// One plot of an indicator, worked out on the bars of the alert's timeframe.
    Indicator {
        study: Box<StudyConfig>,
        #[serde(default)]
        plot: usize,
    },
    /// A drawing of the symbol: the level of a line at the time, or the two sides of a zone.
    Drawing { id: u64 },
    /// The spread of the symbol (ask minus bid), in pips. `pip` is the size of one pip in price,
    /// kept with the alert because the alerts do not read the contracts of the symbols.
    Spread {
        #[serde(default = "default_pip")]
        pip: f64,
    },
    /// What the account, a symbol or a position makes or loses, in the money of the account.
    Pnl { scope: PnlScope },
}

/// Which profit a profit alert follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum PnlScope {
    /// Everything open on the account.
    Account,
    /// The positions of the alert's symbol.
    Symbol,
    /// One position.
    Position { id: i64 },
}

/// The size of a pip of a forex pair with five decimals.
fn default_pip() -> f64 {
    0.0001
}

impl Default for Source {
    fn default() -> Self {
        Self::Price {
            price: PriceKind::Bid,
        }
    }
}

impl Source {
    pub fn is_indicator(&self) -> bool {
        matches!(self, Self::Indicator { .. })
    }

    /// The name shown for it: `Bid`, `RSI (plot 1)`, `Drawing 12`.
    pub fn label(&self) -> String {
        match self {
            Self::Price { price } => price.label().to_owned(),
            Self::Indicator { study, plot } => {
                let spec = study.kind.spec();
                let plot_name = spec.plots.get(*plot).map_or("", |p| p.label);
                if spec.plots.len() > 1 && !plot_name.is_empty() {
                    format!("{} {}", spec.short, plot_name)
                } else {
                    spec.short.to_owned()
                }
            }
            Self::Drawing { id } => format!("drawing {id}"),
            Self::Spread { .. } => "spread".to_owned(),
            Self::Pnl { scope } => match scope {
                PnlScope::Account => "account profit".to_owned(),
                PnlScope::Symbol => "profit".to_owned(),
                PnlScope::Position { id } => format!("profit of position {id}"),
            },
        }
    }

    /// Whether the value is a plain number of the alert's own (a spread, a profit), not a price
    /// or the plot of an indicator: no drawing, no other value to compare with.
    pub fn is_measure(&self) -> bool {
        matches!(self, Self::Spread { .. } | Self::Pnl { .. })
    }

    /// Whether the alert belongs to a symbol. The profit of the whole account does not.
    pub fn has_symbol(&self) -> bool {
        !matches!(
            self,
            Self::Pnl {
                scope: PnlScope::Account
            }
        )
    }

    /// The decimals to write a value of this source with: `digits` for a price.
    pub fn decimals(&self, digits: u32) -> u32 {
        match self {
            Self::Spread { .. } => 1,
            Self::Pnl { .. } => 2,
            _ => digits,
        }
    }

    /// Whether a condition makes sense for the value: a profit has no direction to change, a
    /// number that is not a price does not move by a share of itself.
    pub fn allows(&self, condition: Condition) -> bool {
        match self {
            Self::Spread { .. } => condition != Condition::ChangesDirection,
            Self::Pnl { .. } => {
                !matches!(condition, Condition::ChangesDirection | Condition::MovesBy)
            }
            _ => true,
        }
    }
}

/// What has to happen to a value for the alert to fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// The value goes from under the level to over it.
    CrossingUp,
    /// The value goes from over the level to under it.
    CrossingDown,
    #[default]
    Crossing,
    /// The value becomes greater than the level.
    Above,
    /// The value becomes smaller than the level.
    Below,
    /// The value gets inside the zone between the level and the upper level.
    EntersZone,
    /// The value leaves the zone.
    ExitsZone,
    /// The value moves by a share of itself within some minutes.
    MovesBy,
    /// The value, which was rising, starts to fall, or the other way round.
    ChangesDirection,
}

impl Condition {
    pub const ALL: [Self; 9] = [
        Self::Crossing,
        Self::CrossingUp,
        Self::CrossingDown,
        Self::Above,
        Self::Below,
        Self::EntersZone,
        Self::ExitsZone,
        Self::MovesBy,
        Self::ChangesDirection,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Crossing => "Crossing",
            Self::CrossingUp => "Crossing up",
            Self::CrossingDown => "Crossing down",
            Self::Above => "Becomes greater than",
            Self::Below => "Becomes smaller than",
            Self::EntersZone => "Enters the zone",
            Self::ExitsZone => "Leaves the zone",
            Self::MovesBy => "Moves by",
            Self::ChangesDirection => "Changes direction",
        }
    }

    /// Whether it needs a second level: the other side of a zone.
    pub fn is_zone(self) -> bool {
        matches!(self, Self::EntersZone | Self::ExitsZone)
    }

    /// Whether it compares the value with a level at all.
    pub fn needs_level(self) -> bool {
        !matches!(self, Self::MovesBy | Self::ChangesDirection)
    }

    /// Whether a move from `before` to `now` crosses `price` the way this condition asks. Only
    /// for the crossings.
    #[cfg(test)]
    pub fn crossed(self, before: f64, now: f64, price: f64) -> bool {
        let up = before < price && now >= price;
        let down = before > price && now <= price;
        match self {
            Self::CrossingUp => up,
            Self::CrossingDown => down,
            Self::Crossing => up || down,
            _ => false,
        }
    }
}

/// How often an alert may fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Once, then it stops.
    #[default]
    Once,
    /// Every time the condition comes true.
    EveryTime,
    /// At most once for each bar of the timeframe.
    OncePerBar,
    /// Only when a bar closes: the condition is judged on the closed bars, once each.
    OncePerBarClose,
}

impl Trigger {
    pub const ALL: [Self; 4] = [
        Self::Once,
        Self::EveryTime,
        Self::OncePerBar,
        Self::OncePerBarClose,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Once => "Only once",
            Self::EveryTime => "Every time",
            Self::OncePerBar => "Once per bar",
            Self::OncePerBarClose => "Once per bar close",
        }
    }
}

/// One time an alert fired, for the history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Firing {
    pub alert: u64,
    pub symbol: String,
    /// When, in Unix milliseconds.
    pub at: i64,
    /// What it said.
    pub text: String,
    /// The symbol, for showing its chart. 0 in a history written before this field existed.
    #[serde(default)]
    pub symbol_id: i64,
    /// What the alert watched (`Price`, an indicator name, ...) and its condition, as they read
    /// when it fired: the alert may have been edited or deleted since.
    #[serde(default)]
    pub watched: String,
    #[serde(default)]
    pub condition: String,
    /// The value that made it fire, and it written the way the alert writes it (a price with the
    /// decimals of the symbol, a spread in pips, a profit in money).
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub value_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: u64,
    pub symbol_id: i64,
    /// The broker's name of the symbol, to show and to find it again.
    pub symbol: String,
    /// What it watches.
    #[serde(default)]
    pub source: Source,
    /// The level, in the units of the source (1.0850, or 70 for an RSI). Also the lower side of a
    /// zone. Not used when the alert compares with something else ([`Alert::versus`]).
    pub price: f64,
    /// The upper side of a zone.
    #[serde(default)]
    pub upper: f64,
    /// What it is compared with, when it is not the level.
    #[serde(default)]
    pub versus: Option<Source>,
    #[serde(default)]
    pub condition: Condition,
    #[serde(default)]
    pub trigger: Trigger,
    /// For a move: the share, in percent, and the minutes it may take.
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub minutes: u32,
    /// The timeframe (by code) of the bars an indicator or a bar close is judged on.
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
    /// What to say when it fires; the default text when empty. `{symbol}`, `{value}` and `{level}`
    /// are replaced.
    #[serde(default)]
    pub message: String,
    /// A word to group alerts by.
    #[serde(default)]
    pub tag: String,
    /// The notice stays until it is closed.
    #[serde(default)]
    pub sticky: bool,
    /// The sound of this alert, when it is not the one every alert makes.
    #[serde(default)]
    pub sound: Option<super::sound::SoundKind>,
    /// Only for files that came before triggers: it repeats.
    #[serde(default, skip_serializing)]
    pub repeat: bool,
    /// Whether it is watching.
    #[serde(default = "crate::domain::chart::defaults::yes")]
    pub active: bool,
    /// When it last fired, in Unix milliseconds, and how many times.
    #[serde(default)]
    pub fired_at: Option<i64>,
    #[serde(default)]
    pub fired_count: u32,
    #[serde(default)]
    pub created_at: i64,
    /// When it stops watching by itself.
    #[serde(default)]
    pub expires_at: Option<i64>,
    /// It does not fire until this time.
    #[serde(default)]
    pub snoozed_until: Option<i64>,
    /// The bar (its start time) it last fired on, for the triggers by bar.
    #[serde(default)]
    pub bar_key: Option<i64>,
}

fn default_timeframe() -> String {
    "M15".to_owned()
}

impl Alert {
    /// A price alert: the bid crosses `price`.
    pub fn price(id: u64, symbol_id: i64, symbol: &str, price: f64, now_ms: i64) -> Self {
        Self {
            id,
            symbol_id,
            symbol: symbol.to_owned(),
            source: Source::default(),
            price,
            upper: 0.0,
            versus: None,
            condition: Condition::Crossing,
            trigger: Trigger::Once,
            amount: 0.0,
            minutes: 0,
            timeframe: default_timeframe(),
            message: String::new(),
            tag: String::new(),
            sticky: false,
            sound: None,
            repeat: false,
            active: true,
            fired_at: None,
            fired_count: 0,
            created_at: now_ms,
            expires_at: None,
            snoozed_until: None,
            bar_key: None,
        }
    }

    /// An alert on the spread of a symbol: it fires when the spread, in pips, meets the condition
    /// with `level`. `pip` is the size of a pip in price.
    pub fn spread(
        id: u64,
        symbol_id: i64,
        symbol: &str,
        pip: f64,
        level: f64,
        now_ms: i64,
    ) -> Self {
        let mut alert = Self::price(id, symbol_id, symbol, level, now_ms);
        alert.source = Source::Spread { pip };
        alert.condition = Condition::Above;
        alert.trigger = Trigger::OncePerBar;
        alert
    }

    /// An alert on a profit: it fires when the profit of `scope`, in the money of the account,
    /// meets the condition with `level`. `symbol_id` is 0 for the whole account.
    pub fn pnl(
        id: u64,
        symbol_id: i64,
        symbol: &str,
        scope: PnlScope,
        level: f64,
        now_ms: i64,
    ) -> Self {
        let mut alert = Self::price(id, symbol_id, symbol, level, now_ms);
        alert.source = Source::Pnl { scope };
        alert.condition = Condition::Below;
        alert
    }

    /// Whether it repeats after it fires.
    pub fn repeats(&self) -> bool {
        self.trigger != Trigger::Once
    }

    pub fn is_expired(&self, now_ms: i64) -> bool {
        self.expires_at.is_some_and(|at| now_ms >= at)
    }

    pub fn is_snoozed(&self, now_ms: i64) -> bool {
        self.snoozed_until.is_some_and(|at| now_ms < at)
    }

    /// Whether it is judged on bars, not on the ticks of the price: an indicator, a bar close, a
    /// change of direction.
    pub fn needs_bars(&self) -> bool {
        self.source.is_indicator()
            || self.versus.as_ref().is_some_and(Source::is_indicator)
            || self.trigger == Trigger::OncePerBarClose
            || self.condition == Condition::ChangesDirection
    }

    /// The drawing it compares with, if any.
    pub fn drawing(&self) -> Option<u64> {
        match (&self.versus, &self.source) {
            (Some(Source::Drawing { id }), _) | (None, Source::Drawing { id }) => Some(*id),
            _ => None,
        }
    }

    /// What it watches and waits for, in a line: `EURUSD crossing up 1.08500`.
    pub fn describe(&self, digits: u32) -> String {
        let level = match &self.versus {
            Some(other) => other.label(),
            None if self.condition.is_zone() => {
                format!(
                    "{:.*} to {:.*}",
                    digits as usize, self.price, digits as usize, self.upper
                )
            }
            None => self.level_text(digits),
        };
        let what = match &self.source {
            Source::Price { .. } => self.symbol.clone(),
            other if !other.has_symbol() => other.label(),
            other => format!("{} {}", self.symbol, other.label()),
        };
        let unit = match &self.source {
            Source::Spread { .. } if self.versus.is_none() => " pips",
            _ => "",
        };
        match self.condition {
            Condition::MovesBy => format!(
                "{what} moves by {}% in {} min",
                trim_number(self.amount),
                self.minutes
            ),
            Condition::ChangesDirection => format!("{what} changes direction"),
            condition => format!("{what} {} {level}{unit}", condition.label().to_lowercase()),
        }
    }

    /// The level as text: as many decimals as the symbol has for a price, fewer for an indicator.
    pub fn level_text(&self, digits: u32) -> String {
        match &self.source {
            Source::Indicator { study, .. }
                if !matches!(
                    study.kind.spec().format,
                    crate::domain::indicators::ValueFormat::Price
                ) =>
            {
                trim_number(self.price)
            }
            Source::Spread { .. } | Source::Pnl { .. } => {
                let decimals = self.source.decimals(digits) as usize;
                format!("{:.*}", decimals, self.price)
            }
            _ => format!("{:.*}", digits as usize, self.price),
        }
    }

    /// A value of what the alert watches, written the way the alert writes its level.
    pub fn value_text(&self, value: f64, digits: u32) -> String {
        format!("{value:.*}", self.source.decimals(digits) as usize)
    }

    /// What the user is told when it fires, without a value.
    #[cfg(test)]
    pub fn text(&self, digits: u32) -> String {
        self.render(None, digits)
    }

    /// The message with `{symbol}`, `{value}` and `{level}` replaced.
    pub fn render(&self, value: Option<f64>, digits: u32) -> String {
        if self.message.trim().is_empty() {
            return self.describe(digits);
        }
        let value_text = value.map_or_else(String::new, |v| self.value_text(v, digits));
        self.message
            .replace("{symbol}", &self.symbol)
            .replace("{value}", &value_text)
            .replace("{level}", &self.level_text(digits))
    }
}

/// A number without trailing zeros: `70`, `0.5`.
fn trim_number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertBook {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default = "first_id")]
    pub next_id: u64,
    #[serde(default)]
    pub alerts: Vec<Alert>,
    /// The latest firings, newest first.
    #[serde(default)]
    pub history: Vec<Firing>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn first_id() -> u64 {
    1
}

impl Default for AlertBook {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_id: 1,
            alerts: Vec::new(),
            history: Vec::new(),
        }
    }
}

impl AlertBook {
    /// The book repaired: prices that are not prices dropped, ids made unique, alerts of the old
    /// files that repeat given the trigger that says so.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        self.alerts.retain(|a| {
            (a.symbol_id > 0 || !a.source.has_symbol())
                && if a.condition.needs_level() && a.versus.is_none() {
                    // A profit can be a loss, and a level of 0 means something for it.
                    a.price.is_finite()
                        && (a.price > 0.0 || a.source.is_indicator() || a.source.is_measure())
                } else {
                    true
                }
        });
        self.alerts.truncate(MAX_ALERTS);
        for alert in &mut self.alerts {
            if alert.source.is_measure() {
                // Nothing else to compare a spread or a profit with, and no bars to judge.
                alert.versus = None;
                if !alert.source.allows(alert.condition) {
                    alert.condition = Condition::Crossing;
                }
                if alert.trigger == Trigger::OncePerBarClose {
                    alert.trigger = Trigger::EveryTime;
                }
            }
            if let Source::Spread { pip } = &mut alert.source
                && !(pip.is_finite() && *pip > 0.0)
            {
                *pip = default_pip();
            }
            if alert.repeat && alert.trigger == Trigger::Once {
                alert.trigger = Trigger::EveryTime;
            }
            alert.repeat = false;
            if !alert.upper.is_finite() {
                alert.upper = 0.0;
            }
            if !alert.amount.is_finite() || alert.amount < 0.0 {
                alert.amount = 0.0;
            }
            alert.minutes = alert.minutes.min(24 * 60 * 30);
            if crate::domain::chart::Timeframe::from_code(&alert.timeframe).is_none() {
                alert.timeframe = default_timeframe();
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_id.max(1);
        for alert in &self.alerts {
            next = next.max(alert.id + 1);
        }
        for alert in &mut self.alerts {
            if alert.id == 0 || !seen.insert(alert.id) {
                alert.id = next;
                next += 1;
            }
        }
        self.next_id = next;
        self.history.truncate(MAX_HISTORY);
        self
    }

    #[cfg(test)]
    pub fn add(
        &mut self,
        symbol_id: i64,
        symbol: &str,
        price: f64,
        condition: Condition,
        now_ms: i64,
    ) -> Option<u64> {
        self.add_with_limit(
            symbol_id,
            symbol,
            price,
            condition,
            now_ms,
            crate::app::workspace::DEFAULT_SAVED_ALERTS,
        )
    }

    pub fn add_with_limit(
        &mut self,
        symbol_id: i64,
        symbol: &str,
        price: f64,
        condition: Condition,
        now_ms: i64,
        limit: usize,
    ) -> Option<u64> {
        if !(price.is_finite() && price > 0.0) {
            return None;
        }
        let mut alert = Alert::price(0, symbol_id, symbol, price, now_ms);
        alert.condition = condition;
        self.insert(alert, limit)
    }

    /// Adds an alert (its id is given here), unless the limit is reached.
    pub fn insert(&mut self, mut alert: Alert, limit: usize) -> Option<u64> {
        if self.alerts.len() >= limit.clamp(1, MAX_ALERTS) {
            return None;
        }
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        alert.id = id;
        self.alerts.push(alert);
        Some(id)
    }

    pub fn get(&self, id: u64) -> Option<&Alert> {
        self.alerts.iter().find(|a| a.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Alert> {
        self.alerts.iter_mut().find(|a| a.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.alerts.len();
        self.alerts.retain(|a| a.id != id);
        self.alerts.len() != before
    }

    /// The symbols with an active alert.
    pub fn watched(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .alerts
            .iter()
            // A profit is read from the account, which follows the prices it needs itself.
            .filter(|a| a.active && a.symbol_id > 0 && !matches!(a.source, Source::Pnl { .. }))
            .map(|a| a.symbol_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Records that an alert fired: when, how many times, and in the history. A one-time alert
    /// stops watching. Returns the alert as it is now.
    pub fn fire(
        &mut self,
        id: u64,
        text: String,
        value: Option<f64>,
        digits: u32,
        now_ms: i64,
    ) -> Option<Alert> {
        let alert = self.get_mut(id)?;
        alert.fired_at = Some(now_ms);
        alert.fired_count = alert.fired_count.saturating_add(1);
        if alert.trigger == Trigger::Once {
            alert.active = false;
        }
        let firing = Firing {
            alert: id,
            symbol: alert.symbol.clone(),
            at: now_ms,
            text,
            symbol_id: alert.symbol_id,
            watched: alert.source.label(),
            condition: alert.condition.label().to_owned(),
            value,
            value_text: value.map_or_else(String::new, |v| alert.value_text(v, digits)),
        };
        let alert = alert.clone();
        self.history.insert(0, firing);
        self.history.truncate(MAX_HISTORY);
        Some(alert)
    }

    /// Turns off the alerts whose time is up. Returns their ids.
    pub fn expire(&mut self, now_ms: i64) -> Vec<u64> {
        let mut gone = Vec::new();
        for alert in &mut self.alerts {
            if alert.active && alert.is_expired(now_ms) {
                alert.active = false;
                gone.push(alert.id);
            }
        }
        gone
    }

    /// The firings of one alert, newest first.
    pub fn firings(&self, id: u64) -> impl Iterator<Item = &Firing> {
        self.history.iter().filter(move |f| f.alert == id)
    }
}

#[cfg(test)]
mod tests;
