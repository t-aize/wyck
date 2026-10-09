//! What the editor looks like: the toolbar, the list of scripts, the tabs, the console, the
//! reference and the status bar.

use std::collections::BTreeMap;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, MouseButton, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, Input};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{Disableable, Sizable};

use super::{
    AddToChart, Ask, CONTEXT, CloseTab, ConsoleTab, EditorEvent, IndicatorEditor, NextProblem,
    NextTab, PreviousProblem, PreviousTab, ResizeDrag, ResizeSide, SaveScript, ToggleReference,
};
use crate::indicators;
use wyck_chart::drawing::model::Tool;
use wyck_chart::study::custom::docs::{self, Group};
use wyck_chart::study::custom::library::{Entry as Script, registry};
use wyck_chart::study::custom::templates::TEMPLATES;
use wyck_chart::study::custom::{Problem, Severity};
use crate::ui::kit::{
    controls, icon, layout, menu,
    menu::{Entry, Item, Placement},
    theme, tokens,
};

mod console;
mod explorer;
mod reference;
mod toolbar;

/// A fixed width font that the system has.
fn mono() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "DejaVu Sans Mono"
    }
}

/// The name of a script in the tree: the last part of its id.
fn stem(id: &str) -> &str {
    id.rsplit('/').next().unwrap_or(id)
}

/// The folder of a script: its id without the last part.
fn folder_of(id: &str) -> &str {
    id.rsplit_once('/').map_or("", |(folder, _)| folder)
}

fn source_match(source: &str, query: &str) -> Option<(usize, usize)> {
    source.lines().enumerate().find_map(|(line, text)| {
        let lowered = text.to_lowercase();
        let column = lowered.find(query)?;
        Some((line + 1, lowered[..column].chars().count() + 1))
    })
}

impl IndicatorEditor {
    // ---- the toolbar ----

    // ---- the list of scripts ----

    // ---- the tabs and the editor ----

    fn tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .flex_none()
            .h(px(tokens::height::large()))
            .flex()
            .flex_row()
            .items_end()
            .gap_0p5()
            .px_2()
            .border_b_1()
            .border_color(theme::border_hairline())
            .overflow_x_scrollbar();
        for (index, doc) in self.docs.iter().enumerate() {
            let active = index == self.active;
            let (pick, close) = (cx.entity(), cx.entity());
            let broken = doc.problems.iter().any(|p| p.severity == Severity::Error);
            row = row.child(
                div()
                    .id(("editor-tab", index))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .h(px(tokens::height::control()))
                    .pl_2p5()
                    .pr_1()
                    .rounded_t_md()
                    .cursor_pointer()
                    .border_1()
                    .border_b_0()
                    .border_color(if active {
                        theme::border_subtle()
                    } else {
                        gpui::rgba(0)
                    })
                    .when(active, |el| el.bg(theme::bg()))
                    .when(!active, |el| el.hover(|s| s.bg(theme::surface_hover())))
                    .on_click(move |_, window, cx| {
                        pick.update(cx, |e, cx| e.activate(index, window, cx));
                    })
                    .child(icon::tinted(
                        if broken {
                            IconName::TriangleAlert
                        } else {
                            IconName::FileCode
                        },
                        13.,
                        if broken {
                            theme::destructive()
                        } else {
                            theme::muted_fg()
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(tokens::text::body()))
                            .text_color(if active {
                                theme::fg()
                            } else {
                                theme::muted_fg()
                            })
                            .when(doc.gone, |el| el.line_through())
                            .child(stem(&doc.id).to_owned()),
                    )
                    .child(
                        div()
                            .id(("editor-tab-close", index))
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .hover(|s| s.bg(theme::surface_pressed()))
                            .on_click(move |_, window, cx| {
                                close.update(cx, |e, cx| e.close_tab(index, window, cx));
                            })
                            .child(if doc.dirty {
                                div()
                                    .size(px(8.))
                                    .rounded_full()
                                    .bg(theme::amber())
                                    .into_any_element()
                            } else {
                                icon::tinted(IconName::X, 12., theme::muted_fg()).into_any_element()
                            }),
                    ),
            );
        }
        row.into_any_element()
    }

    /// A line over the editor when the file changed under it, or is gone.
    fn banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let doc = self.current()?;
        let (text, actions): (&str, bool) = if doc.conflict {
            (
                "This file changed on disk, and this tab has changes of its own.",
                true,
            )
        } else if doc.gone {
            (
                "This file was deleted or moved. Saving brings it back.",
                false,
            )
        } else {
            return None;
        };
        let (reload, keep) = (cx.entity(), cx.entity());
        Some(
            div()
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(tokens::height::large()))
                .px_3()
                .bg(theme::amber_bg())
                .border_b_1()
                .border_color(theme::border_hairline())
                .child(icon::tinted(IconName::Info, 14., theme::amber()))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::fg())
                        .child(text.to_owned()),
                )
                .children(actions.then(|| {
                    Button::new("editor-reload")
                        .ghost()
                        .xsmall()
                        .compact()
                        .label("Load the file")
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            reload.update(cx, |e, cx| e.reload_from_disk(window, cx));
                        })
                }))
                .children(actions.then(|| {
                    Button::new("editor-keep")
                        .ghost()
                        .xsmall()
                        .compact()
                        .label("Keep mine")
                        .cursor_pointer()
                        .on_click(move |_, _window, cx| {
                            keep.update(cx, |e, cx| e.keep_mine(cx));
                        })
                }))
                .into_any_element(),
        )
    }

    fn empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let folder = cx.entity();
        let mut cards = div().flex().flex_row().flex_wrap().justify_center().gap_3();
        for (index, template) in TEMPLATES.iter().enumerate() {
            let pick = cx.entity();
            let blank = index < 2;
            cards = cards.child(
                div()
                    .id(("editor-template", index))
                    .w(px(210.))
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::border_subtle())
                    .bg(theme::fg_alpha(0.025))
                    .cursor_pointer()
                    .hover(|s| s.border_color(theme::accent()).bg(theme::surface_hover()))
                    .on_click(move |_, window, cx| {
                        pick.update(cx, |e, cx| e.ask_new(index, window, cx));
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(icon::tinted(
                                if blank {
                                    IconName::FilePlus
                                } else {
                                    IconName::ChartLine
                                },
                                15.,
                                theme::accent(),
                            ))
                            .child(
                                div()
                                    .text_size(px(tokens::text::emphasis()))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child(template.name),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child(template.description),
                    ),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .p_6()
            .child(icon::tinted(IconName::CodeXml, 32., theme::muted_fg()))
            .child(
                div()
                    .text_size(px(tokens::text::title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::fg())
                    .child("Write your own indicators"),
            )
            .child(
                div()
                    .max_w(px(460.))
                    .text_center()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child("Pick a script in the list, or start from one of these. A saved script shows up in the list of indicators, on every chart. The Reference button on the top right explains every function."),
            )
            .child(cards.max_w(px(690.)))
            .child(
                Button::new("editor-empty-folder")
                    .ghost()
                    .xsmall()
                    .compact()
                    .icon(IconName::FolderOpen)
                    .label("Open the folder")
                    .cursor_pointer()
                    .on_click(move |_, _window, cx| {
                        folder.update(cx, |_, cx| indicators::open_folder(cx));
                    }),
            )
            .into_any_element()
    }

    // ---- the console ----

    // ---- the reference ----

    // ---- the status bar ----

    fn status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let position = self
            .cursor(cx)
            .map(|(line, column)| format!("Ln {line}, Col {column}"));
        let doc = self.current();
        let errors = doc.map_or(0, |d| {
            d.problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .count()
        });
        div()
            .flex_none()
            .h(px(tokens::height::compact()))
            .px_3()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .border_t_1()
            .border_color(theme::border_hairline())
            .text_size(px(tokens::text::small()))
            .text_color(theme::muted_fg())
            .child("Rhai")
            .children(position)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(match &self.notice {
                        Some(n) if !n.ok => theme::destructive(),
                        Some(_) => theme::emerald(),
                        None => theme::muted_fg(),
                    })
                    .child(
                        self.notice
                            .as_ref()
                            .map_or_else(String::new, |n| n.text.clone()),
                    ),
            )
            .children(doc.map(|d| {
                if d.dirty {
                    div().text_color(theme::amber()).child("Not saved")
                } else if errors > 0 {
                    div()
                        .text_color(theme::destructive())
                        .child(format!("{errors} problem(s)"))
                } else {
                    div().text_color(theme::emerald()).child("Saved")
                }
            }))
            .into_any_element()
    }
}

fn note(text: &'static str) -> gpui::Div {
    div()
        .text_size(px(tokens::text::body()))
        .text_color(theme::muted_fg())
        .child(text)
}

fn reference_row(
    number: usize,
    title: impl Into<SharedString>,
    summary: impl Into<SharedString>,
    on_click: impl Fn(&mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(("reference-row", number))
        .mx_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .cursor_pointer()
        .hover(|s| s.bg(theme::surface_hover()))
        .on_click(move |_, window, cx| on_click(window, cx))
        .child(
            div()
                .font_family(mono())
                .text_size(px(tokens::text::body()))
                .text_color(theme::accent())
                .child(title.into()),
        )
        .child(
            div()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg())
                .child(summary.into()),
        )
        .into_any_element()
}

impl Render for IndicatorEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toolbar = self.toolbar(window, cx);
        let explorer = self.explorer(window, cx);
        let tabs = self.tabs(cx);
        let banner = self.banner(cx);
        let center = match self.current() {
            Some(doc) => div()
                .flex_1()
                .min_h_0()
                .child(
                    Editor::new(&doc.state)
                        .h_full()
                        .bordered(false)
                        .font_family(mono())
                        .text_size(px(tokens::text::body())),
                )
                .into_any_element(),
            None => self.empty(cx),
        };
        let console = self.console(cx);
        let reference = self.reference_open.then(|| self.reference(cx));
        let status = self.status_bar(cx);
        let prompt = self.prompt_row(cx);
        let _ = controls::child_id;
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .when(self.resize.is_some(), |el| {
                el.on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    this.drag_resize(event, cx);
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.end_resize(cx)),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.end_resize(cx)),
                )
            })
            .on_action(cx.listener(|this, _: &SaveScript, window, cx| this.save(window, cx)))
            .on_action(
                cx.listener(|this, _: &AddToChart, window, cx| this.add_to_chart(window, cx)),
            )
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let at = this.active;
                this.close_tab(at, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleReference, _window, cx| {
                this.reference_open = !this.reference_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &NextProblem, window, cx| {
                this.step_problem(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousProblem, window, cx| {
                this.step_problem(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.cycle_tab(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| {
                this.cycle_tab(false, window, cx);
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .font_family(crate::appearance::font(cx))
            .text_size(px(tokens::text::body()))
            .text_color(theme::fg())
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(explorer)
                    .child(
                        div()
                            .id("editor-explorer-resize")
                            .flex_none()
                            .w(px(5.))
                            .h_full()
                            .cursor_col_resize()
                            .hover(|s| s.bg(theme::accent_alpha(0.35)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                    this.resize = Some(ResizeDrag {
                                        side: ResizeSide::Explorer,
                                        start: f32::from(event.position.x),
                                        size: this.explorer_width,
                                    });
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(tabs)
                            .children(banner)
                            .child(center)
                            .child(console)
                            .children(prompt),
                    )
                    .children(reference.map(|reference| {
                        div()
                            .flex()
                            .flex_row()
                            .child(
                                div()
                                    .id("editor-reference-resize")
                                    .flex_none()
                                    .w(px(5.))
                                    .h_full()
                                    .cursor_col_resize()
                                    .hover(|s| s.bg(theme::accent_alpha(0.35)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                            this.resize = Some(ResizeDrag {
                                                side: ResizeSide::Reference,
                                                start: f32::from(event.position.x),
                                                size: this.reference_width,
                                            });
                                            cx.notify();
                                        }),
                                    ),
                            )
                            .child(reference)
                    })),
            )
            .child(status)
    }
}
