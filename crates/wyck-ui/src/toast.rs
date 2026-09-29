//! Short notices in the corner of the window: an order filled, an alert triggered, a picture
//! saved, something that failed.
//!
//! A notice says what happened in a title and a sentence. It can add what to do about it (a
//! hint), a button that does it (an action) and, for a failure, the exact words behind it that
//! can be copied for a bug report. What went wrong stays on screen until it is closed; what went
//! right goes away by itself. The same notice raised twice shows once.
//!
//! They are gpui-component notifications, pushed to the window from anywhere with only an
//! [`App`]: the push waits until the current update is over (a notice is often raised from inside
//! an event handler, while the window is busy), then goes to the active window.

use std::cell::Cell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{App, ClipboardItem, SharedString, Window, div, px};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::component::{Sizable, StyledExt as _, WindowExt};

use crate::{theme, tokens};

/// What a notice is about, which sets its icon and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Info,
    Success,
    Warning,
    Error,
}

/// A message longer than this stays until it is closed: it takes longer than a few seconds to read.
const LONG_MESSAGE: usize = 140;

/// What a button of a notice does.
type OnAction = Rc<dyn Fn(&mut Window, &mut App)>;

/// Stands for the notices of this module, so that two of them with the same words are the same one.
struct ToastKey;

/// A notice, built up and then shown with [`Toast::show`].
///
/// ```ignore
/// Toast::error("The order was not sent", "Not enough free margin for this order.")
///     .hint("Lower the size, or close a position to free some margin.")
///     .details("NOT_ENOUGH_MONEY")
///     .show(cx);
/// ```
pub struct Toast {
    kind: Kind,
    title: SharedString,
    message: SharedString,
    hint: Option<SharedString>,
    details: Option<SharedString>,
    actions: Vec<(SharedString, OnAction)>,
    sticky: Option<bool>,
}

impl Toast {
    pub fn new(
        kind: Kind,
        title: impl Into<SharedString>,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            message: message.into(),
            hint: None,
            details: None,
            actions: Vec::new(),
            sticky: None,
        }
    }

    pub fn info(title: impl Into<SharedString>, message: impl Into<SharedString>) -> Self {
        Self::new(Kind::Info, title, message)
    }

    pub fn success(title: impl Into<SharedString>, message: impl Into<SharedString>) -> Self {
        Self::new(Kind::Success, title, message)
    }

    pub fn warning(title: impl Into<SharedString>, message: impl Into<SharedString>) -> Self {
        Self::new(Kind::Warning, title, message)
    }

    pub fn error(title: impl Into<SharedString>, message: impl Into<SharedString>) -> Self {
        Self::new(Kind::Error, title, message)
    }

    /// What to do about it, in a line under the message.
    #[must_use]
    pub fn hint(self, hint: impl Into<SharedString>) -> Self {
        self.hint_opt(Some(hint.into()))
    }

    /// [`Self::hint`] for a hint that may not be there.
    #[must_use]
    pub fn hint_opt(mut self, hint: Option<impl Into<SharedString>>) -> Self {
        self.hint = hint.map(Into::into).filter(|h| !h.is_empty());
        self
    }

    /// The exact words behind it (an error code, the server's text), which a button copies.
    #[must_use]
    pub fn details(mut self, details: impl Into<SharedString>) -> Self {
        self.details = Some(details.into()).filter(|d| !d.is_empty());
        self
    }

    /// [`Self::details`] for details that may not be there.
    #[must_use]
    pub fn details_opt(mut self, details: Option<impl Into<SharedString>>) -> Self {
        self.details = details.map(Into::into).filter(|d| !d.is_empty());
        self
    }

    /// A button that does something about it, and closes the notice. Called again, it adds one
    /// more button beside the first.
    #[must_use]
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        run: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.actions.push((label.into(), Rc::new(run)));
        self
    }

    /// Keeps the notice until it is closed (or lets it go by itself), whatever its kind says.
    #[must_use]
    pub fn sticky(mut self, sticky: bool) -> Self {
        self.sticky = Some(sticky);
        self
    }

    /// Whether the notice waits for the user: a failure does, and so does anything with something
    /// to read or press.
    fn stays(&self) -> bool {
        self.sticky.unwrap_or_else(|| {
            self.kind == Kind::Error
                || !self.actions.is_empty()
                || (self.kind == Kind::Warning && (self.hint.is_some() || self.details.is_some()))
                || self.message.len() > LONG_MESSAGE
        })
    }

    /// The identity of the notice: the same words are the same notice, which replaces itself.
    fn key(&self) -> SharedString {
        let mut hasher = DefaultHasher::new();
        (self.kind, &self.title, &self.message).hash(&mut hasher);
        format!("toast-{:x}", hasher.finish()).into()
    }

    /// Shows the notice in the active window.
    pub fn show(self, cx: &mut App) {
        cx.defer(move |cx| {
            let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else {
                return;
            };
            let _ = window.update(cx, |_, window, cx| {
                let note = self.build();
                window.push_notification(note, cx);
            });
        });
    }

    fn build(self) -> Notification {
        let stays = self.stays();
        let key = self.key();
        let kind = match self.kind {
            Kind::Info => NotificationType::Info,
            Kind::Success => NotificationType::Success,
            Kind::Warning => NotificationType::Warning,
            Kind::Error => NotificationType::Error,
        };
        let Self {
            title,
            message,
            hint,
            details,
            actions,
            ..
        } = self;
        let copied = Rc::new(Cell::new(false));
        Notification::new()
            .id1::<ToastKey>(key)
            .with_type(kind)
            .autohide(!stays)
            .content(move |_note, _window, cx| {
                let this = cx.entity();
                let mut column = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(tokens::text::EMPHASIS))
                            .font_semibold()
                            .text_color(theme::fg())
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::BODY))
                            .text_color(theme::fg())
                            .child(message.clone()),
                    );
                if let Some(hint) = &hint {
                    column = column.child(
                        div()
                            .text_size(px(tokens::text::SMALL))
                            .text_color(theme::muted_fg())
                            .child(hint.clone()),
                    );
                }
                if !actions.is_empty() || details.is_some() {
                    let mut row = div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .pt_1();
                    for (index, (label, run)) in actions.iter().enumerate() {
                        let (run, this) = (run.clone(), this.clone());
                        row = row.child(
                            Button::new(("toast-action", index))
                                .cursor_pointer()
                                .outline()
                                .xsmall()
                                .label(label.clone())
                                .on_click(move |_, window, cx| {
                                    run(window, cx);
                                    this.update(cx, |note, cx| note.dismiss(window, cx));
                                }),
                        );
                    }
                    if let Some(details) = &details {
                        let (details, copied) = (details.clone(), copied.clone());
                        let done = copied.get();
                        row = row.child(
                            Button::new("toast-copy")
                                .cursor_pointer()
                                .ghost()
                                .xsmall()
                                .label(if done { "Copied" } else { "Copy details" })
                                .on_click(move |_, window, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        details.to_string(),
                                    ));
                                    copied.set(true);
                                    window.refresh();
                                }),
                        );
                    }
                    column = column.child(row);
                }
                column.into_any_element()
            })
    }
}

/// Shows a notice with a title and a message. For one with a hint, an action or details, build a
/// [`Toast`].
pub fn show(
    cx: &mut App,
    kind: Kind,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
) {
    Toast::new(kind, title, message).show(cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failure_stays_and_a_success_goes() {
        assert!(Toast::error("Failed", "It failed.").stays());
        assert!(!Toast::success("Saved", "It is saved.").stays());
        assert!(!Toast::info("Note", "Short.").stays());
        assert!(!Toast::warning("Careful", "Short.").stays());
    }

    #[test]
    fn a_notice_with_something_to_read_or_press_stays() {
        assert!(Toast::warning("Careful", "Short.").hint("Do this.").stays());
        assert!(
            Toast::info("Note", "Short.")
                .action("Open", |_, _| {})
                .stays()
        );
        assert!(Toast::info("Note", "x".repeat(LONG_MESSAGE + 1)).stays());
        assert!(!Toast::error("Failed", "It failed.").sticky(false).stays());
        assert!(Toast::success("Saved", "Short.").sticky(true).stays());
    }

    #[test]
    fn the_same_words_are_the_same_notice() {
        let a = Toast::error("Failed", "It failed.");
        let b = Toast::error("Failed", "It failed.").hint("Try again.");
        assert_eq!(a.key(), b.key());
        assert_ne!(a.key(), Toast::warning("Failed", "It failed.").key());
        assert_ne!(a.key(), Toast::error("Failed", "It failed again.").key());
    }

    #[test]
    fn an_empty_hint_or_details_is_no_hint() {
        let toast = Toast::info("Note", "Short.").hint("").details("");
        assert!(toast.hint.is_none() && toast.details.is_none());
    }
}
