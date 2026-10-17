//! The tabs, the footer and the body of the properties panel.

use super::{DrawingProps, Tab};

use crate::chart::tool_icon::tool_icon;
use gpui::prelude::*;
use gpui::{Context, Window, div};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::{button, form, form::Head, modal, theme};

impl DrawingProps {
    pub(super) fn tabs(&self) -> Vec<Tab> {
        let mut tabs = if self.tool.is_position() {
            vec![Tab::Position, Tab::Style]
        } else {
            vec![Tab::Style]
        };
        if self.tool.has_levels() {
            tabs.push(Tab::Levels);
        }
        if self.tool.has_words() || self.tool.has_text() || self.tool.takes_label() {
            tabs.push(Tab::Text);
        }
        tabs.push(Tab::Coordinates);
        tabs.push(Tab::Visibility);
        tabs
    }

    /// The footer: the defaults of the tool at the left, Cancel and OK at the right.
    pub(super) fn footer(&self, cx: &mut Context<Self>) -> gpui::Div {
        let has_template = self.has_template(cx);
        let (template, reset, cancel, ok) = (cx.entity(), cx.entity(), cx.entity(), cx.entity());
        form::footer(
            vec![
                button::action(
                    "drawing-template",
                    "Save as default",
                    Some(IconName::Save),
                    false,
                    move |_window, cx| template.update(cx, Self::save_template),
                )
                .tooltip("New drawings of this tool start with this look")
                .into_any_element(),
                button::action(
                    "drawing-reset",
                    if has_template { "Reset" } else { "Reset look" },
                    Some(IconName::RotateCcw),
                    false,
                    move |window, cx| reset.update(cx, |e, cx| e.reset_style(window, cx)),
                )
                .tooltip("Back to the look the tool starts with")
                .into_any_element(),
            ],
            vec![
                button::action(
                    "drawing-cancel",
                    "Cancel",
                    None,
                    false,
                    move |window, cx| {
                        cancel.update(cx, |e, cx| e.finish(false, cx));
                        modal::close(window, cx);
                    },
                )
                .into_any_element(),
                button::action("drawing-ok", "OK", None, true, move |window, cx| {
                    ok.update(cx, |e, cx| e.finish(true, cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        )
    }
}

impl Render for DrawingProps {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(drawing) = self.current(cx) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded_xl()
                .border_1()
                .border_color(theme::border_subtle())
                .bg(theme::bg())
                .child(form::empty(IconName::Info, "This drawing was removed."))
                .into_any_element();
        };
        if self.levels_changed || self.levels.len() != drawing.levels().len() {
            self.make_level_fields(&drawing, window, cx);
        }
        let tabs = self.tabs();
        if !tabs.contains(&self.tab)
            && let Some(&first) = tabs.first()
        {
            self.tab = first;
        }
        let specs: Vec<form::Tab> = tabs.iter().map(|t| t.spec()).collect();
        let active = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
        let this = cx.entity();
        let body = match self.tab {
            Tab::Position => self.position_page(&drawing, cx),
            Tab::Style if drawing.tool.is_position() => self.position_style_page(&drawing, cx),
            Tab::Style => self.style_page(&drawing, cx),
            Tab::Levels => self.levels_page(&drawing, cx),
            Tab::Text => self.text_page(&drawing, cx),
            Tab::Coordinates => self.coordinates_page(&drawing),
            Tab::Visibility => self.visibility_page(&drawing, cx),
        };
        let head = Head {
            icon: tool_icon(drawing.tool),
            title: drawing.title().into(),
            subtitle: format!(
                "{}{}",
                self.symbol,
                if drawing.locked { " - locked" } else { "" }
            )
            .into(),
        };
        let footer = self.footer(cx);
        form::frame(
            head,
            &specs,
            active,
            move |index, _window, cx| {
                let Some(&tab) = tabs.get(index) else {
                    return;
                };
                this.update(cx, |e, cx| {
                    e.tab = tab;
                    e.swatch = None;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
        .into_any_element()
    }
}
