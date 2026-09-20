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

use gpui_kit::base::animation::Lerp as _;
use gpui_kit::base::{MotionReveal, Presence, PresencePhase, Transition, TransitionId, transition};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, Entity, FocusHandle, Focusable as _, KeyDownEvent,
    SharedString, Stateful, Subscription, Task, Window, div, px,
};
use secrecy::SecretString;
use wyck_engine::broker::{ConnectRequest, ServiceKind};
use wyck_engine::domain::now_millis;

use super::motion::{self, Direction, Hover};
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

/// How often toasts are checked for expiry while there are some. It is also the slack before a
/// toast that has faded out is removed, so it stays well under the fade itself.
const TOAST_TICK: Duration = Duration::from_millis(100);

/// One screen as the window keeps it while it is on screen or on its way out. `key` is unique per
/// change of screen and scopes the animation state of everything inside.
#[derive(Clone)]
struct Layer {
    key: usize,
    screen: Screen,
}

/// The root view of the main window.
pub struct AppView {
    pub(super) shell: Shell,
    pub(super) flow: ConnectFlow,
    pub(super) token: Entity<InputState>,
    pub(super) token_masked: bool,
    pub(super) token_error: Option<TokenError>,
    /// How many times the token field has refused what was typed. Each new value shakes it once.
    pub(super) shake: u32,
    /// The screen on show.
    layer: Layer,
    /// The screen that was on show before the last change, until its exit has played.
    outgoing: Option<Layer>,
    /// Which way the last change of screen went.
    direction: Direction,
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
        let subscriptions = vec![
            cx.subscribe_in(&token, window, |this, _, event: &InputEvent, window, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.submit_token(window, cx),
                    InputEvent::Change if this.token_error.take().is_some() => cx.notify(),
                    // The field's ring fades with its focus, so it has to be drawn again.
                    InputEvent::Focus | InputEvent::Blur => cx.notify(),
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
            shake: 0,
            layer: Layer {
                key: 0,
                screen: Screen::Choose,
            },
            outgoing: None,
            direction: Direction::Forward,
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
        view.layer.screen = view.flow.screen().clone();
        view.settle_focus(window, cx);
        view
    }

    /// Follows the flow: when it has moved to another screen, the one on show becomes the
    /// outgoing one and a new layer, with a new key, takes its place. A change inside the same
    /// screen (a refused token on the form) only updates the data.
    fn sync_layers(&mut self) {
        let now = self.flow.screen();
        if motion::same_page(&self.layer.screen, now) {
            self.layer.screen = now.clone();
            return;
        }
        self.direction = motion::direction(&self.layer.screen, now);
        let next = Layer {
            key: self.layer.key + 1,
            screen: now.clone(),
        };
        self.outgoing = Some(std::mem::replace(&mut self.layer, next));
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
                self.shake += 1;
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

    /// The banners under the title bar. A banner opens like a drawer the first time it shows.
    fn banners(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let banners = self.shell.model.read(cx).banners.clone();
        let items: Vec<AnyElement> = banners
            .into_iter()
            .enumerate()
            .map(|(index, banner)| {
                let color = level_color(banner.level);
                let text = match (&banner.detail, banner.hint) {
                    (Some(detail), Some(hint)) => format!("{detail} {hint}"),
                    (Some(detail), None) => detail.clone(),
                    (None, Some(hint)) => hint.to_owned(),
                    (None, None) => String::new(),
                };
                let dismiss = icon_button(
                    SharedString::from(format!("banner-dismiss-{index}")),
                    window,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.shell.model.update(cx, |model, cx| {
                        model.dismiss_banner(index);
                        cx.notify();
                    });
                }))
                .child(glyph(Glyph::X, 12., theme::dim()));
                let progress = Presence::new(TransitionId::from((index, "banner")), true)
                    .transition(Transition::new(motion::SLOW).easing(motion::enter()))
                    .sample(window, cx)
                    .progress;
                let strip = div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(9.))
                    .opacity(progress)
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
                    .child(dismiss);
                MotionReveal::new(("banner", index), progress, strip.into_any_element())
                    .into_any_element()
            })
            .collect();
        div().flex().flex_col().flex_none().children(items)
    }

    /// The toasts at the bottom right. A toast rises 10 px as it fades in, and sinks as it fades
    /// out when it expires or is dismissed.
    fn toasts(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toasts = self.shell.model.read(cx).toasts.clone();
        let items: Vec<AnyElement> = toasts
            .into_iter()
            .filter_map(|toast| {
                let present = toast.leaving.is_none();
                let transition = if present {
                    Transition::new(motion::SLOW).easing(motion::enter())
                } else {
                    motion::screen_out()
                };
                let sample =
                    Presence::new(TransitionId::from((toast.id as usize, "toast")), present)
                        .transition(transition)
                        .sample(window, cx);
                if !sample.should_render() {
                    return None;
                }
                let progress = sample.progress;
                let color = level_color(toast.notice.level);
                let id = toast.id;
                let dismiss = icon_button(
                    SharedString::from(format!("toast-dismiss-{id}")),
                    window,
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.shell.model.update(cx, |model, cx| {
                        model.dismiss_toast(id, now_millis());
                        cx.notify();
                    });
                }))
                .child(glyph(Glyph::X, 12., theme::dim()));
                Some(
                    div()
                        .relative()
                        .top(px((1.0 - progress) * 10.0))
                        .opacity(progress)
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
                        .child(dismiss)
                        .into_any_element(),
                )
            })
            .collect();
        div()
            .absolute()
            .bottom(px(16.))
            .right(px(16.))
            .w(px(330.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .children(items)
    }

    /// The token field with its show and paste buttons. `failed` draws it in the error color.
    ///
    /// Its border and glow fade in and out with the focus. `live` is false for the copy of the
    /// screen that is on its way out: it shows the text as a plain label instead of a second
    /// input.
    pub(super) fn token_field(
        &mut self,
        failed: bool,
        live: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let focused = live && self.token.read(cx).focus_handle(cx).is_focused(window);
        let glow = transition(
            TransitionId::from("token-focus"),
            if focused { 1.0_f32 } else { 0.0 },
            motion::quick(),
            window,
            cx,
        );
        let idle = theme::alpha(theme::fg(), 0.16);
        let border = if failed {
            theme::alpha(theme::red(), 0.45)
        } else {
            idle.lerp(&theme::ring(), glow)
        };
        let field: AnyElement = if live {
            Input::new(&self.token)
                .appearance(false)
                .bordered(false)
                .focus_bordered(false)
                .into_any_element()
        } else {
            // The exit copy: the same text, masked the same way, without a second input.
            let value = self.token.read(cx).value();
            let (text, color) = if value.is_empty() {
                (SharedString::from("Paste your API token"), theme::dim())
            } else if self.token_masked {
                (
                    SharedString::from("\u{2022}".repeat(value.chars().count())),
                    theme::fg(),
                )
            } else {
                (value, theme::fg())
            };
            div()
                .pl(px(11.))
                .text_color(color)
                .child(text)
                .into_any_element()
        };
        let toggle = icon_button("token-toggle", window, cx)
            .on_click(cx.listener(|this, _, window, cx| this.toggle_mask(window, cx)))
            .child(glyph(
                if self.token_masked {
                    Glyph::Eye
                } else {
                    Glyph::EyeOff
                },
                14.,
                theme::dim(),
            ));
        let paste = icon_button("token-paste", window, cx)
            .on_click(cx.listener(|this, _, window, cx| this.paste_token(window, cx)))
            .child(glyph(Glyph::Clipboard, 14., theme::dim()));
        div()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(px(38.))
            .pr(px(6.))
            .gap(px(4.))
            .rounded(px(8.))
            .bg(theme::bg())
            .border_1()
            .border_color(border)
            .when(!failed && glow > 0.0, |el| {
                el.shadow(vec![gpui_kit::BoxShadow {
                    color: theme::alpha(theme::ring(), 0.20 * glow),
                    offset: gpui_kit::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.0 * glow),
                    inset: false,
                }])
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .font_family(theme::MONO)
                    .text_size(px(12.5))
                    .child(field),
            )
            .child(toggle)
            .child(paste)
    }

    /// One screen, placed and faded for its moment of the change. `arriving` is the incoming
    /// screen, and `progress` is 0 when it is gone and 1 when it is settled.
    fn layer_element(
        &mut self,
        layer: &Layer,
        progress: f32,
        arriving: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let content = match &layer.screen {
            Screen::Choose => screens::choose(window, cx).into_any_element(),
            Screen::Searching => {
                screens::searching(&self.local_endpoint(), window, cx).into_any_element()
            }
            Screen::LocalFound(session) => {
                screens::local_found(session, window, cx).into_any_element()
            }
            Screen::LocalNotFound(failure) => {
                screens::local_not_found(failure, &self.local_endpoint(), window, cx)
                    .into_any_element()
            }
            Screen::Token { refused } => {
                screens::token(self, refused.as_ref(), arriving, window, cx).into_any_element()
            }
            Screen::Verifying { hint } => screens::verifying(hint, window, cx).into_any_element(),
            Screen::Connected => screens::connected(self, window, cx).into_any_element(),
        };
        div()
            .id(("layer", layer.key))
            .absolute()
            .top_0()
            .size_full()
            .left(px(motion::slide(self.direction, arriving, progress)))
            .opacity(progress)
            .child(content)
            .into_any_element()
    }
}

/// A small square button with an icon, whose ground fades in under the pointer.
fn icon_button(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Stateful<Div> {
    let id: ElementId = id.into();
    let hover = Hover::track(id.clone(), window, cx);
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(26.))
        .rounded(px(6.))
        .bg(hover.mix(theme::alpha(theme::muted(), 0.0), theme::muted()))
        .cursor_pointer()
        .on_hover(hover.handler())
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
        self.sync_layers();

        // The screen that is leaving, if one is: it plays its exit and is dropped once it is gone.
        let mut layers: Vec<AnyElement> = Vec::new();
        let mut changing = false;
        if let Some(outgoing) = self.outgoing.clone() {
            let sample = Presence::new(TransitionId::from((outgoing.key, "layer")), false)
                .transition(motion::screen_out())
                .sample(window, cx);
            if sample.should_render() {
                changing = true;
                layers.push(self.layer_element(&outgoing, sample.progress, false, window, cx));
            } else {
                self.outgoing = None;
            }
        }

        // The screen that is arriving, or settled.
        let sample = Presence::new(TransitionId::from((self.layer.key, "layer")), true)
            .transition(motion::screen_in())
            .sample(window, cx);
        changing |= sample.phase != PresencePhase::Present;
        let current = self.layer.clone();
        layers.push(self.layer_element(&current, sample.progress, true, window, cx));

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
            .child(titlebar(window, cx))
            .child(self.banners(window, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .children(layers)
                    // While a screen is moving, neither takes a click: the leaving one is on its
                    // way out and the arriving one is not yet where it will be.
                    .when(changing, |el| {
                        el.child(div().absolute().inset_0().occlude())
                    })
                    .child(self.toasts(window, cx)),
            )
    }
}

impl AppView {
    fn local_endpoint(&self) -> String {
        self.shell.controller.local_endpoint().to_owned()
    }
}
