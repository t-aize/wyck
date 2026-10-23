//! Metals: the chemical symbol on a disc the color of the metal.

use super::Mark;

/// The mark of a metal, by the root of its ticker (`XAUUSD`, `GOLD`).
pub(super) fn metal(ticker: &str) -> Option<Mark> {
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
