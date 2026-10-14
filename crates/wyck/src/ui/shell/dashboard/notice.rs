//! What the account has to say, as a toast with the buttons the notice offers.

use gpui::{App, Entity};

use crate::app::account::Account;
use crate::app::account::{Notice, NoticeAction, Tone};
use crate::ui::kit::toast;

/// Shows `notice`. Its buttons act on `account`.
pub(super) fn show(account: &Entity<Account>, notice: Notice, cx: &mut App) {
    let kind = match notice.tone {
        Tone::Info => toast::Kind::Info,
        Tone::Success => toast::Kind::Success,
        Tone::Warning => toast::Kind::Warning,
        Tone::Error => toast::Kind::Error,
    };
    let has_actions = !notice.actions.is_empty();
    let mut toast = toast::Toast::new(kind, notice.title, notice.message)
        .hint_opt(notice.hint)
        .details_opt(notice.details);
    for action in notice.actions {
        let account = account.clone();
        toast = match action {
            NoticeAction::ClosePosition(id) => toast.action("Close position", move |_, cx| {
                account.update(cx, |a, cx| a.close_position(id, None, cx));
            }),
            NoticeAction::BreakEven(id) => toast.action("Stop to entry", move |_, cx| {
                account.update(cx, |a, cx| a.break_even(id, cx));
            }),
            NoticeAction::CancelOrder(id) => toast.action("Cancel order", move |_, cx| {
                account.update(cx, |a, cx| a.cancel_order(id, cx));
            }),
        };
    }
    // A fill with buttons goes away by itself: the panel keeps the same buttons.
    if has_actions && notice.tone == Tone::Success {
        toast = toast.sticky(false);
    }
    toast.show(cx);
}
