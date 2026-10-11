//! The presets of the export dialog.

use super::choices::{
    Decimal, Delimiter, ExportZone, Format, HeaderCase, LineEnding, RangeKind, TimeFormat,
};
use super::columns::ColumnKey;
use super::options::ExportOptions;

use serde::{Deserialize, Serialize};

/// A named set of options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub options: ExportOptions,
}

/// The presets that come with the app.
pub fn builtin_presets() -> Vec<Preset> {
    use ColumnKey as K;
    let ohlcv = vec![K::Open, K::High, K::Low, K::Close, K::Volume];
    let with = |first: Vec<ColumnKey>, rest: &[ColumnKey]| -> Vec<ColumnKey> {
        first.into_iter().chain(rest.iter().copied()).collect()
    };
    let base = ExportOptions::default();
    vec![
        Preset {
            name: "Standard CSV".to_owned(),
            options: ExportOptions {
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "Spreadsheet".to_owned(),
            options: ExportOptions {
                delimiter: Delimiter::Semicolon,
                decimal: Decimal::Comma,
                bom: true,
                line_ending: LineEnding::CrLf,
                time_format: TimeFormat::DateTime,
                ..base.clone()
            },
        },
        Preset {
            name: "NinjaTrader".to_owned(),
            options: ExportOptions {
                columns: with(vec![K::Time], &ohlcv),
                header: false,
                delimiter: Delimiter::Semicolon,
                time_format: TimeFormat::Custom,
                time_pattern: "%Y%m%d %H%M%S".to_owned(),
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "MetaTrader".to_owned(),
            options: ExportOptions {
                columns: with(vec![K::Date, K::Clock], &ohlcv),
                header: false,
                date_pattern: "%Y.%m.%d".to_owned(),
                clock_pattern: "%H:%M".to_owned(),
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "Unix time".to_owned(),
            options: ExportOptions {
                time_format: TimeFormat::UnixSeconds,
                header_case: HeaderCase::Lower,
                zone: ExportZone::Utc,
                ..base.clone()
            },
        },
        Preset {
            name: "JSON".to_owned(),
            options: ExportOptions {
                format: Format::Json,
                notes: true,
                header_case: HeaderCase::Lower,
                ..base.clone()
            },
        },
        Preset {
            name: "Everything".to_owned(),
            options: ExportOptions {
                columns: vec![
                    K::Time,
                    K::Open,
                    K::High,
                    K::Low,
                    K::Close,
                    K::Volume,
                    K::Change,
                    K::ChangePercent,
                    K::Range,
                    K::Body,
                    K::Direction,
                    K::Symbol,
                    K::Timeframe,
                ],
                notes: true,
                range: RangeKind::Loaded,
                ..base
            },
        },
    ]
}
