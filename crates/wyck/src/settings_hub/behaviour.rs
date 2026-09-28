//! The Behaviour page of the settings.

use super::*;

impl SettingsHub {
    pub(super) fn behaviour_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.workspace.read(cx).preferences().clone();
        let (magnet, keep, bar, names) = (
            self.multi.clone(),
            self.multi.clone(),
            self.multi.clone(),
            self.multi.clone(),
        );
        form::page()
            .child(form::group(
                IconName::PenLine,
                "Drawing",
                [
                    form::field(
                        "Stay in drawing mode",
                        Some("Keep the tool picked after a drawing, to draw several in a row"),
                        controls::toggle("behaviour-keep", prefs.keep_drawing, move |on, _w, cx| {
                            if on != prefs.keep_drawing {
                                keep.update(cx, |m, cx| m.toggle_keep_drawing(cx));
                            }
                        }),
                    ),
                    form::field(
                        "Magnet",
                        Some("Drawings snap to the open, high, low and close of the nearest bar"),
                        controls::toggle("behaviour-magnet", prefs.magnet, move |on, _w, cx| {
                            if on != prefs.magnet {
                                magnet.update(cx, |m, cx| m.toggle_magnet(cx));
                            }
                        }),
                    ),
                    form::field(
                        "Favorites bar",
                        Some("The drawing tools you pinned, over the charts"),
                        controls::toggle("behaviour-bar", prefs.favorites_bar, move |on, _w, cx| {
                            if on != prefs.favorites_bar {
                                bar.update(cx, |m, cx| m.toggle_favorites_bar(cx));
                            }
                        }),
                    ),
                    form::field(
                        "Names in the favorites bar",
                        None,
                        controls::toggle(
                            "behaviour-names",
                            prefs.favorites_labels,
                            move |on, _w, cx| {
                                if on != prefs.favorites_labels {
                                    names.update(cx, |m, cx| m.toggle_favorite_names(cx));
                                }
                            },
                        ),
                    ),
                ],
            ))
            .child(form::group(
                IconName::SlidersHorizontal,
                "Usage limits",
                [
                    form::field(
                        "Indicators per chart",
                        Some("Includes hidden indicators. Existing ones stay when you lower it (1 to 64)"),
                        number::field(&self.study_limit, tokens::field::NUMBER),
                    ),
                    form::field(
                        "Saved price alerts",
                        Some("Includes inactive alerts. Existing ones stay when you lower it (1 to 2000)"),
                        number::field(&self.alert_limit, tokens::field::NUMBER),
                    ),
                    form::field(
                        "Drawings per symbol",
                        Some("Includes hidden drawings. Existing ones stay when you lower it (1 to 5000)"),
                        number::field(&self.drawing_limit, tokens::field::NUMBER),
                    ),
                ],
            ))
            .child(form::note(
                "Higher limits can slow charts or use more memory. Script execution has a separate limit on the Indicators page.",
            ))
            .into_any_element()
    }
}
