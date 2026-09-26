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

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui::{Context, EventEmitter};
use wyck_openapi::account::TradeSide;
use wyck_openapi::market::PRICE_SCALE;
use wyck_openapi::session::Session;
use wyck_openapi::trading::{AmendOrderReq, AmendPositionSlTpReq, NewOrderReq};
use wyck_openapi::{Event, OpenApiError, Result as ApiResult};

use super::book::{AccountBook, Notice, Tone, explain, is_buy};
use super::math::{self, Contract, Link, Summary};
use crate::app::chart::LiveHub;
use crate::app::chart::live::{ACCOUNT_OWNER, Wish};
use crate::app::{runtime, toast};

/// How long the recent history reaches back.
const HISTORY_DAYS: i64 = 7;
/// How often the profit is asked while positions are open.
const PNL_EVERY: Duration = Duration::from_secs(2);

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
}

pub struct Account {
    session: Session,
    hub: Rc<LiveHub>,
    pub book: AccountBook,
    /// The last bid and ask of every symbol seen, raw.
    quotes: HashMap<i64, (Option<i64>, Option<i64>)>,
    pub status: Status,
    busy: HashSet<Busy>,
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
    result.unwrap_or(Err(OpenApiError::Closed))
}

impl Account {
    pub fn new(session: Session, hub: Rc<LiveHub>, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PNL_EVERY).await;
                let alive = this.update(cx, |this, cx| {
                    if this.status == Status::Ready && !this.book.positions.is_empty() {
                        this.refresh_pnl(cx);
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
                let client = session.client().ok_or(OpenApiError::Closed)?;
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
        };
        toast::show(cx, kind, notice.title, notice.message);
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
                let client = session.client().ok_or(OpenApiError::Closed)?;
                let account = client.account(session.account_id());
                let data = account.account_data();
                let trader = data.trader().await?;
                let (positions, orders) = data.open_positions_and_orders(false).await?;
                let now = crate::app::chart::now_ms();
                let deals = data
                    .deals(now - HISTORY_DAYS * 86_400_000, now, Some(500))
                    .await
                    .map(|(deals, _)| deals)
                    .unwrap_or_default();
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
                Ok::<_, OpenApiError>((trader, positions, orders, deals, currency))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                match flatten(loaded) {
                    Ok((trader, positions, orders, mut deals, currency)) => {
                        this.book.trader = Some(trader);
                        this.book.currency = currency;
                        this.book.reconcile(positions, orders);
                        deals.sort_by_key(|d| std::cmp::Reverse(d.execution_timestamp));
                        this.book.deals = deals;
                        this.status = Status::Ready;
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
                let client = session.client().ok_or(OpenApiError::Closed)?;
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
                let client = session.client().ok_or(OpenApiError::Closed)?;
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
                let client = session.client().ok_or(OpenApiError::Closed)?;
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
                let applied = self.book.apply(execution);
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
            }
            Event::OrderError(error) => {
                self.tell(
                    Notice {
                        tone: Tone::Error,
                        title: "Order refused".into(),
                        message: error
                            .description
                            .clone()
                            .unwrap_or_else(|| explain(&error.error_code)),
                    },
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
        cx.notify();
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                let client = session.client().ok_or(OpenApiError::Closed)?;
                call(client.account(session.account_id()).trading()).await
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                this.busy.remove(&busy);
                match flatten(result) {
                    // The first execution event answers the request and goes only to it; the
                    // ones after it (a fill after the acceptance) come on the event stream.
                    Ok(event) => this.on_event(&Event::Execution(Box::new(event)), cx),
                    Err(error) => {
                        let message = match error.code() {
                            Some(code) => explain(code),
                            None => error.to_string(),
                        };
                        this.tell(
                            Notice {
                                tone: Tone::Error,
                                title: "The order did not go through".into(),
                                message,
                            },
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
    pub fn place(&mut self, mut order: NewOrderReq, cx: &mut Context<Self>) {
        order.label = Some("wyck".into());
        if let Err(error) = order.validate() {
            self.tell(
                Notice {
                    tone: Tone::Error,
                    title: "The order is not complete".into(),
                    message: error.to_string(),
                },
                cx,
            );
            return;
        }
        self.trade(Busy::Placing, cx, move |trading| async move {
            trading.new_order(order).await
        });
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
        let Some(position) = self.book.positions.get(&position_id).cloned() else {
            return;
        };
        let buy = !is_buy(position.trade_data.trade_side);
        let (symbol, volume) = (position.trade_data.symbol_id, position.trade_data.volume);
        self.close_position(position_id, None, cx);
        self.market(symbol, buy, volume, cx);
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
