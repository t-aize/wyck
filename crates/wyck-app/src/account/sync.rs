//! Keeping the account in step with the broker: what is loaded when the session is ready, and what each event changes.

use super::orders::missed_exits;
use super::reverse::ReverseOrder;
use super::{Account, Conversion, HISTORY_DAYS, Notice, Status, Tone, flatten, refusal};
use crate::broker::{Error as ApiError, Event};
use crate::lost::lost;
use crate::system::runtime;
use gpui::Context;
use std::collections::HashMap;
use wyck_trading::ExecutionType;
use wyck_trading::PositionStatus;
use wyck_trading::math::{Contract, Link};

impl Account {
    /// Asks the server once how the currency of `symbol_id` converts into the deposit one, and
    /// follows the prices of the symbols that convert it.
    pub(super) fn ensure_rate(&mut self, symbol_id: i64, cx: &mut Context<Self>) {
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
            lost(
                "account/sync: ensure_rate",
                this.update(cx, |this, cx| {
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
                }),
            );
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
    pub(super) fn followed(&self) -> Vec<i64> {
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
                let now = crate::market_data::now_ms();
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
            lost("account/sync: on_ready", this.update(cx, |this, cx| {
                if epoch != this.epoch {
                    return;
                }
                match flatten(loaded) {
                    Ok((trader, positions, orders, mut deals, currency, missed)) => {
                        this.book.trader = Some(trader);
                        this.book.currency = currency;
                        this.book.reconcile(positions, orders);
                        #[expect(
                            clippy::needless_collect,
                            reason = "`closed` needs the reversals mutably, so the ids are collected before they are asked for"
                        )]
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
                        this.book.set_deals(deals);
                        this.status = Status::Ready;
                        // The account was read again: whatever a silent call did is in it now.
                        this.uncertain = false;
                        this.settle_in_doubt(cx);
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
            }));
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
            lost(
                "account/sync: ensure_contract",
                this.update(cx, |this, cx| match flatten(fetched) {
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
                }),
            );
        })
        .detach();
    }

    pub(super) fn refresh_pnl(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        let epoch = self.epoch;
        cx.spawn(async move |this, cx| {
            let answer = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                account.account_data().unrealized_pnl().await
            })
            .await;
            lost(
                "account/sync: refresh_pnl",
                this.update(cx, |this, cx| {
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
                }),
            );
        })
        .detach();
    }

    pub(super) fn refresh_trader(&mut self, cx: &mut Context<Self>) {
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
                lost(
                    "account/sync: refresh_trader",
                    this.update(cx, |this, cx| {
                        this.book.trader = Some(trader);
                        cx.notify();
                    }),
                );
            }
        })
        .detach();
    }

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
                // An event that names the id of an order in doubt answers the doubt.
                if let Some(id) = execution
                    .order
                    .as_ref()
                    .and_then(|o| o.client_order_id.as_deref())
                {
                    self.in_doubt.retain(|d| d.client_order_id != id);
                }
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
                if let Some(outcome) = applied.outcome {
                    Self::tell(super::notice::of(outcome), cx);
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
                Self::tell(
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
                Self::tell(
                    Notice::new(
                        Tone::Warning,
                        "Margin call",
                        "The account's margin level reached a margin call threshold.",
                    ),
                    cx,
                );
            }
            _ => {}
        }
    }
}
