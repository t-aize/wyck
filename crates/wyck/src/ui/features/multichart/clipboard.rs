//! Copying and pasting the indicators and the settings of a chart.

use super::MultiChart;
use crate::domain::drawings::model::tool::Tool;
use gpui::Context;

impl MultiChart {
    /// Ctrl+C: copies the selected drawings, or else the indicators of the chart.
    pub fn copy_active_indicators(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        if chart.update(cx, |chart, cx| chart.copy_selected_drawings(cx)) {
            // What was copied last is what Ctrl+V pastes.
            self.clipboard = None;
            return;
        }
        self.copy_chart(self.active, cx);
    }

    /// Ctrl+V: the drawings copied last, or else the indicators or settings copied.
    pub fn paste_active(&mut self, cx: &mut Context<Self>) {
        if self.clipboard.is_none() {
            let chart = self.active_chart().clone();
            if chart.update(cx, |chart, cx| chart.paste_drawings(cx)) {
                return;
            }
        }
        self.paste_chart(self.active, cx);
    }

    /// Ctrl+A on the charts: selects every drawing of the symbol.
    pub fn select_all_drawings(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.select_all_drawings(cx));
    }

    /// An arrow key: moves the selected drawings. Returns whether there were any to move.
    pub fn nudge_drawings(&mut self, bars: f64, steps: f64, cx: &mut Context<Self>) -> bool {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.nudge_selected(bars, steps, cx))
    }

    /// The key of a tool: picks it, or goes back to the pointer when it is in hand already.
    pub fn pick_tool_key(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let current = self.drawings.read(cx).book().tool();
        self.pick_tool(
            if current == Some(tool) {
                None
            } else {
                Some(tool)
            },
            cx,
        );
    }

    pub(super) fn copy_chart(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(slot) = self.slots.get(index) else {
            return;
        };
        self.clipboard = Some(slot.chart.read(cx).settings().studies.clone());
    }

    pub(super) fn paste_chart(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(studies) = self.clipboard.clone() else {
            return;
        };
        let Some(slot) = self.slots.get(index) else {
            return;
        };
        let chart = slot.chart.clone();
        let limit = chart.read(cx).max_studies();
        if studies.len() > limit {
            crate::ui::kit::toast::show(
                cx,
                crate::ui::kit::toast::Kind::Warning,
                "Indicator limit reached",
                format!(
                    "The copied chart has {} indicators; this chart allows {limit}.",
                    studies.len()
                ),
            );
            return;
        }
        chart.update(cx, |chart, cx| {
            chart.edit_settings(cx, |settings| settings.studies = studies)
        });
        self.persist(cx);
    }

    pub(super) fn share_studies(&mut self, from: usize, cx: &mut Context<Self>) {
        if self.syncing_studies {
            return;
        }
        let Some(slot) = self.slots.get(from) else {
            return;
        };
        let studies = slot.chart.read(cx).settings().studies.clone();
        self.syncing_studies = true;
        for (index, slot) in self.slots.iter().enumerate() {
            if index != from {
                let studies = studies.clone();
                slot.chart.update(cx, |chart, cx| {
                    chart.edit_settings(cx, |s| s.studies = studies)
                });
            }
        }
        self.syncing_studies = false;
    }
}
