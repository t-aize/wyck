//! The panel that customizes the account panel: how the rows look, which tabs and figures show,
//! the columns of each table, the history, and the buttons of a row.
//!
//! It is built like the other settings panels (see [`crate::app::settings_ui`]) and shows in the
//! same modal. Every change applies to the account panel at once and is remembered.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Window, div};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Sizable};

use super::AccountPanel;
use super::prefs::{HistoryRange, PanelPrefs, ProfitUnit, RowDensity, Tab, TimeStyle};
use crate::app::settings_ui::{self as ui, Head, Tab as SettingsTab};
use crate::app::trading::ticket::prefs::{Placed, Slot, shift};
use crate::app::{modal, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Look,
    Tabs,
    Columns,
    History,
    Buttons,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::Look,
        Self::Tabs,
        Self::Columns,
        Self::History,
        Self::Buttons,
    ];

    fn tab(self) -> SettingsTab {
        match self {
            Self::Look => SettingsTab {
                label: "Look",
                icon: IconName::Palette,
            },
            Self::Tabs => SettingsTab {
                label: "Tabs and figures",
                icon: IconName::LayoutList,
            },
            Self::Columns => SettingsTab {
                label: "Columns",
                icon: IconName::Columns3,
            },
            Self::History => SettingsTab {
                label: "History",
                icon: IconName::Clock,
            },
            Self::Buttons => SettingsTab {
                label: "Row buttons",
                icon: IconName::MousePointerClick,
            },
        }
    }
}

pub struct Customizer {
    panel: Entity<AccountPanel>,
    page: Page,
    /// The table whose columns the columns page lists.
    table: Tab,
}

/// Opens the panel for an account panel.
pub fn open(panel: Entity<AccountPanel>, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        let editor = cx.new(|cx| {
            cx.observe(&panel, |_this: &mut Customizer, _panel, cx| cx.notify())
                .detach();
            let table = panel.read(cx).prefs().tab;
            Customizer {
                panel,
                page: Page::Look,
                table,
            }
        });
        modal::open(editor, modal::Options::new(760.0, 600.0), window, cx);
    });
}

impl Customizer {
    fn edit(&self, cx: &mut App, change: impl FnOnce(&mut PanelPrefs)) {
        self.panel.update(cx, |p, cx| p.edit_prefs(cx, change));
    }

    fn flag(
        &self,
        id: &'static str,
        on: bool,
        cx: &Context<Self>,
        set: fn(&mut PanelPrefs, bool),
    ) -> AnyElement {
        let this = cx.entity();
        ui::toggle(id, on, move |value, _, cx| {
            this.update(cx, |c, cx| c.edit(cx, |p| set(p, value)));
        })
        .into_any_element()
    }

    fn choice(
        &self,
        id: &'static str,
        options: &[&str],
        selected: usize,
        cx: &Context<Self>,
        set: fn(&mut PanelPrefs, usize),
    ) -> AnyElement {
        let this = cx.entity();
        widgets::segmented(id, options, selected, move |index, _, cx| {
            this.update(cx, |c, cx| c.edit(cx, |p| set(p, index)));
        })
        .into_any_element()
    }

    /// A row of a list: its name, the arrows that move it and the switch that shows it.
    fn slot_row(
        id: SharedString,
        label: &'static str,
        shown: bool,
        first: bool,
        last: bool,
        cx: &Context<Self>,
        edit: impl Fn(&mut Customizer, &mut App, SlotEdit) + Clone + 'static,
    ) -> AnyElement {
        let this = cx.entity();
        let (up, down, toggle) = (this.clone(), this.clone(), this);
        let (edit_up, edit_down, edit_toggle) = (edit.clone(), edit.clone(), edit);
        ui::field(
            label,
            None,
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    Button::new(SharedString::from(format!("{id}-up")))
                        .cursor_pointer()
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronUp)
                        .tooltip("Move up")
                        .disabled(first)
                        .on_click(move |_, _, cx| {
                            let edit = edit_up.clone();
                            up.update(cx, |c, cx| edit(c, cx, SlotEdit::Move(-1)));
                        }),
                )
                .child(
                    Button::new(SharedString::from(format!("{id}-down")))
                        .cursor_pointer()
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronDown)
                        .tooltip("Move down")
                        .disabled(last)
                        .on_click(move |_, _, cx| {
                            let edit = edit_down.clone();
                            down.update(cx, |c, cx| edit(c, cx, SlotEdit::Move(1)));
                        }),
                )
                .child(ui::toggle(
                    SharedString::from(format!("{id}-on")),
                    shown,
                    move |_, _, cx| {
                        let edit = edit_toggle.clone();
                        toggle.update(cx, |c, cx| edit(c, cx, SlotEdit::Toggle));
                    },
                )),
        )
    }

    /// A list of slots kept in the prefs.
    fn slots<T: Slot>(
        &self,
        id: &'static str,
        list: &[Placed<T>],
        pick: fn(&mut PanelPrefs) -> &mut Vec<Placed<T>>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let last = list.len().saturating_sub(1);
        list.iter()
            .enumerate()
            .map(|(index, placed)| {
                Self::slot_row(
                    SharedString::from(format!("{id}-{index}")),
                    placed.item.label(),
                    placed.shown,
                    index == 0,
                    index == last,
                    cx,
                    move |c: &mut Customizer, cx: &mut App, what| {
                        c.edit(cx, |p| match what {
                            SlotEdit::Move(delta) => {
                                shift(pick(p), index, delta);
                            }
                            SlotEdit::Toggle => {
                                let list = pick(p);
                                list[index].shown = !list[index].shown;
                            }
                        });
                    },
                )
            })
            .collect()
    }

    fn look_page(&self, prefs: &PanelPrefs, cx: &Context<Self>) -> AnyElement {
        let densities: Vec<&str> = RowDensity::ALL.iter().map(|d| d.label()).collect();
        let units: Vec<&str> = ProfitUnit::ALL.iter().map(|u| u.label()).collect();
        let times: Vec<&str> = TimeStyle::ALL.iter().map(|t| t.label()).collect();
        ui::page()
            .child(ui::group(
                IconName::Rows3,
                "Rows",
                [
                    ui::field(
                        "Row height",
                        None,
                        self.choice(
                            "panel-density",
                            &densities,
                            RowDensity::ALL
                                .iter()
                                .position(|d| *d == prefs.density)
                                .unwrap_or(1),
                            cx,
                            |p, i| p.density = RowDensity::ALL[i],
                        ),
                    ),
                    ui::field(
                        "Alternate row shading",
                        None,
                        self.flag("panel-zebra", prefs.zebra, cx, |p, v| p.zebra = v),
                    ),
                    ui::field(
                        "Tint the rows that gain or lose",
                        None,
                        self.flag("panel-tint", prefs.tint_rows, cx, |p, v| p.tint_rows = v),
                    ),
                    ui::field(
                        "Buy and sell in color",
                        None,
                        self.flag("panel-side-color", prefs.color_side, cx, |p, v| {
                            p.color_side = v;
                        }),
                    ),
                ],
            ))
            .child(ui::group(
                IconName::Hash,
                "Figures",
                [
                    ui::field(
                        "Profit of a position",
                        Some("How the profit column is written"),
                        self.choice(
                            "panel-profit-unit",
                            &units,
                            ProfitUnit::ALL
                                .iter()
                                .position(|u| *u == prefs.profit_unit)
                                .unwrap_or(0),
                            cx,
                            |p, i| p.profit_unit = ProfitUnit::ALL[i],
                        ),
                    ),
                    ui::field(
                        "Times",
                        None,
                        self.choice(
                            "panel-time-style",
                            &times,
                            TimeStyle::ALL
                                .iter()
                                .position(|t| *t == prefs.time_style)
                                .unwrap_or(0),
                            cx,
                            |p, i| p.time_style = TimeStyle::ALL[i],
                        ),
                    ),
                    ui::field(
                        "Line of totals under a table",
                        None,
                        self.flag("panel-totals", prefs.totals, cx, |p, v| p.totals = v),
                    ),
                ],
            ))
            .child(ui::group(
                IconName::MousePointerClick,
                "Behavior",
                [
                    ui::field(
                        "Search and filters over the table",
                        None,
                        self.flag("panel-filters", prefs.filters, cx, |p, v| p.filters = v),
                    ),
                    ui::field(
                        "A click on a row shows its symbol",
                        Some("On the active chart"),
                        self.flag("panel-click-shows", prefs.click_shows_symbol, cx, |p, v| {
                            p.click_shows_symbol = v
                        }),
                    ),
                    ui::field(
                        "Ask before closing",
                        Some("A position, many at once, a reversal or an order"),
                        self.flag("panel-confirm", prefs.confirm_close, cx, |p, v| {
                            p.confirm_close = v;
                        }),
                    ),
                ],
            ))
            .into_any_element()
    }

    fn tabs_page(&self, prefs: &PanelPrefs, cx: &Context<Self>) -> AnyElement {
        ui::page()
            .child(ui::group(
                IconName::LayoutList,
                "Tabs",
                self.slots("panel-tabs", &prefs.tabs, |p| &mut p.tabs, cx),
            ))
            .child(ui::note(
                "Switch a tab off to hide it, and move it with the arrows to change where it sits.",
            ))
            .child(ui::group(
                IconName::Hash,
                "Figures of the account",
                self.slots("panel-stats", &prefs.stats, |p| &mut p.stats, cx),
            ))
            .into_any_element()
    }

    fn columns_page(&self, prefs: &PanelPrefs, cx: &Context<Self>) -> AnyElement {
        let table = self.table;
        let tabs: Vec<Tab> = Tab::ALL.to_vec();
        let labels: Vec<&str> = tabs.iter().map(|t| t.label()).collect();
        let index = tabs.iter().position(|t| *t == table).unwrap_or(0);
        let this = cx.entity();
        let picker = widgets::segmented("panel-column-table", &labels, index, move |i, _, cx| {
            this.update(cx, |c, cx| {
                c.table = Tab::ALL[i];
                cx.notify();
            });
        });
        let list = prefs.column_list(table);
        let last = list.len().saturating_sub(1);
        let rows: Vec<AnyElement> = list
            .into_iter()
            .enumerate()
            .map(|(slot, (label, shown))| {
                Self::slot_row(
                    SharedString::from(format!("panel-column-{slot}")),
                    label,
                    shown,
                    slot == 0,
                    slot == last,
                    cx,
                    move |c: &mut Customizer, cx: &mut App, what| {
                        let tab = c.table;
                        c.edit(cx, |p| match what {
                            SlotEdit::Move(delta) => p.move_column(tab, slot, delta),
                            SlotEdit::Toggle => p.toggle_column(tab, slot),
                        });
                    },
                )
            })
            .collect();
        let reset = cx.entity();
        ui::page()
            .child(ui::block(picker))
            .child(ui::group(IconName::Columns3, "Columns of the table", rows))
            .child(ui::note(
                "A column is also resized by dragging the edge of its header, and sorted by clicking it. A right click on the header lists the columns.",
            ))
            .child(
                div().flex().flex_row().child(
                    ui::action(
                        "panel-reset-columns",
                        "Reset these columns",
                        Some(IconName::RotateCcw),
                        false,
                        move |_, cx| {
                            reset.update(cx, |c, cx| {
                                let tab = c.table;
                                c.edit(cx, |p| p.reset_columns(tab));
                            });
                        },
                    ),
                ),
            )
            .into_any_element()
    }

    fn history_page(&self, prefs: &PanelPrefs, cx: &Context<Self>) -> AnyElement {
        let ranges: Vec<&str> = HistoryRange::ALL.iter().map(|r| r.label()).collect();
        ui::page()
            .child(ui::group(
                IconName::Clock,
                "History",
                [
                    ui::field(
                        "How far back",
                        Some("The account keeps the last seven days"),
                        self.choice(
                            "panel-history-range",
                            &ranges,
                            HistoryRange::ALL
                                .iter()
                                .position(|r| *r == prefs.history_range)
                                .unwrap_or(3),
                            cx,
                            |p, i| p.history_range = HistoryRange::ALL[i],
                        ),
                    ),
                    ui::field(
                        "Deals that open a position",
                        Some("Beside the deals that close one"),
                        self.flag(
                            "panel-history-opening",
                            prefs.history_opening,
                            cx,
                            |p, v| {
                                p.history_opening = v;
                            },
                        ),
                    ),
                    ui::field(
                        "Figures of the closed trades",
                        Some("Win rate, profit factor, best and worst, over the table"),
                        self.flag("panel-history-stats", prefs.history_stats, cx, |p, v| {
                            p.history_stats = v;
                        }),
                    ),
                ],
            ))
            .into_any_element()
    }

    fn buttons_page(&self, prefs: &PanelPrefs, cx: &Context<Self>) -> AnyElement {
        ui::page()
            .child(ui::group(
                IconName::MousePointerClick,
                "Buttons at the end of a row",
                self.slots("panel-actions", &prefs.actions, |p| &mut p.actions, cx),
            ))
            .child(ui::note(
                "For positions, and for orders (modify and cancel). Everything here is also in the menu of a right click on a row.",
            ))
            .into_any_element()
    }
}

/// What a row of a list asks for.
#[derive(Debug, Clone, Copy)]
enum SlotEdit {
    Move(isize),
    Toggle,
}

impl Render for Customizer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefs = self.panel.read(cx).prefs().clone();
        let tabs: Vec<SettingsTab> = Page::ALL.iter().map(|p| p.tab()).collect();
        let active = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let body = match self.page {
            Page::Look => self.look_page(&prefs, cx),
            Page::Tabs => self.tabs_page(&prefs, cx),
            Page::Columns => self.columns_page(&prefs, cx),
            Page::History => self.history_page(&prefs, cx),
            Page::Buttons => self.buttons_page(&prefs, cx),
        };
        let this = cx.entity();
        let panel = self.panel.clone();
        ui::frame(
            Head {
                icon: IconName::PanelBottom,
                title: "Account panel".into(),
                subtitle: "Every change applies at once".into(),
            },
            &tabs,
            active,
            move |index, _, cx| {
                this.update(cx, |c, cx| {
                    c.page = Page::ALL[index];
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            ui::footer(
                vec![
                    ui::action(
                        "panel-customize-reset",
                        "Reset everything",
                        Some(IconName::RotateCcw),
                        false,
                        move |_, cx| panel.update(cx, |p, cx| p.reset_prefs(cx)),
                    )
                    .into_any_element(),
                ],
                vec![
                    ui::action("panel-customize-done", "Done", None, true, |window, cx| {
                        modal::close(window, cx);
                    })
                    .into_any_element(),
                ],
            ),
        )
    }
}
