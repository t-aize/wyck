//! The one modal window of the app, for the settings panels.
//!
//! It is centered in the window over a dimmed veil. Opening fades it in while it rises a few
//! pixels, closing fades it out, both short. Escape and a click on the veil dismiss it (a panel
//! can say a click on the veil does not), and the focus goes back to where it was.
//!
//! Only one modal is open at a time: opening another replaces the first at once.
//!
//! Every absolute box here is anchored to the corner with `top_0` and `left_0`: without an offset it
//! would sit where it would have been in the flow, which for the panel is below the veil, off the
//! window.
//!
//! The content is any view. Its root must fill the space it is given (`size_full`), since the
//! modal decides the size, clamped to what the window has.

use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Animation, AnimationExt as _, AnyElement, AnyView, App, Context, Entity, FocusHandle, Global,
    KeyBinding, MouseButton, Window, actions, div, px, relative, rgba,
};

use super::anim::ease_out_cubic;

actions!(wyck_modal, [CloseModal]);

/// How long the panel takes to appear and to leave.
const ENTER: Duration = Duration::from_millis(180);
const EXIT: Duration = Duration::from_millis(120);
/// How far the panel rises while it appears, in pixels.
const RISE: f32 = 10.0;

/// Registers the key bindings of the modal. Call once, at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseModal, Some("Modal"))]);
}

/// What a panel is told when it is dismissed.
type DismissFn = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a modal asks for.
pub struct Options {
    /// The size it would like, in pixels. A small window gets less.
    pub width: f32,
    pub height: f32,
    /// Called when Escape, the veil or the close button dismisses it (not when the panel closes
    /// itself with [`close`]), before it starts to leave.
    pub on_dismiss: Option<DismissFn>,
    /// Whether a click on the veil dismisses it.
    pub dismiss_on_veil: bool,
}

impl Options {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            on_dismiss: None,
            dismiss_on_veil: false,
        }
    }

    #[must_use]
    pub fn on_dismiss(mut self, on_dismiss: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(on_dismiss));
        self
    }
}

struct Shown {
    /// Tells one opening from the next, so their animations start again.
    id: u64,
    view: AnyView,
    options: Options,
    leaving: bool,
    previous_focus: Option<FocusHandle>,
}

pub struct ModalHost {
    shown: Option<Shown>,
    next_id: u64,
    focus: FocusHandle,
}

struct Handle(Entity<ModalHost>);

impl Global for Handle {}

/// The host that draws the modal: put it once in the window, over everything but the notices.
pub fn host(cx: &mut App) -> Entity<ModalHost> {
    if let Some(handle) = cx.try_global::<Handle>() {
        return handle.0.clone();
    }
    let host = cx.new(|cx| ModalHost {
        shown: None,
        next_id: 0,
        focus: cx.focus_handle(),
    });
    cx.set_global(Handle(host.clone()));
    host
}

/// Opens `view` in the modal.
pub fn open(view: impl Into<AnyView>, options: Options, window: &mut Window, cx: &mut App) {
    let view = view.into();
    host(cx).update(cx, |host, cx| host.open(view, options, window, cx));
}

/// What Escape does: tells the panel through its `on_dismiss`, then closes. For the close button
/// of a panel that keeps its changes when it is dismissed.
pub fn dismiss(window: &mut Window, cx: &mut App) {
    host(cx).update(cx, |host, cx| host.dismiss(window, cx));
}

/// Closes the modal, without the `on_dismiss` of its options (the panel is already done).
pub fn close(window: &mut Window, cx: &mut App) {
    host(cx).update(cx, |host, cx| host.leave(window, cx));
}

impl ModalHost {
    fn open(
        &mut self,
        view: AnyView,
        options: Options,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.next_id += 1;
        // Focus goes back to where it was before the first modal, not before a replacement.
        let previous_focus = match self.shown.take() {
            Some(old) => old.previous_focus,
            None => window.focused(cx),
        };
        self.shown = Some(Shown {
            id: self.next_id,
            view,
            options,
            leaving: false,
            previous_focus,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Escape, the veil or the close button: tells the panel, then leaves.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_ref().filter(|s| !s.leaving) else {
            return;
        };
        if let Some(on_dismiss) = shown.options.on_dismiss.clone() {
            on_dismiss(window, cx);
        }
        self.leave(window, cx);
    }

    /// Starts the exit, then removes the modal once it has played.
    fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut().filter(|s| !s.leaving) else {
            return;
        };
        shown.leaving = true;
        let id = shown.id;
        if let Some(previous) = shown.previous_focus.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(EXIT).await;
            this.update(cx, |this, cx| {
                // A modal opened meanwhile is not this one.
                if this.shown.as_ref().is_some_and(|s| s.id == id) {
                    this.shown = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

impl Render for ModalHost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(shown) = self.shown.as_ref() else {
            return div().into_any_element();
        };
        let (id, leaving) = (shown.id, shown.leaving);
        let (width, height) = (shown.options.width, shown.options.height);
        let dismiss_on_veil = shown.options.dismiss_on_veil;
        // Each phase has its own animation id, so entering and leaving each play from the start.
        let phase = id * 2 + u64::from(leaving);
        let duration = if !super::appearance::animations() {
            Duration::from_millis(1)
        } else if leaving {
            EXIT
        } else {
            ENTER
        };
        let progress = move |t: f32| {
            let t = ease_out_cubic(t);
            if leaving { 1.0 - t } else { t }
        };

        let veil = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0x0000_00a6))
            .with_animation(
                ("modal-veil", phase),
                Animation::new(duration),
                move |el, t| el.opacity(progress(t)),
            );

        let panel: AnyElement = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .max_w(relative(0.94))
            .max_h(relative(0.92))
            // The panel takes its clicks: the veil behind it must not.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(shown.view.clone())
            .with_animation(
                ("modal-panel", phase),
                Animation::new(duration),
                move |el, t| {
                    let t = progress(t);
                    el.opacity(t).top(px((1.0 - t) * RISE))
                },
            )
            .into_any_element();

        div()
            .key_context("Modal")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| this.dismiss(window, cx)))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(div().size_full().child(veil).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if dismiss_on_veil {
                        this.dismiss(window, cx);
                    }
                }),
            ))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(panel),
            )
            .into_any_element()
    }
}
