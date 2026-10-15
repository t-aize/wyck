//! The header of the panel: the tabs and the figures of the account.

use super::tone_color;
use crate::app::prefs::panel::Tab;
use crate::ui::features::trading::panel::AccountPanel;
use crate::ui::features::trading::panel::data::Tone;
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, FontWeight, Role, SharedString, div, px};
use wyck_core::format::grouped;
use wyck_trading::math::format_amount;
use wyck_ui_kit::prelude::StyledExt as _;
use wyck_ui_kit::{focus::Keyboard, theme, tokens};

impl AccountPanel {
    pub(super) fn tab_button(
        &self,
        tab: Tab,
        count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let chosen = self.prefs.tab == tab;
        div()
            .id(SharedString::from(format!("panel-tab-{tab:?}")))
            .keyboard()
            .role(Role::Tab)
            .aria_label(SharedString::from(if count > 0 {
                format!("{} ({count})", tab.label())
            } else {
                tab.label().to_owned()
            }))
            .aria_selected(chosen)
            .h_full()
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .text_size(px(tokens::text::body()))
            .font_weight(if chosen {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if chosen {
                theme::fg()
            } else {
                theme::muted_fg()
            })
            .border_b_2()
            .border_color(if chosen {
                theme::accent()
            } else {
                gpui::rgba(0x00000000)
            })
            .hover(|s| s.text_color(theme::fg()))
            .on_click(cx.listener(move |this, _, _, cx| this.show_tab(tab, cx)))
            .child(tab.label())
            .children((count > 0).then(|| {
                div()
                    .px_1()
                    .rounded_sm()
                    .bg(theme::fg_alpha(0.08))
                    .text_size(px(tokens::text::caption()))
                    .text_color(theme::muted_fg())
                    .child(count.to_string())
            }))
    }

    /// The figures of the account: balance, equity, free margin and margin level.
    pub(super) fn stats_row(&self, cx: &App) -> AnyElement {
        let account = self.account.read(cx);
        let currency = account.book.currency.clone();
        let summary = account.summary();
        let item = |label: &'static str, value: String, color: gpui::Rgba| {
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_baseline()
                .gap_1()
                .child(
                    div()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(tokens::text::small()))
                        .font_semibold()
                        .text_color(color)
                        .child(value),
                )
        };
        let amount = |value: f64| format_amount(value, &currency, false);
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .child(item("Balance", amount(summary.balance), theme::fg()))
            .child(item("Equity", amount(summary.equity), theme::fg()))
            .child(item(
                "Free margin",
                amount(summary.free_margin),
                theme::fg(),
            ))
            .child(item(
                "Margin level",
                summary.margin_level.map_or_else(
                    || "-".to_owned(),
                    |level| format!("{}%", grouped(level.round() as i64)),
                ),
                match summary.margin_level {
                    Some(level) if level < 100.0 => tone_color(Tone::Down),
                    Some(level) if level < 300.0 => theme::amber(),
                    _ => theme::fg(),
                },
            ))
            .into_any_element()
    }
}
