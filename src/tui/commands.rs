use crate::openapi::account::TradeSide;

pub const HELP: &str = "\
/watchlist, /positions, /orders, /console\n\
/add SYMBOL, /remove SYMBOL, /refresh\n\
/buy SYMBOL LOTS [--sl PRICE] [--tp PRICE]\n\
/sell SYMBOL LOTS [--sl PRICE] [--tp PRICE]\n\
/close POSITION_ID, /cancel ORDER_ID\n\
/clear, /logout, /quit\n\
Trade commands open a confirmation before sending.";

pub const NAMES: &[&str] = &[
    "help",
    "watchlist",
    "positions",
    "orders",
    "console",
    "add",
    "remove",
    "refresh",
    "buy",
    "sell",
    "close",
    "cancel",
    "clear",
    "logout",
    "quit",
];

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Watchlist,
    Positions,
    Orders,
    Console,
    Refresh,
    Clear,
    Logout,
    Quit,
    Add(String),
    Remove(String),
    Trade {
        side: TradeSide,
        symbol: String,
        lots: f64,
        sl: Option<f64>,
        tp: Option<f64>,
    },
    Close(i64),
    Cancel(i64),
}

pub fn parse(text: &str) -> Result<Command, String> {
    let mut words = text.trim().trim_start_matches('/').split_whitespace();
    let name = words.next().unwrap_or_default().to_ascii_lowercase();
    let args: Vec<&str> = words.collect();
    let usage = |syntax: &str| Err(format!("Usage: /{syntax}"));
    match name.as_str() {
        "buy" | "sell" => {
            if args.len() < 2 || !(args.len() - 2).is_multiple_of(2) {
                return usage("buy|sell SYMBOL LOTS [--sl PRICE] [--tp PRICE]");
            }
            let lots = positive(args[1], "Lots")?;
            let (mut sl, mut tp) = (None, None);
            for pair in args[2..].as_chunks::<2>().0 {
                let target = match pair[0] {
                    "--sl" => &mut sl,
                    "--tp" => &mut tp,
                    flag => return Err(format!("Unknown flag: {flag}")),
                };
                if target.is_some() {
                    return Err(format!("Repeated flag: {}", pair[0]));
                }
                *target = Some(positive(pair[1], "Price")?);
            }
            Ok(Command::Trade {
                side: if name == "buy" {
                    TradeSide::Buy
                } else {
                    TradeSide::Sell
                },
                symbol: args[0].to_owned(),
                lots,
                sl,
                tp,
            })
        }
        "add" | "remove" if args.len() == 1 => Ok(if name == "add" {
            Command::Add(args[0].to_owned())
        } else {
            Command::Remove(args[0].to_owned())
        }),
        "add" | "remove" => usage(&format!("{name} SYMBOL")),
        "close" | "cancel" if args.len() == 1 => {
            let id = args[0]
                .parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or_else(|| "ID must be a positive integer".to_owned())?;
            Ok(if name == "close" {
                Command::Close(id)
            } else {
                Command::Cancel(id)
            })
        }
        "close" | "cancel" => usage(&format!("{name} ID")),
        "help" | "watchlist" | "positions" | "orders" | "console" | "refresh" | "clear"
        | "logout" | "quit"
            if !args.is_empty() =>
        {
            usage(&name)
        }
        "help" => Ok(Command::Help),
        "watchlist" => Ok(Command::Watchlist),
        "positions" => Ok(Command::Positions),
        "orders" => Ok(Command::Orders),
        "console" => Ok(Command::Console),
        "refresh" => Ok(Command::Refresh),
        "clear" => Ok(Command::Clear),
        "logout" => Ok(Command::Logout),
        "quit" => Ok(Command::Quit),
        _ => Err(format!("Unknown command: {name}. Use /help.")),
    }
}

fn positive(text: &str, name: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or_else(|| format!("{name} must be a finite number above zero"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_flags_are_validated_before_opening_a_ticket() {
        assert_eq!(
            parse("/buy EURUSD 0.01 --tp 1.12 --sl 1.10"),
            Ok(Command::Trade {
                side: TradeSide::Buy,
                symbol: "EURUSD".into(),
                lots: 0.01,
                sl: Some(1.10),
                tp: Some(1.12)
            })
        );
        for bad in [
            "buy EURUSD NaN",
            "sell EURUSD inf",
            "buy EURUSD -1",
            "buy EURUSD 1 --sl 0",
            "buy EURUSD 1 --sl 1 --sl 2",
            "buy EURUSD 1 --wat 1",
            "buy EURUSD 1 --sl",
            "close -1",
            "quit now",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
