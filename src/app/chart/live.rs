//! Keeps the server's live bar subscription in step with what the chart shows.
//!
//! Switching symbol or timeframe quickly must not leave the wrong subscription behind, so the
//! changes are not sent from wherever they happen: they are written to a `watch` cell and one
//! task applies them one after the other, always aiming at the latest wish.

use tokio::sync::watch;
use wyck::openapi::market::Period;
use wyck::openapi::session::Session;

use crate::app::runtime;

pub type Wish = Option<(i64, Period)>;

pub struct LiveBars {
    wish: watch::Sender<Wish>,
}

impl LiveBars {
    pub fn new(session: Session) -> Self {
        let (wish, mut changes) = watch::channel::<Wish>(None);
        runtime::spawn(async move {
            let mut current: Wish = None;
            while changes.changed().await.is_ok() {
                let want = *changes.borrow_and_update();
                if want == current {
                    continue;
                }
                if let Some((symbol, period)) = current.take() {
                    let _ = session.unsubscribe_live_bars(symbol, period).await;
                }
                if let Some((symbol, period)) = want {
                    // Live bars ride on the price subscription of the symbol.
                    let _ = session.subscribe_spots(&[symbol]).await;
                    if let Err(error) = session.subscribe_live_bars(symbol, period).await {
                        tracing::warn!(%error, symbol, "could not follow the live bars");
                    }
                }
                current = want;
            }
            // The chart is gone: leave nothing subscribed.
            if let Some((symbol, period)) = current {
                let _ = session.unsubscribe_live_bars(symbol, period).await;
            }
        });
        Self { wish }
    }

    /// Follow this symbol and period from now on, or nothing.
    pub fn set(&self, wish: Wish) {
        self.wish.send_replace(wish);
    }
}
