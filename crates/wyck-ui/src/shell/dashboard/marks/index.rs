//! Stock indices: the flag of the country an index belongs to.

use super::leading_letters;

/// The country an index belongs to, from the start of its ticker.
pub(super) fn index_flag(ticker: &str) -> Option<&'static str> {
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
