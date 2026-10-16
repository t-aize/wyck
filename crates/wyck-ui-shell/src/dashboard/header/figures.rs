//! The account menu that hangs from the header, and the figures in it.

use crate::dashboard::{Conn, Dashboard, DashboardEvent, MenuKind};
use gpui::prelude::*;
use gpui::{Context, MouseButton, div, px};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::{anim, button, layout, menu, theme, tokens};

impl Dashboard {
    /// The account's figures and the connection, for the account menu.
    pub(super) fn menu_figures(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let account = self.trading.read(cx);
        let summary = account.summary();
        let currency = account.book.currency.clone();
        let money = |v: f64| wyck_trading::math::format_money(v, &currency);
        let connection = match &self.conn {
            Conn::Ready => ("Connected", theme::emerald()),
            Conn::Connecting => ("Connecting...", theme::muted_fg()),
            Conn::Reconnecting => ("Reconnecting...", theme::amber()),
            Conn::Failed(_) => ("Disconnected", theme::destructive()),
        };
        let row = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_size(px(tokens::text::body()))
                .child(div().text_color(theme::muted_fg()).child(label))
                .child(div().text_color(color).child(value))
        };
        let tone = if summary.unrealized > 0.0 {
            theme::chart_up()
        } else if summary.unrealized < 0.0 {
            theme::chart_down()
        } else {
            theme::fg()
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(row("Connection", connection.0.to_owned(), connection.1))
            .child(row("Balance", money(summary.balance), theme::fg()))
            .child(row("Equity", money(summary.equity), theme::fg()))
            .child(row("Open profit", money(summary.unrealized), tone))
            .child(row("Margin used", money(summary.margin), theme::fg()))
            .child(row("Free margin", money(summary.free_margin), theme::fg()))
            .child(row(
                "Margin level",
                summary
                    .margin_level
                    .map_or_else(|| "-".to_owned(), |l| format!("{l:.0}%")),
                theme::fg(),
            ))
    }

    /// The menu under the account: its figures, and disconnecting.
    pub(in crate::dashboard) fn render_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.menu_is(MenuKind::Account) {
            return None;
        }
        let card = menu::parts::panel(tokens::menu::PANEL_WIDTH)
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
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::fg())
                            .child(self.account.label.clone()),
                    )
                    .child(layout::environment_badge(self.account.is_live)),
            )
            .child(self.menu_figures(cx))
            .child(
                button::outlined("open-settings")
                    .w_full()
                    .icon(IconName::Settings)
                    .label(gpui::SharedString::from(format!("Settings {}", wyck_ui_kit::hints::suffix("open_settings"))))
                    .on_click(cx.listener(|this, _event, window, cx| {
                        this.close_menus();
                        cx.notify();
                        crate::settings_hub::open(
                            this.workspace.clone(),
                            this.multi.clone(),
                            window,
                            cx,
                        );
                    })),
            )
            .child(layout::rule_h())
            .child(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child(
                        "Disconnecting removes the saved sign-in from this device. You can \
                         connect again anytime.",
                    ),
            )
            .child(
                button::primary(
                    "disconnect-account",
button::Fill::Danger, button::Size::Md,
                    "Disconnect",
                    cx.listener(|this, _event, window, cx| {
                        this.close_menus();
                        cx.notify();
                        let dashboard = cx.entity();
                        wyck_ui_kit::confirm::confirm_as(
                            window,
                            cx,
                            wyck_ui_kit::confirm::Tone::Danger,
                            "Disconnect?",
                            "The saved sign-in is removed from this device. You can connect again anytime.",
                            move |_window, cx| {
                                dashboard.update(cx, |_, cx| cx.emit(DashboardEvent::Disconnect));
                            },
                        );
                    }),
                )
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
                    this.close_menus();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(tokens::bar::header() + 6.0))
                    .right_3()
                    .child(anim::enter(card, "account-menu-card", 0)),
            );
        Some(overlay.into_any_element())
    }
}
