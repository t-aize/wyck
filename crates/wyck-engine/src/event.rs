//! Events: what changed, and when.
//!
//! Every event carries the account it concerns and, when it is the consequence of a
//! command, the [`CommandId`] of that command, so a front end can tie `OrderPlanned`,
//! `OrderSubmitted` and `OrderResult` together. Events are delivered on a bounded
//! broadcast channel: a receiver that falls too far behind gets a `Lagged` error and must
//! resynchronize from [`EngineState`](crate::EngineState) (see the crate docs).

use serde::{Deserialize, Serialize};

use crate::domain::{Position, UnixMillis};
use crate::ids::{AccountId, CommandId, PositionId};
use crate::state::{SessionState, TradingMode, Warning};
use crate::trading::{FlattenReport, OrderOutcome, PlanSummary};

/// One thing that happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// When it happened, in Unix milliseconds (local clock).
    pub at: UnixMillis,
    /// The account it concerns, when there is one.
    pub account: Option<AccountId>,
    /// The command that caused it, if any.
    pub command: Option<CommandId>,
    /// What happened.
    pub kind: EventKind,
}

/// The payload of an [`Event`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EventKind {
    /// The session state changed.
    SessionChanged(SessionState),
    /// Trading mode changed (armed, disarmed, or forced back to dry-run).
    ModeChanged {
        /// The new mode.
        mode: TradingMode,
        /// Why, for display.
        reason: String,
    },
    /// Account figures were refreshed.
    AccountUpdated,
    /// Positions changed since the last refresh.
    PositionsChanged {
        /// Newly opened positions.
        opened: Vec<Position>,
        /// Positions whose volume, stops or figures changed.
        modified: Vec<Position>,
        /// Ids of positions that no longer exist.
        closed: Vec<PositionId>,
    },
    /// An order plan was created.
    OrderPlanned(PlanSummary),
    /// An order was sent to the broker.
    OrderSubmitted {
        /// The idempotency label it carries.
        label: String,
    },
    /// An order reached a final or uncertain outcome.
    OrderResult(OrderOutcome),
    /// A position's stop loss or take profit was changed.
    ProtectionChanged {
        /// The position.
        position: PositionId,
    },
    /// A position was closed, fully or partly.
    PositionClosed {
        /// The position.
        position: PositionId,
    },
    /// A flatten finished.
    Flattened(FlattenReport),
    /// A warning was raised.
    WarningRaised(Warning),
    /// A warning was cleared.
    WarningCleared {
        /// The cleared warning's id.
        id: String,
    },
    /// A refresh failed; the previous data stays in place.
    RefreshFailed {
        /// What went wrong.
        message: String,
    },
    /// An order of unknown outcome was matched to a position after a refresh or reconnect.
    Reconciled {
        /// The label of the order.
        label: String,
        /// The position it turned out to have opened.
        position: PositionId,
    },
}
