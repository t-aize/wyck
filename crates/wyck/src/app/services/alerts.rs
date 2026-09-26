//! Price alerts: "tell me when EURUSD crosses 1.0850".
//!
//! An alert watches the bid of a symbol and fires when it crosses the alert's price (up, down or
//! either way, as the alert says). A crossing needs a price on each side: the bid before and the
//! bid now. So an alert made at the current price does not fire at once, and a gap past the price
//! fires it like a crossing would.
//!
//! An alert fires once and stops, or keeps watching (it then fires again the next time the price
//! crosses back and forth). Alerts are saved per account and come back at the next start.
//!
//! [`AlertBook`] is the plain data and the crossing rule, tested on its own; [`Alerts`] is the
//! entity that feeds it prices, saves it and tells the user.

use gpui::{App, Context, EventEmitter};
use serde::{Deserialize, Serialize};
use wyck_config::DocumentStore;
use wyck_openapi::market::PRICE_SCALE;

use crate::app::chart::live::{LiveHub, Wish};
use crate::app::toast;
use crate::app::workspace::Saver;

const DOCUMENT: &str = "alerts";
const SCHEMA_VERSION: u32 = 1;
/// The most alerts kept.
pub const MAX_ALERTS: usize = 500;
/// The owner of the alerts' price subscriptions in the live hub.
pub const ALERTS_OWNER: u64 = u64::MAX - 3;

/// Which crossing fires an alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// The price goes from under to over.
    CrossingUp,
    /// The price goes from over to under.
    CrossingDown,
    /// Either.
    #[default]
    Crossing,
}

impl Condition {
    pub const ALL: [Self; 3] = [Self::Crossing, Self::CrossingUp, Self::CrossingDown];

    pub fn label(self) -> &'static str {
        match self {
            Self::Crossing => "Crossing",
            Self::CrossingUp => "Crossing up",
            Self::CrossingDown => "Crossing down",
        }
    }

    /// Whether a move from `before` to `now` crosses `price` the way this condition asks.
    pub fn crossed(self, before: f64, now: f64, price: f64) -> bool {
        let up = before < price && now >= price;
        let down = before > price && now <= price;
        match self {
            Self::CrossingUp => up,
            Self::CrossingDown => down,
            Self::Crossing => up || down,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: u64,
    pub symbol_id: i64,
    /// The broker's name of the symbol, to show and to find it again.
    pub symbol: String,
    /// The price in the symbol's own units (1.0850).
    pub price: f64,
    #[serde(default)]
    pub condition: Condition,
    /// What to say when it fires; the default text when empty.
    #[serde(default)]
    pub message: String,
    /// Whether it keeps watching after firing.
    #[serde(default)]
    pub repeat: bool,
    /// Whether it is watching.
    #[serde(default = "yes")]
    pub active: bool,
    /// When it last fired, in Unix milliseconds.
    #[serde(default)]
    pub fired_at: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
}

fn yes() -> bool {
    true
}

impl Alert {
    /// What the user is told when it fires.
    pub fn text(&self, digits: u32) -> String {
        if self.message.trim().is_empty() {
            format!(
                "{} {} {:.*}",
                self.symbol,
                self.condition.label().to_lowercase(),
                digits as usize,
                self.price
            )
        } else {
            self.message.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertBook {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default = "first_id")]
    pub next_id: u64,
    #[serde(default)]
    pub alerts: Vec<Alert>,
    /// The last bid seen per symbol, to tell a crossing. Not saved.
    #[serde(skip)]
    last: std::collections::HashMap<i64, f64>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn first_id() -> u64 {
    1
}

impl Default for AlertBook {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_id: 1,
            alerts: Vec::new(),
            last: Default::default(),
        }
    }
}

impl AlertBook {
    /// The book repaired: prices that are not prices dropped, ids made unique.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        self.alerts
            .retain(|a| a.price.is_finite() && a.price > 0.0 && a.symbol_id > 0);
        self.alerts.truncate(MAX_ALERTS);
        let mut seen = std::collections::HashSet::new();
        let mut next = self.next_id.max(1);
        for alert in &mut self.alerts {
            next = next.max(alert.id + 1);
        }
        for alert in &mut self.alerts {
            if alert.id == 0 || !seen.insert(alert.id) {
                alert.id = next;
                next += 1;
            }
        }
        self.next_id = next;
        self
    }

    pub fn add(
        &mut self,
        symbol_id: i64,
        symbol: &str,
        price: f64,
        condition: Condition,
        now_ms: i64,
    ) -> Option<u64> {
        if self.alerts.len() >= MAX_ALERTS || !(price.is_finite() && price > 0.0) {
            return None;
        }
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        self.alerts.push(Alert {
            id,
            symbol_id,
            symbol: symbol.to_owned(),
            price,
            condition,
            message: String::new(),
            repeat: false,
            active: true,
            fired_at: None,
            created_at: now_ms,
        });
        Some(id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Alert> {
        self.alerts.iter_mut().find(|a| a.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.alerts.len();
        self.alerts.retain(|a| a.id != id);
        self.alerts.len() != before
    }

    /// The symbols with an active alert.
    pub fn watched(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .alerts
            .iter()
            .filter(|a| a.active)
            .map(|a| a.symbol_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// A new bid of a symbol: returns the alerts it fired. A one-time alert stops watching.
    pub fn on_price(&mut self, symbol_id: i64, bid: f64, now_ms: i64) -> Vec<Alert> {
        let before = self.last.insert(symbol_id, bid);
        let Some(before) = before else {
            return Vec::new();
        };
        let mut fired = Vec::new();
        for alert in self
            .alerts
            .iter_mut()
            .filter(|a| a.active && a.symbol_id == symbol_id)
        {
            if alert.condition.crossed(before, bid, alert.price) {
                alert.fired_at = Some(now_ms);
                if !alert.repeat {
                    alert.active = false;
                }
                fired.push(alert.clone());
            }
        }
        fired
    }
}

pub enum AlertsEvent {
    Changed,
}

/// The live alerts: fed the prices, saved, and heard.
pub struct Alerts {
    book: AlertBook,
    saver: Saver<AlertBook>,
    hub: std::rc::Rc<LiveHub>,
    /// Decimals of each symbol, for the texts.
    pub digits: std::collections::HashMap<i64, u32>,
}

impl EventEmitter<AlertsEvent> for Alerts {}

impl Alerts {
    pub fn new(store: DocumentStore, hub: std::rc::Rc<LiveHub>, cx: &mut Context<Self>) -> Self {
        let book = store.load_or_default::<AlertBook>(DOCUMENT).normalized();
        cx.on_app_quit(|this, _cx| {
            this.saver.flush();
            async {}
        })
        .detach();
        let alerts = Self {
            book,
            saver: Saver::new(store, DOCUMENT),
            hub,
            digits: Default::default(),
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

    /// Changes the alerts, saves them and tells the views.
    pub fn edit<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut AlertBook) -> R,
    ) -> R {
        let result = change(&mut self.book);
        self.saver.schedule(self.book.clone());
        self.follow();
        cx.emit(AlertsEvent::Changed);
        cx.notify();
        result
    }

    /// A price event: fires what it crosses.
    pub fn on_spot(&mut self, symbol_id: i64, bid: Option<i64>, cx: &mut Context<Self>) {
        let Some(bid) = bid else { return };
        let price = bid as f64 / PRICE_SCALE as f64;
        let fired = self
            .book
            .on_price(symbol_id, price, crate::app::chart::now_ms());
        if fired.is_empty() {
            return;
        }
        for alert in &fired {
            let digits = self.digits.get(&alert.symbol_id).copied().unwrap_or(5);
            notify(cx, alert, digits);
        }
        self.saver.schedule(self.book.clone());
        self.follow();
        cx.emit(AlertsEvent::Changed);
        cx.notify();
    }
}

fn notify(cx: &mut App, alert: &Alert, digits: u32) {
    toast::show(cx, toast::Kind::Warning, "Price alert", alert.text(digits));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossings_are_told_by_the_price_before_and_now() {
        assert!(Condition::CrossingUp.crossed(1.0, 2.0, 1.5));
        assert!(!Condition::CrossingUp.crossed(2.0, 1.0, 1.5));
        assert!(Condition::CrossingDown.crossed(2.0, 1.0, 1.5));
        assert!(Condition::Crossing.crossed(2.0, 1.0, 1.5));
        assert!(
            Condition::Crossing.crossed(1.0, 1.5, 1.5),
            "touching counts"
        );
        assert!(
            !Condition::Crossing.crossed(1.5, 1.6, 1.5),
            "leaving the price does not"
        );
    }

    #[test]
    fn an_alert_fires_once_on_a_crossing_and_stops() {
        let mut book = AlertBook::default();
        let id = book
            .add(7, "EURUSD", 1.0850, Condition::Crossing, 0)
            .unwrap();
        assert!(
            book.on_price(7, 1.0840, 1).is_empty(),
            "the first price only sets the start"
        );
        assert!(book.on_price(7, 1.0845, 2).is_empty());
        let fired = book.on_price(7, 1.0852, 3);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].id, id);
        assert_eq!(fired[0].fired_at, Some(3));
        assert!(
            book.on_price(7, 1.0840, 4).is_empty(),
            "it stopped watching"
        );
        assert!(book.watched().is_empty());
    }

    #[test]
    fn a_repeating_alert_keeps_watching() {
        let mut book = AlertBook::default();
        let id = book
            .add(7, "EURUSD", 1.0850, Condition::Crossing, 0)
            .unwrap();
        book.get_mut(id).unwrap().repeat = true;
        book.on_price(7, 1.0840, 1);
        assert_eq!(book.on_price(7, 1.0860, 2).len(), 1);
        assert_eq!(book.on_price(7, 1.0840, 3).len(), 1);
        assert_eq!(book.watched(), vec![7]);
    }

    #[test]
    fn other_symbols_and_directions_are_ignored() {
        let mut book = AlertBook::default();
        book.add(7, "EURUSD", 1.0850, Condition::CrossingUp, 0);
        book.on_price(8, 1.0, 0);
        assert!(book.on_price(8, 2.0, 1).is_empty());
        book.on_price(7, 1.0860, 2);
        assert!(
            book.on_price(7, 1.0840, 3).is_empty(),
            "down does not fire an up alert"
        );
    }

    #[test]
    fn a_saved_book_reads_back_and_is_repaired() {
        let mut book = AlertBook::default();
        book.add(7, "EURUSD", 1.0850, Condition::CrossingDown, 5);
        let text = toml::to_string_pretty(&book).unwrap();
        let back: AlertBook = toml::from_str(&text).unwrap();
        assert_eq!(back.normalized().alerts, book.alerts);
        let broken: AlertBook = toml::from_str(
            "[[alerts]]\nid = 3\nsymbol_id = 7\nsymbol = \"A\"\nprice = 1.0\n\
             [[alerts]]\nid = 3\nsymbol_id = 7\nsymbol = \"B\"\nprice = 2.0\n\
             [[alerts]]\nid = 4\nsymbol_id = 7\nsymbol = \"C\"\nprice = -1.0\n",
        )
        .unwrap();
        let repaired = broken.normalized();
        assert_eq!(repaired.alerts.len(), 2);
        assert_ne!(repaired.alerts[0].id, repaired.alerts[1].id);
        assert!(repaired.next_id > repaired.alerts.iter().map(|a| a.id).max().unwrap());
        assert!(repaired.alerts.iter().all(|a| a.active));
    }

    #[test]
    fn an_alert_says_what_it_watches() {
        let mut book = AlertBook::default();
        let id = book
            .add(7, "EURUSD", 1.085, Condition::CrossingUp, 0)
            .unwrap();
        assert_eq!(book.alerts[0].text(5), "EURUSD crossing up 1.08500");
        book.get_mut(id).unwrap().message = "Breakout".into();
        assert_eq!(book.alerts[0].text(5), "Breakout");
        assert!(book.remove(id));
        assert!(!book.remove(id));
    }
}
