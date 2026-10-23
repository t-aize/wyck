//! The inputs page of an indicator: its fields, and the script it comes from.

use super::{StudyEditor, grouped, timeframe_picks};
use crate::chart::chart::study::config::StudyConfig;
use crate::chart::chart::study::custom::library::registry;
use crate::chart::chart::study::custom::{Problem, Severity};
use crate::chart::chart::study::kind::StudyKind;
use crate::chart::chart::study::spec::{InputKind, SOURCES};
use crate::kit::icon::IconName;
use crate::kit::{button, controls, form, icon, modal, number, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};
use wyck_domain::base::math::{count_f64, f64_to_u32, f64_to_usize};

impl StudyEditor {
    pub(super) fn inputs_page(&self, config: &StudyConfig, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.input_rows(config, false, cx);
        let mut page = form::page();
        if let Some(group) = self.script_group(config, cx) {
            page = page.child(group);
        }
        if rows.is_empty() {
            page = page.child(form::note("This indicator has no parameters to change."));
        } else {
            for card in grouped(IconName::SlidersHorizontal, "Parameters", rows) {
                page = page.child(card);
            }
        }
        if config.kind == StudyKind::Atr {
            page = page.child(form::note(
                "ATR measures volatility, not direction. Price uses the symbol's price scale; % of close compares volatility across price levels. Percent decimals only affects % of close. Enable the Signal average or True range on the Style tab. Signal length and smoothing affect only the Signal average.",
            ));
        }
        page.into_any_element()
    }

    pub(super) fn input_rows(
        &self,
        config: &StudyConfig,
        style: bool,
        cx: &mut Context<Self>,
    ) -> Vec<(&'static str, AnyElement)> {
        let target = &self.target;
        let mut rows = Vec::new();
        let script = config.script.as_deref().and_then(registry::get);
        for input in config.spec().inputs {
            let script_style = script
                .as_ref()
                .and_then(|entry| entry.script.as_ref())
                .and_then(|script| {
                    script
                        .declaration
                        .inputs
                        .iter()
                        .find(|decl| decl.key == input.key)
                })
                .is_some_and(wyck_domain::indicators::custom::decl::InputDecl::in_style);
            let in_style = script_style
                || match config.kind {
                    StudyKind::VolumeProfile => matches!(
                        input.key,
                        "area_opacity" | "other_opacity" | "normal_opacity"
                    ),
                    _ => false,
                };
            if in_style != style {
                continue;
            }
            let key = input.key;
            let control: Option<AnyElement> = match input.kind {
                InputKind::Int | InputKind::Float => self
                    .fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, state)| {
                        number::field(state, tokens::field::wide()).into_any_element()
                    }),
                InputKind::Source => {
                    let target = target.clone();
                    Some(
                        controls::segmented(
                            SharedString::from(format!("source-{key}")),
                            &SOURCES,
                            f64_to_usize(config.input(key)),
                            move |choice, _window, cx| target.set_input(key, count_f64(choice), cx),
                        )
                        .into_any_element(),
                    )
                }
                InputKind::Choice(options) => {
                    let target = target.clone();
                    Some(
                        controls::segmented(
                            SharedString::from(format!("choice-{key}")),
                            options,
                            f64_to_usize(config.input(key)),
                            move |choice, _window, cx| target.set_input(key, count_f64(choice), cx),
                        )
                        .into_any_element(),
                    )
                }
                InputKind::Symbol | InputKind::Timeframe | InputKind::Session | InputKind::Text => {
                    self.texts
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, state)| {
                            let field = div()
                                .w(px(tokens::field::text()))
                                .child(crate::kit::input::text(state));
                            if input.kind == InputKind::Timeframe {
                                timeframe_picks(
                                    key,
                                    target.clone(),
                                    state.clone(),
                                    &config.text(key),
                                )
                                .child(field)
                                .into_any_element()
                            } else {
                                field.into_any_element()
                            }
                        })
                }
                InputKind::Toggle => {
                    let target = target.clone();
                    Some(
                        controls::toggle(
                            SharedString::from(format!("toggle-{key}")),
                            config.input(key) != 0.0,
                            move |on, _window, cx| {
                                target.set_input(key, if on { 1.0 } else { 0.0 }, cx);
                            },
                        )
                        .into_any_element(),
                    )
                }
                InputKind::Color => {
                    let (this, pick) = (cx.entity(), target.clone());
                    let open = self.color_open == Some(key);
                    Some(controls::color_swatch(
                        SharedString::from(format!("input-color-{key}")),
                        f64_to_u32(config.input(key)),
                        open,
                        cx,
                        move |_window, cx| {
                            this.update(cx, |e, cx| {
                                e.color_open = if e.color_open == Some(key) {
                                    None
                                } else {
                                    Some(key)
                                };
                                cx.notify();
                            });
                        },
                        move |color, _window, cx| pick.set_input(key, f64::from(color), cx),
                    ))
                }
            };
            if let Some(control) = control {
                let hint = (!input.tooltip.is_empty()).then_some(input.tooltip);
                rows.push((input.group, form::field(input.label, hint, control)));
            }
        }
        rows
    }

    /// For an indicator written as a script: which script it is, the way to its editor, and what
    /// is wrong with it when something is.
    pub(super) fn script_group(
        &self,
        config: &StudyConfig,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let id = config.script.clone().filter(|_| config.is_script())?;
        let entry = registry::get(&id);
        let mut problems: Vec<Problem> = entry
            .as_ref()
            .map(|e| e.problems.clone())
            .unwrap_or_default();
        if problems.is_empty()
            && let Some(report) = self.chart.read(cx).script_report(&id)
        {
            problems = report.problems;
        }
        let (edit_chart, edit_id) = (self.chart.clone(), id.clone());
        let reveal_path = entry.as_ref().map(|e| e.path.clone());
        let mut rows = Vec::new();
        let mut about = id.clone();
        if let Some(entry) = &entry {
            let info = &entry.info;
            let mut parts = Vec::new();
            if !info.author.is_empty() {
                parts.push(format!("by {}", info.author));
            }
            if !info.version.is_empty() {
                parts.push(format!("version {}", info.version));
            }
            if !parts.is_empty() {
                about = format!("{id} ({})", parts.join(", "));
            }
        }
        rows.push(form::field(
            "Script",
            entry
                .as_ref()
                .map(|_| "Written in the indicators folder. Save it in the editor and the chart follows")
                .or(Some("The file is not in the indicators folder")),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .max_w(px(tokens::measure::caption()))
                        .truncate()
                        .text_size(px(tokens::text::body()))
                        .text_color(theme::muted_fg())
                        .child(about),
                )
                .child(button::action(
                    "study-edit-script",
                    "Edit",
                    Some(IconName::Pencil),
                    false,
                    move |window, cx| {
                        modal::close(window, cx);
                        let id = edit_id.clone();
                        edit_chart.update(cx, |_, cx| {
                            cx.emit(crate::chart::chart::events::ChartEvent::IndicatorEditor(
                                crate::chart::chart::events::EditorRequest::Edit(id),
                            ));
                        });
                    },
                ))
                .children(reveal_path.map(|path| {
                    button::action(
                        "study-reveal-script",
                        "Show",
                        Some(IconName::FolderOpen),
                        false,
                        move |_window, cx| wyck_app::scripts::reveal(cx, &path),
                    )
                })),
        ));
        if let Some(entry) = &entry
            && !entry.info.description.is_empty()
        {
            rows.push(form::block(form::note(entry.info.description.clone())));
        }
        for problem in problems.iter().take(4) {
            let error = problem.severity == Severity::Error;
            let place = if problem.line > 0 {
                format!("line {}: ", problem.line)
            } else {
                String::new()
            };
            rows.push(form::block(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_start()
                    .text_size(px(tokens::text::body()))
                    .text_color(if error {
                        theme::destructive()
                    } else {
                        theme::amber()
                    })
                    .child(icon::tinted(
                        if error {
                            IconName::CircleAlert
                        } else {
                            IconName::TriangleAlert
                        },
                        13.,
                        if error {
                            theme::destructive()
                        } else {
                            theme::amber()
                        },
                    ))
                    .child(format!("{place}{}", problem.message)),
            ));
        }
        Some(form::group(IconName::CodeXml, "Script", rows).into_any_element())
    }
}
