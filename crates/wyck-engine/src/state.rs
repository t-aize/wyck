//! The published engine state: one immutable snapshot a front end can render.
//!
//! Front ends read [`EngineState`] (through [`EngineHandle::watch_state`]) instead of
//! assembling their own picture from events. Events say *what changed*; the state says
//! *what is true now*, and is always enough to redraw the whole screen. A subscriber that
//! misses events (a lagging broadcast receiver) resynchronizes by reading the state.
//!
//! [`EngineHandle::watch_state`]: crate::EngineHandle::watch_state

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::broker::ServiceKind;
use crate::domain::{AccountSnapshot, Instrument, PendingOrder, Position, Quote, UnixMillis};
use crate::ids::{AccountId, Revision};

/// Where the broker session is in its life.
///
/// ```text
/// Disconnected --connect--> Connecting --> Bootstrapping --> Ready
///      ^                        |               |              |
///      |                      fail            fail        refresh/ping fail
///      |                        v               v              v
///      +----- disconnect ---- Failed <----------+         Reconnecting(n)
///                               ^                              |
///                               +------ attempts exhausted ----+   (success: Ready)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SessionState {
    /// No session.
    Disconnected,
    /// Opening the connection.
    Connecting {
        /// The profile or endpoint label.
        label: String,
    },
    /// Connected; loading account, positions and instruments.
    Bootstrapping,
    /// Fully usable.
    Ready,
    /// The connection dropped; retrying.
    Reconnecting {
        /// The attempt in progress, starting at 1.
        attempt: u32,
    },
    /// The session could not be established or recovered.
    Failed {
        /// Why, for display.
        reason: String,
    },
}

impl SessionState {
    /// Whether requests that need the broker can be served right now.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// A short description for error messages.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Disconnected => "disconnected".to_owned(),
            Self::Connecting { .. } => "connecting".to_owned(),
            Self::Bootstrapping => "loading account data".to_owned(),
            Self::Ready => "ready".to_owned(),
            Self::Reconnecting { attempt } => format!("reconnecting, attempt {attempt}"),
            Self::Failed { reason } => format!("failed: {reason}"),
        }
    }
}

/// Whether the engine may send real orders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TradingMode {
    /// Orders are planned, sized and validated but never sent. The default, and the mode
    /// the engine returns to whenever the session leaves `Ready`.
    #[default]
    DryRun,
    /// Orders are sent to the broker.
    Armed,
}

/// What a [`Warning`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WarningKind {
    /// A high-impact economic release is close.
    News,
    /// An order or the account carries more risk than configured.
    Risk,
    /// Data the decision relies on is old or missing.
    Data,
    /// An order's outcome could not be determined and needs a look.
    UnknownOrder,
    /// The engine made an assumption the user should know about.
    Assumption,
}

/// A non-blocking notice. The engine warns; it never uses a warning to refuse an order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    /// Stable identifier, so a front end can dismiss or de-duplicate it.
    pub id: String,
    /// The category.
    pub kind: WarningKind,
    /// A sentence for the user.
    pub message: String,
    /// When it was raised.
    pub raised_at: UnixMillis,
}

/// How current the calendar data is. Mirrors `wyck_calendar::Freshness`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum NewsFreshness {
    /// The engine does not host a calendar.
    Disabled,
    /// The first fetch has not finished.
    Loading,
    /// No fetch has ever succeeded.
    Unavailable,
    /// The latest fetch succeeded.
    Fresh,
    /// The latest fetch failed; the events shown are from an earlier success.
    Stale,
}

/// One economic event, in the engine's terms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsItem {
    /// The event title, e.g. `"CPI m/m"`.
    pub title: String,
    /// The currency it applies to, or `None` for non-currency events.
    pub currency: Option<String>,
    /// `"High"`, `"Medium"`, `"Low"`, `"Holiday"` or `"Unknown"`.
    pub impact: String,
    /// When it happens, in Unix milliseconds.
    pub at: UnixMillis,
    /// The published forecast, if any.
    pub forecast: Option<String>,
    /// The previous value, if any.
    pub previous: Option<String>,
}

/// The calendar as a front end sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsView {
    /// How current it is.
    pub freshness: NewsFreshness,
    /// When the data was last confirmed current.
    pub fetched_at: Option<UnixMillis>,
    /// The most recent fetch error, if the data is stale or unavailable.
    pub last_error: Option<String>,
    /// The next relevant events, soonest first, already filtered to what the user trades.
    pub upcoming: Vec<NewsItem>,
}

impl Default for NewsView {
    fn default() -> Self {
        Self {
            freshness: NewsFreshness::Disabled,
            fetched_at: None,
            last_error: None,
            upcoming: Vec::new(),
        }
    }
}

/// The engine's whole visible state at one instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineState {
    /// Increases on every published change.
    pub revision: Revision,
    /// The session's life stage.
    pub session: SessionState,
    /// Dry-run or armed.
    pub mode: TradingMode,
    /// Which server family the session uses, once known.
    pub service: Option<ServiceKind>,
    /// The connected account, once known.
    pub account_id: Option<AccountId>,
    /// Latest account figures.
    pub account: Option<AccountSnapshot>,
    /// Open positions.
    pub positions: Vec<Position>,
    /// Working orders.
    pub pending_orders: Vec<PendingOrder>,
    /// Latest quotes, keyed by symbol.
    pub quotes: BTreeMap<String, Quote>,
    /// Symbols whose details the engine has loaded, keyed by symbol.
    pub instruments: BTreeMap<String, Instrument>,
    /// Symbols the engine keeps quotes for (positions' symbols are always included).
    pub watched: Vec<String>,
    /// Symbols with an order currently in flight.
    pub orders_in_flight: Vec<String>,
    /// Active warnings.
    pub warnings: Vec<Warning>,
    /// The economic calendar.
    pub news: NewsView,
    /// The most recent refresh error, cleared by the next success.
    pub last_error: Option<String>,
    /// When the account and positions were last refreshed successfully.
    pub last_refresh: Option<UnixMillis>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            revision: Revision::default(),
            session: SessionState::Disconnected,
            mode: TradingMode::DryRun,
            service: None,
            account_id: None,
            account: None,
            positions: Vec::new(),
            pending_orders: Vec::new(),
            quotes: BTreeMap::new(),
            instruments: BTreeMap::new(),
            watched: Vec::new(),
            orders_in_flight: Vec::new(),
            warnings: Vec::new(),
            news: NewsView::default(),
            last_error: None,
            last_refresh: None,
        }
    }
}

impl EngineState {
    /// Whether real orders would be sent right now.
    #[must_use]
    pub fn is_trading_armed(&self) -> bool {
        self.mode == TradingMode::Armed && self.session.is_ready()
    }

    /// The position with the given id.
    #[must_use]
    pub fn position(&self, id: crate::ids::PositionId) -> Option<&Position> {
        self.positions.iter().find(|p| p.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_state_is_disconnected_and_disarmed() {
        let s = EngineState::default();
        assert_eq!(s.session, SessionState::Disconnected);
        assert_eq!(s.mode, TradingMode::DryRun);
        assert!(!s.is_trading_armed());
    }

    #[test]
    fn armed_only_counts_while_the_session_is_ready() {
        let mut s = EngineState {
            mode: TradingMode::Armed,
            ..EngineState::default()
        };
        assert!(!s.is_trading_armed());
        s.session = SessionState::Ready;
        assert!(s.is_trading_armed());
    }

    #[test]
    fn state_round_trips_through_json() {
        let mut s = EngineState::default();
        s.warnings.push(Warning {
            id: "w1".into(),
            kind: WarningKind::News,
            message: "FOMC in 10 minutes".into(),
            raised_at: 1,
        });
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<EngineState>(&json).unwrap(), s);
    }

    #[test]
    fn session_descriptions_are_human_readable() {
        assert_eq!(SessionState::Ready.describe(), "ready");
        assert!(
            SessionState::Reconnecting { attempt: 2 }
                .describe()
                .contains('2')
        );
    }
}
