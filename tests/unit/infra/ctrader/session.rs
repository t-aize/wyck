use super::supervisor::*;
use super::*;

#[test]
fn failures_are_sorted_into_retry_refresh_or_end() {
    let server = |code: &str| Error::server(code, None, None, None);
    assert_eq!(classify(&Error::Transport("x".into())), Next::Retry);
    assert_eq!(classify(&Error::Timeout { operation: "x" }), Next::Retry);
    assert_eq!(classify(&Error::Closed), Next::Retry);
    assert_eq!(classify(&server("REQUEST_FREQUENCY_EXCEEDED")), Next::Retry);
    assert_eq!(
        classify(&server("SERVER_IS_UNDER_MAINTENANCE")),
        Next::Retry
    );
    assert_eq!(
        classify(&server("CH_ACCESS_TOKEN_INVALID")),
        Next::RefreshThenRetry
    );
    assert_eq!(
        classify(&server("OA_AUTH_TOKEN_EXPIRED")),
        Next::RefreshThenRetry
    );
    assert_eq!(classify(&server("ACCOUNT_NOT_AUTHORIZED")), Next::Fail);
    assert_eq!(classify(&server("CH_CLIENT_AUTH_FAILURE")), Next::Fail);
    assert_eq!(classify(&Error::Config("x".into())), Next::Fail);
}

#[test]
fn a_refusal_of_the_token_endpoint_ends_the_session_but_an_unreachable_one_does_not() {
    assert_eq!(
        classify_refresh_error(&Error::Auth("ACCESS_DENIED".into())),
        Next::Fail
    );
    assert_eq!(
        classify_refresh_error(&Error::Transport("unreachable".into())),
        Next::Retry
    );
}

#[test]
fn transient_errors_are_the_ones_a_reconnect_can_cure() {
    assert!(error_is_transient(&Error::Closed));
    assert!(error_is_transient(&Error::Timeout { operation: "x" }));
    assert!(!error_is_transient(&Error::server(
        "SYMBOL_NOT_FOUND",
        None,
        None,
        None
    )));
}

#[tokio::test(start_paused = true)]
async fn a_stop_cuts_a_wait_short() {
    let (tx, mut rx) = watch::channel(false);
    let waiting =
        tokio::spawn(async move { sleep_or_stop(Duration::from_secs(3600), &mut rx).await });
    tokio::time::sleep(Duration::from_secs(1)).await;
    tx.send_replace(true);
    assert!(waiting.await.unwrap(), "stopped before the hour was up");
}

#[tokio::test(start_paused = true)]
async fn a_wait_that_is_not_interrupted_runs_its_course() {
    let (_tx, mut rx) = watch::channel(false);
    assert!(!sleep_or_stop(Duration::from_secs(5), &mut rx).await);
}
