//! The Style tab of the drawing settings: lines, caps, fill, the measure and profile options, and the saved templates.

use super::{
    AnyElement, Cap, Context, DASHES, DEGREES, Drawing, DrawingProps, ICONS, IconName, IntoElement,
    MeasureFlag, Swatch, Tool, Window, controls, div, form, icon_key, number, px, tokens,
    wave_label, wave_names,
};
use gpui::prelude::*;
use wyck_ui_kit::field;
use wyck_ui_kit::form::Row;

/// The width of the field where a look is named.
const TEMPLATE_NAME_WIDTH: f32 = 170.0;

impl DrawingProps {
    /// A choice of what ends a line, for `set` to apply.
    pub(super) fn cap_picker(
        id: &'static str,
        current: Cap,
        cx: &mut Context<Self>,
        set: fn(&mut Drawing, Cap),
    ) -> impl IntoElement + use<> {
        let caps = [Cap::None, Cap::Arrow, Cap::Circle];
        let this = cx.entity();
        controls::segmented(
            id,
            &["None", "Arrow", "Dot"],
            caps.iter().position(|c| *c == current).unwrap_or(0),
            move |choice, _window, cx| {
                if let Some(&cap) = caps.get(choice) {
                    this.update(cx, |e, cx| e.change(cx, |d| set(d, cap)));
                }
            },
        )
    }

    /// The width of the lines: the presets of the tool, and a field for any other.
    pub(super) fn width_row(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let tool = drawing.tool;
        let widths = tool.widths();
        let index = widths
            .iter()
            .position(|w| (w - drawing.style.width).abs() < 0.01);
        let this = cx.entity();
        let default = tool.default_style().width;
        Row::new("Width")
            .hint("Pick one, or type any thickness in pixels")
            .help("How thick the line is drawn. A thin line is precise, a thick one stands out.")
            .reset(
                (drawing.style.width - default).abs() > 0.01,
                Self::restore(cx, |d, built_in| d.style.width = built_in.width),
            )
            .control(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(controls::width_picker(
                        "props-width",
                        &widths,
                        index,
                        move |choice, window, cx| {
                            if let Some(&width) = tool.widths().get(choice) {
                                this.update(cx, |e, cx| {
                                    e.change(cx, |d| d.style.width = width);
                                    e.set_fields(window, cx);
                                });
                            }
                        },
                    ))
                    .child(field::unit(&self.width, "px", tokens::field::number())),
            )
    }

    /// The caps of a line, for the tools that have some.
    pub(super) fn caps_rows(drawing: &Drawing, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (tool, caps) = (drawing.tool, drawing.style.caps);
        let mut rows = Vec::new();
        if tool.has_start_cap() {
            rows.push(form::field(
                "Start of the line",
                None,
                Self::cap_picker("props-cap-start", caps.start, cx, |d, cap| {
                    d.style.caps.start = cap;
                }),
            ));
        }
        if tool.has_end_cap() {
            rows.push(form::field(
                "End of the line",
                None,
                Self::cap_picker("props-cap-end", caps.end, cx, |d, cap| {
                    d.style.caps.end = cap;
                }),
            ));
        }
        rows
    }

    /// What a measuring tool writes, and the color of a move down.
    pub(super) fn measure_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let tool = drawing.tool;
        let look = drawing.style.measure;
        // Which numbers each tool has to write.
        let (price, percent, bars, time, angle) = match tool {
            Tool::PriceRange => (true, true, false, false, false),
            Tool::DateRange => (false, false, true, true, false),
            Tool::TrendAngle => (false, false, false, false, true),
            Tool::InfoLine => (true, true, true, true, true),
            _ => (true, true, true, true, false),
        };
        let flags: [MeasureFlag; 5] = [
            (
                price,
                "measure-price",
                "Price change",
                look.price,
                |d, on| {
                    d.style.measure.price = on;
                },
            ),
            (
                percent,
                "measure-percent",
                "Percent change",
                look.percent,
                |d, on| d.style.measure.percent = on,
            ),
            (
                bars,
                "measure-bars",
                "Number of bars",
                look.bars,
                |d, on| {
                    d.style.measure.bars = on;
                },
            ),
            (time, "measure-time", "Time span", look.time, |d, on| {
                d.style.measure.time = on;
            }),
            (angle, "measure-angle", "Angle", look.angle, |d, on| {
                d.style.measure.angle = on;
            }),
        ];
        let mut rows: Vec<AnyElement> = Vec::new();
        for (show, id, label, on, set) in flags {
            if show {
                rows.push(form::field(label, None, Self::switch(id, on, cx, set)));
            }
        }
        if tool == Tool::Measure {
            rows.push(form::field(
                "Color of a move down",
                Some("The color of the drawing is for a move up"),
                self.swatch(Swatch::MeasureDown, look.down_color, "props-down-color", cx),
            ));
        }
        form::group(IconName::Ruler, "Numbers written", rows)
    }

    pub(super) fn marker_group(&self) -> gpui::Div {
        form::group(
            IconName::Ruler,
            "Marker",
            [form::field(
                "Size",
                Some("In percent of its usual size"),
                number::field(&self.scale, tokens::field::number()),
            )],
        )
    }

    /// How a volume profile is cut.
    pub(super) fn profile_group(&self) -> gpui::Div {
        form::group(
            IconName::ChartBarBig,
            "Profile",
            [
                form::field(
                    "Rows",
                    Some("0 lets the height on the screen decide"),
                    number::field(&self.profile_rows, tokens::field::number()),
                ),
                form::field(
                    "Value area",
                    Some("The share of the volume it holds, in percent"),
                    number::field(&self.profile_area, tokens::field::number()),
                ),
            ],
        )
    }

    /// Looks saved under a name: apply one, delete one, or keep the look of this drawing.
    pub(super) fn templates_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let names: Vec<String> = self
            .drawings
            .read(cx)
            .book()
            .named_templates(drawing.tool)
            .iter()
            .map(|t| t.name.clone())
            .collect();
        let this = cx.entity();
        let mut rows: Vec<AnyElement> = Vec::new();
        if names.is_empty() {
            rows.push(form::block(form::note(
                "No saved look for this tool yet. Name the current one below to keep it.",
            )));
        }
        for (index, name) in names.iter().enumerate() {
            let (apply, delete) = (this.clone(), this.clone());
            let (apply_name, delete_name) = (name.clone(), name.clone());
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .min_h(px(tokens::height::row()))
                    .child(
                        wyck_ui_kit::button::quiet(("props-template", index))
                            .icon(IconName::Bookmark)
                            .label(name.clone())
                            .tooltip("Apply this look to the drawing")
                            .on_click(move |_, window, cx| {
                                apply.update(cx, |e, cx| e.apply_named(&apply_name, window, cx));
                            }),
                    )
                    .child(div().flex_1())
                    .child(
                        wyck_ui_kit::button::dense(("props-template-delete", index))
                            .icon(IconName::X)
                            .tooltip("Forget this look")
                            .on_click(move |_, _window, cx| {
                                delete.update(cx, |e, cx| e.delete_named(&delete_name, cx));
                            }),
                    )
                    .into_any_element(),
            );
        }
        let save = this;
        rows.push(form::field(
            "Save this look",
            Some("Under a name, for this tool. The same name replaces it"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(TEMPLATE_NAME_WIDTH))
                        .child(wyck_ui_kit::input::text(&self.template_name)),
                )
                .child(
                    wyck_ui_kit::button::accent("props-template-save")
                        .icon(IconName::BookmarkPlus)
                        .label("Save")
                        .on_click(move |_, window, cx| {
                            save.update(cx, |e, cx| e.save_named(window, cx));
                        }),
                ),
        ));
        form::group(IconName::Bookmark, "Saved looks", rows)
    }

    /// Puts the look saved under `name` on the drawing.
    pub(super) fn apply_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let found = self
            .drawings
            .read(cx)
            .book()
            .named_templates(self.tool)
            .into_iter()
            .find(|t| t.name == name)
            .map(|t| (t.style.clone(), t.levels.clone()));
        let Some((style, levels)) = found else {
            return;
        };
        self.change(cx, |d| {
            d.style = style;
            d.levels = levels;
        });
        self.set_fields(window, cx);
    }

    /// Keeps the look of the drawing under the name that was typed.
    pub(super) fn save_named(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.template_name.read(cx).value().to_string();
        let (symbol, id) = (self.symbol.clone(), self.id);
        let saved = self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.save_named_template(&symbol, id, &name))
        });
        if saved {
            self.template_name
                .update(cx, |state, cx| state.set_value("", window, cx));
            wyck_ui_kit::toast::show(
                cx,
                wyck_ui_kit::toast::Kind::Success,
                "Look saved",
                format!("{} is in the saved looks of this tool.", name.trim()),
            );
        } else {
            wyck_ui_kit::toast::show(
                cx,
                wyck_ui_kit::toast::Kind::Warning,
                "Give the look a name",
                "Type a name first, then save.",
            );
        }
        cx.notify();
    }

    pub(super) fn delete_named(&mut self, name: &str, cx: &mut Context<Self>) {
        let tool = self.tool;
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.delete_named_template(tool, name))
        });
        cx.notify();
    }

    /// The color and opacity of the lines, with the widths and line styles the tool takes.
    pub(super) fn line_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let built_in = tool.default_style();
        let mut rows: Vec<AnyElement> = vec![
            Row::new("Color")
                .hint("The color of the line")
                .reset(
                    style.color != built_in.color,
                    Self::restore(cx, |d, built_in| d.style.color = built_in.color),
                )
                .control(self.swatch(Swatch::Line, style.color, "props-line-color", cx)),
            Row::new("Opacity")
                .hint("How solid the line is")
                .help("100% is fully visible. Lower it to let the chart show through.")
                .reset(
                    (style.opacity - built_in.opacity).abs() > 0.005,
                    Self::restore(cx, |d, built_in| d.style.opacity = built_in.opacity),
                )
                .control(self.line_opacity.clone()),
        ];
        if tool.has_width() {
            rows.push(self.width_row(drawing, cx));
        }
        if tool.has_dash() {
            let index = DASHES.iter().position(|d| *d == style.dash).unwrap_or(0);
            let dash_this = this;
            rows.push(form::field(
                "Line style",
                None,
                controls::dash_picker("props-dash", index, move |choice, _window, cx| {
                    if let Some(&dash) = DASHES.get(choice) {
                        dash_this.update(cx, |e, cx| e.change(cx, |d| d.style.dash = dash));
                    }
                }),
            ));
        }
        rows.extend(Self::caps_rows(drawing, cx));
        if tool.has_extend() {
            rows.push(form::field(
                "Extend left",
                Some("Past the first point, to the edge of the chart"),
                Self::switch("props-extend-left", style.extend_left, cx, |d, on| {
                    d.style.extend_left = on;
                }),
            ));
            rows.push(form::field(
                "Extend right",
                Some("Past the last point, to the edge of the chart"),
                Self::switch("props-extend-right", style.extend_right, cx, |d, on| {
                    d.style.extend_right = on;
                }),
            ));
        }
        if let Some(label) = tool.middle_label() {
            rows.push(form::field(
                label,
                None,
                Self::switch("props-middle", style.middle, cx, |d, on| {
                    d.style.middle = on;
                }),
            ));
        }
        if let Some(label) = tool.labels_switch() {
            rows.push(form::field(
                label,
                None,
                Self::switch("props-labels", style.labels, cx, |d, on| {
                    d.style.labels = on;
                }),
            ));
        }
        if tool.has_reverse() {
            rows.push(form::field(
                "Reverse",
                Some("Flips the drawing upside down"),
                Self::switch("props-reverse", drawing.reverse, cx, |d, on| d.reverse = on),
            ));
        }
        form::group(IconName::PenLine, "Line", rows)
    }

    pub(super) fn style_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let tool = drawing.tool;
        let style = &drawing.style;
        let this = cx.entity();
        let mut page = form::page().child(self.line_group(drawing, cx));
        if tool.has_measure_look() {
            page = page.child(self.measure_group(drawing, cx));
        }
        if tool.has_size() {
            page = page.child(self.marker_group());
        }
        if tool.has_profile() {
            page = page.child(self.profile_group());
        }

        if tool == Tool::Icon {
            let keys: Vec<&str> = ICONS.iter().map(|(key, _)| *key).collect();
            let labels: Vec<&str> = ICONS.iter().map(|(_, label)| *label).collect();
            let chosen = keys
                .iter()
                .position(|key| *key == icon_key(&drawing.text))
                .unwrap_or(0);
            let icon_this = this.clone();
            page = page.child(form::group(
                IconName::Sparkles,
                "Icon",
                [form::block(controls::chips(
                    "props-icon",
                    &labels,
                    &[chosen],
                    move |index, _window, cx| {
                        let Some(key) = keys.get(index).map(|key| (*key).to_owned()) else {
                            return;
                        };
                        icon_this.update(cx, |e, cx| e.change(cx, |d| d.text = key));
                    },
                ))],
            ));
        }

        if tool.is_elliott() {
            let degree_this = this;
            let names = wave_names(tool);
            let example: Vec<String> = names
                .iter()
                .skip(1)
                .take(3)
                .map(|n| wave_label(n, drawing.degree))
                .collect();
            page = page.child(form::group(
                IconName::Waypoints,
                "Wave degree",
                [
                    form::block(controls::chips(
                        "props-degree",
                        &DEGREES,
                        &[usize::from(drawing.degree)],
                        move |index, _window, cx| {
                            degree_this.update(cx, |e, cx| {
                                e.change(cx, |d| d.degree = u8::try_from(index).unwrap_or(u8::MAX));
                            });
                        },
                    )),
                    form::block(form::note(format!("Points read {}", example.join(" ")))),
                ],
            ));
        }

        if tool.has_fill() {
            let mut rows = vec![form::field(
                "Fill",
                Some("Colors the area inside the shape"),
                Self::switch("props-fill", style.fill, cx, |d, on| d.style.fill = on),
            )];
            if !tool.has_levels() {
                rows.push(form::field(
                    "Fill color",
                    None,
                    self.swatch(Swatch::Fill, style.fill_color(), "props-fill-color", cx),
                ));
            }
            rows.push(
                Row::new("Fill opacity")
                    .hint("How solid the colored area is")
                    .reset(
                        (style.fill_opacity - tool.default_style().fill_opacity).abs() > 0.005,
                        Self::restore(cx, |d, built_in| {
                            d.style.fill_opacity = built_in.fill_opacity;
                        }),
                    )
                    .control(self.opacity.clone()),
            );
            page = page.child(form::group(IconName::PaintBucket, "Background", rows));
        }
        page.child(self.templates_group(drawing, cx))
            .into_any_element()
    }
}
