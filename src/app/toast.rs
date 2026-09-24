//! Short notices in the corner of the window: an order filled, an alert triggered, a picture
//! saved, something that failed.
//!
//! They are gpui-component notifications, pushed to the window from anywhere with only an
//! [`App`]: the push waits until the current update is over (a notice is often raised from inside
//! an event handler, while the window is busy), then goes to the active window.

use gpui::{App, SharedString};
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::{Notification, NotificationType};

/// What a notice is about, which sets its icon and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Success,
    Warning,
    Error,
}

/// Shows a notice with a title and a message.
pub fn show(
    cx: &mut App,
    kind: Kind,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
) {
    let (title, message) = (title.into(), message.into());
    cx.defer(move |cx| {
        let Some(window) = cx.active_window().or_else(|| cx.windows().first().copied()) else {
            return;
        };
        let _ = window.update(cx, |_, window, cx| {
            let kind = match kind {
                Kind::Info => NotificationType::Info,
                Kind::Success => NotificationType::Success,
                Kind::Warning => NotificationType::Warning,
                Kind::Error => NotificationType::Error,
            };
            let note = Notification::new()
                .title(title)
                .message(message)
                .with_type(kind);
            window.push_notification(note, cx);
        });
    });
}
