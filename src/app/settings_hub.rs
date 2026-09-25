//! The settings of the whole app, opened from the account menu: how it looks, the colors of the
//! charts, how the tools behave, and the backup of everything the user made.
//!
//! Every change applies at once and is saved as it is made, so there is nothing to confirm: the
//! app behind the panel shows the result live.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px, rgb};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Disableable, Sizable};

use super::appearance::presets::CANDLE_SETS;
use super::appearance::{self, ColorField, Mode};
use super::multichart::MultiChart;
use super::settings_ui::{self as ui, Head};
use super::theme::Colors;
use super::workspace::Workspace;
use super::{backup, modal, theme, toast, widgets};

/// Opens the settings.
pub fn open(
    workspace: Entity<Workspace>,
    multi: Entity<MultiChart>,
    window: &mut Window,
    cx: &mut App,
) {
    // Opened once whatever asked is done updating.
    window.defer(cx, move |window, cx| {
        let hub = cx.new(|cx| SettingsHub::new(workspace, multi, window, cx));
        modal::open(hub, modal::Options::new(920.0, 700.0), window, cx);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Appearance,
    Charts,
    Behaviour,
    Data,
    About,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::Appearance,
        Self::Charts,
        Self::Behaviour,
        Self::Data,
        Self::About,
    ];

    fn tab(self) -> ui::Tab {
        let (label, icon) = match self {
            Self::Appearance => ("Appearance", IconName::Palette),
            Self::Charts => ("Charts", IconName::ChartCandlestick),
            Self::Behaviour => ("Behavior", IconName::SlidersHorizontal),
            Self::Data => ("Data and backup", IconName::Database),
            Self::About => ("About", IconName::Info),
        };
        ui::Tab { label, icon }
    }
}

/// Which color panel is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    Accent,
    CandleUp,
    CandleDown,
    ChartLine,
    ChartBackground,
    Theme(ColorField),
}

/// The colors offered for the accent.
const ACCENTS: [u32; 10] = [
    0x7c86ff, 0x3b82f6, 0x06b6d4, 0x10b981, 0x84cc16, 0xeab308, 0xf97316, 0xef4444, 0xec4899,
    0xa855f7,
];

/// How many fonts the list shows at once.
const FONTS_SHOWN: usize = 60;

/// What a backup step last said.
struct Notice {
    ok: bool,
    text: String,
}

struct SettingsHub {
    workspace: Entity<Workspace>,
    multi: Entity<MultiChart>,
    page: Page,
    pick: Option<Pick>,
    /// The theme of the user's whose colors are being edited.
    editing: Option<String>,
    new_theme: Entity<InputState>,
    rename: Entity<InputState>,
    font_filter: Entity<InputState>,
    fonts: Vec<String>,
    fonts_open: bool,
    notice: Option<Notice>,
    /// What the backup waiting to be applied holds, once one was chosen.
    waiting: Option<Vec<String>>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsHub {
    fn new(
        workspace: Entity<Workspace>,
        multi: Entity<MultiChart>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let new_theme =
            cx.new(|cx| InputState::new(window, cx).placeholder("Name of the new theme"));
        let rename = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let font_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search the fonts"));
        let subscriptions = vec![
            cx.observe(&workspace, |_this, _workspace, cx| cx.notify()),
            cx.subscribe(&font_filter, |_this, _input, _event: &InputEvent, cx| {
                cx.notify();
            }),
            cx.subscribe(&rename, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur)
                    && let Some(id) = this.editing.clone()
                {
                    let name = state.read(cx).value().to_string();
                    appearance::update(cx, |a| {
                        a.rename_theme(&id, &name);
                    });
                }
            }),
        ];
        let mut fonts = cx.text_system().all_font_names();
        fonts.retain(|name| !name.starts_with('.') && !name.starts_with('@'));
        fonts.sort_by_key(|name| name.to_lowercase());
        fonts.dedup();
        let waiting = cx
            .try_global::<PendingSummary>()
            .map(|p| p.0.clone())
            .filter(|_| config_dir().is_some_and(|dir| backup::pending(&dir)));
        Self {
            workspace,
            multi,
            page: Page::Appearance,
            pick: None,
            editing: None,
            new_theme,
            rename,
            font_filter,
            fonts,
            fonts_open: false,
            notice: None,
            waiting,
            _subscriptions: subscriptions,
        }
    }

    fn toggle_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        self.pick = if self.pick == Some(pick) {
            None
        } else {
            Some(pick)
        };
        cx.notify();
    }

    /// A color swatch for a setting the look holds, with the panel that opens under it.
    fn swatch(
        &self,
        pick: Pick,
        color: u32,
        id: &str,
        cx: &mut Context<Self>,
        set: impl Fn(&mut appearance::Appearance, u32) + Clone + 'static,
    ) -> AnyElement {
        let (toggle, choose) = (cx.entity(), cx.entity());
        let _ = &choose;
        widgets::color_swatch(
            SharedString::from(id.to_owned()),
            color,
            self.pick == Some(pick),
            cx,
            move |_window, cx| toggle.update(cx, |e, cx| e.toggle_pick(pick, cx)),
            move |color, _window, cx| {
                let set = set.clone();
                appearance::update(cx, |a| set(a, color));
            },
        )
    }

    // ---- appearance ----

    /// A card that shows a theme: its colors in miniature, its name, and a check when it is the
    /// one in force.
    fn theme_card(
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
                            .text_size(px(12.))
                            .text_color(theme::fg())
                            .child(name.to_owned()),
                    )
                    .children(custom.then(|| ui::small_icon(IconName::Pencil, theme::muted_fg())))
                    .children(active.then(|| ui::small_icon(IconName::Check, theme::accent()))),
            )
            .into_any_element()
    }

    fn appearance_page(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let theme_group = ui::group(
            IconName::Palette,
            "Theme",
            [
                ui::field(
                    "Mode",
                    Some("System follows the light or dark setting of your computer"),
                    widgets::segmented(
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
                ui::block(grid),
                ui::block(ui::note(
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
        let accent_group = ui::group(
            IconName::Sparkles,
            "Accent",
            [
                ui::block(accents),
                ui::field(
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
        let motion = ui::group(
            IconName::Zap,
            "Motion",
            [ui::field(
                "Animations",
                Some("Screens and panels fade and slide in. Off makes them appear at once"),
                ui::toggle(
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

        ui::page()
            .child(theme_group)
            .child(accent_group)
            .child(themes_group)
            .child(font_group)
            .child(motion)
            .child(div().flex().flex_row().child(reset))
            .into_any_element()
    }

    /// A round choice of accent: `None` is the theme's own.
    fn accent_dot(&self, color: Option<u32>, chosen: bool, cx: &mut Context<Self>) -> AnyElement {
        let shown = color.unwrap_or(0x000000);
        let base = div()
            .id(SharedString::from(format!("accent-{}", color.unwrap_or(0))))
            .flex()
            .items_center()
            .justify_center()
            .size(px(26.))
            .rounded_full()
            .border_2()
            .border_color(if chosen {
                theme::fg()
            } else {
                theme::border_hairline()
            })
            .cursor_pointer()
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(match color {
                    Some(_) => "Use this accent",
                    None => "The theme's own accent",
                })
                .build(window, cx)
            })
            .on_click(cx.listener(move |_this, _event, _window, cx| {
                appearance::update(cx, |a| a.accent = color);
            }));
        match color {
            Some(_) => base
                .child(div().size(px(18.)).rounded_full().bg(rgb(shown)))
                .into_any_element(),
            None => base
                .child(ui::small_icon(IconName::RotateCcw, theme::muted_fg()))
                .into_any_element(),
        }
    }

    fn custom_themes_group(&self, a: &appearance::Appearance, cx: &mut Context<Self>) -> gpui::Div {
        let this = cx.entity();
        let mut rows: Vec<AnyElement> = Vec::new();
        if a.custom_themes.is_empty() {
            rows.push(ui::block(ui::note(
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
                            .text_size(px(13.))
                            .text_color(theme::fg())
                            .child(name.clone()),
                    )
                    .children(used.then(|| {
                        div()
                            .text_size(px(11.))
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
        rows.push(ui::field(
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
        ui::group(IconName::Wand, "Your themes", rows)
    }

    /// The colors of a theme of the user's, each with the swatch that changes it.
    fn theme_editor(
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
                .h(px(34.))
                .child(
                    div()
                        .w(px(150.))
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child("Name"),
                )
                .child(div().w(px(220.)).child(Input::new(&self.rename).small()))
                .child(
                    div()
                        .text_size(px(11.))
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
                    .h(px(34.))
                    .child(
                        div()
                            .w(px(150.))
                            .text_size(px(12.))
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
    fn create_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn font_group(&self, a: &appearance::Appearance, cx: &mut Context<Self>) -> gpui::Div {
        let this = cx.entity();
        let toggle = this.clone();
        let mut rows = vec![ui::field(
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
                items.push(ui::note("No font matches.").p_3().into_any_element());
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
                        .h(px(30.))
                        .px_2()
                        .cursor_pointer()
                        .text_size(px(13.))
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
                        .children(chosen.then(|| ui::small_icon(IconName::Check, theme::accent())))
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
            rows.push(ui::block(
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
        ui::group(IconName::Type, "Typography", rows)
    }

    // ---- charts ----

    /// A small drawing of candles in the colors in force, to see a change before leaving.
    fn candle_preview(&self) -> AnyElement {
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

    fn charts_page(&self, cx: &mut Context<Self>) -> AnyElement {
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
                    .h(px(34.))
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
                    .child(div().text_size(px(12.)).text_color(theme::fg()).child(name)),
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

        let candles = ui::group(
            IconName::ChartCandlestick,
            "Candles",
            [
                ui::block(self.candle_preview()),
                ui::block(sets),
                ui::field(
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
                ui::field(
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
                ui::field(
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

        let background = ui::group(
            IconName::PaintBucket,
            "Background",
            [ui::field(
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

        ui::page()
            .child(candles)
            .child(background)
            .child(ui::note(
                "The type of a chart (candles, hollow candles, bars, Heikin Ashi, line...) is set on each chart, from its toolbar.",
            ))
            .into_any_element()
    }

    // ---- behavior ----

    fn behaviour_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.workspace.read(cx).preferences().clone();
        let (magnet, keep, bar, names) = (
            self.multi.clone(),
            self.multi.clone(),
            self.multi.clone(),
            self.multi.clone(),
        );
        ui::page()
            .child(ui::group(
                IconName::PenLine,
                "Drawing",
                [
                    ui::field(
                        "Stay in drawing mode",
                        Some("Keep the tool picked after a drawing, to draw several in a row"),
                        ui::toggle("behaviour-keep", prefs.keep_drawing, move |on, _w, cx| {
                            if on != prefs.keep_drawing {
                                keep.update(cx, |m, cx| m.toggle_keep_drawing(cx));
                            }
                        }),
                    ),
                    ui::field(
                        "Magnet",
                        Some("Drawings snap to the open, high, low and close of the nearest bar"),
                        ui::toggle("behaviour-magnet", prefs.magnet, move |on, _w, cx| {
                            if on != prefs.magnet {
                                magnet.update(cx, |m, cx| m.toggle_magnet(cx));
                            }
                        }),
                    ),
                    ui::field(
                        "Favorites bar",
                        Some("The drawing tools you pinned, over the charts"),
                        ui::toggle("behaviour-bar", prefs.favorites_bar, move |on, _w, cx| {
                            if on != prefs.favorites_bar {
                                bar.update(cx, |m, cx| m.toggle_favorites_bar(cx));
                            }
                        }),
                    ),
                    ui::field(
                        "Names in the favorites bar",
                        None,
                        ui::toggle(
                            "behaviour-names",
                            prefs.favorites_labels,
                            move |on, _w, cx| {
                                if on != prefs.favorites_labels {
                                    names.update(cx, |m, cx| m.toggle_favorite_names(cx));
                                }
                            },
                        ),
                    ),
                ],
            ))
            .child(ui::note(
                "More settings live where they are used: the order ticket, the chart settings, and each drawing and indicator.",
            ))
            .into_any_element()
    }

    // ---- data ----

    fn data_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let (export, import, cancel, restart, open_folder) = (
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
            cx.entity(),
        );
        let dir = config_dir();
        let mut rows: Vec<AnyElement> = vec![
            ui::field(
                "Export everything",
                Some(
                    "One file: your look, charts and indicators, drawings and their saved looks, favorites, watchlists and alerts. No sign-in is in it",
                ),
                Button::new("backup-export")
                    .cursor_pointer()
                    .primary()
                    .small()
                    .icon(IconName::FileDown)
                    .label("Export")
                    .on_click(move |_, window, cx| {
                        export.update(cx, |e, cx| e.export(window, cx));
                    }),
            ),
            ui::field(
                "Import a backup",
                Some(
                    "Checked first, then applied when wyck starts again. What it replaces is kept in a folder of backups",
                ),
                Button::new("backup-import")
                    .cursor_pointer()
                    .ghost()
                    .small()
                    .icon(IconName::FileUp)
                    .label("Import")
                    .on_click(move |_, window, cx| {
                        import.update(cx, |e, cx| e.import(window, cx));
                    }),
            ),
        ];
        if let Some(lines) = &self.waiting {
            let mut waiting = div().flex().flex_col().gap_1();
            waiting = waiting.child(
                div()
                    .text_size(px(13.))
                    .text_color(theme::amber())
                    .child("A backup is waiting to be applied when wyck starts again:"),
            );
            for line in lines {
                waiting = waiting.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child(format!("- {line}")),
                );
            }
            waiting = waiting.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .pt_1p5()
                    .child(
                        Button::new("backup-restart")
                            .cursor_pointer()
                            .primary()
                            .small()
                            .icon(IconName::RefreshCw)
                            .label("Restart now")
                            .on_click(move |_, _window, cx| {
                                restart.update(cx, |_, _| {});
                                cx.restart();
                            }),
                    )
                    .child(
                        Button::new("backup-cancel")
                            .cursor_pointer()
                            .ghost()
                            .small()
                            .label("Cancel the import")
                            .on_click(move |_, _window, cx| {
                                cancel.update(cx, |e, cx| e.cancel_import(cx));
                            }),
                    ),
            );
            rows.push(ui::block(waiting));
        }
        if let Some(notice) = &self.notice {
            rows.push(ui::block(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(ui::small_icon(
                        if notice.ok {
                            IconName::CircleCheck
                        } else {
                            IconName::Info
                        },
                        if notice.ok {
                            theme::emerald()
                        } else {
                            theme::destructive()
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .text_color(theme::fg())
                            .child(notice.text.clone()),
                    ),
            ));
        }
        let backup_group = ui::group(IconName::Archive, "Backup", rows);

        let mut place: Vec<AnyElement> = vec![ui::field(
            "Settings folder",
            Some("Where everything is kept, one readable file per kind of data"),
            Button::new("backup-folder")
                .cursor_pointer()
                .ghost()
                .small()
                .icon(IconName::FolderOpen)
                .label("Open")
                .disabled(dir.is_none())
                .on_click(move |_, _window, cx| {
                    open_folder.update(cx, |_, _| {});
                    if let Some(dir) = config_dir() {
                        cx.reveal_path(&dir);
                    }
                }),
        )];
        if let Some(dir) = &dir {
            place.push(ui::block(ui::note(dir.display().to_string())));
        }
        ui::page()
            .child(backup_group)
            .child(ui::group(IconName::HardDrive, "Where it is", place))
            .child(ui::note(
                "The keys that sign in to your broker are not in a backup: they stay in the system's secure storage.",
            ))
            .into_any_element()
    }

    /// Asks where to save, then writes the backup there.
    fn export(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = config_dir() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        // What waits to be saved goes to the disk first, so the backup is what is on screen.
        self.multi.read(cx).flush_documents(cx);
        appearance::save_now(cx);
        let stamp = chrono::Local::now().format("%Y-%m-%d").to_string();
        let name = format!("wyck-backup-{stamp}.toml");
        let start = directories_start();
        let picked = cx.prompt_for_new_path(&start, Some(&name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let created = chrono::Local::now().to_rfc3339();
            let result = backup::collect(&dir, env!("CARGO_PKG_VERSION"), &created)
                .map_err(backup::BackupError::from)
                .and_then(|b| backup::to_text(&b).map(|text| (b, text)))
                .and_then(|(b, text)| {
                    std::fs::write(&path, text)?;
                    Ok(b)
                });
            let _ = this.update(cx, |this, cx| match result {
                Ok(b) => {
                    let count = b.files.len();
                    this.say(
                        true,
                        format!("Saved {count} document(s) to {}.", path.display()),
                        cx,
                    );
                }
                Err(error) => this.say(false, error.to_string(), cx),
            });
        })
        .detach();
    }

    /// Asks for a backup, checks it, and sets it aside for the next start.
    fn import(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = config_dir() else {
            self.say(false, "The settings folder could not be found.", cx);
            return;
        };
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a wyck backup".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = std::fs::read_to_string(&path)
                .map_err(backup::BackupError::from)
                .and_then(|text| backup::stage(&dir, &text));
            let _ = this.update(cx, |this, cx| match result {
                Ok(b) => {
                    let lines = backup::summary(&b);
                    cx.set_global(PendingSummary(lines.clone()));
                    this.waiting = Some(lines);
                    this.say(
                        true,
                        "The backup is valid and waits for the next start.",
                        cx,
                    );
                }
                Err(error) => this.say(false, error.to_string(), cx),
            });
        })
        .detach();
    }

    fn cancel_import(&mut self, cx: &mut Context<Self>) {
        if let Some(dir) = config_dir() {
            let _ = backup::cancel_pending(&dir);
        }
        self.waiting = None;
        self.say(true, "The import is cancelled. Nothing was changed.", cx);
    }

    fn say(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(Notice {
            ok,
            text: text.into(),
        });
        cx.notify();
    }

    // ---- about ----

    fn about_page(&self) -> AnyElement {
        let facts = [
            ("Version", env!("CARGO_PKG_VERSION").to_owned()),
            ("License", "Apache License 2.0".to_owned()),
            ("Built with", "Rust and GPUI".to_owned()),
        ];
        let rows: Vec<AnyElement> = facts
            .into_iter()
            .map(|(label, value)| {
                ui::field(
                    label,
                    None,
                    div()
                        .text_size(px(13.))
                        .text_color(theme::fg())
                        .child(value),
                )
            })
            .collect();
        ui::page()
            .child(ui::group(IconName::Info, "wyck", rows))
            .child(ui::note(
                "wyck is an independent project. It is not affiliated with, endorsed by, or sponsored by cTrader or Spotware Systems. Trading carries a high risk of loss, and nothing here is financial advice.",
            ))
            .into_any_element()
    }
}

/// What the backup that waits holds, kept for as long as the app runs so that the panel can show
/// it again after being closed.
struct PendingSummary(Vec<String>);

impl gpui::Global for PendingSummary {}

/// The settings folder.
fn config_dir() -> Option<std::path::PathBuf> {
    wyck::config::AppPaths::discover()
        .ok()
        .map(|paths| paths.config_dir().to_path_buf())
}

/// Where the file dialogs start.
fn directories_start() -> std::path::PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(std::path::Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir)
}

impl Render for SettingsHub {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<ui::Tab> = Page::ALL.iter().map(|p| p.tab()).collect();
        let active = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let this = cx.entity();
        let body = match self.page {
            Page::Appearance => self.appearance_page(cx),
            Page::Charts => self.charts_page(cx),
            Page::Behaviour => self.behaviour_page(cx),
            Page::Data => self.data_page(cx),
            Page::About => self.about_page(),
        };
        let head = Head {
            icon: IconName::Settings,
            title: "Settings".into(),
            subtitle: "Look, charts, behavior, and the backup of everything you made".into(),
        };
        let footer = ui::footer(
            vec![],
            vec![
                ui::action("settings-hub-close", "Close", None, true, |window, cx| {
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        ui::frame(
            head,
            &tabs,
            active,
            move |index, _window, cx| {
                let page = Page::ALL[index];
                this.update(cx, |e, cx| {
                    e.page = page;
                    e.pick = None;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
        .into_any_element()
    }
}
