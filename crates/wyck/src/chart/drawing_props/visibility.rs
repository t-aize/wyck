//! The Visibility tab of the drawing settings: the timeframes a drawing shows on.

use gpui::prelude::*;

use super::*;

impl DrawingProps {
    pub(super) fn visibility_page(&self, drawing: &Drawing, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let all = drawing.timeframes.is_none();
        let mut page = form::page().child(form::group(
            IconName::Eye,
            "Display",
            [
                form::field(
                    "Hidden",
                    Some("Keeps the drawing but does not show it"),
                    self.switch("props-hidden", drawing.hidden, cx, |d, on| d.hidden = on),
                ),
                form::field(
                    "Locked",
                    Some("Stops it from being moved, changed or deleted"),
                    self.switch("props-locked", drawing.locked, cx, |d, on| d.locked = on),
                ),
                form::field(
                    "Name",
                    Some("As shown in the list of drawings"),
                    div().w(px(220.)).child(Input::new(&self.name).small()),
                ),
            ],
        ));

        let mut rows: Vec<AnyElement> = vec![form::field(
            "On every timeframe",
            None,
            self.switch("props-all-tf", all, cx, |d, on| {
                d.timeframes = if on { None } else { Some(every_timeframe()) };
            }),
        )];
        if !all {
            let shown: Vec<String> = drawing.timeframes.clone().unwrap_or_default();
            for (group, frames) in GROUPS {
                let codes: Vec<String> = frames.iter().map(|f| f.code()).collect();
                let labels: Vec<String> = frames.iter().map(|f| f.label()).collect();
                let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                let selected: Vec<usize> = codes
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| shown.contains(c))
                    .map(|(i, _)| i)
                    .collect();
                let chip_this = this.clone();
                let codes_for_click = codes.clone();
                rows.push(form::block(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(form::note(group))
                        .child(controls::chips(
                            SharedString::from(format!("props-tf-{group}")),
                            &label_refs,
                            &selected,
                            move |index, _window, cx| {
                                let code = codes_for_click[index].clone();
                                chip_this.update(cx, |e, cx| {
                                    e.change(cx, |d| {
                                        let mut list = d.timeframes.clone().unwrap_or_default();
                                        if let Some(at) = list.iter().position(|c| *c == code) {
                                            list.remove(at);
                                        } else {
                                            list.push(code);
                                        }
                                        d.timeframes = Some(list);
                                    });
                                });
                            },
                        )),
                ));
            }
        }
        page = page.child(form::group(IconName::Clock, "Timeframes", rows));
        page.into_any_element()
    }
}
