//! Crypto currencies: the logo of the coin, or its letters.

use super::{Mark, leading_letters};
use crate::kit::assets::has_mark;

/// The coin logo file for a ticker such as `BTCUSD` (the broker shortens some coins' names).
pub(super) fn crypto_slug(ticker: &str) -> Option<&'static str> {
    let coin = ticker.strip_suffix("USD").unwrap_or(ticker);
    Some(match coin.to_ascii_uppercase().as_str() {
        "AAV" => "aave",
        "ADA" => "ada",
        "ALG" => "algo",
        "AVA" => "avax",
        "BAR" => "hbar",
        "BCH" => "bch",
        "BNB" => "bnb",
        "BTC" => "btc",
        "DASH" => "dash",
        "DOGE" => "doge",
        "DOT" => "dot",
        "ETC" => "etc",
        "ETH" => "eth",
        "GAL" => "gala",
        "GRT" => "grt",
        "ICP" => "icp",
        "IMX" => "imx",
        "LNK" => "link",
        "LTC" => "ltc",
        "MAN" => "mana",
        "NEO" => "neo",
        "NER" => "near",
        "SAN" => "sand",
        "SOL" => "sol",
        "UNI" => "uni",
        "VEC" => "vet",
        "XLM" => "xlm",
        "XMR" => "xmr",
        "XRP" => "xrp",
        "XTZ" => "xtz",
        _ => return None,
    })
}

pub(super) fn crypto(ticker: &str) -> Mark {
    let letters = || Mark::Letters {
        text: leading_letters(ticker).chars().take(3).collect(),
        color: 0x00F7_931A,
    };
    let Some(slug) = crypto_slug(ticker) else {
        return letters();
    };
    let colored = format!("marks/crypto/{slug}.svg");
    if has_mark(&colored) {
        return Mark::Logo(colored);
    }
    // A few coins only have the one-color logo of Simple Icons.
    let mono = match slug {
        "hbar" => Some(("hedera", 0x0022_2222)),
        "near" => Some(("near", 0x0000_0000)),
        _ => None,
    };
    match mono {
        Some((file, color)) if has_mark(&format!("marks/brands/{file}.svg")) => Mark::Brand {
            path: format!("marks/brands/{file}.svg"),
            color,
        },
        _ => letters(),
    }
}
