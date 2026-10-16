//! Pictures for broker symbols. Known companies use bundled logos, metals use their chemical
//! symbols, and physical products use distinct glyphs. Unknown symbols keep a class glyph or
//! their own letters.
//!
//! Flags are from circle-flags (MIT), coin logos from cryptocurrency-icons (CC0) and company
//! logos from Simple Icons (CC0); the licenses sit next to the files in `assets/marks`. Company
//! names and logos belong to their owners.

use gpui::prelude::*;
use gpui::{AnyElement, FontWeight, Rgba, div, img, px, rgb, svg};
use wyck_ui_kit::icon::IconName;

use super::catalog::Class;
use wyck_ui_kit::assets::has_mark;

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
    if ticker.len() != 6 || !ticker.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let (base, quote) = ticker.split_at_checked(3)?;
    Some((base.to_ascii_uppercase(), quote.to_ascii_uppercase()))
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
    let ticker = ticker.trim().to_ascii_uppercase();
    let root = ticker.split('.').next()?;
    let code = root.get(..3).unwrap_or(root);
    let (symbol, color) = match code {
        "XAU" => ("Au", 0x00E0_B23A),
        "XAG" => ("Ag", 0x00C5_CBD3),
        "XCU" => ("Cu", 0x00C9_733D),
        "XPD" => ("Pd", 0x009F_B3C8),
        "XPT" => ("Pt", 0x00D8_DEE6),
        "XAL" => ("Al", 0x00BC_C7D1),
        "XNI" => ("Ni", 0x00A7_B9A1),
        "XZN" => ("Zn", 0x009C_AFC0),
        "XPB" => ("Pb", 0x0085_91A0),
        "XSN" => ("Sn", 0x00BA_C6D0),
        _ => match root {
            "GOLD" => ("Au", 0x00E0_B23A),
            "SILVER" => ("Ag", 0x00C5_CBD3),
            "COPPER" | "HGCOPPER" => ("Cu", 0x00C9_733D),
            "PLATINUM" => ("Pt", 0x00D8_DEE6),
            "PALLADIUM" => ("Pd", 0x009F_B3C8),
            "ALUMINUM" | "ALUMINIUM" => ("Al", 0x00BC_C7D1),
            "NICKEL" => ("Ni", 0x00A7_B9A1),
            "ZINC" => ("Zn", 0x009C_AFC0),
            "LEAD" => ("Pb", 0x0085_91A0),
            "TIN" => ("Sn", 0x00BA_C6D0),
            "IRON" | "IRONORE" => ("Fe", 0x00A9_6848),
            "URANIUM" => ("U", 0x0093_A849),
            _ => return None,
        },
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

/// A company's logo file and brand color, by the ticker the broker lists it under.
fn company(ticker: &str) -> Option<(&'static str, u32)> {
    Some(match ticker {
        "MMM" => ("3m", 0x00D7_1920),
        "AAPL" => ("apple", 0x0000_0000),
        "ABNB" => ("airbnb", 0x00FF_5A5F),
        "ABT" => ("abbott", 0x0000_8CBA),
        "ABBV" => ("abbvie", 0x0007_1D49),
        "ADSGN" => ("adidas", 0x0000_0000),
        "ADSK" => ("autodesk", 0x0000_0000),
        "AIRF" => ("airfrance", 0x0000_2157),
        "AIR" => ("airbus", 0x0000_54A6),
        "AMD" => ("amd", 0x00ED_1C24),
        "AMZN" => ("amazon", 0x00FF_9900),
        "ARM" => ("arm", 0x0000_91BD),
        "AVGO" => ("broadcom", 0x00E3_1837),
        "AXP" => ("americanexpress", 0x002E_77BC),
        "BA" => ("boeing", 0x001D_439C),
        "BABA" => ("alibabadotcom", 0x00FF_6A00),
        "BAC" => ("bankofamerica", 0x0001_2169),
        "BARC" => ("barclays", 0x0000_AEEF),
        "BIDU" => ("baidu", 0x0029_32E1),
        "BILI" => ("bilibili", 0x0000_A1D6),
        "BKNG" => ("bookingdotcom", 0x0000_3580),
        "BMW" => ("bmw", 0x0000_66B1),
        "CAT" => ("caterpillar", 0x00FF_CD11),
        "CSCO" => ("cisco", 0x001B_A0D7),
        "DAL" => ("delta", 0x00C8_102E),
        "DDOG" => ("datadog", 0x0063_2CA6),
        "DE" => ("johndeere", 0x0036_7C2B),
        "DBKGN" => ("deutschebank", 0x0000_18A8),
        "DELL" => ("dell", 0x0000_7DB8),
        "ERIC" => ("ericsson", 0x0000_82F0),
        "FDX" => ("fedex", 0x004D_148C),
        "GE" => ("generalelectric", 0x0008_70D8),
        "GM" => ("generalmotors", 0x0001_70CE),
        "GOOG" | "GOOGL" => ("google", 0x0042_85F4),
        "GS" => ("goldmansachs", 0x0073_99C6),
        "HMC" => ("honda", 0x00CC_0000),
        "HPQ" => ("hp", 0x0000_96D6),
        "HSBC" => ("hsbc", 0x00DB_0011),
        "IBM" => ("ibm", 0x000F_62FE),
        "INTC" => ("intel", 0x0000_71C5),
        "INTU" => ("intuit", 0x0023_6CFF),
        "KO" => ("cocacola", 0x00D0_0013),
        "LNVGY" => ("lenovo", 0x00E2_231A),
        "LHA" => ("lufthansa", 0x0005_164D),
        "MA" => ("mastercard", 0x00EB_001B),
        "MBG" => ("mercedes", 0x0000_0000),
        "MCD" => ("mcdonalds", 0x00FB_C817),
        "MDB" => ("mongodb", 0x0047_A248),
        "META" => ("meta", 0x0004_67DF),
        "MSFT" => ("microsoft", 0x0000_78D4),
        "MSTR" => ("microstrategy", 0x00D9_232E),
        "NET" => ("cloudflare", 0x00F3_8020),
        "NFLX" => ("netflix", 0x00E5_0914),
        "NKE" => ("nike", 0x0011_1111),
        "NOK" => ("nokia", 0x0000_5AFF),
        "NVDA" => ("nvidia", 0x0076_B900),
        "P911" => ("porsche", 0x0000_0000),
        "PINS" => ("pinterest", 0x00BD_081C),
        "PLTR" => ("palantir", 0x0010_1113),
        "PYPL" => ("paypal", 0x0000_3087),
        "QCOM" => ("qualcomm", 0x0032_53DC),
        "RACE" => ("ferrari", 0x00D4_0000),
        "RBLX" => ("roblox", 0x0011_1111),
        "RDDT" => ("reddit", 0x00FF_4500),
        "RMS" => ("hermes", 0x00F3_7021),
        "RNO" => ("renault", 0x00FF_CC33),
        "RYA" | "RYAAY" => ("ryanair", 0x0007_3590),
        "RYCEY" => ("rollsroyce", 0x0010_069F),
        "SAP" => ("sap", 0x000F_A6D4),
        "SBUX" => ("starbucks", 0x0000_6241),
        "SHEL" => ("shell", 0x00FF_D500),
        "SIEGN" => ("siemens", 0x0000_9999),
        "SSNLF" => ("samsung", 0x0014_28A0),
        "SHOP" => ("shopify", 0x007A_B55C),
        "SNAP" => ("snapchat", 0x00FF_FC00),
        "SNOW" => ("snowflake", 0x0029_B5E8),
        "SONY" => ("sony", 0x0000_0000),
        "SPCX" => ("spacex", 0x0000_0000),
        "SPOT" => ("spotify", 0x001D_B954),
        "TCEHY" => ("tencenthy", 0x0000_52D9),
        "TEAM" => ("atlassian", 0x0000_52CC),
        "TGT" => ("target", 0x00CC_0000),
        "TM" => ("toyota", 0x00EB_0A1E),
        "TSLA" => ("tesla", 0x00CC_0000),
        "UBER" => ("uber", 0x0000_0000),
        "UBI" => ("ubisoft", 0x0000_0000),
        "U" => ("unity", 0x0000_0000),
        "UL" | "UNA" => ("unilever", 0x001F_36C7),
        "UPS" => ("ups", 0x006B_4006),
        "V" => ("visa", 0x001A_1F71),
        "VOWG_P" => ("volkswagen", 0x0015_1F5D),
        "VOLVB" | "VOLV_B" => ("volvo", 0x0000_3057),
        "WFC" => ("wellsfargo", 0x00D7_1E28),
        "WMT" => ("walmart", 0x0000_71CE),
        "XIACY" => ("xiaomi", 0x00FF_6900),
        "ZM" => ("zoom", 0x000B_5CFF),
        _ => return None,
    })
}

fn share(ticker: &str) -> Mark {
    let ticker = ticker.trim();
    let root = ticker.split('.').next().unwrap_or(ticker);
    if let Some((slug, color)) = company(&root.to_ascii_uppercase()) {
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

/// Products with common broker aliases. The broker's asset class still takes precedence.
fn commodity(ticker: &str) -> Option<(Class, Mark)> {
    let root = ticker.trim().split('.').next()?.to_ascii_uppercase();
    let (class, icon, color) = match root.as_str() {
        "USOIL" | "WTI" | "WTICOUSD" | "XTIUSD" | "CL" | "CRUDEOIL" => {
            (Class::Energies, IconName::Droplet, 0x0064_748B)
        }
        "UKOIL" | "BRENT" | "BRN" | "XBRUSD" | "BRENT_OIL" => {
            (Class::Energies, IconName::Droplet, 0x0094_A3B8)
        }
        "NATGAS" | "NGAS" | "NATURALGAS" | "NG" | "XNGUSD" => {
            (Class::Energies, IconName::Flame, 0x0060_A5FA)
        }
        "HEATOIL" | "HEATINGOIL" | "HO" => (Class::Energies, IconName::Flame, 0x00FF_8A4C),
        "GASOLINE" | "RBOB" | "RB" => (Class::Energies, IconName::Fuel, 0x00F5_9E0B),
        "COAL" => (Class::Energies, IconName::Pickaxe, 0x0071_717A),
        "ELECTRICITY" | "POWER" => (Class::Energies, IconName::Zap, 0x00FA_CC15),
        "COCOA" | "CC" => (Class::Commodities, IconName::Cookie, 0x009A_6B4B),
        "COFFEE" | "KC" => (Class::Commodities, IconName::Coffee, 0x00B0_7A56),
        "CORN" | "MAIZE" | "ZC" => (Class::Commodities, IconName::Wheat, 0x00F2_C94C),
        "COTTON" | "CT" => (Class::Commodities, IconName::Shirt, 0x00C7_D2FE),
        "SOYBEAN" | "SOYBEANS" | "SOY" | "ZS" => (Class::Commodities, IconName::Bean, 0x009B_C53D),
        "SOYMEAL" | "SOYOIL" => (Class::Commodities, IconName::Bean, 0x0088_A83B),
        "SUGAR" | "SB" => (Class::Commodities, IconName::Candy, 0x00F9_A8D4),
        "WHEAT" | "ZW" => (Class::Commodities, IconName::Wheat, 0x00E3_B04B),
        "RICE" | "ZR" | "OATS" | "ZO" | "BARLEY" => {
            (Class::Commodities, IconName::Wheat, 0x00D8_B36A)
        }
        "ORANGE" | "ORANGEJUICE" | "OJ" => (Class::Commodities, IconName::Citrus, 0x00F9_7316),
        "LUMBER" | "WOOD" | "LB" => (Class::Commodities, IconName::TreePine, 0x009A_683F),
        "CATTLE" | "LIVECATTLE" | "LE" | "FEEDERCATTLE" | "GF" => {
            (Class::Commodities, IconName::Beef, 0x00B7_6E55)
        }
        "LEANHOGS" | "HOGS" | "HE" => (Class::Commodities, IconName::Ham, 0x00EA_A0A0),
        "MILK" | "DAIRY" => (Class::Commodities, IconName::Milk, 0x00CB_D5E1),
        "PALMOIL" | "CANOLA" | "RAPESEED" => (Class::Commodities, IconName::Leaf, 0x0084_A642),
        "RUBBER" => (Class::Commodities, IconName::TreePine, 0x00A8_8964),
        _ => return None,
    };
    Some((class, Mark::Glyph { icon, color }))
}

#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the ticker is upper-cased above and these are the broker's own suffixes, not file extensions"
)]
pub(super) fn known_physical_class(ticker: &str) -> Option<Class> {
    if metal(ticker).is_some() {
        Some(Class::Metals)
    } else {
        let upper = ticker.trim().to_ascii_uppercase();
        let root = upper.split('.').next()?;
        if root.len() <= 2 && !upper.ends_with(".C") && !upper.ends_with(".CASH") {
            return None;
        }
        commodity(ticker).map(|(class, _)| class)
    }
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

/// A stable, readable color for letters that stand for something with no picture.
fn hashed_color(text: &str) -> u32 {
    const COLORS: [u32; 8] = [
        0x007C_86FF,
        0x0038_BDF8,
        0x0034_D399,
        0x00FB_BF24,
        0x00FB_7185,
        0x00C0_84FC,
        0x00F9_7316,
        0x002D_D4BF,
    ];
    let sum = text
        .bytes()
        .fold(0usize, |a, b| a.wrapping_add(usize::from(b)));
    COLORS.get(sum % COLORS.len()).copied().unwrap_or_default()
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
                (rgb(0x00F4_F4F5), rgb(*color))
            } else {
                (rgb(*color), rgb(0x00FF_FFFF))
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
            .text_color(rgb(0x002B_2100))
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
pub(crate) fn render(icon: &Icon, size: f32, ring: Rgba) -> AnyElement {
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
    fn company_logos_accept_broker_suffixes_and_unknowns_keep_letters() {
        assert!(matches!(share("JPM"), Mark::Letters { .. }));
        assert!(matches!(share("AAPL"), Mark::Brand { .. }));
        assert!(matches!(share("aapl.us"), Mark::Brand { .. }));
        assert!(matches!(share("ADSGn.DE"), Mark::Brand { .. }));
        assert!(matches!(share("VOWG_p.DE"), Mark::Brand { .. }));
        for ticker in [
            "ABNB", "AXP", "CAT", "DE", "MA", "NET", "P911", "PYPL", "SHOP", "SONY", "TGT", "WFC",
        ] {
            assert!(matches!(share(ticker), Mark::Brand { .. }), "{ticker}");
        }
    }

    #[test]
    fn physical_products_have_a_class_and_a_specific_mark() {
        for (ticker, class) in [
            ("GOLD", Class::Metals),
            ("XCUUSD", Class::Metals),
            ("ALUMINUM", Class::Metals),
            ("UKOIL.cash", Class::Energies),
            ("BRENT", Class::Energies),
            ("XNGUSD", Class::Energies),
            ("GASOLINE", Class::Energies),
            ("COFFEE.c", Class::Commodities),
            ("SOYBEANS", Class::Commodities),
            ("LUMBER", Class::Commodities),
            ("CATTLE", Class::Commodities),
        ] {
            assert_eq!(known_physical_class(ticker), Some(class), "{ticker}");
            assert!(
                !matches!(
                    icon_for(ticker, class, None, None).primary,
                    Mark::Letters { .. }
                ),
                "{ticker}"
            );
        }
        assert_eq!(known_physical_class("CL"), None);
        assert_eq!(known_physical_class("CL.c"), Some(Class::Energies));
        assert!(commodity("UNKNOWN.c").is_none());
    }

    #[test]
    fn dark_brand_colors_are_recognized() {
        assert!(is_dark(0x0000_0000));
        assert!(is_dark(0x0011_1111));
        assert!(!is_dark(0x00FF_9900));
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
