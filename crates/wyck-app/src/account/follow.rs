//! Keeping the stop loss of a pending order on the ATR while it waits (see
//! [`wyck_trading::follow`]).
//!
//! The rule travels in the label of the order, so it is found again after a restart. The account
//! looks at the orders that carry one every few seconds, reads the ATR of their symbol on the
//! timeframe the rule names (the bars are loaded here, because no chart may be showing that
//! symbol) and amends the order when the stop is off by half a pip or more.

use crate::lost::lost;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::actions::Origin;
use super::{Account, Busy, Notice, Status, Tone, flatten};
use crate::market_data::load;
use crate::market_data::load::Loaded;
use crate::market_data::now_ms;
use crate::system::runtime;
use gpui::Context;
use wyck_market::Bar;
use wyck_market::Timeframe;
use wyck_trading::OrderStatus;
use wyck_trading::follow::{Step, Tracked, bars_are_fresh, next_step};
use wyck_trading::plan::Label;

/// How long the account waits before asking for bars again after a failure.
const RETRY_GAP: Duration = Duration::from_secs(15);

/// How long the account waits before amending the same order again, so a refusal is not asked
/// again at once.
const FOLLOW_GAP: Duration = Duration::from_secs(45);

/// The bars an ATR is read from, and when the last of them was loaded.
pub(super) struct AtrBars {
    bars: Vec<Bar>,
    loaded_ms: i64,
}

/// What the account remembers about the orders it follows.
#[derive(Default)]
pub(super) struct Following {
    bars: HashMap<(i64, Timeframe), AtrBars>,
    loading: HashSet<(i64, Timeframe)>,
    /// When a load last failed, so it is not asked again at once.
    failed: HashMap<(i64, Timeframe), Instant>,
    /// The stop the rule put on each order last, and when.
    set: HashMap<i64, (Instant, f64)>,
    /// Orders the user took over by moving their stop.
    released: HashSet<i64>,
}

/// One working order whose stop the rule on the ATR may move.
struct Job {
    id: i64,
    symbol: i64,
    timeframe: Timeframe,
    rule: wyck_trading::follow::AtrFollow,
    order: Tracked,
}

impl Account {
    /// Looks at the working orders that carry a rule on the ATR and amends the ones whose stop
    /// is no longer where the ATR puts it. Cheap when there is none.
    pub(super) fn follow_atr(&mut self, cx: &mut Context<Self>) {
        if self.status != Status::Ready {
            return;
        }
        let jobs: Vec<Job> = self
            .book
            .orders
            .values()
            .filter(|o| o.status() == Some(OrderStatus::Accepted))
            .filter_map(|o| {
                let rule = Label::decode(o.trade_data.label.as_deref()?)?.follow?;
                let timeframe = Timeframe::from_code(rule.atr.timeframe.as_deref()?)?;
                // A stop order enters at its stop price, a limit order at its limit price; a stop
                // limit order has both and enters at the stop price.
                let entry = o.stop_price.or(o.limit_price)?;
                Some(Job {
                    id: o.order_id,
                    symbol: o.trade_data.symbol_id,
                    timeframe,
                    rule,
                    order: Tracked {
                        buy: o.trade_data.is_buy(),
                        entry,
                        stop: o.stop_loss?,
                        target: o.take_profit,
                    },
                })
            })
            .collect();
        let ids: HashSet<i64> = jobs.iter().map(|j| j.id).collect();
        self.following.set.retain(|id, _| ids.contains(id));
        self.following.released.retain(|id| ids.contains(id));
        let wanted: HashSet<(i64, Timeframe)> =
            jobs.iter().map(|j| (j.symbol, j.timeframe)).collect();
        self.following.bars.retain(|key, _| wanted.contains(key));
        self.following.failed.retain(|key, _| wanted.contains(key));

        let (now, now_ms) = (Instant::now(), now_ms());
        for job in jobs {
            if self.following.released.contains(&job.id) {
                continue;
            }
            let key = (job.symbol, job.timeframe);
            let fresh = self.following.bars.get(&key).is_some_and(|b| {
                bars_are_fresh(job.timeframe, job.rule.atr.current_bar, b.loaded_ms, now_ms)
            });
            if !fresh {
                self.load_atr_bars(key, cx);
                continue;
            }
            let Some(loaded) = self.following.bars.get(&key) else {
                continue;
            };
            let last_open = loaded
                .bars
                .last()
                .is_some_and(|b| job.timeframe.bar_key(b.time_ms) == job.timeframe.bar_key(now_ms));
            let Some(atr) = job.rule.atr.value(&loaded.bars, last_open) else {
                continue;
            };
            let contract = self.book.contract(job.symbol);
            let pip = contract.pip();
            if pip <= 0.0 {
                continue;
            }
            let last_set = self.following.set.get(&job.id).map(|(_, stop)| *stop);
            match next_step(&job.rule, &job.order, atr, last_set, pip / 2.0, |p| {
                contract.round_price(p)
            }) {
                Step::Keep => {}
                Step::Release => {
                    self.following.released.insert(job.id);
                    Self::tell(
                        Notice::new(
                            Tone::Info,
                            "ATR following stopped",
                            "You moved the stop loss of an order that followed the ATR. Wyck leaves it where you put it.",
                        ),
                        cx,
                    );
                }
                Step::Move { stop, target } => {
                    let recent = self
                        .following
                        .set
                        .get(&job.id)
                        .is_some_and(|(at, _)| now.duration_since(*at) < FOLLOW_GAP);
                    if recent || self.busy.contains(&Busy::Amending(job.id)) {
                        continue;
                    }
                    self.following.set.insert(job.id, (now, stop));
                    self.protect_order_as(Origin::Plan, job.id, Some(stop), target, cx);
                }
            }
        }
    }

    /// Loads the bars an ATR is read from, then looks at the orders again.
    fn load_atr_bars(&mut self, key: (i64, Timeframe), cx: &mut Context<Self>) {
        if self
            .following
            .failed
            .get(&key)
            .is_some_and(|at| at.elapsed() < RETRY_GAP)
            || !self.following.loading.insert(key)
        {
            return;
        }
        let session = self.session.clone();
        let (symbol, timeframe) = key;
        cx.spawn(async move |this, cx| {
            let result = runtime::spawn(async move {
                load::initial(&session, symbol, timeframe, now_ms()).await
            })
            .await;
            lost(
                "account/follow: load_atr_bars",
                this.update(cx, |this, cx| {
                    this.following.loading.remove(&key);
                    if let Ok(Loaded::Bars(bars) | Loaded::Grouped { bars, .. }) = flatten(result) {
                        this.following.failed.remove(&key);
                        this.following.bars.insert(
                            key,
                            AtrBars {
                                bars,
                                loaded_ms: now_ms(),
                            },
                        );
                        this.follow_atr(cx);
                    } else {
                        this.following.failed.insert(key, Instant::now());
                    }
                }),
            );
        })
        .detach();
    }

    /// Whether any working order carries a rule on the ATR.
    pub(super) fn follows_atr(&self) -> bool {
        self.book.orders.values().any(|o| {
            o.status() == Some(OrderStatus::Accepted)
                && o.trade_data
                    .label
                    .as_deref()
                    .and_then(Label::decode)
                    .is_some_and(|l| l.follow.is_some())
        })
    }
}
