//! Retry-with-backoff for the transient error classes [`crate::error::CTraderError`]
//! already documents as retryable.
//!
//! # Why this only ever wraps session establishment and read-only calls
//!
//! [`crate::remote::dto`]'s order DTOs carry an optional `label`/`comment`, but nothing
//! in this crate (or, as far as the `ctrader-mcp-servers` skill documents, the live
//! servers) enforces or verifies server-side idempotency deduplication on a mutating
//! `tools/call`. If a [`crate::error::CTraderError::Transport`] happens because the
//! response for a `create_order`/`amend_order`/`cancel_order`/`amend_position`/
//! `close_position` (Remote) or `place_*_order`/`cancel_all_pending_orders` (Local) call
//! was lost in transit, the request may already have reached the broker: blindly
//! retrying it risks placing (or cancelling) the same order twice. `retry_with_backoff`
//! is therefore only ever invoked by [`crate::transport::McpSession::connect`] (nothing
//! has been sent yet at that point) and by
//! [`crate::transport::McpSession::call_idempotent`] /
//! [`crate::transport::McpSession::call_no_args_idempotent`], which
//! [`crate::remote::RemoteClient`] and [`crate::local::LocalClient`] use only for their
//! read-only getters. Every mutating method on both clients deliberately keeps using the
//! plain, non-retried [`crate::transport::McpSession::call`] /
//! [`crate::transport::McpSession::call_no_args`].

use std::future::Future;
use std::time::Duration;

use crate::error::CTraderError;

/// How many attempts to make, and how long to wait between them, for a call wrapped in
/// `retry_with_backoff`. The wait doubles after each failed attempt (`base_delay`,
/// `2 * base_delay`, `4 * base_delay`, ...), capped at `max_delay`.
///
/// Deliberately has no random jitter: jitter exists to prevent many independent clients
/// from retrying in lockstep and hammering a shared server at the same instant, which
/// matters at the scale of a server-side thundering herd. `wyck` is a single desktop
/// client making at most a handful of concurrent calls, so a plain deterministic backoff
/// is sufficient and keeps this module dependency-free (no `rand`).
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    /// Total number of attempts (the first try plus every retry). `1` disables
    /// retrying entirely.
    pub max_attempts: u32,
    /// Delay before the second attempt. Doubles for each subsequent attempt.
    pub base_delay: Duration,
    /// Upper bound on the delay between any two attempts, regardless of how many
    /// doublings `base_delay` has gone through.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    /// 3 total attempts, starting at a 250ms delay and doubling up to a 4s cap (250ms,
    /// 500ms): enough to ride out a brief SSE reconnect or a transient DNS/TLS blip
    /// without leaving a caller waiting for more than a few seconds.
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(4),
        }
    }
}

impl RetryPolicy {
    /// A policy that never retries: every call gets exactly one attempt. Useful for
    /// tests that want deterministic, immediate failure, or for a caller that wants to
    /// implement its own retry strategy at a higher level.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            base_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
        }
    }

    /// The delay to wait after `attempt` (1-based) has just failed, before making
    /// attempt `attempt + 1`.
    fn delay_after(&self, attempt: u32) -> Duration {
        let doublings = attempt.saturating_sub(1);
        let scaled = 2u32
            .checked_pow(doublings)
            .and_then(|factor| self.base_delay.checked_mul(factor));
        scaled.unwrap_or(self.max_delay).min(self.max_delay)
    }
}

impl CTraderError {
    /// The maximum number of TOTAL attempts (first try + retries) this error class
    /// should ever be allowed, independent of what a [`RetryPolicy`] otherwise permits.
    /// Mirrors the per-variant guidance already documented on [`CTraderError`]'s own doc
    /// comments (the `self-healing-playbook.md` §3 classification matrix):
    ///
    /// - [`CTraderError::Connect`] / [`CTraderError::Transport`]: "safe to retry with
    ///   backoff": no independent cap, deferred entirely to the [`RetryPolicy`].
    /// - [`CTraderError::UpstreamBrokerError`]: "retry at most once": capped at 2 total
    ///   attempts even if the policy would otherwise allow more.
    /// - Everything else (schema mismatches, server rejections, local faults, decode
    ///   failures, ...): never retryable: capped at 1 (the original attempt only).
    pub(crate) fn max_retry_attempts(&self) -> u32 {
        match self {
            CTraderError::Connect { .. } | CTraderError::Transport { .. } => u32::MAX,
            CTraderError::UpstreamBrokerError { .. } => 2,
            _ => 1,
        }
    }
}

/// Calls `operation` repeatedly until it succeeds, it returns an error whose
/// [`CTraderError::max_retry_attempts`] has been reached, or `policy.max_attempts` has
/// been reached (whichever limit is stricter): sleeping with [`RetryPolicy`]'s
/// doubling backoff between attempts and logging every retry via [`tracing::warn`].
///
/// See this module's top-level doc comment for which call sites are safe to wrap in
/// this function.
pub(crate) async fn retry_with_backoff<T, F, Fut>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, CTraderError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CTraderError>>,
{
    let mut attempt: u32 = 1;
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                let cap = policy.max_attempts.min(error.max_retry_attempts());
                if attempt >= cap {
                    return Err(error);
                }
                let delay = policy.delay_after(attempt);
                tracing::warn!(
                    attempt,
                    next_delay = ?delay,
                    error = %error,
                    "retrying cTrader MCP call after a transient error"
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn transport_error() -> CTraderError {
        CTraderError::Transport {
            tool: "get_balance".into(),
            message: "connection reset".to_owned(),
        }
    }

    fn schema_mismatch() -> CTraderError {
        CTraderError::SchemaMismatch {
            tool: "get_trendbars".into(),
            message: "period must be one of ...".to_owned(),
        }
    }

    fn upstream_broker_error() -> CTraderError {
        CTraderError::UpstreamBrokerError {
            tool: "get_trendbars".into(),
            code: "502".to_owned(),
            message: "uProxy error: broker unavailable".to_owned(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn succeeds_immediately_without_sleeping() {
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(&RetryPolicy::default(), || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, CTraderError>(42) }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn recovers_after_transient_failures_within_policy() {
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(&RetryPolicy::default(), || {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
            async move {
                if attempt < 3 {
                    Err(transport_error())
                } else {
                    Ok(attempt)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 3);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_once_policy_max_attempts_is_exhausted() {
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(&RetryPolicy::default(), || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(transport_error()) }
        })
        .await;

        assert!(matches!(result, Err(CTraderError::Transport { .. })));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn non_retryable_errors_stop_after_the_first_attempt() {
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(&RetryPolicy::default(), || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(schema_mismatch()) }
        })
        .await;

        assert!(matches!(result, Err(CTraderError::SchemaMismatch { .. })));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn upstream_broker_errors_retry_at_most_once_even_with_a_higher_policy() {
        let generous_policy = RetryPolicy {
            max_attempts: 10,
            ..RetryPolicy::default()
        };
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(&generous_policy, || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(upstream_broker_error()) }
        })
        .await;

        assert!(matches!(
            result,
            Err(CTraderError::UpstreamBrokerError { .. })
        ));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn retry_policy_none_makes_exactly_one_attempt() {
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(&RetryPolicy::none(), || {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(transport_error()) }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn delay_after_doubles_and_caps() {
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(350),
        };
        assert_eq!(policy.delay_after(1), Duration::from_millis(100));
        assert_eq!(policy.delay_after(2), Duration::from_millis(200));
        // 400ms would be next but the policy caps at 350ms.
        assert_eq!(policy.delay_after(3), Duration::from_millis(350));
        assert_eq!(policy.delay_after(4), Duration::from_millis(350));
    }
}
