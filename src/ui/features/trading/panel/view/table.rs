//! The table: its rows, their buttons, and the columns that are sorted and resized.

use super::{
    BUTTON_W, RESIZE_GRIP_HOVER, RESIZE_HANDLE, ROW_MARKER_WIDTH, ask, density_height,
    density_text, icon_button, position_actions, tint, tone_color,
};
use crate::app::prefs::panel::{PanelPrefs, RowAction, Tab};
use crate::app::prefs::ticket::Slot;
use crate::ui::features::trading::panel::data::{Cell, Row, RowKind, Table};
use crate::ui::features::trading::panel::dialogs::{Target, open_alert, open_protection};
use crate::ui::features::trading::panel::{AccountPanel, MenuTarget, PanelEvent, Resize};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::prelude::{ElementExt as _, StyledExt as _};
use crate::ui::kit::{
    button,
    focus::Keyboard,
    icon,
    menu::{self as popup},
    theme, tokens,
};
use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, MouseDownEvent, Role, SharedString, div, px};
use std::rc::Rc;

impl AccountPanel {
    /// The buttons at the end of a row, by what the row is.
    pub(super) fn row_buttons(&self, row: &Row) -> (Vec<AnyElement>, f32) {
        let prefs = &self.prefs;
        let mut buttons: Vec<AnyElement> = Vec::new();
        match &row.kind {
            RowKind::Position(position) => {
                let acts = position_actions(&self.account, position, prefs.confirm_close);
                let key = &row.key;
                let busy = position.busy;
                for placed in prefs.actions.iter().filter(|a| a.shown) {
                    let id = SharedString::from(format!("{key}-{:?}", placed.item));
                    buttons.push(match placed.item {
                        RowAction::Edit => icon_button(
                            id,
                            IconName::Pencil,
                            "Stop loss and take profit",
                            true,
                            false,
                            acts.edit.clone(),
                        )
                        .into_any_element(),
                        RowAction::Half => icon_button(
                            id,
                            IconName::Scissors,
                            "Close half",
                            position.half.is_some() && !busy,
                            false,
                            acts.half.clone(),
                        )
                        .into_any_element(),
                        RowAction::BreakEven => {
                            let at_entry = position
                                .entry
                                .zip(position.stop_loss)
                                .is_some_and(|(e, s)| (e - s).abs() < 1e-9);
                            icon_button(
                                id,
                                IconName::ShieldCheck,
                                "Move the stop loss to the entry",
                                position.entry.is_some() && !at_entry && !busy,
                                at_entry,
                                acts.break_even.clone(),
                            )
                            .into_any_element()
                        }
                        RowAction::Trail => icon_button(
                            id,
                            IconName::TrendingUp,
                            if position.trailing {
                                "Stop trailing the stop loss"
                            } else {
                                "Trail the stop loss"
                            },
                            (position.stop_loss.is_some() || position.trailing) && !busy,
                            position.trailing,
                            acts.trail.clone(),
                        )
                        .into_any_element(),
                        RowAction::Reverse => icon_button(
                            id,
                            IconName::ArrowUpDown,
                            "Reverse",
                            !busy,
                            false,
                            acts.reverse.clone(),
                        )
                        .into_any_element(),
                        RowAction::Close => {
                            icon_button(id, IconName::X, "Close", !busy, false, acts.close.clone())
                                .into_any_element()
                        }
                    });
                }
            }
            RowKind::Order { id, busy, describe } => {
                let id = *id;
                let (account, edit) = (self.account.clone(), self.account.clone());
                let cancel = ask(
                    prefs.confirm_close,
                    "Cancel this order?",
                    describe.clone(),
                    move |cx| account.update(cx, |acc, cx| acc.cancel_order(id, cx)),
                );
                if prefs.shows_action(RowAction::Edit) {
                    buttons.push(
                        icon_button(
                            SharedString::from(format!("order-edit-{id}")),
                            IconName::Pencil,
                            "Price, stop loss and take profit",
                            true,
                            false,
                            Rc::new(move |window, cx| {
                                open_protection(edit.clone(), Target::Order(id), window, cx);
                            }),
                        )
                        .into_any_element(),
                    );
                }
                if prefs.shows_action(RowAction::Close) {
                    buttons.push(
                        icon_button(
                            SharedString::from(format!("order-cancel-{id}")),
                            IconName::X,
                            "Cancel",
                            !busy,
                            false,
                            cancel,
                        )
                        .into_any_element(),
                    );
                }
            }
            RowKind::Alert { id, active } => {
                let (id, active) = (*id, *active);
                let (edit, toggle, remove) = (
                    self.alerts.clone(),
                    self.alerts.clone(),
                    self.alerts.clone(),
                );
                buttons.push(
                    icon_button(
                        SharedString::from(format!("alert-edit-{id}")),
                        IconName::Pencil,
                        "Edit",
                        true,
                        false,
                        Rc::new(move |window, cx| open_alert(edit.clone(), id, window, cx)),
                    )
                    .into_any_element(),
                );
                buttons.push(
                    icon_button(
                        SharedString::from(format!("alert-toggle-{id}")),
                        if active {
                            IconName::BellOff
                        } else {
                            IconName::BellRing
                        },
                        if active { "Pause" } else { "Watch again" },
                        true,
                        false,
                        Rc::new(move |_, cx| {
                            toggle.update(cx, |alerts, cx| {
                                alerts.edit(cx, |book| {
                                    if let Some(alert) = book.get_mut(id) {
                                        alert.active = !alert.active;
                                        alert.fired_at = None;
                                    }
                                });
                            });
                        }),
                    )
                    .into_any_element(),
                );
                buttons.push(
                    icon_button(
                        SharedString::from(format!("alert-delete-{id}")),
                        IconName::Trash,
                        "Delete",
                        true,
                        false,
                        Rc::new(move |_, cx| {
                            remove.update(cx, |alerts, cx| alerts.edit(cx, |book| book.remove(id)));
                        }),
                    )
                    .into_any_element(),
                );
            }
            RowKind::Deal | RowKind::Exposure | RowKind::Firing { .. } => {}
        }
        let width = buttons.len() as f32 * tokens::scaled(BUTTON_W);
        (buttons, width)
    }

    /// The table: its header, its rows and its totals.
    pub(super) fn table_view(
        &self,
        table: &Table,
        menu: &popup::Menu,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let prefs: &PanelPrefs = &self.prefs;
        let tab = table.tab;
        let height = tokens::scaled(density_height(prefs.density));
        let text = tokens::scaled(density_text(prefs.density));
        // The width the buttons of the rows take, from the first row that has some.
        let has_more = |row: &Row| matches!(row.kind, RowKind::Position(_) | RowKind::Order { .. });
        let buttons_w = table.rows.first().map_or(0.0, |row| {
            self.row_buttons(row).1
                + if has_more(row) {
                    tokens::scaled(BUTTON_W)
                } else {
                    0.0
                }
        });
        let content_w: f32 = table
            .columns
            .iter()
            .map(|c| tokens::scaled(c.width) + 8.0)
            .sum::<f32>()
            + buttons_w
            + 24.0;

        let header_menu = menu.clone();
        let this = cx.entity();
        let header_this = this.clone();
        let mut header = div()
            .id("panel-header")
            .role(Role::Row)
            .aria_row_index(1)
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(tokens::height::compact()))
            .px_3()
            .text_size(px(tokens::text::small()))
            .text_color(theme::muted_fg())
            .border_b_1()
            .border_color(theme::border_hairline())
            .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                header_this.update(cx, |p, _| p.menu_target = Some(MenuTarget::Header));
                header_menu.open(Some(event.position), cx);
            });
        for col in &table.columns {
            let (slot, start_width) = (col.slot, col.width);
            let sorted = match col.sorted {
                Some(true) => ", sorted descending",
                Some(false) => ", sorted ascending",
                None => "",
            };
            header = header.child(
                div()
                    .id(SharedString::from(format!("panel-col-{}", col.slot)))
                    .keyboard()
                    .role(Role::ColumnHeader)
                    .aria_label(SharedString::from(format!("{}{sorted}", col.label)))
                    .aria_column_index(col.slot + 1)
                    .relative()
                    .w(px(tokens::scaled(col.width)))
                    .flex_none()
                    .h_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .when(col.right, |el| el.justify_end())
                    .hover(|s| s.text_color(theme::fg()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_prefs(cx, |prefs| prefs.cycle_sort(tab, slot));
                    }))
                    .child(div().min_w_0().truncate().child(col.label))
                    .children(col.sorted.map(|descending| {
                        icon::tinted(
                            if descending {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronUp
                            },
                            11.,
                            theme::accent(),
                        )
                    }))
                    .child({
                        // The edge of the column: a thin mark that is always there, and lights
                        // up under the pointer and while the column is dragged.
                        let group = SharedString::from(format!("panel-col-grip-{}", col.slot));
                        let active = self.resize.is_some_and(|r| r.tab == tab && r.slot == slot);
                        div()
                            .id(group.clone())
                            .group(group.clone())
                            .absolute()
                            .top_0()
                            .right(px(-RESIZE_HANDLE))
                            .h_full()
                            .w(px(RESIZE_HANDLE))
                            .flex()
                            .justify_center()
                            .items_center()
                            .cursor_col_resize()
                            .child(
                                div()
                                    .w(px(if active { 2. } else { 1. }))
                                    .h(px(if active { 22. } else { 12. }))
                                    .rounded_full()
                                    .bg(if active {
                                        theme::accent()
                                    } else {
                                        theme::border_strong()
                                    })
                                    .group_hover(group, |s| {
                                        s.bg(theme::accent()).h(px(RESIZE_GRIP_HOVER))
                                    }),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    this.resize = Some(Resize {
                                        tab,
                                        slot,
                                        start_x: f32::from(event.position.x),
                                        start_width,
                                    });
                                    cx.stop_propagation();
                                }),
                            )
                    }),
            );
        }
        if buttons_w > 0.0 {
            header = header.child(div().w(px(buttons_w)).flex_none());
        }

        let click_shows = prefs.click_shows_symbol;
        let current = self.current_index();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, row) in table.rows.iter().enumerate() {
            let (mut buttons, _) = self.row_buttons(row);
            let (row_menu, row_this, key) = (menu.clone(), this.clone(), row.key.clone());
            // What the row has no button for is in its menu, as it is under a right click.
            if has_more(row) {
                let (more_menu, more_this, more_key) =
                    (row_menu.clone(), row_this.clone(), key.clone());
                buttons.push(
                    button::icon_dense(
                        SharedString::from(format!("{key}-more")),
                        IconName::Ellipsis,
                        "More",
                    )
                    .on_click(move |event, _, cx| {
                        more_this.update(cx, |p, _| {
                            p.menu_target = Some(MenuTarget::Row(more_key.clone()));
                        });
                        more_menu.open(Some(event.position()), cx);
                    })
                    .into_any_element(),
                );
            }
            let symbol = row.symbol;
            let active_symbol = symbol.is_some() && symbol == self.symbol;
            let summary: String = table
                .columns
                .iter()
                .map(|col| format!("{}: {}", col.label, row.cells[col.index].text))
                .collect::<Vec<_>>()
                .join(", ");
            let mut el = div()
                .id(SharedString::from(row.key.clone()))
                .role(Role::Row)
                .aria_label(SharedString::from(summary))
                .aria_row_index(index + 2)
                .aria_selected(active_symbol)
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(height))
                .px_3()
                .text_size(px(text))
                .border_b_1()
                .border_color(theme::border_hairline())
                .when(prefs.zebra && index % 2 == 1, |el| {
                    el.bg(theme::fg_alpha(0.025))
                })
                .when(prefs.tint_rows, |el| match row.profit {
                    Some(p) if p > 0.0 => el.bg(tint(theme::chart_up(), 0.07)),
                    Some(p) if p < 0.0 => el.bg(tint(theme::chart_down(), 0.07)),
                    _ => el,
                })
                .relative()
                .hover(|s| s.bg(theme::surface_hover()))
                .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                    row_this.update(cx, |p, _| {
                        p.menu_target = Some(MenuTarget::Row(key.clone()))
                    });
                    row_menu.open(Some(event.position), cx);
                });
            // The row of the symbol on the active chart carries a bar at its left edge. It is a box of its own: a colored border would color the line under the row too.
            if active_symbol {
                el = el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .h_full()
                        .w(px(ROW_MARKER_WIDTH))
                        .bg(theme::accent()),
                );
            }
            // The keyboard: the row is a stop of Tab only when it is the current one, and the
            // arrows move among the rows.
            if let Some(handle) = self.row_focus.get(index) {
                let is_current = index == current;
                let handle = handle.clone().tab_index(0).tab_stop(is_current);
                let (key_this, key_menu) = (this.clone(), menu.clone());
                let (focus_this, focus_key) = (this.clone(), row.key.clone());
                el = el
                    .track_focus(&handle)
                    .focus_visible(|style| {
                        style
                            .border_color(theme::accent())
                            .bg(theme::accent_alpha(0.22))
                    })
                    .on_key_down(move |event, window, cx| {
                        key_this.update(cx, |panel, cx| {
                            panel.row_key(index, event, &key_menu, window, cx);
                        });
                    })
                    // A click puts the keyboard where the mouse is.
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        focus_this.update(cx, |panel, _| {
                            panel.current_row = Some(focus_key.clone());
                        });
                    });
                if is_current {
                    let bounds = self.row_bounds.clone();
                    el = el.on_prepaint(move |b, _, _| bounds.set(Some(b)));
                }
            }
            if click_shows && let Some(symbol) = symbol {
                el = el
                    .cursor_pointer()
                    .on_click(cx.listener(move |_this, _, _, cx| {
                        cx.emit(PanelEvent::ShowSymbol(symbol));
                    }));
            }
            for col in &table.columns {
                let cell: &Cell = &row.cells[col.index];
                el = el.child(
                    div()
                        .id(SharedString::from(format!("{}-c{}", row.key, col.slot)))
                        .role(Role::Cell)
                        .aria_label(SharedString::from(cell.text.to_string()))
                        .aria_column_index(col.slot + 1)
                        .w(px(tokens::scaled(col.width)))
                        .flex_none()
                        .truncate()
                        .text_color(tone_color(cell.tone))
                        .when(col.right, |el| el.text_right())
                        .child(cell.text.clone()),
                );
            }
            if !buttons.is_empty() {
                el = el.child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_1()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .children(buttons),
                );
            }
            rows.push(el.into_any_element());
        }

        let body = if rows.is_empty() {
            let text = if table.unfiltered > 0 {
                "Nothing matches the search and the filters."
            } else {
                match tab {
                    Tab::Positions => "No open position.",
                    Tab::Orders => "No working order.",
                    Tab::History => "No trade in this period.",
                    Tab::Exposure => "Nothing is open.",
                    Tab::Alerts => {
                        "No alert. Right click a chart, or press Alt+A, to add one at a price."
                    }
                    Tab::AlertLog => "No alert has fired yet.",
                }
            };
            div()
                .flex_1()
                .py_6()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(icon::tinted(
                    match tab {
                        Tab::Alerts | Tab::AlertLog => IconName::Bell,
                        _ => IconName::Inbox,
                    },
                    20.,
                    theme::fg_alpha(0.35),
                ))
                .child(text)
                .into_any_element()
        } else {
            div().flex().flex_col().children(rows).into_any_element()
        };

        let has_totals =
            prefs.totals && !table.rows.is_empty() && table.totals.iter().any(|t| !t.is_empty());
        let totals = has_totals.then(|| {
            let mut row = div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(tokens::height::compact()))
                .px_3()
                .text_size(px(text))
                .font_semibold()
                .border_t_1()
                .border_color(theme::border_subtle())
                .bg(theme::fg_alpha(0.03));
            for (col, total) in table.columns.iter().zip(&table.totals) {
                row = row.child(
                    div()
                        .w(px(tokens::scaled(col.width)))
                        .flex_none()
                        .truncate()
                        .text_color(theme::fg())
                        .when(col.right, |el| el.text_right())
                        .child(total.clone()),
                );
            }
            if buttons_w > 0.0 {
                row = row.child(div().w(px(buttons_w)).flex_none());
            }
            row
        });

        div()
            .id("panel-scroll-x")
            .role(Role::Table)
            .aria_label(SharedString::from(format!("{} table", tab.label())))
            .aria_row_count(table.rows.len() + 1)
            .aria_column_count(table.columns.len())
            .flex_1()
            .min_h_0()
            .overflow_x_scroll()
            .child(
                div()
                    .min_w(px(content_w))
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(
                        div()
                            .id("panel-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(body),
                    )
                    .children(totals),
            )
            .into_any_element()
    }
}
