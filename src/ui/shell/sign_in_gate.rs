//! The sign-in modal: one card over the app that stays until there is a session.
//!
//! The phases and their words come from [`crate::app::sign_in`]. This view draws them, keeps what
//! the person typed, and does nothing else. Escape and a click on the veil never close it: during
//! a step they cancel the step, otherwise the card shakes.

use gpui::prelude::*;
use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, Focusable, KeyBinding, KeyDownEvent,
    SharedString, Subscription, Window, div, px,
};

use crate::app::broker::Environment;
use crate::app::sign_in::rules::redirect_uri;
use crate::app::sign_in::{AccountChoice, CALLBACK_PORT, Failure, Phase, SignIn};
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::{self, InputEvent, InputState};
use crate::ui::kit::prelude::Disableable;
use crate::ui::kit::{anim, button, controls, field, icon, layout, theme, tokens};

gpui::actions!(wyck_sign_in, [Dismiss]);

/// Registers the key binding of the modal. Call once, at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", Dismiss, Some("SignInGate"))]);
}

/// The modal.
pub struct SignInGate {
    sign_in: Entity<SignIn>,
    client_id: Entity<InputState>,
    client_secret: Entity<InputState>,
    environment: Environment,
    remember: bool,
    help_open: bool,
    copied: bool,
    /// Counts refused dismissals, so each one replays the shake.
    shakes: u64,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SignInGate {
    /// A modal for `sign_in`.
    pub fn new(sign_in: Entity<SignIn>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let client_id =
            cx.new(|cx| InputState::new(window, cx).placeholder("e.g. 3492_oXQnP7vLk9fZ2mR8bQwT"));
        let client_secret = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("your application's client secret")
                .masked(true)
        });
        let mut subscriptions = vec![cx.observe_in(&sign_in, window, |this, _, window, cx| {
            this.settle_focus(window, cx);
            cx.notify();
        })];
        for state in [&client_id, &client_secret] {
            subscriptions.push(cx.subscribe(state, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit(cx);
                }
            }));
        }
        let mut this = Self {
            sign_in,
            client_id,
            client_secret,
            environment: Environment::Demo,
            remember: true,
            help_open: false,
            copied: false,
            shakes: 0,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.prefill(false, window, cx);
        this
    }

    /// Fills the form from the saved sign-in, when there is one. The secret is filled too only
    /// when `with_secret` is set, for a session that ended and has to be consented again.
    pub fn prefill(&mut self, with_secret: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (id, secret, environment) = {
            let sign_in = self.sign_in.read(cx);
            (
                sign_in.saved_client_id(),
                if with_secret {
                    sign_in.saved_secret()
                } else {
                    None
                },
                sign_in.saved_environment(),
            )
        };
        if let Some(environment) = environment {
            self.environment = environment;
        }
        self.client_id.update(cx, |input, cx| {
            input.set_value(id.unwrap_or_default(), window, cx);
        });
        self.client_secret.update(cx, |input, cx| {
            input.set_value(secret.unwrap_or_default(), window, cx);
        });
        self.settle_focus(window, cx);
        cx.notify();
    }

    /// Puts the focus where typing or Escape will land.
    fn settle_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editing = *self.sign_in.read(cx).state().phase() == Phase::Editing;
        if editing {
            let id_empty = self.client_id.read(cx).value().trim().is_empty();
            let target = if id_empty {
                &self.client_id
            } else {
                &self.client_secret
            };
            target.update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.focus_handle, cx);
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let id = self.client_id.read(cx).value().to_string();
        let secret = self.client_secret.read(cx).value().to_string();
        let (environment, remember) = (self.environment, self.remember);
        self.sign_in.update(cx, |sign_in, cx| {
            sign_in.submit(&id, &secret, environment, remember, cx);
        });
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        let busy = self.sign_in.read(cx).state().is_busy();
        if busy {
            self.sign_in.update(cx, |sign_in, cx| sign_in.cancel(cx));
        } else {
            self.shakes += 1;
            cx.notify();
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let phase = self.sign_in.read(cx).state().phase().clone();
        if let Phase::ChoosingAccount { accounts, selected } = phase {
            let last = accounts.len().saturating_sub(1);
            match event.keystroke.key.as_str() {
                "down" => self
                    .sign_in
                    .update(cx, |s, cx| s.choose((selected + 1).min(last), cx)),
                "up" => self
                    .sign_in
                    .update(cx, |s, cx| s.choose(selected.saturating_sub(1), cx)),
                "enter" => self.sign_in.update(cx, |s, cx| s.connect_chosen(cx)),
                _ => {}
            }
        }
    }

    fn heading(
        &self,
        name: IconName,
        title: &'static str,
        subtitle: &'static str,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .child(layout::icon_tile(
                name,
                48.,
                22.,
                theme::accent_selected(),
                theme::fg(),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(tokens::text::display()))
                            .text_color(theme::fg())
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::muted_fg())
                            .child(subtitle),
                    ),
            )
    }

    fn waiting_line(&self, id: &'static str, text: impl Into<SharedString>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .text_size(px(tokens::text::emphasis()))
            .text_color(theme::accent())
            .child(anim::spin(
                icon::tinted(IconName::LoaderCircle, 15., theme::accent()),
                id,
            ))
            .child(text.into())
    }

    fn restoring(&self) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .py_6()
            .child(self.waiting_line("gate-restore-spin", "Restoring your session"))
            .into_any_element()
    }

    fn form(
        &self,
        failure: Option<&Failure>,
        failures: u64,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let this = cx.entity();
        let live = self.environment == Environment::Live;
        let env_index = usize::from(live);
        let environment = controls::segmented("gate-environment", &["Demo", "Live"], env_index, {
            let this = this.clone();
            move |index, _window, cx| {
                this.update(cx, |gate, cx| {
                    gate.environment = if index == 1 {
                        Environment::Live
                    } else {
                        Environment::Demo
                    };
                    cx.notify();
                });
            }
        });
        let help = self.help_open;
        let redirect = redirect_uri(CALLBACK_PORT);
        let copied = self.copied;
        let submit_label = if busy { "Verifying..." } else { "Sign in" };

        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.heading(
                IconName::KeyRound,
                "Sign in to cTrader",
                "Use your own cTrader Open API application. Wyck keeps the sign-in on this device only.",
            ))
            .children(failure.map(|failure| {
                layout::error_banner(
                    "sign-in-error",
                    failures,
                    failure.title(),
                    failure.message(),
                )
            }))
            .child(layout::stacked_field(
                IconName::Globe,
                "Environment",
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(environment)
                    .children(live.then(|| layout::environment_badge(true))),
            ))
            .child(layout::stacked_field(
                IconName::Hash,
                "Client ID",
                input::text(&self.client_id).disabled(busy),
            ))
            .child(layout::stacked_field(
                IconName::Lock,
                "Client secret",
                input::text(&self.client_secret).mask_toggle().disabled(busy),
            ))
            .child(field::check(
                "gate-remember",
                "Stay signed in on this device",
                self.remember,
                {
                    let this = this.clone();
                    move |value, _window, cx| {
                        this.update(cx, |gate, cx| {
                            gate.remember = value;
                            cx.notify();
                        });
                    }
                },
            ))
            .child(
                button::quiet("gate-help")
                    .icon(IconName::Info)
                    .label("Where do I find these?")
                    .on_click({
                        let this = this.clone();
                        move |_, _window, cx| {
                            this.update(cx, |gate, cx| {
                                gate.help_open = !gate.help_open;
                                cx.notify();
                            });
                        }
                    }),
            )
            .children(help.then(|| {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child(
                        "Create an application on cTrader Connect and copy its Client ID and secret. \
                         Add this exact address to its redirect list; Wyck listens on it locally to \
                         catch the answer.",
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(div().flex_1().truncate().text_color(theme::fg()).child(redirect.clone()))
                            .child(
                                button::quiet("gate-copy-redirect")
                                    .icon(if copied { IconName::Check } else { IconName::Copy })
                                    .label(if copied { "Copied" } else { "Copy" })
                                    .on_click({
                                        let this = this.clone();
                                        move |_, _window, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(redirect.clone()));
                                            this.update(cx, |gate, cx| {
                                                gate.copied = true;
                                                cx.notify();
                                            });
                                        }
                                    }),
                            ),
                    )
            }))
            .child(
                button::primary(
                    "gate-submit",
                    submit_label,
                    cx.listener(|this, _, _window, cx| this.submit(cx)),
                )
                .loading(busy)
                .disabled(busy),
            )
            .child(div().flex().justify_center().child(if busy {
                button::quiet("gate-cancel")
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _window, cx| this.dismiss(cx)))
            } else {
                button::quiet("gate-quit")
                    .icon(IconName::Power)
                    .label("Quit Wyck")
                    .on_click(|_, _window, cx| cx.quit())
            }))
            .into_any_element()
    }

    fn waiting_browser(&self, url: String, cx: &mut Context<Self>) -> gpui::AnyElement {
        let open = url.clone();
        let copy = url;
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.heading(
                IconName::Globe,
                "Finish this in your browser",
                "We opened cTrader's sign-in page. Allow access there; Wyck picks it up here by itself.",
            ))
            .child(self.waiting_line("gate-wait-spin", "Waiting for cTrader..."))
            .child(
                button::outlined("gate-reopen")
                    .icon(IconName::ExternalLink)
                    .label("Open the page again")
                    .on_click(move |_, _window, cx| cx.open_url(&open)),
            )
            .child(
                button::quiet("gate-copy-link")
                    .icon(IconName::Copy)
                    .label("Copy the link")
                    .on_click(move |_, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                    }),
            )
            .child(
                button::quiet("gate-cancel-browser")
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _window, cx| this.dismiss(cx))),
            )
            .into_any_element()
    }

    fn choosing(
        &self,
        accounts: &[AccountChoice],
        selected: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = accounts.iter().enumerate().map(|(index, account)| {
            let chosen = index == selected;
            div()
                .id(("gate-account", index))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .h(px(tokens::height::large()))
                .rounded_lg()
                .border_1()
                .border_color(if chosen {
                    theme::accent()
                } else {
                    theme::border_subtle()
                })
                .bg(if chosen {
                    theme::accent_selected()
                } else {
                    theme::bg()
                })
                .text_size(px(tokens::text::emphasis()))
                .text_color(theme::fg())
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _window, cx| {
                    this.sign_in.update(cx, |s, cx| s.choose(index, cx));
                }))
                .child(icon::tinted(IconName::Wallet, 15., theme::muted_fg()))
                .child(div().flex_1().truncate().child(account.label.clone()))
                .child(layout::environment_badge(account.is_live))
        });
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(self.heading(
                IconName::Wallet,
                "Choose a trading account",
                "These accounts fit the environment you chose. Pick the one to trade on.",
            ))
            .child(div().flex().flex_col().gap_2().children(rows))
            .child(
                button::primary(
                    "gate-connect-account",
                    "Connect this account",
                    cx.listener(|this, _, _window, cx| {
                        this.sign_in.update(cx, |s, cx| s.connect_chosen(cx));
                    }),
                )
                .icon(IconName::PlugZap),
            )
            .child(
                div().flex().justify_center().child(
                    button::quiet("gate-cancel-choice")
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _window, cx| this.dismiss(cx))),
                ),
            )
            .into_any_element()
    }

    fn authorizing(&self, label: &str) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_4()
            .py_6()
            .child(self.waiting_line("gate-authorize-spin", format!("Connecting {label}")))
            .into_any_element()
    }
}

impl Focusable for SignInGate {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SignInGate {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (phase, failure, failures, busy) = {
            let state = self.sign_in.read(cx).state();
            (
                state.phase().clone(),
                state.failure().cloned(),
                state.failures(),
                state.is_busy(),
            )
        };
        let body = match phase {
            Phase::Restoring => self.restoring(),
            Phase::Editing => self.form(failure.as_ref(), failures, false, cx),
            Phase::Verifying => self.form(failure.as_ref(), failures, busy, cx),
            Phase::WaitingBrowser { url } => self.waiting_browser(url, cx),
            Phase::ChoosingAccount { accounts, selected } => self.choosing(&accounts, selected, cx),
            Phase::Authorizing { label } => self.authorizing(&label),
            Phase::Connected => div().into_any_element(),
        };
        let card = layout::card().max_w_full().child(body);
        let card = if self.shakes > 0 {
            anim::shake(card, ("gate-shake", self.shakes)).into_any_element()
        } else {
            card.into_any_element()
        };

        div()
            .key_context("SignInGate")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Dismiss, _window, cx| this.dismiss(cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                this.key_down(event, cx);
            }))
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::lock_veil())
            .child(card)
    }
}
