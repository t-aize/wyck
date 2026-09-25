//! How the account panel looks: its header with the tabs and the figures of the account, the bar
//! of filters, the table with its sortable and resizable columns, the totals, the strip of
//! figures over the history, and the menus of a row and of a header.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, FontWeight, MouseButton, MouseDownEvent,
    MouseMoveEvent, SharedString, Window, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable, Selectable, Sizable, StyledExt as _};

use super::data::{self, Cell, Ctx, PositionRow, Row, RowKind, Table, Tone};
use super::dialogs::{Target, open_alert, open_protection};
use super::prefs::{HistoryRange, PanelPrefs, RowAction, SideFilter, Stat, Tab};
use super::stats::HistoryStats;
use super::{AccountPanel, MenuTarget, PanelEvent, Resize, customize};
use crate::app::connection::ui;
use crate::app::menu::{self as popup, Entry, Item};
use crate::app::trading::account::{Account, Status};
use crate::app::trading::math::format_money;
use crate::app::trading::ticket::confirm;
use crate::app::trading::ticket::prefs::Slot;
use crate::app::{theme, widgets};

/// What a button or a menu entry of a row does.
type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// The width of one button of a row.
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
    Button::new(id)
        .cursor_pointer()
        .when(!enabled, |button| button.cursor_not_allowed())
        .ghost()
        .xsmall()
        .icon(icon)
        .tooltip(tip)
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
    crate::app::toast::show(
        cx,
        crate::app::toast::Kind::Info,
        title,
        "Copied to the clipboard.",
    );
}

impl AccountPanel {
    /// Makes the search field the first time it is needed.
    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            return;
        }
        let state = cx.new(|cx| InputState::new(window, cx).placeholder("Search symbol, side, id"));
        self._subscriptions
            .push(cx.subscribe(&state, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.query = state.read(cx).value().to_string();
                    cx.notify();
                }
            }));
        self.search = Some(state);
    }

    fn tab_button(&self, tab: Tab, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let chosen = self.prefs.tab == tab;
        let label = if count > 0 {
            format!("{} ({count})", tab.label())
        } else {
            tab.label().to_owned()
        };
        div()
            .id(SharedString::from(format!("panel-tab-{tab:?}")))
            .h_full()
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(12.))
            .font_weight(if chosen {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if chosen {
                theme::fg()
            } else {
                theme::muted_fg()
            })
            .border_b_2()
            .border_color(if chosen {
                theme::accent()
            } else {
                gpui::rgba(0x00000000)
            })
            .hover(|s| s.text_color(theme::fg()))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.edit_prefs(cx, |prefs| prefs.tab = tab);
            }))
            .child(label)
    }

    /// The figures of the account the user chose, in the order chosen.
    fn stats_row(&self, cx: &App) -> AnyElement {
        let account = self.account.read(cx);
        let currency = account.book.currency.clone();
        let summary = account.summary();
        let item = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_1p5()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_semibold()
                        .text_color(color)
                        .child(value),
                )
        };
        let tone = |value: f64| tone_color(Tone::of(value));
        let mut row = div().flex_none().flex().flex_row().items_center().gap_4();
        for stat in self.prefs.visible_stats() {
            row = row.child(match stat {
                Stat::Balance => item(
                    "Balance",
                    format_money(summary.balance, &currency),
                    theme::fg(),
                ),
                Stat::Equity => item(
                    "Equity",
                    format_money(summary.equity, &currency),
                    theme::fg(),
                ),
                Stat::Margin => item(
                    "Margin",
                    format_money(summary.margin, &currency),
                    theme::fg(),
                ),
                Stat::FreeMargin => item(
                    "Free",
                    format_money(summary.free_margin, &currency),
                    theme::fg(),
                ),
                Stat::MarginLevel => item(
                    "Level",
                    summary
                        .margin_level
                        .map_or_else(|| "-".to_owned(), |l| format!("{l:.0}%")),
                    match summary.margin_level {
                        Some(l) if l < 100.0 => theme::chart_down(),
                        Some(l) if l < 300.0 => theme::amber(),
                        _ => theme::fg(),
                    },
                ),
                Stat::Profit => item(
                    "Profit",
                    format_money(summary.unrealized, &currency),
                    tone(summary.unrealized),
                ),
                Stat::ProfitPercent => item(
                    "Profit %",
                    if summary.balance > 0.0 {
                        format!("{:+.2}%", summary.unrealized / summary.balance * 100.0)
                    } else {
                        "-".to_owned()
                    },
                    tone(summary.unrealized),
                ),
            });
        }
        row.into_any_element()
    }

    /// The entries of the menu that closes many positions or cancels the orders.
    fn bulk_items(&self, cx: &App) -> Vec<Item> {
        let confirm_on = self.prefs.confirm_close;
        let account = self.account.read(cx);
        let mut all = Vec::new();
        let mut winners = Vec::new();
        let mut losers = Vec::new();
        let mut buys = Vec::new();
        let mut sells = Vec::new();
        let mut here = Vec::new();
        for position in account.book.positions.values() {
            let id = position.position_id;
            all.push(id);
            match account.net_profit(id) {
                Some(p) if p > 0.0 => winners.push(id),
                Some(p) if p < 0.0 => losers.push(id),
                _ => {}
            }
            if crate::app::trading::book::is_buy(position.trade_data.trade_side) {
                buys.push(id);
            } else {
                sells.push(id);
            }
            if Some(position.trade_data.symbol_id) == self.symbol {
                here.push(id);
            }
        }
        let orders: Vec<i64> = account.book.orders.keys().copied().collect();
        let symbol_name = self
            .symbol
            .map_or_else(|| "this symbol".to_owned(), |s| account.book.name(s));
        let close = |ids: Vec<i64>, label: &'static str, what: String| {
            let account = self.account.clone();
            let enabled = !ids.is_empty();
            let n = ids.len();
            let action = ask(
                confirm_on,
                "Close these positions?",
                format!("{what} ({n})"),
                move |cx| {
                    account.update(cx, |acc, cx| {
                        for id in &ids {
                            acc.close_position(*id, None, cx);
                        }
                    });
                },
            );
            Entry::new(label)
                .icon(IconName::X)
                .hint(n.to_string())
                .disabled(!enabled)
                .on_click(move |window, cx| action(window, cx))
                .into()
        };
        let cancel = {
            let account = self.account.clone();
            let n = orders.len();
            let action = ask(
                confirm_on,
                "Cancel every working order?",
                format!("{n} working orders are cancelled."),
                move |cx| {
                    account.update(cx, |acc, cx| {
                        for id in &orders {
                            acc.cancel_order(*id, cx);
                        }
                    });
                },
            );
            Entry::new("Cancel all orders")
                .icon(IconName::Ban)
                .hint(n.to_string())
                .disabled(n == 0)
                .on_click(move |window, cx| action(window, cx))
        };
        vec![
            close(all, "Close all positions", "Every open position".into()),
            Item::Separator,
            close(
                winners,
                "Close winning positions",
                "Every position in profit".into(),
            ),
            close(
                losers,
                "Close losing positions",
                "Every position in loss".into(),
            ),
            close(buys, "Close buys", "Every buy".into()),
            close(sells, "Close sells", "Every sell".into()),
            close(
                here,
                "Close this symbol",
                format!("Every position on {symbol_name}"),
            ),
            Item::Separator,
            cancel.into(),
        ]
    }

    /// What a right click on a row or a header offers.
    fn menu_items(&self, table: &Table, cx: &mut Context<Self>) -> Vec<Item> {
        let Some(target) = self.menu_target.clone() else {
            return Vec::new();
        };
        let this = cx.entity();
        let tab = self.prefs.tab;
        let confirm_on = self.prefs.confirm_close;
        match target {
            MenuTarget::Header => {
                let mut items = vec![Item::Title("Columns".into())];
                for (slot, (label, shown)) in self.prefs.column_list(tab).into_iter().enumerate() {
                    let this = this.clone();
                    items.push(
                        Entry::new(label)
                            .checked(shown)
                            .keep_open()
                            .on_click(move |_, cx| {
                                this.update(cx, |p, cx| {
                                    p.edit_prefs(cx, |prefs| prefs.toggle_column(tab, slot));
                                });
                            })
                            .into(),
                    );
                }
                let (reset, custom) = (this.clone(), this.clone());
                let text = data::to_csv(table);
                items.push(Item::Separator);
                items.push(
                    Entry::new("Reset the columns")
                        .icon(IconName::RotateCcw)
                        .keep_open()
                        .on_click(move |_, cx| {
                            reset.update(cx, |p, cx| {
                                p.edit_prefs(cx, |prefs| prefs.reset_columns(tab))
                            });
                        })
                        .into(),
                );
                items.push(
                    Entry::new("Copy the table as CSV")
                        .icon(IconName::Copy)
                        .on_click(move |_, cx| copy(cx, "Table copied", text.clone()))
                        .into(),
                );
                items.push(
                    Entry::new("Customize the panel...")
                        .icon(IconName::SlidersHorizontal)
                        .on_click(move |window, cx| customize::open(custom.clone(), window, cx))
                        .into(),
                );
                items
            }
            MenuTarget::Row(key) => {
                let Some(row) = table.rows.iter().find(|r| r.key == key) else {
                    return Vec::new();
                };
                let mut items: Vec<Item> = Vec::new();
                let text = row_text(table, row);
                if let Some(symbol) = row.symbol {
                    let show = this.clone();
                    items.push(
                        Entry::new("Show on the chart")
                            .icon(IconName::ChartCandlestick)
                            .on_click(move |_, cx| {
                                show.update(cx, |_, cx| cx.emit(PanelEvent::ShowSymbol(symbol)));
                            })
                            .into(),
                    );
                    items.push(Item::Separator);
                }
                match &row.kind {
                    RowKind::Position(position) => {
                        let acts = position_actions(&self.account, position, confirm_on);
                        let busy = position.busy;
                        let (edit, half, be, trail, rev, close) = (
                            acts.edit.clone(),
                            acts.half.clone(),
                            acts.break_even.clone(),
                            acts.trail.clone(),
                            acts.reverse.clone(),
                            acts.close.clone(),
                        );
                        items.extend([
                            Entry::new("Modify stop loss and take profit...")
                                .icon(IconName::Pencil)
                                .on_click(move |w, cx| edit(w, cx))
                                .into(),
                            Entry::new("Move the stop loss to the entry")
                                .icon(IconName::ShieldCheck)
                                .disabled(position.entry.is_none() || busy)
                                .on_click(move |w, cx| be(w, cx))
                                .into(),
                            Entry::new(if position.trailing {
                                "Stop trailing the stop loss"
                            } else {
                                "Trail the stop loss"
                            })
                            .icon(IconName::TrendingUp)
                            .disabled((position.stop_loss.is_none() && !position.trailing) || busy)
                            .on_click(move |w, cx| trail(w, cx))
                            .into(),
                            Item::Separator,
                            Entry::new("Close half")
                                .icon(IconName::Scissors)
                                .disabled(position.half.is_none() || busy)
                                .on_click(move |w, cx| half(w, cx))
                                .into(),
                            Entry::new("Reverse")
                                .icon(IconName::ArrowUpDown)
                                .disabled(busy)
                                .on_click(move |w, cx| rev(w, cx))
                                .into(),
                            Entry::new("Close the position")
                                .icon(IconName::X)
                                .danger()
                                .disabled(busy)
                                .on_click(move |w, cx| close(w, cx))
                                .into(),
                        ]);
                    }
                    RowKind::Order { id, busy, .. } => {
                        let id = *id;
                        let (account, edit) = (self.account.clone(), self.account.clone());
                        let cancel = ask(
                            confirm_on,
                            "Cancel this order?",
                            match &row.kind {
                                RowKind::Order { describe, .. } => describe.clone(),
                                _ => String::new(),
                            },
                            move |cx| account.update(cx, |acc, cx| acc.cancel_order(id, cx)),
                        );
                        items.extend([
                            Entry::new("Modify the order...")
                                .icon(IconName::Pencil)
                                .on_click(move |window, cx| {
                                    open_protection(edit.clone(), Target::Order(id), window, cx);
                                })
                                .into(),
                            Entry::new("Cancel the order")
                                .icon(IconName::X)
                                .danger()
                                .disabled(*busy)
                                .on_click(move |w, cx| cancel(w, cx))
                                .into(),
                        ]);
                    }
                    RowKind::Alert { id, active } => {
                        let id = *id;
                        let (edit, toggle, remove) = (
                            self.alerts.clone(),
                            self.alerts.clone(),
                            self.alerts.clone(),
                        );
                        let active = *active;
                        items.extend([
                            Entry::new("Edit the alert...")
                                .icon(IconName::Pencil)
                                .on_click(move |window, cx| {
                                    open_alert(edit.clone(), id, window, cx)
                                })
                                .into(),
                            Entry::new(if active { "Pause" } else { "Watch again" })
                                .icon(if active {
                                    IconName::BellOff
                                } else {
                                    IconName::BellRing
                                })
                                .on_click(move |_, cx| {
                                    toggle.update(cx, |alerts, cx| {
                                        alerts.edit(cx, |book| {
                                            if let Some(alert) = book.get_mut(id) {
                                                alert.active = !alert.active;
                                                alert.fired_at = None;
                                            }
                                        });
                                    });
                                })
                                .into(),
                            Entry::new("Delete the alert")
                                .icon(IconName::Trash)
                                .danger()
                                .on_click(move |_, cx| {
                                    remove.update(cx, |alerts, cx| {
                                        alerts.edit(cx, |book| book.remove(id));
                                    });
                                })
                                .into(),
                        ]);
                    }
                    RowKind::Exposure => {
                        if let Some(symbol) = row.symbol {
                            let account = self.account.clone();
                            let name = self.account.read(cx).book.name(symbol);
                            let action = ask(
                                confirm_on,
                                "Close every position on this symbol?",
                                format!("Every position on {name} is closed at the market."),
                                move |cx| {
                                    account.update(cx, |acc, cx| acc.close_all(Some(symbol), cx));
                                },
                            );
                            items.push(
                                Entry::new("Close every position on the symbol")
                                    .icon(IconName::X)
                                    .danger()
                                    .on_click(move |w, cx| action(w, cx))
                                    .into(),
                            );
                        }
                    }
                    RowKind::Deal => {}
                }
                if !matches!(items.last(), Some(Item::Separator) | None) {
                    items.push(Item::Separator);
                }
                let copy_row = text;
                items.push(
                    Entry::new("Copy the row")
                        .icon(IconName::Copy)
                        .on_click(move |_, cx| copy(cx, "Row copied", copy_row.clone()))
                        .into(),
                );
                items
            }
        }
    }

    /// The bar of the search and the filters.
    fn filter_bar(&self, table: &Table, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let prefs = &self.prefs;
        let tab = prefs.tab;
        let this = cx.entity();
        let symbol_name = self
            .symbol
            .map(|s| self.account.read(cx).book.name(s))
            .unwrap_or_default();
        let chip = |id: &'static str, label: String, on: bool, run: Action| {
            div()
                .id(id)
                .flex()
                .items_center()
                .h(px(22.))
                .px_2()
                .rounded_md()
                .border_1()
                .cursor_pointer()
                .text_size(px(11.))
                .border_color(if on {
                    theme::accent()
                } else {
                    theme::border_subtle()
                })
                .text_color(if on { theme::fg() } else { theme::muted_fg() })
                .when(on, |el| el.bg(theme::accent_selected()))
                .hover(|s| s.bg(theme::surface_hover()))
                .on_click(move |_, window, cx| run(window, cx))
                .child(label)
        };
        let only = {
            let this = this.clone();
            let on = prefs.only_symbol;
            chip(
                "panel-only-symbol",
                if symbol_name.is_empty() {
                    "This symbol".to_owned()
                } else {
                    symbol_name
                },
                on,
                Rc::new(move |_, cx| {
                    this.update(cx, |p, cx| {
                        p.edit_prefs(cx, |prefs| prefs.only_symbol = !on)
                    });
                }),
            )
        };
        let sides: Vec<&str> = SideFilter::ALL.iter().map(|s| s.label()).collect();
        let side_index = SideFilter::ALL
            .iter()
            .position(|s| *s == prefs.side)
            .unwrap_or(0);
        let side = {
            let this = this.clone();
            widgets::segmented("panel-side", &sides, side_index, move |index, _, cx| {
                this.update(cx, |p, cx| {
                    p.edit_prefs(cx, |prefs| prefs.side = SideFilter::ALL[index]);
                });
            })
        };
        let ranges: Vec<&str> = HistoryRange::ALL.iter().map(|r| r.label()).collect();
        let range_index = HistoryRange::ALL
            .iter()
            .position(|r| *r == prefs.history_range)
            .unwrap_or(0);
        let range = {
            let this = this.clone();
            widgets::segmented("panel-range", &ranges, range_index, move |index, _, cx| {
                this.update(cx, |p, cx| {
                    p.edit_prefs(cx, |prefs| prefs.history_range = HistoryRange::ALL[index]);
                });
            })
        };
        let csv = data::to_csv(table);
        let shown = table.rows.len();
        let _ = window;
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(32.))
            .px_3()
            .border_b_1()
            .border_color(theme::border_hairline())
            .children(self.search.as_ref().map(|search| {
                div()
                    .w(px(200.))
                    .child(Input::new(search).small().cleanable(true))
            }))
            .child(only)
            .when(
                matches!(tab, Tab::Positions | Tab::Orders | Tab::History),
                |el| el.child(side),
            )
            .when(tab == Tab::History, |el| el.child(range))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(if shown == table.unfiltered {
                        format!("{shown} rows")
                    } else {
                        format!("{shown} of {} rows", table.unfiltered)
                    }),
            )
            .child(
                Button::new("panel-copy-csv")
                    .cursor_pointer()
                    .ghost()
                    .xsmall()
                    .icon(IconName::Copy)
                    .tooltip("Copy the table as CSV")
                    .on_click(move |_, _, cx| copy(cx, "Table copied", csv.clone())),
            )
            .into_any_element()
    }

    /// The figures of the closed trades, over the history.
    fn history_strip(&self, stats: &HistoryStats, cx: &App) -> AnyElement {
        let currency = self.account.read(cx).book.currency.clone();
        let item = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_1p5()
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_semibold()
                        .text_color(color)
                        .child(value),
                )
        };
        let money = |v: f64| format_money(v, &currency);
        let dash = || "-".to_owned();
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .h(px(28.))
            .px_3()
            .overflow_hidden()
            .border_b_1()
            .border_color(theme::border_hairline())
            .bg(theme::fg_alpha(0.02))
            .child(item("Trades", stats.closed.to_string(), theme::fg()))
            .child(item(
                "Net",
                money(stats.total),
                tone_color(Tone::of(stats.total)),
            ))
            .child(item(
                "Win rate",
                stats.win_rate().map_or_else(dash, |r| format!("{r:.0}%")),
                theme::fg(),
            ))
            .child(item(
                "Profit factor",
                stats
                    .profit_factor()
                    .map_or_else(dash, |f| format!("{f:.2}")),
                theme::fg(),
            ))
            .child(item(
                "Average win",
                stats.average_win().map_or_else(dash, money),
                theme::chart_up(),
            ))
            .child(item(
                "Average loss",
                stats.average_loss().map_or_else(dash, money),
                theme::chart_down(),
            ))
            .child(item("Best", money(stats.best), theme::chart_up()))
            .child(item("Worst", money(stats.worst), theme::chart_down()))
            .child(item(
                "Per trade",
                stats.expectancy().map_or_else(dash, money),
                theme::fg(),
            ))
            .into_any_element()
    }

    /// The buttons at the end of a row, by what the row is.
    fn row_buttons(&self, row: &Row) -> (Vec<AnyElement>, f32) {
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
            RowKind::Deal | RowKind::Exposure => {}
        }
        let width = buttons.len() as f32 * BUTTON_W;
        (buttons, width)
    }

    /// The table: its header, its rows and its totals.
    fn table_view(&self, table: &Table, menu: &popup::Menu, cx: &mut Context<Self>) -> AnyElement {
        let prefs: &PanelPrefs = &self.prefs;
        let tab = table.tab;
        let height = prefs.density.height();
        let text = prefs.density.text();
        // The width the buttons of the rows take, from the first row that has some.
        let buttons_w = table
            .rows
            .first()
            .map_or(0.0, |row| self.row_buttons(row).1);
        let content_w: f32 =
            table.columns.iter().map(|c| c.width + 8.0).sum::<f32>() + buttons_w + 24.0;

        let header_menu = menu.clone();
        let this = cx.entity();
        let header_this = this.clone();
        let mut header = div()
            .id("panel-header")
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(26.))
            .px_3()
            .text_size(px(11.))
            .text_color(theme::muted_fg())
            .border_b_1()
            .border_color(theme::border_hairline())
            .on_mouse_down(MouseButton::Right, move |event, _, cx| {
                header_this.update(cx, |p, _| p.menu_target = Some(MenuTarget::Header));
                header_menu.open(Some(event.position), cx);
            });
        for col in &table.columns {
            let (slot, start_width) = (col.slot, col.width);
            header = header.child(
                div()
                    .id(SharedString::from(format!("panel-col-{}", col.slot)))
                    .relative()
                    .w(px(col.width))
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
                        ui::icon_colored(
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
                            .right(px(-9.))
                            .h_full()
                            .w(px(9.))
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
                                    .group_hover(group, |s| s.bg(theme::accent()).h(px(22.))),
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
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, row) in table.rows.iter().enumerate() {
            let (buttons, _) = self.row_buttons(row);
            let (row_menu, row_this, key) = (menu.clone(), this.clone(), row.key.clone());
            let symbol = row.symbol;
            let active_symbol = symbol.is_some() && symbol == self.symbol;
            let mut el = div()
                .id(SharedString::from(row.key.clone()))
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
                        .w(px(2.))
                        .bg(theme::accent()),
                );
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
                        .w(px(col.width))
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
                        .gap_0p5()
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
                }
            };
            div()
                .py_6()
                .flex()
                .justify_center()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
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
                .h(px(26.))
                .px_3()
                .text_size(px(text))
                .font_semibold()
                .border_t_1()
                .border_color(theme::border_subtle())
                .bg(theme::fg_alpha(0.03));
            for (col, total) in table.columns.iter().zip(&table.totals) {
                row = row.child(
                    div()
                        .w(px(col.width))
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
            Tab::History | Tab::Exposure => 0,
        };
        let menu = popup::Menu::new("panel-context-menu", window, cx);
        let bulk = popup::Menu::new("panel-bulk-menu", window, cx);

        // The alerts do not depend on the account, so they show while it loads or failed.
        let ready = status == Status::Ready || prefs.tab == Tab::Alerts;
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
                .text_size(px(12.))
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
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child("Reading the account...")
                .into_any_element(),
        };

        let tab_buttons: Vec<AnyElement> = prefs
            .visible_tabs()
            .map(|tab| self.tab_button(tab, count(tab), cx).into_any_element())
            .collect();
        let this = cx.entity();
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
                    .h(px(36.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .pr_2()
                    .border_b_1()
                    .border_color(theme::border_hairline())
                    .child(
                        div()
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
                                        Button::new("panel-bulk")
                                            .cursor_pointer()
                                            .ghost()
                                            .xsmall()
                                            .label("Close")
                                            .icon(IconName::ChevronDown)
                                            .on_click(move |_, _, cx| toggle_bulk.toggle(cx)),
                                    )
                                    .children(bulk.popup(
                                        bulk_items,
                                        popup::Placement::Below(26.),
                                        window,
                                        cx,
                                    )),
                            )
                            .child(
                                Button::new("panel-customize")
                                    .cursor_pointer()
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::SlidersHorizontal)
                                    .tooltip("Customize the panel")
                                    .on_click(move |_, window, cx| {
                                        customize::open(this.clone(), window, cx);
                                    }),
                            )
                            .child(
                                Button::new("panel-hide")
                                    .cursor_pointer()
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::ChevronDown)
                                    .tooltip("Hide the panel")
                                    .on_click(cx.listener(|_this, _, _, cx| {
                                        cx.emit(PanelEvent::Hide);
                                    })),
                            ),
                    ),
            )
            .child(body)
            .children(menu.popup(context_items, popup::Placement::Cursor, window, cx))
    }
}
