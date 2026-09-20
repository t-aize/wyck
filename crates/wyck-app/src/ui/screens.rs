//! One function per screen: the seven of the connection flow and the connected screen.
//!
//! A function reads what it needs (the flow's data, the engine state) and returns elements. A
//! click calls a method of [`AppView`], which changes the flow. The words on the screens are
//! the design's, except where the design showed sample data or a fact that is not true of the
//! real servers: addresses, versions and accounts come from the engine, and the instructions
//! name the real menu of cTrader Desktop and the real place of the Remote token.

use gpui_kit::prelude::*;
use gpui_kit::{Context, Div, FontWeight, SharedString, Stateful, Window, div, px};

use super::app_view::{AppView, LOCAL_HELP_URL, REMOTE_HELP_URL};
use super::theme;
use super::widgets::{
    Glyph, back_button, badge, card, error_box, glyph, lead, mono, or_divider, panel,
    primary_button, progress_bar, pulse_dot, row, secondary_button, spinner, status_disc,
    text_link, title,
};
use crate::flow::{Failure, FailureKind, LocalSession, endpoint_authority};
use crate::presentation::header;

/// The area a screen is drawn in: fills the space under the title bar, centers its card, and
/// scrolls when the window is too low for it. (The card centers itself with auto margins, which
/// give way to scrolling instead of cutting off its top.)
fn stage() -> Stateful<Div> {
    div()
        .id("stage")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .items_center()
        .py(px(20.))
}

/// The Back link, wired to [`AppView::go_back`].
fn back(cx: &mut Context<AppView>) -> Stateful<Div> {
    back_button("back").on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx)))
}

/// One of the two big choices of the first screen.
fn choice(
    id: &'static str,
    icon: Glyph,
    name: &'static str,
    tag: Option<&'static str>,
    description: &'static str,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(px(13.))
        .w_full()
        .py(px(16.))
        .pl(px(18.))
        .pr(px(16.))
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border())
        .rounded(px(10.))
        .cursor_pointer()
        .hover(|style| {
            style
                .border_color(theme::alpha(theme::fg(), 0.22))
                .bg(theme::alpha(theme::fg(), 0.02))
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .size(px(34.))
                .rounded(px(8.))
                .bg(theme::muted())
                .child(glyph(icon, 16., theme::fg())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme::fg())
                                .child(name),
                        )
                        .children(
                            tag.map(|t| super::widgets::pill(t, theme::dim(), theme::muted())),
                        ),
                )
                .child(
                    div()
                        .mt(px(3.))
                        .text_size(px(12.))
                        .line_height(gpui_kit::relative(1.5))
                        .text_color(theme::dim())
                        .child(description),
                ),
        )
        .child(glyph(Glyph::ChevronRight, 15., theme::dim()))
}

/// What to say under the checklist of the "not found" screen: where nothing answered, or what
/// went wrong when it was not that.
fn failure_text(failure: &Failure, endpoint: &str) -> String {
    match failure.kind {
        FailureKind::Unreachable => format!("Nothing answered at {endpoint}."),
        _ => failure.detail.clone(),
    }
}

/// The first screen: local session or token.
pub(super) fn choose(cx: &mut Context<AppView>) -> impl IntoElement {
    stage().child(
        card(440.)
            .child(title("Connect to cTrader"))
            .child(lead(
                "Choose how Wyck talks to your cTrader account. You can switch at any time.",
                360.,
            ))
            .child(
                choice(
                    "choose-local",
                    Glyph::Monitor,
                    "Local session",
                    Some("Recommended"),
                    "Finds cTrader Desktop running on this machine. No token needed.",
                )
                .mb(px(10.))
                .on_click(cx.listener(|this, _, _, cx| this.start_local(cx))),
            )
            .child(
                choice(
                    "choose-remote",
                    Glyph::Cloud,
                    "Remote: use a token",
                    None,
                    "Paste a token from cTrader Web to control your account from another machine.",
                )
                .on_click(cx.listener(|this, _, window, cx| this.open_token(window, cx))),
            ),
    )
}

/// "Use a token instead", under the local screens.
fn token_instead(label: &'static str, cx: &mut Context<AppView>) -> Stateful<Div> {
    text_link("token-instead", label, Some(Glyph::ChevronRight))
        .mt(px(18.))
        .on_click(cx.listener(|this, _, window, cx| this.open_token(window, cx)))
}

/// Looking for cTrader Desktop.
pub(super) fn searching(endpoint: &str, cx: &mut Context<AppView>) -> impl IntoElement {
    stage().child(back(cx)).child(
        card(400.)
            .child(spinner(Glyph::Monitor, 38., true))
            .child(title("Looking for cTrader Desktop..."))
            .child(lead(
                "Wyck is looking for the local MCP server of cTrader Desktop. Keep cTrader open on this machine.",
                350.,
            ))
            .child(
                panel()
                    .child(row(
                        None,
                        "Server",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(pulse_dot(theme::dim()))
                            .child(mono(endpoint.to_owned())),
                        false,
                    ))
                    .child(progress_bar()),
            )
            .child(token_instead("Prefer to connect a different way? Use a token instead", cx)),
    )
}

/// cTrader Desktop answered.
pub(super) fn local_found(session: &LocalSession, cx: &mut Context<AppView>) -> impl IntoElement {
    let version = session.server_version.clone();
    stage().child(back(cx)).child(
        card(400.)
            .child(status_disc(
                Glyph::Check,
                theme::green(),
                theme::alpha(theme::green(), 0.14),
            ))
            .child(title("cTrader Desktop detected"))
            .child(lead(
                "Wyck found the local MCP server of cTrader Desktop on this machine. No further setup needed.",
                340.,
            ))
            .child(
                panel()
                    .child(row(
                        Some(Glyph::Scan),
                        "Server",
                        mono(session.endpoint.clone()),
                        true,
                    ))
                    .children(version.map(|v| row(Some(Glyph::AppWindow), "cTrader version", mono(v), true)))
                    .child(row(
                        Some(Glyph::User),
                        "Account",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(mono(format!("#{}", session.account_id)))
                            .child(badge(&session.kind)),
                        false,
                    )),
            )
            .child(
                primary_button("continue", "Continue to Wyck")
                    .mt(px(22.))
                    .on_click(cx.listener(|this, _, _, cx| this.continue_local(cx))),
            )
            .child(token_instead("Prefer to connect a different way? Use a token instead", cx)),
    )
}

/// cTrader Desktop did not answer.
pub(super) fn local_not_found(
    failure: &Failure,
    endpoint: &str,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let address = endpoint_authority(endpoint).to_owned();
    let checks: [SharedString; 3] = [
        "cTrader Desktop is open on this machine and you are logged in".into(),
        "The MCP server is on: Settings > MCP Server > Enable MCP server".into(),
        format!("Nothing blocks {address}, the port set on that settings page").into(),
    ];
    let last = checks.len() - 1;
    stage().child(back(cx)).child(
        card(400.)
            .child(status_disc(Glyph::CircleAlert, theme::fg(), theme::muted()))
            .child(title("Couldn't find cTrader Desktop"))
            .child(lead(
                "Wyck didn't get an answer from a local session. Check the following, then try again.",
                350.,
            ))
            .child(panel().children(checks.into_iter().enumerate().map(|(i, text)| {
                row(Some(Glyph::Circle), text, div(), i != last)
            })))
            .child(
                div()
                    .w_full()
                    .mt(px(12.))
                    .font_family(theme::MONO)
                    .text_size(px(11.))
                    .line_height(gpui_kit::relative(1.5))
                    .text_color(theme::dim())
                    .child(failure_text(failure, endpoint)),
            )
            .child(
                primary_button("retry-local", "Try again")
                    .mt(px(22.))
                    .on_click(cx.listener(|this, _, _, cx| this.start_local(cx))),
            )
            .child(
                text_link("local-help", "How to enable the local MCP server", Some(Glyph::ArrowUpRight))
                    .mt(px(18.))
                    .on_click(cx.listener(|this, _, _, cx| this.open_url(LOCAL_HELP_URL, cx))),
            )
            .child(token_instead("Or connect with a token instead", cx)),
    )
}

/// The token form, fresh or after a failed attempt.
pub(super) fn token(
    view: &mut AppView,
    refused: Option<&Failure>,
    window: &Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let failed = refused.is_some() || view.token_error.is_some();
    let field = view.token_field(failed, window, cx);
    let input_error = view.token_error;

    let label = div()
        .w_full()
        .mb(px(6.))
        .flex()
        .items_center()
        .justify_between()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::dim())
        .child("API token")
        .when(refused.is_none(), |el| {
            el.child(
                text_link(
                    "token-help",
                    "Where do I find this?",
                    Some(Glyph::ArrowUpRight),
                )
                .text_size(px(11.))
                .on_click(cx.listener(|this, _, _, cx| this.open_url(REMOTE_HELP_URL, cx))),
            )
        });
    let field = div()
        .w_full()
        .mb(px(8.))
        .child(label)
        .child(field)
        .children(input_error.map(|error| {
            div()
                .mt(px(6.))
                .text_size(px(11.5))
                .line_height(gpui_kit::relative(1.5))
                .text_color(theme::red_text())
                .child(error.to_string())
        }));

    let connect = |label: &'static str, cx: &mut Context<AppView>| {
        primary_button("connect-token", label)
            .on_click(cx.listener(|this, _, window, cx| this.submit_token(window, cx)))
    };

    match refused {
        None => stage().child(back(cx)).child(
            card(420.)
                .child(status_disc(Glyph::Cloud, theme::fg(), theme::muted()))
                .child(title("Connect with a token"))
                .child(lead(
                    "Copy the token from cTrader Web (Settings > Remote MCP) to control your account from another machine. Each trading account has its own token.",
                    340.,
                ))
                .child(field)
                .child(connect("Connect", cx))
                .child(or_divider())
                .child(
                    text_link(
                        "local-instead",
                        "Have cTrader Desktop open here instead? Auto-detect",
                        Some(Glyph::ChevronRight),
                    )
                    .mt(px(14.))
                    .on_click(cx.listener(|this, _, _, cx| this.start_local(cx))),
                ),
        ),
        Some(failure) => {
            let (heading, text) = match failure.kind {
                FailureKind::Refused => (
                    "That token didn't work",
                    "Double check you copied the full token, and that it hasn't expired or been revoked.",
                ),
                FailureKind::Unreachable => (
                    "Couldn't reach cTrader",
                    "Wyck could not get an answer from the server. Check your internet connection and try again.",
                ),
                FailureKind::Other => (
                    "Couldn't connect",
                    "Something went wrong while connecting. The details are below.",
                ),
            };
            stage().child(back(cx)).child(
                card(400.)
                    .child(status_disc(
                        Glyph::CircleX,
                        theme::red(),
                        theme::alpha(theme::red(), 0.12),
                    ))
                    .child(title(heading))
                    .child(lead(text, 340.))
                    .child(error_box(failure.detail.clone()))
                    .child(field)
                    .child(connect("Try again", cx))
                    .child(
                        text_link(
                            "local-instead",
                            "Use a local session instead",
                            Some(Glyph::ChevronRight),
                        )
                        .mt(px(16.))
                        .on_click(cx.listener(|this, _, _, cx| this.start_local(cx))),
                    ),
            )
        }
    }
}

/// Checking a token.
pub(super) fn verifying(hint: &str, cx: &mut Context<AppView>) -> impl IntoElement {
    stage().child(back(cx)).child(
        card(380.)
            .child(spinner(Glyph::Cloud, 34., false))
            .child(title("Verifying your token..."))
            .child(lead(
                "Checking access and syncing your account. This only takes a moment.",
                300.,
            ))
            .child(
                panel()
                    .child(row(None, "Token", mono(hint.to_owned()), false))
                    .child(progress_bar()),
            ),
    )
}

/// Connected. The trading screens are not built yet: this shows the account and the way back.
pub(super) fn connected(view: &mut AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let state = view.shell.model.read(cx).state.clone();
    let head = header(&state);
    let figure =
        |label: &'static str, value: String, divider: bool| row(None, label, mono(value), divider);
    stage().child(
        card(460.)
            .child(status_disc(
                Glyph::Check,
                theme::green(),
                theme::alpha(theme::green(), 0.14),
            ))
            .child(title("Connected"))
            .child(lead(
                "The trading screens are not built yet. The account below is live, and the global shortcuts plan dry-run orders.",
                360.,
            ))
            .child(
                panel()
                    .child(row(
                        None,
                        "Server",
                        mono(head.service.unwrap_or("-")),
                        true,
                    ))
                    .child(row(
                        None,
                        "Account",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(mono(head.account_id.map_or_else(|| "-".to_owned(), |id| format!("#{id}"))))
                            .child(badge(&head.kind)),
                        true,
                    ))
                    .child(row(None, "Session", badge(&head.session), true))
                    .child(row(None, "Orders", badge(&head.mode), true))
                    .child(figure("Balance", head.balance, true))
                    .child(figure("Equity", head.equity, false)),
            )
            .child(
                secondary_button("switch-connection", "Switch connection")
                    .mt(px(22.))
                    .on_click(cx.listener(|this, _, window, cx| this.switch_connection(window, cx))),
            ),
    )
}
