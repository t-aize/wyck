//! The picture of a symbol: two round flags for a forex pair, a metal's chemical symbol or a coin's
//! logo with the flag of its quote currency, a company's logo, the flag of an index's country, and
//! a glyph for a commodity. Nothing is invented: a symbol with no known picture falls back to its
//! letters.
//!
//! Flags are from circle-flags (MIT), coin logos from cryptocurrency-icons (CC0) and company
//! logos from Simple Icons (CC0); the licenses sit next to the files in `assets/marks`. Company
//! names and logos belong to their owners.

use gpui::prelude::*;
use gpui::{AnyElement, FontWeight, Rgba, div, img, px, rgb, svg};
use gpui_kit::assets::IconName;

use super::catalog::Class;
use crate::app::assets::has_mark;

/// One round picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
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
pub struct Icon {
    pub primary: Mark,
    pub secondary: Option<Mark>,
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

/// The flag of the country whose currency this is.
fn currency_flag(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_uppercase().as_str() {
        "EUR" => "eu",
        "USD" => "us",
        "GBP" => "gb",
        "JPY" => "jp",
        "CHF" => "ch",
        "AUD" => "au",
        "NZD" => "nz",
        "CAD" => "ca",
        "CNH" | "CNY" => "cn",
        "HKD" => "hk",
        "SGD" => "sg",
        "SEK" => "se",
        "NOK" => "no",
        "DKK" => "dk",
        "PLN" => "pl",
        "CZK" => "cz",
        "HUF" => "hu",
        "TRY" => "tr",
        "ZAR" => "za",
        "MXN" => "mx",
        "ILS" => "il",
        "THB" => "th",
        "INR" => "in",
        "KRW" => "kr",
        "BRL" => "br",
        "RUB" => "ru",
        _ => return None,
    })
}

/// A currency as a mark: its flag, or its letters when it has none.
fn currency_mark(code: &str) -> Mark {
    currency_flag(code).map_or_else(
        || Mark::Letters {
            text: code
                .chars()
                .take(3)
                .collect::<String>()
                .to_ascii_uppercase(),
            color: hashed_color(code),
        },
        Mark::Flag,
    )
}

/// The two currencies of a six-letter ticker such as `EURUSD`.
fn split_pair(ticker: &str) -> Option<(String, String)> {
    let ticker = ticker.trim();
    (ticker.len() == 6 && ticker.chars().all(|c| c.is_ascii_alphabetic())).then(|| {
        (
            ticker[..3].to_ascii_uppercase(),
            ticker[3..].to_ascii_uppercase(),
        )
    })
}

/// The leading letters of a ticker, upper-cased: `US2000.cash` gives `US`, `N25.cash` gives `N`.
fn leading_letters(ticker: &str) -> String {
    ticker
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_uppercase()
}

/// The country an index belongs to, from the start of its ticker.
fn index_flag(ticker: &str) -> Option<&'static str> {
    Some(match leading_letters(ticker).as_str() {
        "US" | "DXY" => "us",
        "AUS" => "au",
        "EU" => "eu",
        "FRA" => "fr",
        "GER" => "de",
        "HK" => "hk",
        "JP" => "jp",
        "N" => "nl",
        "SPN" => "es",
        "UK" => "gb",
        _ => return None,
    })
}

fn metal(ticker: &str) -> Option<Mark> {
    let (symbol, color) = match ticker.get(..3)?.to_ascii_uppercase().as_str() {
        "XAU" => ("Au", 0xE0B23A),
        "XAG" => ("Ag", 0xC5CBD3),
        "XCU" => ("Cu", 0xC9733D),
        "XPD" => ("Pd", 0x9FB3C8),
        "XPT" => ("Pt", 0xD8DEE6),
        _ => return None,
    };
    Some(Mark::Metal { symbol, color })
}

/// The coin logo file for a ticker such as `BTCUSD` (the broker shortens some coins' names).
fn crypto_slug(ticker: &str) -> Option<&'static str> {
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

fn crypto(ticker: &str) -> Mark {
    let letters = || Mark::Letters {
        text: leading_letters(ticker).chars().take(3).collect(),
        color: 0xF7931A,
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
        "hbar" => Some(("hedera", 0x222222)),
        "near" => Some(("near", 0x000000)),
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

/// A company's logo file and brand color, by the ticker the broker lists it under.
fn company(ticker: &str) -> Option<(&'static str, u32)> {
    Some(match ticker {
        "AAPL" => ("apple", 0x000000),
        "ADSGn" => ("adidas", 0x000000),
        "AIRF" => ("airfrance", 0x002157),
        "AMD" => ("amd", 0xED1C24),
        "AMZN" => ("amazon", 0xFF9900),
        "ARM" => ("arm", 0x0091BD),
        "AVGO" => ("broadcom", 0xE31837),
        "BA" => ("boeing", 0x1D439C),
        "BABA" => ("alibabadotcom", 0xFF6A00),
        "BAC" => ("bankofamerica", 0x012169),
        "BMW" => ("bmw", 0x0066B1),
        "CSCO" => ("cisco", 0x1BA0D7),
        "DBKGn" => ("deutschebank", 0x0018A8),
        "FDX" => ("fedex", 0x4D148C),
        "GE" => ("generalelectric", 0x0870D8),
        "GM" => ("generalmotors", 0x0170CE),
        "GOOG" => ("google", 0x4285F4),
        "IBM" => ("ibm", 0x0F62FE),
        "INTC" => ("intel", 0x0071C5),
        "KO" => ("cocacola", 0xD00013),
        "MBG" => ("mercedes", 0x000000),
        "MCD" => ("mcdonalds", 0xFBC817),
        "META" | "META " => ("meta", 0x0467DF),
        "MSFT" => ("microsoft", 0x0078D4),
        "MSTR" => ("microstrategy", 0xD9232E),
        "NFLX" => ("netflix", 0xE50914),
        "NKE" => ("nike", 0x111111),
        "NVDA" => ("nvidia", 0x76B900),
        "PLTR" => ("palantir", 0x101113),
        "QCOM" => ("qualcomm", 0x3253DC),
        "RACE" => ("ferrari", 0xD40000),
        "SBUX" => ("starbucks", 0x006241),
        "SIEGn" => ("siemens", 0x009999),
        "SNOW" => ("snowflake", 0x29B5E8),
        "SPCX" => ("spacex", 0x000000),
        "TSLA" => ("tesla", 0xCC0000),
        "V" => ("visa", 0x1A1F71),
        "VOWG_p" => ("volkswagen", 0x151F5D),
        "WMT" => ("walmart", 0x0071CE),
        "ZM" => ("zoom", 0x0B5CFF),
        _ => return None,
    })
}

fn share(ticker: &str) -> Mark {
    let ticker = ticker.trim();
    if let Some((slug, color)) = company(ticker) {
        let path = format!("marks/brands/{slug}.svg");
        if has_mark(&path) {
            return Mark::Brand { path, color };
        }
    }
    Mark::Letters {
        text: ticker
            .chars()
            .filter(char::is_ascii_uppercase)
            .take(3)
            .collect(),
        color: hashed_color(ticker),
    }
}

/// The glyph and color of an energy or an agricultural commodity, from the start of its ticker.
fn commodity(ticker: &str) -> Option<Mark> {
    let (icon, color) = match ticker.split('.').next()?.to_ascii_uppercase().as_str() {
        "COCOA" => (IconName::Cookie, 0x9A6B4B),
        "COFFEE" => (IconName::Coffee, 0xB07A56),
        "CORN" => (IconName::Wheat, 0xF2C94C),
        "COTTON" => (IconName::Shirt, 0xC7D2FE),
        "SOYBEAN" => (IconName::Bean, 0x9BC53D),
        "SUGAR" => (IconName::Candy, 0xF9A8D4),
        "WHEAT" => (IconName::Wheat, 0xE3B04B),
        "HEATOIL" => (IconName::Flame, 0xFF8A4C),
        "NATGAS" => (IconName::Flame, 0x60A5FA),
        "UKOIL" | "USOIL" => (IconName::Droplet, 0x94A3B8),
        _ => return None,
    };
    Some(Mark::Glyph { icon, color })
}

/// The picture of a symbol, from what the broker says about it.
pub fn icon_for(ticker: &str, class: Class, base: Option<&str>, quote: Option<&str>) -> Icon {
    let quote_flag = || quote.and_then(currency_flag).map(Mark::Flag);
    let fallback = || {
        Icon::single(Mark::Glyph {
            icon: class.icon(),
            color: 0xA1A1A1,
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
        Class::Indices => index_flag(ticker)
            .map(|code| Icon::single(Mark::Flag(code)))
            .unwrap_or_else(fallback),
        Class::Energies | Class::Commodities => {
            commodity(ticker).map(Icon::single).unwrap_or_else(fallback)
        }
        Class::Bonds | Class::Other => fallback(),
    }
}

/// A stable, readable color for letters that stand for something with no picture.
fn hashed_color(text: &str) -> u32 {
    const COLORS: [u32; 8] = [
        0x7C86FF, 0x38BDF8, 0x34D399, 0xFBBF24, 0xFB7185, 0xC084FC, 0xF97316, 0x2DD4BF,
    ];
    let sum = text
        .bytes()
        .fold(0usize, |a, b| a.wrapping_add(usize::from(b)));
    COLORS[sum % COLORS.len()]
}

/// Whether a brand color is too dark to show on the app's dark background.
fn is_dark(color: u32) -> bool {
    let (r, g, b) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
    (2126 * r + 7152 * g + 722 * b) / 10_000 < 60
}

fn tinted(color: u32, alpha: f32) -> Rgba {
    let mut c = rgb(color);
    c.a = alpha;
    c
}

/// One mark as a round picture of `diameter` pixels.
fn render_mark(mark: &Mark, diameter: f32) -> AnyElement {
    let disc = || {
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(diameter))
            .rounded_full()
            .overflow_hidden()
    };
    match mark {
        Mark::Flag(code) => img(format!("marks/flags/{code}.svg"))
            .size(px(diameter))
            .flex_none()
            .into_any_element(),
        Mark::Logo(path) => img(path.clone())
            .size(px(diameter))
            .flex_none()
            .into_any_element(),
        Mark::Brand { path, color } => {
            // A dark logo goes on a light disc, so it stays visible on the dark interface.
            let (background, glyph) = if is_dark(*color) {
                (rgb(0xF4F4F5), rgb(*color))
            } else {
                (rgb(*color), rgb(0xFFFFFF))
            };
            disc()
                .bg(background)
                .child(
                    svg()
                        .path(path.clone())
                        .size(px(diameter * 0.56))
                        .flex_none()
                        .text_color(glyph),
                )
                .into_any_element()
        }
        Mark::Metal { symbol, color } => disc()
            .bg(rgb(*color))
            .text_size(px(diameter * 0.42))
            .font_weight(FontWeight::BOLD)
            .text_color(rgb(0x2B2100))
            .child(*symbol)
            .into_any_element(),
        Mark::Glyph { icon, color } => disc()
            .bg(tinted(*color, 0.18))
            .child(
                svg()
                    .path(icon.path())
                    .size(px(diameter * 0.54))
                    .flex_none()
                    .text_color(rgb(*color)),
            )
            .into_any_element(),
        Mark::Letters { text, color } => disc()
            .bg(tinted(*color, 0.2))
            .text_size(px(diameter * 0.34))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(*color))
            .child(text.clone())
            .into_any_element(),
    }
}

/// The icon in a square of `size` pixels. A second mark is drawn smaller at the bottom right, ringed
/// in `ring` (the color behind the icon) so it reads as laid over the first.
pub fn render(icon: &Icon, size: f32, ring: Rgba) -> AnyElement {
    let Some(secondary) = &icon.secondary else {
        return div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(size))
            .child(render_mark(&icon.primary, size * 0.94))
            .into_any_element();
    };
    // A pair of flags is two equal discs. Otherwise the main picture (a coin, a company, a metal)
    // is big and the currency it is priced in is a small badge at the corner, so it never covers
    // the main one's letters.
    let both_flags = matches!(icon.primary, Mark::Flag(_)) && matches!(secondary, Mark::Flag(_));
    let (main, badge) = if both_flags {
        (size * 0.68, size * 0.68)
    } else {
        (size * 0.82, size * 0.46)
    };
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .child(render_mark(&icon.primary, main)),
        )
        .child(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .rounded_full()
                .border_2()
                .border_color(ring)
                .child(render_mark(secondary, badge - 3.0)),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forex_pair_is_two_flags() {
        let icon = icon_for("EURUSD", Class::Forex, Some("EUR"), Some("USD"));
        assert_eq!(icon.primary, Mark::Flag("eu"));
        assert_eq!(icon.secondary, Some(Mark::Flag("us")));
    }

    #[test]
    fn a_pair_falls_back_to_its_ticker_when_the_assets_are_unknown() {
        let icon = icon_for("GBPJPY", Class::Forex, None, None);
        assert_eq!(icon.primary, Mark::Flag("gb"));
        assert_eq!(icon.secondary, Some(Mark::Flag("jp")));
    }

    #[test]
    fn a_currency_without_a_flag_shows_letters() {
        assert!(matches!(currency_mark("XYZ"), Mark::Letters { .. }));
    }

    #[test]
    fn an_index_takes_the_flag_of_its_country() {
        let flag = |t| index_flag(t);
        assert_eq!(flag("US30.cash"), Some("us"));
        assert_eq!(flag("US2000.cash"), Some("us"));
        assert_eq!(flag("GER40.cash"), Some("de"));
        assert_eq!(flag("N25.cash"), Some("nl"));
        assert_eq!(flag("UK100.cash"), Some("gb"));
        assert_eq!(flag("DXY.cash"), Some("us"));
        assert_eq!(flag("NOPE.cash"), None);
    }

    #[test]
    fn metals_carry_their_chemical_symbol_and_the_flag_of_the_quote() {
        let icon = icon_for("XAUEUR", Class::Metals, Some("XAU"), Some("EUR"));
        assert!(matches!(icon.primary, Mark::Metal { symbol: "Au", .. }));
        assert_eq!(icon.secondary, Some(Mark::Flag("eu")));
    }

    #[test]
    fn coins_are_found_under_the_brokers_shortened_names() {
        assert_eq!(crypto_slug("BTCUSD"), Some("btc"));
        assert_eq!(crypto_slug("LNKUSD"), Some("link"));
        assert_eq!(crypto_slug("VECUSD"), Some("vet"));
        assert_eq!(crypto_slug("ZZZUSD"), None);
    }

    #[test]
    fn a_coin_with_a_bundled_logo_uses_it() {
        assert_eq!(crypto("BTCUSD"), Mark::Logo("marks/crypto/btc.svg".into()));
    }

    #[test]
    fn a_company_without_a_logo_shows_letters() {
        assert!(matches!(share("JPM"), Mark::Letters { .. }));
        assert!(matches!(share("AAPL"), Mark::Brand { .. }));
    }

    #[test]
    fn commodities_get_their_own_glyph() {
        assert!(commodity("COFFEE.c").is_some());
        assert!(commodity("USOIL.cash").is_some());
        assert!(commodity("UNKNOWN.c").is_none());
    }

    #[test]
    fn dark_brand_colors_are_recognized() {
        assert!(is_dark(0x000000));
        assert!(is_dark(0x111111));
        assert!(!is_dark(0xFF9900));
    }

    #[test]
    fn every_flag_a_mapping_can_name_is_bundled() {
        let codes = [
            "eu", "us", "gb", "jp", "ch", "au", "nz", "ca", "cn", "hk", "sg", "se", "no", "dk",
            "pl", "cz", "hu", "tr", "za", "mx", "il", "th", "in", "kr", "br", "ru", "de", "fr",
            "nl", "es",
        ];
        for code in codes {
            assert!(
                has_mark(&format!("marks/flags/{code}.svg")),
                "missing {code}"
            );
        }
    }
}
