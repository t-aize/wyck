//! The table: its header, its rows with their two buttons, and the line of totals.

use super::{Action, ask, tone_color};
use crate::app::prefs::panel::Tab;
use crate::ui::features::trading::panel::data::{Column, Row, RowKind, Table};
use crate::ui::features::trading::panel::dialogs::alert_editor::open_alert;
use crate::ui::features::trading::panel::dialogs::{Target, open_protection};
use crate::ui::features::trading::panel::{AccountPanel, PanelEvent};
use gpui::prelude::*;
use gpui::{AnyElement, Context, MouseButton, Role, SharedString, div, px};
use std::rc::Rc;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::prelude::Disableable;
use wyck_ui_kit::{button, icon, theme, tokens};

/// A small button of a row.
fn icon_button(
    id: SharedString,
    glyph: IconName,
    tip: &'static str,
    enabled: bool,
    run: Action,
) -> AnyElement {
    button::icon(id, button::Size::Sm, glyph, tip)
        .when(!enabled, |button| button.cursor_not_allowed())
        .disabled(!enabled)
        .on_click(move |_, window, cx| run(window, cx))
        .into_any_element()
}

/// A cell of the header, of a row or of the totals: the columns line up because they all start
/// from the same width and share the room left.
fn cell(col: &Column) -> gpui::Div {
    div()
        .w(px(col.width))
        .flex_grow_1()
        .flex_shrink_0()
        .min_w_0()
        .truncate()
        .when(col.right, |el| el.text_right())
}

impl AccountPanel {
    /// The two buttons at the end of a row, by what the row is: modify and close for a position or
    /// an order (cancel), modify and delete for an alert.
    fn row_buttons(&self, row: &Row) -> Vec<AnyElement> {
        match &row.kind {
            RowKind::Position(position) => {
                let (account, edit, id) = (self.account.clone(), self.account.clone(), position.id);
                let close = ask(
                    "Close this position?",
                    position.describe.clone(),
                    move |cx| account.update(cx, |acc, cx| acc.close_position(id, None, cx)),
                );
                vec![
                    icon_button(
                        SharedString::from(format!("{}-edit", row.key)),
                        IconName::Pencil,
                        "Stop loss and take profit",
                        true,
                        Rc::new(move |window, cx| {
                            open_protection(edit.clone(), Target::Position(id), window, cx);
                        }),
                    ),
                    icon_button(
                        SharedString::from(format!("{}-close", row.key)),
                        IconName::X,
                        "Close",
                        !position.busy,
                        close,
                    ),
                ]
            }
            RowKind::Order { id, busy, describe } => {
                let (account, edit, id) = (self.account.clone(), self.account.clone(), *id);
                let cancel = ask("Cancel this order?", describe.clone(), move |cx| {
                    account.update(cx, |acc, cx| acc.cancel_order(id, cx));
                });
                vec![
                    icon_button(
                        SharedString::from(format!("{}-edit", row.key)),
                        IconName::Pencil,
                        "Price, stop loss and take profit",
                        true,
                        Rc::new(move |window, cx| {
                            open_protection(edit.clone(), Target::Order(id), window, cx);
                        }),
                    ),
                    icon_button(
                        SharedString::from(format!("{}-cancel", row.key)),
                        IconName::X,
                        "Cancel",
                        !busy,
                        cancel,
                    ),
                ]
            }
            RowKind::Alert { id } => {
                let (edit, remove, id) = (self.alerts.clone(), self.alerts.clone(), *id);
                vec![
                    icon_button(
                        SharedString::from(format!("{}-edit", row.key)),
                        IconName::Pencil,
                        "Edit",
                        true,
                        Rc::new(move |window, cx| open_alert(edit.clone(), id, window, cx)),
                    ),
                    icon_button(
                        SharedString::from(format!("{}-delete", row.key)),
                        IconName::Trash,
                        "Delete",
                        true,
                        Rc::new(move |_, cx| {
                            remove.update(cx, |alerts, cx| alerts.edit(cx, |book| book.remove(id)));
                        }),
                    ),
                ]
            }
            RowKind::Plain => Vec::new(),
        }
    }

    /// The table: its header, its rows and its totals.
    pub(super) fn table_view(&self, table: &Table, cx: &mut Context<Self>) -> AnyElement {
        let tab = table.tab;
        let text = tokens::text::body();
        // The width the buttons of the rows take: two, on the tabs whose rows have some.
        let has_buttons = matches!(tab, Tab::Positions | Tab::Orders | Tab::Alerts);
        let buttons_w = if has_buttons {
            2.0 * tokens::height::compact() + tokens::scaled(4.0)
        } else {
            0.0
        };
        let content_w: f32 = table
            .columns
            .iter()
            .map(|c| c.width + tokens::scaled(8.0))
            .sum::<f32>()
            + buttons_w
            + tokens::scaled(24.0);

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
            .border_color(theme::border_hairline());
        for (slot, col) in table.columns.iter().enumerate() {
            header = header.child(
                cell(col)
                    .id(SharedString::from(format!("panel-col-{slot}")))
                    .role(Role::ColumnHeader)
                    .aria_column_index(slot + 1)
                    .child(col.label),
            );
        }
        if has_buttons {
            header = header.child(div().w(px(buttons_w)).flex_none());
        }

        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, row) in table.rows.iter().enumerate() {
            let buttons = self.row_buttons(row);
            let summary: String = table
                .columns
                .iter()
                .zip(&row.cells)
                .map(|(col, cell)| format!("{}: {}", col.label, cell.text))
                .collect::<Vec<_>>()
                .join(", ");
            let mut el = div()
                .id(SharedString::from(row.key.clone()))
                .role(Role::Row)
                .aria_label(SharedString::from(summary))
                .aria_row_index(index + 2)
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(tokens::height::control()))
                .px_3()
                .text_size(px(text))
                .border_b_1()
                .border_color(theme::border_hairline())
                .hover(|s| s.bg(theme::surface_hover()));
            if let Some(symbol) = row.symbol {
                el = el
                    .cursor_pointer()
                    .on_click(cx.listener(move |_this, _, _, cx| {
                        cx.emit(PanelEvent::ShowSymbol(symbol));
                    }));
            }
            for (slot, (col, value)) in table.columns.iter().zip(&row.cells).enumerate() {
                el = el.child(
                    cell(col)
                        .id(SharedString::from(format!("{}-c{slot}", row.key)))
                        .role(Role::Cell)
                        .aria_column_index(slot + 1)
                        .text_color(tone_color(value.tone))
                        .child(value.text.clone()),
                );
            }
            if has_buttons {
                el = el.child(
                    div()
                        .w(px(buttons_w))
                        .flex_none()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_1()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .children(buttons),
                );
            }
            rows.push(el.into_any_element());
        }

        let body = if rows.is_empty() {
            let alerts_empty = match wyck_ui_kit::hints::hint("add_alert") {
                Some(keys) => format!(
                    "No alert. Right click a chart, or press {keys}, to add one at a price."
                ),
                None => "No alert. Right click a chart to add one at a price.".to_owned(),
            };
            let message = match tab {
                Tab::Positions => "No open position.",
                Tab::Orders => "No working order.",
                Tab::History => "No trade closed in the last 7 days.",
                Tab::Exposure => "Nothing is open.",
                Tab::Alerts => alerts_empty.as_str(),
                Tab::AlertLog => "No alert has fired yet.",
            };
            div()
                .flex_1()
                .py_6()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_size(px(text))
                .text_color(theme::muted_fg())
                .child(icon::tinted(
                    match tab {
                        Tab::Alerts | Tab::AlertLog => IconName::Bell,
                        _ => IconName::Inbox,
                    },
                    20.,
                    theme::fg_alpha(0.35),
                ))
                .child(message.to_owned())
                .into_any_element()
        } else {
            div().flex().flex_col().children(rows).into_any_element()
        };

        let totals = (!table.rows.is_empty() && table.totals.iter().any(|(t, _)| !t.is_empty()))
            .then(|| {
                let mut row = div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(tokens::height::compact()))
                    .px_3()
                    .text_size(px(tokens::text::small()))
                    .border_t_1()
                    .border_color(theme::border_subtle());
                for (col, (total, tone)) in table.columns.iter().zip(&table.totals) {
                    row = row.child(cell(col).text_color(tone_color(*tone)).child(total.clone()));
                }
                if has_buttons {
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
