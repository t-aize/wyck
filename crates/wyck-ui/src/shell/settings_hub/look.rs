//! The Interface page of the settings: the font, the size of the interface and the motion.

mod fonts;

use super::{
    AnyElement, Context, FontChosen, FontPicker, IconName, SettingsHub, SharedString, Window,
    appearance, controls, div, form, px, rgb, theme,
};
use crate::kit::prelude::Disableable;
use gpui::prelude::*;

/// The sizes of the interface on offer, in percent.
const SCALES: [u32; 6] = [90, 100, 115, 130, 150, 160];

impl SettingsHub {
    pub(super) fn interface_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let a = appearance::get(cx);

        // The font and the motion.
        let font_group = self.font_group(&a, cx);
        let motion = form::group(
            IconName::Zap,
            "Motion",
            [form::field(
                "Animations",
                Some("Screens and panels fade and slide in. Off makes them appear at once"),
                controls::toggle("settings-animations", a.animations, |on, _window, cx| {
                    appearance::update(cx, |a| a.animations = on);
                }),
            )],
        );

        // The size of the text and the controls.
        let labels: Vec<String> = SCALES.iter().map(|s| format!("{s}%")).collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let scale_index = SCALES.iter().position(|s| *s == a.ui_scale).unwrap_or(1);
        let size = form::group(
            IconName::Type,
            "Size of the interface",
            [form::field(
                "Text and controls",
                Some("Makes everything larger or smaller together. Chart text keeps its own size"),
                controls::segmented(
                    "settings-ui-scale",
                    &label_refs,
                    scale_index,
                    |choice, _window, cx| {
                        if let Some(&scale) = SCALES.get(choice) {
                            appearance::update(cx, |a| a.ui_scale = scale);
                        }
                    },
                ),
            )],
        );

        // The colors of the charts have their own page and their own reset.
        let reset = crate::kit::button::quiet("settings-reset-look")
            .icon(IconName::RotateCcw)
            .label("Reset the interface")
            .disabled(a.font == appearance::DEFAULT_FONT && a.animations && a.ui_scale == 100)
            .on_click(|_, _window, cx| {
                appearance::update(cx, |a| {
                    let defaults = appearance::Appearance::default();
                    a.font = defaults.font;
                    a.animations = defaults.animations;
                    a.ui_scale = defaults.ui_scale;
                });
            });

        form::page()
            .child(font_group)
            .child(size)
            .child(motion)
            .child(div().flex().flex_row().child(reset))
            .into_any_element()
    }
}
