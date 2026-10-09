//! The Levels tab of the drawing settings.

use super::*;

impl DrawingProps {
    /// The button of a level that picks its line style: the drawing's own, then solid, dashed and
    /// dotted, one click at a time.
    pub(super) fn level_dash_button(
        &self,
        index: usize,
        current: Option<Dash>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let this = cx.entity();
        let next = match current {
            None => Some(Dash::Solid),
            Some(Dash::Solid) => Some(Dash::Dashed),
            Some(Dash::Dashed) => Some(Dash::Dotted),
            Some(Dash::Dotted) => None,
        };
        let glyph: AnyElement = match current {
            None => div()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg())
                .child("Auto")
                .into_any_element(),
            Some(Dash::Solid) => controls::dash_glyph(0, theme::fg()).into_any_element(),
            Some(Dash::Dashed) => controls::dash_glyph(1, theme::fg()).into_any_element(),
            Some(Dash::Dotted) => controls::dash_glyph(2, theme::fg()).into_any_element(),
        };
        div()
            .id(("props-level-dash", index))
            .flex()
            .items_center()
            .justify_center()
            .w(px(44.))
            .h(px(tokens::height::control()))
            .rounded_md()
            .border_1()
            .border_color(theme::border_subtle())
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover()))
            .tooltip(controls::tooltip(
                "Line style of this level: the drawing's, solid, dashed, dotted",
            ))
            .on_click(move |_, _window, cx| {
                this.update(cx, |e, cx| {
                    e.change(cx, |d| {
                        d.levels = d.levels();
                        if let Some(level) = d.levels.get_mut(index) {
                            level.dash = next;
                        }
                    });
                });
            })
            .child(glyph)
    }

    pub(super) fn levels_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let levels = drawing.levels();
        let full = levels.len() >= MAX_LEVELS;
        let (add_this, reset_this) = (this.clone(), this.clone());
        let controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                crate::ui::kit::button::dense("props-level-add")
                    .when(full, |button| button.cursor_not_allowed())
                    .icon(IconName::Plus)
                    .label("Add")
                    .disabled(full)
                    .on_click(move |_, window, cx| {
                        add_this.update(cx, |e, cx| e.add_level(window, cx));
                    }),
            )
            .child(
                crate::ui::kit::button::dense("props-level-reset")
                    .icon(IconName::RotateCcw)
                    .label("Default")
                    .on_click(move |_, window, cx| {
                        reset_this.update(cx, |e, cx| {
                            e.change(cx, |d| d.levels = Vec::new());
                            e.levels_changed = true;
                            e.set_fields(window, cx);
                        });
                    }),
            )
            .into_any_element();

        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, level) in levels.iter().enumerate() {
            let Some(field) = self.levels.get(index) else {
                continue;
            };
            let remove_this = this.clone();
            let removable = levels.len() > 1;
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .min_h(px(42.))
                    .py_1p5()
                    .child(self.switch(
                        &format!("props-level-on-{index}"),
                        level.visible,
                        cx,
                        move |d, on| {
                            d.levels = d.levels();
                            if let Some(level) = d.levels.get_mut(index) {
                                level.visible = on;
                            }
                        },
                    ))
                    .child(number::field(field, tokens::field::number()))
                    .child(self.swatch(
                        Swatch::Level(index),
                        level.color,
                        &format!("props-level-color-{index}"),
                        cx,
                    ))
                    .children(
                        self.level_widths
                            .get(index)
                            .map(|width| number::field(width, tokens::field::narrow())),
                    )
                    .child(self.level_dash_button(index, level.dash, cx))
                    .child(div().flex_1())
                    .child(
                        crate::ui::kit::button::dense(SharedString::from(format!(
                            "props-level-remove-{index}"
                        )))
                        .icon(IconName::X)
                        .tooltip("Remove this level")
                        .disabled(!removable)
                        .on_click(move |_, window, cx| {
                            remove_this.update(cx, |e, cx| e.remove_level(index, window, cx));
                        }),
                    )
                    .into_any_element(),
            );
        }
        form::page()
            .child(self.captions_group(drawing, cx))
            .child(form::group_with(
                IconName::SlidersHorizontal,
                format!("Levels ({}/{MAX_LEVELS})", levels.len()),
                Some(controls),
                rows,
            ))
            .child(form::note(
                "Each level is a ratio of the move between the points. The switch hides one without losing it. A width of 0 follows the drawing, and the line button cycles through the drawing's style, solid, dashed and dotted.",
            ))
            .into_any_element()
    }

    pub(super) fn add_level(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.change(cx, |d| {
            let mut levels = d.levels();
            let next = levels.iter().map(|l| l.value).fold(0.0, f64::max) + 0.5;
            let color = levels.last().map_or(d.style.color, |l| l.color);
            levels.push(Level {
                value: next,
                color,
                visible: true,
                ..Level::default()
            });
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    pub(super) fn remove_level(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change(cx, |d| {
            let mut levels = d.levels();
            if index < levels.len() && levels.len() > 1 {
                levels.remove(index);
            }
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }
}
