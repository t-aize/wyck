//! One function per screen: the seven of the connection flow. The dashboard is in [`super::dashboard`].
//!
//! A function reads what it needs (the flow's data, the engine state) and returns elements. A
//! click calls a method of [`AppView`], which changes the flow. The words on the screens are
//! the design's, except where the design showed sample data or a fact that is not true of the
//! real servers: addresses, versions and accounts come from the engine, and the instructions
//! name the real menu of cTrader Desktop and the real place of the Remote token.
//!
//! A screen does not animate itself in as a whole: [`AppView`] moves it in and out (see
//! [`super::motion`]). What a screen does animate is what belongs to it: the pieces of its card
//! (see [`card`]), its mark, its pointer reactions, and the token field's shake.

use gpui_kit::base::{Presence, Transition, TransitionId};
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, Context, Div, ElementId, FontWeight, SharedString,
    Stateful, Window, div,
};

use super::app_view::{AppView, LOCAL_HELP_URL, REMOTE_HELP_URL};
use super::motion::{self, Hover};
use super::theme::{self, sz};
use super::widgets::{
    Glyph, back_button, badge, card, error_box, glyph, lead, or_divider, panel, primary_button,
    progress_bar, pulse_dot, row, spinner, status_disc, text_link, title, value,
};
use crate::flow::{Failure, FailureKind, LocalSession, endpoint_authority};

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
        .py(sz(20.))
}

/// The Back link, wired to [`AppView::go_back`].
fn back(window: &mut Window, cx: &mut Context<AppView>) -> Stateful<Div> {
    back_button("back", window, cx)
        .on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx)))
}

/// One of the two big choices of the first screen. Under the pointer its border and ground fade
/// in and its chevron slides 3 px.
fn choice(
    id: &'static str,
    icon: Glyph,
    name: &'static str,
    tag: Option<&'static str>,
    description: &'static str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    let hover = Hover::track(id, window, cx);
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap(sz(13.))
        .w_full()
        .py(sz(16.))
        .pl(sz(18.))
        .pr(sz(16.))
        .bg(hover.mix(theme::bg(), theme::over(theme::bg(), theme::fg(), 0.04)))
        .border_1()
        .border_color(hover.mix(theme::border(), theme::alpha(theme::fg(), 0.24)))
        .rounded(sz(10.))
        .cursor_pointer()
        .on_hover(hover.handler())
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .size(sz(34.))
                .rounded(sz(8.))
                .bg(hover.mix(
                    theme::muted(),
                    theme::over(theme::muted(), theme::fg(), 0.12),
                ))
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
                        .gap(sz(8.))
                        .child(
                            div()
                                .text_size(sz(13.))
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
                        .mt(sz(3.))
                        .text_size(sz(12.))
                        .line_height(gpui_kit::relative(1.5))
                        .text_color(theme::dim())
                        .child(description),
                ),
        )
        .child(
            glyph(
                Glyph::ChevronRight,
                15.,
                hover.mix(theme::dim(), theme::fg()),
            )
            .relative()
            .left(sz(3.0 * hover.amount)),
        )
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
pub(super) fn choose(window: &mut Window, cx: &mut Context<AppView>) -> impl IntoElement {
    let local = choice(
        "choose-local",
        Glyph::Monitor,
        "Local session",
        Some("Recommended"),
        "Finds cTrader Desktop running on this machine. No token needed.",
        window,
        cx,
    )
    .mb(sz(10.))
    .on_click(cx.listener(|this, _, _, cx| this.start_local(cx)));
    let remote = choice(
        "choose-remote",
        Glyph::Cloud,
        "Remote: use a token",
        None,
        "Paste a token from cTrader Web to control your account from another machine.",
        window,
        cx,
    )
    .on_click(cx.listener(|this, _, window, cx| this.open_token(window, cx)));
    stage().child(
        card(440.)
            .child(title("Connect to cTrader"))
            .child(lead(
                "Choose how Wyck talks to your cTrader account. You can switch at any time.",
                360.,
            ))
            .child(local)
            .child(remote),
    )
}

/// "Use a token instead", under the local screens.
fn token_instead(
    label: &'static str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    text_link(
        "token-instead",
        label,
        Some(Glyph::ChevronRight),
        window,
        cx,
    )
    .mt(sz(18.))
    .on_click(cx.listener(|this, _, window, cx| this.open_token(window, cx)))
}

/// Looking for cTrader Desktop.
pub(super) fn searching(
    endpoint: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let back = back(window, cx);
    let instead = token_instead(
        "Prefer to connect a different way? Use a token instead",
        window,
        cx,
    );
    stage().child(back).child(
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
                            .gap(sz(7.))
                            .child(pulse_dot(theme::dim()))
                            .child(value(endpoint.to_owned())),
                        false,
                    ))
                    .child(progress_bar()),
            )
            .child(instead),
    )
}

/// cTrader Desktop answered.
pub(super) fn local_found(
    session: &LocalSession,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let version = session.server_version.clone();
    let back = back(window, cx);
    let disc = status_disc(
        Glyph::Check,
        theme::green(),
        theme::alpha(theme::green(), 0.14),
        window,
        cx,
    );
    let go = primary_button("continue", "Continue to Wyck", window, cx)
        .mt(sz(22.))
        .on_click(cx.listener(|this, _, _, cx| this.continue_local(cx)));
    let instead = token_instead(
        "Prefer to connect a different way? Use a token instead",
        window,
        cx,
    );
    stage().child(back).child(
        card(400.)
            .child(disc)
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
                        value(session.endpoint.clone()),
                        true,
                    ))
                    .children(version.map(|v| {
                        row(Some(Glyph::AppWindow), "cTrader version", value(v), true)
                    }))
                    .child(row(
                        Some(Glyph::User),
                        "Account",
                        div()
                            .flex()
                            .items_center()
                            .gap(sz(8.))
                            .child(value(format!("#{}", session.account_id)))
                            .child(badge(&session.kind)),
                        false,
                    )),
            )
            .child(go)
            .child(instead),
    )
}

/// cTrader Desktop did not answer.
pub(super) fn local_not_found(
    failure: &Failure,
    endpoint: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let address = endpoint_authority(endpoint).to_owned();
    let checks: [SharedString; 3] = [
        "cTrader Desktop is open on this machine and you are logged in".into(),
        "The MCP server is on: Settings > MCP Server > Enable MCP server".into(),
        format!("Nothing blocks {address}, the port set on that settings page").into(),
    ];
    let last = checks.len() - 1;
    let back = back(window, cx);
    let disc = status_disc(Glyph::CircleAlert, theme::fg(), theme::muted(), window, cx);
    let retry = primary_button("retry-local", "Try again", window, cx)
        .mt(sz(22.))
        .on_click(cx.listener(|this, _, _, cx| this.start_local(cx)));
    let help = text_link(
        "local-help",
        "How to enable the local MCP server",
        Some(Glyph::ArrowUpRight),
        window,
        cx,
    )
    .mt(sz(18.))
    .on_click(cx.listener(|this, _, _, cx| this.open_url(LOCAL_HELP_URL, cx)));
    let instead = token_instead("Or connect with a token instead", window, cx);
    stage().child(back).child(
        card(400.)
            .child(disc)
            .child(title("Couldn't find cTrader Desktop"))
            .child(lead(
                "Wyck didn't get an answer from a local session. Check the following, then try again.",
                350.,
            ))
            .child(panel().children(
                checks
                    .into_iter()
                    .enumerate()
                    .map(|(i, text)| row(Some(Glyph::Circle), text, div(), i != last)),
            ))
            .child(
                div()
                    .w_full()
                    .mt(sz(12.))
                    .font_features(theme::tabular())
                    .text_size(sz(11.))
                    .line_height(gpui_kit::relative(1.5))
                    .text_color(theme::dim())
                    .child(failure_text(failure, endpoint)),
            )
            .child(retry)
            .child(help)
            .child(instead),
    )
}

/// Wraps the token field so that it shakes when `generation` changes to a new non-zero value: a
/// short, damped side to side that says "no" (see [`motion::shake`]). With generation 0 nothing
/// has gone wrong yet and there is no wrapper.
fn shaken(field: impl IntoElement, generation: u32) -> AnyElement {
    if generation == 0 {
        return field.into_any_element();
    }
    div()
        .relative()
        .w_full()
        .child(field)
        .with_animation(
            ElementId::from(("shake", generation as usize)),
            Animation::new(motion::SHAKE_TIME),
            |el, t| el.left(sz(motion::shake(t))),
        )
        .into_any_element()
}

/// The token form, fresh or after a failed attempt.
///
/// `live` is false for the copy of the screen that is on its way out: it draws the field without
/// the input itself, so that there is only ever one text input on screen.
pub(super) fn token(
    view: &mut AppView,
    refused: Option<&Failure>,
    live: bool,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let failed = refused.is_some() || view.token_error.is_some();
    let field = view.token_field(failed, live, window, cx);
    let input_error = view.token_error;
    let shake = view.shake;

    let help = refused.is_none().then(|| {
        text_link(
            "token-help",
            "Where do I find this?",
            Some(Glyph::ArrowUpRight),
            window,
            cx,
        )
        .text_size(sz(11.))
        .on_click(cx.listener(|this, _, _, cx| this.open_url(REMOTE_HELP_URL, cx)))
    });
    let label = div()
        .w_full()
        .mb(sz(6.))
        .flex()
        .items_center()
        .justify_between()
        .text_size(sz(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::dim())
        .child("API token")
        .children(help);
    // The message under the field fades in and settles 4 px, once per new error.
    let message = input_error.map(|error| {
        let progress = Presence::new(TransitionId::from((shake as usize, "token-error")), true)
            .transition(Transition::new(motion::NORMAL).easing(motion::enter()))
            .sample(window, cx)
            .progress;
        div()
            .mt(sz(6.))
            .relative()
            .top(sz((1.0 - progress) * -4.0))
            .opacity(progress)
            .text_size(sz(11.5))
            .line_height(gpui_kit::relative(1.5))
            .text_color(theme::red_text())
            .child(error.to_string())
    });
    let field = div()
        .w_full()
        .mb(sz(8.))
        .child(label)
        .child(shaken(field, shake))
        .children(message);

    let back = back(window, cx);
    match refused {
        None => {
            let disc = status_disc(Glyph::Cloud, theme::fg(), theme::muted(), window, cx);
            let connect = primary_button("connect-token", "Connect", window, cx)
                .on_click(cx.listener(|this, _, window, cx| this.submit_token(window, cx)));
            let instead = text_link(
                "local-instead",
                "Have cTrader Desktop open here instead? Auto-detect",
                Some(Glyph::ChevronRight),
                window,
                cx,
            )
            .mt(sz(14.))
            .on_click(cx.listener(|this, _, _, cx| this.start_local(cx)));
            stage().child(back).child(
                card(420.)
                    .child(disc)
                    .child(title("Connect with a token"))
                    .child(lead(
                        "Copy the token from cTrader Web (Settings > Remote MCP) to control your account from another machine. Each trading account has its own token.",
                        340.,
                    ))
                    .child(field)
                    .child(connect)
                    .child(or_divider())
                    .child(instead),
            )
        }
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
            let disc = status_disc(
                Glyph::CircleX,
                theme::red(),
                theme::alpha(theme::red(), 0.12),
                window,
                cx,
            );
            let retry = primary_button("connect-token", "Try again", window, cx)
                .on_click(cx.listener(|this, _, window, cx| this.submit_token(window, cx)));
            let instead = text_link(
                "local-instead",
                "Use a local session instead",
                Some(Glyph::ChevronRight),
                window,
                cx,
            )
            .mt(sz(16.))
            .on_click(cx.listener(|this, _, _, cx| this.start_local(cx)));
            stage().child(back).child(
                card(400.)
                    .child(disc)
                    .child(title(heading))
                    .child(lead(text, 340.))
                    .child(error_box(failure.detail.clone()))
                    .child(field)
                    .child(retry)
                    .child(instead),
            )
        }
    }
}

/// Checking a token.
pub(super) fn verifying(
    hint: &str,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let back = back(window, cx);
    stage().child(back).child(
        card(380.)
            .child(spinner(Glyph::Cloud, 34., false))
            .child(title("Verifying your token..."))
            .child(lead(
                "Checking access and syncing your account. This only takes a moment.",
                300.,
            ))
            .child(
                panel()
                    .child(row(None, "Token", value(hint.to_owned()), false))
                    .child(progress_bar()),
            ),
    )
}
