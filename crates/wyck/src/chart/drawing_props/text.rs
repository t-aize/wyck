//! The Text tab of the drawing settings: the words, their place and the captions of the levels.

use super::*;

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

    /// A choice of how the label sits, for `set` to apply. `names` are the words for each choice.
    pub(super) fn align_picker<const N: usize>(
        &self,
        id: &'static str,
        names: [&'static str; N],
        current: usize,
        cx: &mut Context<Self>,
        set: fn(&mut Drawing, usize),
    ) -> AnyElement {
        let this = cx.entity();
        controls::segmented(id, &names, current, move |choice, _window, cx| {
            this.update(cx, |e, cx| e.change(cx, |d| set(d, choice)));
        })
        .into_any_element()
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
        let horizontal = self.align_picker(
            "label-align",
            ["Start", "Center", "End"],
            match layout.align {
                HAlign::Start => 0,
                HAlign::Center => 1,
                HAlign::End => 2,
            },
            cx,
            |d, choice| {
                d.style.text_layout.align = [HAlign::Start, HAlign::Center, HAlign::End][choice];
            },
        );
        let across = self.align_picker(
            "label-valign",
            vertical,
            match layout.valign {
                VAlign::Auto | VAlign::Top => 0,
                VAlign::Middle => 1,
                VAlign::Bottom => 2,
            },
            cx,
            |d, choice| {
                d.style.text_layout.valign = [VAlign::Top, VAlign::Middle, VAlign::Bottom][choice];
            },
        );
        let mut placement = vec![
            form::field("Along the drawing", None, horizontal),
            form::field("Across the drawing", None, across),
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
        let style = &drawing.style;
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
        page.child(form::group(
            IconName::Type,
            "Font",
            [
                form::field(
                    "Color",
                    None,
                    self.swatch(Swatch::Text, style.text_color(), "props-text-color", cx),
                ),
                form::field(
                    "Size",
                    None,
                    number::field(&self.text_size, tokens::field::NUMBER),
                ),
                form::field(
                    "Bold",
                    None,
                    self.switch("props-bold", style.bold, cx, |d, on| d.style.bold = on),
                ),
            ],
        ))
        .into_any_element()
    }
}
