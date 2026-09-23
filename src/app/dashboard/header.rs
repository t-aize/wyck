//! The bar across the top of the dashboard, and the account menu that hangs from it.

use gpui::prelude::*;
use gpui::{Context, FontWeight, MouseButton, SharedString, Window, anchored, deferred, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};

use super::marks;
use super::{Conn, Dashboard, DashboardEvent, Tick};
use crate::app::connection::ui;
use crate::app::{anim, chart, theme};

impl Dashboard {
    pub(super) fn render_header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
            .child(self.price_block())
            .child(div().flex_1())
            .child(self.timeframe_strip(cx))
            .child(self.status_block())
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
    fn price_block(&self) -> impl IntoElement {
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
            .children(ask.map(|ask| chip("Ask", ask)))
            .children(spread.map(|pips| chip("Spread", format!("{pips} pips"))))
    }

    /// The main timeframes as buttons, and a button that opens all of them: ticks, seconds,
    /// minutes, hours and days.
    fn timeframe_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.chart.read(cx).timeframe();
        let mut quick = Vec::new();
        for timeframe in chart::QUICK {
            quick.push(timeframe_chip("tf", timeframe, current == timeframe, cx));
        }

        // When the chart is on a timeframe that has no button of its own, the menu button says which.
        let in_quick = chart::QUICK.contains(&current);
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
        self.chart
            .update(cx, |chart, cx| chart.set_timeframe(timeframe, cx));
        cx.notify();
    }

    /// Every timeframe, in sections, under the strip.
    fn timeframe_menu(
        &self,
        current: chart::Timeframe,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut card = div()
            .w(px(300.))
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
                chips.push(timeframe_chip(
                    "tf-menu",
                    timeframe,
                    current == timeframe,
                    cx,
                ));
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
        deferred(
            anchored()
                .anchor(gpui::Anchor::TopRight)
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

    /// The account kind and the state of the connection (the account name is in its menu).
    fn status_block(&self) -> impl IntoElement {
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
                    .child(text),
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
