//! What the properties panel does to a drawing: change it, keep it, reset its style, save it as a template.

use super::{DrawingProps, Swatch};

use crate::domain::drawings::model::{Drawing, MIN_LINE_OPACITY};
use crate::ui::kit::prelude::Switch;
use crate::ui::kit::{controls, number};
use gpui::{App, Context, SharedString, Window};

impl DrawingProps {
    pub(super) fn current(&self, cx: &App) -> Option<Drawing> {
        self.drawings
            .read(cx)
            .book()
            .get(&self.symbol, self.id)
            .cloned()
    }

    /// What a row's reset button does: puts one part of the look back to the tool's built-in
    /// value. `restore` copies that part from the built-in style into the drawing's.
    pub(super) fn restore(
        &self,
        cx: &mut Context<Self>,
        restore: fn(&mut Drawing, &crate::domain::drawings::model::Style),
    ) -> impl Fn(&mut Window, &mut App) + 'static + use<> {
        let this = cx.entity();
        move |window, cx| {
            this.update(cx, |e, cx| {
                e.change(cx, |d| {
                    let built_in = d.tool.default_style();
                    restore(d, &built_in);
                });
                e.set_fields(window, cx);
            });
        }
    }

    pub(super) fn has_template(&self, cx: &App) -> bool {
        self.drawings.read(cx).book().has_template(self.tool)
    }

    /// Changes the drawing and shows it at once.
    pub(super) fn change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Drawing)) {
        let Some(mut drawing) = self.current(cx) else {
            return;
        };
        change(&mut drawing);
        let symbol = self.symbol.clone();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.preview(&symbol, drawing))
        });
        cx.notify();
    }

    /// Ends the dialog: keeps the changes as one undo step, or puts the drawing back.
    pub(super) fn finish(&mut self, keep: bool, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.finished, true) {
            return;
        }
        let (symbol, before) = (self.symbol.clone(), std::mem::take(&mut self.before));
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| {
                if keep {
                    book.settle(&symbol, before)
                } else {
                    book.revert(&symbol, before)
                }
            })
        });
    }

    pub(super) fn save_template(&mut self, cx: &mut Context<Self>) {
        let (symbol, id) = (self.symbol.clone(), self.id);
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.save_template(&symbol, id))
        });
        crate::ui::kit::toast::show(
            cx,
            crate::ui::kit::toast::Kind::Success,
            "Saved as default",
            format!(
                "New {} drawings start with this look.",
                self.tool.label().to_lowercase()
            ),
        );
    }

    /// Back to the look new drawings of the tool start with; pressed again with a saved default,
    /// back to the built-in look (and the saved default is forgotten).
    pub(super) fn reset_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tool = self.tool;
        let (style, levels) = self.drawings.read(cx).book().starting_style(tool);
        let same = self
            .current(cx)
            .is_some_and(|d| d.style == style && d.levels == levels);
        let (style, levels) = if same && self.has_template(cx) {
            self.drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.forget_template(tool))
            });
            (tool.default_style(), Vec::new())
        } else {
            (style, levels)
        };
        self.change(cx, |d| {
            d.style = style;
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    /// Puts the values of the drawing back in the number fields.
    pub(super) fn set_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drawing) = self.current(cx) else {
            return;
        };
        let opacity = f64::from(drawing.style.fill_opacity) * 100.0;
        let size = number::format(f64::from(drawing.style.text_size), 0);
        let line = f64::from(drawing.style.opacity) * 100.0;
        self.opacity.update(cx, |s, cx| s.show(opacity, window, cx));
        self.line_opacity
            .update(cx, |s, cx| s.show(line, window, cx));
        let numbers = [
            (
                &self.width,
                number::format(f64::from(drawing.style.width), 1),
            ),
            (
                &self.scale,
                number::format(f64::from(drawing.style.scale) * 100.0, 0),
            ),
            (
                &self.profile_rows,
                number::format(f64::from(drawing.style.profile.rows), 0),
            ),
            (
                &self.profile_area,
                number::format(f64::from(drawing.style.profile.value_area) * 100.0, 0),
            ),
        ];
        for (state, text) in numbers {
            state.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        self.text_size
            .update(cx, |s, cx| s.set_value(size, window, cx));
        let tag = f64::from(drawing.style.text_layout.tag_opacity()) * 100.0;
        self.tag_opacity.update(cx, |s, cx| s.show(tag, window, cx));
        if let Some(pos) = &self.pos {
            let p = &drawing.style.position;
            let texts = [
                (&pos.account, number::format(p.account, 2)),
                (&pos.risk, number::format(p.risk, 2)),
                (&pos.lot_size, number::format(p.lot_size, 4)),
                (&pos.leverage, number::format(p.leverage, 0)),
                (&pos.point_value, number::format(p.point_value, 4)),
                (
                    &pos.qty_precision,
                    number::format(f64::from(p.qty_precision), 0),
                ),
                (&pos.currency, p.currency.clone()),
            ];
            for (state, text) in texts {
                state.update(cx, |s, cx| s.set_value(text, window, cx));
            }
        }
        self.make_level_fields(&drawing, window, cx);
    }

    pub(super) fn toggle_swatch(&mut self, swatch: Swatch, cx: &mut Context<Self>) {
        self.swatch = if self.swatch == Some(swatch) {
            None
        } else {
            Some(swatch)
        };
        cx.notify();
    }

    pub(super) fn swatch(
        &self,
        swatch: Swatch,
        color: u32,
        id: &str,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (toggle, pick) = (cx.entity(), cx.entity());
        // The lines and the zones each have an opacity, shown as a bar in their color panel. The
        // number field beside it is kept in step. What a swatch colors decides which one it is: the
        // lines (the line, a position's entry) or the area (the fill, a position's target and stop
        // zones). The other colors have no opacity of their own.
        let kind: Option<(&'static str, bool)> = match swatch {
            Swatch::Line | Swatch::Entry => Some(("LINES", true)),
            Swatch::Fill => Some(("FILL", false)),
            Swatch::Target | Swatch::Stop => Some(("ZONE", false)),
            _ => None,
        };
        let opacity = kind.and_then(|(label, lines)| {
            let drawing = self.current(cx)?;
            let value = if lines {
                drawing.style.opacity
            } else {
                drawing.style.fill_opacity
            };
            let this = cx.entity();
            let change: crate::ui::kit::color_picker::ChangeOpacity =
                std::rc::Rc::new(move |value, window, cx| {
                    this.update(cx, |e, cx| {
                        e.change(cx, |d| {
                            if lines {
                                d.style.opacity = value.clamp(MIN_LINE_OPACITY, 1.0);
                            } else {
                                d.style.fill_opacity = value;
                            }
                        });
                        e.set_fields(window, cx);
                    });
                });
            Some(crate::ui::kit::color_picker::Opacity {
                value,
                label,
                change,
            })
        });
        controls::color_swatch_with_opacity(
            SharedString::from(id.to_owned()),
            color,
            opacity,
            self.swatch == Some(swatch),
            cx,
            move |_window, cx| toggle.update(cx, |e, cx| e.toggle_swatch(swatch, cx)),
            move |color, _window, cx| {
                pick.update(cx, |e, cx| {
                    e.change(cx, |d| match swatch {
                        Swatch::Line => d.style.color = color,
                        Swatch::Fill => d.style.fill_color = Some(color),
                        Swatch::Text => d.style.text_color = Some(color),
                        Swatch::Target => d.style.position.target_color = color,
                        Swatch::Stop => d.style.position.stop_color = color,
                        Swatch::Entry => d.style.position.entry_color = color,
                        Swatch::TextBackground => {
                            d.style.text_layout.background_color = Some(color);
                        }
                        Swatch::MeasureDown => d.style.measure.down_color = color,
                        Swatch::Level(index) => {
                            d.levels = d.levels();
                            if let Some(level) = d.levels.get_mut(index) {
                                level.color = color;
                            }
                        }
                    });
                });
            },
        )
    }

    /// A switch that sets one flag of the drawing.
    pub(super) fn switch(
        &self,
        id: &str,
        on: bool,
        cx: &mut Context<Self>,
        set: impl Fn(&mut Drawing, bool) + 'static,
    ) -> Switch {
        let this = cx.entity();
        controls::toggle(
            SharedString::from(id.to_owned()),
            on,
            move |checked, _window, cx| {
                this.update(cx, |e, cx| e.change(cx, |d| set(d, checked)));
            },
        )
    }
}
