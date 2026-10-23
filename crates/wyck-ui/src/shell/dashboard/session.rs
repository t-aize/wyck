//! Following the session of the account: its events, the catalog of symbols and the active symbol.

use super::view::quote_text;
use super::{
    Active, Conn, Dashboard, DashboardEvent, Load, Peek, Tick, failure_message, save_picture,
};

use super::catalog::{Catalog, Entry};
use crate::chart::chart::Chart;
use crate::chart::multichart::{MultiChartEvent, SymbolRef};
use crate::kit::toast;
use gpui::{Context, SharedString};
use std::sync::Arc;
use wyck_app::broker::session::{SessionEvent, SessionState};
use wyck_app::broker::{Error as ApiError, Event};
use wyck_app::lost::lost;
use wyck_app::market_data::live::{PEEK_OWNER, Wish};
use wyck_app::system::runtime;
use wyck_app::system::runtime::RecvError;
use wyck_domain::market::Spot;

impl Dashboard {
    pub(super) fn on_active_chart(
        &self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Chart, &mut Context<Chart>),
    ) {
        let chart = self.multi.read(cx).active_chart().clone();
        chart.update(cx, f);
    }

    /// Stops the session: no more reconnects, and the connection closes.
    pub(crate) fn stop(&self) {
        let session = self.session.clone();
        runtime::spawn(async move { session.stop().await });
    }

    /// Feeds the session's events into this view, for as long as it lives.
    pub(super) fn follow_session(&mut self, cx: &mut Context<Self>) {
        let mut events = self.session.events();
        if matches!(*self.session.state().borrow(), SessionState::Ready) {
            self.on_ready(cx);
        }
        cx.spawn(async move |this, cx| {
            loop {
                match events.recv().await {
                    Ok(event) => {
                        if this
                            .update(cx, |this, cx| this.on_event(event, cx))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => break,
                }
            }
        })
        .detach();
    }

    pub(super) fn on_event(&mut self, event: SessionEvent, cx: &mut Context<Self>) {
        match event {
            SessionEvent::Ready => self.on_ready(cx),
            SessionEvent::Data(Event::Spot(spot)) => {
                self.multi.update(cx, |multi, cx| multi.on_spot(&spot, cx));
                self.alerts.update(cx, |alerts, cx| {
                    alerts.on_spot(spot.symbol_id, spot.bid, spot.ask, cx);
                });
                self.trading.update(cx, |account, cx| {
                    account.on_event(&Event::Spot(spot.clone()), cx);
                });
                self.apply_spot(Spot::from(&spot), cx);
            }
            SessionEvent::Data(event) => {
                self.trading
                    .update(cx, |account, cx| account.on_event(&event, cx));
            }
            SessionEvent::Reconnecting { attempt, .. } => {
                tracing::debug!(attempt, "the session is reconnecting");
                self.conn = Conn::Reconnecting;
                cx.notify();
            }
            SessionEvent::Failed(error) => {
                tracing::warn!(%error, "the session failed");
                self.conn = Conn::Failed(failure_message(&error).into());
                cx.notify();
            }
            _ => {}
        }
    }

    pub(super) fn on_ready(&mut self, cx: &mut Context<Self>) {
        self.conn = Conn::Ready;
        self.multi
            .update(cx, crate::chart::multichart::MultiChart::on_ready);
        self.trading
            .update(cx, wyck_app::account::Account::on_ready);
        self.replay.update(cx, wyck_app::replay::Replay::on_ready);
        if !self.catalog_requested {
            self.catalog_requested = true;
            self.load_catalog(cx);
        }
        cx.notify();
    }

    pub(super) fn load_catalog(&mut self, cx: &mut Context<Self>) {
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let loaded = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                let market = account.market();
                let symbols = market.symbols().await?;
                let categories = market.symbol_categories().await?;
                let classes = market.asset_classes().await?;
                let assets = market.assets().await?;
                Ok::<_, ApiError>(Catalog::build(symbols, &categories, &classes, &assets))
            })
            .await;
            lost(
                "session: load_catalog",
                this.update(cx, |this, cx| {
                    match loaded {
                        Ok(Ok(catalog)) => this.catalog_loaded(&Arc::new(catalog), cx),
                        Ok(Err(error)) => {
                            tracing::warn!(%error, "could not load the symbol list");
                            this.catalog = Load::Failed(error.to_string().into());
                            // Ask again the next time the session comes back.
                            this.catalog_requested = false;
                        }
                        Err(_) => {
                            this.catalog = Load::Failed("the background runtime stopped".into());
                            this.catalog_requested = false;
                        }
                    }
                    cx.notify();
                }),
            );
        })
        .detach();
    }

    pub(super) fn catalog_loaded(&mut self, catalog: &Arc<Catalog>, cx: &mut Context<Self>) {
        let remembered = self
            .initial_symbol
            .take()
            .and_then(|name| catalog.by_name(&name).cloned());
        let start = remembered
            .or_else(|| catalog.by_name("EURUSD").cloned())
            .or_else(|| catalog.entry(0).cloned());
        self.catalog = Load::Ready(Arc::clone(catalog));
        let entries: Vec<&Entry> = (0..catalog.total())
            .filter_map(|i| catalog.entry(i))
            .collect();
        let names = entries.iter().map(|e| (e.id, e.name.clone())).collect();
        let quotes = entries
            .iter()
            .filter_map(|e| Some((e.id, e.quote.clone()?)))
            .collect();
        let quote_assets = entries
            .iter()
            .filter_map(|e| Some((e.id, e.quote_asset?)))
            .collect();
        self.trading.update(cx, |account, cx| {
            account.set_symbols(names, quotes, quote_assets, cx);
        });
        let ids = entries
            .iter()
            .map(|e| {
                (
                    wyck_domain::indicators::custom::other::key_of(&e.name),
                    e.id,
                )
            })
            .collect();
        self.multi
            .update(cx, |multi, cx| multi.set_symbol_ids(ids, cx));
        // Each chart gets the symbol it was saved with, or the one to start on.
        let wanted = self.multi.read(cx).wanted_symbols(cx);
        let linked = self.multi.read(cx).sync().symbol;
        for (index, name) in wanted {
            let entry = name
                .as_deref()
                .filter(|_| !linked)
                .and_then(|name| catalog.by_name(name).cloned())
                .or_else(|| start.clone());
            if let Some(entry) = entry {
                let symbol = self.symbol_ref(&entry);
                self.ensure_details(entry.id, cx);
                self.multi
                    .update(cx, |multi, cx| multi.restore_symbol(index, symbol, cx));
            }
        }
        self.refresh_active(cx);
    }

    /// Shows `entry` on the chart the picker was opened for (every chart when the symbol is
    /// linked), and (when the user picked it) tells the app to remember it.
    pub(super) fn select(&mut self, entry: &Entry, remember: bool, cx: &mut Context<Self>) {
        let target = self
            .picker_target
            .take()
            .unwrap_or_else(|| self.multi.read(cx).active_index());
        if remember {
            cx.emit(DashboardEvent::SymbolChosen(entry.name.clone()));
        }
        let symbol = self.symbol_ref(entry);
        self.multi
            .update(cx, |multi, cx| multi.set_symbol(target, &symbol, cx));
        self.ensure_details(entry.id, cx);
        self.refresh_active(cx);
    }

    /// A symbol of the list, with the decimals the broker gave for it (5 until it says).
    pub(super) fn symbol_ref(&self, entry: &Entry) -> SymbolRef {
        let (digits, pip_position) = match self.details.get(&entry.id) {
            Some(super::details::Detail::Ready(symbol)) => (
                u32::try_from(symbol.digits).unwrap_or(5),
                Some(symbol.pip_position),
            ),
            _ => (5, None),
        };
        SymbolRef {
            id: entry.id,
            name: SharedString::from(entry.name.clone()),
            digits,
            pip_position,
        }
    }

    /// Asks the broker how a symbol is quoted, once, and tells the charts.
    pub(super) fn ensure_details(&mut self, id: i64, cx: &mut Context<Self>) {
        if matches!(
            self.details.get(&id),
            Some(super::details::Detail::Ready(_) | super::details::Detail::Loading)
        ) {
            return;
        }
        self.details.insert(id, super::details::Detail::Loading);
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let fetched = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                let details = account.market().symbol_details(&[id]).await?;
                details
                    .into_iter()
                    .find(|d| d.symbol_id == id)
                    .ok_or_else(|| ApiError::Protocol("the broker sent no details".into()))
            })
            .await;
            lost(
                "session: ensure_details",
                this.update(cx, |this, cx| match fetched {
                    Ok(Ok(symbol)) => {
                        let digits = u32::try_from(symbol.digits).unwrap_or(5);
                        let pip_position = symbol.pip_position;
                        let hours = wyck_domain::market::TradingHours::from_symbol(&symbol);
                        this.multi
                            .update(cx, |multi, cx| multi.set_hours(id, hours, cx));
                        this.details
                            .insert(id, super::details::Detail::Ready(symbol));
                        this.multi.update(cx, |multi, cx| {
                            multi.set_quote_details(id, digits, pip_position, cx);
                        });
                        this.refresh_active(cx);
                    }
                    Ok(Err(error)) => {
                        tracing::warn!(%error, symbol_id = id, "could not load a symbol's details");
                        this.details
                            .insert(id, super::details::Detail::Failed(error.to_string().into()));
                    }
                    Err(_) => {
                        this.details.remove(&id);
                    }
                }),
            );
        })
        .detach();
    }

    /// The header follows the active chart: its symbol, how it is quoted, its last price.
    pub(super) fn refresh_active(&mut self, cx: &mut Context<Self>) {
        let Some(symbol) = self.multi.read(cx).active_symbol(cx) else {
            self.active = None;
            cx.notify();
            return;
        };
        let entry = match &self.catalog {
            Load::Ready(catalog) => catalog.by_name(&symbol.name).cloned(),
            _ => None,
        };
        let Some(entry) = entry else {
            // The symbol is not known yet, but the chart buttons of the header still follow the chart.
            cx.notify();
            return;
        };
        let (digits, pip_position) = match self.details.get(&symbol.id) {
            Some(super::details::Detail::Ready(details)) => (
                u32::try_from(details.digits).unwrap_or(5),
                details.pip_position,
            ),
            _ => (5, 4),
        };
        if self.active.as_ref().map(|a| a.entry.id) != Some(symbol.id) {
            self.tick = None;
        }
        self.quote = self.quotes.get(&symbol.id).copied().unwrap_or_default();
        self.active = Some(Active {
            entry,
            digits,
            pip_position,
        });
        cx.notify();
    }

    pub(super) fn on_multi_event(&mut self, event: &MultiChartEvent, cx: &mut Context<Self>) {
        match event {
            MultiChartEvent::PickSymbol(index) => {
                self.picker_target = Some(*index);
                self.pending_picker = true;
                cx.notify();
            }
            MultiChartEvent::ActiveChanged => self.refresh_active(cx),
            MultiChartEvent::Picture(png, name) => save_picture(png.clone(), name.clone(), cx),
            MultiChartEvent::PictureFailed(error) => {
                toast::Toast::error(
                    "Could not take the picture",
                    "The chart could not be turned into an image.",
                )
                .hint("Try again. If it keeps failing, copy the details into a bug report.")
                .details(error.clone())
                .show(cx);
            }
            MultiChartEvent::Action(symbol, action) => self.on_chart_action(symbol, action, cx),
            MultiChartEvent::LineMoved(id, price) => self.on_line_moved(*id, *price, cx),
            MultiChartEvent::LineClosed(id) => self.on_line_closed(*id, cx),
            MultiChartEvent::IndicatorEditor(request) => {
                self.pending
                    .push(super::trade::Pending::Editor(request.clone()));
                cx.notify();
            }
            MultiChartEvent::ReplayBlocked => {
                self.replay_blocks(cx);
            }
            MultiChartEvent::ReplayFailed(message) => {
                toast::Toast::error("The replay stopped", message.clone()).show(cx);
            }
        }
    }

    pub(super) fn apply_spot(&mut self, spot: Spot, cx: &mut Context<Self>) {
        let quote = self.quotes.entry(spot.symbol_id).or_default();
        quote.bid = spot.bid.or(quote.bid);
        quote.ask = spot.ask.or(quote.ask);
        // A symbol the picker is showing (not the one being followed) has its own quote.
        if let Some(peek) = self.peek.as_mut().filter(|peek| peek.id == spot.symbol_id) {
            peek.quote.bid = spot.bid.or(peek.quote.bid);
            peek.quote.ask = spot.ask.or(peek.quote.ask);
            cx.notify();
        }
        if self.active.as_ref().map(|a| a.entry.id) != Some(spot.symbol_id) {
            return;
        }
        let previous = self.quote.bid;
        // An event only carries what changed.
        self.quote.bid = spot.bid.or(self.quote.bid);
        self.quote.ask = spot.ask.or(self.quote.ask);
        if let (Some(before), Some(now)) = (previous, self.quote.bid) {
            if now > before {
                self.tick = Some(Tick::Up);
            } else if now < before {
                self.tick = Some(Tick::Down);
            }
        }
        cx.notify();
    }

    /// The bid, the ask and the spread in pips, as the followed symbol quotes them.
    pub(super) fn price_text(&self) -> Option<super::details::Live> {
        let active = self.active.as_ref()?;
        quote_text(self.quote, active.digits, active.pip_position)
    }

    /// The same for the symbol the picker highlights, once its details say how it is quoted.
    pub(super) fn peek_text(&self, id: i64) -> Option<super::details::Live> {
        let peek = self.peek.as_ref().filter(|peek| peek.id == id)?;
        let Some(super::details::Detail::Ready(symbol)) = self.details.get(&id) else {
            return None;
        };
        let digits = u32::try_from(symbol.digits).unwrap_or(5);
        quote_text(peek.quote, digits, symbol.pip_position)
    }

    /// Stops following the price of the symbol the picker was showing.
    pub(super) fn drop_peek(&mut self) {
        if self.peek.take().is_some() {
            self.hub.set(PEEK_OWNER, None);
        }
    }

    /// Follows the price of the symbol the picker shows.
    pub(crate) fn peek_at(&mut self, id: i64) {
        self.peek = Some(Peek {
            id,
            quote: self.quotes.get(&id).copied().unwrap_or_default(),
        });
        self.hub.set(PEEK_OWNER, Some(Wish::spots([id])));
    }
}
