//! The summary card of the order ticket and the warnings under it.

use super::{
    AnyElement, Frame, IconName, Limit, Line, METER_HEIGHT, OrderTicket, Slot, card, card_head,
    div, format_units, icon, math, number, px, theme, tokens,
};
use gpui::prelude::*;
use wyck_core::math::{f32_of, i64_to_f64};

impl OrderTicket {
    pub(super) fn summary(&self, f: &Frame) -> AnyElement {
        let plan = &f.plan;
        let ratio = plan
            .risk
            .zip(plan.reward)
            .filter(|(r, _)| *r > 0.0)
            .map(|(r, w)| format!("1 : {:.2}", w / r));
        let mut card = card(f.m).child(card_head("Summary", None));
        // What is risked against what may be won, drawn to scale.
        if let Some((risk, reward)) = plan
            .risk
            .zip(plan.reward)
            .filter(|(r, w)| *r > 0.0 && *w > 0.0)
        {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .h(px(METER_HEIGHT))
                            .rounded_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(f32_of(risk / (risk + reward))))
                                    .bg(theme::chart_down()),
                            )
                            .child(div().h_full().flex_1().bg(theme::chart_up())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(px(tokens::text::small()))
                            .child(
                                div()
                                    .text_color(theme::chart_down())
                                    .child(format!("Risk {}", f.money(risk))),
                            )
                            .child(
                                div()
                                    .text_color(theme::chart_up())
                                    .child(format!("Reward {}", f.money(reward))),
                            ),
                    ),
            );
        }
        let mut rows = div().flex().flex_col().gap_1();
        for shown in self.layout.visible_lines() {
            let text = match shown {
                Line::Volume => f.lots.zip(f.units).map_or_else(
                    || "-".into(),
                    |(l, u)| format!("{} lots, {} units", math::format_lots(l), format_units(u)),
                ),
                Line::Notional => plan.sized.zip(plan.entry).zip(plan.rate).map_or_else(
                    || "-".into(),
                    |((s, e), r)| f.money(i64_to_f64(s.volume) / 100.0 * e * r),
                ),
                Line::Risk => match plan.risk {
                    Some(r) => format!("{}  {}", f.money(r), f.share(r)),
                    None if self.stop_on => "-".into(),
                    None => "No stop loss".into(),
                },
                Line::Reward => plan
                    .reward
                    .map_or_else(|| "-".into(), |r| format!("{}  {}", f.money(r), f.share(r))),
                Line::RiskReward => ratio.clone().unwrap_or_else(|| "-".into()),
                Line::Margin => f.margin.map_or_else(|| "-".into(), |m| f.money(m)),
                Line::PipValue => match (f.units, plan.rate) {
                    (Some(u), Some(rate)) => f.money(f.contract.pip() * u * rate),
                    (Some(u), None) => math::format_money(f.contract.pip() * u, &f.quote_currency),
                    _ => "-".into(),
                },
                Line::SpreadCost => f
                    .bid
                    .zip(f.ask)
                    .zip(f.units)
                    .zip(plan.rate)
                    .map_or_else(|| "-".into(), |(((b, a), u), r)| f.money((a - b) * u * r)),
            };
            rows = rows.child(summary_row(shown.label(), text));
        }
        card.child(rows).into_any_element()
    }

    /// The warnings that do not stop the order, then what does.
    pub(super) fn warnings(&self, f: &Frame) -> Vec<String> {
        let mut warnings: Vec<String> = Vec::new();
        match f.plan.sized.and_then(|s| s.limit) {
            Some(Limit::Min) => warnings.push(format!(
                "Raised to the least volume, {} lots: more at stake than asked",
                math::format_lots(f.contract.lots_of_volume(f.contract.min_volume))
            )),
            Some(Limit::Max) => warnings.push("Cut to the most volume the broker takes".into()),
            None => {}
        }
        if let Some(m) = f.margin
            && m > f.free_margin
        {
            warnings.push("Not enough free margin".into());
        }
        let high = self.layout.high_risk;
        if let Some(risk) = f.plan.risk
            && f.balance > 0.0
            && risk / f.balance * 100.0 > high
        {
            warnings.push(format!(
                "Risks more than {}% of the balance",
                number::format(high, 2)
            ));
        }
        warnings.extend(f.plan.problem.as_ref().map(super::super::problem::text));
        warnings
    }
}

pub(super) fn warning_rows(warnings: &[String]) -> Vec<AnyElement> {
    warnings
        .iter()
        .map(|message| {
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .p_2()
                .rounded_md()
                .bg(theme::amber_bg())
                .text_size(px(tokens::text::body()))
                .text_color(theme::amber())
                .child(
                    div()
                        .flex_none()
                        .pt(px(tokens::HAIRLINE))
                        .child(icon::tinted(IconName::TriangleAlert, 13., theme::amber())),
                )
                .child(div().flex_1().min_w_0().child(message.clone()))
                .into_any_element()
        })
        .collect()
}

fn summary_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .gap_2()
        .text_size(px(tokens::text::body()))
        .child(div().text_color(theme::muted_fg()).child(label))
        .child(div().text_color(theme::fg()).child(value))
}
