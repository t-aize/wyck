//! The options block of the order ticket: the expiry, the slippage, the stop loss options and
//! the comment, folded away until they are wanted.

use super::line;
use super::{
    AnyElement, Context, Entry, Frame, IconName, Kind, OrderTicket, Span, Tif, Window, card,
    controls, div, hint, icon, number, px, theme, tokens,
};
use crate::ui::kit::prelude::Disableable;
use crate::ui::kit::prelude::StyledExt as _;
use gpui::prelude::*;

impl OrderTicket {
    /// The expiry of a pending order, the slippage of a market one, the trailing and the
    /// guarantee of a stop loss, and the comment.
    pub(super) fn options(
        &self,
        f: &Frame,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.options_open;
        let fold = cx.entity();
        let head = div()
            .id("ticket-options-fold")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .h(px(tokens::height::compact()))
            .cursor_pointer()
            .text_color(theme::muted_fg())
            .hover(|s| s.text_color(theme::fg()))
            .on_click(move |_, _, cx| {
                fold.update(cx, |t, cx| {
                    t.options_open = !t.options_open;
                    cx.notify();
                });
            })
            .child(
                div()
                    .text_size(px(tokens::text::body()))
                    .font_semibold()
                    .child("More options"),
            )
            .child(icon::tinted(
                if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                },
                13.,
                theme::muted_fg(),
            ));
        let card = card(f.m).child(head);
        if !open {
            return card.into_any_element();
        }

        let this = cx.entity();
        let span_this = cx.entity();
        let span = self.expiry_span;
        let span_menu = self.select(
            "ticket-expiry-span",
            span.label().to_owned(),
            move || {
                Span::ALL
                    .into_iter()
                    .map(|choice| {
                        let this = span_this.clone();
                        Entry::new(choice.label())
                            .checked(choice == span)
                            .on_click(move |_, cx| {
                                this.update(cx, |t, cx| {
                                    t.expiry_span = choice;
                                    cx.notify();
                                });
                            })
                            .into()
                    })
                    .collect()
            },
            window,
            cx,
        );
        let stop_on = self.stop_on;
        let switch = |id: &'static str, text: &'static str, on: bool, enabled: bool| {
            let this = this.clone();
            controls::switch(id, on && enabled)
                .disabled(!enabled)
                .label(text)
                .on_click(move |checked: &bool, _, cx| {
                    let checked = *checked;
                    this.update(cx, |t, cx| {
                        match id {
                            "ticket-trailing" => t.trailing = checked,
                            "ticket-guaranteed" => t.guaranteed = checked,
                            _ => t.slippage_on = checked,
                        }
                        cx.notify();
                    });
                })
        };
        let tif_this = cx.entity();
        let tif_index = usize::from(self.tif == Tif::GoodTillDate);
        card.when(self.kind.is_pending(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(hint("Expires"))
                    .child(controls::segmented_fill(
                        "ticket-tif",
                        &["Until cancelled", "Good till date"],
                        tif_index,
                        move |choice, _, cx| {
                            tif_this.update(cx, |t, cx| {
                                t.tif = if choice == 1 {
                                    Tif::GoodTillDate
                                } else {
                                    Tif::GoodTillCancel
                                };
                                cx.notify();
                            });
                        },
                    ))
                    .when(self.tif == Tif::GoodTillDate, |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .flex_1()
                                        .child(crate::ui::kit::input::number(&self.expiry)),
                                )
                                .child(span_menu),
                        )
                    }),
            )
        })
        .when(self.kind == Kind::Market, |el| {
            el.child(switch(
                "ticket-slippage",
                "Limit the slippage",
                self.slippage_on,
                true,
            ))
            .when(self.slippage_on, |el| {
                el.child(line(
                    "Pips at most",
                    number::field(&self.slippage, tokens::field::narrow()),
                ))
            })
        })
        .child(switch(
            "ticket-trailing",
            "Trailing stop",
            self.trailing,
            stop_on,
        ))
        .child(switch(
            "ticket-guaranteed",
            "Guaranteed stop loss",
            self.guaranteed,
            stop_on,
        ))
        .when(!stop_on, |el| {
            el.child(hint("Turn the stop loss on to use these."))
        })
        .child(crate::ui::kit::input::text(&self.comment))
        .into_any_element()
    }
}
