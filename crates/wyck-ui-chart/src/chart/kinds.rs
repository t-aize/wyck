//! The chart types as the menus list them: their icons and their sections.

use wyck_chart::ChartKind;
use wyck_ui_kit::icon::IconName;

/// The icon that stands for a chart type.
pub fn kind_icon(kind: ChartKind) -> IconName {
    match kind {
        ChartKind::Candles | ChartKind::Hollow | ChartKind::VolumeCandles => {
            IconName::ChartCandlestick
        }
        ChartKind::HeikinAshi => IconName::ChartNoAxesCombined,
        ChartKind::Bars => IconName::ChartNoAxesColumn,
        ChartKind::Line => IconName::ChartLine,
        ChartKind::Step => IconName::Activity,
        ChartKind::Area => IconName::ChartArea,
        ChartKind::Baseline => IconName::ChartSpline,
        ChartKind::Renko => IconName::Blocks,
        ChartKind::LineBreak => IconName::ChartColumnIncreasing,
        ChartKind::Kagi => IconName::Spline,
        ChartKind::PointFigure => IconName::Grid3x3,
        ChartKind::Range => IconName::ChartColumnBig,
        ChartKind::Footprint => IconName::Rows3,
        ChartKind::VolumeBars => IconName::ChartBarBig,
        ChartKind::Tpo => IconName::ChartNoAxesGantt,
    }
}

/// The kinds of the menu, in sections.
pub(super) const KIND_SECTIONS: [(&str, &[ChartKind]); 4] = [
    (
        "Bars",
        &[
            ChartKind::Candles,
            ChartKind::Hollow,
            ChartKind::VolumeCandles,
            ChartKind::HeikinAshi,
            ChartKind::Bars,
        ],
    ),
    (
        "Lines",
        &[
            ChartKind::Line,
            ChartKind::Step,
            ChartKind::Area,
            ChartKind::Baseline,
        ],
    ),
    (
        "Price based",
        &[
            ChartKind::Renko,
            ChartKind::LineBreak,
            ChartKind::Kagi,
            ChartKind::PointFigure,
            ChartKind::Range,
            ChartKind::VolumeBars,
        ],
    ),
    (
        "Order flow and profiles",
        &[ChartKind::Footprint, ChartKind::Tpo],
    ),
];
