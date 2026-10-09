//! The console of the indicator editor: the problems of the script and what it printed.

use super::{
    AnyElement, ConsoleTab, Context, IconName, IndicatorEditor, MouseButton, Problem, ResizeDrag,
    ResizeSide, Severity, div, icon, mono, note, px, theme, tokens,
};
use gpui::prelude::*;

impl IndicatorEditor {
    pub(super) fn console(&self, cx: &mut Context<Self>) -> AnyElement {
        let problems: &[Problem] = self.current().map_or(&[], |d| d.problems.as_slice());
        let (a, b, previous, next) = (cx.entity(), cx.entity(), cx.entity(), cx.entity());
        let tab = |id: &'static str, label: &'static str, count: Option<usize>, chosen: bool| {
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_1p5()
                .h(px(tokens::height::compact()))
                .px_2p5()
                .cursor_pointer()
                .text_size(px(tokens::text::body()))
                .border_b_2()
                .border_color(if chosen {
                    theme::accent()
                } else {
                    gpui::rgba(0)
                })
                .text_color(if chosen {
                    theme::fg()
                } else {
                    theme::muted_fg()
                })
                .child(label)
                .children(count.filter(|c| *c > 0).map(|c| {
                    div()
                        .px_1p5()
                        .rounded_full()
                        .bg(theme::destructive_bg())
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::destructive())
                        .child(c.to_string())
                }))
        };
        let body: AnyElement = match self.console {
            ConsoleTab::Problems => self.problems_list(problems, cx),
            ConsoleTab::Output => self.output_list(cx),
        };
        div()
            .flex_none()
            .h(px(self.console_height))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::border_hairline())
            .child(
                div()
                    .id("editor-console-resize")
                    .flex_none()
                    .h(px(5.))
                    .w_full()
                    .cursor_row_resize()
                    .hover(|s| s.bg(theme::accent_alpha(0.35)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                            this.resize = Some(ResizeDrag {
                                side: ResizeSide::Console,
                                start: f32::from(event.position.y),
                                size: this.console_height,
                            });
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .px_1()
                    .border_b_1()
                    .border_color(theme::border_hairline())
                    .child(
                        tab(
                            "console-problems",
                            "Problems",
                            Some(problems.len()),
                            self.console == ConsoleTab::Problems,
                        )
                        .on_click(move |_, _window, cx| {
                            a.update(cx, |e, cx| {
                                e.console = ConsoleTab::Problems;
                                cx.notify();
                            });
                        }),
                    )
                    .child(
                        tab(
                            "console-output",
                            "Output",
                            None,
                            self.console == ConsoleTab::Output,
                        )
                        .on_click(move |_, _window, cx| {
                            b.update(cx, |e, cx| {
                                e.console = ConsoleTab::Output;
                                cx.notify();
                            });
                        }),
                    )
                    .child(div().flex_1())
                    .child(
                        self.tool_button(
                            "problem-prev",
                            IconName::ChevronUp,
                            None,
                            "Previous problem (Shift+F8)",
                            !problems.is_empty(),
                        )
                        .on_click(move |_, window, cx| {
                            previous.update(cx, |e, cx| e.step_problem(false, window, cx));
                        }),
                    )
                    .child(
                        self.tool_button(
                            "problem-next",
                            IconName::ChevronDown,
                            None,
                            "Next problem (F8)",
                            !problems.is_empty(),
                        )
                        .on_click(move |_, window, cx| {
                            next.update(cx, |e, cx| e.step_problem(true, window, cx));
                        }),
                    ),
            )
            .child(body)
            .into_any_element()
    }

    pub(super) fn problems_list(&self, problems: &[Problem], cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("console-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1p5()
            .flex()
            .flex_col();
        if problems.is_empty() {
            let text = if self.current().is_some() {
                "No problem found in this script."
            } else {
                "Open a script to see its problems."
            };
            return list
                .child(
                    div()
                        .p_2()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::muted_fg())
                        .child(icon::tinted(IconName::Check, 14., theme::emerald()))
                        .child(text),
                )
                .into_any_element();
        }
        for (index, problem) in problems.iter().enumerate() {
            let this = cx.entity();
            let (line, column) = (problem.line, problem.column);
            let error = problem.severity == Severity::Error;
            list = list.child(
                div()
                    .id(("console-problem", index))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, window, cx| {
                        this.update(cx, |e, cx| e.jump_to(line, column, window, cx));
                    })
                    .child(icon::tinted(
                        if error {
                            IconName::CircleAlert
                        } else {
                            IconName::TriangleAlert
                        },
                        14.,
                        if error {
                            theme::destructive()
                        } else {
                            theme::amber()
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(problem.message.clone()),
                    )
                    .children((line > 0).then(|| {
                        div()
                            .flex_none()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child(format!("line {line}, column {column}"))
                    })),
            );
        }
        list.into_any_element()
    }

    pub(super) fn output_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("console-output-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_0p5();
        let Some(doc) = self.current() else {
            return list
                .child(note("Open a script to see what it prints."))
                .into_any_element();
        };
        let chart = self.multi.read(cx).active_chart().clone();
        match chart.read(cx).script_report(&doc.id) {
            None => {
                list = list.child(note(
                    "This script is not on the active chart. Use Add to chart (F5) to see it run, and what it prints here.",
                ));
            }
            Some(report) => {
                let mut line = format!("On the active chart: {} bars", report.bars);
                if let Some(elapsed) = report.elapsed {
                    line.push_str(&format!(
                        ", computed in {:.1} ms",
                        elapsed.as_secs_f64() * 1000.0
                    ));
                }
                if report.running {
                    line.push_str(", running...");
                }
                list = list.child(
                    div()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(line),
                );
                if let Some(problem) = report.problems.first() {
                    list = list.child(
                        div()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::destructive())
                            .child(problem.message.clone()),
                    );
                }
                if report.log.is_empty() && report.problems.is_empty() {
                    list = list.child(note(
                        "It printed nothing. print(x) in the script writes here.",
                    ));
                }
                for text in report.log {
                    list = list.child(
                        div()
                            .font_family(mono())
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::fg())
                            .child(text),
                    );
                }
            }
        }
        list.into_any_element()
    }
}
