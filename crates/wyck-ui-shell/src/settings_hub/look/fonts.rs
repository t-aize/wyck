//! The fonts on the Interface page, and the preview of the candle colors of the Charts page.

use super::{
    AnyElement, Context, FontChosen, FontPicker, IconName, SettingsHub, SharedString, Window,
    appearance, div, form, px, rgb, theme,
};
use gpui::prelude::*;

/// The height of the preview of candle colors.
const CANDLE_PREVIEW_HEIGHT: f32 = 96.0;
/// The space between two candles of the preview.
const CANDLE_GAP: f32 = 9.0;
/// The width of a candle body of the preview.
const CANDLE_BODY_WIDTH: f32 = 8.0;
/// The height of the price line of the preview.
const CANDLE_LINE_HEIGHT: f32 = 2.0;

impl SettingsHub {
    /// Opens or closes the list of fonts, made the first time it opens.
    fn toggle_fonts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.font_open = !self.font_open;
        if self.font_open && self.font_picker.is_none() {
            let current = appearance::font(cx).to_string();
            let picker =
                cx.new(|cx| FontPicker::new(Some(current), appearance::DEFAULT_FONT, window, cx));
            self._font_subscription = Some(cx.subscribe(
                &picker,
                |_this, _picker, event: &FontChosen, cx| {
                    let name = event
                        .0
                        .clone()
                        .unwrap_or_else(|| appearance::DEFAULT_FONT.to_owned());
                    appearance::update(cx, |a| a.font = name);
                },
            ));
            self.font_picker = Some(picker);
        }
        cx.notify();
    }

    pub(in crate::settings_hub) fn font_group(
        &self,
        a: &appearance::Appearance,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let toggle = cx.entity();
        let mut rows = vec![form::field(
            "Interface font",
            Some("Any font installed on this computer. Inter comes with the app"),
            wyck_ui_kit::button::quiet("settings-font-open")
                .icon(IconName::Type)
                .label(SharedString::from(a.font.clone()))
                .toggled(self.font_open)
                .on_click(move |_, window, cx| {
                    toggle.update(cx, |e, cx| e.toggle_fonts(window, cx));
                }),
        )];
        if let Some(picker) = self.font_picker.as_ref().filter(|_| self.font_open) {
            picker.update(cx, |picker, cx| {
                picker.set_current(Some(a.font.clone()), cx)
            });
            rows.push(form::block(picker.clone()));
        }
        form::group(IconName::Type, "Typography", rows)
    }

    /// A small drawing of candles in the colors in force, to see a change before leaving.
    pub(in crate::settings_hub) fn candle_preview(&self) -> AnyElement {
        let c = theme::colors();
        // (rising, body height, wick above, wick below)
        let shape: [(bool, f32, f32, f32); 16] = [
            (true, 14., 6., 5.),
            (true, 20., 4., 4.),
            (false, 12., 5., 7.),
            (true, 22., 8., 3.),
            (true, 10., 5., 5.),
            (false, 18., 6., 4.),
            (false, 24., 4., 8.),
            (true, 12., 7., 3.),
            (true, 26., 5., 5.),
            (true, 16., 8., 4.),
            (false, 10., 4., 6.),
            (false, 20., 6., 5.),
            (true, 14., 5., 4.),
            (true, 24., 7., 3.),
            (false, 16., 5., 6.),
            (true, 22., 6., 4.),
        ];
        let mut row = div()
            .h(px(CANDLE_PREVIEW_HEIGHT))
            .px_3()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_subtle())
            .bg(rgb(c.chart_bg))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(CANDLE_GAP));
        for (up, body, above, below) in shape {
            let color = rgb(if up { c.up } else { c.down });
            row = row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w(px(wyck_ui_kit::tokens::HAIRLINE))
                            .h(px(above))
                            .bg(color),
                    )
                    .child(div().w(px(CANDLE_BODY_WIDTH)).h(px(body)).bg(color))
                    .child(
                        div()
                            .w(px(wyck_ui_kit::tokens::HAIRLINE))
                            .h(px(below))
                            .bg(color),
                    ),
            );
        }
        row.child(
            div()
                .flex_1()
                .h(px(CANDLE_LINE_HEIGHT))
                .rounded_full()
                .bg(rgb(c.line)),
        )
        .into_any_element()
    }
}
