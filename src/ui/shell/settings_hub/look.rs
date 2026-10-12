//! The Appearance page of the settings: the theme, the accent, the user's own themes, the fonts and the candle colors.

mod fonts;
mod themes;

use super::{
    ACCENTS, AnyElement, ColorField, Colors, Context, FontChosen, FontPicker, IconName, Mode, Pick,
    SettingsHub, SharedString, Window, appearance, contrast, controls, div, form, icon, px, rgb,
    theme, toast, tokens,
};
use crate::ui::kit::prelude::Disableable;
use gpui::prelude::*;

/// The dots of the title bar of a theme preview.
const PREVIEW_DOT: f32 = 8.0;
/// The height of a theme preview.
const PREVIEW_HEIGHT: f32 = 58.0;
/// The height of the title line of a theme preview.
const PREVIEW_TITLE_HEIGHT: f32 = 5.0;
/// The width of the title line of a theme preview.
const PREVIEW_TITLE_WIDTH: f32 = 60.0;
/// The height of a text line of a theme preview.
const PREVIEW_LINE_HEIGHT: f32 = 4.0;
/// The width of a text line of a theme preview.
const PREVIEW_LINE_WIDTH: f32 = 40.0;
impl SettingsHub {
    /// A card that shows a theme: its colors in miniature, its name, and a check when it is the
    /// one in force.
    pub(super) fn theme_card(
        &self,
        id: &str,
        name: &str,
        colors: Colors,
        active: bool,
        custom: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dot = |color: u32| div().size(px(PREVIEW_DOT)).rounded_full().bg(rgb(color));
        let preview = div()
            .h(px(PREVIEW_HEIGHT))
            .p_2()
            .rounded_md()
            .bg(rgb(colors.bg))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .h(px(PREVIEW_TITLE_HEIGHT))
                    .w(px(PREVIEW_TITLE_WIDTH))
                    .rounded_full()
                    .bg(rgb(colors.fg)),
            )
            .child(
                div()
                    .h(px(PREVIEW_LINE_HEIGHT))
                    .w(px(PREVIEW_LINE_WIDTH))
                    .rounded_full()
                    .bg(rgb(colors.muted)),
            )
            .child(
                div()
                    .flex_1()
                    .rounded_sm()
                    .bg(rgb(colors.surface))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(dot(colors.accent))
                    .child(dot(colors.up))
                    .child(dot(colors.down))
                    .child(dot(colors.line)),
            );
        let id_owned = id.to_owned();
        let light = colors.is_light();
        div()
            .id(SharedString::from(format!("theme-{id}")))
            .w(px(crate::ui::kit::tokens::field::select()))
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(if active {
                theme::accent()
            } else {
                theme::border_subtle()
            })
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover()))
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                let id = id_owned.clone();
                appearance::update(cx, |a| {
                    // The theme goes in the slot of its kind, and the mode follows unless it
                    // follows the system.
                    if light {
                        a.light_theme = id;
                    } else {
                        a.dark_theme = id;
                    }
                    if a.mode != Mode::System {
                        a.mode = if light { Mode::Light } else { Mode::Dark };
                    }
                });
            }))
            .child(preview)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(name.to_owned()),
                    )
                    .children(custom.then(|| icon::small(IconName::Pencil, theme::muted_fg())))
                    .children(active.then(|| icon::small(IconName::Check, theme::accent()))),
            )
            .into_any_element()
    }

    pub(super) fn appearance_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let a = appearance::get(cx);
        let active = appearance::active_id(cx);
        let this = cx.entity();

        // The mode, and every theme.
        let mode_this = this.clone();
        let mode_labels: Vec<&str> = Mode::ALL.iter().map(|m| m.label()).collect();
        let mut grid = div().flex().flex_row().flex_wrap().gap_2();
        for (id, name) in a.themes() {
            let Some((_, colors)) = a.find(&id) else {
                continue;
            };
            let custom = a.custom_themes.iter().any(|t| t.id == id);
            grid = grid.child(self.theme_card(&id, &name, colors, id == active, custom, cx));
        }
        let theme_group = form::group(
            IconName::Palette,
            "Theme",
            [
                form::field(
                    "Mode",
                    Some("System follows the light or dark setting of your computer"),
                    controls::segmented(
                        "settings-mode",
                        &mode_labels,
                        Mode::ALL.iter().position(|m| *m == a.mode).unwrap_or(0),
                        move |choice, _window, cx| {
                            let mode = Mode::ALL[choice];
                            mode_this.update(cx, |_, _| {});
                            appearance::update(cx, |a| a.mode = mode);
                        },
                    ),
                ),
                form::block(grid),
                form::block(form::note(
                    "A theme goes in the dark or the light slot by what it is. In System mode both slots are used, one at night and one by day.",
                )),
            ],
        );

        // The accent.
        let mut accents = div().flex().flex_row().flex_wrap().items_center().gap_2();
        let theme_accent = a
            .find(a.active_id(true))
            .map_or(Colors::WYCK_DARK.accent, |(_, c)| c.accent);
        let _ = theme_accent;
        accents = accents.child(self.accent_dot(None, a.accent.is_none(), cx));
        for color in ACCENTS {
            accents = accents.child(self.accent_dot(Some(color), a.accent == Some(color), cx));
        }
        let accent_now = a.accent.unwrap_or_else(|| theme::colors().accent);
        let accent_group = form::group(
            IconName::Sparkles,
            "Accent",
            [
                form::block(accents),
                form::field(
                    "Any other color",
                    Some("The theme's own accent when none is picked"),
                    self.swatch(Pick::Accent, accent_now, "settings-accent", cx, |a, c| {
                        a.accent = Some(c);
                    }),
                ),
            ],
        );

        // The themes of the user.
        let themes_group = self.custom_themes_group(&a, cx);

        // The font and the motion.
        let font_group = self.font_group(&a, cx);
        let motion_this = this.clone();
        let motion = form::group(
            IconName::Zap,
            "Motion",
            [form::field(
                "Animations",
                Some("Screens and panels fade and slide in. Off makes them appear at once"),
                controls::toggle(
                    "settings-animations",
                    a.animations,
                    move |on, _window, cx| {
                        motion_this.update(cx, |_, _| {});
                        appearance::update(cx, |a| a.animations = on);
                    },
                ),
            )],
        );

        // The size of the text and the controls.
        const SCALES: [u32; 6] = [90, 100, 115, 130, 150, 160];
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
                        appearance::update(cx, |a| a.ui_scale = SCALES[choice]);
                    },
                ),
            )],
        );

        let reset = crate::ui::kit::button::quiet("settings-reset-look")
            .icon(IconName::RotateCcw)
            .label("Reset the appearance")
            .disabled(a.is_default())
            .on_click(|_, _window, cx| {
                let customs = appearance::get(cx).custom_themes;
                // The themes the user made are kept: only the choices go back.
                appearance::update(cx, |a| {
                    *a = appearance::Appearance {
                        custom_themes: customs,
                        ..appearance::Appearance::default()
                    };
                });
            });

        form::page()
            .child(theme_group)
            .child(accent_group)
            .child(themes_group)
            .child(font_group)
            .child(size)
            .child(motion)
            .child(div().flex().flex_row().child(reset))
            .into_any_element()
    }

    /// A round choice of accent: `None` is the theme's own.
    pub(super) fn accent_dot(
        &self,
        color: Option<u32>,
        chosen: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = color.unwrap_or(0x000000);
        let base = div()
            .id(SharedString::from(format!("accent-{}", color.unwrap_or(0))))
            .flex()
            .items_center()
            .justify_center()
            .size(px(tokens::height::compact()))
            .rounded_full()
            .border_2()
            .border_color(if chosen {
                theme::fg()
            } else {
                theme::border_hairline()
            })
            .cursor_pointer()
            .tooltip(controls::tooltip(match color {
                Some(_) => "Use this accent",
                None => "The theme's own accent",
            }))
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                appearance::update(cx, |a| a.accent = color);
            }));
        match color {
            Some(_) => base
                .child(
                    div()
                        .size(px(crate::ui::kit::tokens::swatch::large()))
                        .rounded_full()
                        .bg(rgb(shown)),
                )
                .into_any_element(),
            None => base
                .child(icon::small(IconName::RotateCcw, theme::muted_fg()))
                .into_any_element(),
        }
    }
}
