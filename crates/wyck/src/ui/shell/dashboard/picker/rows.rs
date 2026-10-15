//! The class chips over the list of the symbol picker and one row of the list.

use gpui::prelude::*;
use gpui::{Context, FontWeight, MouseMoveEvent, Stateful, div, px};

use super::{RESULT_COLUMN_WIDTH, ROW_HEIGHT};
use crate::ui::shell::dashboard::catalog::{Class, Entry};
use crate::ui::shell::dashboard::{Dashboard, lists, marks};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::{icon, theme, tokens};

/// A filter chip: one asset class, or `None` for all of them.
pub(super) fn class_chip(
    class: Option<Class>,
    selected: bool,
    cx: &mut Context<Dashboard>,
) -> Stateful<gpui::Div> {
    let label = class.map_or("All", Class::label);
    div()
        .id(("class-chip", class.map_or(0, |c| c as u64 + 1)))
        .px_3()
        .py_1()
        .rounded_full()
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .bg(if selected {
            theme::accent_selected()
        } else {
            theme::bg()
        })
        .text_size(px(tokens::text::body()))
        .text_color(if selected {
            theme::fg()
        } else {
            theme::muted_fg()
        })
        .cursor_pointer()
        .when(!selected, |el| {
            el.hover(|style| style.bg(theme::surface_hover()).text_color(theme::fg()))
        })
        .on_click(cx.listener(move |this, _event, _window, cx| {
            if let Some(picker) = this.picker.as_mut() {
                picker.class = class;
                cx.notify();
            }
        }))
        .child(label)
}

pub(super) fn symbol_row(
    row: usize,
    entry: &Entry,
    highlighted: bool,
    active: bool,
    favorite: bool,
    cx: &mut Context<Dashboard>,
) -> Stateful<gpui::Div> {
    let star = lists::star(row, favorite, cx, entry.name.clone().into());
    let main = div()
        .id(("symbol-row", row as u64))
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .active(|style| style.bg(theme::surface_pressed()))
        .on_mouse_move(
            cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                if let Some(picker) = this.picker.as_mut()
                    && picker.highlighted != row
                {
                    picker.highlighted = row;
                    cx.notify();
                }
            }),
        )
        .on_click(cx.listener(move |this, _event, window, cx| this.choose(row, window, cx)))
        .child(marks::render(
            &entry.icon,
            38.,
            if highlighted {
                theme::surface_hover()
            } else {
                theme::surface()
            },
        ))
        .child(
            div()
                .w(px(RESULT_COLUMN_WIDTH))
                .flex_none()
                .truncate()
                .text_size(px(tokens::text::title()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::fg())
                .child(entry.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(entry.description.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg())
                .child(
                    entry
                        .category
                        .clone()
                        .unwrap_or_else(|| entry.class.label().to_owned()),
                ),
        )
        .child(
            div()
                .w(px(wyck_ui_kit::tokens::space::lg()))
                .flex_none()
                .children(active.then(|| icon::tinted(IconName::Check, 16., theme::accent()))),
        );
    div()
        .id(("symbol-line", row as u64))
        .w_full()
        .h(px(ROW_HEIGHT))
        .pl_4()
        .pr_2()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .when(highlighted, |el| el.bg(theme::surface_hover()))
        .child(main)
        .child(star)
}
