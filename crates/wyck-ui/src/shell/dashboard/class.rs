//! The kind of instrument, as the picker files it, and the class a broker's wording stands for.

use crate::kit::icon::IconName;

/// The kind of instrument, as the picker files it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Class {
    Forex,
    Metals,
    Energies,
    Indices,
    Crypto,
    Shares,
    Commodities,
    Bonds,
    Other,
}

impl Class {
    pub(crate) const ALL: [Self; 9] = [
        Self::Forex,
        Self::Metals,
        Self::Energies,
        Self::Indices,
        Self::Crypto,
        Self::Shares,
        Self::Commodities,
        Self::Bonds,
        Self::Other,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Forex => "Forex",
            Self::Metals => "Metals",
            Self::Energies => "Energies",
            Self::Indices => "Indices",
            Self::Crypto => "Crypto",
            Self::Shares => "Shares",
            Self::Commodities => "Commodities",
            Self::Bonds => "Bonds",
            Self::Other => "Other",
        }
    }

    pub(crate) fn icon(self) -> IconName {
        match self {
            Self::Forex => IconName::ArrowLeftRight,
            Self::Metals => IconName::Gem,
            Self::Energies => IconName::Fuel,
            Self::Indices => IconName::ChartLine,
            Self::Crypto => IconName::Bitcoin,
            Self::Shares => IconName::Building2,
            Self::Commodities => IconName::Wheat,
            Self::Bonds => IconName::Landmark,
            Self::Other => IconName::CircleDot,
        }
    }

    /// The class a broker's own wording stands for (`Forex`, `Crypto Currencies`, `Energies`).
    pub(crate) fn from_broker(text: &str) -> Self {
        let text = text.to_ascii_lowercase();
        let has = |needle: &str| text.contains(needle);
        if has("forex") || text == "fx" {
            Self::Forex
        } else if has("metal") {
            Self::Metals
        } else if has("energ") {
            Self::Energies
        } else if has("indic") || has("index") {
            Self::Indices
        } else if has("crypto") {
            Self::Crypto
        } else if has("share") || has("stock") || has("equit") {
            Self::Shares
        } else if has("commod") {
            Self::Commodities
        } else if has("bond") {
            Self::Bonds
        } else {
            Self::Other
        }
    }
}
