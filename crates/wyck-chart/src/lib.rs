//! Chart models and calculations: data, studies (indicators), drawings, scene commands and export.
//!
//! The modules are private: everything the app uses is re-exported here, flat, so a path is
//! `wyck_chart::Cmd` and never `wyck_chart::scene::Cmd`. A new public item is added to the list
//! below, in the group of the module that holds it. Two names are never the same here, so a
//! module name is not needed to tell them apart.

mod axis;
mod data;
mod display;
mod export;
mod lines;
mod look;
mod options;
mod order_flow;
mod price;
mod projection;
mod replay;
mod scene;
mod settings;
mod transform;
mod view;
mod zone;

pub use axis::full_time;
pub use data::{
    MAX_BARS, MAX_TICKS, Series, aggregate_ticks, apply_live_bar, bucket_start, fold_group,
    fold_tick, group_bars, last_group, merge_bars, prepend_bars, prepend_ticks, splice_ticks,
    touch_last_bar, trim_front,
};
pub use display::{Display, study_input};
pub use export::PREVIEW_ROWS;
pub use export::choices::{
    Content, Decimal, Delimiter, Empty, ExportZone, Format, HeaderCase, LineEnding, Order,
    PriceDigits, Quote, RangeKind, TimeFormat,
};
pub use export::columns::ColumnKey;
pub use export::options::{ExportOptions, pattern_is_valid};
pub use export::presets::{Preset, builtin_presets};
pub use export::render::{clashes, file_name, render};
pub use export::source::{Cell, Source, Table, TableColumn};
pub use export::table::{date_is_valid, table};
pub use lines::LineId;
pub use look::{ChartContent, ChartLook};
pub use options::{ChartColors, CrosshairStyle, PriceLines, ScaleMargin, StatusLine, TradingLines};
pub use order_flow::flow::{BarFlow, Flow, Held, Level, Step, bar_open, next_step};
pub use order_flow::footprint::{
    CellMode, DEFAULT_BAR_PX, FootprintSettings, HeatScope, compact, signed, supports,
};
pub use order_flow::tpo::{
    Profile, SessionKind, SinglePrint, TpoColor, TpoDisplay, TpoSettings, format_clock, parse_clock,
};
pub use order_flow::volume::{
    ColorBy, Fill, VolumeBarSettings, VolumeCandleSettings, VolumeSize, WidthReference, WidthScale,
};
pub use price::{PriceMap, format_value, quote_unit};
pub use projection::ChartProjection;
pub use replay::{
    ClockStep, REPLAY_RATES, REPLAY_STEP_HELP, ReplayAdvance, ReplayBounds, ReplayChange,
    ReplayClock, ReplayFeed, ReplayInput, ReplayNoGaps, ReplayStep, ReplayTimeline, parse_step,
    replay_align_to_bar, replay_auto_step, replay_fold_minutes, replay_partial_bar,
    replay_random_start,
};
pub use scene::cmd::{
    Align, Cmd, P, fitted_origin, fitted_size, font, hsla, line_height, rgb_alpha, with_alpha,
};
pub use scene::color::{Hsla, Rgba};
pub use scene::geometry::{Band, Geometry, axis_h, axis_w, zone_w};
pub use scene::palette::Palette;
pub use scene::scale::{geometry, main_map};
pub use scene::{DrawingView, Frame, PriceMark, build};
pub use settings::{
    ChartKind, ChartSettings, DEFAULT_STUDIES_LIMIT, MAX_STUDIES, PaneRef, ScaleMode,
};
pub use transform::settings::{BoxSize, TransformSettings};
pub use transform::{KagiLine, PnfColumn, PricePath};
pub use view::{PriceScale, View};
pub use zone::{TIME_PATTERN, Zone};
