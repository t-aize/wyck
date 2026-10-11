//! How the account panel looks: its header with the tabs and the figures of the account, the bar
//! of filters, the table with its sortable and resizable columns, the totals, the strip of
//! figures over the history, and the menus of a row and of a header.

use std::rc::Rc;

use crate::ui::kit::icon::IconName;
use crate::ui::kit::prelude::{Disableable, Selectable};
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, MouseButton, MouseMoveEvent, Role,
    SharedString, Window, div, px,
};

use super::data::{self, Ctx, PositionRow, Row, Table, Tone};
use super::dialogs::{Target, open_protection};
use super::{AccountPanel, MenuTarget, PanelEvent};
use crate::app::account::{Account, Status};
use crate::app::prefs::panel::{RowDensity, Tab};
use crate::ui::kit::{
    button,
    confirm::confirm,
    menu::{self as popup},
    theme, tokens,
};

/// The width of the strip that grabs a column edge.
const RESIZE_HANDLE: f32 = 9.0;
/// The height of the grip of a column edge while the pointer is over it.
const RESIZE_GRIP_HOVER: f32 = 22.0;
/// The width of the mark at the left of the selected row.
const ROW_MARKER_WIDTH: f32 = 2.0;

/// What a button or a menu entry of a row does.
type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// The width of one button of a row, at the base size of the interface.
const BUTTON_W: f32 = 24.0;

fn tone_color(tone: Tone) -> gpui::Rgba {
    match tone {
        Tone::Plain => theme::fg(),
        Tone::Muted => theme::muted_fg(),
        Tone::Up => theme::chart_up(),
        Tone::Down => theme::chart_down(),
        Tone::Accent => theme::emerald(),
    }
}

/// A tint of a color, for the background of a row.
fn tint(color: gpui::Rgba, alpha: f32) -> gpui::Hsla {
    let mut hsla: gpui::Hsla = color.into();
    hsla.a = alpha;
    hsla
}

/// Asks first, unless the user turned the questions off.
fn ask(on: bool, title: &'static str, text: String, run: impl Fn(&mut App) + 'static) -> Action {
    let run = Rc::new(run);
    Rc::new(move |window, cx| {
        if on {
            let run = run.clone();
            confirm(window, cx, title, text.clone(), move |_, cx| run(cx));
        } else {
            run(cx);
        }
    })
}

/// What can be done to a position, as the button and the menu both do it.
struct PositionActions {
    close: Action,
    half: Action,
    reverse: Action,
    break_even: Action,
    trail: Action,
    edit: Action,
}

fn position_actions(
    account: &Entity<Account>,
    row: &PositionRow,
    confirm: bool,
) -> PositionActions {
    let id = row.id;
    let (a, b, c, d, e, f) = (
        account.clone(),
        account.clone(),
        account.clone(),
        account.clone(),
        account.clone(),
        account.clone(),
    );
    let (half, entry, take_profit, trailing) = (row.half, row.entry, row.take_profit, row.trailing);
    PositionActions {
        close: ask(
            confirm,
            "Close this position?",
            row.describe.clone(),
            move |cx| a.update(cx, |acc, cx| acc.close_position(id, None, cx)),
        ),
        half: ask(
            confirm,
            "Close half of this position?",
            format!("Half of {}", row.describe),
            move |cx| {
                if let Some(half) = half {
                    b.update(cx, |acc, cx| acc.close_position(id, Some(half), cx));
                }
            },
        ),
        reverse: ask(
            confirm,
            "Reverse this position?",
            row.describe.clone(),
            move |cx| c.update(cx, |acc, cx| acc.reverse_position(id, cx)),
        ),
        break_even: Rc::new(move |_, cx| {
            if let Some(entry) = entry {
                d.update(cx, |acc, cx| {
                    acc.protect_position(id, Some(entry), take_profit, cx);
                });
            }
        }),
        trail: Rc::new(move |_, cx| {
            e.update(cx, |acc, cx| acc.trail_position(id, !trailing, cx));
        }),
        edit: Rc::new(move |window, cx| {
            open_protection(f.clone(), Target::Position(id), window, cx);
        }),
    }
}

/// A small button of a row.
fn icon_button(
    id: SharedString,
    icon: IconName,
    tip: &'static str,
    enabled: bool,
    on: bool,
    run: Action,
) -> impl IntoElement {
    button::icon_dense(id, icon, tip)
        .when(!enabled, |button| button.cursor_not_allowed())
        .selected(on)
        .disabled(!enabled)
        .on_click(move |_, window, cx| run(window, cx))
}

/// The text of a row, its shown cells side by side, for the clipboard.
fn row_text(table: &Table, row: &Row) -> String {
    table
        .columns
        .iter()
        .map(|c| format!("{}: {}", c.label, row.cells[c.index].text))
        .collect::<Vec<_>>()
        .join(", ")
}

fn copy(cx: &mut App, title: &'static str, text: String) {
    cx.write_to_clipboard(ClipboardItem::new_string(text));
    crate::ui::kit::toast::show(
        cx,
        crate::ui::kit::toast::Kind::Info,
        title,
        "Copied to the clipboard.",
    );
}

mod header;
mod menus;
mod table;

impl Render for AccountPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let prefs = self.prefs.clone();
        let (positions, orders, status) = {
            let account = self.account.read(cx);
            (
                account.book.positions.len(),
                account.book.orders.len(),
                account.status.clone(),
            )
        };
        let alerts = self
            .alerts
            .read(cx)
            .book()
            .alerts
            .iter()
            .filter(|a| a.active)
            .count();
        let count = |tab: Tab| match tab {
            Tab::Positions => positions,
            Tab::Orders => orders,
            Tab::Alerts => alerts,
            Tab::History | Tab::Exposure | Tab::AlertLog => 0,
        };
        let menu = popup::Menu::new("panel-context-menu", window, cx);
        let bulk = popup::Menu::new("panel-bulk-menu", window, cx);

        // The alerts do not depend on the account, so they show while it loads or failed.
        let ready = status == Status::Ready || matches!(prefs.tab, Tab::Alerts | Tab::AlertLog);
        let table = ready.then(|| {
            let ctx = Ctx {
                account: self.account.read(cx),
                alerts: self.alerts.read(cx),
                prefs: &self.prefs,
                symbol: self.symbol,
                query: &self.query,
            };
            data::build(&ctx, prefs.tab)
        });
        // The rows on screen get their focus handles, and the tab that was left forgets its row.
        let keys: Vec<String> = table
            .as_ref()
            .map(|t| t.rows.iter().map(|r| r.key.clone()).collect())
            .unwrap_or_default();
        self.set_rows(keys, cx);

        let context_items = match (&table, menu.is_open(cx)) {
            (Some(table), true) => self.menu_items(table, cx),
            _ => Vec::new(),
        };
        let bulk_items = if bulk.is_open(cx) {
            self.bulk_items(cx)
        } else {
            Vec::new()
        };
        let toggle_bulk = bulk.clone();

        let body: AnyElement = match (&status, &table) {
            (_, Some(table)) => {
                let mut column = div().flex_1().min_h_0().flex().flex_col();
                if prefs.filters {
                    column = column.child(self.filter_bar(table, window, cx));
                }
                if prefs.tab == Tab::History
                    && let Some(stats) = &table.stats
                {
                    column = column.child(self.history_strip(stats, cx));
                }
                column
                    .child(self.table_view(table, &menu, cx))
                    .into_any_element()
            }
            (Status::Failed(message), None) => div()
                .py_6()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .text_size(px(tokens::text::body()))
                .child(
                    div()
                        .text_color(theme::destructive())
                        .child("Could not read the account"),
                )
                .child(div().text_color(theme::muted_fg()).child(message.clone()))
                .into_any_element(),
            _ => div()
                .py_6()
                .flex()
                .justify_center()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child("Reading the account...")
                .into_any_element(),
        };

        let tab_buttons: Vec<AnyElement> = prefs
            .visible_tabs()
            .map(|tab| self.tab_button(tab, count(tab), cx).into_any_element())
            .collect();
        let this = cx.entity();
        let more_menu = menu.clone();
        let resizing = self.resize.is_some();
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
            .when(resizing, |el| {
                el.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        this.drag_column(f32::from(event.position.x), cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resize = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resize = None;
                            cx.notify();
                        }),
                    )
            })
            .child(
                div()
                    .flex_none()
                    .h(px(tokens::height::large()))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pr_2()
                    .border_b_1()
                    .border_color(theme::border_hairline())
                    .child(
                        div()
                            .id("panel-tabs")
                            .role(Role::TabList)
                            .aria_label("Account panel")
                            .flex_none()
                            .h_full()
                            .flex()
                            .flex_row()
                            .children(tab_buttons),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_3()
                            .children(self.safety_chip(cx))
                            // When the panel is narrow the figures give way from the left, where
                            // the least needed are.
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .flex()
                                    .flex_row()
                                    .justify_end()
                                    .child(self.stats_row(cx)),
                            )
                            .child(
                                div()
                                    .relative()
                                    .child(
                                        crate::ui::kit::button::dense("panel-bulk")
                                            .label("Close")
                                            .icon(IconName::ChevronDown)
                                            .on_click(move |_, _, cx| toggle_bulk.toggle(cx)),
                                    )
                                    .children(bulk.popup(
                                        bulk_items,
                                        popup::Placement::Below(tokens::height::tiny()),
                                        window,
                                        cx,
                                    )),
                            )
                            // One menu for what is about the table: its columns, a copy as CSV,
                            // and the settings of the panel.
                            .child(
                                button::icon_dense(
                                    "panel-more",
                                    IconName::Ellipsis,
                                    "Columns and settings",
                                )
                                .on_click(move |event, _, cx| {
                                    this.update(cx, |p, _| {
                                        p.menu_target = Some(MenuTarget::Header)
                                    });
                                    more_menu.open(Some(event.position()), cx);
                                }),
                            )
                            .child(
                                button::icon_dense(
                                    "panel-hide",
                                    IconName::PanelBottom,
                                    "Hide the panel",
                                )
                                .on_click(cx.listener(
                                    |_this, _, _, cx| {
                                        cx.emit(PanelEvent::Hide);
                                    },
                                )),
                            ),
                    ),
            )
            .child(body)
            .children(menu.popup(context_items, popup::Placement::Cursor, window, cx))
    }
}

/// The height of a row at a density, in pixels.
fn density_height(density: RowDensity) -> f32 {
    match density {
        RowDensity::Compact => tokens::height::compact(),
        RowDensity::Comfortable => tokens::height::control(),
        RowDensity::Roomy => tokens::height::large(),
    }
}

/// The size of the text of a row at a density, in pixels.
fn density_text(density: RowDensity) -> f32 {
    match density {
        RowDensity::Compact => tokens::text::small(),
        RowDensity::Comfortable | RowDensity::Roomy => tokens::text::body(),
    }
}
