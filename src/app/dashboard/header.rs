//! The bar across the top of the dashboard, and the account menu that hangs from it.

use gpui::prelude::*;
use gpui::{Context, FontWeight, MouseButton, SharedString, Window, anchored, deferred, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Selectable;
use gpui_kit::component::button::{Button, ButtonVariants};

use super::marks;
use super::{Conn, Dashboard, DashboardEvent, Tick};
use crate::app::connection::ui;
use crate::app::{anim, chart, theme, trading};

/// Which parts of the bar the window is wide enough for.
#[derive(Clone, Copy)]
struct Fit {
    status_text: bool,
    spread: bool,
    equity: bool,
    ask: bool,
}

impl Dashboard {
    pub(super) fn render_header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // What the bar leaves out as the window narrows, least needed first, so what stays
        // never runs off its right end.
        let width = f32::from(window.viewport_size().width);
        let fit = Fit {
            status_text: width >= 1_460.0,
            spread: width >= 1_360.0,
            equity: width >= 1_260.0,
            ask: width >= 1_150.0,
        };
        div()
            .flex_none()
            .w_full()
            .h(px(54.))
            .px_5()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(theme::border_hairline())
            .child(self.symbol_button(cx))
            .child(self.price_block(fit))
            .child(div().flex_1().min_w_0())
            .child(self.layout_button(window, cx))
            .child(self.timeframe_strip(cx))
            .child(self.account_block(fit, cx))
            .child(self.status_block(fit))
            .child(self.controls(window, cx))
    }

    /// The symbol: its tile, ticker and name. It is a button that opens the picker.
    fn symbol_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (tile, title, subtitle) = match &self.active {
            Some(active) => (
                marks::render(&active.entry.icon, 34., theme::bg()),
                active.entry.name.clone(),
                active.entry.description.clone(),
            ),
            None => (
                ui::icon_tile(
                    IconName::Search,
                    34.,
                    18.,
                    theme::surface(),
                    theme::muted_fg(),
                )
                .into_any_element(),
                match &self.catalog {
                    super::Load::Loading => "Loading symbols...".to_owned(),
                    super::Load::Failed(_) => "Symbols unavailable".to_owned(),
                    super::Load::Ready(_) => "Select a symbol".to_owned(),
                },
                match &self.catalog {
                    super::Load::Failed(message) => format!("{message}. Click to try again."),
                    _ => String::new(),
                },
            ),
        };

        div()
            .id("symbol-button")
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .py_1()
            .pl_1p5()
            .pr_3()
            .rounded_lg()
            .cursor_pointer()
            .hover(|style| style.bg(theme::surface_hover()))
            .active(|style| style.bg(theme::surface_pressed()))
            .on_click(cx.listener(|this, _event, window, cx| this.open_picker(window, cx)))
            .child(tile)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg())
                            .child(title),
                    )
                    .when(!subtitle.is_empty(), |el| {
                        el.child(
                            div()
                                .max_w(px(170.))
                                .truncate()
                                .text_size(px(11.))
                                .text_color(theme::muted_fg())
                                .child(subtitle),
                        )
                    }),
            )
            .child(ui::icon_colored(
                IconName::ChevronsUpDown,
                15.,
                theme::muted_fg(),
            ))
    }

    /// The bid, the direction of its last move, the ask and the spread.
    fn price_block(&self, fit: Fit) -> impl IntoElement {
        let block = div().flex_none().flex().flex_row().items_center().gap_3();
        let Some((bid, ask, spread)) = self.price_text() else {
            return block.child(
                div()
                    .text_size(px(13.))
                    .text_color(theme::muted_fg())
                    .child(if self.active.is_some() {
                        "Waiting for a price..."
                    } else {
                        ""
                    }),
            );
        };
        let tone = match self.tick {
            Some(Tick::Up) => theme::emerald(),
            Some(Tick::Down) => theme::destructive(),
            None => theme::fg(),
        };
        let chip = |label: &'static str, value: String| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .px_2p5()
                .py_1()
                .rounded_md()
                .bg(theme::surface())
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(label)
                .child(div().text_color(theme::fg()).child(value))
        };

        block
            .child(
                div()
                    .text_size(px(19.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(tone)
                    .child(bid),
            )
            .children(self.tick.map(|tick| {
                ui::icon_colored(
                    match tick {
                        Tick::Up => IconName::ArrowUp,
                        Tick::Down => IconName::ArrowDown,
                    },
                    15.,
                    tone,
                )
            }))
            .children(ask.filter(|_| fit.ask).map(|ask| chip("Ask", ask)))
            .children(
                spread
                    .filter(|_| fit.spread)
                    .map(|pips| chip("Spread", format!("{pips} pips"))),
            )
    }

    /// The favorite timeframes as buttons, and a button that opens all of them: ticks, seconds,
    /// minutes, hours and days.
    fn timeframe_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.multi.read(cx).active_timeframe(cx);
        let favorites = self.workspace.read(cx).preferences().favorites();
        let mut quick = Vec::new();
        for timeframe in favorites.iter().copied() {
            quick.push(timeframe_chip("tf", timeframe, current == timeframe, cx));
        }

        // When the chart is on a timeframe that has no button of its own, the menu button says which.
        let in_quick = favorites.contains(&current);
        let more = div()
            .id("tf-more")
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(px(28.))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(if in_quick {
                theme::muted_fg()
            } else {
                theme::fg()
            })
            .when(!in_quick || self.tf_menu_open, |el| {
                el.bg(theme::accent_selected())
            })
            .hover(|style| style.bg(theme::surface_hover()))
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.tf_menu_open = !this.tf_menu_open;
                this.layout_menu_open = false;
                cx.notify();
            }))
            .when(!in_quick, |el| el.child(current.label()))
            .child(ui::icon_colored(
                IconName::ChevronDown,
                14.,
                theme::muted_fg(),
            ));

        div()
            .relative()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_0p5()
            .children(quick)
            .child(more)
            .children(self.tf_menu_open.then(|| self.timeframe_menu(current, cx)))
    }

    fn pick_timeframe(&mut self, timeframe: chart::Timeframe, cx: &mut Context<Self>) {
        self.tf_menu_open = false;
        self.multi
            .update(cx, |multi, cx| multi.set_timeframe(timeframe, cx));
        cx.notify();
    }

    fn toggle_favorite_timeframe(&mut self, timeframe: chart::Timeframe, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.toggle_favorite_timeframe(timeframe));
        });
    }

    /// Every timeframe, in sections, under the strip. A star keeps one in the header.
    fn timeframe_menu(
        &self,
        current: chart::Timeframe,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let prefs = self.workspace.read(cx).preferences().clone();
        let mut card = div()
            .w(px(320.))
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .occlude();
        for (title, items) in chart::GROUPS {
            let mut chips = Vec::new();
            for timeframe in items.iter().copied() {
                let favorite = prefs.is_favorite_timeframe(timeframe);
                chips.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .child(timeframe_chip(
                            "tf-menu",
                            timeframe,
                            current == timeframe,
                            cx,
                        ))
                        .child(
                            div()
                                .id(SharedString::from(format!("tf-star-{}", timeframe.code())))
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(22.))
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::surface_hover()))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.toggle_favorite_timeframe(timeframe, cx);
                                }))
                                .child(ui::icon_colored(
                                    IconName::Star,
                                    13.,
                                    if favorite {
                                        theme::amber()
                                    } else {
                                        theme::muted_fg()
                                    },
                                )),
                        ),
                );
            }
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::muted_fg())
                            .child(title),
                    )
                    .child(div().flex().flex_row().flex_wrap().gap_1().children(chips)),
            );
        }
        card = card.child(
            div()
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child("Star a timeframe to keep it in the header."),
        );
        deferred(
            anchored()
                .anchor(gpui::Anchor::TopLeft)
                .offset(gpui::point(px(0.), px(0.)))
                .snap_to_window_with_margin(px(8.))
                .child(
                    div()
                        .pt(px(40.))
                        .child(anim::enter(card, "timeframe-menu", 0)),
                ),
        )
        .with_priority(1)
    }

    /// The equity and the open profit of the account, and the switches of the account panel
    /// and the order ticket.
    fn account_block(&self, fit: Fit, cx: &mut Context<Self>) -> impl IntoElement {
        let account = self.trading.read(cx);
        let ready = account.status == trading::account::Status::Ready;
        let summary = account.summary();
        let currency = account.book.currency.clone();
        let profit_color = if summary.unrealized > 0.0 {
            theme::chart_up()
        } else if summary.unrealized < 0.0 {
            theme::chart_down()
        } else {
            theme::muted_fg()
        };
        let figure = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color)
                        .child(value),
                )
        };
        let chip = div()
            .id("header-account")
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_2()
            .py_0p5()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover()))
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.panel_open;
                this.set_panel_open(open, cx);
            }))
            .when(ready, |el| {
                el.when(fit.equity, |el| {
                    el.child(figure(
                        "Equity",
                        trading::math::format_money(summary.equity, &currency),
                        theme::fg(),
                    ))
                })
                .child(figure(
                    "Open P&L",
                    trading::math::format_money(summary.unrealized, &currency),
                    profit_color,
                ))
            })
            .when(!ready, |el| {
                el.child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child("Account..."),
                )
            });
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(chip)
            .child(
                Button::new("toggle-panel")
                    .ghost()
                    .selected(self.panel_open)
                    .icon(IconName::PanelBottom)
                    .tooltip("Positions, orders and alerts")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let open = !this.panel_open;
                        this.set_panel_open(open, cx);
                    })),
            )
            .child(
                Button::new("toggle-ticket")
                    .ghost()
                    .selected(self.ticket_open)
                    .icon(IconName::PanelRight)
                    .tooltip("Order ticket")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| {
                        let open = !this.ticket_open;
                        this.set_ticket_open(open, cx);
                    })),
            )
    }

    /// The account kind and the state of the connection (the account name is in its menu).
    fn status_block(&self, fit: Fit) -> impl IntoElement {
        let (dot, text): (gpui::AnyElement, &str) = match &self.conn {
            Conn::Ready => (
                ui::status_dot(theme::emerald()).into_any_element(),
                "Connected",
            ),
            Conn::Connecting => (
                anim::spin(
                    ui::icon_colored(IconName::LoaderCircle, 13., theme::muted_fg()),
                    "header-connecting",
                )
                .into_any_element(),
                "Connecting...",
            ),
            Conn::Reconnecting => (
                ui::status_dot(theme::amber()).into_any_element(),
                "Reconnecting...",
            ),
            Conn::Failed(_) => (
                ui::status_dot(theme::destructive()).into_any_element(),
                "Disconnected",
            ),
        };
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .child(ui::environment_badge(self.account.is_live))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .text_color(theme::fg())
                    .child(dot)
                    .when(fit.status_text, |el| el.child(text)),
            )
    }

    fn controls(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fullscreen = window.is_fullscreen();
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                Button::new("toggle-fullscreen")
                    .ghost()
                    .icon(if fullscreen {
                        IconName::Minimize
                    } else {
                        IconName::Maximize
                    })
                    .tooltip(if fullscreen {
                        "Exit full screen"
                    } else {
                        "Full screen"
                    })
                    .cursor_pointer()
                    .on_click(|_event, window, _cx| window.toggle_fullscreen()),
            )
            .child(
                Button::new("account-menu")
                    .ghost()
                    .icon(IconName::LogOut)
                    .tooltip("Account")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.menu_open = !this.menu_open;
                        cx.notify();
                    })),
            )
    }

    /// The menu under the account button: for now, just disconnecting.
    pub(super) fn render_menu(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.menu_open {
            return None;
        }
        let card = div()
            .w(px(300.))
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(theme::fg())
                            .child(self.account.label.clone()),
                    )
                    .child(ui::environment_badge(self.account.is_live)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted_fg())
                    .child(
                        "Disconnecting removes the saved sign-in from this device. You can \
                         connect again anytime.",
                    ),
            )
            .child(
                ui::primary_button(
                    "disconnect-account",
                    "Disconnect",
                    cx.listener(|_this, _event, _window, cx| {
                        cx.emit(DashboardEvent::Disconnect);
                    }),
                )
                .danger()
                .icon(IconName::Unplug),
            );

        // A transparent sheet behind the card: a click anywhere else closes the menu.
        let overlay = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.menu_open = false;
                    cx.notify();
                }),
            )
            .child(div().absolute().top(px(58.)).right_5().child(anim::enter(
                card,
                "account-menu-card",
                0,
            )));
        Some(overlay.into_any_element())
    }
}

/// One timeframe as a small button; the active one is tinted.
fn timeframe_chip(
    prefix: &str,
    timeframe: chart::Timeframe,
    active: bool,
    cx: &mut Context<Dashboard>,
) -> impl IntoElement + use<> {
    div()
        .id(SharedString::from(format!(
            "{prefix}-{}",
            timeframe.label()
        )))
        .h(px(28.))
        .px_2()
        .flex()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(if active {
            theme::fg()
        } else {
            theme::muted_fg()
        })
        .when(active, |el| el.bg(theme::accent_selected()))
        .hover(|style| style.bg(theme::surface_hover()))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.pick_timeframe(timeframe, cx);
        }))
        .child(timeframe.label())
}
