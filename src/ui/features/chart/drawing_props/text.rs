//! The Text tab of the drawing settings: the words, their place and the captions of the levels.

use super::*;
use crate::ui::kit::field;
use crate::ui::kit::form::Row;

impl DrawingProps {
    /// What the levels are called on the chart, and the side their labels stand on.
    pub(super) fn captions_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let style = &drawing.style;
        let this = cx.entity();
        let labels: Vec<&str> = LevelText::ALL.iter().map(|t| t.label()).collect();
        let index = LevelText::ALL
            .iter()
            .position(|t| *t == style.level_text)
            .unwrap_or(0);
        let mut rows = vec![form::field(
            "Caption",
            Some("What is written beside each level"),
            controls::segmented(
                "props-level-text",
                &labels,
                index,
                move |choice, _window, cx| {
                    this.update(cx, |e, cx| {
                        e.change(cx, |d| d.style.level_text = LevelText::ALL[choice]);
                    });
                },
            ),
        )];
        if drawing.tool.has_label_side() {
            let side_this = cx.entity();
            rows.push(form::field(
                "Side",
                Some("Where the captions stand: left or right of the levels"),
                controls::segmented(
                    "props-level-side",
                    &["Left", "Right"],
                    usize::from(style.label_side == LabelSide::Right),
                    move |choice, _window, cx| {
                        side_this.update(cx, |e, cx| {
                            e.change(cx, |d| {
                                d.style.label_side = if choice == 0 {
                                    LabelSide::Left
                                } else {
                                    LabelSide::Right
                                };
                            });
                        });
                    },
                ),
            ));
        }
        form::group(IconName::Tag, "Captions", rows)
    }

    /// The words a line or a shape carries, and where they stand.
    pub(super) fn label_groups(&self, drawing: &Drawing, cx: &mut Context<Self>) -> Vec<gpui::Div> {
        let tool = drawing.tool;
        let layout = drawing.style.text_layout;
        // On a line the words go above it, on it or below it; in a shape, at the top, the middle
        // or the bottom.
        let on_a_line = matches!(
            tool,
            Tool::HorizontalLine | Tool::CrossLine | Tool::HorizontalRay | Tool::VerticalLine
        ) || (tool.anchors() == 2 && !tool.is_box());
        let vertical = if on_a_line {
            ["Above", "On it", "Below"]
        } else {
            ["Top", "Middle", "Bottom"]
        };
        let (along_this, across_this) = (cx.entity(), cx.entity());
        let horizontal = field::icon_choice(
            "label-align",
            &[
                (
                    IconName::AlignHorizontalJustifyStart,
                    "Start of the drawing",
                ),
                (IconName::AlignHorizontalJustifyCenter, "Center"),
                (IconName::AlignHorizontalJustifyEnd, "End of the drawing"),
            ],
            match layout.align {
                HAlign::Start => 0,
                HAlign::Center => 1,
                HAlign::End => 2,
            },
            move |choice, _window, cx| {
                along_this.update(cx, |e, cx| {
                    e.change(cx, |d| {
                        d.style.text_layout.align =
                            [HAlign::Start, HAlign::Center, HAlign::End][choice];
                    });
                });
            },
        );
        let across = field::icon_choice(
            "label-valign",
            &[
                (IconName::AlignVerticalJustifyStart, vertical[0]),
                (IconName::AlignVerticalJustifyCenter, vertical[1]),
                (IconName::AlignVerticalJustifyEnd, vertical[2]),
            ],
            match layout.valign {
                VAlign::Auto | VAlign::Top => 0,
                VAlign::Middle => 1,
                VAlign::Bottom => 2,
            },
            move |choice, _window, cx| {
                across_this.update(cx, |e, cx| {
                    e.change(cx, |d| {
                        d.style.text_layout.valign =
                            [VAlign::Top, VAlign::Middle, VAlign::Bottom][choice];
                    });
                });
            },
        );
        let mut placement = vec![
            form::field(
                "Along the drawing",
                Some("Start, center or end of the drawing"),
                horizontal,
            ),
            form::field(
                "Across the drawing",
                Some("Which side of the line or shape the words sit on"),
                across,
            ),
            form::field(
                "Background",
                Some("Puts the words on a filled tag"),
                self.switch("label-background", layout.background, cx, |d, on| {
                    d.style.text_layout.background = on
                }),
            ),
        ];
        if layout.background {
            placement.push(form::field(
                "Tag color",
                None,
                self.swatch(
                    Swatch::TextBackground,
                    layout.background_color.unwrap_or(0x1b1d24),
                    "label-background-color",
                    cx,
                ),
            ));
            placement.push(
                Row::new("Tag opacity")
                    .hint("How solid the tag behind the words is")
                    .reset(
                        (layout.tag_opacity() - crate::domain::drawings::look::DEFAULT_TAG_OPACITY)
                            .abs()
                            > 0.005,
                        self.restore(cx, |d, built_in| {
                            d.style.text_layout.background_opacity =
                                built_in.text_layout.background_opacity;
                        }),
                    )
                    .control(self.tag_opacity.clone()),
            );
        }
        vec![
            form::group(
                IconName::TextCursorInput,
                "Label",
                [form::block(Textarea::new(&self.text).h(px(72.)))],
            ),
            form::group(IconName::Move, "Placement", placement),
        ]
    }

    pub(super) fn text_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let mut page = form::page();
        if drawing.tool.has_text() {
            page = page.child(form::group(
                IconName::TextCursorInput,
                "Words",
                [form::block(Textarea::new(&self.text).h(px(96.)))],
            ));
        }
        if drawing.tool.takes_label() {
            for group in self.label_groups(drawing, cx) {
                page = page.child(group);
            }
        }
        page.child(self.font_group(drawing, cx)).into_any_element()
    }

    /// Opens or closes the list of fonts, made the first time it opens.
    pub(super) fn toggle_fonts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.font_open = !self.font_open;
        if self.font_open && self.font_picker.is_none() {
            let current = self.current(cx).and_then(|d| d.style.font);
            let picker = cx.new(|cx| FontPicker::new(current, "interface font", window, cx));
            self._font_subscription = Some(cx.subscribe(
                &picker,
                |this, _picker, event: &FontChosen, cx| {
                    let font = event.0.clone();
                    this.change(cx, |d| d.style.font = font);
                },
            ));
            self.font_picker = Some(picker);
        }
        cx.notify();
    }

    /// The font, size, weight and color of the words, and a line that shows them.
    pub(super) fn font_group(&self, drawing: &Drawing, cx: &mut Context<Self>) -> gpui::Div {
        let style = &drawing.style;
        let this = cx.entity();
        let toggle = this.clone();
        let family = style
            .font
            .clone()
            .unwrap_or_else(|| "Interface font".to_owned());
        let mut rows = vec![form::field(
            "Font",
            Some("Any font installed on this computer"),
            Button::new("props-font-open")
                .cursor_pointer()
                .ghost()
                .small()
                .icon(IconName::Type)
                .label(SharedString::from(family))
                .toggled(self.font_open)
                .on_click(move |_, window, cx| {
                    toggle.update(cx, |e, cx| e.toggle_fonts(window, cx));
                }),
        )];
        if let Some(picker) = self.font_picker.as_ref().filter(|_| self.font_open) {
            picker.update(cx, |picker, cx| picker.set_current(style.font.clone(), cx));
            rows.push(form::block(picker.clone()));
        }
        let index = SIZES
            .iter()
            .position(|size| (*size - style.text_size).abs() < 0.01);
        let labels: Vec<String> = SIZES.iter().map(|size| format!("{size}")).collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let size_this = this.clone();
        rows.push(
            Row::new("Size")
                .hint("Pick a size or type one")
                .reset(
                    (style.text_size - tool_default_size(drawing)).abs() > 0.01,
                    self.restore(cx, |d, built_in| d.style.text_size = built_in.text_size),
                )
                .control(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(controls::segmented(
                            "props-size",
                            &label_refs,
                            index.unwrap_or(usize::MAX),
                            move |choice, window, cx| {
                                size_this.update(cx, |e, cx| {
                                    e.change(cx, |d| d.style.text_size = SIZES[choice]);
                                    e.set_fields(window, cx);
                                });
                            },
                        ))
                        .child(field::unit(&self.text_size, "pt", tokens::field::number())),
                ),
        );
        rows.push(form::field(
            "Color",
            None,
            self.swatch(Swatch::Text, style.text_color(), "props-text-color", cx),
        ));
        rows.push(form::field(
            "Bold",
            None,
            self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
        ));
        rows.push(form::field(
            "Italic",
            None,
            self.switch("props-italic", style.italic, cx, |d, on| {
                d.style.italic = on
            }),
        ));
        rows.push(form::block(self.font_preview(drawing)));
        form::group(IconName::Type, "Font", rows)
    }

    /// A line of words in the font in force, to see a change before leaving.
    fn font_preview(&self, drawing: &Drawing) -> gpui::Div {
        let style = &drawing.style;
        let mut line = div()
            .h(px(52.))
            .px_3()
            .flex()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(theme::border_subtle())
            .bg(theme::chart_bg())
            .text_size(px(style.text_size.clamp(6.0, 48.0)))
            .text_color(gpui::rgb(style.text_color()))
            .child("Support 1.0850  Resistance 1.0920  0123456789");
        if let Some(font) = style.font.as_deref().filter(|f| !f.trim().is_empty()) {
            line = line.font_family(SharedString::from(font.trim().to_owned()));
        }
        if style.bold {
            line = line.font_weight(gpui::FontWeight::SEMIBOLD);
        }
        if style.italic {
            line = line.italic();
        }
        line
    }
}

/// The sizes offered as one click, in points.
const SIZES: [f32; 7] = [10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 32.0];

fn tool_default_size(drawing: &Drawing) -> f32 {
    drawing.tool.default_style().text_size
}
