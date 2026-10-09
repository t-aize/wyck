//! The one modal window of the app, for the settings panels.
//!
//! It is centered in the window over a dimmed veil. Opening fades it in while it rises a few
//! pixels, closing fades it out, both short. Escape and a click on the veil dismiss it (a panel
//! can say a click on the veil does not), and the focus goes back to where it was.
//!
//! The header of a panel is its handle: dragging it moves the panel anywhere in the window (see
//! [`begin_drag`]). The position is forgotten when the next modal opens.
//!
//! Only one modal is open at a time: opening another replaces the first at once.
//!
//! For the keyboard: Tab and Shift+Tab cycle inside the panel and never reach what is behind it
//! (a focus trap), and on opening the focus moves to the first control of the panel, unless the
//! panel already took it (a search field, for instance). For a screen reader the panel is a
//! dialog, with the name the caller gave it in [`Options::label`].
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
    KeyBinding, MouseButton, MouseMoveEvent, Pixels, Point, Role, SharedString, Window, actions,
    div, point, px, relative,
};
use gpui_kit::component::FocusTrapElement as _;

use crate::ui::kit::anim::{self, ease_out_cubic};

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

/// The share of the window a panel may fill, as in the render below.
const MAX_WIDTH_SHARE: f32 = 0.94;
const MAX_HEIGHT_SHARE: f32 = 0.92;
/// How much of a dragged panel stays inside the window sideways, and how low its top edge may go
/// (enough for the header, so it can always be grabbed back).
const KEEP_VISIBLE: f32 = 80.0;
const KEEP_HEADER: f32 = 56.0;

/// How long after opening the first control gets the focus: the panel is drawn by then.
const FOCUS_AFTER: Duration = Duration::from_millis(60);
/// The most controls looked at to find the first one inside the panel.
const MAX_TAB_WALK: usize = 200;

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
    /// What a screen reader calls the panel.
    pub label: SharedString,
}

impl Options {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            on_dismiss: None,
            dismiss_on_veil: false,
            label: "Dialog".into(),
        }
    }

    /// Names the panel for a screen reader: `Settings`, `Edit alert`.
    #[must_use]
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
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

/// A drag of the header: where the mouse went down, and where the panel was then.
struct Drag {
    origin: Point<Pixels>,
    start: Point<Pixels>,
}

pub struct ModalHost {
    shown: Option<Shown>,
    /// How far the panel has been moved from the center of the window.
    offset: Point<Pixels>,
    drag: Option<Drag>,
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
        offset: point(px(0.), px(0.)),
        drag: None,
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

/// Starts moving the panel: call it when the mouse goes down on the header, with its position. The
/// host follows the mouse from there until the button is released.
pub fn begin_drag(position: Point<Pixels>, cx: &mut App) {
    host(cx).update(cx, |host, cx| host.begin_drag(position, cx));
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
        self.offset = point(px(0.), px(0.));
        self.drag = None;
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
        self.focus_first_control(self.next_id, window, cx);
        cx.notify();
    }

    /// Once the panel has been drawn (its controls are known only then), moves the focus to its
    /// first control, unless the panel or the user already put it somewhere inside.
    fn focus_first_control(&self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(FOCUS_AFTER).await;
            this.update_in(cx, |this, window, cx| {
                let still_this = this
                    .shown
                    .as_ref()
                    .is_some_and(|s| s.id == id && !s.leaving);
                // Only when the focus is still on the host itself.
                if still_this && this.focus.is_focused(window) {
                    this.focus_into_panel(window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Moves the focus to the next control that is inside the panel. Controls behind it are
    /// skipped; the walk is bounded, so a panel with nothing to focus does no harm.
    fn focus_into_panel(&self, window: &mut Window, cx: &mut Context<Self>) {
        let start = window.focused(cx);
        for _ in 0..MAX_TAB_WALK {
            window.focus_next(cx);
            if self.focus.within_focused(window, cx) {
                return;
            }
            if window.focused(cx) == start {
                break;
            }
        }
        // Nothing to reach: the host keeps the focus, so Escape still works.
        window.focus(&self.focus, cx);
    }

    fn begin_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.shown.as_ref().is_some_and(|s| !s.leaving) {
            self.drag = Some(Drag {
                origin: position,
                start: self.offset,
            });
            cx.notify();
        }
    }

    /// Follows the mouse while the header is held, keeping the panel reachable: its top edge in
    /// the window, its header above the bottom, and some of its width inside.
    fn drag_to(&mut self, event: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.as_ref() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            // The button was released where nobody heard it (outside the window).
            self.drag = None;
            cx.notify();
            return;
        }
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let view = window.viewport_size();
        let (vw, vh) = (f32::from(view.width), f32::from(view.height));
        let pw = shown.options.width.min(vw * MAX_WIDTH_SHARE);
        let ph = shown.options.height.min(vh * MAX_HEIGHT_SHARE);
        let reach_x = ((vw + pw) / 2.0 - KEEP_VISIBLE).max(0.0);
        let min_y = -(vh - ph) / 2.0;
        let max_y = ((vh + ph) / 2.0 - KEEP_HEADER).max(min_y);
        let dx = f32::from(drag.start.x) + f32::from(event.position.x - drag.origin.x);
        let dy = f32::from(drag.start.y) + f32::from(event.position.y - drag.origin.y);
        self.offset = point(px(dx.clamp(-reach_x, reach_x)), px(dy.clamp(min_y, max_y)));
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
        let offset = self.offset;
        let dragging = self.drag.is_some();
        // Each phase has its own animation id, so entering and leaving each play from the start.
        let phase = id * 2 + u64::from(leaving);
        let duration = if !anim::enabled() {
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
            .bg(crate::ui::kit::theme::veil())
            .with_animation(
                ("modal-veil", phase),
                Animation::new(duration),
                move |el, t| el.opacity(progress(t)),
            );

        let panel: AnyElement = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .max_w(relative(MAX_WIDTH_SHARE))
            .max_h(relative(MAX_HEIGHT_SHARE))
            // The panel takes its clicks: the veil behind it must not.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("wyck-modal-dialog")
                    .role(Role::Dialog)
                    .aria_label(shown.options.label.clone())
                    .size_full()
                    .child(shown.view.clone()),
            )
            .with_animation(
                ("modal-panel", phase),
                Animation::new(duration),
                move |el, t| {
                    let t = progress(t);
                    el.opacity(t)
                        .left(offset.x)
                        .top(offset.y + px((1.0 - t) * RISE))
                },
            )
            .into_any_element();

        div()
            .key_context("Modal")
            .on_action(cx.listener(|this, _: &CloseModal, window, cx| this.dismiss(window, cx)))
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.drag_to(event, window, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag = None;
                    cx.notify();
                }),
            )
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
            // While held, a clear sheet over everything gives the closed hand, over the header's
            // open one.
            .when(dragging, |el| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .cursor_grabbing(),
                )
            })
            // Tab stays inside the panel: it is the one thing that can be operated.
            .focus_trap("wyck-modal-trap", &self.focus)
            .into_any_element()
    }
}
