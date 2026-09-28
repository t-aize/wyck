//! The Replay page of the settings.

use super::*;

impl SettingsHub {
    pub(super) fn replay_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let speed = self.workspace.read(cx).preferences().replay.default_speed;
        let selected = wyck_market_data::replay::prefs::SPEED_PRESETS
            .iter()
            .position(|preset| (*preset - speed).abs() < 0.001)
            .unwrap_or(2);
        let labels: Vec<String> = wyck_market_data::replay::prefs::SPEED_PRESETS
            .iter()
            .map(|s| format!("{s}x"))
            .collect();
        let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let workspace = self.workspace.clone();
        form::page()
            .child(form::group(
                IconName::RotateCcw,
                "Replay",
                [form::field(
                    "Default speed",
                    Some("The playback speed a new replay starts at"),
                    controls::segmented("replay-default-speed", &label_refs, selected, move |index, _w, cx| {
                        let speed = wyck_market_data::replay::prefs::SPEED_PRESETS[index];
                        workspace.update(cx, |workspace, cx| {
                            workspace.edit_preferences(cx, |prefs| prefs.replay.default_speed = speed);
                        });
                    }),
                )],
            ))
            .child(form::note(
                "Replay holds back the active chart's most recently loaded bars and plays them forward again. It does not yet support picking an arbitrary historical start date, or scrubbing backward once started.",
            ))
            .into_any_element()
    }
}
