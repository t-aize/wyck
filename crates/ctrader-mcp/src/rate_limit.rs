//! A minimal async token-bucket rate limiter, used by [`crate::remote::RemoteClient`] to
//! stay under Remote's documented 5 requests/second cap on its historical-data endpoints
//! (`references/remote-http-server.md`, and `crate::workflows::backfill`'s doc comment).
//!
//! Not `governor` or another rate-limiting crate: a single global token bucket guarding
//! one client's outgoing calls is a couple dozen lines, and this workspace deliberately
//! keeps its dependency list small (see the `native-tls` comment on the workspace
//! `Cargo.toml`'s `rmcp` entry for the same philosophy applied elsewhere). Local is
//! intentionally never wrapped in this: the 5 req/s cap is specific to the Remote
//! `rest-proxy`, not a general MCP transport concern.

use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

struct Bucket {
    /// Fractional so a partial refill (e.g. after 150ms at 5 tokens/sec = 0.75 tokens)
    /// isn't lost to rounding between calls.
    tokens: f64,
    last_refill: Instant,
}

/// A token bucket allowing up to `capacity` requests to burst immediately, then
/// refilling at `rate_per_second` thereafter. [`RateLimiter::acquire`] resolves
/// immediately while tokens are available and otherwise sleeps for exactly as long as
/// the next token needs to accrue.
pub(crate) struct RateLimiter {
    rate_per_second: f64,
    capacity: f64,
    bucket: Mutex<Bucket>,
}

impl RateLimiter {
    /// `rate_per_second` tokens accrue per second, up to `capacity` banked at once. The
    /// bucket starts full, so the first `capacity` calls on a fresh limiter never wait.
    pub(crate) fn new(rate_per_second: f64, capacity: u32) -> Self {
        debug_assert!(rate_per_second > 0.0, "rate_per_second must be positive");
        debug_assert!(capacity > 0, "capacity must be positive");
        Self {
            rate_per_second,
            capacity: f64::from(capacity),
            bucket: Mutex::new(Bucket {
                tokens: f64::from(capacity),
                last_refill: Instant::now(),
            }),
        }
    }

    /// Waits, if necessary, until a token is available, then consumes it. Calls queue
    /// fairly in arrival order because the bucket is guarded by a single [`Mutex`] held
    /// across the wait.
    pub(crate) async fn acquire(&self) {
        loop {
            let wait = {
                let mut bucket = self.bucket.lock().await;
                let now = Instant::now();
                let elapsed = now
                    .saturating_duration_since(bucket.last_refill)
                    .as_secs_f64();
                bucket.tokens = (bucket.tokens + elapsed * self.rate_per_second).min(self.capacity);
                bucket.last_refill = now;

                if bucket.tokens >= 1.0 {
                    bucket.tokens -= 1.0;
                    None
                } else {
                    let shortfall = 1.0 - bucket.tokens;
                    Some(Duration::from_secs_f64(shortfall / self.rate_per_second))
                }
            };

            match wait {
                None => return,
                Some(duration) => tokio::time::sleep(duration).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_full_burst_up_to_capacity_never_waits() {
        let limiter = RateLimiter::new(5.0, 5);
        let start = Instant::now();

        for _ in 0..5 {
            limiter.acquire().await;
        }

        assert_eq!(
            Instant::now(),
            start,
            "the initial burst must not sleep at all"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn exceeding_the_burst_waits_for_the_next_token() {
        let limiter = RateLimiter::new(5.0, 5);
        for _ in 0..5 {
            limiter.acquire().await;
        }

        let start = Instant::now();
        limiter.acquire().await;
        let elapsed = Instant::now().saturating_duration_since(start);

        // At 5 tokens/sec, one token takes 200ms to accrue.
        assert_eq!(elapsed, Duration::from_millis(200));
    }

    #[tokio::test(start_paused = true)]
    async fn sustained_calls_are_paced_to_the_configured_rate() {
        let limiter = RateLimiter::new(5.0, 1);
        let start = Instant::now();

        for _ in 0..6 {
            limiter.acquire().await;
        }

        // 1 token up front (immediate) + 5 more at 200ms apart = 1000ms for the 6th call.
        let elapsed = Instant::now().saturating_duration_since(start);
        assert_eq!(elapsed, Duration::from_millis(1000));
    }

    #[tokio::test(start_paused = true)]
    async fn idle_time_refills_the_bucket_back_up_to_capacity() {
        let limiter = RateLimiter::new(5.0, 3);
        for _ in 0..3 {
            limiter.acquire().await;
        }

        tokio::time::advance(Duration::from_secs(10)).await;

        let start = Instant::now();
        for _ in 0..3 {
            limiter.acquire().await;
        }
        assert_eq!(
            Instant::now(),
            start,
            "a long idle period must refill the bucket, not just one token"
        );
    }
}
