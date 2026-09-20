//! The root view: title bar, banners, the current screen and the toasts.
//!
//! [`AppView`] owns the [`ConnectFlow`] and the token field. It draws what the flow says
//! ([`super::screens`]), and turns clicks into flow transitions and controller calls: the
//! network work runs in a spawned task, and its result comes back through the flow, which drops
//! it if the user has moved on in the meantime (see [`crate::flow`]).
//!
//! # Startup
//!
//! With a saved account (or the environment variables of [`crate::settings`]) the view starts on
//! the matching waiting screen and connects by itself: success goes straight to the connected
//! screen, failure lands on the screen that explains it. Without one, it starts on the first
//! screen. A connection the user makes is saved on success, so the next start is the first case
//! (see [`crate::startup::remember_connection`]).

use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable as _, KeyDownEvent, SharedString, Subscription, Task,
    Window, div, px,
};
use secrecy::SecretString;
use wyck_engine::broker::{ConnectRequest, ServiceKind};
use wyck_engine::domain::now_millis;

use super::screens;
use super::theme;
use super::titlebar::titlebar;
use super::widgets::{Glyph, glyph};
use crate::flow::{
    ConnectFlow, Delivery, Exit, Failure, LocalSession, Screen, TokenError, mask_token,
    validate_token,
};
use crate::messages::{Level, Notice, describe_startup};
use crate::shell::Shell;
use crate::startup::{StartupError, open_user_config, remember_connection};

/// Where the token is found, opened by the link on the token screen.
pub const REMOTE_HELP_URL: &str = "https://help.ctrader.com/ctrader-ai-agent-connect/remote-mcp/";
/// How to enable the local MCP server in cTrader Desktop.
pub const LOCAL_HELP_URL: &str =
    "https://help.ctrader.com/ctrader-ai-agent-connect/local-mcp/setup/";

/// How often expired toasts are removed while there are some.
const TOAST_TICK: Duration = Duration::from_millis(500);

/// The root view of the main window.
pub struct AppView {
    pub(super) shell: Shell,
    pub(super) flow: ConnectFlow,
    pub(super) token: Entity<InputState>,
    pub(super) token_masked: bool,
    pub(super) token_error: Option<TokenError>,
    focus: FocusHandle,
    toast_timer: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl AppView {
    /// Builds the view and starts the automatic connection, if there is one to start.
    ///
    /// `preview` (see [`super::preview`]) opens on a given screen and connects to nothing.
    pub fn new(
        shell: Shell,
        connection: Result<ConnectRequest, StartupError>,
        preview: Option<Screen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let token = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Paste your API token")
        });
        let subscriptions =
            vec![
                cx.subscribe_in(&token, window, |this, _, event: &InputEvent, window, cx| {
                    match event {
                        InputEvent::PressEnter { .. } => this.submit_token(window, cx),
                        InputEvent::Change if this.token_error.take().is_some() => cx.notify(),
                        _ => {}
                    }
                }),
                cx.observe(&shell.model, |this, _, cx| this.model_changed(cx)),
            ];

        let focus = cx.focus_handle();
        let mut view = Self {
            shell,
            flow: ConnectFlow::new(),
            token,
            token_masked: true,
            token_error: None,
            focus,
            toast_timer: None,
            _subscriptions: subscriptions,
        };

        if let Some(screen) = preview {
            view.flow = ConnectFlow::showing(screen);
        } else {
            match connection {
                Ok(request) => view.resume(request, cx),
                Err(error) => {
                    tracing::info!(%error, "no connection to open at startup");
                    if let Some(notice) = describe_startup(&error) {
                        view.shell
                            .model
                            .update(cx, |model, _| model.banners.push(notice));
                    }
                }
            }
        }
        view.settle_focus(window, cx);
        view
    }

    // ---- automatic connection at startup ----

    fn resume(&mut self, request: ConnectRequest, cx: &mut Context<Self>) {
        let local = request.service == ServiceKind::CtraderLocal;
        let (flow, attempt) = ConnectFlow::resume(local, "your saved token");
        self.flow = flow;
        let controller = self.shell.controller.clone();
        cx.spawn(async move |this, cx| {
            let result = controller.connect_saved(request).await;
            this.update(cx, |this, cx| {
                if local {
                    let session = result.and_then(|s| {
                        s.ok_or_else(|| Failure::other("cTrader Desktop reported no account."))
                    });
                    this.deliver_local(attempt, session, cx);
                } else {
                    this.deliver_remote(attempt, result.map(|_| ()), None, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    // ---- flow actions, called by the screens ----

    /// "Local session": looks for cTrader Desktop.
    pub(super) fn start_local(&mut self, cx: &mut Context<Self>) {
        if self.flow.is_busy() {
            return;
        }
        let attempt = self.flow.begin_local();
        cx.notify();
        let controller = self.shell.controller.clone();
        cx.spawn(async move |this, cx| {
            let result = controller.connect_local().await;
            this.update(cx, |this, cx| this.deliver_local(attempt, result, cx))
                .ok();
        })
        .detach();
    }

    /// "Remote": shows the token form.
    pub(super) fn open_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flow.is_busy() {
            return;
        }
        self.flow.open_token();
        self.token_error = None;
        self.settle_focus(window, cx);
        cx.notify();
    }

    /// "Connect" on the token form: checks the text, then verifies the token.
    pub(super) fn submit_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.flow.screen(), Screen::Token { .. }) {
            return;
        }
        let raw = self.token.read(cx).value();
        let token = match validate_token(&raw) {
            Ok(token) => token,
            Err(error) => {
                self.token_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.token_error = None;
        let attempt = self.flow.begin_remote(mask_token(&token));
        self.settle_focus(window, cx);
        cx.notify();

        let controller = self.shell.controller.clone();
        cx.spawn(async move |this, cx| {
            let result = controller.connect_remote(token.clone()).await;
            this.update(cx, |this, cx| {
                this.deliver_remote(attempt, result, Some(token), cx);
            })
            .ok();
        })
        .detach();
    }

    /// "Back", or Escape.
    pub(super) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let exit = self.flow.back();
        if exit == Exit::DropSession {
            self.drop_session(cx);
        }
        self.settle_focus(window, cx);
        cx.notify();
    }

    /// "Continue to Wyck" on the found screen.
    pub(super) fn continue_local(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.flow.screen(), Screen::LocalFound(_)) {
            return;
        }
        self.flow.accept_local();
        self.remember(ServiceKind::CtraderLocal, None, cx);
        cx.notify();
    }

    /// "Switch connection" on the connected screen.
    pub(super) fn switch_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.flow.switch() == Exit::DropSession {
            self.drop_session(cx);
        }
        self.settle_focus(window, cx);
        cx.notify();
    }

    /// The eye button.
    pub(super) fn toggle_mask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.token_masked = !self.token_masked;
        let masked = self.token_masked;
        self.token
            .update(cx, |state, cx| state.set_masked(masked, window, cx));
        cx.notify();
    }

    /// The paste button: replaces the field with the clipboard text.
    pub(super) fn paste_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let text = text.trim().to_owned();
        self.token
            .update(cx, |state, cx| state.set_value(text, window, cx));
        self.token_error = None;
        cx.notify();
    }

    /// Opens `url` in the browser.
    pub(super) fn open_url(&self, url: &'static str, cx: &mut Context<Self>) {
        cx.open_url(url);
    }

    // ---- results ----

    fn deliver_local(
        &mut self,
        attempt: crate::flow::Attempt,
        result: Result<LocalSession, Failure>,
        cx: &mut Context<Self>,
    ) {
        let opened = result.is_ok();
        if let Err(failure) = &result {
            tracing::info!(reason = %failure.raw, "no local session");
        }
        match self.flow.finish_local(attempt, result) {
            Delivery::Applied => cx.notify(),
            Delivery::Stale => self.drop_stale(opened, cx),
        }
    }

    fn deliver_remote(
        &mut self,
        attempt: crate::flow::Attempt,
        result: Result<(), Failure>,
        token: Option<SecretString>,
        cx: &mut Context<Self>,
    ) {
        let opened = result.is_ok();
        if let Err(failure) = &result {
            tracing::info!(kind = ?failure.kind, reason = %failure.raw, "the token was not accepted");
        }
        match self.flow.finish_remote(attempt, result) {
            Delivery::Applied => {
                if opened && token.is_some() {
                    self.remember(ServiceKind::CtraderRemote, token, cx);
                }
                cx.notify();
            }
            Delivery::Stale => self.drop_stale(opened, cx),
        }
    }

    /// A success that arrived after the user walked away opened a session nobody wants. Drop it,
    /// unless another attempt has started or a session is on screen: that one owns the engine now.
    fn drop_stale(&mut self, opened: bool, cx: &mut Context<Self>) {
        if opened && !self.flow.is_busy() && !self.flow.holds_session() {
            tracing::info!("dropping a session opened by an abandoned attempt");
            self.drop_session(cx);
        }
    }

    fn drop_session(&self, cx: &mut Context<Self>) {
        let controller = self.shell.controller.clone();
        cx.spawn(async move |_, _| controller.disconnect().await)
            .detach();
    }

    /// Saves the account just connected to, off the UI thread (the credential store can block).
    fn remember(&self, service: ServiceKind, token: Option<SecretString>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let saved = cx
                .background_executor()
                .spawn(async move { remember_connection(service, token, open_user_config) })
                .await;
            if let Err(error) = saved {
                tracing::warn!(%error, "the account could not be saved");
                this.update(cx, |this, cx| {
                    let notice = Notice::warning(
                        "This account was not saved",
                        format!("You will have to connect again next time. {error}"),
                    );
                    this.shell.model.update(cx, |model, cx| {
                        model.push_notice(notice, now_millis());
                        cx.notify();
                    });
                })
                .ok();
            }
        })
        .detach();
    }

    // ---- focus, toasts ----

    /// Gives the keyboard to the token field on the token form, and to the window otherwise, so
    /// that Escape always reaches the view.
    fn settle_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.flow.screen(), Screen::Token { .. }) {
            self.token.update(cx, |state, cx| state.focus(window, cx));
        } else {
            self.focus.focus(window, cx);
        }
    }

    fn model_changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        if self.toast_timer.is_some() || self.shell.model.read(cx).toasts.is_empty() {
            return;
        }
        self.toast_timer = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TOAST_TICK).await;
                let Ok(done) = this.update(cx, |this, cx| {
                    let now = now_millis();
                    let empty = this.shell.model.update(cx, |model, cx| {
                        if model.expire_toasts(now) {
                            cx.notify();
                        }
                        model.toasts.is_empty()
                    });
                    if empty {
                        this.toast_timer = None;
                    }
                    empty
                }) else {
                    break;
                };
                if done {
                    break;
                }
            }
        }));
    }

    // ---- drawing ----

    fn banners(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let banners = self.shell.model.read(cx).banners.clone();
        div()
            .flex()
            .flex_col()
            .flex_none()
            .children(banners.into_iter().enumerate().map(|(index, banner)| {
                let color = level_color(banner.level);
                let text = match (&banner.detail, banner.hint) {
                    (Some(detail), Some(hint)) => format!("{detail} {hint}"),
                    (Some(detail), None) => detail.clone(),
                    (None, Some(hint)) => hint.to_owned(),
                    (None, None) => String::new(),
                };
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(9.))
                    .bg(theme::alpha(color, 0.08))
                    .border_b_1()
                    .border_color(theme::alpha(color, 0.25))
                    .child(
                        div()
                            .mt(px(1.))
                            .child(glyph(level_glyph(banner.level), 14., color)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .line_height(gpui_kit::relative(1.5))
                            .text_color(theme::fg())
                            .child(
                                div()
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .child(banner.title),
                            )
                            .when(!text.is_empty(), |el| {
                                el.child(div().text_color(theme::dim()).child(text))
                            }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("banner-dismiss-{index}")))
                            .flex_none()
                            .p(px(3.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::muted()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.shell.model.update(cx, |model, cx| {
                                    model.dismiss_banner(index);
                                    cx.notify();
                                });
                            }))
                            .child(glyph(Glyph::X, 12., theme::dim())),
                    )
            }))
    }

    fn toasts(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toasts = self.shell.model.read(cx).toasts.clone();
        div()
            .absolute()
            .bottom(px(16.))
            .right(px(16.))
            .w(px(330.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(toasts.into_iter().map(|toast| {
                let color = level_color(toast.notice.level);
                let id = toast.id;
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(10.))
                    .px(px(13.))
                    .py(px(11.))
                    .bg(theme::card())
                    .border_1()
                    .border_color(theme::alpha(color, 0.35))
                    .rounded(px(10.))
                    .child(div().mt(px(1.)).child(glyph(
                        level_glyph(toast.notice.level),
                        14.,
                        color,
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .text_size(px(12.))
                            .line_height(gpui_kit::relative(1.5))
                            .child(
                                div()
                                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                                    .text_color(theme::fg())
                                    .child(toast.notice.title.clone()),
                            )
                            .children(
                                toast
                                    .notice
                                    .detail
                                    .clone()
                                    .map(|d| div().text_color(theme::dim()).child(d)),
                            )
                            .children(
                                toast
                                    .notice
                                    .hint
                                    .map(|h| div().text_color(theme::dim()).child(h)),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("toast-dismiss-{id}")))
                            .flex_none()
                            .p(px(3.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::muted()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.shell.model.update(cx, |model, cx| {
                                    model.dismiss_toast(id);
                                    cx.notify();
                                });
                            }))
                            .child(glyph(Glyph::X, 12., theme::dim())),
                    )
            }))
    }

    /// The token field with its show and paste buttons. `failed` draws it in the error color.
    pub(super) fn token_field(
        &mut self,
        failed: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let focused = self.token.read(cx).focus_handle(cx).is_focused(window);
        let border = if failed {
            theme::alpha(theme::red(), 0.45)
        } else if focused {
            theme::ring()
        } else {
            theme::alpha(theme::fg(), 0.16)
        };
        let small_button = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .size(px(26.))
                .rounded(px(6.))
                .text_color(theme::dim())
                .cursor_pointer()
                .hover(|s| s.bg(theme::muted()))
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(38.))
            .pl(px(0.))
            .pr(px(6.))
            .gap(px(4.))
            .rounded(px(8.))
            .bg(theme::bg())
            .border_1()
            .border_color(border)
            .when(focused && !failed, |el| {
                el.shadow(vec![gpui_kit::BoxShadow {
                    color: theme::alpha(theme::ring(), 0.20),
                    offset: gpui_kit::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_family(theme::MONO)
                    .text_size(px(12.5))
                    .child(
                        Input::new(&self.token)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false),
                    ),
            )
            .child(
                small_button("token-toggle")
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_mask(window, cx)))
                    .child(glyph(
                        if self.token_masked {
                            Glyph::Eye
                        } else {
                            Glyph::EyeOff
                        },
                        14.,
                        theme::dim(),
                    )),
            )
            .child(
                small_button("token-paste")
                    .on_click(cx.listener(|this, _, window, cx| this.paste_token(window, cx)))
                    .child(glyph(Glyph::Clipboard, 14., theme::dim())),
            )
    }
}

/// The color of a notice level.
pub(super) fn level_color(level: Level) -> gpui_kit::Hsla {
    match level {
        Level::Info => theme::dim(),
        Level::Success => theme::green(),
        Level::Warning => theme::amber(),
        Level::Error => theme::red(),
    }
}

/// The icon of a notice level.
pub(super) fn level_glyph(level: Level) -> Glyph {
    match level {
        Level::Info => Glyph::Info,
        Level::Success => Glyph::CircleCheck,
        Level::Warning => Glyph::TriangleAlert,
        Level::Error => Glyph::CircleAlert,
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let screen = self.flow.screen().clone();
        let content = match screen {
            Screen::Choose => screens::choose(cx).into_any_element(),
            Screen::Searching => screens::searching(&self.local_endpoint(), cx).into_any_element(),
            Screen::LocalFound(session) => screens::local_found(&session, cx).into_any_element(),
            Screen::LocalNotFound(failure) => {
                screens::local_not_found(&failure, &self.local_endpoint(), cx).into_any_element()
            }
            Screen::Token { refused } => {
                screens::token(self, refused.as_ref(), window, cx).into_any_element()
            }
            Screen::Verifying { hint } => screens::verifying(&hint, cx).into_any_element(),
            Screen::Connected => screens::connected(self, cx).into_any_element(),
        };

        div()
            .id("app")
            .key_context("WyckApp")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.go_back(window, cx);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::fg())
            .font_family(theme::SANS)
            .text_size(px(13.))
            .child(titlebar(window))
            .child(self.banners(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(content)
                    .child(self.toasts(cx)),
            )
    }
}

impl AppView {
    fn local_endpoint(&self) -> String {
        self.shell.controller.local_endpoint().to_owned()
    }
}
