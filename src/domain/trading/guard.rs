//! Trading safety: the limits the user sets and the checks every new order goes through before
//! it is sent.
//!
//! The rules are plain data and pure functions, so they are tested without a window or a broker.
//! [`crate::app::account::Account::place`] is the one funnel for new orders (the ticket, one-click, the
//! chart and a reversal all end there), so the checks cannot be skipped by another way in. An
//! order that only reduces what is open is never blocked: closing must always work.
//!
//! Some limits are hard and refuse the order ([`Verdict::Block`]); the others only warn
//! ([`Verdict::Warn`]) and can be sent anyway from the confirmation.

use serde::{Deserialize, Serialize};

/// The most a limit can be set to, so a typo cannot turn a limit into no limit.
const HUGE: f64 = 1e9;

/// What the user set to keep an account safe. A number limit of 0 is off.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskPrefs {
    /// The most lots one order may have.
    #[serde(default)]
    pub max_lots_per_order: f64,
    /// The most lots that may be open on one symbol, counting the new order.
    #[serde(default)]
    pub max_lots_per_symbol: f64,
    /// The most positions open at once.
    #[serde(default)]
    pub max_open_positions: u32,
    /// The most the account may lose in a day, in its money (realized plus open).
    #[serde(default)]
    pub max_daily_loss: f64,
    /// The same, as a share of the balance the day started with, in percent.
    #[serde(default)]
    pub max_daily_loss_pct: f64,
    /// The most orders that may be sent in a day.
    #[serde(default)]
    pub max_daily_trades: u32,
    /// A spread wider than this, in pips, warns.
    #[serde(default)]
    pub max_spread_pips: f64,
    /// A pending order priced further than this from the market, in percent, is refused: a
    /// misplaced decimal point.
    #[serde(default = "default_collar")]
    pub price_collar_pct: f64,
    /// An order with no stop loss is refused.
    #[serde(default)]
    pub require_stop_loss: bool,
    /// After a losing trade closes, no new order for this many minutes.
    #[serde(default)]
    pub cooldown_minutes: u32,
    /// The most lots an order may have when sent by one click.
    #[serde(default)]
    pub one_click_max_lots: f64,
    /// Nothing new can be sent while this is on.
    #[serde(default)]
    pub kill_switch: bool,
}

fn default_collar() -> f64 {
    10.0
}

impl Default for RiskPrefs {
    fn default() -> Self {
        Self {
            max_lots_per_order: 0.0,
            max_lots_per_symbol: 0.0,
            max_open_positions: 0,
            max_daily_loss: 0.0,
            max_daily_loss_pct: 0.0,
            max_daily_trades: 0,
            max_spread_pips: 0.0,
            price_collar_pct: default_collar(),
            require_stop_loss: false,
            cooldown_minutes: 0,
            one_click_max_lots: 0.0,
            kill_switch: false,
        }
    }
}

impl RiskPrefs {
    /// The limits repaired: numbers that are not numbers, or negative, are off.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let clean = |v: f64| {
            if v.is_finite() && v > 0.0 {
                v.min(HUGE)
            } else {
                0.0
            }
        };
        self.max_lots_per_order = clean(self.max_lots_per_order);
        self.max_lots_per_symbol = clean(self.max_lots_per_symbol);
        self.max_daily_loss = clean(self.max_daily_loss);
        self.max_daily_loss_pct = clean(self.max_daily_loss_pct).min(100.0);
        self.max_spread_pips = clean(self.max_spread_pips);
        self.price_collar_pct = clean(self.price_collar_pct).min(1_000.0);
        self.one_click_max_lots = clean(self.one_click_max_lots);
        self.max_open_positions = self.max_open_positions.min(10_000);
        self.max_daily_trades = self.max_daily_trades.min(100_000);
        self.cooldown_minutes = self.cooldown_minutes.min(24 * 60);
        self
    }

    /// The loss that locks the account today, given the balance the day started with: the lower
    /// of the two limits that are set.
    pub fn daily_loss_limit(&self, day_start_balance: f64) -> Option<f64> {
        let by_share = (self.max_daily_loss_pct > 0.0 && day_start_balance > 0.0)
            .then(|| day_start_balance * self.max_daily_loss_pct / 100.0);
        let by_amount = (self.max_daily_loss > 0.0).then_some(self.max_daily_loss);
        match (by_share, by_amount) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Whether any limit is on.
    #[cfg(test)]
    pub fn any(&self) -> bool {
        self.kill_switch
            || self.max_lots_per_order > 0.0
            || self.max_lots_per_symbol > 0.0
            || self.max_open_positions > 0
            || self.max_daily_loss > 0.0
            || self.max_daily_loss_pct > 0.0
            || self.max_daily_trades > 0
            || self.max_spread_pips > 0.0
            || self.price_collar_pct > 0.0
            || self.require_stop_loss
            || self.cooldown_minutes > 0
            || self.one_click_max_lots > 0.0
    }
}

/// Where the account stands today, worked out by the account from what it holds and its deals.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Standing {
    /// Positions open now.
    pub open_positions: usize,
    /// Orders sent today.
    pub trades_today: u32,
    /// What the day made or lost so far: closed trades after costs plus what is open. Negative
    /// is a loss.
    pub day_pnl: f64,
    /// The balance the day started with (now, less what closed trades made today).
    pub day_start_balance: f64,
    /// When the last losing trade closed, in Unix milliseconds.
    pub last_loss_at: Option<i64>,
    /// Now, in Unix milliseconds.
    pub now: i64,
}

/// Why nothing new can be sent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lock {
    KillSwitch,
    /// The day's loss reached its limit.
    DailyLoss {
        loss: f64,
        limit: f64,
    },
    /// Waiting after a loss, until this time.
    Cooldown {
        until: i64,
    },
}

impl Lock {
    /// The reason, as a sentence for a banner. `money` writes an amount with its currency.
    pub fn text(&self, money: &dyn Fn(f64) -> String, now: i64) -> String {
        match *self {
            Self::KillSwitch => "Kill switch is on: no new orders can be sent.".to_owned(),
            Self::DailyLoss { loss, limit } => format!(
                "Daily loss limit reached ({} of {}): no new orders today.",
                money(loss),
                money(limit)
            ),
            Self::Cooldown { until } => {
                let left = ((until - now).max(0) + 59_999) / 60_000;
                format!("Cooling down after a loss: {left} min left.")
            }
        }
    }
}

/// The lock in force, if any. The kill switch and the daily loss come first: they last longer
/// than a cooldown.
pub fn lock(prefs: &RiskPrefs, standing: &Standing) -> Option<Lock> {
    if prefs.kill_switch {
        return Some(Lock::KillSwitch);
    }
    if let Some(limit) = prefs.daily_loss_limit(standing.day_start_balance) {
        let loss = -standing.day_pnl;
        if loss >= limit {
            return Some(Lock::DailyLoss { loss, limit });
        }
    }
    if prefs.cooldown_minutes > 0
        && let Some(at) = standing.last_loss_at
    {
        let until = at + i64::from(prefs.cooldown_minutes) * 60_000;
        if standing.now < until {
            return Some(Lock::Cooldown { until });
        }
    }
    None
}

/// A new order, as far as the checks are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OrderFacts {
    pub lots: f64,
    /// The lots already open on this symbol.
    pub symbol_lots: f64,
    /// A stop loss or take profit is set, and a stop loss in particular.
    pub has_stop: bool,
    /// How far a pending order is priced from the market, in percent.
    pub away_pct: Option<f64>,
    /// The spread of the symbol now, in pips.
    pub spread_pips: Option<f64>,
    /// Sent without a confirmation.
    pub one_click: bool,
    /// The order only closes or reduces what is open.
    pub reduces: bool,
}

/// What the checks say about an order.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok,
    /// Refuse it, with the reason.
    Block(String),
    /// Ask first: these things are worth a second look.
    Warn(Vec<String>),
}

impl Verdict {
    #[cfg(test)]
    pub fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }
}

/// Checks an order against the limits and where the account stands.
pub fn check(
    prefs: &RiskPrefs,
    standing: &Standing,
    order: &OrderFacts,
    money: &dyn Fn(f64) -> String,
) -> Verdict {
    if order.reduces {
        return Verdict::Ok;
    }
    if let Some(lock) = lock(prefs, standing) {
        return Verdict::Block(lock.text(money, standing.now));
    }
    if prefs.max_lots_per_order > 0.0 && order.lots > prefs.max_lots_per_order + 1e-9 {
        return Verdict::Block(format!(
            "The order is {} lots: the limit per order is {}.",
            lots(order.lots),
            lots(prefs.max_lots_per_order)
        ));
    }
    if order.one_click
        && prefs.one_click_max_lots > 0.0
        && order.lots > prefs.one_click_max_lots + 1e-9
    {
        return Verdict::Block(format!(
            "One-click orders are limited to {} lots. Use the confirmation for a larger one.",
            lots(prefs.one_click_max_lots)
        ));
    }
    if prefs.max_lots_per_symbol > 0.0
        && order.symbol_lots + order.lots > prefs.max_lots_per_symbol + 1e-9
    {
        return Verdict::Block(format!(
            "That would make {} lots open on this symbol: the limit is {}.",
            lots(order.symbol_lots + order.lots),
            lots(prefs.max_lots_per_symbol)
        ));
    }
    if prefs.max_open_positions > 0 && standing.open_positions >= prefs.max_open_positions as usize
    {
        return Verdict::Block(format!(
            "{} positions are open: the limit is {}.",
            standing.open_positions, prefs.max_open_positions
        ));
    }
    if prefs.max_daily_trades > 0 && standing.trades_today >= prefs.max_daily_trades {
        return Verdict::Block(format!(
            "{} orders were sent today: the limit is {}.",
            standing.trades_today, prefs.max_daily_trades
        ));
    }
    if prefs.require_stop_loss && !order.has_stop {
        return Verdict::Block("A stop loss is required by your safety settings.".to_owned());
    }
    if prefs.price_collar_pct > 0.0
        && let Some(away) = order.away_pct
        && away > prefs.price_collar_pct
    {
        return Verdict::Block(format!(
            "The price is {away:.1}% from the market: the limit is {:.1}%. Check the decimal point.",
            prefs.price_collar_pct
        ));
    }
    let mut warnings = Vec::new();
    if prefs.max_spread_pips > 0.0
        && let Some(spread) = order.spread_pips
        && spread > prefs.max_spread_pips
    {
        warnings.push(format!(
            "The spread is {spread:.1} pips, wider than your limit of {:.1}.",
            prefs.max_spread_pips
        ));
    }
    if warnings.is_empty() {
        Verdict::Ok
    } else {
        Verdict::Warn(warnings)
    }
}

fn lots(value: f64) -> String {
    super::math::format_lots(value)
}

/// Whether two orders are the same one sent twice: the same symbol, side, volume, kind and price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    pub symbol: i64,
    pub buy: bool,
    pub volume: i64,
    pub kind: i64,
    /// The price in points, 0 for a market order.
    pub price: i64,
}

/// How long an identical order is taken for a double click.
pub const DUPLICATE_WINDOW_MS: i64 = 2_000;

/// Remembers the last order sent, to drop an identical one sent again at once.
#[derive(Debug, Default)]
pub struct DuplicateGuard {
    last: Option<(Fingerprint, i64)>,
}

impl DuplicateGuard {
    /// Whether `order` at `now` repeats the last one. When it does not, it becomes the last.
    pub fn is_repeat(&mut self, order: Fingerprint, now: i64) -> bool {
        if let Some((last, at)) = self.last
            && last == order
            && now - at < DUPLICATE_WINDOW_MS
        {
            return true;
        }
        self.last = Some((order, now));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(v: f64) -> String {
        format!("{v:.2}")
    }

    fn standing() -> Standing {
        Standing {
            day_start_balance: 10_000.0,
            now: 1_000_000,
            ..Standing::default()
        }
    }

    fn order(lots: f64) -> OrderFacts {
        OrderFacts {
            lots,
            has_stop: true,
            ..OrderFacts::default()
        }
    }

    #[test]
    fn with_no_limit_set_an_order_goes_through() {
        let prefs = RiskPrefs::default();
        assert_eq!(
            check(&prefs, &standing(), &order(50.0), &money),
            Verdict::Ok
        );
        assert!(
            !RiskPrefs {
                price_collar_pct: 0.0,
                ..RiskPrefs::default()
            }
            .any()
        );
        assert!(prefs.any(), "the price collar is on by default");
    }

    #[test]
    fn a_size_limit_blocks_only_what_is_over_it() {
        let prefs = RiskPrefs {
            max_lots_per_order: 2.0,
            ..RiskPrefs::default()
        };
        assert_eq!(check(&prefs, &standing(), &order(2.0), &money), Verdict::Ok);
        assert!(check(&prefs, &standing(), &order(2.01), &money).is_block());
        let prefs = RiskPrefs {
            max_lots_per_symbol: 3.0,
            ..RiskPrefs::default()
        };
        let held = OrderFacts {
            symbol_lots: 2.5,
            ..order(1.0)
        };
        assert!(check(&prefs, &standing(), &held, &money).is_block());
    }

    #[test]
    fn the_daily_loss_locks_at_the_lower_of_its_two_limits() {
        let prefs = RiskPrefs {
            max_daily_loss: 500.0,
            max_daily_loss_pct: 3.0,
            ..RiskPrefs::default()
        };
        assert_eq!(prefs.daily_loss_limit(10_000.0), Some(300.0));
        assert_eq!(prefs.daily_loss_limit(0.0), Some(500.0));
        let mut day = standing();
        day.day_pnl = -299.0;
        assert_eq!(lock(&prefs, &day), None);
        day.day_pnl = -300.0;
        assert!(matches!(lock(&prefs, &day), Some(Lock::DailyLoss { .. })));
        assert!(check(&prefs, &day, &order(0.1), &money).is_block());
    }

    #[test]
    fn closing_is_never_blocked() {
        let prefs = RiskPrefs {
            kill_switch: true,
            ..RiskPrefs::default()
        };
        let closing = OrderFacts {
            reduces: true,
            ..order(5.0)
        };
        assert_eq!(check(&prefs, &standing(), &closing, &money), Verdict::Ok);
        assert!(check(&prefs, &standing(), &order(0.1), &money).is_block());
    }

    #[test]
    fn a_cooldown_runs_out() {
        let prefs = RiskPrefs {
            cooldown_minutes: 30,
            ..RiskPrefs::default()
        };
        let mut day = standing();
        day.last_loss_at = Some(day.now - 10 * 60_000);
        assert!(matches!(
            lock(&prefs, &day),
            Some(Lock::Cooldown { until }) if until == day.now + 20 * 60_000
        ));
        day.last_loss_at = Some(day.now - 31 * 60_000);
        assert_eq!(lock(&prefs, &day), None);
        let text = Lock::Cooldown {
            until: day.now + 90_000,
        }
        .text(&money, day.now);
        assert!(text.contains("2 min"), "{text}");
    }

    #[test]
    fn counts_stop_price_and_one_click_limits() {
        let prefs = RiskPrefs {
            max_open_positions: 2,
            max_daily_trades: 5,
            require_stop_loss: true,
            one_click_max_lots: 1.0,
            ..RiskPrefs::default()
        };
        let mut day = standing();
        day.open_positions = 2;
        assert!(check(&prefs, &day, &order(0.1), &money).is_block());
        day.open_positions = 0;
        day.trades_today = 5;
        assert!(check(&prefs, &day, &order(0.1), &money).is_block());
        day.trades_today = 0;
        let bare = OrderFacts {
            has_stop: false,
            ..order(0.1)
        };
        assert!(check(&prefs, &day, &bare, &money).is_block());
        let click = OrderFacts {
            one_click: true,
            ..order(1.5)
        };
        assert!(check(&prefs, &day, &click, &money).is_block());
        assert_eq!(check(&prefs, &day, &order(1.5), &money), Verdict::Ok);
        let far = OrderFacts {
            away_pct: Some(40.0),
            ..order(0.1)
        };
        assert!(
            check(&prefs, &day, &far, &money).is_block(),
            "default collar"
        );
    }

    #[test]
    fn a_wide_spread_only_warns() {
        let prefs = RiskPrefs {
            max_spread_pips: 2.0,
            ..RiskPrefs::default()
        };
        let wide = OrderFacts {
            spread_pips: Some(3.5),
            ..order(0.1)
        };
        assert!(matches!(
            check(&prefs, &standing(), &wide, &money),
            Verdict::Warn(w) if w.len() == 1
        ));
    }

    #[test]
    fn a_repeat_within_the_window_is_dropped() {
        let a = Fingerprint {
            symbol: 1,
            buy: true,
            volume: 100_000,
            kind: 1,
            price: 0,
        };
        let mut guard = DuplicateGuard::default();
        assert!(!guard.is_repeat(a, 10_000));
        assert!(guard.is_repeat(a, 10_500));
        assert!(!guard.is_repeat(Fingerprint { buy: false, ..a }, 10_600));
        assert!(!guard.is_repeat(a, 10_600 + DUPLICATE_WINDOW_MS));
    }

    #[test]
    fn bad_numbers_turn_a_limit_off() {
        let prefs = RiskPrefs {
            max_lots_per_order: f64::NAN,
            max_daily_loss: -5.0,
            max_daily_loss_pct: 900.0,
            ..RiskPrefs::default()
        }
        .normalized();
        assert_eq!(prefs.max_lots_per_order, 0.0);
        assert_eq!(prefs.max_daily_loss, 0.0);
        assert_eq!(prefs.max_daily_loss_pct, 100.0);
    }

    #[test]
    fn an_old_file_gets_the_defaults() {
        let prefs: RiskPrefs = toml::from_str("kill_switch = true").unwrap();
        assert!(prefs.kill_switch);
        assert_eq!(prefs.price_collar_pct, 10.0);
    }
}
