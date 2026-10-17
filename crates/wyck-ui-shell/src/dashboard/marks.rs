//! Pictures for broker symbols. Known companies use bundled logos, metals use their chemical
//! symbols, and physical products use distinct glyphs. Unknown symbols keep a class glyph or
//! their own letters.
//!
//! Flags are from circle-flags (MIT), coin logos from cryptocurrency-icons (CC0) and company
//! logos from Simple Icons (CC0); the licenses sit next to the files in `assets/marks`. Company
//! names and logos belong to their owners.

mod commodity;
mod crypto;
mod forex;
mod index;
mod metal;
mod paint;
mod shares;
#[cfg(test)]
mod tests;

use self::commodity::commodity;
pub(super) use self::commodity::known_physical_class;
use self::crypto::crypto;
use self::forex::{currency_flag, currency_mark, split_pair};
use self::index::index_flag;
use self::metal::metal;
pub(crate) use self::paint::render;
use self::shares::share;

use super::catalog::Class;
use wyck_ui_kit::icon::IconName;

/// One round picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Mark {
    /// A country's round flag, by its code.
    Flag(&'static str),
    /// A ready-made colored round logo (a coin).
    Logo(String),
    /// A one-color logo, drawn in `color` on a light disc (or in white on a disc of `color`).
    Brand { path: String, color: u32 },
    /// A metal's chemical symbol on a disc the color of the metal.
    Metal { symbol: &'static str, color: u32 },
    /// A glyph on a tinted disc.
    Glyph { icon: IconName, color: u32 },
    /// Letters on a tinted disc.
    Letters { text: String, color: u32 },
}

/// The picture of a symbol: one mark, or two overlapped (the second one smaller, at the corner).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Icon {
    pub(crate) primary: Mark,
    pub(crate) secondary: Option<Mark>,
}

impl Icon {
    fn single(mark: Mark) -> Self {
        Self {
            primary: mark,
            secondary: None,
        }
    }

    fn with(primary: Mark, secondary: Option<Mark>) -> Self {
        Self { primary, secondary }
    }
}

/// The leading letters of a ticker, upper-cased: `US2000.cash` gives `US`, `N25.cash` gives `N`.
fn leading_letters(ticker: &str) -> String {
    ticker
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_uppercase()
}

/// The picture of a symbol, from what the broker says about it.
pub(crate) fn icon_for(
    ticker: &str,
    class: Class,
    base: Option<&str>,
    quote: Option<&str>,
) -> Icon {
    let quote_flag = || quote.and_then(currency_flag).map(Mark::Flag);
    let fallback = || {
        Icon::single(Mark::Glyph {
            icon: class.icon(),
            color: 0x00A1_A1A1,
        })
    };
    match class {
        Class::Forex => {
            let pair = split_pair(ticker);
            let base = base
                .map(str::to_owned)
                .or_else(|| pair.as_ref().map(|(b, _)| b.clone()));
            let quote = quote.map(str::to_owned).or_else(|| pair.map(|(_, q)| q));
            match (base, quote) {
                (Some(b), Some(q)) => Icon::with(currency_mark(&b), Some(currency_mark(&q))),
                _ => fallback(),
            }
        }
        Class::Metals => metal(ticker).map_or_else(fallback, |m| Icon::with(m, quote_flag())),
        Class::Crypto => Icon::with(crypto(ticker), quote_flag()),
        Class::Shares => Icon::with(share(ticker), quote_flag()),
        Class::Indices => {
            index_flag(ticker).map_or_else(fallback, |code| Icon::single(Mark::Flag(code)))
        }
        Class::Energies | Class::Commodities => {
            commodity(ticker).map_or_else(fallback, |(_, mark)| Icon::single(mark))
        }
        Class::Bonds | Class::Other => fallback(),
    }
}
