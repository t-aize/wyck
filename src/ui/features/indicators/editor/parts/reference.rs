//! The reference of the indicator editor: every function a script can call.

use super::{
    AnyElement, Context, FontWeight, Group, IconName, IndicatorEditor, Tool, div, docs, icon, menu,
    px, reference_row, theme, tokens,
};
use gpui::prelude::*;

impl IndicatorEditor {
    pub(super) fn reference(&self, cx: &mut Context<Self>) -> AnyElement {
        let query = self.reference_filter.read(cx).value().to_lowercase();
        let words: Vec<&str> = query.split_whitespace().collect();
        let matches = |text: &str| {
            let text = text.to_lowercase();
            words.iter().all(|w| text.contains(w))
        };
        let mut list = div()
            .id("reference-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb_2()
            .flex()
            .flex_col();
        let mut row_number = 0usize;
        // The names a script starts with.
        let globals: Vec<_> = docs::GLOBALS
            .iter()
            .filter(|g| matches(&format!("{} {}", g.name, g.summary)))
            .collect();
        if !globals.is_empty() {
            list = list.child(menu::section_title("Names you start with").px_3().pt_2p5());
            for global in globals {
                row_number += 1;
                let this = cx.entity();
                let name = global.name;
                list = list.child(reference_row(
                    row_number,
                    name,
                    global.summary,
                    move |window, cx| this.update(cx, |e, cx| e.insert(name, window, cx)),
                ));
            }
        }
        for group in Group::ALL {
            let items: Vec<_> = docs::FUNCTIONS
                .iter()
                .filter(|d| d.group == group)
                .filter(|d| matches(&format!("{} {} {}", d.name, d.signature, d.summary)))
                .collect();
            if items.is_empty() {
                continue;
            }
            list = list.child(menu::section_title(group.label()).px_3().pt_2p5());
            for doc in items {
                row_number += 1;
                let this = cx.entity();
                let example = doc.example;
                list = list.child(reference_row(
                    row_number,
                    doc.signature,
                    doc.summary,
                    move |window, cx| this.update(cx, |e, cx| e.insert(example, window, cx)),
                ));
            }
        }
        let tools: Vec<_> = Tool::ALL
            .iter()
            .filter_map(|tool| {
                let name = serde_json::to_value(tool).ok()?.as_str()?.to_owned();
                matches(&format!("{} {}", name, tool.label())).then_some((name, tool.label()))
            })
            .collect();
        if !tools.is_empty() {
            list = list.child(menu::section_title("Drawing tool names").px_3().pt_2p5());
            for (name, label) in tools {
                row_number += 1;
                let this = cx.entity();
                let snippet = format!("\"{name}\"");
                list = list.child(reference_row(row_number, name, label, move |window, cx| {
                    this.update(cx, |e, cx| e.insert(&snippet, window, cx))
                }));
            }
        }
        div()
            .flex_none()
            .w(px(self.reference_width))
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(theme::border_hairline())
            .bg(theme::fg_alpha(0.02))
            .child(
                div()
                    .flex_none()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .px_1()
                            .text_size(px(tokens::text::small()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::muted_fg())
                            .child("REFERENCE"),
                    )
                    .child(
                        crate::ui::kit::input::dense(&self.reference_filter).prefix(icon::tinted(
                            IconName::Search,
                            14.,
                            theme::muted_fg(),
                        )),
                    ),
            )
            .child(list)
            .child(
                div()
                    .flex_none()
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme::border_hairline())
                    .text_size(px(tokens::text::small()))
                    .text_color(theme::muted_fg())
                    .child("Click a line to write its example where the cursor is."),
            )
            .into_any_element()
    }
}
