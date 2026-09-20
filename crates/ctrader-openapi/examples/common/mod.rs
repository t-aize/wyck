//! Shared by the examples: reading the environment, connecting, choosing an account, and reading
//! time ranges from the command line. Not part of the crate.

#![allow(dead_code)]

use std::time::{SystemTime, UNIX_EPOCH};

use ctrader_openapi::config::{ClientCredentials, ConnectionConfig, Environment};
use ctrader_openapi::{AccountClient, Client};

/// A variable that must be set. Exits with a clear message when it is not.
pub fn need(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        eprintln!("Set {name} first (see the top of the example, and the crate README).");
        std::process::exit(2);
    })
}

/// A variable that may be set.
pub fn maybe(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// The application credentials from the environment.
pub fn credentials() -> ClientCredentials {
    ClientCredentials::new(
        need("WYCK_OPENAPI_CLIENT_ID"),
        need("WYCK_OPENAPI_CLIENT_SECRET"),
    )
}

/// Demo unless `WYCK_OPENAPI_ENV=live`.
pub fn environment() -> Environment {
    match maybe("WYCK_OPENAPI_ENV").as_deref() {
        Some("live") => Environment::Live,
        _ => Environment::Demo,
    }
}

/// A connected, signed in client and the account it works on.
pub struct Setup {
    /// The connection.
    pub client: Client,
    /// The account, already authorized on the connection.
    pub account: AccountClient,
    /// The access token in use.
    pub access_token: String,
}

/// Connects, identifies the application, and authorizes an account.
///
/// The account is `WYCK_OPENAPI_ACCOUNT_ID` when set, otherwise the first demo account the token
/// covers. A live account is only used when the environment is `live` and it is named explicitly:
/// these examples read, but a real account deserves a deliberate choice.
pub async fn setup() -> Setup {
    let credentials = credentials();
    let access_token = need("WYCK_OPENAPI_ACCESS_TOKEN");
    let environment = environment();

    let client = Client::connect(&ConnectionConfig::new(environment))
        .await
        .expect("cannot connect");
    client
        .authenticate_application(&credentials)
        .await
        .expect("the application sign in failed");

    let accounts = client
        .accounts(&access_token)
        .await
        .expect("cannot list the accounts: is the access token valid?");
    let account_id = match maybe("WYCK_OPENAPI_ACCOUNT_ID").and_then(|v| v.parse::<i64>().ok()) {
        Some(id) => id,
        None => {
            let demo = accounts
                .ctid_trader_account
                .iter()
                .find(|a| a.is_live == Some(false));
            match demo {
                Some(account) => account.ctid_trader_account_id,
                None => {
                    eprintln!(
                        "The token covers no demo account. Name one with WYCK_OPENAPI_ACCOUNT_ID."
                    );
                    std::process::exit(2);
                }
            }
        }
    };
    let account = client.account(account_id);
    account
        .authorize(&access_token)
        .await
        .expect("the account sign in failed");
    eprintln!("Signed in on account {account_id} ({environment:?}).");
    Setup {
        client,
        account,
        access_token,
    }
}

/// The current time in Unix milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A point in time from the command line: `now`, a number of Unix milliseconds, or a length of time
/// ago written `30m`, `6h` or `2d`.
pub fn parse_time(text: &str, now: i64) -> Option<i64> {
    let text = text.trim();
    if text == "now" {
        return Some(now);
    }
    if let Ok(ms) = text.parse::<i64>() {
        return Some(ms);
    }
    let (digits, unit) = text.split_at(text.len().checked_sub(1)?);
    let count: i64 = digits.parse().ok()?;
    let per_unit = match unit {
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return None,
    };
    Some(now - count * per_unit)
}

/// The value after `--name` in the arguments, if given.
pub fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// The arguments that are not options (and not the values of options).
pub fn positional(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if arg.starts_with("--") {
            skip = true;
        } else {
            out.push(arg.clone());
        }
    }
    out
}

/// The range asked for with `--from` and `--to` (default: the last `default_span_ms`), or an
/// error message.
pub fn range(args: &[String], default_span_ms: i64) -> Result<(i64, i64), String> {
    let now = now_ms();
    let to = match option(args, "--to") {
        Some(text) => parse_time(&text, now).ok_or(format!("cannot read --to {text}"))?,
        None => now,
    };
    let from = match option(args, "--from") {
        Some(text) => parse_time(&text, now).ok_or(format!("cannot read --from {text}"))?,
        None => to - default_span_ms,
    };
    if from >= to {
        return Err("--from must be before --to".to_owned());
    }
    Ok((from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn times_are_read_as_now_milliseconds_or_a_time_ago() {
        assert_eq!(parse_time("now", NOW), Some(NOW));
        assert_eq!(parse_time("1789763388144", NOW), Some(1_789_763_388_144));
        assert_eq!(parse_time("30m", NOW), Some(NOW - 30 * 60_000));
        assert_eq!(parse_time("6h", NOW), Some(NOW - 6 * 3_600_000));
        assert_eq!(parse_time("2d", NOW), Some(NOW - 2 * 86_400_000));
        assert_eq!(parse_time(" 1h ", NOW), Some(NOW - 3_600_000));
    }

    #[test]
    fn nonsense_times_are_refused() {
        for text in ["", "h", "6x", "abc", "-", "1.5h", "3 h"] {
            assert_eq!(parse_time(text, NOW), None, "{text:?}");
        }
    }

    #[test]
    fn options_and_positional_arguments_are_told_apart() {
        let args = strings(&["EURUSD", "M5", "--from", "2d", "--out", "x.csv", "GBPUSD"]);
        assert_eq!(option(&args, "--from").as_deref(), Some("2d"));
        assert_eq!(option(&args, "--out").as_deref(), Some("x.csv"));
        assert_eq!(option(&args, "--to"), None);
        assert_eq!(positional(&args), strings(&["EURUSD", "M5", "GBPUSD"]));
    }

    #[test]
    fn a_flag_without_a_value_at_the_end_is_harmless() {
        let args = strings(&["EURUSD", "--depth"]);
        assert_eq!(option(&args, "--depth"), None);
        assert_eq!(positional(&args), strings(&["EURUSD"]));
    }

    #[test]
    fn the_range_defaults_to_the_span_before_now_and_refuses_a_backwards_one() {
        let (from, to) = range(&strings(&[]), 3_600_000).unwrap();
        assert_eq!(to - from, 3_600_000);
        let (from, to) = range(&strings(&["--from", "10h", "--to", "2h"]), 1).unwrap();
        assert_eq!(to - from, 8 * 3_600_000);
        assert!(range(&strings(&["--from", "1h", "--to", "2h"]), 1).is_err());
        assert!(range(&strings(&["--from", "nope"]), 1).is_err());
    }
}
