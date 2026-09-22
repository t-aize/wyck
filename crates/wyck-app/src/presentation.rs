//! What the screens show, computed from the engine's state and free of any UI toolkit.
//!
//! Every function here is pure: it takes an [`EngineState`] (or a piece of one) and, when
//! the wall clock matters, the current time, and returns plain strings and a [`Tone`]. The
//! front end only maps a tone to a color. That keeps the rules that matter, such as "an
//! account of unknown kind is never shown as a demo account", testable without a window.

use crate::calendar::{NewsFreshness, NewsItem, NewsView};
use time::OffsetDateTime;
use time::macros::format_description;
use wyck_engine::domain::{AccountKind, Instrument, Position, Quote, UnixMillis};
use wyck_engine::state::{Warning, WarningKind};
use wyck_engine::{EngineState, Event, EventKind, OrderOutcome, SessionState, TradingMode};

/// How much attention something deserves. The UI maps it to a color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Nothing to draw attention to.
    Neutral,
    /// All is well.
    Good,
    /// Look at this.
    Warn,
    /// Something is wrong, or money is at stake.
    Bad,
}

/// A short label with a tone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    /// The text.
    pub text: String,
    /// How it is drawn.
    pub tone: Tone,
}

impl Badge {
    fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

/// The badge for the session's life stage. For a failed session the reason is in
/// [`session_detail`].
#[must_use]
pub fn session_badge(session: &SessionState) -> Badge {
    match session {
        SessionState::Disconnected => Badge::new("Disconnected", Tone::Neutral),
        SessionState::Connecting { .. } => Badge::new("Connecting", Tone::Warn),
        SessionState::Bootstrapping => Badge::new("Loading account", Tone::Warn),
        SessionState::Ready => Badge::new("Ready", Tone::Good),
        SessionState::Reconnecting { attempt } => {
            Badge::new(format!("Reconnecting (attempt {attempt})"), Tone::Warn)
        }
        SessionState::Failed { .. } => Badge::new("Failed", Tone::Bad),
        _ => Badge::new("Unknown state", Tone::Warn),
    }
}

/// The sentence that goes with the session badge, when there is one.
#[must_use]
pub fn session_detail(session: &SessionState) -> Option<String> {
    match session {
        SessionState::Connecting { label } => Some(format!("Connecting to {label}")),
        SessionState::Failed { reason } => Some(reason.clone()),
        _ => None,
    }
}

/// The badge for the account kind. An account whose kind could not be determined is **never**
/// drawn like a demo account: it may be a live one.
#[must_use]
pub fn kind_badge(kind: Option<AccountKind>) -> Badge {
    match kind {
        Some(AccountKind::Demo) => Badge::new("DEMO", Tone::Good),
        Some(AccountKind::Live) => Badge::new("LIVE", Tone::Bad),
        Some(_) => Badge::new("UNKNOWN ACCOUNT", Tone::Warn),
        None => Badge::new("No account", Tone::Neutral),
    }
}

/// The badge for the trading mode. Armed is loud on purpose.
#[must_use]
pub fn mode_badge(mode: TradingMode) -> Badge {
    match mode {
        TradingMode::DryRun => Badge::new("DRY RUN", Tone::Neutral),
        TradingMode::Armed => Badge::new("ARMED", Tone::Bad),
        // A mode this build does not know may be able to send orders: draw it as loudly as Armed.
        _ => Badge::new("UNKNOWN MODE", Tone::Bad),
    }
}

/// Formats a price with `digits` decimals.
#[must_use]
pub fn format_price(price: f64, digits: u32) -> String {
    format!("{price:.*}", digits as usize)
}

/// Formats an amount of money with two decimals and its currency, or a dash when unknown.
#[must_use]
pub fn format_money(amount: Option<f64>, currency: Option<&str>) -> String {
    match amount {
        Some(v) => match currency {
            Some(c) => format!("{v:.2} {c}"),
            None => format!("{v:.2}"),
        },
        None => "-".to_owned(),
    }
}

/// Formats a signed amount with an explicit plus sign, and its tone.
#[must_use]
pub fn format_pnl(amount: Option<f64>) -> (String, Tone) {
    match amount {
        Some(v) if v > 0.0 => (format!("+{v:.2}"), Tone::Good),
        Some(v) if v < 0.0 => (format!("{v:.2}"), Tone::Bad),
        Some(v) => (format!("{v:.2}"), Tone::Neutral),
        None => ("-".to_owned(), Tone::Neutral),
    }
}

/// The header of the main window.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// The session badge.
    pub session: Badge,
    /// The sentence that goes with it.
    pub session_detail: Option<String>,
    /// Demo, live or unknown.
    pub kind: Badge,
    /// Dry run or armed.
    pub mode: Badge,
    /// The account id, once known.
    pub account_id: Option<String>,
    /// Balance, with currency.
    pub balance: String,
    /// Equity, with currency.
    pub equity: String,
    /// Free margin, with currency.
    pub free_margin: String,
    /// The server family, once known.
    pub service: Option<&'static str>,
}

/// Builds the [`Header`].
#[must_use]
pub fn header(state: &EngineState) -> Header {
    let currency = state.account.as_ref().and_then(|a| a.currency.as_deref());
    let figure = |f: fn(&wyck_engine::domain::AccountSnapshot) -> Option<f64>| {
        format_money(state.account.as_ref().and_then(f), currency)
    };
    Header {
        session: session_badge(&state.session),
        session_detail: session_detail(&state.session),
        kind: kind_badge(state.account.as_ref().map(|a| a.kind)),
        mode: mode_badge(state.mode),
        account_id: state.account_id.as_ref().map(ToString::to_string),
        balance: figure(|a| a.balance),
        equity: figure(|a| a.equity),
        free_margin: figure(|a| a.free_margin),
        service: state.service.map(|s| match s {
            wyck_engine::broker::ServiceKind::CtraderRemote => "cTrader Remote",
            wyck_engine::broker::ServiceKind::CtraderLocal => "cTrader Local",
            _ => "cTrader",
        }),
    }
}

/// One row of the positions table, already formatted.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionRow {
    /// The position id, for selection.
    pub id: i64,
    /// The ticker.
    pub symbol: String,
    /// `Buy` or `Sell`.
    pub side: String,
    /// Size, in units and in lots when the lot size is known.
    pub volume: String,
    /// Entry price.
    pub entry: String,
    /// Stop loss, or a dash.
    pub stop_loss: String,
    /// Take profit, or a dash.
    pub take_profit: String,
    /// Floating profit.
    pub pnl: String,
    /// How the profit is drawn.
    pub pnl_tone: Tone,
}

/// The digits to show for `symbol`: what the engine loaded for it, else five.
fn digits_for(state: &EngineState, symbol: &str) -> u32 {
    state
        .instruments
        .get(symbol)
        .map_or(5, |i: &Instrument| i.price_digits)
}

/// Formats a volume as `0.02 lot (2000 units)` when the lot size is known, else as units.
#[must_use]
pub fn format_volume(volume: wyck_engine::domain::Volume, lot_size: Option<f64>) -> String {
    match lot_size {
        Some(lot) if lot > 0.0 => {
            let lots = volume.to_lots(lot);
            format!("{} lot ({volume})", trim_zeros(&format!("{lots:.4}")))
        }
        _ => volume.to_string(),
    }
}

fn trim_zeros(text: &str) -> String {
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text.to_owned()
    }
}

fn position_row(state: &EngineState, p: &Position) -> PositionRow {
    let digits = digits_for(state, &p.symbol);
    let price = |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| format_price(v, digits));
    let lot_size = state.instruments.get(&p.symbol).map(|i| i.volume.lot_size);
    let (pnl, pnl_tone) = format_pnl(p.unrealized_pnl);
    PositionRow {
        id: p.id.get(),
        symbol: p.symbol.clone(),
        side: p.side.to_string(),
        volume: format_volume(p.volume, lot_size),
        entry: price(p.entry_price),
        stop_loss: price(p.stop_loss),
        take_profit: price(p.take_profit),
        pnl,
        pnl_tone,
    }
}

/// The rows of the positions table, in the engine's order.
#[must_use]
pub fn position_rows(state: &EngineState) -> Vec<PositionRow> {
    state
        .positions
        .iter()
        .map(|p| position_row(state, p))
        .collect()
}

/// One quote as the floating panel shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct QuoteLine {
    /// The ticker.
    pub symbol: String,
    /// Best bid.
    pub bid: String,
    /// Best ask.
    pub ask: String,
    /// The spread in pips, when the instrument is known.
    pub spread_pips: Option<String>,
}

/// The quote of `symbol`, formatted, or `None` when there is none yet.
#[must_use]
pub fn quote_line(state: &EngineState, symbol: &str) -> Option<QuoteLine> {
    let q: &Quote = state.quotes.get(symbol)?;
    let digits = digits_for(state, symbol);
    let spread_pips = state
        .instruments
        .get(symbol)
        .filter(|i| i.pip_size > 0.0)
        .map(|i| format!("{:.1}", i.distance_to_pips(q.ask - q.bid)));
    Some(QuoteLine {
        symbol: symbol.to_owned(),
        bid: format_price(q.bid, digits),
        ask: format_price(q.ask, digits),
        spread_pips,
    })
}

/// One warning as the UI lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarningRow {
    /// Stable id, to dismiss it.
    pub id: String,
    /// The category, as a word.
    pub category: &'static str,
    /// The sentence.
    pub message: String,
    /// How it is drawn. An order of unknown outcome is the loudest.
    pub tone: Tone,
    /// Whether the user can dismiss it (only an unknown-order notice, which stays otherwise).
    pub dismissible: bool,
}

/// The warning rows, unknown-order notices first.
#[must_use]
pub fn warning_rows(warnings: &[Warning]) -> Vec<WarningRow> {
    let mut rows: Vec<WarningRow> = warnings
        .iter()
        .map(|w| {
            let (category, tone) = match w.kind {
                WarningKind::News => ("News", Tone::Warn),
                WarningKind::Risk => ("Risk", Tone::Warn),
                WarningKind::Data => ("Data", Tone::Warn),
                WarningKind::UnknownOrder => ("Unknown order", Tone::Bad),
                WarningKind::Assumption => ("Assumption", Tone::Neutral),
                _ => ("Notice", Tone::Neutral),
            };
            WarningRow {
                id: w.id.clone(),
                category,
                message: w.message.clone(),
                tone,
                dismissible: w.kind == WarningKind::UnknownOrder,
            }
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.tone == Tone::Bad));
    rows
}

/// One upcoming economic event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsRow {
    /// `HH:MM` in UTC.
    pub time: String,
    /// The currency, or a dash.
    pub currency: String,
    /// The title.
    pub title: String,
    /// `High`, `Medium`, ...
    pub impact: String,
    /// How the impact is drawn.
    pub tone: Tone,
    /// `in 12 min`, `in 2 h 5 min`, `3 min ago`.
    pub countdown: String,
}

/// The news rows relative to `now`, and a line about how current the data is.
#[must_use]
pub fn news_rows(items: &[NewsItem], now: UnixMillis) -> Vec<NewsRow> {
    items
        .iter()
        .map(|n| NewsRow {
            time: format_clock(n.at, false),
            currency: n.currency.clone().unwrap_or_else(|| "-".to_owned()),
            title: n.title.clone(),
            impact: n.impact.clone(),
            tone: match n.impact.as_str() {
                "High" => Tone::Bad,
                "Medium" => Tone::Warn,
                _ => Tone::Neutral,
            },
            countdown: countdown(n.at - now),
        })
        .collect()
}

/// A sentence about the calendar's freshness, or `None` when the app hosts no calendar.
#[must_use]
pub fn news_status(news: &NewsView) -> Option<(String, Tone)> {
    match news.freshness {
        NewsFreshness::Disabled => None,
        NewsFreshness::Loading => Some(("News: loading".to_owned(), Tone::Neutral)),
        NewsFreshness::Fresh => Some(("News: up to date".to_owned(), Tone::Good)),
        _ => Some((
            match &news.last_error {
                Some(e) => format!("News: out of date ({e})"),
                None => "News: out of date".to_owned(),
            },
            Tone::Warn,
        )),
    }
}

/// `in 12 min`, `in 2 h 5 min`, `now`, `3 min ago`, for a distance in milliseconds.
#[must_use]
pub fn countdown(delta_ms: i64) -> String {
    let minutes = delta_ms.abs() / 60_000;
    let span = if minutes >= 60 {
        let (h, m) = (minutes / 60, minutes % 60);
        if m == 0 {
            format!("{h} h")
        } else {
            format!("{h} h {m} min")
        }
    } else {
        format!("{minutes} min")
    };
    match delta_ms {
        d if d.abs() < 60_000 => "now".to_owned(),
        d if d > 0 => format!("in {span}"),
        _ => format!("{span} ago"),
    }
}

/// `HH:MM` or `HH:MM:SS` in UTC.
#[must_use]
pub fn format_clock(at: UnixMillis, seconds: bool) -> String {
    let Ok(t) = OffsetDateTime::from_unix_timestamp(at.div_euclid(1000)) else {
        return "--:--".to_owned();
    };
    let formatted = if seconds {
        t.format(format_description!("[hour]:[minute]:[second]"))
    } else {
        t.format(format_description!("[hour]:[minute]"))
    };
    formatted.unwrap_or_else(|_| "--:--".to_owned())
}

/// One line of the activity log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRow {
    /// `HH:MM:SS` in UTC.
    pub time: String,
    /// What happened.
    pub text: String,
    /// How it is drawn.
    pub tone: Tone,
}

/// Describes an event for the activity log. Routine refreshes are not shown.
#[must_use]
pub fn activity_row(event: &Event) -> Option<ActivityRow> {
    let (text, tone) = match &event.kind {
        EventKind::AccountUpdated => return None,
        EventKind::SessionChanged(s) => (
            session_detail(s).unwrap_or_else(|| session_badge(s).text),
            session_badge(s).tone,
        ),
        EventKind::ModeChanged { mode, reason } => (
            format!("{}: {reason}", mode_badge(*mode).text),
            mode_badge(*mode).tone,
        ),
        EventKind::PositionsChanged {
            opened,
            modified,
            closed,
        } => {
            let mut parts = Vec::new();
            for (n, what) in [
                (opened.len(), "opened"),
                (modified.len(), "changed"),
                (closed.len(), "closed"),
            ] {
                if n > 0 {
                    parts.push(format!("{n} {what}"));
                }
            }
            if parts.is_empty() {
                return None;
            }
            (format!("Positions: {}", parts.join(", ")), Tone::Neutral)
        }
        EventKind::OrderPlanned(p) => (
            format!("Planned {} {} {}", p.side, p.volume, p.symbol),
            Tone::Neutral,
        ),
        EventKind::OrderSubmitted { label } => (format!("Order sent ({label})"), Tone::Warn),
        EventKind::OrderResult(outcome) => (outcome_text(outcome), outcome_tone(outcome)),
        EventKind::ProtectionChanged { position } => {
            (format!("Protection changed on {position}"), Tone::Neutral)
        }
        EventKind::PositionClosed { position } => {
            (format!("Position {position} closed"), Tone::Neutral)
        }
        EventKind::Flattened(r) => (
            format!(
                "Flatten: {} closed, {} cancelled, {} errors",
                r.closed.len(),
                r.cancelled.len(),
                r.errors.len()
            ),
            if r.errors.is_empty() {
                Tone::Neutral
            } else {
                Tone::Bad
            },
        ),
        EventKind::WarningRaised(w) => (w.message.clone(), Tone::Warn),
        EventKind::WarningCleared { .. } => return None,
        EventKind::RefreshFailed { message } => (format!("Refresh failed: {message}"), Tone::Warn),
        EventKind::Reconciled { label, position } => (
            format!("Order {label} matched to position {position}"),
            Tone::Good,
        ),
        _ => return None,
    };
    Some(ActivityRow {
        time: format_clock(event.at, true),
        text,
        tone,
    })
}

/// One sentence for an order outcome.
#[must_use]
pub fn outcome_text(outcome: &OrderOutcome) -> String {
    match outcome {
        OrderOutcome::Filled { position, .. } => format!(
            "Filled: {} {} {}",
            position.side, position.volume, position.symbol
        ),
        OrderOutcome::PartiallyFilled {
            position,
            requested,
            ..
        } => format!(
            "Partly filled: {} of {requested} on {}",
            position.volume, position.symbol
        ),
        OrderOutcome::Rejected { reason, .. } => format!("Rejected: {reason}"),
        OrderOutcome::DryRun { would_send } => format!(
            "Dry run, nothing sent: {} {} {}",
            would_send.side, would_send.volume, would_send.symbol
        ),
        OrderOutcome::Unknown { reason, .. } => {
            format!("Outcome unknown, check the platform: {reason}")
        }
        _ => "Order outcome not recognized".to_owned(),
    }
}

/// How an order outcome is drawn. Unknown is as loud as a rejection.
#[must_use]
pub fn outcome_tone(outcome: &OrderOutcome) -> Tone {
    match outcome {
        OrderOutcome::Filled { .. } => Tone::Good,
        OrderOutcome::DryRun { .. } => Tone::Neutral,
        OrderOutcome::PartiallyFilled { .. } => Tone::Warn,
        _ => Tone::Bad,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use wyck_engine::EngineConfig;
    use wyck_engine::domain::{AccountSnapshot, Side, SpecsSource, Volume, VolumeSpecs};
    use wyck_engine::{AccountId, PositionId};

    use super::*;

    fn blank_state() -> EngineState {
        // A state as the engine publishes it before anything is connected.
        let config = EngineConfig {
            ..EngineConfig::default()
        };
        let engine = wyck_engine::Engine::start(config).unwrap();
        let state = (*engine.handle().state()).clone();
        drop(engine);
        state
    }

    fn eurusd() -> Instrument {
        Instrument {
            symbol: "EURUSD".to_owned(),
            symbol_id: Some(1),
            price_digits: 5,
            pip_size: 0.0001,
            base_currency: Some("EUR".to_owned()),
            quote_currency: Some("USD".to_owned()),
            enabled: true,
            volume: VolumeSpecs {
                lot_size: 100_000.0,
                min: Volume::from_units(1_000),
                step: Volume::from_units(1_000),
                max: None,
            },
            specs_source: SpecsSource::Broker,
        }
    }

    fn position() -> Position {
        Position {
            id: PositionId(7),
            symbol: "EURUSD".to_owned(),
            side: Side::Buy,
            volume: Volume::from_units(2_000),
            entry_price: Some(1.08501),
            stop_loss: Some(1.08301),
            take_profit: None,
            swap: None,
            commission: None,
            unrealized_pnl: Some(-3.5),
            label: None,
        }
    }

    #[test]
    fn an_unknown_account_kind_is_never_drawn_like_a_demo() {
        assert_eq!(kind_badge(Some(AccountKind::Demo)).tone, Tone::Good);
        assert_eq!(kind_badge(Some(AccountKind::Live)).tone, Tone::Bad);
        let unknown = kind_badge(Some(AccountKind::Unknown));
        assert_eq!(unknown.tone, Tone::Warn);
        assert!(unknown.text.contains("UNKNOWN"));
        assert_ne!(unknown, kind_badge(Some(AccountKind::Demo)));
        assert_eq!(kind_badge(None).text, "No account");
    }

    #[test]
    fn armed_is_loud_and_dry_run_is_quiet() {
        assert_eq!(mode_badge(TradingMode::Armed).tone, Tone::Bad);
        assert_eq!(mode_badge(TradingMode::DryRun).tone, Tone::Neutral);
    }

    #[test]
    fn session_badges_follow_the_life_cycle() {
        assert_eq!(session_badge(&SessionState::Ready).tone, Tone::Good);
        assert_eq!(
            session_badge(&SessionState::Reconnecting { attempt: 3 }).text,
            "Reconnecting (attempt 3)"
        );
        let failed = SessionState::Failed {
            reason: "bad token".to_owned(),
        };
        assert_eq!(session_badge(&failed).tone, Tone::Bad);
        assert_eq!(session_detail(&failed).as_deref(), Some("bad token"));
        assert_eq!(session_detail(&SessionState::Ready), None);
    }

    #[test]
    fn numbers_are_formatted_for_reading() {
        assert_eq!(format_price(1.5, 5), "1.50000");
        assert_eq!(format_price(156.7678, 3), "156.768");
        assert_eq!(format_money(Some(9995.256), Some("USD")), "9995.26 USD");
        assert_eq!(format_money(None, Some("USD")), "-");
        assert_eq!(format_pnl(Some(2.0)), ("+2.00".to_owned(), Tone::Good));
        assert_eq!(format_pnl(Some(-0.35)), ("-0.35".to_owned(), Tone::Bad));
        assert_eq!(format_pnl(Some(0.0)), ("0.00".to_owned(), Tone::Neutral));
        assert_eq!(format_pnl(None).0, "-");
    }

    #[test]
    fn volumes_show_lots_when_the_lot_size_is_known() {
        let v = Volume::from_units(2_000);
        assert_eq!(format_volume(v, Some(100_000.0)), "0.02 lot (2000 units)");
        assert_eq!(
            format_volume(Volume::from_units(100_000), Some(100_000.0)),
            "1 lot (100000 units)"
        );
        assert_eq!(
            format_volume(Volume::from_cents(1), Some(1.0)),
            "0.01 lot (0.01 units)"
        );
        assert_eq!(format_volume(v, None), "2000 units");
    }

    #[test]
    fn position_rows_use_the_instruments_digits_and_show_gaps_as_dashes() {
        let mut state = blank_state();
        state.positions = vec![position()];
        state.instruments = BTreeMap::from([("EURUSD".to_owned(), eurusd())]);
        let rows = position_rows(&state);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.id, 7);
        assert_eq!(r.side, "Buy");
        assert_eq!(r.volume, "0.02 lot (2000 units)");
        assert_eq!(r.entry, "1.08501");
        assert_eq!(r.stop_loss, "1.08301");
        assert_eq!(r.take_profit, "-");
        assert_eq!((r.pnl.as_str(), r.pnl_tone), ("-3.50", Tone::Bad));
    }

    #[test]
    fn the_header_summarizes_the_account() {
        let mut state = blank_state();
        state.session = SessionState::Ready;
        state.account_id = Some(AccountId::new("remote"));
        state.account = Some(AccountSnapshot {
            account_id: AccountId::new("remote"),
            kind: AccountKind::Demo,
            currency: Some("USD".to_owned()),
            balance: Some(9995.26),
            equity: Some(9990.0),
            free_margin: None,
            used_margin: None,
            margin_level_pct: None,
            server_version: None,
            captured_at: 0,
        });
        let h = header(&state);
        assert_eq!(h.session.text, "Ready");
        assert_eq!(h.kind.text, "DEMO");
        assert_eq!(h.mode.text, "DRY RUN");
        assert_eq!(h.balance, "9995.26 USD");
        assert_eq!(h.free_margin, "-");
        assert_eq!(h.account_id.as_deref(), Some("remote"));
    }

    #[test]
    fn a_quote_line_carries_the_spread_in_pips() {
        let mut state = blank_state();
        state.instruments = BTreeMap::from([("EURUSD".to_owned(), eurusd())]);
        state.quotes = BTreeMap::from([(
            "EURUSD".to_owned(),
            Quote {
                symbol: "EURUSD".to_owned(),
                bid: 1.14879,
                ask: 1.1488,
                timestamp: None,
            },
        )]);
        let line = quote_line(&state, "EURUSD").unwrap();
        assert_eq!(
            (line.bid.as_str(), line.ask.as_str()),
            ("1.14879", "1.14880")
        );
        assert_eq!(line.spread_pips.as_deref(), Some("0.1"));
        assert!(quote_line(&state, "XAUUSD").is_none());
    }

    #[test]
    fn unknown_order_warnings_come_first_and_can_be_dismissed() {
        let warn = |id: &str, kind| Warning {
            id: id.to_owned(),
            kind,
            message: id.to_owned(),
            raised_at: 0,
        };
        let rows = warning_rows(&[
            warn("news", WarningKind::News),
            warn("unknown-order:x", WarningKind::UnknownOrder),
        ]);
        assert_eq!(rows[0].id, "unknown-order:x");
        assert_eq!(rows[0].tone, Tone::Bad);
        assert!(rows[0].dismissible);
        assert!(!rows[1].dismissible);
    }

    #[test]
    fn countdowns_read_naturally() {
        assert_eq!(countdown(30_000), "now");
        assert_eq!(countdown(12 * 60_000), "in 12 min");
        assert_eq!(countdown((2 * 60 + 5) * 60_000), "in 2 h 5 min");
        assert_eq!(countdown(3 * 3_600_000), "in 3 h");
        assert_eq!(countdown(-3 * 60_000), "3 min ago");
    }

    #[test]
    fn news_rows_carry_time_impact_and_countdown() {
        let at = 1_789_839_318_399; // 2026-09-19 17:35:18 UTC
        let rows = news_rows(
            &[NewsItem {
                title: "CPI m/m".to_owned(),
                currency: Some("USD".to_owned()),
                impact: "High".to_owned(),
                at,
                forecast: None,
                previous: None,
            }],
            at - 10 * 60_000,
        );
        assert_eq!(rows[0].time, "17:35");
        assert_eq!(rows[0].countdown, "in 10 min");
        assert_eq!(rows[0].tone, Tone::Bad);
        assert_eq!(format_clock(at, true), "17:35:18");
    }

    #[test]
    fn outcomes_are_described_and_unknown_is_as_loud_as_a_rejection() {
        use wyck_engine::PlanId;
        let plan_id = |n: u64| -> PlanId { serde_json::from_value(serde_json::json!(n)).unwrap() };
        let unknown = OrderOutcome::Unknown {
            plan: plan_id(1),
            label: "x".to_owned(),
            reason: "timeout".to_owned(),
        };
        assert_eq!(outcome_tone(&unknown), Tone::Bad);
        assert!(outcome_text(&unknown).contains("check the platform"));
        let rejected = OrderOutcome::Rejected {
            plan: plan_id(1),
            reason: "no".to_owned(),
        };
        assert_eq!(outcome_tone(&rejected), Tone::Bad);
        assert_eq!(outcome_text(&rejected), "Rejected: no");
    }

    #[test]
    fn routine_events_stay_out_of_the_activity_log() {
        let event = |kind| Event {
            at: 1_789_839_318_399,
            account: None,
            command: None,
            kind,
        };
        assert!(activity_row(&event(EventKind::AccountUpdated)).is_none());
        assert!(activity_row(&event(EventKind::WarningCleared { id: "x".to_owned() })).is_none());
        let row = activity_row(&event(EventKind::RefreshFailed {
            message: "boom".to_owned(),
        }))
        .unwrap();
        assert_eq!(row.time, "17:35:18");
        assert!(row.text.contains("boom"));
    }
}
