//! The Charts page of the settings.

use super::*;

impl SettingsHub {
    pub(super) fn charts_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let a = appearance::get(cx);
        let now = theme::colors();

        let mut sets = div().flex().flex_row().flex_wrap().gap_2();
        for (index, (name, up, down)) in CANDLE_SETS.iter().copied().enumerate() {
            let chosen = a.candle_up == Some(up) && a.candle_down == Some(down);
            sets = sets.child(
                div()
                    .id(("candle-set", index))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(tokens::height::LARGE))
                    .px_2p5()
                    .rounded_md()
                    .border_1()
                    .border_color(if chosen {
                        theme::accent()
                    } else {
                        theme::border_subtle()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(cx.listener(move |_this, _event, _window, cx| {
                        appearance::update(cx, |a| {
                            a.candle_up = Some(up);
                            a.candle_down = Some(down);
                        });
                    }))
                    .child(div().size(px(12.)).rounded_sm().bg(rgb(up)))
                    .child(div().size(px(12.)).rounded_sm().bg(rgb(down)))
                    .child(
                        div()
                            .text_size(px(tokens::text::BODY))
                            .text_color(theme::fg())
                            .child(name),
                    ),
            );
        }

        let reset = |id: &'static str, set: bool, clear: fn(&mut appearance::Appearance)| {
            Button::new(id)
                .cursor_pointer()
                .ghost()
                .xsmall()
                .icon(IconName::RotateCcw)
                .label("Theme")
                .tooltip("Go back to the color of the theme")
                .disabled(!set)
                .on_click(move |_, _window, cx| appearance::update(cx, clear))
        };
        let with_reset = |swatch: AnyElement, button: Button| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(swatch)
                .child(button)
        };

        let candles = form::group(
            IconName::ChartCandlestick,
            "Candles",
            [
                form::block(self.candle_preview()),
                form::block(sets),
                form::field(
                    "Rising candle",
                    None,
                    with_reset(
                        self.swatch(Pick::CandleUp, now.up, "settings-up", cx, |a, c| {
                            a.candle_up = Some(c);
                        }),
                        reset("settings-up-reset", a.candle_up.is_some(), |a| {
                            a.candle_up = None
                        }),
                    ),
                ),
                form::field(
                    "Falling candle",
                    None,
                    with_reset(
                        self.swatch(Pick::CandleDown, now.down, "settings-down", cx, |a, c| {
                            a.candle_down = Some(c);
                        }),
                        reset("settings-down-reset", a.candle_down.is_some(), |a| {
                            a.candle_down = None;
                        }),
                    ),
                ),
                form::field(
                    "Line of the line charts",
                    Some("Line, area, step and baseline charts"),
                    with_reset(
                        self.swatch(Pick::ChartLine, now.line, "settings-line", cx, |a, c| {
                            a.chart_line = Some(c);
                        }),
                        reset("settings-line-reset", a.chart_line.is_some(), |a| {
                            a.chart_line = None;
                        }),
                    ),
                ),
            ],
        );

        let background = form::group(
            IconName::PaintBucket,
            "Background",
            [form::field(
                "Chart background",
                Some("Apart from the rest of the app"),
                with_reset(
                    self.swatch(
                        Pick::ChartBackground,
                        now.chart_bg,
                        "settings-chart-bg",
                        cx,
                        |a, c| a.chart_background = Some(c),
                    ),
                    reset(
                        "settings-chart-bg-reset",
                        a.chart_background.is_some(),
                        |a| {
                            a.chart_background = None;
                        },
                    ),
                ),
            )],
        );

        form::page()
            .child(candles)
            .child(background)
            .child(form::note(
                "The type of a chart (candles, hollow candles, bars, Heikin Ashi, line...) is set on each chart, from its toolbar.",
            ))
            .into_any_element()
    }
}
