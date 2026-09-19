//! Identifier newtypes.
//!
//! Distinct types for distinct things (a position id is not an order id), so the
//! compiler catches the classic mix-up of passing one where the other is expected.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

macro_rules! numeric_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl $name {
            /// The raw broker-side value.
            #[must_use]
            pub fn get(self) -> i64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }
    };
}

numeric_id!(
    /// A broker position id.
    PositionId
);
numeric_id!(
    /// A broker pending-order id.
    OrderId
);

/// Identifies a trading account inside the engine.
///
/// Every command and event carries one so that supporting several accounts at once later
/// does not change the API. Built from the broker's trader id when known, otherwise from
/// the connection endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    /// Wraps a raw identifier.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The identifier as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Correlates the events a single command produced (`OrderPlanned`, `OrderSubmitted`,
/// `OrderResult`, ...) with the call that started them. Unique within a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(u64);

impl CommandId {
    /// Allocates the next id.
    #[must_use]
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    /// The raw counter value.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for CommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cmd-{}", self.0)
    }
}

/// A monotonically increasing counter stamped on every published
/// [`EngineState`](crate::EngineState). Two snapshots with the same revision are
/// identical; a higher revision is newer.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Revision(pub u64);

impl Revision {
    /// The following revision.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_are_unique_and_increasing() {
        let a = CommandId::next();
        let b = CommandId::next();
        assert!(b > a);
    }

    #[test]
    fn ids_serialize_as_bare_values() {
        assert_eq!(serde_json::to_string(&PositionId(7)).unwrap(), "7");
        assert_eq!(
            serde_json::to_string(&AccountId::new("acc-1")).unwrap(),
            "\"acc-1\""
        );
    }

    #[test]
    fn revision_advances() {
        assert_eq!(Revision(4).next(), Revision(5));
        assert_eq!(Revision(u64::MAX).next(), Revision(u64::MAX));
    }
}
