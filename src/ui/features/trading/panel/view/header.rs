//! The header of the panel: the tabs, the figures, the filters and the strip over the history.

use super::{Action, tone_color};
use crate::app::prefs::panel::{HistoryRange, SideFilter, Stat, Tab};
use crate::app::prefs::ticket::Slot;
use crate::domain::trading::math::format_money;
use crate::ui::features::trading::panel::data::{Table, Tone};
use crate::ui::features::trading::panel::stats::HistoryStats;
use crate::ui::features::trading::panel::{AccountPanel, NewAlert, PanelEvent};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::{InputEvent, InputState};
use crate::ui::kit::prelude::StyledExt as _;
use crate::ui::kit::{button, controls, focus::Keyboard, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, FontWeight, Role, SharedString, Window, div, px};
use std::rc::Rc;

impl AccountPanel {
    /// Makes the search field the first time it is needed.
    pub(super) fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn tab_button(
        &self,
        tab: Tab,
        count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chosen = self.prefs.tab == tab;
        let label = if count > 0 {
            format!("{} ({count})", tab.label())
        } else {
            tab.label().to_owned()
        };
        div()
            .id(SharedString::from(format!("panel-tab-{tab:?}")))
            .keyboard()
            .role(Role::Tab)
            .aria_label(SharedString::from(label.clone()))
            .aria_selected(chosen)
            .h_full()
            .px_3()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_size(px(tokens::text::body()))
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
    /// A chip in the header when the safety limits hold something back: locked, no answer from the
    /// server, or the kill switch.
    pub(super) fn safety_chip(&self, cx: &App) -> Option<AnyElement> {
        let account = self.account.read(cx);
        let (text, color, back) = if let Some(lock) = account.lock() {
            let text = match lock {
                crate::domain::trading::guard::Lock::KillSwitch => "KILL SWITCH",
                crate::domain::trading::guard::Lock::DailyLoss { .. } => "DAILY LOSS LIMIT",
                crate::domain::trading::guard::Lock::Cooldown { .. } => "COOLING DOWN",
            };
            (text, theme::destructive(), theme::destructive_bg())
        } else if account.is_uncertain() {
            ("CHECK ACCOUNT", theme::amber(), theme::amber_bg())
        } else {
            return None;
        };
        Some(
            div()
                .flex_none()
                .px_2()
                .py_1()
                .rounded_full()
                .bg(back)
                .text_size(px(tokens::text::caption()))
                .font_semibold()
                .text_color(color)
                .child(text)
                .into_any_element(),
        )
    }

    pub(super) fn stats_row(&self, cx: &App) -> AnyElement {
        let account = self.account.read(cx);
        let currency = account.book.currency.clone();
        let summary = account.summary();
        let item = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_2()
                .child(
                    div()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(tokens::text::body()))
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

    /// The bar of the search and the filters.
    pub(super) fn filter_bar(
        &self,
        table: &Table,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let prefs = &self.prefs;
        let tab = prefs.tab;
        let this = cx.entity();
        let symbol_name = self
            .symbol
            .map(|s| self.account.read(cx).book.name(s))
            .unwrap_or_default();
        let chip = |id: &'static str, label: String, on: bool, run: Action| {
            controls::chip(id, on)
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
            controls::segmented("panel-side", &sides, side_index, move |index, _, cx| {
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
            controls::segmented("panel-range", &ranges, range_index, move |index, _, cx| {
                this.update(cx, |p, cx| {
                    p.edit_prefs(cx, |prefs| prefs.history_range = HistoryRange::ALL[index]);
                });
            })
        };
        let shown = table.rows.len();
        let _ = window;
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(tokens::height::large()))
            .px_3()
            .border_b_1()
            .border_color(theme::border_hairline())
            .children(self.search.as_ref().map(|search| {
                div()
                    .w(px(tokens::field::text()))
                    .child(crate::ui::kit::input::text(search).cleanable(true))
            }))
            .child(only)
            .when(
                matches!(tab, Tab::Positions | Tab::Orders | Tab::History),
                |el| el.child(side),
            )
            .when(tab == Tab::History, |el| el.child(range))
            .child(div().flex_1())
            .when(tab == Tab::Alerts, |el| {
                let (spread, profit) = (this.clone(), this.clone());
                el.child(button::action(
                    "panel-new-spread-alert",
                    "Spread alert",
                    Some(IconName::Plus),
                    false,
                    move |_, cx| {
                        spread.update(cx, |_, cx| cx.emit(PanelEvent::NewAlert(NewAlert::Spread)));
                    },
                ))
                .child(button::action(
                    "panel-new-profit-alert",
                    "Profit alert",
                    Some(IconName::Plus),
                    false,
                    move |_, cx| {
                        profit.update(cx, |_, cx| cx.emit(PanelEvent::NewAlert(NewAlert::Profit)));
                    },
                ))
            })
            .child(
                div()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child(if shown == table.unfiltered {
                        format!("{shown} rows")
                    } else {
                        format!("{shown} of {} rows", table.unfiltered)
                    }),
            )
            .into_any_element()
    }

    /// The figures of the closed trades, over the history.
    pub(super) fn history_strip(&self, stats: &HistoryStats, cx: &App) -> AnyElement {
        let currency = self.account.read(cx).book.currency.clone();
        let item = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_2()
                .child(
                    div()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(tokens::text::body()))
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
            .h(px(tokens::height::control()))
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
}
