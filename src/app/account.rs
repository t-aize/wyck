//! The live account: a gpui entity that keeps an [`AccountBook`] current and sends the user's
//! orders.
//!
//! On every (re)connection it asks what the account holds (balance, positions, orders, recent
//! deals); after that the server's execution events keep it current. The profit of the positions
//! is asked every two seconds while any are open, and follows the prices in between (see
//! [`crate::domain::trading::math::live_net`]).
//!
//! Every trading call can fail or time out. A timeout does not mean the order was not placed (see
//! the non-idempotency notes of [`crate::app::broker::trading`]), so after a failed call the account is
//! reconciled again rather than the order resent.

pub mod notice;

use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::app::broker::session::Session;
use crate::app::broker::trading::NewOrderReq;
use crate::app::broker::{Error as ApiError, Result as ApiResult};
use crate::domain::market::PRICE_SCALE;
use crate::domain::trading::TradeSide;
use gpui::{Context, EventEmitter};

pub use self::notice::{Notice, NoticeAction, Reason, Tone, describe, explain, refusal};
use crate::app::market_data::live::LiveHub;
use crate::app::market_data::live::{ACCOUNT_OWNER, Wish};
use crate::domain::trading::book::AccountBook;
use crate::domain::trading::guard;
use crate::domain::trading::guard::{
    DuplicateGuard, Fingerprint, Lock, OrderFacts, RiskPrefs, Standing, Verdict,
};
use crate::domain::trading::math;
use crate::domain::trading::math::{Link, Summary};

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

#[derive(Debug, Clone, PartialEq)]
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
    live: bool,
    /// Orders of a plan waiting for the one before them to be answered.
    queue: VecDeque<NewOrderReq>,
    /// The limits that keep the account safe, and the zone the day is counted in.
    risk: RiskPrefs,
    zone: crate::domain::chart::Zone,
    duplicates: DuplicateGuard,
    /// Orders sent by this app today, as (day, count).
    sent_today: (i64, u32),
    reversals: ReverseTracker,
    /// When a time stop last tried to close each position, so a refusal is not retried at once.
    time_stops: HashMap<i64, Instant>,
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

/// What makes an order the same as another one sent again.
fn fingerprint(order: &NewOrderReq) -> Fingerprint {
    Fingerprint {
        symbol: order.symbol_id,
        buy: order.trade_side == TradeSide::Buy.number(),
        volume: order.volume,
        kind: i64::from(order.order_type),
        price: order
            .limit_price
            .or(order.stop_price)
            .map_or(0, |p| (p * PRICE_SCALE as f64).round() as i64),
    }
}

fn flatten<T>(result: Result<ApiResult<T>, tokio::task::JoinError>) -> ApiResult<T> {
    result.unwrap_or(Err(ApiError::Closed))
}

impl Account {
    pub fn new(session: Session, hub: Rc<LiveHub>, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PNL_EVERY).await;
                let alive = this.update(cx, |this, cx| {
                    if this.status == Status::Ready && !this.book.positions.is_empty() {
                        this.refresh_pnl(cx);
                        this.enforce_time_stops(cx);
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
            live: false,
            queue: VecDeque::new(),
            risk: RiskPrefs::default(),
            zone: crate::domain::chart::Zone::default(),
            duplicates: DuplicateGuard::default(),
            sent_today: (0, 0),
            reversals: ReverseTracker::default(),
            time_stops: HashMap::new(),
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
        self.live
    }

    pub fn set_live(&mut self, live: bool) {
        self.live = live;
    }

    /// Whether a call got no answer in time and the account has not been read again since.
    pub fn is_uncertain(&self) -> bool {
        self.uncertain
    }

    pub fn risk(&self) -> &RiskPrefs {
        &self.risk
    }

    /// Sets the safety limits and the zone the day is counted in.
    pub fn set_risk(
        &mut self,
        risk: RiskPrefs,
        zone: crate::domain::chart::Zone,
        cx: &mut Context<Self>,
    ) {
        if self.risk != risk || self.zone != zone {
            self.risk = risk;
            self.zone = zone;
            cx.notify();
        }
    }

    /// Where the account stands today: what the day made or lost, the orders sent, the last loss.
    pub fn standing(&self) -> Standing {
        let now = crate::app::market_data::now_ms();
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
    pub fn money(&self, amount: f64) -> String {
        math::format_money(amount, &self.book.currency)
    }

    /// What the safety checks say about a new order.
    pub fn assess(&self, order: &NewOrderReq, one_click: bool) -> Verdict {
        let contract = self.book.contract(order.symbol_id);
        let (bid, ask) = self.quote(order.symbol_id);
        let mid = math::mid(bid, ask);
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
        let facts = OrderFacts {
            lots: contract.lots_of_volume(order.volume),
            symbol_lots,
            has_stop: order.stop_loss.is_some() || order.relative_stop_loss.is_some(),
            away_pct,
            spread_pips: bid.zip(ask).map(|(b, a)| contract.pips(a - b)),
            one_click,
            reduces: false,
        };
        guard::check(&self.risk, &self.standing(), &facts, &|m| self.money(m))
    }

    /// The real bid and ask of a symbol.
    pub fn quote(&self, symbol_id: i64) -> (Option<f64>, Option<f64>) {
        let (bid, ask) = self.quotes.get(&symbol_id).copied().unwrap_or((None, None));
        let real = |raw: Option<i64>| raw.map(|r| r as f64 / PRICE_SCALE as f64);
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
    pub fn profits(&self) -> crate::app::alerts::Profits {
        let mut profits = crate::app::alerts::Profits::default();
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
                Conversion::Chain(chain) => math::chain_rate(*quote, deposit, chain, &|id| {
                    let (bid, ask) = self.quote(id);
                    math::mid(bid, ask)
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

    fn tell(&self, notice: Notice, cx: &mut Context<Self>) {
        cx.emit(AccountEvent::Notice(notice));
    }
}

mod actions;
mod orders;
mod reverse;
mod sync;

use reverse::{ReverseOrder, ReverseTracker};

#[cfg(test)]
#[path = "../../tests/unit/app/account.rs"]
mod tests;
