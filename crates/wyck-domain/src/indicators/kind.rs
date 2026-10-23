//! The indicators on offer, and what each one is (see [`Spec`]).

pub(crate) mod specs;

use serde::{Deserialize, Serialize};

use super::spec::Spec;

/// The indicators on offer. The names are written in saved files and never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StudyKind {
    Sma,
    Ema,
    Wma,
    Hma,
    MtfMa,
    Vwap,
    Bollinger,
    Keltner,
    Donchian,
    Ichimoku,
    ParabolicSar,
    VolumeProfile,
    Volume,
    Rsi,
    Macd,
    Stochastic,
    Atr,
    Cci,
    WilliamsR,
    Momentum,
    Obv,
    Dmi,
    Rma,
    Supertrend,
    Aroon,
    Alligator,
    Envelopes,
    LinearRegression,
    PivotPoints,
    Fractals,
    StochasticRsi,
    Roc,
    AwesomeOscillator,
    AcceleratorOscillator,
    Dpo,
    Trix,
    UltimateOscillator,
    RelativeVigorIndex,
    DeMarker,
    StandardDeviation,
    TrueRange,
    HistoricalVolatility,
    ChaikinVolatility,
    ChoppinessIndex,
    MoneyFlowIndex,
    ChaikinMoneyFlow,
    AccumulationDistribution,
    PriceVolumeTrend,
    ForceIndex,
    VolumeOscillator,
    EaseOfMovement,
    /// An indicator written as a script: which one is in [`super::config::StudyConfig::script`].
    Custom,
    /// An indicator a newer version wrote. Dropped on load.
    #[serde(other)]
    Unknown,
}

impl StudyKind {
    /// Every indicator a user can add, in menu order.
    pub const ALL: [Self; 51] = [
        Self::Sma,
        Self::Ema,
        Self::Wma,
        Self::Hma,
        Self::MtfMa,
        Self::Vwap,
        Self::Bollinger,
        Self::Keltner,
        Self::Donchian,
        Self::Ichimoku,
        Self::ParabolicSar,
        Self::VolumeProfile,
        Self::Volume,
        Self::Rsi,
        Self::Macd,
        Self::Stochastic,
        Self::Atr,
        Self::Cci,
        Self::WilliamsR,
        Self::Momentum,
        Self::Obv,
        Self::Dmi,
        Self::Rma,
        Self::Supertrend,
        Self::Aroon,
        Self::Alligator,
        Self::Envelopes,
        Self::LinearRegression,
        Self::PivotPoints,
        Self::Fractals,
        Self::StochasticRsi,
        Self::Roc,
        Self::AwesomeOscillator,
        Self::AcceleratorOscillator,
        Self::Dpo,
        Self::Trix,
        Self::UltimateOscillator,
        Self::RelativeVigorIndex,
        Self::DeMarker,
        Self::StandardDeviation,
        Self::TrueRange,
        Self::HistoricalVolatility,
        Self::ChaikinVolatility,
        Self::ChoppinessIndex,
        Self::MoneyFlowIndex,
        Self::ChaikinMoneyFlow,
        Self::AccumulationDistribution,
        Self::PriceVolumeTrend,
        Self::ForceIndex,
        Self::VolumeOscillator,
        Self::EaseOfMovement,
    ];

    pub fn spec(self) -> Spec {
        specs::spec_of(self)
    }
}
