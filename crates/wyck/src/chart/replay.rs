//! Historical bid ticks delivered through the same update path as live prices.

use wyck_openapi::market::{Bar, Period, Tick};

use super::live::LiveUpdate;

/// Holds data at or after the picked time. Server bar times identify the real
/// period boundaries; OHLC values are built only from revealed ticks.
pub struct ReplayFeed {
    symbol_id: i64,
    period: Period,
    bar_times: Vec<i64>,
    ticks: Vec<Tick>,
    revealed: usize,
    current_bar: Option<Bar>,
}

impl ReplayFeed {
    pub fn new(symbol_id: i64, period: Period, bars: Vec<Bar>, ticks: Vec<Tick>) -> Self {
        let mut bar_times: Vec<i64> = bars.into_iter().map(|bar| bar.time_ms).collect();
        bar_times.sort_unstable();
        bar_times.dedup();
        let mut ticks = ticks;
        ticks.sort_by_key(|tick| tick.time_ms);
        Self {
            symbol_id,
            period,
            bar_times,
            ticks,
            revealed: 0,
            current_bar: None,
        }
    }

    pub fn append_ticks(&mut self, ticks: Vec<Tick>) {
        let last_time = self.ticks.last().map(|tick| tick.time_ms);
        let mut ticks = ticks;
        ticks.sort_by_key(|tick| tick.time_ms);
        self.ticks.extend(
            ticks
                .into_iter()
                .filter(|tick| last_time.is_none_or(|last| tick.time_ms >= last)),
        );
    }

    pub fn append_bars(&mut self, bars: Vec<Bar>) {
        self.bar_times
            .extend(bars.into_iter().map(|bar| bar.time_ms));
        self.bar_times.sort_unstable();
        self.bar_times.dedup();
    }

    pub fn next_time(&self) -> Option<i64> {
        self.ticks.get(self.revealed).map(|tick| tick.time_ms)
    }

    pub fn reveal_one(&mut self, cursor_ms: i64) -> Vec<LiveUpdate> {
        self.reveal_limit(cursor_ms, 1)
    }

    fn reveal_limit(&mut self, cursor_ms: i64, limit: usize) -> Vec<LiveUpdate> {
        let mut updates = Vec::new();
        while updates.len() < limit {
            let Some(tick) = self.ticks.get(self.revealed).copied() else {
                break;
            };
            if tick.time_ms > cursor_ms {
                break;
            }
            let at = self.bar_times.partition_point(|time| *time <= tick.time_ms);
            let bar_time = if at > 0 {
                self.bar_times[at - 1]
            } else {
                tick.time_ms.div_euclid(self.period.millis()) * self.period.millis()
            };
            let bar = match &mut self.current_bar {
                Some(bar) if bar.time_ms == bar_time => {
                    bar.high = bar.high.max(tick.price);
                    bar.low = bar.low.min(tick.price);
                    bar.close = tick.price;
                    bar.volume += 1;
                    *bar
                }
                _ => {
                    let bar = Bar {
                        time_ms: bar_time,
                        open: tick.price,
                        high: tick.price,
                        low: tick.price,
                        close: tick.price,
                        volume: 1,
                    };
                    self.current_bar = Some(bar);
                    bar
                }
            };
            updates.push(LiveUpdate {
                symbol_id: self.symbol_id,
                bid: Some(tick.price),
                ask: None,
                timestamp: Some(tick.time_ms),
                bars: vec![(self.period, bar)],
            });
            self.revealed += 1;
        }
        updates
    }

    pub fn is_exhausted(&self) -> bool {
        self.revealed >= self.ticks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_build_only_the_ohlc_seen_so_far() {
        let bars = vec![Bar {
            time_ms: 0,
            open: 1,
            high: 9,
            low: 0,
            close: 8,
            volume: 100,
        }];
        let ticks = vec![
            Tick {
                time_ms: 1,
                price: 5,
            },
            Tick {
                time_ms: 2,
                price: 7,
            },
        ];
        let mut feed = ReplayFeed::new(7, Period::M1, bars, ticks);
        let first = feed.reveal_one(1);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].bars[0].1.high, 5);
        assert_eq!(feed.next_time(), Some(2));
        let second = feed.reveal_one(2);
        assert_eq!(second[0].bars[0].1.high, 7);
        assert!(feed.is_exhausted());
    }

    #[test]
    fn one_step_reveals_one_tick_even_when_times_match() {
        let ticks = vec![
            Tick {
                time_ms: 1,
                price: 5,
            },
            Tick {
                time_ms: 1,
                price: 6,
            },
        ];
        let mut feed = ReplayFeed::new(7, Period::M1, Vec::new(), ticks);
        assert_eq!(feed.reveal_one(1).len(), 1);
        assert_eq!(feed.next_time(), Some(1));
        assert_eq!(feed.reveal_one(1)[0].bid, Some(6));
    }

    #[test]
    fn two_charts_keep_their_own_ticks_at_one_shared_time() {
        let mut first = ReplayFeed::new(
            7,
            Period::M1,
            Vec::new(),
            vec![
                Tick {
                    time_ms: 1,
                    price: 5,
                },
                Tick {
                    time_ms: 3,
                    price: 6,
                },
            ],
        );
        let mut second = ReplayFeed::new(
            8,
            Period::M5,
            Vec::new(),
            vec![Tick {
                time_ms: 2,
                price: 9,
            }],
        );
        assert_eq!(first.reveal_one(1)[0].symbol_id, 7);
        assert!(second.reveal_one(1).is_empty());
        assert_eq!(second.reveal_one(2)[0].symbol_id, 8);
        assert_eq!(first.next_time(), Some(3));
    }
}
