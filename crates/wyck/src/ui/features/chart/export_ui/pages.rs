//! The data, columns and preview pages of the export dialog.

use super::{ExportDialog, human_bytes, mono};
use crate::ui::features::chart::settings_rows::named;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::prelude::Disableable;
use crate::ui::kit::{form, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};
use wyck_chart::export;
use wyck_chart::export::PREVIEW_ROWS;
use wyck_chart::export::choices::{Content, Order, RangeKind};
use wyck_chart::export::options::ExportOptions;

impl ExportDialog {
    pub(super) fn data_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let o = &self.options;
        let derived = self.snapshot.display.is_derived();
        let mut what = Vec::new();
        if derived {
            what.push(Self::choice(
                cx,
                "export-content",
                "Export",
                Some("The chart draws its own elements, which are not the prices"),
                export::choices::Content::ALL
                    .iter()
                    .map(|c| {
                        (
                            *c,
                            if *c == Content::Shown {
                                self.snapshot.kind.label()
                            } else {
                                "Prices"
                            },
                        )
                    })
                    .collect(),
                o.content,
                |o, v| o.content = v,
            ));
        }
        what.push(Self::choice(
            cx,
            "export-range",
            "Rows",
            Some("Which part of the data goes in the file"),
            named(&RangeKind::ALL, RangeKind::label),
            o.range,
            |o, v| o.range = v,
        ));
        if o.range == RangeKind::Dates {
            what.push(Self::text(
                "From",
                Some("A date or a date and time, in the time zone chosen in Format. Empty is the start"),
                &self.inputs.from,
                190.,
            ));
            what.push(Self::text(
                "To",
                Some("A date alone takes that whole day. Empty is the end"),
                &self.inputs.to,
                190.,
            ));
        }
        if o.range == RangeKind::Last {
            what.push(Self::number("How many", None, &self.inputs.last));
        }
        let refine = vec![
            Self::choice(
                cx,
                "export-order",
                "Order",
                None,
                named(&Order::ALL, Order::label),
                o.order,
                |o, v| o.order = v,
            ),
            self.switch(
                cx,
                "export-forming",
                "Leave out the forming row",
                Some("The newest bar (or element) is not final yet"),
                o.skip_forming,
                |o, on| o.skip_forming = on,
            ),
            Self::number(
                "One row in",
                Some("1 keeps every row, 5 keeps one in five"),
                &self.inputs.every,
            ),
            Self::number("At most (rows)", Some("0 has no limit"), &self.inputs.limit),
        ];
        let summary = form::field(
            "This export",
            None,
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(format!(
                    "{} rows, about {}",
                    self.preview.rows,
                    human_bytes(self.preview.bytes)
                )),
        );
        form::page()
            .child(form::group(IconName::Layers, "What to export", what))
            .child(form::group(IconName::ListFilter, "Which rows", refine))
            .child(form::group(IconName::Info, "Result", [summary]))
            .into_any_element()
    }

    pub(super) fn columns_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let source = self.snapshot.source();
        let this = cx.entity();
        let count = self.options.columns.len();
        let mut chosen: Vec<AnyElement> = Vec::new();
        for (index, key) in self.options.columns.iter().copied().enumerate() {
            let code = key.code();
            let up = this.clone();
            let down = this.clone();
            let gone = this.clone();
            let icon_button = |id: String, icon: IconName, tip: &'static str| {
                crate::ui::kit::button::quiet(SharedString::from(id))
                    .icon(icon)
                    .tooltip(tip)
            };
            chosen.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .child(
                        div()
                            .w(px(crate::ui::kit::tokens::field::select()))
                            .flex_none()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::muted_fg())
                            .truncate()
                            .child(source.column_name(key)),
                    )
                    .child(
                        div().flex_1().min_w_0().children(
                            self.names
                                .get(&code)
                                .map(|state| form::text_field(state, tokens::field::text())),
                        ),
                    )
                    .child(
                        icon_button(format!("col-up-{code}"), IconName::ArrowUp, "Move up")
                            .disabled(index == 0)
                            .on_click(move |_, _, cx| {
                                up.update(cx, |d, cx| {
                                    d.options.columns.swap(index, index - 1);
                                    d.edited(cx);
                                });
                            }),
                    )
                    .child(
                        icon_button(format!("col-down-{code}"), IconName::ArrowDown, "Move down")
                            .disabled(index + 1 >= count)
                            .on_click(move |_, _, cx| {
                                down.update(cx, |d, cx| {
                                    d.options.columns.swap(index, index + 1);
                                    d.edited(cx);
                                });
                            }),
                    )
                    .child(
                        icon_button(format!("col-del-{code}"), IconName::X, "Leave out").on_click(
                            move |_, _, cx| {
                                gone.update(cx, |d, cx| {
                                    d.options.columns.remove(index);
                                    d.edited(cx);
                                });
                            },
                        ),
                    )
                    .into_any_element(),
            );
        }
        if chosen.is_empty() {
            chosen.push(
                div()
                    .py_2()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child("No column: pick some below.")
                    .into_any_element(),
            );
        }
        let mut wrap = div().flex().flex_row().flex_wrap().gap_2();
        for key in self
            .available()
            .into_iter()
            .filter(|k| !self.options.columns.contains(k))
        {
            let add = this.clone();
            wrap = wrap.child(
                div()
                    .id(SharedString::from(format!("col-add-{}", key.code())))
                    .h(px(tokens::height::control()))
                    .flex()
                    .items_center()
                    .px_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::border_subtle())
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, _, cx| {
                        add.update(cx, |d, cx| {
                            d.options.columns.push(key);
                            d.edited(cx);
                        });
                    })
                    .child(format!("+ {}", source.column_name(key))),
            );
        }
        let reset = this.clone();
        let standard = crate::ui::kit::button::quiet("col-standard")
            .label("Standard columns")
            .on_click(move |_, _, cx| {
                reset.update(cx, |d, cx| {
                    let available = d.available();
                    d.options.columns = ExportOptions::default()
                        .columns
                        .into_iter()
                        .filter(|c| available.contains(c))
                        .collect();
                    d.edited(cx);
                });
            });
        form::page()
            .child(form::group(
                IconName::Rows3,
                "Columns in the file, in order",
                chosen,
            ))
            .child(form::group(
                IconName::Plus,
                "Add a column",
                [form::block(wrap), form::block(standard)],
            ))
            .into_any_element()
    }

    pub(super) fn preview_page(&self) -> AnyElement {
        let p = &self.preview;
        let mut column = div().flex().flex_col().gap_1();
        for line in &p.lines {
            column = column.child(div().whitespace_nowrap().child(if line.is_empty() {
                SharedString::from(" ")
            } else {
                SharedString::from(line.clone())
            }));
        }
        let code = div()
            .id("export-preview")
            .w_full()
            .p_3()
            .rounded_md()
            .bg(theme::fg_alpha(0.05))
            .border_1()
            .border_color(theme::border_hairline())
            .overflow_x_scroll()
            .font_family(mono())
            .text_size(px(tokens::text::small()))
            .text_color(theme::fg())
            .child(column);
        let summary = form::field(
            "File",
            Some("Where the dialog will offer to save it"),
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(p.file_name.clone()),
        );
        let size = form::field(
            "Size",
            None,
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(format!("{} rows, about {}", p.rows, human_bytes(p.bytes))),
        );
        let mut page = form::page().child(form::group(IconName::Info, "The file", [summary, size]));
        if !p.problems.is_empty() {
            let rows = p.problems.iter().map(|text| {
                div()
                    .py_2()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::destructive())
                    .child(text.clone())
                    .into_any_element()
            });
            page = page.child(form::group(IconName::TriangleAlert, "To look at", rows));
        }
        page.child(form::group(
            IconName::Eye,
            format!("First {} rows", PREVIEW_ROWS),
            [form::block(code)],
        ))
        .into_any_element()
    }
}
