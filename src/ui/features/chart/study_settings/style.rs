//! The style and display pages of an indicator: plots, levels, fills and where it is drawn.

use super::{PANE_HEIGHTS, StudyEditor};
use crate::domain::drawings::model::style::{DASHES, WIDTHS};
use crate::ui::features::chart::study::config::StudyConfig;
use crate::ui::features::chart::study::kind::StudyKind;
use crate::ui::features::chart::study::spec::{Placement, PlotKind};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::{controls, form, number, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div};

impl StudyEditor {
    pub(super) fn style_page(&self, config: &StudyConfig, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let mut page = form::page();
        let style_rows = self.input_rows(config, true, cx);
        if !style_rows.is_empty() {
            page = page.child(form::group(
                IconName::Palette,
                "Indicator style",
                style_rows,
            ));
        } else if config.is_script() && config.spec().plots.is_empty() {
            page = page.child(form::note("This script has no style settings. Declare a color, opacity, width or text size input, or use section: \"style\" on an input."));
        }
        for plot in config.spec().plots {
            let key = plot.key;
            let style = config.plot_style(key);
            let open = self.color_open == Some(key);
            let toggle_this = this.clone();
            let (color_target, shown_target) = (self.target.clone(), self.target.clone());
            let icon = match plot.kind {
                PlotKind::Line => IconName::Spline,
                PlotKind::Histogram => IconName::ChartColumn,
                PlotKind::Dots => IconName::CircleDot,
            };
            let shown = controls::toggle(
                SharedString::from(format!("plot-visible-{key}")),
                style.visible,
                move |on, _window, cx| shown_target.plot(key, cx, |s| s.visible = on),
            )
            .into_any_element();

            let swatch = controls::color_swatch(
                SharedString::from(format!("plot-color-{key}")),
                style.color,
                open,
                cx,
                move |_window, cx| {
                    toggle_this.update(cx, |this, cx| {
                        this.color_open = if this.color_open == Some(key) {
                            None
                        } else {
                            Some(key)
                        };
                        cx.notify();
                    });
                },
                move |color, _window, cx| color_target.plot(key, cx, |s| s.color = color),
            );
            let opacity = self
                .opacity
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, state)| number::field(state, tokens::field::number()));
            let mut rows = vec![form::field(
                "Color",
                Some("Opacity in percent"),
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(swatch)
                    .children(opacity),
            )];

            let kind_target = self.target.clone();
            let displayed_kind = style.kind.unwrap_or(plot.kind);
            if config.kind != StudyKind::VolumeProfile {
                rows.push(form::field(
                    "Plot type",
                    None,
                    controls::segmented(
                        SharedString::from(format!("plot-type-{key}")),
                        &["Line", "Dots", "Columns"],
                        match displayed_kind {
                            PlotKind::Line => 0,
                            PlotKind::Dots => 1,
                            PlotKind::Histogram => 2,
                        },
                        move |choice, _window, cx| {
                            let kind = match choice {
                                1 => PlotKind::Dots,
                                2 => PlotKind::Histogram,
                                _ => PlotKind::Line,
                            };
                            kind_target.plot(key, cx, |s| s.kind = Some(kind));
                        },
                    ),
                ));
            }

            let width_index = WIDTHS.iter().position(|w| (w - style.width).abs() < 0.01);
            let width_target = self.target.clone();
            let picker = controls::width_picker(
                // One id per plot: the pickers of several plots must not share state.
                SharedString::from(format!("plot-width-{key}")),
                &WIDTHS,
                width_index,
                move |choice, _window, cx| {
                    width_target.plot(key, cx, |s| s.width = WIDTHS[choice]);
                },
            );
            // The presets, and a field for any other width.
            let width = div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(picker)
                .children(
                    self.widths
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, state)| number::field(state, tokens::field::narrow())),
                );
            if config.kind == StudyKind::VolumeProfile {
                if key == "poc" {
                    let dash_target = self.target.clone();
                    let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
                    rows.push(form::field("Width", None, width));
                    rows.push(form::field(
                        "Line style",
                        None,
                        controls::dash_picker(
                            SharedString::from("profile-poc-dash"),
                            dash_index,
                            move |choice, _window, cx| {
                                dash_target.plot(key, cx, |s| s.dash = DASHES[choice])
                            },
                        ),
                    ));
                }
            } else {
                match displayed_kind {
                    PlotKind::Line => {
                        let dash_target = self.target.clone();
                        let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
                        rows.push(form::field("Width", None, width));
                        rows.push(form::field(
                            "Line style",
                            None,
                            controls::dash_picker(
                                SharedString::from(format!("plot-dash-{key}")),
                                dash_index,
                                move |choice, _window, cx| {
                                    dash_target.plot(key, cx, |s| s.dash = DASHES[choice]);
                                },
                            ),
                        ));
                    }
                    PlotKind::Dots => rows.push(form::field("Size", None, width)),
                    PlotKind::Histogram => {
                        let up_target = self.target.clone();
                        let down_target = self.target.clone();
                        let up_open = self.up_color_open == Some(key);
                        let down_open = self.down_color_open == Some(key);
                        let this_up = this.clone();
                        let this_down = this.clone();
                        rows.push(form::field(
                            "Up color",
                            None,
                            controls::color_swatch(
                                SharedString::from(format!("plot-up-{key}")),
                                style.up_color.unwrap_or(
                                    crate::ui::features::chart::study::compute::UP_COLOR,
                                ),
                                up_open,
                                cx,
                                move |_window, cx| {
                                    this_up.update(cx, |e, cx| {
                                        e.up_color_open = if e.up_color_open == Some(key) {
                                            None
                                        } else {
                                            Some(key)
                                        };
                                        cx.notify();
                                    })
                                },
                                move |color, _window, cx| {
                                    up_target.plot(key, cx, |s| s.up_color = Some(color))
                                },
                            ),
                        ));
                        rows.push(form::field(
                            "Down color",
                            None,
                            controls::color_swatch(
                                SharedString::from(format!("plot-down-{key}")),
                                style.down_color.unwrap_or(
                                    crate::ui::features::chart::study::compute::DOWN_COLOR,
                                ),
                                down_open,
                                cx,
                                move |_window, cx| {
                                    this_down.update(cx, |e, cx| {
                                        e.down_color_open = if e.down_color_open == Some(key) {
                                            None
                                        } else {
                                            Some(key)
                                        };
                                        cx.notify();
                                    })
                                },
                                move |color, _window, cx| {
                                    down_target.plot(key, cx, |s| s.down_color = Some(color))
                                },
                            ),
                        ));
                        if let Some((_, state)) = self.bar_widths.iter().find(|(k, _)| *k == key) {
                            rows.push(form::field(
                                "Column width (%)",
                                None,
                                number::field(state, tokens::field::number()),
                            ));
                        }
                    }
                }
            }
            page = page.child(form::group_with(icon, plot.label, Some(shown), rows));
        }
        for (index, value) in config.default_levels().into_iter().enumerate() {
            let style = config.levels.get(&index).copied().unwrap_or_default();
            let target = self.target.clone();
            let shown = controls::toggle(
                SharedString::from(format!("level-visible-{index}")),
                style.visible,
                move |on, _window, cx| target.level(index, cx, |s| s.visible = on),
            );
            let pick = self.target.clone();
            let open = self.level_color_open == Some(index);
            let owner = this.clone();
            let color = controls::color_swatch(
                SharedString::from(format!("level-color-{index}")),
                style.color,
                open,
                cx,
                move |_window, cx| {
                    owner.update(cx, |e, cx| {
                        e.level_color_open = if e.level_color_open == Some(index) {
                            None
                        } else {
                            Some(index)
                        };
                        cx.notify();
                    })
                },
                move |color, _window, cx| pick.level(index, cx, |s| s.color = color),
            );
            let dash_target = self.target.clone();
            let dash_index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
            let mut rows = vec![form::field("Color", None, color).into_any_element()];
            if let Some((_, state)) = self.level_values.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field(
                        "Value",
                        Some("Overrides the calculated level"),
                        number::field(state, tokens::field::wide()),
                    )
                    .into_any_element(),
                );
            }
            if let Some((_, state)) = self.level_widths.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field("Width", None, number::field(state, tokens::field::number()))
                        .into_any_element(),
                );
            }
            if let Some((_, state)) = self.level_opacity.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field(
                        "Opacity (%)",
                        None,
                        number::field(state, tokens::field::number()),
                    )
                    .into_any_element(),
                );
            }
            rows.push(
                form::field(
                    "Line style",
                    None,
                    controls::dash_picker(
                        SharedString::from(format!("level-dash-{index}")),
                        dash_index,
                        move |choice, _window, cx| {
                            dash_target.level(index, cx, |s| s.dash = DASHES[choice])
                        },
                    ),
                )
                .into_any_element(),
            );
            page = page.child(form::group_with(
                IconName::Minus,
                format!("Level {} ({})", index + 1, number::format(value, 2)),
                Some(shown.into_any_element()),
                rows,
            ));
        }
        for index in 0..2 {
            let Some(default) = config.default_fill(index) else {
                continue;
            };
            let style = config.fills.get(&index).copied().unwrap_or(default);
            let shown_target = self.target.clone();
            let shown = controls::toggle(
                SharedString::from(format!("fill-visible-{index}")),
                style.visible,
                move |on, _window, cx| shown_target.fill(index, default, cx, |s| s.visible = on),
            );
            let color_target = self.target.clone();
            let owner = this.clone();
            let color = controls::color_swatch(
                SharedString::from(format!("fill-color-{index}")),
                style.color,
                self.fill_color_open == Some(index),
                cx,
                move |_window, cx| {
                    owner.update(cx, |e, cx| {
                        e.fill_color_open = if e.fill_color_open == Some(index) {
                            None
                        } else {
                            Some(index)
                        };
                        cx.notify();
                    })
                },
                move |color, _window, cx| {
                    color_target.fill(index, default, cx, |s| s.color = color)
                },
            );
            let mut rows = vec![form::field("Color", None, color).into_any_element()];
            if let Some(other) = style.other {
                let other_target = self.target.clone();
                let owner = this.clone();
                let other = controls::color_swatch(
                    SharedString::from(format!("fill-other-{index}")),
                    other,
                    self.fill_other_open == Some(index),
                    cx,
                    move |_window, cx| {
                        owner.update(cx, |e, cx| {
                            e.fill_other_open = if e.fill_other_open == Some(index) {
                                None
                            } else {
                                Some(index)
                            };
                            cx.notify();
                        })
                    },
                    move |color, _window, cx| {
                        other_target.fill(index, default, cx, |s| s.other = Some(color))
                    },
                );
                rows.push(form::field("Other color", None, other).into_any_element());
            }
            if let Some((_, state)) = self.fill_opacity.iter().find(|(i, _)| *i == index) {
                rows.push(
                    form::field(
                        "Opacity (%)",
                        None,
                        number::field(state, tokens::field::number()),
                    )
                    .into_any_element(),
                );
            }
            page = page.child(form::group_with(
                IconName::Palette,
                format!("Fill {}", index + 1),
                Some(shown.into_any_element()),
                rows,
            ));
        }
        if let Some(default) = config.default_band() {
            let style = config.band.unwrap_or(default);
            let shown_target = self.target.clone();
            let shown = controls::toggle("band-visible", style.visible, move |on, _window, cx| {
                shown_target.band(default, cx, |s| s.visible = on)
            });
            let color_target = self.target.clone();
            let owner = this.clone();
            let color = controls::color_swatch(
                "band-color",
                style.color,
                self.band_color_open,
                cx,
                move |_window, cx| {
                    owner.update(cx, |e, cx| {
                        e.band_color_open = !e.band_color_open;
                        cx.notify();
                    })
                },
                move |color, _window, cx| color_target.band(default, cx, |s| s.color = color),
            );
            let mut rows = vec![form::field("Color", None, color).into_any_element()];
            if let Some(state) = &self.band_opacity {
                rows.push(
                    form::field(
                        "Opacity (%)",
                        None,
                        number::field(state, tokens::field::number()),
                    )
                    .into_any_element(),
                );
            }
            page = page.child(form::group_with(
                IconName::Palette,
                "Threshold zone",
                Some(shown.into_any_element()),
                rows,
            ));
        }
        page.into_any_element()
    }

    pub(super) fn display_page(&self, config: &StudyConfig) -> AnyElement {
        let mut page = form::page();
        let target = self.target.clone();
        let mut rows = vec![form::field(
            "Show on the chart",
            Some("Hides the indicator without removing it"),
            controls::toggle("study-visible", config.visible, move |on, _window, cx| {
                target.edit(cx, |s| s.visible = on);
            }),
        )];
        rows.push(form::field(
            "Name",
            Some("Leave empty to use the indicator name"),
            crate::ui::kit::field::text(&self.name),
        ));
        let axis = self.target.clone();
        rows.push(form::field(
            "Axis value labels",
            None,
            controls::toggle(
                "study-axis-labels",
                config.axis_labels,
                move |on, _window, cx| {
                    axis.edit(cx, |s| s.axis_labels = on);
                },
            ),
        ));
        let legend = self.target.clone();
        rows.push(form::field(
            "Values in legend",
            None,
            controls::toggle(
                "study-legend-values",
                config.legend_values,
                move |on, _window, cx| {
                    legend.edit(cx, |s| s.legend_values = on);
                },
            ),
        ));
        let auto = self.target.clone();
        rows.push(form::field(
            "Automatic precision",
            None,
            controls::toggle(
                "study-auto-precision",
                config.precision.is_none(),
                move |on, _window, cx| {
                    auto.edit(cx, |s| s.precision = if on { None } else { Some(2) });
                },
            ),
        ));
        if config.precision.is_some() {
            rows.push(form::field(
                "Decimal places",
                None,
                number::field(&self.precision, tokens::field::number()),
            ));
        }
        if config.spec().placement == Placement::Pane {
            let target = self.target.clone();
            rows.push(form::field(
                "Pane height",
                Some("Next to the price chart. Drag the divider for any other height"),
                controls::height_picker(
                    "study-height",
                    PANE_HEIGHTS,
                    config.pane_weight(),
                    move |weight, _window, cx| target.edit(cx, |s| s.weight = weight),
                ),
            ));
        }
        page = page.child(form::group(IconName::Eye, "Visibility", rows));
        page.into_any_element()
    }
}
