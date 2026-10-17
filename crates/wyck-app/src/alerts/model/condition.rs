//! When an alert fires: the condition on the value it watches and how often it may fire.

use serde::{Deserialize, Serialize};

/// What has to happen to a value for the alert to fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    /// The value goes from under the level to over it.
    CrossingUp,
    /// The value goes from over the level to under it.
    CrossingDown,
    #[default]
    Crossing,
    /// The value becomes greater than the level.
    Above,
    /// The value becomes smaller than the level.
    Below,
    /// The value gets inside the zone between the level and the upper level.
    EntersZone,
    /// The value leaves the zone.
    ExitsZone,
    /// The value moves by a share of itself within some minutes.
    MovesBy,
    /// The value, which was rising, starts to fall, or the other way round.
    ChangesDirection,
}

impl Condition {
    pub const ALL: [Self; 9] = [
        Self::Crossing,
        Self::CrossingUp,
        Self::CrossingDown,
        Self::Above,
        Self::Below,
        Self::EntersZone,
        Self::ExitsZone,
        Self::MovesBy,
        Self::ChangesDirection,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Crossing => "Crossing",
            Self::CrossingUp => "Crossing up",
            Self::CrossingDown => "Crossing down",
            Self::Above => "Becomes greater than",
            Self::Below => "Becomes smaller than",
            Self::EntersZone => "Enters the zone",
            Self::ExitsZone => "Leaves the zone",
            Self::MovesBy => "Moves by",
            Self::ChangesDirection => "Changes direction",
        }
    }

    /// Whether it needs a second level: the other side of a zone.
    pub fn is_zone(self) -> bool {
        matches!(self, Self::EntersZone | Self::ExitsZone)
    }

    /// Whether it compares the value with a level at all.
    pub fn needs_level(self) -> bool {
        !matches!(self, Self::MovesBy | Self::ChangesDirection)
    }

    /// Whether a move from `before` to `now` crosses `price` the way this condition asks. Only
    /// for the crossings.
    #[cfg(test)]
    pub(crate) fn crossed(self, before: f64, now: f64, price: f64) -> bool {
        let up = before < price && now >= price;
        let down = before > price && now <= price;
        match self {
            Self::CrossingUp => up,
            Self::CrossingDown => down,
            Self::Crossing => up || down,
            _ => false,
        }
    }
}

/// How often an alert may fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    /// Once, then it stops.
    #[default]
    Once,
    EveryTime,
    /// At most once for each bar of the timeframe.
    OncePerBar,
    /// Only when a bar closes: the condition is judged on the closed bars, once each.
    OncePerBarClose,
}

impl Trigger {
    pub const ALL: [Self; 4] = [
        Self::Once,
        Self::EveryTime,
        Self::OncePerBar,
        Self::OncePerBarClose,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Once => "Only once",
            Self::EveryTime => "Every time",
            Self::OncePerBar => "Once per bar",
            Self::OncePerBarClose => "Once per bar close",
        }
    }
}
