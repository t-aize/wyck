//! Energies and commodities: a glyph on a tinted disc, by the usual names of the product.

use super::Mark;
use super::metal::metal;
use crate::dashboard::class::Class;
use wyck_ui_kit::icon::IconName;

/// Products with common broker aliases. The broker's asset class still takes precedence.
pub(super) fn commodity(ticker: &str) -> Option<(Class, Mark)> {
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
pub(crate) fn known_physical_class(ticker: &str) -> Option<Class> {
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
