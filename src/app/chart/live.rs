//! Keeps the server's price and live bar subscriptions in step with what the app shows.
//!
//! Several charts may follow the same symbol and period, the header follows a symbol, the account
//! follows the symbols it holds positions in, and a chart that changes symbol or timeframe must
//! not cancel a subscription something else still needs. So nobody subscribes themselves: each
//! owner states what it wants to a [`LiveHub`], which works out the union, and one task applies
//! the differences one after the other, always aiming at the latest wish.
//!
//! Price events then come back through the session; [`LiveUpdate`] carries one to the charts with
//! its live bars already corrected (see [`wyck::openapi::market::LiveBarTracker`]).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

use tokio::sync::watch;
use wyck::openapi::market::{Bar, Period, SpotEvent};
use wyck::openapi::session::Session;

use crate::app::runtime;

/// What one owner wants followed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wish {
    /// Symbols whose prices it wants.
    pub spots: BTreeSet<i64>,
    /// A symbol and period whose live bars it wants.
    pub bars: Option<(i64, Period)>,
}

impl Wish {
    /// The prices of one symbol, and optionally its live bars of a period.
    pub fn symbol(symbol: i64, period: Option<Period>) -> Self {
        Self {
            spots: BTreeSet::from([symbol]),
            bars: period.map(|period| (symbol, period)),
        }
    }

    pub fn spots(symbols: impl IntoIterator<Item = i64>) -> Self {
        Self {
            spots: symbols.into_iter().collect(),
            bars: None,
        }
    }
}

/// Everything wanted together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Wanted {
    pub spots: BTreeSet<i64>,
    pub bars: BTreeSet<(i64, Period)>,
}

/// What each owner wants.
#[derive(Default)]
struct Wishes(HashMap<u64, Wish>);

impl Wishes {
    /// Records the wish of one owner and returns the union of every wish. A symbol with live bars
    /// always has its prices too: live bars ride on the price subscription.
    fn set(&mut self, owner: u64, wish: Option<Wish>) -> Wanted {
        match wish {
            Some(wish) if wish != Wish::default() => self.0.insert(owner, wish),
            _ => self.0.remove(&owner),
        };
        let mut wanted = Wanted::default();
        for wish in self.0.values() {
            wanted.spots.extend(&wish.spots);
            if let Some(bars) = wish.bars {
                wanted.spots.insert(bars.0);
                wanted.bars.insert(bars);
            }
        }
        wanted
    }
}

/// The steps that take the server from `current` to `want`, in the order they must run: live bars
/// go before the prices they ride on, and come after them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub unsubscribe_bars: Vec<(i64, Period)>,
    pub unsubscribe_spots: Vec<i64>,
    pub subscribe_spots: Vec<i64>,
    pub subscribe_bars: Vec<(i64, Period)>,
}

pub fn plan(current: &Wanted, want: &Wanted) -> Plan {
    Plan {
        unsubscribe_bars: current.bars.difference(&want.bars).copied().collect(),
        unsubscribe_spots: current.spots.difference(&want.spots).copied().collect(),
        subscribe_spots: want.spots.difference(&current.spots).copied().collect(),
        subscribe_bars: want.bars.difference(&current.bars).copied().collect(),
    }
}

/// Owners that are not charts.
pub const PEEK_OWNER: u64 = u64::MAX - 1;
pub const ACCOUNT_OWNER: u64 = u64::MAX - 2;

pub struct LiveHub {
    wishes: RefCell<Wishes>,
    wanted: watch::Sender<Wanted>,
}

impl LiveHub {
    pub fn new(session: Session) -> Self {
        let (wanted, mut changes) = watch::channel::<Wanted>(Wanted::default());
        runtime::spawn(async move {
            let mut current = Wanted::default();
            while changes.changed().await.is_ok() {
                let want = changes.borrow_and_update().clone();
                let steps = plan(&current, &want);
                for (symbol, period) in steps.unsubscribe_bars {
                    let _ = session.unsubscribe_live_bars(symbol, period).await;
                }
                if !steps.unsubscribe_spots.is_empty() {
                    let _ = session.unsubscribe_spots(&steps.unsubscribe_spots).await;
                }
                if !steps.subscribe_spots.is_empty()
                    && let Err(error) = session.subscribe_spots(&steps.subscribe_spots).await
                {
                    tracing::warn!(%error, "could not follow the prices");
                }
                for (symbol, period) in steps.subscribe_bars {
                    if let Err(error) = session.subscribe_live_bars(symbol, period).await {
                        tracing::warn!(%error, symbol, "could not follow the live bars");
                    }
                }
                current = want;
            }
            // Every owner is gone: leave nothing subscribed.
            for (symbol, period) in current.bars {
                let _ = session.unsubscribe_live_bars(symbol, period).await;
            }
            let spots: Vec<i64> = current.spots.into_iter().collect();
            if !spots.is_empty() {
                let _ = session.unsubscribe_spots(&spots).await;
            }
        });
        Self {
            wishes: RefCell::default(),
            wanted,
        }
    }

    /// The owner follows this from now on (or nothing, with `None`).
    pub fn set(&self, owner: u64, wish: Option<Wish>) {
        let union = self.wishes.borrow_mut().set(owner, wish);
        self.wanted.send_if_modified(|current| {
            if *current == union {
                false
            } else {
                *current = union;
                true
            }
        });
    }
}

/// A price event for the charts, with its live bars corrected.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveUpdate {
    pub symbol_id: i64,
    pub bid: Option<i64>,
    pub ask: Option<i64>,
    pub timestamp: Option<i64>,
    pub bars: Vec<(Period, Bar)>,
}

impl LiveUpdate {
    pub fn new(spot: &SpotEvent, bars: Vec<(Period, Bar)>) -> Self {
        Self {
            symbol_id: spot.symbol_id,
            bid: spot.bid,
            ask: spot.ask,
            timestamp: spot.timestamp,
            bars,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_charts_on_one_period_share_a_subscription() {
        let mut wishes = Wishes::default();
        wishes.set(1, Some(Wish::symbol(7, Some(Period::M5))));
        assert_eq!(
            wishes
                .set(2, Some(Wish::symbol(7, Some(Period::M5))))
                .bars
                .len(),
            1
        );
        assert_eq!(
            wishes.set(1, None).bars.len(),
            1,
            "the other chart still needs it"
        );
        let empty = wishes.set(2, None);
        assert!(empty.bars.is_empty() && empty.spots.is_empty());
    }

    #[test]
    fn a_chart_that_changes_period_moves_its_wish() {
        let mut wishes = Wishes::default();
        wishes.set(1, Some(Wish::symbol(7, Some(Period::M5))));
        let now = wishes.set(1, Some(Wish::symbol(7, Some(Period::H1))));
        assert_eq!(now.bars, BTreeSet::from([(7, Period::H1)]));
        assert_eq!(now.spots, BTreeSet::from([7]));
    }

    #[test]
    fn a_price_stays_while_anyone_wants_it() {
        let mut wishes = Wishes::default();
        wishes.set(u64::MAX, Some(Wish::spots([7])));
        wishes.set(1, Some(Wish::symbol(7, Some(Period::M1))));
        let after = wishes.set(1, Some(Wish::symbol(9, None)));
        assert_eq!(
            after.spots,
            BTreeSet::from([7, 9]),
            "the header still follows 7"
        );
        assert!(after.bars.is_empty());
    }

    #[test]
    fn the_plan_moves_bars_off_before_their_prices_and_on_after() {
        let current = Wanted {
            spots: BTreeSet::from([1, 2]),
            bars: BTreeSet::from([(1, Period::M1)]),
        };
        let want = Wanted {
            spots: BTreeSet::from([2, 3]),
            bars: BTreeSet::from([(3, Period::H1)]),
        };
        let steps = plan(&current, &want);
        assert_eq!(steps.unsubscribe_bars, vec![(1, Period::M1)]);
        assert_eq!(steps.unsubscribe_spots, vec![1]);
        assert_eq!(steps.subscribe_spots, vec![3]);
        assert_eq!(steps.subscribe_bars, vec![(3, Period::H1)]);
        assert_eq!(plan(&want, &want), Plan::default());
    }
}
