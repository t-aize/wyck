//! The volume chart types' part of the chart settings panel: how the candles get wide, how they
//! are colored and filled, and how much volume a volume bar holds. Every change applies at once.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Entity, Subscription, Window, div};
use gpui_kit::assets::IconName;
use gpui_kit::component::input::InputState;

use super::Chart;
use super::settings::ChartSettings;
use super::settings_rows::{Numbers, choice, edit, named, number, switch};
use super::transform::PricePath;
use super::volume::{
    ColorBy, Fill, VolumeBarSettings, VolumeCandleSettings, VolumeSize, WidthReference, WidthScale,
};
use crate::app::settings_ui as ui;
use crate::app::widgets;

/// The numbers that are typed in.
pub(super) struct Inputs {
    min_width: Entity<InputState>,
    max_width: Entity<InputState>,
    wick: Entity<InputState>,
    /// The volume of a bar, or its multiple of the average, by the mode the settings are in.
    size: Entity<InputState>,
}

fn size_value(size: VolumeSize) -> f64 {
    match size {
        VolumeSize::Fixed { volume } => f64::from(volume),
        VolumeSize::Average { multiple } => multiple,
    }
}

/// The inputs of the numbers, with the subscriptions that apply what is typed to `chart`.
pub(super) fn inputs<T: 'static>(
    chart: &Entity<Chart>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> (Inputs, Vec<Subscription>) {
    let (candles, bars) = {
        let settings = &chart.read(cx).settings;
        (settings.volume_candles, settings.volume_bars)
    };
    let mut numbers = Numbers::new(chart, window, cx);
    let min_width = numbers.add(
        f64::from(candles.min_width) * 100.0,
        (5.0, 100.0),
        0,
        |s, v| s.volume_candles.min_width = (v / 100.0) as f32,
    );
    let max_width = numbers.add(
        f64::from(candles.max_width) * 100.0,
        (10.0, 100.0),
        0,
        |s, v| s.volume_candles.max_width = (v / 100.0) as f32,
    );
    let wick = numbers.add(f64::from(candles.wick_width), (1.0, 4.0), 0, |s, v| {
        s.volume_candles.wick_width = v as f32;
    });
    let size = numbers.add(size_value(bars.size), (0.01, 1e9), 2, |s, v| {
        s.volume_bars.size = match s.volume_bars.size {
            VolumeSize::Fixed { .. } => VolumeSize::Fixed {
                volume: v.round().clamp(1.0, 1e9) as u32,
            },
            VolumeSize::Average { .. } => VolumeSize::Average { multiple: v },
        };
    });
    (
        Inputs {
            min_width,
            max_width,
            wick,
            size,
        },
        numbers.subscriptions,
    )
}

/// The groups of the settings of the volume candles.
pub(super) fn candle_groups(
    chart: &Entity<Chart>,
    inputs: &Inputs,
    s: &VolumeCandleSettings,
) -> Vec<AnyElement> {
    let width = vec![
        choice(
            chart,
            "vc-scale",
            "Width follows the volume",
            Some("How quickly a candle gets wider with its volume"),
            named(&WidthScale::ALL, WidthScale::label),
            s.scale,
            |st, v| st.volume_candles.scale = v,
        ),
        choice(
            chart,
            "vc-reference",
            "Widest candle",
            Some("The volume that gets the full width"),
            named(&WidthReference::ALL, WidthReference::label),
            s.reference,
            |st, v| st.volume_candles.reference = v,
        ),
        number(
            "Narrowest (%)",
            Some("Width of a candle with no volume, out of the room it has"),
            &inputs.min_width,
        ),
        number(
            "Widest (%)",
            Some("Width of the busiest candle. 100 leaves no gap"),
            &inputs.max_width,
        ),
    ];
    let look = vec![
        choice(
            chart,
            "vc-color",
            "Colored by",
            Some("Rising or falling, from the candle itself or from the close before"),
            named(&ColorBy::ALL, ColorBy::label),
            s.color_by,
            |st, v| st.volume_candles.color_by = v,
        ),
        choice(
            chart,
            "vc-fill",
            "Bodies",
            None,
            named(&Fill::ALL, Fill::label),
            s.fill,
            |st, v| st.volume_candles.fill = v,
        ),
        switch(
            chart,
            "vc-outline",
            "Edge between candles",
            Some("A thin line in the background color, for candles that touch"),
            s.outline,
            |st, on| st.volume_candles.outline = on,
        ),
        number("Wick width (px)", None, &inputs.wick),
        switch(
            chart,
            "vc-labels",
            "Volume over each candle",
            Some("Written when a candle is wide enough to hold it"),
            s.labels,
            |st, on| st.volume_candles.labels = on,
        ),
    ];
    vec![
        ui::group(IconName::ChartCandlestick, "Width of the candles", width).into_any_element(),
        ui::group(IconName::Palette, "Look of the candles", look).into_any_element(),
    ]
}

/// The group of the settings of the volume bars. `resolved` is the volume a bar holds now (0
/// when the chart has no bars yet).
pub(super) fn bar_group(
    chart: &Entity<Chart>,
    inputs: &Inputs,
    s: &VolumeBarSettings,
    resolved: i64,
) -> AnyElement {
    let mode = match s.size {
        VolumeSize::Fixed { .. } => 0,
        VolumeSize::Average { .. } => 1,
    };
    let (mode_chart, state) = (chart.clone(), inputs.size.clone());
    let current = s.size;
    let size = ui::field(
        "Volume of a bar",
        Some("A bar closes once it has seen this many ticks"),
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(widgets::segmented(
                "vb-size-mode",
                &["Fixed", "x average"],
                mode,
                move |chosen, window, cx| {
                    let next = if chosen == 0 {
                        VolumeSize::Fixed {
                            volume: u32::try_from(resolved.max(1)).unwrap_or(u32::MAX),
                        }
                    } else if matches!(current, VolumeSize::Average { .. }) {
                        current
                    } else {
                        VolumeSize::Average { multiple: 1.0 }
                    };
                    edit(&mode_chart, cx, |st: &mut ChartSettings| {
                        st.volume_bars.size = next;
                    });
                    state.update(cx, |state, cx| {
                        state.set_value(widgets::format_number(size_value(next), 2), window, cx);
                    });
                },
            ))
            .child(widgets::number_field(&inputs.size, 110.)),
    );
    let mut rows = vec![size];
    if resolved > 0 {
        rows.push(ui::field(
            "Volume of a bar now",
            Some("What the chart uses with the setting above"),
            div().child(format!("{resolved} ticks")),
        ));
    }
    rows.push(choice(
        chart,
        "vb-path",
        "Prices read",
        Some("Close only is quick; open, high, low and close follows the price inside each bar"),
        named(&PricePath::ALL, PricePath::label),
        s.path,
        |st, v| st.volume_bars.path = v,
    ));
    ui::group(IconName::ChartBarBig, "Volume bar construction", rows).into_any_element()
}
