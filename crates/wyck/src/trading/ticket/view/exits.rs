//! The exits block of the order ticket: cutting the order into several take profits, moving the
//! stop to the entry once the first is reached, and an OCO pair. What the block builds is
//! described in [`crate::trading::plan`].

use super::*;

impl OrderTicket {
    pub(super) fn exits_block(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let exits = &self.exits;
        let on = exits.on;
        let this = cx.entity();
        let switch = controls::switch("ticket-exits", on)
            .on_click(move |checked: &bool, _, cx| {
                let checked = *checked;
                this.update(cx, |t, cx| t.edit_exits(cx, |e| e.on = checked));
            })
            .into_any_element();
        let head = card(f.m).child(card_head("Exits", Some(switch)));
        if !on {
            return head
                .child(hint(
                    "Cut the order into several take profits, with a break-even.",
                ))
                .into_any_element();
        }
        let count = exits.legs.len().clamp(2, plan::MAX_LEGS);
        let counter = cx.entity();
        let legs = controls::segmented_fill(
            "ticket-exit-count",
            &["2 exits", "3 exits"],
            count - 2,
            move |choice, window, cx| {
                counter.update(cx, |t, cx| t.set_leg_count(choice + 2, window, cx));
            },
        );
        let mut rows = div().flex().flex_col().gap_1();
        rows = rows.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg())
                .child(div().w(px(34.)))
                .child(div().flex_1().child("Share (%)"))
                .child(div().w(px(10.)))
                .child(div().flex_1().child("Take profit (R)"))
                .child(div().w(px(10.))),
        );
        for (index, (share, target)) in self.leg_inputs.iter().take(count).enumerate() {
            rows = rows.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w(px(34.))
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::muted_fg())
                            .child(format!("TP{}", index + 1)),
                    )
                    .child(div().flex_1().child(NumberInput::new(share).small()))
                    .child(
                        div()
                            .w(px(10.))
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child("%"),
                    )
                    .child(div().flex_1().child(NumberInput::new(target).small()))
                    .child(
                        div()
                            .w(px(10.))
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child("R"),
                    ),
            );
        }
        let be = exits.break_even;
        let be_switch = {
            let this = cx.entity();
            controls::switch("ticket-break-even", be.on)
                .label("Stop to entry after a take profit")
                .on_click(move |checked: &bool, _, cx| {
                    let checked = *checked;
                    this.update(cx, |t, cx| t.edit_exits(cx, |e| e.break_even.on = checked));
                })
        };
        let leg_labels: Vec<String> = (1..count).map(|n| format!("TP{n}")).collect();
        let leg_labels: Vec<&str> = leg_labels.iter().map(String::as_str).collect();
        let be_leg = {
            let this = cx.entity();
            controls::segmented_fill(
                "ticket-break-even-leg",
                &leg_labels,
                usize::from(be.after_leg).saturating_sub(1).min(count - 2),
                move |choice, _, cx| {
                    this.update(cx, |t, cx| {
                        t.edit_exits(cx, |e| e.break_even.after_leg = (choice + 1) as u8)
                    });
                },
            )
        };
        let trail = {
            let this = cx.entity();
            controls::switch("ticket-trail-last", exits.trail_last)
                .label("Trail the last exit")
                .on_click(move |checked: &bool, _, cx| {
                    let checked = *checked;
                    this.update(cx, |t, cx| t.edit_exits(cx, |e| e.trail_last = checked));
                })
        };
        head.child(legs)
            .child(rows)
            .child(hint(
                "The share of the volume for each exit, and where it takes profit in multiples of the risk. An exit at 0 R has no take profit.",
            ))
            .child(be_switch)
            .when(be.on, |el| {
                el.child(line("After", be_leg)).child(line(
                    "Plus (pips)",
                    number::field(&self.be_offset, tokens::field::narrow()),
                ))
            })
            .child(trail)
            .child(line(
                "OCO: opposite side (pips away)",
                number::field(&self.oco_pips, tokens::field::narrow()),
            ))
            .child(hint(
                "OCO needs a limit or stop order, 0 is off. When one side fills, the other is cancelled.",
            ))
            .child(hint(
                "Every exit is its own order, with its own stop loss and take profit kept by the broker. The break-even and the OCO are run by this app while it is open.",
            ))
            .into_any_element()
    }
}
