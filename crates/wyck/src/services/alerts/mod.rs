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

use gpui::{App, Context, Entity, EventEmitter};
use wyck_chart::Timeframe;
use wyck_chart::data::Series;
use wyck_chart::study::{self, ValueFormat};
use wyck_config::DocumentStore;
use wyck_openapi::market::{Bar, PRICE_SCALE};
use wyck_openapi::session::Session;

use crate::chart::drawing::Drawings;
use crate::chart::live::{LiveHub, Wish};
use crate::chart::load::{self, Loaded};
use crate::workspace::Saver;
use crate::{chart::now_ms, runtime};

pub mod eval;
pub mod model;

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
            let Source::Price { price } = &alert.source else {
                continue;
            };
            let Some(value) = price.of(bid, ask) else {
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
            let before = self.last.insert(alert.id, reading);
            let bar = Timeframe::from_code(&alert.timeframe)
                .and_then(Timeframe::bar_ms)
                .map(|ms| now.div_euclid(ms) * ms);
            let hit = if alert.condition == Condition::MovesBy {
                let history = self.moves.entry(alert.id).or_default();
                history.push((now, value));
                let window = i64::from(alert.minutes.max(1)) * 60_000;
                history.retain(|(at, _)| now - at <= window);
                eval::moved(history, window, alert.amount)
            } else {
                before.is_some_and(|before| eval::fired(alert.condition, &before, &reading))
            };
            let by_bar =
                alert.trigger == Trigger::OncePerBar && bar.is_some() && alert.bar_key == bar;
            if hit && !by_bar {
                hits.push((alert.id, value, bar));
                self.moves.remove(&alert.id);
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
            Some(Source::Indicator { .. }) => None,
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
        let input =
            wyck_chart::display::study_input(&Series::Bars(bars.to_vec()), wyck_chart::Zone::Utc);
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
        let Some(mut alert) = self.book.fire(id, text.clone(), now) else {
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
