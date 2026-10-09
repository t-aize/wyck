//! The Coordinates tab of the drawing settings: the exact time and price of each point.

use super::{
    AnyElement, Drawing, DrawingProps, IconName, SharedString, div, form, point_fields, point_name,
    px, theme, tokens,
};
use gpui::prelude::*;

impl DrawingProps {
    pub(super) fn coordinates_page(&self, drawing: &Drawing) -> AnyElement {
        if drawing.tool.is_freehand() {
            return form::empty(
                IconName::Brush,
                "A freehand stroke is moved as a whole, by dragging it on the chart.",
            )
            .into_any_element();
        }
        let head = |text: SharedString, width: Option<f32>| {
            let cell = div()
                .text_size(px(tokens::text::small()))
                .text_color(theme::muted_fg());
            match width {
                Some(width) => cell.w(px(width)).child(text),
                None => cell.flex_1().child(text),
            }
        };
        let mut rows: Vec<AnyElement> = vec![
            div()
                .flex()
                .flex_row()
                .gap_2()
                .pt_2()
                .pb_1()
                .child(head("POINT".into(), Some(110.)))
                .child(head("PRICE".into(), Some(130.)))
                .child(head(
                    format!("TIME ({})", self.zone.label(drawing.points[0].t)).into(),
                    None,
                ))
                .into_any_element(),
        ];
        for (index, (price, time)) in self.prices.iter().zip(&self.times).enumerate() {
            let (price_on, time_on) = point_fields(drawing.tool, index);
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .h(px(crate::ui::kit::tokens::height::row()))
                    .child(
                        div()
                            .w(px(tokens::field::number()))
                            .text_size(px(tokens::text::emphasis()))
                            .text_color(theme::fg())
                            .child(point_name(drawing.tool, index)),
                    )
                    .child(
                        div()
                            .w(px(tokens::field::wide()))
                            .when(price_on, |el| el.child(crate::ui::kit::input::text(price))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .when(time_on, |el| el.child(crate::ui::kit::input::text(time))),
                    )
                    .into_any_element(),
            );
        }
        form::page()
            .child(form::group(IconName::Crosshair, "Points", rows))
            .child(form::note(format!(
                "Prices have {} decimals. Times snap to the bars when the drawing is moved.",
                self.digits
            )))
            .into_any_element()
    }
}
