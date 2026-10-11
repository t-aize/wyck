//! The menus of a row, of a header and of a selection.

use super::{ask, copy, position_actions, row_text};
use crate::app::prefs::panel::Tab;
use crate::ui::features::trading::panel::data::{self, RowKind, Table};
use crate::ui::features::trading::panel::dialogs::{Target, open_alert, open_protection};
use crate::ui::features::trading::panel::{
    AccountPanel, MenuTarget, NewAlert, PanelEvent, customize,
};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::menu::{self as popup, Entry, Item};
use gpui::{App, Context};

impl AccountPanel {
    /// The entries of the menu that closes many positions or cancels the orders.
    pub(super) fn bulk_items(&self, cx: &App) -> Vec<Item> {
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
            if position.trade_data.is_buy() {
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
        popup::sections([
            popup::Section::new(
                "Positions",
                vec![close(
                    all,
                    "Close all positions",
                    "Every open position".into(),
                )],
            ),
            popup::Section::new(
                "Close some",
                vec![
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
                ],
            ),
            popup::Section::new("Orders", vec![cancel.into()]),
        ])
    }

    /// What a right click on a row or a header offers.
    pub(super) fn menu_items(&self, table: &Table, cx: &mut Context<Self>) -> Vec<Item> {
        let Some(target) = self.menu_target.clone() else {
            return Vec::new();
        };
        let this = cx.entity();
        let tab = self.prefs.tab;
        let confirm_on = self.prefs.confirm_close;
        match target {
            MenuTarget::Header => {
                let mut columns = Vec::new();
                for (slot, (label, shown)) in self.prefs.column_list(tab).into_iter().enumerate() {
                    let this = this.clone();
                    columns.push(
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
                let mut log = Vec::new();
                if tab == Tab::AlertLog {
                    let alerts = self.alerts.clone();
                    let clear = ask(
                        confirm_on,
                        "Clear the alert log?",
                        "Every recorded time an alert fired is removed. The alerts stay."
                            .to_owned(),
                        move |cx| {
                            alerts.update(cx, |alerts, cx| {
                                alerts.edit(cx, |book| book.history.clear())
                            });
                        },
                    );
                    log.push(
                        Entry::new("Clear the log")
                            .icon(IconName::Trash)
                            .danger()
                            .on_click(move |w, cx| clear(w, cx))
                            .into(),
                    );
                }
                let mut table_items = Vec::new();
                table_items.push(
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
                table_items.push(
                    Entry::new("Copy the table as CSV")
                        .icon(IconName::Copy)
                        .on_click(move |_, cx| copy(cx, "Table copied", text.clone()))
                        .into(),
                );
                table_items.push(
                    Entry::new("Customize the panel...")
                        .icon(IconName::SlidersHorizontal)
                        .on_click(move |window, cx| customize::open(custom.clone(), window, cx))
                        .into(),
                );
                popup::sections([
                    popup::Section::new("Columns", columns),
                    popup::Section::new("Alert log", log),
                    popup::Section::new("Table", table_items),
                ])
            }
            MenuTarget::Row(key) => {
                let Some(row) = table.rows.iter().find(|r| r.key == key) else {
                    return Vec::new();
                };
                let mut groups: Vec<popup::Section> = Vec::new();
                let text = row_text(table, row);
                if let Some(symbol) = row.symbol {
                    let show = this.clone();
                    groups.push(popup::Section::untitled(vec![
                        Entry::new("Show on the chart")
                            .icon(IconName::ChartCandlestick)
                            .on_click(move |_, cx| {
                                show.update(cx, |_, cx| cx.emit(PanelEvent::ShowSymbol(symbol)));
                            })
                            .into(),
                    ]));
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
                        let alert = this.clone();
                        let (position_id, symbol_id) = (position.id, row.symbol.unwrap_or(0));
                        groups.push(popup::Section::untitled(vec![
                            Entry::new("Modify stop loss and take profit...")
                                .icon(IconName::Pencil)
                                .on_click(move |w, cx| edit(w, cx))
                                .into(),
                            Entry::new("Alert on its profit...")
                                .icon(IconName::BellRing)
                                .on_click(move |_, cx| {
                                    alert.update(cx, |_, cx| {
                                        cx.emit(PanelEvent::NewAlert(NewAlert::Position {
                                            id: position_id,
                                            symbol_id,
                                        }));
                                    });
                                })
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
                        ]));
                        groups.push(popup::Section::untitled(vec![
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
                        ]));
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
                        groups.push(popup::Section::untitled(vec![
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
                        ]));
                    }
                    RowKind::Alert { id, active } => {
                        let id = *id;
                        let (edit, toggle, remove, snooze, copy) = (
                            self.alerts.clone(),
                            self.alerts.clone(),
                            self.alerts.clone(),
                            self.alerts.clone(),
                            self.alerts.clone(),
                        );
                        let active = *active;
                        groups.push(popup::Section::untitled(vec![
                            Entry::new("Snooze for an hour")
                                .icon(IconName::Timer)
                                .disabled(!active)
                                .on_click(move |_, cx| {
                                    snooze.update(cx, |alerts, cx| {
                                        alerts.snooze(
                                            id,
                                            crate::app::market_data::now_ms() + 3_600_000,
                                            cx,
                                        )
                                    });
                                })
                                .into(),
                            Entry::new("Duplicate")
                                .icon(IconName::Copy)
                                .on_click(move |_, cx| {
                                    copy.update(cx, |alerts, cx| {
                                        alerts.edit(cx, |book| {
                                            if let Some(mut alert) = book.get(id).cloned() {
                                                alert.active = true;
                                                alert.fired_at = None;
                                                alert.fired_count = 0;
                                                alert.snoozed_until = None;
                                                alert.bar_key = None;
                                                alert.created_at =
                                                    crate::app::market_data::now_ms();
                                                book.insert(
                                                    alert,
                                                    crate::app::workspace::MAX_SAVED_ALERTS,
                                                );
                                            }
                                        });
                                    });
                                })
                                .into(),
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
                        ]));
                        groups.push(popup::Section::untitled(vec![
                            Entry::new("Delete the alert")
                                .icon(IconName::Trash)
                                .danger()
                                .on_click(move |_, cx| {
                                    remove.update(cx, |alerts, cx| {
                                        alerts.edit(cx, |book| book.remove(id));
                                    });
                                })
                                .into(),
                        ]));
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
                            groups.push(popup::Section::untitled(vec![
                                Entry::new("Close every position on the symbol")
                                    .icon(IconName::X)
                                    .danger()
                                    .on_click(move |w, cx| action(w, cx))
                                    .into(),
                            ]));
                        }
                    }
                    RowKind::Firing { alert } => {
                        let alert = *alert;
                        if self.alerts.read(cx).book().get(alert).is_some() {
                            let edit = self.alerts.clone();
                            groups.push(popup::Section::untitled(vec![
                                Entry::new("Open the alert...")
                                    .icon(IconName::Pencil)
                                    .on_click(move |window, cx| {
                                        open_alert(edit.clone(), alert, window, cx)
                                    })
                                    .into(),
                            ]));
                        }
                    }
                    RowKind::Deal => {}
                }
                let copy_row = text;
                groups.push(popup::Section::untitled(vec![
                    Entry::new("Copy the row")
                        .icon(IconName::Copy)
                        .on_click(move |_, cx| copy(cx, "Row copied", copy_row.clone()))
                        .into(),
                ]));
                popup::sections(groups)
            }
        }
    }
}
