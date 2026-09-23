//! Keeps the server's live bar subscriptions in step with what the charts show.
//!
//! Several charts may follow the same symbol and period, and a chart that changes timeframe must
//! not cancel a subscription another chart still needs. So charts do not subscribe themselves:
//! each states what it wants to a [`LiveHub`], which works out the union and one task applies
//! the differences one after the other, always aiming at the latest wish.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use tokio::sync::watch;
use wyck::openapi::market::Period;
use wyck::openapi::session::Session;

use crate::app::runtime;

pub type Wish = Option<(i64, Period)>;
type Wanted = HashSet<(i64, Period)>;

/// What each chart wants, and so what all of them want together.
#[derive(Default)]
struct Wishes(HashMap<u64, (i64, Period)>);

impl Wishes {
    /// Records the wish of one chart and returns the union of every wish.
    fn set(&mut self, owner: u64, wish: Wish) -> Wanted {
        match wish {
            Some(wish) => self.0.insert(owner, wish),
            None => self.0.remove(&owner),
        };
        self.0.values().copied().collect()
    }
}

pub struct LiveHub {
    wishes: RefCell<Wishes>,
    wanted: watch::Sender<Wanted>,
}

impl LiveHub {
    pub fn new(session: Session) -> Self {
        let (wanted, mut changes) = watch::channel::<Wanted>(Wanted::new());
        runtime::spawn(async move {
            let mut current = Wanted::new();
            while changes.changed().await.is_ok() {
                let want = changes.borrow_and_update().clone();
                for (symbol, period) in current.difference(&want) {
                    let _ = session.unsubscribe_live_bars(*symbol, *period).await;
                }
                for (symbol, period) in want.difference(&current) {
                    // Live bars ride on the price subscription of the symbol.
                    let _ = session.subscribe_spots(&[*symbol]).await;
                    if let Err(error) = session.subscribe_live_bars(*symbol, *period).await {
                        tracing::warn!(%error, symbol, "could not follow the live bars");
                    }
                }
                current = want;
            }
            // Every chart is gone: leave nothing subscribed.
            for (symbol, period) in current {
                let _ = session.unsubscribe_live_bars(symbol, period).await;
            }
        });
        Self {
            wishes: RefCell::default(),
            wanted,
        }
    }

    /// The chart `owner` follows this symbol and period from now on, or nothing.
    pub fn set(&self, owner: u64, wish: Wish) {
        let union = self.wishes.borrow_mut().set(owner, wish);
        self.wanted.send_replace(union);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_charts_on_one_period_share_a_subscription() {
        let mut wishes = Wishes::default();
        wishes.set(1, Some((7, Period::M5)));
        assert_eq!(wishes.set(2, Some((7, Period::M5))).len(), 1);
        assert_eq!(
            wishes.set(1, None).len(),
            1,
            "the other chart still needs it"
        );
        assert!(wishes.set(2, None).is_empty());
    }

    #[test]
    fn a_chart_that_changes_period_moves_its_wish() {
        let mut wishes = Wishes::default();
        wishes.set(1, Some((7, Period::M5)));
        let now = wishes.set(1, Some((7, Period::H1)));
        assert_eq!(now, Wanted::from([(7, Period::H1)]));
    }
}
