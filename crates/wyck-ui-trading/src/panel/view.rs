//! How the account panel looks: its header with the tabs, the figures of the account and the
//! button that closes everything, then the table of the open tab.

pub mod header;
pub mod table;

use super::AccountPanel;
use super::data::{self, Ctx, Tone};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Role, Window, div, px};
use std::rc::Rc;
use wyck_app::account::Status;
use wyck_app::prefs::panel::Tab;
use wyck_ui_kit::confirm::{Tone as AskTone, confirm_as};
use wyck_ui_kit::prelude::Disableable;
use wyck_ui_kit::{button, theme, tokens};

/// What a button of a row does.
type Action = Rc<dyn Fn(&mut Window, &mut App)>;

fn tone_color(tone: Tone) -> gpui::Rgba {
    match tone {
        Tone::Plain => theme::fg(),
        Tone::Muted => theme::muted_fg(),
        Tone::Up => theme::chart_up(),
        Tone::Down => theme::chart_down(),
        Tone::Accent => theme::emerald(),
    }
}

/// Asks first, then runs.
fn ask(title: &'static str, text: String, run: impl Fn(&mut App) + 'static) -> Action {
    let run = Rc::new(run);
    Rc::new(move |window, cx| {
        let run = run.clone();
        confirm_as(
            window,
            cx,
            AskTone::Caution,
            title,
            text.clone(),
            move |_, cx| run(cx),
        );
    })
}

impl Render for AccountPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.prefs.tab;
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

        // The alerts do not depend on the account, so they show while it loads or failed.
        let ready = status == Status::Ready || matches!(tab, Tab::Alerts | Tab::AlertLog);
        let body: AnyElement = if ready {
            let table = data::build(
                &Ctx {
                    account: self.account.read(cx),
                    alerts: self.alerts.read(cx),
                },
                tab,
            );
            self.table_view(&table, cx)
        } else if let Status::Failed(message) = &status {
            div()
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
                .into_any_element()
        } else {
            div()
                .py_6()
                .flex()
                .justify_center()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child("Reading the account...")
                .into_any_element()
        };

        let tab_buttons: Vec<AnyElement> = Tab::ALL
            .iter()
            .map(|tab| self.tab_button(*tab, count(*tab), cx).into_any_element())
            .collect();
        let account = self.account.clone();
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme::bg())
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
                                button::outlined("panel-close-all")
                                    .label("Close all")
                                    .disabled(positions == 0)
                                    .on_click(move |_, window, cx| {
                                        close_all(&account, window, cx);
                                    }),
                            ),
                    ),
            )
            .child(body)
    }
}

/// The panic button: asks, then closes every open position. Working orders stay.
fn close_all(
    account: &gpui::Entity<wyck_app::account::Account>,
    window: &mut Window,
    cx: &mut App,
) {
    let count = account.read(cx).book.positions.len();
    if count == 0 {
        return;
    }
    let account = account.clone();
    confirm_as(
        window,
        cx,
        AskTone::Danger,
        "Close every position?",
        format!("{count} open position(s) will be closed at market."),
        move |_, cx| account.update(cx, |a, cx| a.close_all(None, cx)),
    );
}
