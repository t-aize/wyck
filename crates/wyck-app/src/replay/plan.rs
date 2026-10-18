//! What a replay fetches next: plain rules over the range already held, tested without a window.

/// Empty history this long in a row, going back, means the history has begun.
pub(crate) const EMPTY_HISTORY_MS: i64 = 10 * DAY_MS;
const DAY_MS: i64 = 86_400_000;
const MAX_AHEAD_MS: i64 = 14 * DAY_MS;
const MINUTE_MS: i64 = 60_000;

/// The time range a store has asked the server for (both ends included). A range with no bar in
/// it is covered too: a weekend is not asked for twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Coverage {
    pub(crate) from_ms: i64,
    pub(crate) to_ms: i64,
    /// The server has nothing older.
    pub(crate) exhausted_before: bool,
}

impl Coverage {
    /// Nothing held yet, around `cursor_ms`.
    pub(crate) fn at(cursor_ms: i64) -> Self {
        Self {
            from_ms: cursor_ms,
            to_ms: cursor_ms - 1,
            exhausted_before: false,
        }
    }
}

/// What a store must hold for the cursor where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "every field is a time or a span in milliseconds"
)]
pub(crate) struct Need {
    pub(crate) cursor_ms: i64,
    /// How far before the cursor.
    pub(crate) back_ms: i64,
    /// How far after the cursor (never past `now_ms`).
    pub(crate) ahead_ms: i64,
    pub(crate) now_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Job {
    /// The chunk that ends just before `before_ms`.
    Before { before_ms: i64 },
    /// The chunk that starts at `from_ms`, up to `limit_ms`.
    Forward { from_ms: i64, limit_ms: i64 },
}

/// How far ahead of the cursor to keep data: a day at least, and what a minute of playing eats.
pub(crate) fn prefetch_ahead_ms(rate_hz: f64, step_ms: i64) -> i64 {
    let eaten =
        wyck_core::math::f64_to_i64(wyck_core::math::i64_to_f64(step_ms) * rate_hz.max(0.0) * 60.0);
    eaten.clamp(DAY_MS, MAX_AHEAD_MS)
}

/// The first minute not yet over at `now_ms`: the cursor goes no further, a minute in progress
/// is not final.
pub(crate) fn live_edge_ms(now_ms: i64) -> i64 {
    now_ms - now_ms.rem_euclid(MINUTE_MS)
}

/// Whether the history before the cursor is held (or the server has no more of it).
pub(crate) fn backdrop_held(coverage: &Coverage, need: &Need) -> bool {
    coverage.exhausted_before || coverage.from_ms <= need.cursor_ms - need.back_ms
}

/// Whether data reaches up to where the replay can go: the end of what is wanted ahead, or the
/// live edge.
pub(crate) fn forward_held(coverage: &Coverage, need: &Need) -> bool {
    coverage.to_ms >= forward_target(need)
}

fn forward_target(need: &Need) -> i64 {
    (need.cursor_ms + need.ahead_ms).min(live_edge_ms(need.now_ms) - 1)
}

/// The next request for a store, if it needs one. Older history comes first, so the picture
/// around the start is complete before the replay runs ahead.
pub(crate) fn next_job(coverage: &Coverage, need: &Need) -> Option<Job> {
    if !backdrop_held(coverage, need) {
        return Some(Job::Before {
            before_ms: coverage.from_ms,
        });
    }
    if !forward_held(coverage, need) {
        return Some(Job::Forward {
            from_ms: coverage.to_ms + 1,
            limit_ms: live_edge_ms(need.now_ms) - 1,
        });
    }
    None
}

/// Whether the replay can start from this store: the backdrop is held and data reaches the
/// cursor, so the first steps show something.
pub(crate) fn ready_to_play(coverage: &Coverage, need: &Need) -> bool {
    backdrop_held(coverage, need)
        && coverage.to_ms >= (need.cursor_ms + MINUTE_MS).min(live_edge_ms(need.now_ms) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_767_571_200_000;

    fn need(cursor_ms: i64) -> Need {
        Need {
            cursor_ms,
            back_ms: 2 * DAY_MS,
            ahead_ms: DAY_MS,
            now_ms: NOW,
        }
    }

    #[test]
    fn a_new_store_asks_for_the_past_first() {
        let cursor = NOW - 30 * DAY_MS;
        assert_eq!(
            next_job(&Coverage::at(cursor), &need(cursor)),
            Some(Job::Before { before_ms: cursor })
        );
    }

    #[test]
    fn once_the_past_is_held_it_asks_for_the_future_from_where_it_stops() {
        let cursor = NOW - 30 * DAY_MS;
        let coverage = Coverage {
            from_ms: cursor - 3 * DAY_MS,
            to_ms: cursor - 1,
            exhausted_before: false,
        };
        assert_eq!(
            next_job(&coverage, &need(cursor)),
            Some(Job::Forward {
                from_ms: cursor,
                limit_ms: live_edge_ms(NOW) - 1
            })
        );
    }

    #[test]
    fn a_store_that_holds_what_is_needed_asks_for_nothing() {
        let cursor = NOW - 30 * DAY_MS;
        let coverage = Coverage {
            from_ms: cursor - 3 * DAY_MS,
            to_ms: cursor + 2 * DAY_MS,
            exhausted_before: false,
        };
        assert_eq!(next_job(&coverage, &need(cursor)), None);
    }

    #[test]
    fn a_history_that_has_begun_is_not_asked_for_again() {
        let cursor = NOW - 30 * DAY_MS;
        let coverage = Coverage {
            from_ms: cursor - 100,
            to_ms: cursor + 2 * DAY_MS,
            exhausted_before: true,
        };
        assert!(backdrop_held(&coverage, &need(cursor)));
        assert_eq!(next_job(&coverage, &need(cursor)), None);
    }

    #[test]
    fn nothing_is_asked_past_the_live_edge() {
        let cursor = NOW - 60_000;
        let coverage = Coverage {
            from_ms: cursor - 3 * DAY_MS,
            to_ms: live_edge_ms(NOW) - 1,
            exhausted_before: false,
        };
        assert_eq!(next_job(&coverage, &need(cursor)), None);
    }

    #[test]
    fn the_live_edge_is_the_start_of_the_current_minute() {
        assert_eq!(live_edge_ms(NOW + 59_999), NOW);
        assert_eq!(live_edge_ms(NOW), NOW);
    }

    #[test]
    fn the_prefetch_grows_with_the_speed_within_bounds() {
        assert_eq!(prefetch_ahead_ms(1.0, 60_000), DAY_MS);
        assert_eq!(prefetch_ahead_ms(30.0, 3_600_000), MAX_AHEAD_MS);
        assert!(prefetch_ahead_ms(30.0, 600_000) > DAY_MS);
    }

    #[test]
    fn a_store_is_ready_when_the_past_is_held_and_data_reaches_the_cursor() {
        let cursor = NOW - 30 * DAY_MS;
        let mut coverage = Coverage {
            from_ms: cursor - 3 * DAY_MS,
            to_ms: cursor - 1,
            exhausted_before: false,
        };
        assert!(!ready_to_play(&coverage, &need(cursor)));
        coverage.to_ms = cursor + 25 * 3_600_000;
        assert!(ready_to_play(&coverage, &need(cursor)));
    }
}
