//! The Appearance page of the settings: the theme, the accent, the user's own themes, the fonts and the candle colors.

use super::*;

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
        let dot = |color: u32| div().size(px(8.)).rounded_full().bg(rgb(color));
        let preview = div()
            .h(px(58.))
            .p_1p5()
            .rounded_md()
            .bg(rgb(colors.bg))
            .flex()
            .flex_col()
            .gap_1()
            .child(div().h(px(5.)).w(px(60.)).rounded_full().bg(rgb(colors.fg)))
            .child(
                div()
                    .h(px(4.))
                    .w(px(40.))
                    .rounded_full()
                    .bg(rgb(colors.muted)),
            )
            .child(
                div()
                    .flex_1()
                    .rounded_sm()
                    .bg(rgb(colors.surface))
                    .px_1p5()
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
            .w(px(150.))
            .flex()
            .flex_col()
            .gap_1()
            .p_1p5()
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
                    .px_0p5()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(tokens::text::BODY))
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
        let mut accents = div().flex().flex_row().flex_wrap().items_center().gap_1p5();
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

        let reset = Button::new("settings-reset-look")
            .cursor_pointer()
            .ghost()
            .small()
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
            .size(px(tokens::height::COMPACT))
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
                .child(div().size(px(18.)).rounded_full().bg(rgb(shown)))
                .into_any_element(),
            None => base
                .child(icon::small(IconName::RotateCcw, theme::muted_fg()))
                .into_any_element(),
        }
    }

    pub(super) fn custom_themes_group(
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
                    .min_h(px(42.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(tokens::text::EMPHASIS))
                            .text_color(theme::fg())
                            .child(name.clone()),
                    )
                    .children(used.then(|| {
                        div()
                            .text_size(px(tokens::text::SMALL))
                            .text_color(theme::accent())
                            .child("In use")
                    }))
                    .child(
                        Button::new(SharedString::from(format!("theme-edit-{id}")))
                            .cursor_pointer()
                            .ghost()
                            .small()
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
                        Button::new(SharedString::from(format!("theme-delete-{id}")))
                            .cursor_pointer()
                            .ghost()
                            .small()
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
                .child(div().w(px(170.)).child(Input::new(&self.new_theme).small()))
                .child(
                    Button::new("theme-create")
                        .cursor_pointer()
                        .primary()
                        .small()
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
    pub(super) fn theme_editor(
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
                .h(px(tokens::height::LARGE))
                .child(
                    div()
                        .w(px(150.))
                        .text_size(px(tokens::text::BODY))
                        .text_color(theme::muted_fg())
                        .child("Name"),
                )
                .child(div().w(px(220.)).child(Input::new(&self.rename).small()))
                .child(
                    div()
                        .text_size(px(tokens::text::SMALL))
                        .text_color(theme::muted_fg())
                        .child("Enter to rename"),
                ),
        );
        for field in ColorField::ALL {
            let id_for_set = id.clone();
            let color = field.get(&theme_of_user.colors);
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(tokens::height::LARGE))
                    .child(
                        div()
                            .w(px(150.))
                            .text_size(px(tokens::text::BODY))
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
                    )),
            );
        }
        list.into_any_element()
    }

    /// Makes a theme of the user's from the one in force, and starts editing it.
    pub(super) fn create_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn font_group(
        &self,
        a: &appearance::Appearance,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let this = cx.entity();
        let toggle = this.clone();
        let mut rows = vec![form::field(
            "Interface font",
            Some("Any font installed on this computer. Inter comes with the app"),
            Button::new("settings-font-open")
                .cursor_pointer()
                .ghost()
                .small()
                .icon(IconName::Type)
                .label(SharedString::from(a.font.clone()))
                .toggled(self.fonts_open)
                .on_click(move |_, _window, cx| {
                    toggle.update(cx, |e, cx| {
                        e.fonts_open = !e.fonts_open;
                        cx.notify();
                    });
                }),
        )];
        if self.fonts_open {
            let query = self.font_filter.read(cx).value().to_lowercase();
            let mut items: Vec<AnyElement> = Vec::new();
            let matching: Vec<&String> = self
                .fonts
                .iter()
                .filter(|f| query.is_empty() || f.to_lowercase().contains(&query))
                .take(FONTS_SHOWN)
                .collect();
            if matching.is_empty() {
                items.push(form::note("No font matches.").p_3().into_any_element());
            }
            for name in matching {
                let chosen = *name == a.font;
                let name = name.clone();
                items.push(
                    div()
                        .id(SharedString::from(format!("font-{name}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .h(px(tokens::height::CONTROL))
                        .px_2()
                        .cursor_pointer()
                        .text_size(px(tokens::text::EMPHASIS))
                        .font_family(SharedString::from(name.clone()))
                        .text_color(if chosen {
                            theme::fg()
                        } else {
                            theme::muted_fg()
                        })
                        .when(chosen, |el| el.bg(theme::accent_selected()))
                        .hover(|s| s.bg(theme::surface_hover()))
                        .on_click({
                            let name = name.clone();
                            move |_, _window, cx| {
                                let name = name.clone();
                                appearance::update(cx, |a| a.font = name);
                            }
                        })
                        .child(name.clone())
                        .children(chosen.then(|| icon::small(IconName::Check, theme::accent())))
                        .into_any_element(),
                );
            }
            // A fixed height and the scrollbar of the components: a list inside a panel that scrolls
            // must have its own scroll, or the wheel goes to the panel.
            let list = div()
                .id("settings-font-list")
                .flex()
                .flex_col()
                .h(px(220.))
                .rounded_md()
                .border_1()
                .border_color(theme::border_subtle())
                .overflow_y_scrollbar()
                .children(items);
            let reset_font = this.clone();
            rows.push(form::block(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(div().flex_1().child(Input::new(&self.font_filter).small()))
                            .child(
                                Button::new("settings-font-default")
                                    .cursor_pointer()
                                    .ghost()
                                    .small()
                                    .icon(IconName::RotateCcw)
                                    .label("Inter")
                                    .on_click(move |_, _window, cx| {
                                        reset_font.update(cx, |_, _| {});
                                        appearance::update(cx, |a| {
                                            a.font = appearance::DEFAULT_FONT.to_owned();
                                        });
                                    }),
                            ),
                    )
                    // The list scrolls by itself: the wheel stops here, so the panel around it stays put.
                    .child(
                        div()
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .child(list),
                    ),
            ));
        }
        form::group(IconName::Type, "Typography", rows)
    }

    /// A small drawing of candles in the colors in force, to see a change before leaving.
    pub(super) fn candle_preview(&self) -> AnyElement {
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
            .h(px(96.))
            .px_3()
            .rounded_lg()
            .border_1()
            .border_color(theme::border_subtle())
            .bg(rgb(c.chart_bg))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(9.));
        for (up, body, above, below) in shape {
            let color = rgb(if up { c.up } else { c.down });
            row = row.child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(div().w(px(1.)).h(px(above)).bg(color))
                    .child(div().w(px(8.)).h(px(body)).bg(color))
                    .child(div().w(px(1.)).h(px(below)).bg(color)),
            );
        }
        row.child(div().flex_1().h(px(2.)).rounded_full().bg(rgb(c.line)))
            .into_any_element()
    }
}
