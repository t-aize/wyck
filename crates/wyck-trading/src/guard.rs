//! Trading safety: the limits the user sets and the checks every new order goes through before
//! it is sent.
//!
//! The rules are plain data and pure functions, so they are tested without a window or a broker.
//! The account service (`Account::place`) is the one funnel for new orders (the ticket, one-click,
//! the chart and a reversal all end there), and the funnel is also enforced by a type: the
//! connection sends a new order only with a [`Cleared`], which [`clear`] and [`clear_batch`] alone
//! make, for the order and the kind of account ([`Mode`]) they were made for. An order that only
//! reduces what is open is never blocked: closing must always work.
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

/// Why nothing new can be sent. These are codes with the numbers behind them: the sentence a
/// person reads belongs to the screen.
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

/// Why the limits refuse an order. These are codes with the numbers behind them: the sentence a
/// person reads belongs to the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {
    /// Nothing new can be sent now.
    Locked(Lock),
    /// More lots than the limit per order.
    OrderTooBig { lots: f64, limit: f64 },
    /// More lots than a one-click order may have.
    OneClickTooBig { limit: f64 },
    /// More lots open on the symbol than its limit, this order included.
    SymbolTooBig { lots: f64, limit: f64 },
    /// As many positions open as the limit allows.
    TooManyPositions { open: usize, limit: u32 },
    /// As many orders sent today as the limit allows.
    TooManyOrdersToday { sent: u32, limit: u32 },
    /// The safety settings ask for a stop loss and the order has none.
    StopLossRequired,
    /// A pending order priced further from the market than the collar allows.
    PriceTooFar { away_pct: f64, limit_pct: f64 },
}

/// Something worth a second look before an order is sent, which does not stop it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Warning {
    /// The spread is wider than the limit.
    SpreadTooWide { spread_pips: f64, limit_pips: f64 },
}

/// What the checks say about an order.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok,
    /// Refuse it, with the reason.
    Block(Refusal),
    /// Ask first: these things are worth a second look.
    Warn(Vec<Warning>),
}

impl Verdict {
    #[cfg(test)]
    pub(crate) fn is_block(&self) -> bool {
        matches!(self, Self::Block(_))
    }
}

/// Checks an order against the limits and where the account stands.
pub fn check(prefs: &RiskPrefs, standing: &Standing, order: &OrderFacts) -> Verdict {
    if order.reduces {
        return Verdict::Ok;
    }
    if let Some(lock) = lock(prefs, standing) {
        return Verdict::Block(Refusal::Locked(lock));
    }
    if prefs.max_lots_per_order > 0.0 && order.lots > prefs.max_lots_per_order + 1e-9 {
        return Verdict::Block(Refusal::OrderTooBig {
            lots: order.lots,
            limit: prefs.max_lots_per_order,
        });
    }
    if order.one_click
        && prefs.one_click_max_lots > 0.0
        && order.lots > prefs.one_click_max_lots + 1e-9
    {
        return Verdict::Block(Refusal::OneClickTooBig {
            limit: prefs.one_click_max_lots,
        });
    }
    if prefs.max_lots_per_symbol > 0.0
        && order.symbol_lots + order.lots > prefs.max_lots_per_symbol + 1e-9
    {
        return Verdict::Block(Refusal::SymbolTooBig {
            lots: order.symbol_lots + order.lots,
            limit: prefs.max_lots_per_symbol,
        });
    }
    if prefs.max_open_positions > 0 && standing.open_positions >= prefs.max_open_positions as usize
    {
        return Verdict::Block(Refusal::TooManyPositions {
            open: standing.open_positions,
            limit: prefs.max_open_positions,
        });
    }
    if prefs.max_daily_trades > 0 && standing.trades_today >= prefs.max_daily_trades {
        return Verdict::Block(Refusal::TooManyOrdersToday {
            sent: standing.trades_today,
            limit: prefs.max_daily_trades,
        });
    }
    if prefs.require_stop_loss && !order.has_stop {
        return Verdict::Block(Refusal::StopLossRequired);
    }
    if prefs.price_collar_pct > 0.0
        && let Some(away) = order.away_pct
        && away > prefs.price_collar_pct
    {
        return Verdict::Block(Refusal::PriceTooFar {
            away_pct: away,
            limit_pct: prefs.price_collar_pct,
        });
    }
    let mut warnings = Vec::new();
    if prefs.max_spread_pips > 0.0
        && let Some(spread) = order.spread_pips
        && spread > prefs.max_spread_pips
    {
        warnings.push(Warning::SpreadTooWide {
            spread_pips: spread,
            limit_pips: prefs.max_spread_pips,
        });
    }
    if warnings.is_empty() {
        Verdict::Ok
    } else {
        Verdict::Warn(warnings)
    }
}

/// What kind of account an order goes to: simulated money or real money.
///
/// It is a type and not a flag so that a demo account and a live one cannot be taken for each
/// other where an order is cleared and where it is sent: both sides carry a `Mode` and the order
/// is refused when they differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Simulated money.
    Demo,
    /// Real money.
    Live,
}

impl Mode {
    /// `Live` when `live`, else `Demo`.
    #[must_use]
    pub const fn from_live(live: bool) -> Self {
        if live { Self::Live } else { Self::Demo }
    }

    /// Whether this is real money.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Live)
    }

    /// The word for a screen or a log.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Demo => "demo",
            Self::Live => "live",
        }
    }
}

/// Why an order was not cleared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blocked(pub Refusal);

/// Proof that an order went through [`check`] and was not refused, for an account of one [`Mode`].
///
/// It can only be made by [`clear`] and [`clear_batch`], and the client that sends orders asks for
/// one (`TradingClient::new_order`), so an order cannot reach the broker without the checks having
/// been made on it. It names the order it was made for ([`Fingerprint`]) and the mode of the
/// account: another order, or another kind of account, is refused at the door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cleared {
    order: Fingerprint,
    mode: Mode,
}

impl Cleared {
    /// The order this was made for.
    #[must_use]
    pub fn order(&self) -> Fingerprint {
        self.order
    }

    /// The kind of account it was made for.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether this clears `order` for an account of `mode`.
    #[must_use]
    pub fn covers(&self, order: Fingerprint, mode: Mode) -> bool {
        self.order == order && self.mode == mode
    }
}

/// Checks an order like [`check`], and gives the proof that it passed. A warning does not stop an
/// order (the confirmation of the ticket is where it is read); a refusal does.
///
/// # Errors
///
/// [`Blocked`] with the reason when [`check`] refuses the order.
pub fn clear(
    prefs: &RiskPrefs,
    standing: &Standing,
    facts: &OrderFacts,
    order: Fingerprint,
    mode: Mode,
) -> Result<Cleared, Blocked> {
    match check(prefs, standing, facts) {
        Verdict::Block(reason) => Err(Blocked(reason)),
        Verdict::Ok | Verdict::Warn(_) => Ok(Cleared { order, mode }),
    }
}

/// Checks the orders of a plan together, as `facts` (the side that sends the most, with its
/// volumes added up), and gives one proof for each of `orders`.
///
/// # Errors
///
/// [`Blocked`] with the reason when [`check`] refuses `facts`.
pub fn clear_batch(
    prefs: &RiskPrefs,
    standing: &Standing,
    facts: &OrderFacts,
    orders: &[Fingerprint],
    mode: Mode,
) -> Result<Vec<Cleared>, Blocked> {
    match check(prefs, standing, facts) {
        Verdict::Block(reason) => Err(Blocked(reason)),
        Verdict::Ok | Verdict::Warn(_) => Ok(orders
            .iter()
            .map(|&order| Cleared { order, mode })
            .collect()),
    }
}

/// An id of our own for an order, so that the order can be recognized on the account later,
/// whatever happened to the answer: `wyck-<time>-<process>-<count>`, at most 50 characters (the
/// broker's limit), unique for a process (the count) and across starts (the time).
#[must_use]
pub fn client_order_id(now_ms: i64, process: u32, count: u64) -> String {
    format!("wyck-{now_ms:x}-{process:x}-{count:x}")
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
pub(crate) const DUPLICATE_WINDOW_MS: i64 = 2_000;

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
        assert_eq!(check(&prefs, &standing(), &order(50.0)), Verdict::Ok);
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
        assert_eq!(check(&prefs, &standing(), &order(2.0)), Verdict::Ok);
        assert!(check(&prefs, &standing(), &order(2.01)).is_block());
        let prefs = RiskPrefs {
            max_lots_per_symbol: 3.0,
            ..RiskPrefs::default()
        };
        let held = OrderFacts {
            symbol_lots: 2.5,
            ..order(1.0)
        };
        assert!(check(&prefs, &standing(), &held).is_block());
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
        assert!(check(&prefs, &day, &order(0.1)).is_block());
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
        assert_eq!(check(&prefs, &standing(), &closing), Verdict::Ok);
        assert!(check(&prefs, &standing(), &order(0.1)).is_block());
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
        assert!(check(&prefs, &day, &order(0.1)).is_block());
        day.open_positions = 0;
        day.trades_today = 5;
        assert!(check(&prefs, &day, &order(0.1)).is_block());
        day.trades_today = 0;
        let bare = OrderFacts {
            has_stop: false,
            ..order(0.1)
        };
        assert!(check(&prefs, &day, &bare).is_block());
        let click = OrderFacts {
            one_click: true,
            ..order(1.5)
        };
        assert!(check(&prefs, &day, &click).is_block());
        assert_eq!(check(&prefs, &day, &order(1.5)), Verdict::Ok);
        let far = OrderFacts {
            away_pct: Some(40.0),
            ..order(0.1)
        };
        assert!(check(&prefs, &day, &far).is_block(), "default collar");
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
            check(&prefs, &standing(), &wide),
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

    fn fingerprint(volume: i64) -> Fingerprint {
        Fingerprint {
            symbol: 1,
            buy: true,
            volume,
            kind: 1,
            price: 0,
        }
    }

    #[test]
    fn a_refused_order_gets_no_proof_and_a_passing_one_gets_a_proof_for_itself() {
        let prefs = RiskPrefs {
            max_lots_per_order: 1.0,
            ..RiskPrefs::default()
        };
        let ok = OrderFacts {
            lots: 1.0,
            ..OrderFacts::default()
        };
        let too_big = OrderFacts {
            lots: 2.0,
            ..OrderFacts::default()
        };
        let cleared = clear(&prefs, &standing(), &ok, fingerprint(100), Mode::Demo)
            .expect("one lot is within the limit");
        assert!(cleared.covers(fingerprint(100), Mode::Demo));
        assert!(
            !cleared.covers(fingerprint(200), Mode::Demo),
            "another order"
        );
        assert!(
            !cleared.covers(fingerprint(100), Mode::Live),
            "another kind of account"
        );
        let blocked = clear(&prefs, &standing(), &too_big, fingerprint(200), Mode::Demo);
        assert!(matches!(
            blocked,
            Err(Blocked(Refusal::OrderTooBig { lots, limit }))
                if (lots - 2.0).abs() < 1e-9 && (limit - 1.0).abs() < 1e-9
        ));
    }

    #[test]
    fn a_warning_does_not_stop_the_proof() {
        let prefs = RiskPrefs {
            max_spread_pips: 1.0,
            ..RiskPrefs::default()
        };
        let wide = OrderFacts {
            lots: 1.0,
            spread_pips: Some(3.0),
            ..OrderFacts::default()
        };
        assert!(clear(&prefs, &standing(), &wide, fingerprint(1), Mode::Live).is_ok());
    }

    #[test]
    fn a_plan_is_cleared_together_and_the_kill_switch_stops_all_of_it() {
        let orders = [fingerprint(1), fingerprint(2)];
        let facts = OrderFacts {
            lots: 1.0,
            ..OrderFacts::default()
        };
        let proofs = clear_batch(
            &RiskPrefs::default(),
            &standing(),
            &facts,
            &orders,
            Mode::Demo,
        )
        .unwrap();
        assert_eq!(proofs.len(), 2);
        assert!(proofs[1].covers(fingerprint(2), Mode::Demo));
        let locked = RiskPrefs {
            kill_switch: true,
            ..RiskPrefs::default()
        };
        assert!(clear_batch(&locked, &standing(), &facts, &orders, Mode::Demo).is_err());
    }

    #[test]
    fn client_order_ids_are_unique_and_fit_the_brokers_limit() {
        let first = client_order_id(1_800_000_000_000, 4_194_304, 0);
        let second = client_order_id(1_800_000_000_000, 4_194_304, 1);
        assert_ne!(first, second);
        assert!(first.starts_with("wyck-"));
        let widest = client_order_id(i64::MAX, u32::MAX, u64::MAX);
        assert!(widest.chars().count() <= 50, "{widest}");
    }
}
