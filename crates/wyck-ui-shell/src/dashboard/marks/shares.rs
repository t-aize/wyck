//! Shares: the logo of the company and its brand color, or the letters of the ticker.

use super::Mark;
use super::paint::hashed_color;
use wyck_ui_kit::assets::has_mark;

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

pub(super) fn share(ticker: &str) -> Mark {
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
