//! Keeping under the request limits: the server allows 50 requests per second per connection (5
//! for history) and answers `REQUEST_FREQUENCY_EXCEEDED` beyond that.

use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::trace;

/// An even-spacing rate limiter.
#[derive(Debug)]
pub struct RateLimiter {
    interval: Duration,
    burst: Duration,
    next: Mutex<Instant>,
}

impl RateLimiter {
    /// A limiter of `per_second` requests per second.
    #[must_use]
    pub fn new(per_second: u32) -> Self {
        let rate = per_second.max(1);
        Self {
            interval: Duration::from_secs(1) / rate,
            // A burst of about a fifth of a second of requests, at least two: a quiet client can
            // send a couple at once, a busy one is smoothed.
            burst: Duration::from_secs(1) * 2 / rate.max(10),
            next: Mutex::new(Instant::now()),
        }
    }

    /// The gap kept between requests.
    #[must_use]
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Waits until the next request may be sent, then reserves that slot.
    pub async fn acquire(&self) {
        let wait_until = {
            let mut next = self.next.lock().await;
            let now = Instant::now();
            // Idle time is credited, but only up to the burst allowance.
            let earliest = now.checked_sub(self.burst).unwrap_or(now);
            let slot = (*next).max(earliest);
            *next = slot + self.interval;
            slot
        };
        // Sleep outside the lock, so later callers can queue up behind this one.
        let now = Instant::now();
        if wait_until > now {
            let wait = wait_until - now;
            trace!(?wait, "throttled to stay under the rate limit");
            tokio::time::sleep_until(wait_until).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn requests_are_spaced_at_the_configured_rate() {
        let limiter = RateLimiter::new(5);
        let start = Instant::now();
        for _ in 0..11 {
            limiter.acquire().await;
        }
        // 11 requests at 5 per second need about 2 seconds, less the burst credit at the start.
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(1_500), "{elapsed:?}");
        assert!(elapsed <= Duration::from_millis(2_200), "{elapsed:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_quiet_client_can_burst_a_little_but_not_a_lot() {
        let limiter = RateLimiter::new(50);
        let start = Instant::now();
        for _ in 0..3 {
            limiter.acquire().await;
        }
        assert!(
            start.elapsed() < Duration::from_millis(60),
            "a small burst is free"
        );
        tokio::time::sleep(Duration::from_secs(10)).await;
        let idle_start = Instant::now();
        for _ in 0..200 {
            limiter.acquire().await;
        }
        // Idle time is not banked without limit: 200 requests still take about 4 seconds.
        assert!(idle_start.elapsed() >= Duration::from_millis(3_700));
    }

    #[tokio::test(start_paused = true)]
    async fn callers_are_served_in_order() {
        let limiter = std::sync::Arc::new(RateLimiter::new(2));
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut tasks = Vec::new();
        for i in 0..6 {
            let (limiter, order) = (limiter.clone(), order.clone());
            tasks.push(tokio::spawn(async move {
                limiter.acquire().await;
                order.lock().unwrap().push(i);
            }));
            tokio::task::yield_now().await;
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_zero_rate_is_treated_as_one() {
        assert_eq!(RateLimiter::new(0).interval(), Duration::from_secs(1));
        assert_eq!(RateLimiter::new(50).interval(), Duration::from_millis(20));
    }
}
