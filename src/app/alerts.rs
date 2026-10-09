//! Alerts: "tell me when EURUSD crosses 1.0850", "when the RSI of the hourly chart crosses 70",
//! "when the price touches this trend line".
//!
//! An alert watches a value (the price, one plot of an indicator), compares it with a level, a
//! drawing or another value, and fires when its condition comes true (see [`model`]). How often
//! it may fire, when it stops, and how it is judged are on the alert itself. The rules that decide
//! are plain functions ([`eval`]), tested on their own.
//!
//! The price is judged on every tick. What needs bars (an indicator, a bar close, a change of
//! direction) is judged on the bars of the alert's timeframe, which are read again every few
//! seconds, so it works with no chart open; the delay is that of the reading.
//!
//! Alerts are saved per account and come back at the next start. [`Alerts`] is the entity that
//! feeds them, saves them, and tells the app when one fires.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use crate::domain::chart::Timeframe;
use crate::domain::chart::data::Series;
use crate::domain::indicators as study;
use crate::domain::indicators::ValueFormat;
use crate::infra::ctrader::market::{Bar, PRICE_SCALE};
use crate::infra::ctrader::session::Session;
use crate::infra::storage::DocumentStore;
use gpui::{App, Context, Entity, EventEmitter};

use crate::app::workspace::Saver;
use crate::infra::platform::runtime;
use crate::ui::features::chart::drawing::Drawings;
use crate::ui::features::chart::live::{LiveHub, Wish};
use crate::ui::features::chart::load;
use crate::ui::features::chart::load::Loaded;
use crate::ui::features::chart::now_ms;

pub mod eval;
pub mod model;
pub mod sound;

use self::eval::{Judged, Reading};
pub use self::model::{Alert, AlertBook, Condition, Source, Trigger};

const DOCUMENT: &str = "alerts";
/// The owner of the alerts' price subscriptions in the live hub.
pub const ALERTS_OWNER: u64 = u64::MAX - 3;
/// How often the bars an alert needs are read again.
const POLL: Duration = Duration::from_secs(15);

pub enum AlertsEvent {
    Changed,
    /// An alert fired, with what it says.
    Fired(Box<Alert>, String),
    /// An alert stopped watching because its time was up.
    Expired(Box<Alert>),
}

/// What the account makes or loses right now, in its money: the numbers a profit alert reads.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Profits {
    /// Everything open; `None` until the account is read.
    pub account: Option<f64>,
    /// The sum of the positions of each symbol.
    pub symbols: HashMap<i64, f64>,
    /// Each open position.
    pub positions: HashMap<i64, f64>,
}

impl Profits {
    /// The profit an alert follows. A symbol with nothing open makes nothing; a position that is
    /// closed has no profit to read, so its alert waits.
    pub fn of(&self, scope: &model::PnlScope, symbol_id: i64) -> Option<f64> {
        match scope {
            model::PnlScope::Account => self.account,
            model::PnlScope::Symbol => self
                .account
                .map(|_| self.symbols.get(&symbol_id).copied().unwrap_or(0.0)),
            model::PnlScope::Position { id } => self.positions.get(id).copied(),
        }
    }
}

/// The bars of one symbol and timeframe, as last read.
struct Fetched {
    bars: Vec<Bar>,
    at: i64,
}

/// The live alerts: fed the prices, saved, and heard.
pub struct Alerts {
    book: AlertBook,
    saver: Saver<AlertBook>,
    hub: Rc<LiveHub>,
    session: Session,
    drawings: Entity<Drawings>,
    /// Decimals of each symbol, for the texts.
    pub digits: HashMap<i64, u32>,
    /// The size of a pip of each symbol, for the alerts on a spread.
    pips: HashMap<i64, f64>,
    /// The last bid and ask of each symbol, real.
    quotes: HashMap<i64, (Option<f64>, Option<f64>)>,
    /// What each alert saw last, to tell a crossing. Not saved.
    last: HashMap<u64, Reading>,
    /// The values a move is judged on, as (time, value).
    moves: HashMap<u64, Vec<(i64, f64)>>,
    bars: HashMap<(i64, String), Fetched>,
    loading: HashSet<(i64, String)>,
    _poll: gpui::Task<()>,
}

impl EventEmitter<AlertsEvent> for Alerts {}

impl Alerts {
    pub fn new(
        store: DocumentStore,
        hub: Rc<LiveHub>,
        session: Session,
        drawings: Entity<Drawings>,
        cx: &mut Context<Self>,
    ) -> Self {
        let book = store.load_or_default::<AlertBook>(DOCUMENT).normalized();
        cx.on_app_quit(|this, _cx| {
            this.saver.flush();
            async {}
        })
        .detach();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        });
        let alerts = Self {
            book,
            saver: Saver::new(store, DOCUMENT),
            hub,
            session,
            drawings,
            digits: HashMap::new(),
            pips: HashMap::new(),
            quotes: HashMap::new(),
            last: HashMap::new(),
            moves: HashMap::new(),
            bars: HashMap::new(),
            loading: HashSet::new(),
            _poll: poll,
        };
        alerts.follow();
        alerts
    }

    pub fn book(&self) -> &AlertBook {
        &self.book
    }

    fn follow(&self) {
        self.hub
            .set(ALERTS_OWNER, Some(Wish::spots(self.book.watched())));
    }

    fn digits_of(&self, symbol_id: i64) -> u32 {
        self.digits.get(&symbol_id).copied().unwrap_or(5)
    }

    /// The size of a pip of a symbol: the one read from its contract, or worked out from its
    /// decimals until then.
    pub fn pip_of(&self, symbol_id: i64) -> f64 {
        self.pips
            .get(&symbol_id)
            .copied()
            .unwrap_or_else(|| model::pip_from_digits(self.digits_of(symbol_id)))
    }

    /// Every symbol an alert is on, watching or not: the ones whose decimals the alerts need.
    pub fn symbols(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .book
            .alerts
            .iter()
            .map(|a| a.symbol_id)
            .filter(|id| *id > 0)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Learns how a symbol is written and how big its pip is, from its contract. Alerts are saved
    /// without them, so a symbol nobody opened since the start would show five decimals.
    /// Returns whether anything changed.
    pub fn learn(&mut self, symbol_id: i64, digits: u32, pip: f64, cx: &mut Context<Self>) -> bool {
        let changed = self.digits.insert(symbol_id, digits) != Some(digits)
            || self.pips.insert(symbol_id, pip) != Some(pip);
        if changed {
            cx.notify();
        }
        changed
    }

    /// Sets the pip of a symbol the alert editor shows.
    pub fn set_pip(&mut self, symbol_id: i64, pip: f64) {
        self.pips.insert(symbol_id, pip);
    }

    /// Changes the alerts, saves them and tells the views.
    pub fn edit<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut AlertBook) -> R,
    ) -> R {
        let result = change(&mut self.book);
        // What an alert saw belongs to how it was set: it starts again.
        self.last.clear();
        self.moves.clear();
        self.saver.schedule(self.book.clone());
        self.follow();
        cx.emit(AlertsEvent::Changed);
        cx.notify();
        // What needs bars is read at once, so a new alert does not wait for the next round.
        self.poll(cx);
        result
    }

    /// Silences an alert until `until` (Unix milliseconds).
    pub fn snooze(&mut self, id: u64, until: i64, cx: &mut Context<Self>) {
        self.edit(cx, |book| {
            if let Some(alert) = book.get_mut(id) {
                alert.snoozed_until = Some(until);
            }
        });
    }

    /// Whether an alert is being judged now: on, not snoozed, not out of time.
    fn live(alert: &Alert, now: i64) -> bool {
        alert.active && !alert.is_snoozed(now) && !alert.is_expired(now)
    }

    // ---- prices ----

    /// A price event: judges the alerts that follow the price.
    pub fn on_spot(
        &mut self,
        symbol_id: i64,
        bid: Option<i64>,
        ask: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        let real = |raw: Option<i64>| raw.map(|r| r as f64 / PRICE_SCALE as f64);
        let entry = self.quotes.entry(symbol_id).or_insert((None, None));
        entry.0 = real(bid).or(entry.0);
        entry.1 = real(ask).or(entry.1);
        let (bid, ask) = *entry;
        let now = now_ms();
        let candidates: Vec<Alert> = self
            .book
            .alerts
            .iter()
            .filter(|a| a.symbol_id == symbol_id && Self::live(a, now) && !a.needs_bars())
            .cloned()
            .collect();
        let mut hits: Vec<(u64, f64, Option<i64>)> = Vec::new();
        for alert in candidates {
            let value = match &alert.source {
                Source::Price { price } => price.of(bid, ask),
                Source::Spread { pip } => bid
                    .zip(ask)
                    .map(|(bid, ask)| eval::spread_pips(bid, ask, *pip)),
                _ => None,
            };
            let Some(value) = value else {
                continue;
            };
            let Some((level, upper)) = self.level_of(&alert, bid, ask, now, cx) else {
                continue;
            };
            let reading = Reading {
                value,
                level,
                upper: upper.filter(|_| alert.condition.is_zone()),
            };
            if let Some(bar) = self.judge(&alert, reading, now) {
                hits.push((alert.id, value, bar));
            }
        }
        for (id, value, bar) in hits {
            self.fire(id, value, bar, cx);
        }
    }

    /// Judges an alert on a new reading of its value. `Some(bar)` when it fires, with the bar
    /// (its start time) it fired on, for the triggers by bar.
    fn judge(&mut self, alert: &Alert, reading: Reading, now: i64) -> Option<Option<i64>> {
        let before = self.last.insert(alert.id, reading);
        let bar = Timeframe::from_code(&alert.timeframe)
            .and_then(Timeframe::bar_ms)
            .map(|ms| now.div_euclid(ms) * ms);
        let hit = if alert.condition == Condition::MovesBy {
            let history = self.moves.entry(alert.id).or_default();
            history.push((now, reading.value));
            let window = i64::from(alert.minutes.max(1)) * 60_000;
            history.retain(|(at, _)| now - at <= window);
            eval::moved(history, window, alert.amount)
        } else {
            before.is_some_and(|before| eval::fired(alert.condition, &before, &reading))
        };
        let by_bar = alert.trigger == Trigger::OncePerBar && bar.is_some() && alert.bar_key == bar;
        if hit && !by_bar {
            self.moves.remove(&alert.id);
            Some(bar)
        } else {
            None
        }
    }

    // ---- profits ----

    /// Whether an alert waits for a profit, so the account need not be read for nothing.
    pub fn watches_profit(&self) -> bool {
        self.book
            .alerts
            .iter()
            .any(|a| a.active && matches!(a.source, Source::Pnl { .. }))
    }

    /// The profits of the account changed: judges the alerts on a profit.
    pub fn on_profit(&mut self, profits: &Profits, cx: &mut Context<Self>) {
        let now = now_ms();
        let candidates: Vec<Alert> = self
            .book
            .alerts
            .iter()
            .filter(|a| matches!(a.source, Source::Pnl { .. }) && Self::live(a, now))
            .cloned()
            .collect();
        let mut hits: Vec<(u64, f64, Option<i64>)> = Vec::new();
        for alert in candidates {
            let Source::Pnl { scope } = &alert.source else {
                continue;
            };
            let Some(value) = profits.of(scope, alert.symbol_id) else {
                continue;
            };
            let reading = Reading {
                value,
                level: alert.price,
                upper: alert.condition.is_zone().then_some(alert.upper),
            };
            if let Some(bar) = self.judge(&alert, reading, now) {
                hits.push((alert.id, value, bar));
            }
        }
        for (id, value, bar) in hits {
            self.fire(id, value, bar, cx);
        }
    }

    /// The level an alert compares with now: its own number, the price of another kind, or where
    /// a drawing is, and the other side of a zone.
    fn level_of(
        &self,
        alert: &Alert,
        bid: Option<f64>,
        ask: Option<f64>,
        now: i64,
        cx: &App,
    ) -> Option<(f64, Option<f64>)> {
        match &alert.versus {
            None => Some((alert.price, Some(alert.upper))),
            Some(Source::Price { price }) => price.of(bid, ask).map(|p| (p, None)),
            Some(Source::Drawing { id }) => self.drawing_level(alert, *id, now, cx),
            Some(Source::Indicator { .. } | Source::Spread { .. } | Source::Pnl { .. }) => None,
        }
    }

    fn drawing_level(
        &self,
        alert: &Alert,
        id: u64,
        time: i64,
        cx: &App,
    ) -> Option<(f64, Option<f64>)> {
        let drawings = self.drawings.read(cx);
        let drawing = drawings.book().get(&alert.symbol, id)?;
        // A drawing keeps raw prices.
        let scale = PRICE_SCALE as f64;
        eval::drawing_level(drawing.tool, &drawing.points, time)
            .map(|(level, upper)| (level / scale, upper.map(|u| u / scale)))
    }

    // ---- bars ----

    /// Every few seconds: alerts out of time stop, and the bars the alerts need are read again.
    fn poll(&mut self, cx: &mut Context<Self>) {
        let now = now_ms();
        let gone = self.book.expire(now);
        if !gone.is_empty() {
            self.saver.schedule(self.book.clone());
            self.follow();
            for id in gone {
                if let Some(alert) = self.book.get(id).cloned() {
                    cx.emit(AlertsEvent::Expired(Box::new(alert)));
                }
            }
            cx.emit(AlertsEvent::Changed);
            cx.notify();
        }
        let wanted: HashSet<(i64, String)> = self
            .book
            .alerts
            .iter()
            .filter(|a| Self::live(a, now) && a.needs_bars())
            .map(|a| (a.symbol_id, a.timeframe.clone()))
            .collect();
        // What nothing waits for any more is forgotten.
        self.bars.retain(|key, _| wanted.contains(key));
        for key in wanted {
            let fresh = self
                .bars
                .get(&key)
                .is_some_and(|f| now - f.at < POLL.as_millis() as i64 - 1_000);
            if fresh || self.loading.contains(&key) {
                continue;
            }
            let Some(timeframe) = Timeframe::from_code(&key.1).filter(|t| t.bar_ms().is_some())
            else {
                continue;
            };
            self.loading.insert(key.clone());
            let session = self.session.clone();
            let symbol = key.0;
            cx.spawn(async move |this, cx| {
                let loaded = runtime::spawn(async move {
                    load::initial(&session, symbol, timeframe, now_ms()).await
                })
                .await;
                let _ = this.update(cx, |this, cx| {
                    this.loading.remove(&key);
                    if let Ok(Ok(Loaded::Bars(bars) | Loaded::Grouped { bars, .. })) = loaded {
                        this.bars
                            .insert(key.clone(), Fetched { bars, at: now_ms() });
                        this.judge_bars(&key, cx);
                    }
                });
            })
            .detach();
        }
    }

    /// One plot of an indicator over the bars, in the units of the price where it is one.
    fn plot_of(config: &study::StudyConfig, plot: usize, bars: &[Bar]) -> Option<Vec<f64>> {
        if config.is_script() || config.kind == study::StudyKind::VolumeProfile {
            return None;
        }
        let input = crate::domain::chart::display::study_input(
            &Series::Bars(bars.to_vec()),
            crate::domain::chart::Zone::Utc,
        );
        let output = study::compute(config, &input);
        let values = output.plots.get(plot)?.values.clone();
        let scale = if matches!(config.value_format(), ValueFormat::Price) {
            PRICE_SCALE as f64
        } else {
            1.0
        };
        Some(values.into_iter().map(|v| v / scale).collect())
    }

    /// The series a source has over the bars.
    fn series_of(
        &self,
        alert: &Alert,
        source: &Source,
        bars: &[Bar],
        cx: &App,
    ) -> Option<Vec<f64>> {
        match source {
            Source::Price { .. } => Some(
                bars.iter()
                    .map(|b| b.close as f64 / PRICE_SCALE as f64)
                    .collect(),
            ),
            Source::Indicator { study, plot } => Self::plot_of(study, *plot, bars),
            Source::Drawing { id } => Some(
                bars.iter()
                    .map(|b| {
                        self.drawing_level(alert, *id, b.time_ms, cx)
                            .map_or(f64::NAN, |(level, _)| level)
                    })
                    .collect(),
            ),
            // Judged on every tick or on every reading of the account, never on bars.
            Source::Spread { .. } | Source::Pnl { .. } => None,
        }
    }

    /// Judges the alerts of a symbol and timeframe on the bars just read.
    fn judge_bars(&mut self, key: &(i64, String), cx: &mut Context<Self>) {
        let now = now_ms();
        let Some(bars) = self.bars.get(key).map(|f| f.bars.clone()) else {
            return;
        };
        let alerts: Vec<Alert> = self
            .book
            .alerts
            .iter()
            .filter(|a| {
                a.symbol_id == key.0 && a.timeframe == key.1 && Self::live(a, now) && a.needs_bars()
            })
            .cloned()
            .collect();
        let times: Vec<i64> = bars.iter().map(|b| b.time_ms).collect();
        let mut hits: Vec<(u64, f64, i64)> = Vec::new();
        for alert in alerts {
            let Some(value) = self.series_of(&alert, &alert.source, &bars, cx) else {
                continue;
            };
            let level = match &alert.versus {
                None => Some(vec![alert.price; bars.len()]),
                Some(other) => self.series_of(&alert, other, &bars, cx),
            };
            let Some(level) = level else { continue };
            // A zone from a drawing is the drawing's two sides at the last bar.
            let upper = match (&alert.versus, alert.condition.is_zone()) {
                (None, true) => Some(alert.upper),
                (Some(Source::Drawing { id }), true) => self
                    .drawing_level(&alert, *id, times.last().copied().unwrap_or(now), cx)
                    .and_then(|(_, upper)| upper),
                _ => None,
            };
            let last = self.last.get(&alert.id).copied();
            let Some(Judged {
                fired,
                bar,
                reading,
            }) = eval::judge_bars(
                alert.condition,
                alert.trigger,
                &times,
                &value,
                &level,
                upper,
                alert.created_at,
                alert.bar_key,
                last,
            )
            else {
                continue;
            };
            self.last.insert(alert.id, reading);
            if fired {
                hits.push((alert.id, reading.value, bar));
            }
        }
        for (id, value, bar) in hits {
            self.fire(id, value, Some(bar), cx);
        }
    }

    // ---- firing ----

    /// An alert fired: it is counted and put in the history, saved, and the app is told.
    fn fire(&mut self, id: u64, value: f64, bar: Option<i64>, cx: &mut Context<Self>) {
        let Some(alert) = self.book.get(id) else {
            return;
        };
        let digits = self.digits_of(alert.symbol_id);
        let text = alert.render(Some(value), digits);
        let now = now_ms();
        let Some(mut alert) = self.book.fire(id, text.clone(), Some(value), digits, now) else {
            return;
        };
        if let Some(stored) = self.book.get_mut(id) {
            stored.bar_key = bar.or(stored.bar_key);
            alert.bar_key = stored.bar_key;
        }
        self.saver.schedule(self.book.clone());
        self.follow();
        cx.emit(AlertsEvent::Fired(Box::new(alert), text));
        cx.emit(AlertsEvent::Changed);
        cx.notify();
    }
}
