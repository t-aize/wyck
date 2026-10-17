//! The columns an export can hold.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKey {
    Time,
    /// The date alone, by the date pattern.
    Date,
    /// The time of day alone, by the clock pattern.
    Clock,
    Open,
    High,
    Low,
    Close,
    /// The price of a tick.
    Price,
    Volume,
    /// The close against the close of the row before, in price.
    Change,
    ChangePercent,
    /// High minus low.
    Range,
    /// Close minus open.
    Body,
    /// The average of the high, low and close.
    Typical,
    /// The average of the high and low.
    Median,
    /// `up`, `down` or `flat`, by the close against the open.
    Direction,
    /// The number of the row, from 1.
    Index,
    Symbol,
    Timeframe,
    ChartType,
    /// The size of a box, reversal, range or row of the chart type, in price.
    BoxSize,
    // The levels of a TPO profile.
    Poc,
    ValueAreaHigh,
    ValueAreaLow,
    InitialBalanceHigh,
    InitialBalanceLow,
    Marks,
    Periods,
    PoorHigh,
    PoorLow,
    /// `yang` or `yin`, for the line of a Kagi chart.
    Thickness,
    /// `X` or `O`, for the column of a point and figure chart.
    Glyph,
    /// How many boxes a point and figure column has.
    Boxes,
    /// One line of an indicator: the position of the indicator in the chart, then of the line.
    Study {
        study: usize,
        plot: usize,
    },
}

impl ColumnKey {
    /// The name written in saved presets.
    pub fn code(self) -> String {
        match self {
            Self::Time => "time".into(),
            Self::Date => "date".into(),
            Self::Clock => "clock".into(),
            Self::Open => "open".into(),
            Self::High => "high".into(),
            Self::Low => "low".into(),
            Self::Close => "close".into(),
            Self::Price => "price".into(),
            Self::Volume => "volume".into(),
            Self::Change => "change".into(),
            Self::ChangePercent => "change_percent".into(),
            Self::Range => "range".into(),
            Self::Body => "body".into(),
            Self::Typical => "typical".into(),
            Self::Median => "median".into(),
            Self::Direction => "direction".into(),
            Self::Index => "index".into(),
            Self::Symbol => "symbol".into(),
            Self::Timeframe => "timeframe".into(),
            Self::ChartType => "chart_type".into(),
            Self::BoxSize => "box_size".into(),
            Self::Poc => "poc".into(),
            Self::ValueAreaHigh => "value_area_high".into(),
            Self::ValueAreaLow => "value_area_low".into(),
            Self::InitialBalanceHigh => "initial_balance_high".into(),
            Self::InitialBalanceLow => "initial_balance_low".into(),
            Self::Marks => "marks".into(),
            Self::Periods => "periods".into(),
            Self::PoorHigh => "poor_high".into(),
            Self::PoorLow => "poor_low".into(),
            Self::Thickness => "thickness".into(),
            Self::Glyph => "glyph".into(),
            Self::Boxes => "boxes".into(),
            Self::Study { study, plot } => format!("study.{study}.{plot}"),
        }
    }

    pub(crate) fn from_code(code: &str) -> Option<Self> {
        const FIXED: [ColumnKey; 33] = [
            ColumnKey::Time,
            ColumnKey::Date,
            ColumnKey::Clock,
            ColumnKey::Open,
            ColumnKey::High,
            ColumnKey::Low,
            ColumnKey::Close,
            ColumnKey::Price,
            ColumnKey::Volume,
            ColumnKey::Change,
            ColumnKey::ChangePercent,
            ColumnKey::Range,
            ColumnKey::Body,
            ColumnKey::Typical,
            ColumnKey::Median,
            ColumnKey::Direction,
            ColumnKey::Index,
            ColumnKey::Symbol,
            ColumnKey::Timeframe,
            ColumnKey::ChartType,
            ColumnKey::BoxSize,
            ColumnKey::Poc,
            ColumnKey::ValueAreaHigh,
            ColumnKey::ValueAreaLow,
            ColumnKey::InitialBalanceHigh,
            ColumnKey::InitialBalanceLow,
            ColumnKey::Marks,
            ColumnKey::Periods,
            ColumnKey::PoorHigh,
            ColumnKey::PoorLow,
            ColumnKey::Thickness,
            ColumnKey::Glyph,
            ColumnKey::Boxes,
        ];
        if let Some(rest) = code.strip_prefix("study.") {
            let (study, plot) = rest.split_once('.')?;
            return Some(Self::Study {
                study: study.parse().ok()?,
                plot: plot.parse().ok()?,
            });
        }
        FIXED.into_iter().find(|key| key.code() == code)
    }

    /// The name of the column when the user has not chosen one.
    pub(crate) fn default_name(self) -> String {
        match self {
            Self::Time => "Time".into(),
            Self::Date => "Date".into(),
            Self::Clock => "Clock".into(),
            Self::Open => "Open".into(),
            Self::High => "High".into(),
            Self::Low => "Low".into(),
            Self::Close => "Close".into(),
            Self::Price => "Price".into(),
            Self::Volume => "Volume".into(),
            Self::Change => "Change".into(),
            Self::ChangePercent => "Change %".into(),
            Self::Range => "Range".into(),
            Self::Body => "Body".into(),
            Self::Typical => "Typical".into(),
            Self::Median => "Median".into(),
            Self::Direction => "Direction".into(),
            Self::Index => "N".into(),
            Self::Symbol => "Symbol".into(),
            Self::Timeframe => "Timeframe".into(),
            Self::ChartType => "Type".into(),
            Self::BoxSize => "Box size".into(),
            Self::Poc => "POC".into(),
            Self::ValueAreaHigh => "VAH".into(),
            Self::ValueAreaLow => "VAL".into(),
            Self::InitialBalanceHigh => "IB high".into(),
            Self::InitialBalanceLow => "IB low".into(),
            Self::Marks => "Marks".into(),
            Self::Periods => "Periods".into(),
            Self::PoorHigh => "Poor high".into(),
            Self::PoorLow => "Poor low".into(),
            Self::Thickness => "Thickness".into(),
            Self::Glyph => "Glyph".into(),
            Self::Boxes => "Boxes".into(),
            Self::Study { study, plot } => format!("Study {} line {}", study + 1, plot + 1),
        }
    }

    /// What kind of value the column holds, for the formats that tell them apart.
    pub(super) fn is_text(self) -> bool {
        matches!(
            self,
            Self::Direction
                | Self::Symbol
                | Self::Timeframe
                | Self::ChartType
                | Self::Thickness
                | Self::Glyph
        )
    }
}

impl Serialize for ColumnKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.code())
    }
}

impl<'de> Deserialize<'de> for ColumnKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        // A column an older or newer version does not know is read as the time, and dropped
        // as a repeat by [`ExportOptions::normalized`] when the time is already there.
        Ok(Self::from_code(&code).unwrap_or(Self::Time))
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn the_codes_of_the_columns_are_a_file_format() {
        for code in [
            "time",
            "date",
            "clock",
            "open",
            "high",
            "low",
            "close",
            "price",
            "volume",
            "change",
            "change_percent",
            "range",
            "body",
            "typical",
            "median",
            "direction",
            "index",
            "symbol",
            "timeframe",
            "chart_type",
            "box_size",
            "poc",
            "value_area_high",
            "value_area_low",
            "initial_balance_high",
            "initial_balance_low",
            "marks",
            "periods",
            "poor_high",
            "poor_low",
            "thickness",
            "glyph",
            "boxes",
        ] {
            let key = ColumnKey::from_code(code).unwrap_or_else(|| panic!("{code}"));
            assert_eq!(key.code(), code);
        }
        assert_eq!(
            ColumnKey::from_code("study.2.1"),
            Some(ColumnKey::Study { study: 2, plot: 1 })
        );
        assert_eq!(ColumnKey::from_code("study.x.1"), None);
        assert_eq!(ColumnKey::from_code("nonsense"), None);
        assert_eq!(ColumnKey::Study { study: 0, plot: 3 }.code(), "study.0.3");
    }
}
