//! The Visibility tab of the drawing settings: the timeframes a drawing shows on.

use super::*;
use wyck_ui::field;

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
                    field::text(&self.name),
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
            let here = self
                .chart
                .as_ref()
                .map(|chart| chart.read(cx).timeframe.code());
            let labels = ["Intraday", "Daily and above", "This timeframe", "None"];
            let preset_this = this.clone();
            rows.push(form::block(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(form::note("Quick choices"))
                    .child(controls::chips(
                        "props-tf-presets",
                        &labels,
                        &[],
                        move |index, _window, cx| {
                            let here = here.clone();
                            preset_this.update(cx, |e, cx| {
                                e.change(cx, |d| {
                                    d.timeframes = Some(match index {
                                        0 => timeframes_where(|ms| ms < DAY_MS),
                                        1 => timeframes_where(|ms| ms >= DAY_MS),
                                        2 => here.into_iter().collect(),
                                        _ => Vec::new(),
                                    });
                                });
                            });
                        },
                    )),
            ));
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

/// One day, in milliseconds.
const DAY_MS: i64 = 86_400_000;

/// The codes of the timeframes whose bar lasts `keep` milliseconds (ticks count as no time at all).
fn timeframes_where(keep: impl Fn(i64) -> bool) -> Vec<String> {
    GROUPS
        .iter()
        .flat_map(|(_, frames)| frames.iter())
        .filter(|frame| keep(frame.bar_ms().unwrap_or(0)))
        .map(|frame| frame.code())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quick_choices_split_the_timeframes_at_the_day() {
        let intraday = timeframes_where(|ms| ms < DAY_MS);
        let daily = timeframes_where(|ms| ms >= DAY_MS);
        assert!(intraday.contains(&"M5".to_owned()) && !intraday.contains(&"D1".to_owned()));
        assert!(daily.contains(&"D1".to_owned()) && !daily.contains(&"M5".to_owned()));
        assert_eq!(intraday.len() + daily.len(), every_timeframe().len());
    }
}
