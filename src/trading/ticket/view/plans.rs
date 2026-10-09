//! The time stop and the saved plans of the order ticket. What they hold is described in
//! [`crate::trading::plan`] and [`super::super::prefs`].

use super::*;
use crate::trading::plan::Only;
use crate::trading::ticket::prefs;

impl OrderTicket {
    /// Closing the position once it has been open for a while.
    pub(super) fn time_stop_block(
        &self,
        f: &Frame,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stop = self.time_stop;
        let this = cx.entity();
        let switch = controls::switch("ticket-time-stop", stop.on)
            .on_click(move |checked: &bool, _, cx| {
                let checked = *checked;
                this.update(cx, |t, cx| t.edit_time_stop(cx, |s| s.on = checked));
            })
            .into_any_element();
        let head = card(f.m).child(card_head("Time stop", Some(switch)));
        if !stop.on {
            return head
                .child(hint(
                    "Close the position by itself once it has been open for a while.",
                ))
                .into_any_element();
        }
        let spans: Vec<&str> = prefs::Span::ALL.iter().map(|s| s.label()).collect();
        let span_at = prefs::Span::ALL
            .iter()
            .position(|s| *s == stop.span)
            .unwrap_or(0);
        let span_this = cx.entity();
        let only: Vec<&str> = Only::ALL.iter().map(|o| o.label()).collect();
        let only_at = Only::ALL.iter().position(|o| *o == stop.only).unwrap_or(0);
        let only_this = cx.entity();
        let rule = self.time_stop_now(cx).rule();
        head.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .flex_1()
                        .child(NumberInput::new(&self.time_stop_amount).small()),
                )
                .child(controls::segmented(
                    "ticket-time-stop-span",
                    &spans,
                    span_at,
                    move |choice, _, cx| {
                        span_this.update(cx, |t, cx| {
                            t.edit_time_stop(cx, |s| s.span = prefs::Span::ALL[choice])
                        });
                    },
                )),
        )
        .child(line(
            "Close",
            controls::segmented(
                "ticket-time-stop-only",
                &only,
                only_at,
                move |choice, _, cx| {
                    only_this.update(cx, |t, cx| t.edit_time_stop(cx, |s| s.only = Only::ALL[choice]));
                },
            ),
        ))
        .child(hint(match rule {
            Some(rule) => format!(
                "The position closes {}, counted from when it opens. A pending order starts counting when it fills.",
                rule.describe()
            ),
            None => "Set a time.".to_owned(),
        }))
        .child(hint(
            "This app closes it, so it must be running: after a restart, a time that ran out closes it at the start.",
        ))
        .into_any_element()
    }

    /// The setups saved under a name: apply one, or save the ticket as one.
    pub(super) fn plans_block(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let mut card = card(f.m).child(card_head(
            "Saved plans",
            Some(
                div()
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child(format!("{}", self.plans.len()))
                    .into_any_element(),
            ),
        ));
        if self.plans.is_empty() {
            card = card.child(hint(
                "Save the size, the stop loss, the take profit, the exits and the time stop under a name, and bring them back with one click.",
            ));
        }
        for (index, plan) in self.plans.iter().enumerate() {
            let (apply, delete) = (cx.entity(), cx.entity());
            let (apply_plan, delete_name) = (plan.clone(), plan.name.clone());
            card = card.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .id(SharedString::from(format!("ticket-plan-{index}")))
                            .keyboard()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::surface_hover()))
                            .on_click(move |_, window, cx| {
                                apply.update(cx, |t, cx| t.apply_template(&apply_plan, window, cx));
                            })
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(tokens::text::body()))
                                    .text_color(theme::fg())
                                    .child(plan.name.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(tokens::text::small()))
                                    .text_color(theme::muted_fg())
                                    .child(plan.summary()),
                            ),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("ticket-plan-delete-{index}")))
                            .keyboard()
                            .flex_none()
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::surface_hover()))
                            .on_click(move |_, _, cx| {
                                delete.update(cx, |t, cx| t.delete_template(&delete_name, cx));
                            })
                            .child(icon::tinted(IconName::Trash, 13., theme::muted_fg())),
                    ),
            );
        }
        let save = cx.entity();
        let named = !prefs::clean_name(&self.plan_name.read(cx).value()).is_empty();
        card.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(div().flex_1().child(Input::new(&self.plan_name).small()))
                .child(
                    Button::new("ticket-plan-save")
                        .small()
                        .label("Save")
                        .disabled(!named)
                        .on_click(move |_, window, cx| {
                            save.update(cx, |t, cx| t.save_template(window, cx));
                        }),
                ),
        )
        .child(hint(
            "Saves the ticket as it is now. A stop loss or take profit given as a price, or an ATR stop, is not kept. The same name replaces a plan.",
        ))
        .into_any_element()
    }
}
