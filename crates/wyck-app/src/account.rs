//! The live account: a gpui entity that keeps an [`AccountBook`] current and sends the user's
//! orders.
//!
//! On every (re)connection it asks what the account holds (balance, positions, orders, recent
//! deals); after that the server's execution events keep it current. The profit of the positions
//! is asked every two seconds while any are open, and follows the prices in between (see
//! [`wyck_trading::profit::live_net`]).
//!
//! Every trading call can fail or time out. A timeout does not mean the order was not placed (see
//! the non-idempotency notes of [`crate::broker::trading`]), so after a failed call the account is
//! reconciled again rather than the order resent.

pub(crate) mod actions;
pub(crate) mod follow;
pub(crate) mod notice;
pub(crate) mod orders;
pub(crate) mod plans;
pub(crate) mod reverse;
pub(crate) mod sync;

pub use self::notice::{Notice, NoticeAction, Reason, Tone, describe, explain, refusal};
use crate::broker::session::Session;
use crate::broker::trading::NewOrderReq;
use crate::broker::{Error as ApiError, Result as ApiResult};
use crate::market_data::live::LiveHub;
use crate::market_data::live::{ACCOUNT_OWNER, Wish};
use gpui::{Context, EventEmitter};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};
use wyck_core::math::i64_to_f64;
use wyck_market::PRICE_SCALE;
use wyck_trading::book::AccountBook;
use wyck_trading::conversion::Link;
use wyck_trading::doubt::InDoubt;
use wyck_trading::guard;
use wyck_trading::guard::{
    Blocked, Cleared, DuplicateGuard, Lock, Mode, OrderFacts, Refusal, RiskPrefs, Standing, Verdict,
};
use wyck_trading::summary::Summary;
use wyck_trading::{amounts, conversion};

/// How long the recent history reaches back.
const HISTORY_DAYS: i64 = 7;
/// How often the profit is asked while positions are open.
const PNL_EVERY: Duration = Duration::from_secs(2);
const REVERSE_RECHECK: Duration = Duration::from_secs(10);
const REVERSE_FINAL_WAIT: Duration = Duration::from_secs(20);
/// How long a trading call may stay unanswered before the account says it cannot tell whether it
/// went through.
const CALL_TIMEOUT: Duration = Duration::from_secs(15);
/// How long a time stop waits before it tries again to close a position it could not.
const TIME_STOP_RETRY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Loading,
    Ready,
    Failed(String),
}

/// Something in flight, so its button waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Busy {
    Placing,
    Closing(i64),
    Cancelling(i64),
    Amending(i64),
}

/// Tells the views the account changed (they also observe the entity).
pub enum AccountEvent {
    Changed,
    /// Something to tell the user, with the things they can do about it.
    Notice(Notice),
}

pub struct Account {
    session: Session,
    hub: Rc<LiveHub>,
    pub book: AccountBook,
    /// The last bid and ask of every symbol seen, raw.
    quotes: HashMap<i64, (Option<i64>, Option<i64>)>,
    pub status: Status,
    busy: HashSet<Busy>,
    /// The call in flight for each kind of busy, so a late answer is not taken for a newer one.
    calls: HashMap<Busy, u64>,
    call_seq: u64,
    /// A call got no answer in time: what the server did with it is not known until the account
    /// is read again.
    uncertain: bool,
    /// Demo or live money. It is fixed when the account is made: an order is cleared for it and
    /// refused by the connection if the connection is for the other kind.
    mode: Mode,
    /// Orders of a plan waiting for the one before them to be answered, each with the proof that
    /// the safety checks passed for it.
    queue: VecDeque<(NewOrderReq, Cleared)>,
    /// Orders sent by this run, to give each its own id (see [`guard::client_order_id`]).
    orders_sent: u64,
    /// The new order in flight, kept in case its answer never comes.
    placing: Option<InDoubt>,
    /// New orders whose answer never came: the account says what became of each once it is read
    /// again, and none is sent a second time because of the doubt.
    in_doubt: Vec<InDoubt>,
    /// The limits that keep the account safe, and the zone the day is counted in.
    risk: RiskPrefs,
    zone: wyck_chart::Zone,
    duplicates: DuplicateGuard,
    /// Orders sent by this app today, as (day, count).
    sent_today: (i64, u32),
    reversals: reverse::ReverseTracker,
    /// When a time stop last tried to close each position, so a refusal is not retried at once.
    time_stops: HashMap<i64, Instant>,
    /// The pending orders whose stop follows the ATR.
    following: follow::Following,
    /// The asset each symbol is priced in, and how an asset converts into the deposit one.
    quote_assets: HashMap<i64, i64>,
    conversions: HashMap<i64, Conversion>,
    /// The symbol of the order ticket: its prices are followed and its rate asked.
    focus: Option<i64>,
    /// Bumped on every (re)load, so answers to an old one are ignored.
    epoch: u64,
    _poll: gpui::Task<()>,
}

impl EventEmitter<AccountEvent> for Account {}

/// How an asset converts into the deposit currency.
enum Conversion {
    Asking,
    /// Through these symbols, at their live prices.
    Chain(Vec<Link>),
    Failed,
}

fn flatten<T>(result: Result<ApiResult<T>, tokio::task::JoinError>) -> ApiResult<T> {
    result.unwrap_or(Err(ApiError::Closed))
}

impl Account {
    pub fn new(session: Session, hub: Rc<LiveHub>, mode: Mode, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PNL_EVERY).await;
                let alive = this.update(cx, |this, cx| {
                    if this.status == Status::Ready && !this.book.positions.is_empty() {
                        this.refresh_pnl(cx);
                        this.enforce_time_stops(cx);
                    }
                    if this.follows_atr() {
                        this.follow_atr(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            session,
            hub,
            book: AccountBook::default(),
            quotes: HashMap::new(),
            status: Status::Loading,
            busy: HashSet::new(),
            calls: HashMap::new(),
            call_seq: 0,
            uncertain: false,
            mode,
            queue: VecDeque::new(),
            orders_sent: 0,
            placing: None,
            in_doubt: Vec::new(),
            risk: RiskPrefs::default(),
            zone: wyck_chart::Zone::default(),
            duplicates: DuplicateGuard::default(),
            sent_today: (0, 0),
            reversals: reverse::ReverseTracker::default(),
            time_stops: HashMap::new(),
            following: follow::Following::default(),
            quote_assets: HashMap::new(),
            conversions: HashMap::new(),
            focus: None,
            epoch: 0,
            _poll: poll,
        }
    }

    pub fn session(&self) -> Session {
        self.session.clone()
    }

    pub fn is_busy(&self, what: Busy) -> bool {
        self.busy.contains(&what)
            || matches!(what, Busy::Closing(id) if self.reversals.0.contains_key(&id))
    }

    /// Whether this is a live account (real money) rather than a demo one.
    pub fn is_live(&self) -> bool {
        self.mode.is_live()
    }

    /// Demo or live.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether a call got no answer in time and the account has not been read again since.
    pub fn is_uncertain(&self) -> bool {
        self.uncertain
    }

    pub fn risk(&self) -> &RiskPrefs {
        &self.risk
    }

    /// Sets the safety limits and the zone the day is counted in.
    pub fn set_risk(&mut self, risk: RiskPrefs, zone: wyck_chart::Zone, cx: &mut Context<Self>) {
        if self.risk != risk || self.zone != zone {
            self.risk = risk;
            self.zone = zone;
            cx.notify();
        }
    }

    /// Where the account stands today: what the day made or lost, the orders sent, the last loss.
    pub fn standing(&self) -> Standing {
        let now = crate::market_data::now_ms();
        let today = self.zone.day(now);
        let (mut realized, mut opened) = (0.0, 0u32);
        let mut last_loss: Option<i64> = None;
        for deal in &self.book.deals {
            let pnl = deal.realized_pnl();
            if pnl.is_some_and(|p| p < 0.0) {
                last_loss = last_loss.max(Some(deal.execution_timestamp));
            }
            if self.zone.day(deal.execution_timestamp) != today {
                continue;
            }
            match pnl {
                Some(p) => realized += p,
                None => opened += 1,
            }
        }
        let summary = self.summary();
        let sent = if self.sent_today.0 == today {
            self.sent_today.1
        } else {
            0
        };
        Standing {
            open_positions: self.book.positions.len(),
            trades_today: opened.max(sent),
            day_pnl: realized + summary.unrealized,
            day_start_balance: summary.balance - realized,
            last_loss_at: last_loss,
            now,
        }
    }

    /// What stops new orders now, if anything.
    pub fn lock(&self) -> Option<Lock> {
        guard::lock(&self.risk, &self.standing())
    }

    /// Writes an amount in the account's money.
    pub(crate) fn money(&self, amount: f64) -> String {
        amounts::format_money(amount, &self.book.currency)
    }

    /// What the safety checks say about a new order.
    pub(crate) fn assess(&self, order: &NewOrderReq, one_click: bool) -> Verdict {
        guard::check(
            &self.risk,
            &self.standing(),
            &self.facts_of(order, one_click),
        )
    }

    /// Why the limits refuse an order, in words.
    pub fn refusal_text(&self, refusal: &Refusal) -> String {
        notice::refusal_text(refusal, &|m| self.money(m), crate::market_data::now_ms())
    }

    /// Why nothing new can be sent, in words, for a banner.
    pub fn lock_text(&self, lock: &Lock) -> String {
        notice::lock_text(lock, &|m| self.money(m), crate::market_data::now_ms())
    }

    /// A verdict in words: the reason when a hard limit refuses the order, else the warnings it
    /// is worth reading before sending (none when all is well).
    pub fn verdict_text(&self, verdict: &Verdict) -> Result<Vec<String>, String> {
        match verdict {
            Verdict::Block(refusal) => Err(self.refusal_text(refusal)),
            Verdict::Warn(warnings) => Ok(warnings.iter().map(notice::warning_text).collect()),
            Verdict::Ok => Ok(Vec::new()),
        }
    }

    /// Checks a new order and gives the proof the connection asks for before it sends it.
    pub(crate) fn clear(&self, order: &NewOrderReq, one_click: bool) -> Result<Cleared, Blocked> {
        guard::clear(
            &self.risk,
            &self.standing(),
            &self.facts_of(order, one_click),
            order.fingerprint(),
            self.mode,
        )
    }

    /// An order as the safety checks see it.
    fn facts_of(&self, order: &NewOrderReq, one_click: bool) -> OrderFacts {
        let contract = self.book.contract(order.symbol_id);
        let (bid, ask) = self.quote(order.symbol_id);
        let mid = conversion::mid(bid, ask);
        let symbol_lots: f64 = self
            .book
            .positions
            .values()
            .filter(|p| p.trade_data.symbol_id == order.symbol_id)
            .map(|p| contract.lots_of_volume(p.trade_data.volume))
            .sum();
        let away_pct = order
            .limit_price
            .or(order.stop_price)
            .zip(mid)
            .filter(|(_, mid)| *mid > 0.0)
            .map(|(price, mid)| (price - mid).abs() / mid * 100.0);
        OrderFacts {
            lots: contract.lots_of_volume(order.volume),
            symbol_lots,
            has_stop: order.stop_loss.is_some() || order.relative_stop_loss.is_some(),
            away_pct,
            spread_pips: bid.zip(ask).map(|(b, a)| contract.pips(a - b)),
            one_click,
            reduces: false,
        }
    }

    /// The real bid and ask of a symbol.
    pub fn quote(&self, symbol_id: i64) -> (Option<f64>, Option<f64>) {
        let (bid, ask) = self.quotes.get(&symbol_id).copied().unwrap_or((None, None));
        let real = |raw: Option<i64>| raw.map(|r| i64_to_f64(r) / i64_to_f64(PRICE_SCALE));
        (real(bid), real(ask))
    }

    pub fn summary(&self) -> Summary {
        self.book.summary(&|id| self.quote(id))
    }

    pub fn net_profit(&self, position_id: i64) -> Option<f64> {
        self.book.net_profit(position_id, &|id| self.quote(id))
    }

    /// What the open positions make or lose now, for the alerts on a profit. Empty until the
    /// account is read.
    pub fn profits(&self) -> crate::alerts::Profits {
        let mut profits = crate::alerts::Profits::default();
        if self.status != Status::Ready {
            return profits;
        }
        profits.account = Some(self.summary().unrealized);
        for position in self.book.positions.values() {
            if let Some(net) = self.net_profit(position.position_id) {
                profits.positions.insert(position.position_id, net);
                *profits
                    .symbols
                    .entry(position.trade_data.symbol_id)
                    .or_default() += net;
            }
        }
        profits
    }

    /// The broker's names of the symbols (for the lists), the currency each is quoted in (for
    /// the profit between the server's answers) and the id of that currency's asset (for the
    /// rate that converts it into the deposit currency).
    pub fn set_symbols(
        &mut self,
        names: HashMap<i64, String>,
        quote_currency: HashMap<i64, String>,
        quote_assets: HashMap<i64, i64>,
        cx: &mut Context<Self>,
    ) {
        self.book.names = names;
        self.book.quote_currency = quote_currency;
        self.quote_assets = quote_assets;
        cx.notify();
    }

    /// Deposit currency per unit of the currency `symbol_id` is quoted in, at the live prices.
    /// `None` until the conversion is known and priced.
    pub fn rate(&self, symbol_id: i64) -> Option<f64> {
        let deposit = self.book.trader.as_ref()?.deposit_asset_id;
        match (self.quote_assets.get(&symbol_id), deposit) {
            (Some(quote), Some(deposit)) if *quote == deposit => Some(1.0),
            (Some(quote), Some(deposit)) => match self.conversions.get(quote)? {
                Conversion::Chain(chain) => conversion::chain_rate(*quote, deposit, chain, &|id| {
                    let (bid, ask) = self.quote(id);
                    conversion::mid(bid, ask)
                }),
                Conversion::Asking | Conversion::Failed => None,
            },
            // Without the asset ids, only a symbol quoted in the deposit currency is sure.
            _ => {
                let quote = self.book.quote_currency.get(&symbol_id)?;
                (!quote.is_empty() && *quote == self.book.currency).then_some(1.0)
            }
        }
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        // The prices of what the account holds are followed for the profit and the lists.
        self.hub
            .set(ACCOUNT_OWNER, Some(Wish::spots(self.followed())));
        cx.emit(AccountEvent::Changed);
        cx.notify();
    }

    fn tell(notice: Notice, cx: &mut Context<Self>) {
        cx.emit(AccountEvent::Notice(notice));
    }
}
