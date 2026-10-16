//! What an alert watches: a price of the market, an indicator, a spread or a profit.

use serde::{Deserialize, Serialize};
use wyck_indicators::config::StudyConfig;

use super::condition::Condition;

/// Which price of the market a price alert follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceKind {
    #[default]
    Bid,
    Ask,
    Mid,
}

impl PriceKind {
    pub const ALL: [Self; 3] = [Self::Bid, Self::Ask, Self::Mid];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bid => "Bid",
            Self::Ask => "Ask",
            Self::Mid => "Mid",
        }
    }

    /// The price of this kind in a quote.
    pub(crate) fn of(self, bid: Option<f64>, ask: Option<f64>) -> Option<f64> {
        match self {
            Self::Bid => bid,
            Self::Ask => ask,
            Self::Mid => bid.zip(ask).map(|(b, a)| f64::midpoint(b, a)),
        }
    }
}

/// Something that has a value at every moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// The price of the symbol.
    Price {
        #[serde(default)]
        price: PriceKind,
    },
    /// One plot of an indicator, worked out on the bars of the alert's timeframe.
    Indicator {
        study: Box<StudyConfig>,
        #[serde(default)]
        plot: usize,
    },
    /// A drawing of the symbol: the level of a line at the time, or the two sides of a zone.
    Drawing { id: u64 },
    /// The spread of the symbol (ask minus bid), in pips. `pip` is the size of one pip in price,
    /// kept with the alert because the alerts do not read the contracts of the symbols.
    Spread {
        #[serde(default = "default_pip")]
        pip: f64,
    },
    /// What the account, a symbol or a position makes or loses, in the money of the account.
    Pnl { scope: PnlScope },
}

/// Which profit a profit alert follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum PnlScope {
    /// Everything open on the account.
    Account,
    /// The positions of the alert's symbol.
    Symbol,
    /// One position.
    Position { id: i64 },
}

/// The size of a pip of a forex pair with five decimals.
pub(super) fn default_pip() -> f64 {
    0.0001
}

impl Default for Source {
    fn default() -> Self {
        Self::Price {
            price: PriceKind::Bid,
        }
    }
}

impl Source {
    pub fn is_indicator(&self) -> bool {
        matches!(self, Self::Indicator { .. })
    }

    /// The name shown for it: `Bid`, `RSI (plot 1)`, `Drawing 12`.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Price { price } => price.label().to_owned(),
            Self::Indicator { study, plot } => {
                let spec = study.kind.spec();
                let plot_name = spec.plots.get(*plot).map_or("", |p| p.label);
                if spec.plots.len() > 1 && !plot_name.is_empty() {
                    format!("{} {}", spec.short, plot_name)
                } else {
                    spec.short.to_owned()
                }
            }
            Self::Drawing { id } => format!("drawing {id}"),
            Self::Spread { .. } => "spread".to_owned(),
            Self::Pnl { scope } => match scope {
                PnlScope::Account => "account profit".to_owned(),
                PnlScope::Symbol => "profit".to_owned(),
                PnlScope::Position { id } => format!("profit of position {id}"),
            },
        }
    }

    /// Whether the value is a plain number of the alert's own (a spread, a profit), not a price
    /// or the plot of an indicator: no drawing, no other value to compare with.
    pub fn is_measure(&self) -> bool {
        matches!(self, Self::Spread { .. } | Self::Pnl { .. })
    }

    /// Whether the alert belongs to a symbol. The profit of the whole account does not.
    pub fn has_symbol(&self) -> bool {
        !matches!(
            self,
            Self::Pnl {
                scope: PnlScope::Account
            }
        )
    }

    /// The decimals to write a value of this source with: `digits` for a price.
    pub fn decimals(&self, digits: u32) -> u32 {
        match self {
            Self::Spread { .. } => 1,
            Self::Pnl { .. } => 2,
            _ => digits,
        }
    }

    /// Whether a condition makes sense for the value: a profit has no direction to change, a
    /// number that is not a price does not move by a share of itself.
    pub fn allows(&self, condition: Condition) -> bool {
        match self {
            Self::Spread { .. } => condition != Condition::ChangesDirection,
            Self::Pnl { .. } => {
                !matches!(condition, Condition::ChangesDirection | Condition::MovesBy)
            }
            _ => true,
        }
    }
}
