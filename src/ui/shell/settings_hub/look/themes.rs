//! The user's own themes on the Appearance page: the list, the editor and the copy of a theme.

use super::{
    AnyElement, ColorField, Context, IconName, Pick, SettingsHub, SharedString, Window, appearance,
    contrast, div, form, px, theme, toast, tokens,
};
use crate::ui::kit::field;
use crate::ui::kit::prelude::Disableable;
use gpui::prelude::*;

/// The least height of a row with a color swatch.
const SWATCH_ROW_HEIGHT: f32 = 42.0;

impl SettingsHub {
    pub(in crate::ui::shell::settings_hub) fn custom_themes_group(
        &self,
        a: &appearance::Appearance,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let this = cx.entity();
        let mut rows: Vec<AnyElement> = Vec::new();
        if a.custom_themes.is_empty() {
            rows.push(form::block(form::note(
                "Copy a theme to make it yours: rename it and change any of its colors.",
            )));
        }
        for theme_of_user in &a.custom_themes {
            let id = theme_of_user.id.clone();
            let editing = self.editing.as_deref() == Some(id.as_str());
            let (edit_this, delete_this) = (this.clone(), this.clone());
            let (edit_id, delete_id) = (id.clone(), id.clone());
            let name = theme_of_user.name.clone();
            let used = a.active_id(true) == id || a.active_id(false) == id;
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .min_h(px(SWATCH_ROW_HEIGHT))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::fg())
                            .child(name.clone()),
                    )
                    .children(used.then(|| {
                        div()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::accent())
                            .child("In use")
                    }))
                    .child(
                        crate::ui::kit::button::quiet(SharedString::from(format!(
                            "theme-edit-{id}"
                        )))
                        .icon(IconName::Pencil)
                        .label(if editing { "Done" } else { "Edit colors" })
                        .on_click(move |_, window, cx| {
                            let (id, name) = (edit_id.clone(), name.clone());
                            edit_this.update(cx, |e, cx| {
                                e.pick = None;
                                if e.editing.as_deref() == Some(id.as_str()) {
                                    e.editing = None;
                                } else {
                                    e.editing = Some(id);
                                    e.rename.update(cx, |s, cx| s.set_value(name, window, cx));
                                }
                                cx.notify();
                            });
                        }),
                    )
                    .child(
                        crate::ui::kit::button::quiet(SharedString::from(format!(
                            "theme-delete-{id}"
                        )))
                        .icon(IconName::Trash)
                        .tooltip("Delete this theme")
                        .on_click(move |_, _window, cx| {
                            let id = delete_id.clone();
                            delete_this.update(cx, |e, _| {
                                if e.editing.as_deref() == Some(id.as_str()) {
                                    e.editing = None;
                                }
                            });
                            appearance::update(cx, |a| {
                                a.delete_theme(&id);
                            });
                        }),
                    )
                    .into_any_element(),
            );
            if editing {
                rows.push(self.theme_editor(theme_of_user, cx));
            }
        }
        let create = this.clone();
        rows.push(form::field(
            "New theme",
            Some("A copy of the theme in force"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(field::text(&self.new_theme))
                .child(
                    crate::ui::kit::button::accent("theme-create")
                        .icon(IconName::Copy)
                        .label("Copy")
                        .on_click(move |_, window, cx| {
                            create.update(cx, |e, cx| e.create_theme(window, cx));
                        }),
                ),
        ));
        form::group(IconName::Wand, "Your themes", rows)
    }

    /// The colors of a theme of the user's, each with the swatch that changes it.
    pub(in crate::ui::shell::settings_hub) fn theme_editor(
        &self,
        theme_of_user: &appearance::CustomTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = theme_of_user.id.clone();
        let mut list = div().flex().flex_col().gap_1().py_2();
        list = list.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(tokens::height::large()))
                .child(
                    div()
                        .w(px(crate::ui::kit::tokens::field::select()))
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::muted_fg())
                        .child("Name"),
                )
                .child(field::text(&self.rename))
                .child(
                    div()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child("Enter to rename"),
                ),
        );
        for field in ColorField::ALL {
            let id_for_set = id.clone();
            let color = field.get(&theme_of_user.colors);
            let findings = contrast::failing_for(&theme_of_user.colors, field);
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .min_h(px(tokens::height::large()))
                    .child(
                        div()
                            .w(px(crate::ui::kit::tokens::field::select()))
                            .flex_none()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(field.label()),
                    )
                    .child(self.swatch(
                        Pick::Theme(field),
                        color,
                        &format!("theme-color-{field:?}"),
                        cx,
                        move |a, value| {
                            a.set_theme_color(&id_for_set, field, value);
                        },
                    ))
                    .when(!findings.is_empty(), |el| {
                        el.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(tokens::text::small()))
                                .text_color(theme::amber())
                                .child(
                                    findings
                                        .iter()
                                        .map(contrast::Finding::describe)
                                        .collect::<Vec<_>>()
                                        .join(". "),
                                ),
                        )
                    }),
            );
        }
        let failing = contrast::failing(&theme_of_user.colors);
        let fix = cx.entity();
        let fix_id = id.clone();
        list = list.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .pt_2()
                .child(
                    div()
                        .flex_1()
                        .text_size(px(tokens::text::small()))
                        .text_color(if failing.is_empty() {
                            theme::muted_fg()
                        } else {
                            theme::amber()
                        })
                        .child(if failing.is_empty() {
                            "Every color reads well: text 4.5:1, the accent and the chart 3:1 (WCAG 2.2)."
                                .to_owned()
                        } else {
                            format!(
                                "{} of {} contrast checks fall short. A low contrast is yours to keep.",
                                failing.len(),
                                contrast::RULES.len()
                            )
                        }),
                )
                .child(
                    crate::ui::kit::button::standard("theme-fix-contrast")
                        .icon(IconName::Wand)
                        .label("Fix contrast")
                        .disabled(failing.is_empty())
                        .on_click(move |_, _, cx| {
                            let id = fix_id.clone();
                            appearance::update(cx, |a| {
                                a.fix_theme_contrast(&id);
                            });
                            fix.update(cx, |_, cx| cx.notify());
                        }),
                ),
        );
        list.into_any_element()
    }

    /// Makes a theme of the user's from the one in force, and starts editing it.
    pub(in crate::ui::shell::settings_hub) fn create_theme(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.new_theme.read(cx).value().to_string();
        let from = appearance::active_id(cx);
        let mut made: Option<String> = None;
        appearance::update(cx, |a| {
            made = a.copy_theme(&from, &name);
        });
        match made {
            Some(id) => {
                self.new_theme
                    .update(cx, |s, cx| s.set_value("", window, cx));
                let name = appearance::get(cx)
                    .find(&id)
                    .map(|(n, _)| n)
                    .unwrap_or_default();
                self.rename
                    .update(cx, |s, cx| s.set_value(name, window, cx));
                self.editing = Some(id);
                self.pick = None;
            }
            None => toast::show(
                cx,
                toast::Kind::Warning,
                "Name the theme first",
                "Type a name, then copy. You can keep up to 30 themes.",
            ),
        }
        cx.notify();
    }
}
