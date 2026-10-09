//! The live account: a gpui entity that keeps an [`AccountBook`] current and sends the user's
//! orders.
//!
//! On every (re)connection it asks what the account holds (balance, positions, orders, recent
//! deals); after that the server's execution events keep it current. The profit of the positions
//! is asked every two seconds while any are open, and follows the prices in between (see
//! [`super::math::live_net`]).
//!
//! Every trading call can fail or time out. A timeout does not mean the order was not placed (see
//! the non-idempotency notes of [`wyck_openapi::trading`]), so after a failed call the account is
//! reconciled again rather than the order resent.

use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{Context, EventEmitter};
use wyck_openapi::account::PositionStatus;
use wyck_openapi::account::TradeSide;
use wyck_openapi::market::PRICE_SCALE;
use wyck_openapi::session::Session;
use wyck_openapi::trading::{AmendOrderReq, AmendPositionSlTpReq, ExecutionType, NewOrderReq};
use wyck_openapi::{Error as ApiError, Event, Result as ApiResult};

use super::book::{AccountBook, Notice, NoticeAction, Tone, describe, is_buy, refusal};
use super::guard::{
    self, DuplicateGuard, Fingerprint, Lock, OrderFacts, RiskPrefs, Standing, Verdict,
};
use super::math::{self, Contract, Link, Summary};
use crate::chart::LiveHub;
use crate::chart::live::{ACCOUNT_OWNER, Wish};
use crate::runtime;
use crate::ui::kit::toast;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReverseOrder {
    symbol: i64,
    buy: bool,
    volume: i64,
}

#[derive(Debug, Clone, Copy)]
struct PendingReverse {
    order: ReverseOrder,
    started: Instant,
    close_order_id: Option<i64>,
    acknowledged: bool,
    closed: bool,
}

#[derive(Default)]
struct ReverseTracker(HashMap<i64, PendingReverse>);

impl ReverseTracker {
    fn start(&mut self, position_id: i64, order: ReverseOrder) -> bool {
        if self.0.contains_key(&position_id) {
            return false;
        }
        self.0.insert(
            position_id,
            PendingReverse {
                order,
                started: Instant::now(),
                close_order_id: None,
                acknowledged: false,
                closed: false,
            },
        );
        true
    }

    fn acknowledged(
        &mut self,
        position_id: i64,
        kind: Option<ExecutionType>,
        close_order_id: Option<i64>,
    ) -> Option<ReverseOrder> {
        if !matches!(
            kind,
            Some(
                ExecutionType::OrderAccepted
                    | ExecutionType::OrderPartialFill
                    | ExecutionType::OrderFilled
            )
        ) {
            self.0.remove(&position_id);
            return None;
        }
        if let Some(pending) = self.0.get_mut(&position_id) {
            pending.acknowledged = true;
            pending.close_order_id = close_order_id;
        }
        self.take_ready(position_id)
    }

    fn cancel_related(&mut self, position_id: Option<i64>, order_id: Option<i64>) {
        if position_id.is_none() && order_id.is_none() {
            self.0.clear();
            return;
        }
        self.0.retain(|id, pending| {
            Some(*id) != position_id && !(order_id.is_some() && pending.close_order_id == order_id)
        });
    }

    fn closed(&mut self, position_id: i64) -> Option<ReverseOrder> {
        if let Some(pending) = self.0.get_mut(&position_id) {
            pending.closed = true;
        }
        self.take_ready(position_id)
    }

    fn take_ready(&mut self, position_id: i64) -> Option<ReverseOrder> {
        self.0
            .get(&position_id)
            .is_some_and(|p| p.acknowledged && p.closed)
            .then(|| self.0.remove(&position_id))
            .flatten()
            .map(|p| p.order)
    }
}

/// Tells the views the account changed (they also observe the entity).
pub enum AccountEvent {
    Changed,
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
    zone: crate::chart_core::Zone,
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
            zone: crate::chart_core::Zone::default(),
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
    pub fn set_risk(&mut self, risk: RiskPrefs, zone: crate::chart_core::Zone, cx: &mut Context<Self>) {
        if self.risk != risk || self.zone != zone {
            self.risk = risk;
            self.zone = zone;
            cx.notify();
        }
    }

    /// Where the account stands today: what the day made or lost, the orders sent, the last loss.
    pub fn standing(&self) -> Standing {
        let now = crate::chart::now_ms();
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

    /// Asks the server once how the currency of `symbol_id` converts into the deposit one, and
    /// follows the prices of the symbols that convert it.
    fn ensure_rate(&mut self, symbol_id: i64, cx: &mut Context<Self>) {
        let deposit = self.book.trader.as_ref().and_then(|t| t.deposit_asset_id);
        let (Some(&quote), Some(deposit)) = (self.quote_assets.get(&symbol_id), deposit) else {
            return;
        };
        if quote == deposit || self.conversions.contains_key(&quote) {
            return;
        }
        self.conversions.insert(quote, Conversion::Asking);
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let chain = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                client
                    .account(session.account_id())
                    .market()
                    .symbols_for_conversion(quote, deposit)
                    .await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                let conversion = match flatten(chain) {
                    Ok(symbols) => {
                        let links: Option<Vec<Link>> = symbols
                            .iter()
                            .map(|s| {
                                Some(Link {
                                    symbol_id: s.symbol_id,
                                    base: s.base_asset_id?,
                                    quote: s.quote_asset_id?,
                                })
                            })
                            .collect();
                        links.map_or(Conversion::Failed, Conversion::Chain)
                    }
                    Err(error) => {
                        tracing::debug!(%error, quote, deposit, "no conversion chain");
                        Conversion::Failed
                    }
                };
                this.conversions.insert(quote, conversion);
                this.changed(cx);
            });
        })
        .detach();
    }

    /// The order ticket is for `symbol_id` now: how it trades and the rate of its currency are
    /// asked, and its prices followed.
    pub fn focus(&mut self, symbol_id: Option<i64>, cx: &mut Context<Self>) {
        self.focus = symbol_id;
        if let Some(id) = symbol_id {
            self.ensure_contract(id, cx);
            self.ensure_rate(id, cx);
        }
        self.changed(cx);
    }

    /// The symbols whose prices the account follows: what it holds, the ticket's, and what
    /// converts the currencies into the deposit one.
    fn followed(&self) -> Vec<i64> {
        let mut ids = self.book.symbols();
        ids.extend(self.focus);
        for conversion in self.conversions.values() {
            if let Conversion::Chain(chain) = conversion {
                ids.extend(chain.iter().map(|link| link.symbol_id));
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        // The prices of what the account holds are followed for the profit and the lists.
        self.hub
            .set(ACCOUNT_OWNER, Some(Wish::spots(self.followed())));
        cx.emit(AccountEvent::Changed);
        cx.notify();
    }

    fn tell(&self, notice: Notice, cx: &mut Context<Self>) {
        let kind = match notice.tone {
            Tone::Info => toast::Kind::Info,
            Tone::Success => toast::Kind::Success,
            Tone::Warning => toast::Kind::Warning,
            Tone::Error => toast::Kind::Error,
            _ => toast::Kind::Info,
        };
        let has_actions = !notice.actions.is_empty();
        let mut toast = toast::Toast::new(kind, notice.title, notice.message)
            .hint_opt(notice.hint)
            .details_opt(notice.details);
        let account = cx.entity();
        for action in notice.actions {
            let account = account.clone();
            toast = match action {
                NoticeAction::ClosePosition(id) => toast.action("Close position", move |_, cx| {
                    account.update(cx, |a, cx| a.close_position(id, None, cx));
                }),
                NoticeAction::BreakEven(id) => toast.action("Stop to entry", move |_, cx| {
                    account.update(cx, |a, cx| a.break_even(id, cx));
                }),
                NoticeAction::CancelOrder(id) => toast.action("Cancel order", move |_, cx| {
                    account.update(cx, |a, cx| a.cancel_order(id, cx));
                }),
                _ => toast,
            };
        }
        // A fill with buttons goes away by itself: the panel keeps the same buttons.
        if has_actions && notice.tone == Tone::Success {
            toast = toast.sticky(false);
        }
        toast.show(cx);
    }

    fn watch_reverse(&mut self, position_id: i64, started: Instant, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REVERSE_RECHECK).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .reversals
                    .0
                    .get(&position_id)
                    .is_some_and(|pending| pending.started == started)
                {
                    this.on_ready(cx);
                }
            });
            cx.background_executor().timer(REVERSE_FINAL_WAIT).await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .reversals
                    .0
                    .get(&position_id)
                    .is_some_and(|pending| pending.started == started)
                {
                    this.reversals.0.remove(&position_id);
                    this.tell(
                        Notice::new(
                            Tone::Warning,
                            "Reverse stopped",
                            "The position's closure was not confirmed.",
                        )
                        .hint(Some("Check the account before trying again.".to_owned())),
                        cx,
                    );
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ---- loading ----

    /// The session is connected (again): read everything afresh.
    pub fn on_ready(&mut self, cx: &mut Context<Self>) {
        self.epoch += 1;
        let epoch = self.epoch;
        if self.book.trader.is_none() {
            self.status = Status::Loading;
        }
        cx.notify();
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let loaded = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                let data = account.account_data();
                let trader = data.trader().await?;
                let (positions, orders) = data.open_positions_and_orders(false).await?;
                let now = crate::chart::now_ms();
                let deals = data
                    .deals(now - HISTORY_DAYS * 86_400_000, now, Some(500))
                    .await
                    .map(|(deals, _)| deals)
                    .unwrap_or_default();
                let missed = missed_exits(&account, &positions, &orders, &deals, now).await;
                let currency = match trader.deposit_asset_id {
                    Some(asset) => account
                        .market()
                        .assets()
                        .await
                        .ok()
                        .and_then(|assets| {
                            assets
                                .into_iter()
                                .find(|a| a.asset_id == asset)
                                .map(|a| a.display_name.unwrap_or(a.name))
                        })
                        .unwrap_or_default(),
                    None => String::new(),
                };
                Ok::<_, ApiError>((trader, positions, orders, deals, currency, missed))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                match flatten(loaded) {
                    Ok((trader, positions, orders, mut deals, currency, missed)) => {
                        this.book.trader = Some(trader);
                        this.book.currency = currency;
                        this.book.reconcile(positions, orders);
                        let closed: Vec<i64> = this
                            .reversals
                            .0
                            .keys()
                            .copied()
                            .filter(|id| !this.book.positions.contains_key(id))
                            .collect();
                        let reverse_orders: Vec<ReverseOrder> = closed
                            .into_iter()
                            .filter_map(|id| this.reversals.closed(id))
                            .collect();
                        deals.sort_by_key(|d| std::cmp::Reverse(d.execution_timestamp));
                        this.book.deals = deals;
                        this.status = Status::Ready;
                        // The account was read again: whatever a silent call did is in it now.
                        this.uncertain = false;
                        for symbol in this.book.symbols() {
                            this.ensure_contract(symbol, cx);
                        }
                        // A chain that failed is asked again after a reconnection.
                        this.conversions
                            .retain(|_, c| matches!(c, Conversion::Chain(_)));
                        if let Some(symbol) = this.focus {
                            this.ensure_rate(symbol, cx);
                        }
                        this.refresh_pnl(cx);
                        this.changed(cx);
                        this.catch_up(missed, cx);
                        for order in reverse_orders {
                            this.market(order.symbol, order.buy, order.volume, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "could not read the account");
                        this.status = Status::Failed(error.to_string());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Asks the broker how a symbol trades, once.
    pub fn ensure_contract(&mut self, symbol_id: i64, cx: &mut Context<Self>) {
        if self.book.contracts.contains_key(&symbol_id) {
            return;
        }
        // A placeholder until the answer, so the symbol is asked only once.
        self.book.contracts.insert(symbol_id, Contract::default());
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let fetched = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                account.market().symbol_details(&[symbol_id]).await
            })
            .await;
            let _ = this.update(cx, |this, cx| match flatten(fetched) {
                Ok(details) => {
                    if let Some(symbol) = details.iter().find(|s| s.symbol_id == symbol_id) {
                        this.book
                            .contracts
                            .insert(symbol_id, Contract::from_symbol(symbol));
                        cx.notify();
                    }
                }
                Err(_) => {
                    this.book.contracts.remove(&symbol_id);
                }
            });
        })
        .detach();
    }

    fn refresh_pnl(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let epoch = self.epoch;
        cx.spawn(async move |this, cx| {
            let answer = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                account.account_data().unrealized_pnl().await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                if let Ok(answer) = flatten(answer) {
                    let digits = answer
                        .money_digits
                        .and_then(|d| u32::try_from(d).ok())
                        .or_else(|| this.book.money_digits());
                    let quotes: HashMap<i64, (Option<f64>, Option<f64>)> = this
                        .book
                        .symbols()
                        .into_iter()
                        .map(|id| (id, this.quote(id)))
                        .collect();
                    this.book
                        .set_pnl(&answer.position_unrealized_pnl, digits, &|id| {
                            quotes.get(&id).copied().unwrap_or((None, None))
                        });
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn refresh_trader(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let trader = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                client
                    .account(session.account_id())
                    .account_data()
                    .trader()
                    .await
            })
            .await;
            if let Ok(trader) = flatten(trader) {
                let _ = this.update(cx, |this, cx| {
                    this.book.trader = Some(trader);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    // ---- what the server says ----

    pub fn on_event(&mut self, event: &Event, cx: &mut Context<Self>) {
        match event {
            Event::Spot(spot) => {
                let entry = self.quotes.entry(spot.symbol_id).or_insert((None, None));
                entry.0 = spot.bid.or(entry.0);
                entry.1 = spot.ask.or(entry.1);
                if self.followed().contains(&spot.symbol_id) {
                    cx.notify();
                }
            }
            Event::Execution(execution) => {
                if matches!(
                    execution.kind(),
                    Some(
                        ExecutionType::OrderRejected
                            | ExecutionType::OrderCancelled
                            | ExecutionType::OrderExpired
                    )
                ) {
                    self.reversals.cancel_related(
                        execution
                            .position
                            .as_ref()
                            .map(|p| p.position_id)
                            .or_else(|| execution.order.as_ref().and_then(|o| o.position_id)),
                        execution.order.as_ref().map(|o| o.order_id),
                    );
                }
                let applied = self.book.apply(execution);
                self.manage(execution, cx);
                let reverse_order = if execution.kind() == Some(ExecutionType::OrderFilled) {
                    execution.position.as_ref().and_then(|position| {
                        (position.status() == Some(PositionStatus::Closed))
                            .then(|| self.reversals.closed(position.position_id))
                            .flatten()
                    })
                } else {
                    None
                };
                if let Some(notice) = applied.notice {
                    self.tell(notice, cx);
                }
                if applied.balance_changed {
                    self.refresh_trader(cx);
                }
                for symbol in self.book.symbols() {
                    self.ensure_contract(symbol, cx);
                }
                self.refresh_pnl(cx);
                self.changed(cx);
                if let Some(order) = reverse_order {
                    self.market(order.symbol, order.buy, order.volume, cx);
                }
            }
            Event::OrderError(error) => {
                self.reversals
                    .cancel_related(error.position_id, error.order_id);
                let reason = refusal(&error.error_code, error.description.as_deref());
                self.tell(
                    Notice::new(Tone::Error, "Order refused", reason.message)
                        .hint(reason.hint)
                        .details(match &error.description {
                            Some(text) => format!("{}: {text}", error.error_code),
                            None => error.error_code.clone(),
                        }),
                    cx,
                );
            }
            Event::TraderUpdated(updated) => {
                self.book.trader = Some(updated.trader.clone());
                cx.notify();
            }
            Event::MarginChanged(changed) => {
                if let Some(position) = self.book.positions.get_mut(&changed.position_id) {
                    position.used_margin = Some(changed.used_margin);
                    cx.notify();
                }
            }
            Event::TrailingSlChanged(changed) => {
                if let Some(position) = self.book.positions.get_mut(&changed.position_id) {
                    position.stop_loss = Some(changed.stop_price);
                    self.changed(cx);
                }
            }
            Event::MarginCallTriggered(_) => {
                toast::show(
                    cx,
                    toast::Kind::Warning,
                    "Margin call",
                    "The account's margin level reached a margin call threshold.",
                );
            }
            _ => {}
        }
    }

    // ---- what the user does ----

    /// Runs a trading call; on failure says why and reads the account again (the call may have
    /// reached the server even so).
    fn trade<F, Fut>(&mut self, busy: Busy, cx: &mut Context<Self>, call: F)
    where
        F: FnOnce(wyck_openapi::trading::TradingClient) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ApiResult<wyck_openapi::trading::ExecutionEvent>>
            + Send
            + 'static,
    {
        if !self.busy.insert(busy) {
            return;
        }
        self.call_seq += 1;
        let seq = self.call_seq;
        self.calls.insert(busy, seq);
        cx.notify();
        // No answer in time: the button is free again, but the account cannot tell what became of
        // the call, so it says so and reads the account.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CALL_TIMEOUT).await;
            let _ = this.update(cx, |this, cx| {
                if this.calls.get(&busy) == Some(&seq) {
                    this.calls.remove(&busy);
                    this.busy.remove(&busy);
                    this.uncertain = true;
                    this.queue.clear();
                    this.tell(
                        Notice::new(
                            Tone::Warning,
                            "No answer from the server",
                            "The request may or may not have gone through. Do not send it again yet.",
                        )
                        .hint(Some(
                            "Check the positions and orders of the account, then try again if it is not there."
                                .to_owned(),
                        )),
                        cx,
                    );
                    this.on_ready(cx);
                    cx.notify();
                }
            });
        })
        .detach();
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                call(client.account(session.account_id()).trading()).await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                // A call that was given up on has nothing to release: a newer one may hold it.
                if this.calls.get(&busy) == Some(&seq) {
                    this.calls.remove(&busy);
                    this.busy.remove(&busy);
                }
                match flatten(result) {
                    // The first execution event answers the request and goes only to it; the
                    // ones after it (a fill after the acceptance) come on the event stream.
                    Ok(event) => {
                        let reverse_order = match busy {
                            Busy::Closing(id) => this.reversals.acknowledged(
                                id,
                                event.kind(),
                                event.order.as_ref().map(|o| o.order_id),
                            ),
                            _ => None,
                        };
                        this.on_event(&Event::Execution(Box::new(event)), cx);
                        if let Some(order) = reverse_order {
                            this.market(order.symbol, order.buy, order.volume, cx);
                        }
                        if let Busy::Closing(id) = busy
                            && let Some(pending) = this.reversals.0.get(&id)
                        {
                            this.watch_reverse(id, pending.started, cx);
                        }
                        if busy == Busy::Placing {
                            this.send_next(cx);
                        }
                    }
                    Err(error) => {
                        if let Busy::Closing(id) = busy {
                            this.reversals.0.remove(&id);
                        }
                        if busy == Busy::Placing {
                            this.drop_queue(cx);
                        }
                        let reason = describe(&error);
                        let title = match busy {
                            Busy::Placing => "The order was not sent",
                            Busy::Closing(_) => "The position was not closed",
                            Busy::Cancelling(_) => "The order was not cancelled",
                            Busy::Amending(_) => "The change was not applied",
                        };
                        this.tell(
                            Notice::new(Tone::Error, title, reason.message)
                                .hint(reason.hint)
                                .details(error.to_string()),
                            cx,
                        );
                        this.on_ready(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Sends a new order.
    pub fn place(&mut self, order: NewOrderReq, cx: &mut Context<Self>) {
        self.place_with(order, false, cx);
    }

    /// Sends a new order. `one_click` says it was sent without a confirmation, which the safety
    /// limits treat more strictly. The order is refused when a hard limit says so, and dropped
    /// when it repeats the one just sent.
    pub fn place_with(&mut self, mut order: NewOrderReq, one_click: bool, cx: &mut Context<Self>) {
        order.label.get_or_insert_with(|| "wyck".into());
        if let Err(error) = order.validate() {
            self.tell(
                Notice::new(Tone::Error, "The order is not complete", error.to_string()),
                cx,
            );
            return;
        }
        if let Verdict::Block(reason) = self.assess(&order, one_click) {
            self.tell(
                Notice::new(Tone::Warning, "Blocked by your safety settings", reason)
                    .hint(Some("Change the limits in Settings, Safety.".to_owned())),
                cx,
            );
            return;
        }
        let now = crate::chart::now_ms();
        let fingerprint = fingerprint(&order);
        if self.busy.contains(&Busy::Placing) || self.duplicates.is_repeat(fingerprint, now) {
            self.tell(
                Notice::new(
                    Tone::Info,
                    "Order not sent again",
                    "The same order was sent a moment ago.",
                ),
                cx,
            );
            return;
        }
        let today = self.zone.day(now);
        if self.sent_today.0 != today {
            self.sent_today = (today, 0);
        }
        self.sent_today.1 += 1;
        self.trade(Busy::Placing, cx, move |trading| async move {
            trading.new_order(order).await
        });
    }

    /// What the safety checks say about the orders of a plan together: the side that sends the
    /// most is checked with its volumes added up (an OCO pair fills on one side only).
    pub fn assess_batch(&self, orders: &[NewOrderReq], one_click: bool) -> Verdict {
        let mut totals: Vec<(i32, i64, usize)> = Vec::new();
        for (index, order) in orders.iter().enumerate() {
            match totals
                .iter_mut()
                .find(|(side, _, _)| *side == order.trade_side)
            {
                Some(entry) => entry.1 += order.volume,
                None => totals.push((order.trade_side, order.volume, index)),
            }
        }
        let Some(&(_, volume, first)) = totals.iter().max_by_key(|(_, volume, _)| *volume) else {
            return Verdict::Ok;
        };
        let mut probe = orders[first].clone();
        probe.volume = volume;
        self.assess(&probe, one_click)
    }

    /// Sends the orders of a plan one after the other, each once the one before it was answered.
    /// They are checked and counted as one order. If one fails, the ones after it are not sent.
    pub fn place_batch(
        &mut self,
        orders: Vec<NewOrderReq>,
        one_click: bool,
        cx: &mut Context<Self>,
    ) {
        if orders.len() <= 1 {
            if let Some(order) = orders.into_iter().next() {
                self.place_with(order, one_click, cx);
            }
            return;
        }
        for order in &orders {
            if let Err(error) = order.validate() {
                self.tell(
                    Notice::new(Tone::Error, "The order is not complete", error.to_string()),
                    cx,
                );
                return;
            }
        }
        if let Verdict::Block(reason) = self.assess_batch(&orders, one_click) {
            self.tell(
                Notice::new(Tone::Warning, "Blocked by your safety settings", reason)
                    .hint(Some("Change the limits in Settings, Safety.".to_owned())),
                cx,
            );
            return;
        }
        let now = crate::chart::now_ms();
        if self.busy.contains(&Busy::Placing)
            || !self.queue.is_empty()
            || self.duplicates.is_repeat(fingerprint(&orders[0]), now)
        {
            self.tell(
                Notice::new(
                    Tone::Info,
                    "Order not sent again",
                    "The same order was sent a moment ago.",
                ),
                cx,
            );
            return;
        }
        let today = self.zone.day(now);
        if self.sent_today.0 != today {
            self.sent_today = (today, 0);
        }
        self.sent_today.1 += 1;
        for mut order in orders {
            order.label.get_or_insert_with(|| "wyck".into());
            self.queue.push_back(order);
        }
        self.send_next(cx);
    }

    /// Sends the next order of a plan, when none is in flight.
    fn send_next(&mut self, cx: &mut Context<Self>) {
        if self.busy.contains(&Busy::Placing) {
            return;
        }
        if let Some(order) = self.queue.pop_front() {
            self.trade(Busy::Placing, cx, move |trading| async move {
                trading.new_order(order).await
            });
        }
    }

    /// Drops the orders of a plan that were waiting, and says so.
    fn drop_queue(&mut self, cx: &mut Context<Self>) {
        let left = self.queue.len();
        self.queue.clear();
        if left > 0 {
            self.tell(
                Notice::new(
                    Tone::Warning,
                    "The rest of the plan was not sent",
                    format!("{left} order(s) of the plan were left out."),
                )
                .hint(Some(
                    "Check what is open before sending it again.".to_owned(),
                )),
                cx,
            );
        }
    }

    /// What the app adds to the orders of a plan (see [`super::plan`]): an OCO pair whose one
    /// side filled loses the other, and a leg that closed in profit moves the stops of the legs
    /// left to their entry.
    fn manage(
        &mut self,
        execution: &wyck_openapi::trading::ExecutionEvent,
        cx: &mut Context<Self>,
    ) {
        use super::plan::{self, Label, Open};
        if execution.kind() != Some(ExecutionType::OrderFilled) {
            return;
        }
        if let Some(order) = &execution.order
            && let Some(label) = order.trade_data.label.as_deref().and_then(Label::decode)
            && label.oco
        {
            let buy = is_buy(order.trade_data.trade_side);
            let working: Vec<(i64, Label, bool)> = self
                .book
                .orders
                .values()
                .filter_map(|o| {
                    Some((
                        o.order_id,
                        Label::decode(o.trade_data.label.as_deref()?)?,
                        is_buy(o.trade_data.trade_side),
                    ))
                })
                .collect();
            for id in plan::oco_siblings(&label, buy, &working) {
                self.cancel_order(id, cx);
            }
        }
        if let (Some(position), Some(deal)) = (&execution.position, &execution.deal)
            && position.status() == Some(PositionStatus::Closed)
            && let Some(profit) = deal.realized_pnl()
            && let Some(label) = position.trade_data.label.as_deref().and_then(Label::decode)
        {
            let pip = self.book.contract(position.trade_data.symbol_id).pip();
            let open: Vec<Open> = self
                .book
                .positions
                .values()
                .filter_map(|p| {
                    Some(Open {
                        position: p.position_id,
                        label: Label::decode(p.trade_data.label.as_deref()?)?,
                        buy: is_buy(p.trade_data.trade_side),
                        entry: p.price?,
                        stop_loss: p.stop_loss,
                    })
                })
                .collect();
            for (id, stop) in plan::break_even_moves(&label, profit, &open, pip) {
                let take_profit = self.book.positions.get(&id).and_then(|p| p.take_profit);
                self.protect_position(id, Some(stop), take_profit, cx);
            }
        }
    }

    /// Closes the positions whose time stop is up (see [`super::plan::TimeStop`]). It runs with
    /// every reading of the profit, so a time that ran out while the app was closed is caught at
    /// the next start. Closing is never held back by the safety limits: they are for new orders.
    fn enforce_time_stops(&mut self, cx: &mut Context<Self>) {
        use super::plan::Label;
        // While a call is unanswered the account is not known well enough to close on its own.
        if self.status != Status::Ready || self.uncertain {
            return;
        }
        let now = crate::chart::now_ms();
        self.time_stops
            .retain(|id, _| self.book.positions.contains_key(id));
        let mut due = Vec::new();
        for position in self.book.positions.values() {
            let id = position.position_id;
            let Some(rule) = position
                .trade_data
                .label
                .as_deref()
                .and_then(Label::decode)
                .and_then(|l| l.time_stop)
            else {
                continue;
            };
            let Some(opened) = position.trade_data.open_timestamp else {
                continue;
            };
            let retry = self
                .time_stops
                .get(&id)
                .is_none_or(|at| at.elapsed() >= TIME_STOP_RETRY);
            if retry
                && !self.is_busy(Busy::Closing(id))
                && rule.due(opened, now, self.net_profit(id))
            {
                due.push((id, rule, position.trade_data.symbol_id));
            }
        }
        for (id, rule, symbol) in due {
            self.time_stops.insert(id, Instant::now());
            let name = self.book.name(symbol);
            self.tell(
                Notice::new(
                    Tone::Info,
                    "Time stop",
                    format!("Closing {name}, {}.", rule.describe()),
                ),
                cx,
            );
            self.close_position(id, None, cx);
        }
    }

    /// Does what a plan asked while the app was closed or cut off (see [`super::plan::catch_up`]),
    /// and says so once.
    fn catch_up(&mut self, missed: Vec<super::plan::Catch>, cx: &mut Context<Self>) {
        use super::plan::Catch;
        let (mut stops, mut cancels) = (0, 0);
        for step in missed {
            match step {
                Catch::Stop { position, stop } => {
                    let Some(open) = self.book.positions.get(&position) else {
                        continue;
                    };
                    let take_profit = open.take_profit;
                    stops += 1;
                    self.protect_position(position, Some(stop), take_profit, cx);
                }
                Catch::Cancel { order } => {
                    if self.book.orders.contains_key(&order) {
                        cancels += 1;
                        self.cancel_order(order, cx);
                    }
                }
            }
        }
        if stops + cancels == 0 {
            return;
        }
        let mut parts = Vec::new();
        if stops > 0 {
            parts.push(format!(
                "{stops} stop loss{} moved to the entry",
                if stops == 1 { "" } else { "es" }
            ));
        }
        if cancels > 0 {
            parts.push(format!(
                "{cancels} order{} of an OCO pair cancelled",
                if cancels == 1 { "" } else { "s" }
            ));
        }
        self.tell(
            Notice::new(
                Tone::Info,
                "Plan caught up",
                format!("While the app was away: {}.", parts.join(", ")),
            ),
            cx,
        );
    }

    /// Moves the stop loss of a position to its entry price, keeping the take profit.
    pub fn break_even(&mut self, position_id: i64, cx: &mut Context<Self>) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let Some(entry) = position.price else { return };
        let take_profit = position.take_profit;
        self.protect_position(position_id, Some(entry), take_profit, cx);
    }

    /// A market order of `volume` on a side.
    pub fn market(&mut self, symbol_id: i64, buy: bool, volume: i64, cx: &mut Context<Self>) {
        let side = if buy { TradeSide::Buy } else { TradeSide::Sell };
        self.place(NewOrderReq::market(symbol_id, side, volume), cx);
    }

    /// Closes a position, all of it or `volume` of it.
    pub fn close_position(
        &mut self,
        position_id: i64,
        volume: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let volume = volume.unwrap_or(position.trade_data.volume);
        self.trade(Busy::Closing(position_id), cx, move |trading| async move {
            trading.close_position(position_id, volume).await
        });
    }

    /// Closes every open position (optionally only those of a symbol).
    pub fn close_all(&mut self, symbol: Option<i64>, cx: &mut Context<Self>) {
        let ids: Vec<i64> = self
            .book
            .positions
            .values()
            .filter(|p| symbol.is_none_or(|s| p.trade_data.symbol_id == s))
            .map(|p| p.position_id)
            .collect();
        for id in ids {
            self.close_position(id, None, cx);
        }
    }

    /// Reverses a position: closes it and opens the same volume the other way.
    pub fn reverse_position(&mut self, position_id: i64, cx: &mut Context<Self>) {
        if self.busy.contains(&Busy::Closing(position_id)) {
            return;
        }
        let Some(position) = self.book.positions.get(&position_id).cloned() else {
            return;
        };
        let buy = !is_buy(position.trade_data.trade_side);
        let (symbol, volume) = (position.trade_data.symbol_id, position.trade_data.volume);
        // The position closes first: if the order that opens the other way would be refused, it is
        // better to know before anything is closed.
        let side = if buy { TradeSide::Buy } else { TradeSide::Sell };
        if let Verdict::Block(reason) =
            self.assess(&NewOrderReq::market(symbol, side, volume), false)
        {
            self.tell(
                Notice::new(
                    Tone::Warning,
                    "Reverse blocked by your safety settings",
                    reason,
                )
                .hint(Some("Nothing was closed.".to_owned())),
                cx,
            );
            return;
        }
        if !self.reversals.start(
            position_id,
            ReverseOrder {
                symbol,
                buy,
                volume,
            },
        ) {
            return;
        }
        self.close_position(position_id, None, cx);
    }

    pub fn cancel_order(&mut self, order_id: i64, cx: &mut Context<Self>) {
        self.trade(Busy::Cancelling(order_id), cx, move |trading| async move {
            trading.cancel_order(order_id).await
        });
    }

    /// Moves a working order to a new price (its limit or stop price).
    pub fn move_order(&mut self, order_id: i64, price: f64, cx: &mut Context<Self>) {
        let Some(order) = self.book.orders.get(&order_id) else {
            return;
        };
        let contract = self.book.contract(order.trade_data.symbol_id);
        let price = contract.round_price(price);
        let mut request = AmendOrderReq::new(order_id);
        if order.stop_price.is_some() && order.limit_price.is_none() {
            request.stop_price = Some(price);
        } else {
            request.limit_price = Some(price);
        }
        // The protection stays as it is.
        request.stop_loss = order.stop_loss;
        request.take_profit = order.take_profit;
        self.trade(Busy::Amending(order_id), cx, move |trading| async move {
            trading.amend_order(request).await
        });
    }

    /// Sets the stop loss or take profit of a working order.
    pub fn protect_order(
        &mut self,
        order_id: i64,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        let Some(order) = self.book.orders.get(&order_id) else {
            return;
        };
        let contract = self.book.contract(order.trade_data.symbol_id);
        let mut request = AmendOrderReq::new(order_id);
        request.limit_price = order.limit_price;
        request.stop_price = order.stop_price;
        request.stop_loss = stop_loss.map(|p| contract.round_price(p));
        request.take_profit = take_profit.map(|p| contract.round_price(p));
        self.trade(Busy::Amending(order_id), cx, move |trading| async move {
            trading.amend_order(request).await
        });
    }

    /// Turns the trailing of a position's stop loss on or off, keeping the levels as they are.
    pub fn trail_position(&mut self, position_id: i64, on: bool, cx: &mut Context<Self>) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let contract = self.book.contract(position.trade_data.symbol_id);
        let mut request = AmendPositionSlTpReq::new(position_id);
        request.stop_loss = position.stop_loss.map(|p| contract.round_price(p));
        request.take_profit = position.take_profit.map(|p| contract.round_price(p));
        request.trailing_stop_loss = Some(on);
        self.trade(Busy::Amending(position_id), cx, move |trading| async move {
            trading.amend_position_sl_tp(request).await
        });
    }

    /// Sets the stop loss and take profit of a position (both are sent, so neither is lost).
    pub fn protect_position(
        &mut self,
        position_id: i64,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        let Some(position) = self.book.positions.get(&position_id) else {
            return;
        };
        let contract = self.book.contract(position.trade_data.symbol_id);
        let mut request = AmendPositionSlTpReq::new(position_id);
        request.stop_loss = stop_loss.map(|p| contract.round_price(p));
        request.take_profit = take_profit.map(|p| contract.round_price(p));
        request.trailing_stop_loss = position.trailing_stop_loss;
        self.trade(Busy::Amending(position_id), cx, move |trading| async move {
            trading.amend_position_sl_tp(request).await
        });
    }
}

/// What the plans of the account missed while the app was away, read from the broker. It asks
/// for the recent orders only when something open belongs to a plan with a rule to keep, and
/// gives nothing when any call fails: the next connection tries again.
async fn missed_exits(
    account: &wyck_openapi::AccountClient,
    positions: &[wyck_openapi::account::Position],
    orders: &[wyck_openapi::account::Order],
    deals: &[wyck_openapi::account::Deal],
    now: i64,
) -> Vec<super::plan::Catch> {
    use super::plan::{self, Label, Open, Past};
    use wyck_openapi::account::OrderStatus;
    let label_of = |text: &Option<String>| text.as_deref().and_then(Label::decode);
    let rules = positions
        .iter()
        .filter_map(|p| label_of(&p.trade_data.label))
        .any(|l| l.break_even.is_some())
        || orders
            .iter()
            .filter_map(|o| label_of(&o.trade_data.label))
            .any(|l| l.oco);
    if !rules {
        return Vec::new();
    }
    let Ok((history, _)) = account
        .account_data()
        .orders(now - HISTORY_DAYS * 86_400_000, now)
        .await
    else {
        return Vec::new();
    };
    let mut past: Vec<Past> = history
        .iter()
        .filter(|o| o.status() == Some(OrderStatus::Filled))
        .filter_map(|o| {
            let label = label_of(&o.trade_data.label)?;
            (label.oco || label.break_even.is_some()).then(|| Past {
                label,
                buy: is_buy(o.trade_data.trade_side),
                filled: true,
                position: o.position_id,
                pip: 0.0,
            })
        })
        .collect();
    if past.is_empty() {
        return Vec::new();
    }
    let symbols: Vec<i64> = {
        let mut ids: Vec<i64> = history
            .iter()
            .filter(|o| {
                o.position_id
                    .is_some_and(|id| past.iter().any(|p| p.position == Some(id)))
            })
            .map(|o| o.trade_data.symbol_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let Ok(details) = account.market().symbol_details(&symbols).await else {
        return Vec::new();
    };
    let pip_of = |symbol: i64| {
        details
            .iter()
            .find(|s| s.symbol_id == symbol)
            .map(|s| Contract::from_symbol(s).pip())
    };
    for old in &mut past {
        let symbol = history
            .iter()
            .find(|o| o.position_id == old.position && old.position.is_some())
            .map(|o| o.trade_data.symbol_id);
        old.pip = symbol.and_then(pip_of).unwrap_or(0.0);
    }
    // A pip we could not read would put the offset and the tolerance at zero: leave those out.
    past.retain(|p| p.pip > 0.0);
    let mut profits: HashMap<i64, f64> = HashMap::new();
    for deal in deals {
        if let Some(profit) = deal.realized_pnl() {
            *profits.entry(deal.position_id).or_default() += profit;
        }
    }
    let open: Vec<Open> = positions
        .iter()
        .filter_map(|p| {
            Some(Open {
                position: p.position_id,
                label: label_of(&p.trade_data.label)?,
                buy: is_buy(p.trade_data.trade_side),
                entry: p.price?,
                stop_loss: p.stop_loss,
            })
        })
        .collect();
    let working: Vec<(i64, Label, bool)> = orders
        .iter()
        .filter(|o| o.status() == Some(OrderStatus::Accepted))
        .filter_map(|o| {
            Some((
                o.order_id,
                label_of(&o.trade_data.label)?,
                is_buy(o.trade_data.trade_side),
            ))
        })
        .collect();
    plan::catch_up(&past, &profits, &open, &working)
}

#[cfg(test)]
mod tests {
    use super::{ReverseOrder, ReverseTracker};
    use wyck_openapi::trading::ExecutionType;

    fn order() -> ReverseOrder {
        ReverseOrder {
            symbol: 7,
            buy: false,
            volume: 1_000,
        }
    }

    #[test]
    fn reverse_waits_for_both_close_acceptance_and_full_closure() {
        let mut tracker = ReverseTracker::default();
        assert!(tracker.start(11, order()));
        assert!(!tracker.start(11, order()));
        assert_eq!(
            tracker.acknowledged(11, Some(ExecutionType::OrderAccepted), Some(101)),
            None
        );
        assert_eq!(tracker.closed(11), Some(order()));
        assert_eq!(tracker.closed(11), None);

        assert!(tracker.start(12, order()));
        assert_eq!(tracker.closed(12), None);
        assert_eq!(
            tracker.acknowledged(12, Some(ExecutionType::OrderPartialFill), Some(102)),
            Some(order())
        );
    }

    #[test]
    fn rejected_or_uncertain_close_never_opens_the_reverse_order() {
        for kind in [Some(ExecutionType::OrderRejected), None] {
            let mut tracker = ReverseTracker::default();
            assert!(tracker.start(11, order()));
            assert_eq!(tracker.closed(11), None);
            assert_eq!(tracker.acknowledged(11, kind, None), None);
            assert_eq!(tracker.closed(11), None);
        }
    }

    #[test]
    fn a_later_close_error_cancels_only_its_reverse() {
        let mut tracker = ReverseTracker::default();
        assert!(tracker.start(11, order()));
        assert!(tracker.start(12, order()));
        assert_eq!(
            tracker.acknowledged(11, Some(ExecutionType::OrderAccepted), Some(101)),
            None
        );
        assert_eq!(
            tracker.acknowledged(12, Some(ExecutionType::OrderAccepted), Some(102)),
            None
        );
        tracker.cancel_related(None, Some(101));
        assert_eq!(tracker.closed(11), None);
        assert_eq!(tracker.closed(12), Some(order()));
    }
}
