//! The task behind a session: connecting, serving, renewing the tokens and waiting after a failure.

use super::{SessionConfig, SessionEvent, SessionState, Shared, TokenStore, lock};

use crate::infra::ctrader::auth::{OAuthClient, TokenSet};
use crate::infra::ctrader::error::{Error, ErrorKind, Result};
use crate::infra::ctrader::event::Event;
use crate::infra::ctrader::transport::connection::Client;
use secrecy::ExposeSecret;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{broadcast, watch};
use tracing::{debug, info, warn};

/// Whether an error is about the connection or the load, not about the request itself.
pub(super) fn error_is_transient(error: &Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::Transport
            | ErrorKind::Timeout
            | ErrorKind::Closed
            | ErrorKind::RateLimited
            | ErrorKind::Maintenance
    )
}

/// What to do after a connection attempt or a connection ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Next {
    /// Try again after a wait.
    Retry,
    /// Refresh the tokens, then try again at once.
    RefreshThenRetry,
    /// End the session: only the user can fix this.
    Fail,
}

/// Decides what a failure means for the session.
pub(super) fn classify(error: &Error) -> Next {
    match error.kind() {
        ErrorKind::TokenInvalid => Next::RefreshThenRetry,
        ErrorKind::NotAuthorized | ErrorKind::SignIn | ErrorKind::Config | ErrorKind::Rejected => {
            Next::Fail
        }
        ErrorKind::Transport
        | ErrorKind::Timeout
        | ErrorKind::Closed
        | ErrorKind::RateLimited
        | ErrorKind::Maintenance
        | ErrorKind::Protocol => Next::Retry,
    }
}

/// A cheap source of jitter: the sub-second part of the clock.
pub(super) fn noise() -> u32 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
}

/// Runs `future` unless the session is asked to stop first, which gives `None` and drops the future.
/// Every long step of the supervisor goes through it, so a stop is never held up by a connection
/// attempt or a token request that is still waiting for its timeout.
pub(super) async fn or_stop<T>(
    stop: &mut watch::Receiver<bool>,
    future: impl Future<Output = T>,
) -> Option<T> {
    if *stop.borrow() {
        return None;
    }
    tokio::select! {
        value = future => Some(value),
        _ = stop.changed() => None,
    }
}

/// Waits for `duration`, or returns `true` early when the session is asked to stop.
pub(super) async fn sleep_or_stop(duration: Duration, stop: &mut watch::Receiver<bool>) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::select! {
        () = tokio::time::sleep(duration) => false,
        _ = stop.changed() => true,
    }
}

/// The supervisor: connect, serve, and start over until told to stop or something unfixable happens.
pub(super) async fn supervise(
    config: SessionConfig,
    shared: Arc<Shared>,
    oauth: OAuthClient,
    store: Arc<dyn TokenStore>,
    mut tokens: TokenSet,
) {
    let mut stop = shared.stop.subscribe();
    let mut attempt: u32 = 0;
    let mut force_refresh = false;
    // A refresh forced by an invalid token is tried once; if the next connection still finds the
    // token invalid the refresh did not help and the session ends.
    let mut refreshed_for_invalid = false;

    let outcome: Option<Error> = 'session: loop {
        if *stop.borrow() {
            break None;
        }
        attempt += 1;
        shared.set_state(SessionState::Connecting { attempt });

        // 1. Tokens: refresh when they are about to expire, or when the server said they are bad.
        if force_refresh || tokens.expires_within(SystemTime::now(), config.refresh_margin) {
            // Token refresh is not cancellation safe: the endpoint may rotate the refresh
            // token before returning its answer. Finish the request and persistence attempt
            // before honoring a stop, so a stop cannot discard a pair already received.
            let refreshed = refresh(&oauth, &store, &shared, &tokens).await;
            if *stop.borrow() {
                break None;
            }
            match refreshed {
                Ok(fresh) => {
                    tokens = fresh;
                    force_refresh = false;
                }
                Err(error) => match classify_refresh_error(&error) {
                    Next::Fail => break 'session Some(error),
                    _ => {
                        // The endpoint could not be reached: wait and try again.
                        if let Some(end) =
                            wait_after_failure(&config, &shared, &mut stop, attempt, &error).await
                        {
                            break 'session end;
                        }
                        continue;
                    }
                },
            }
        }

        // 2. Connect and sign in.
        let Some(connected) = or_stop(&mut stop, connect(&config, &tokens)).await else {
            break None;
        };
        let client = match connected {
            Ok(client) => client,
            Err(error) => match classify(&error) {
                Next::Fail => break 'session Some(error),
                Next::RefreshThenRetry => {
                    if refreshed_for_invalid {
                        break 'session Some(error);
                    }
                    refreshed_for_invalid = true;
                    force_refresh = true;
                    attempt = attempt.saturating_sub(1);
                    continue;
                }
                Next::Retry => {
                    if let Some(end) =
                        wait_after_failure(&config, &shared, &mut stop, attempt, &error).await
                    {
                        break 'session end;
                    }
                    continue;
                }
            },
        };

        // 3. Serve: restore the subscriptions, announce, and forward events until it ends. The
        //    attempt counter is set below, by how the connection ends.
        refreshed_for_invalid = false;
        let mut events = client.events();
        shared.set_client(Some(client.clone()));
        if or_stop(&mut stop, restore_subscriptions(&client, &shared))
            .await
            .is_none()
        {
            client.close().await;
            break None;
        }
        shared.set_state(SessionState::Ready);
        shared.emit(SessionEvent::Ready);
        info!(account = shared.account_id, "the Open API session is ready");

        let connected_at = Instant::now();
        let mut refresh_in = refresh_delay(&tokens, &config);
        let ended = loop {
            let ended = serve(&client, &mut events, &shared, &mut stop, refresh_in).await;
            if !matches!(ended, Served::RefreshDue) {
                break ended;
            }
            // The connection stays up: the access token is only checked when signing in, so
            // renewing it now is what keeps the next reconnect from starting with a stale pair.
            match refresh(&oauth, &store, &shared, &tokens).await {
                Ok(fresh) => {
                    tokens = fresh;
                    refresh_in = refresh_delay(&tokens, &config);
                }
                Err(error) => {
                    warn!(%error, "the tokens could not be renewed ahead of time");
                    refresh_in = Some(config.min_refresh_wait * 5);
                }
            }
        };
        shared.set_client(None);
        match ended {
            Served::Stopped => {
                client.close().await;
                break None;
            }
            Served::TokensInvalid => {
                client.close().await;
                force_refresh = true;
                // Not a failure of the connection: reconnect at once with fresh tokens.
                attempt = 0;
            }
            // Never returned by the loop above; reconnecting is the harmless reading.
            Served::RefreshDue => {}
            Served::Disconnected(reason) => {
                // A connection that held for a while was a good one: start the backoff over. A
                // short one keeps the wait growing.
                if connected_at.elapsed() >= config.stable_after {
                    attempt = 1;
                }
                let error = Error::Transport(reason.clone());
                if let Some(end) =
                    wait_after_failure(&config, &shared, &mut stop, attempt, &error).await
                {
                    break 'session end;
                }
            }
        }
    };

    shared.set_client(None);
    match outcome {
        Some(error) => {
            warn!(%error, "the Open API session ended");
            shared.set_state(SessionState::Failed(error.to_string()));
            shared.emit(SessionEvent::Failed(error));
        }
        None => {
            shared.set_state(SessionState::Stopped);
            shared.emit(SessionEvent::Stopped);
        }
    }
}

/// How the served connection ended.
pub(super) enum Served {
    /// The session was asked to stop.
    Stopped,
    /// The server said the tokens are no longer valid.
    TokensInvalid,
    /// The access token is about to expire: renew it while the connection is up.
    RefreshDue,
    /// The connection ended.
    Disconnected(String),
}

/// Forwards the connection's events until it ends, the tokens are invalidated, or a stop is asked.
pub(super) async fn serve(
    client: &Client,
    events: &mut broadcast::Receiver<Event>,
    shared: &Shared,
    stop: &mut watch::Receiver<bool>,
    refresh_in: Option<Duration>,
) -> Served {
    let refresh_due = async {
        match refresh_in {
            Some(wait) => tokio::time::sleep(wait).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(refresh_due);
    loop {
        tokio::select! {
            _ = stop.changed() => return Served::Stopped,
            () = &mut refresh_due => return Served::RefreshDue,
            received = events.recv() => match received {
                Ok(Event::Disconnected(reason)) => return Served::Disconnected(format!("{reason:?}")),
                Ok(Event::TokensInvalidated(notice)) => {
                    let ours = notice.ctid_trader_account_ids.is_empty()
                        || notice.ctid_trader_account_ids.contains(&shared.account_id);
                    shared.emit(SessionEvent::Data(Event::TokensInvalidated(notice)));
                    if ours {
                        return Served::TokensInvalid;
                    }
                }
                // The server tore down just this account's session on an otherwise live
                // connection (the account was deleted, its cTID was deleted, or its token was
                // refreshed or revoked elsewhere). The documented recovery is to send a fresh
                // account auth, not to reconnect: the connection itself is still good.
                Ok(Event::AccountDisconnected(notice)) => {
                    let ours = notice.ctid_trader_account_id == shared.account_id;
                    shared.emit(SessionEvent::Data(Event::AccountDisconnected(notice)));
                    if ours && !reauthorize_account(client, shared).await {
                        return Served::Disconnected(
                            "the account session was ended by the server and could not be \
                             re-authorized"
                                .to_owned(),
                        );
                    }
                }
                // The server is ending every session on this connection. The documented recovery
                // is to reconnect and sign everything in again from scratch.
                Ok(Event::ServerDisconnecting(notice)) => {
                    shared.emit(SessionEvent::Data(Event::ServerDisconnecting(notice)));
                    return Served::Disconnected(
                        "the server is ending the connection".to_owned(),
                    );
                }
                Ok(event) => shared.emit(SessionEvent::Data(event)),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    warn!(missed, "the session fell behind and lost events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Served::Disconnected("the event channel closed".to_owned());
                }
            },
        }
        if client.is_closed() && events.is_empty() {
            return Served::Disconnected("the connection is closed".to_owned());
        }
    }
}

/// Re-sends the account auth after the server ends just this account's session
/// (`Event::AccountDisconnected`), and restores its subscriptions on success. `false` means the
/// current tokens no longer open the account either, and the connection should be replaced.
pub(super) async fn reauthorize_account(client: &Client, shared: &Shared) -> bool {
    let access_token = lock(&shared.tokens).access_token.clone();
    match client
        .account(shared.account_id)
        .authorize(access_token.expose_secret())
        .await
    {
        Ok(()) => {
            info!(
                account = shared.account_id,
                "re-authorized the account after the server ended its session"
            );
            restore_subscriptions(client, shared).await;
            true
        }
        Err(error) => {
            warn!(
                %error,
                account = shared.account_id,
                "could not re-authorize the account after the server ended its session"
            );
            false
        }
    }
}

/// Connects, identifies the application and authorizes the account.
pub(super) async fn connect(config: &SessionConfig, tokens: &TokenSet) -> Result<Client> {
    let client = Client::connect(&config.connection).await?;
    let signed_in = async {
        client.authenticate_application(&config.credentials).await?;
        client
            .account(config.account_id)
            .authorize(tokens.access_token.expose_secret())
            .await
    }
    .await;
    match signed_in {
        Ok(()) => Ok(client),
        Err(error) => {
            client.close().await;
            Err(error)
        }
    }
}

/// Subscribes again to everything in the registry. A refusal is reported and skipped: one bad
/// symbol must not stop the others, or the session.
pub(super) async fn restore_subscriptions(client: &Client, shared: &Shared) {
    let registry = lock(&shared.registry).clone();
    let market = client.account(shared.account_id).market();
    let report = |what: String, error: Error| {
        // Already subscribed is what we wanted.
        if error.code() != Some("ALREADY_SUBSCRIBED") {
            shared.emit(SessionEvent::SubscriptionFailed { what, error });
        }
    };
    if !registry.spots.is_empty() {
        let ids: Vec<i64> = registry.spots.iter().copied().collect();
        if let Err(error) = market.subscribe_spots(&ids).await {
            report(format!("prices of {ids:?}"), error);
        }
    }
    for (symbol, period) in &registry.live_bars {
        if let Err(error) = market.subscribe_live_bars(*symbol, *period).await {
            report(format!("live {} bars of {symbol}", period.label()), error);
        }
    }
    if !registry.depth.is_empty() {
        let ids: Vec<i64> = registry.depth.iter().copied().collect();
        if let Err(error) = market.subscribe_depth(&ids).await {
            report(format!("the order book of {ids:?}"), error);
        }
    }
}

/// How long to wait before renewing a pair that is still good, or `None` when it never expires.
/// Never less than `min_refresh_wait`, so a pair that is already inside the margin cannot make a busy loop.
pub(super) fn refresh_delay(tokens: &TokenSet, config: &SessionConfig) -> Option<Duration> {
    let expires_at = tokens.expires_at()?;
    let due = expires_at
        .checked_sub(config.refresh_margin)
        .and_then(|at| at.duration_since(SystemTime::now()).ok())
        .unwrap_or_default();
    Some(due.max(config.min_refresh_wait))
}

/// Refreshes the tokens and saves the new pair before returning it.
pub(super) async fn refresh(
    oauth: &OAuthClient,
    store: &Arc<dyn TokenStore>,
    shared: &Shared,
    tokens: &TokenSet,
) -> Result<TokenSet> {
    let fresh = oauth.refresh(tokens.refresh_token.expose_secret()).await?;
    // The endpoint has already retired the old refresh token, so the new pair is kept in memory
    // whatever the store does. A store that fails costs a sign-in after a restart, not the
    // running session.
    *lock(&shared.tokens) = fresh.clone();
    let saved = match tokio::time::timeout(Duration::from_secs(30), store.save(&fresh)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("saving the new tokens timed out".to_owned()),
    };
    match saved {
        Ok(()) => shared.emit(SessionEvent::TokensRefreshed),
        Err(reason) => {
            warn!(%reason, "the refreshed tokens could not be saved");
            shared.emit(SessionEvent::TokensNotSaved(reason));
        }
    }
    debug!("the Open API tokens were refreshed");
    Ok(fresh)
}

/// A refusal by the token endpoint ends the session; failing to reach it is retried.
pub(super) fn classify_refresh_error(error: &Error) -> Next {
    match error {
        Error::Transport(_) | Error::Timeout { .. } => Next::Retry,
        _ => Next::Fail,
    }
}

/// Announces the failed attempt and waits before the next one. Returns `Some(end)` when the session
/// must end instead: `Some(None)` for a stop, `Some(Some(error))` for giving up.
pub(super) async fn wait_after_failure(
    config: &SessionConfig,
    shared: &Shared,
    stop: &mut watch::Receiver<bool>,
    attempt: u32,
    error: &Error,
) -> Option<Option<Error>> {
    if config
        .max_reconnect_attempts
        .is_some_and(|max| attempt > max)
    {
        return Some(Some(Error::Transport(format!(
            "gave up after {} attempts: {error}",
            attempt.saturating_sub(1).max(1)
        ))));
    }
    // The server's own advice (rate limit, maintenance) is a floor for the wait.
    let mut wait = config.backoff.jittered(attempt, noise());
    if let Some(advice) = error.retry_after() {
        wait = wait.max(advice.min(config.backoff.max));
    }
    shared.set_state(SessionState::Waiting { attempt });
    shared.emit(SessionEvent::Reconnecting {
        attempt,
        retry_in: wait,
        reason: error.to_string(),
    });
    debug!(attempt, ?wait, %error, "the Open API connection is down, waiting");
    let started = Instant::now();
    if sleep_or_stop(wait, stop).await {
        debug!(waited = ?started.elapsed(), "stopped while waiting");
        return Some(None);
    }
    None
}
