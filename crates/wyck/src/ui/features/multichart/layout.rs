//! The layout of the charts: choosing it, linking charts together, and dragging the lines between them.

use super::layouts::{LayoutKey, layout};
use super::links::Link;
use super::split::Node;
use super::{MultiChart, MultiChartEvent};
use crate::app::workspace::{ChartState, NEW_CHART_TIMEFRAMES, Preferences};
use crate::ui::features::chart::ChartSettings;
use gpui::{Context, MouseButton, MouseMoveEvent};
use wyck_chart::ChartContent;
use wyck_market::Timeframe;

impl MultiChart {
    /// Arranges the charts as `key` says. Charts that stay keep their state, in order; new ones
    /// are added at the end; those that no longer fit are closed.
    pub fn set_layout(&mut self, key: LayoutKey, cx: &mut Context<Self>) {
        let wanted = layout(key).count();
        self.slots.truncate(wanted);
        while self.slots.len() < wanted {
            let timeframe = if self.sync.interval {
                self.active_timeframe(cx)
            } else {
                Timeframe::from_code(
                    NEW_CHART_TIMEFRAMES[self.slots.len() % NEW_CHART_TIMEFRAMES.len()],
                )
                .unwrap_or(Timeframe::DEFAULT)
            };
            // A new chart looks as the others do, and has their indicators when those are linked.
            let active = self.active_chart().read(cx).settings();
            let settings = ChartSettings::from_parts(
                ChartContent {
                    studies: if self.sync.studies {
                        active.studies.clone()
                    } else {
                        Vec::new()
                    },
                    ..ChartContent::default()
                },
                &active.look(),
            );
            // A new chart shows the active chart's symbol even when symbols are not linked.
            let symbol = self.active_chart().read(cx).symbol().cloned();
            self.add_chart(
                ChartState {
                    timeframe,
                    settings,
                    symbol: None,
                },
                cx,
            );
            if let Some(symbol) = symbol
                && let Some(slot) = self.slots.last()
                && slot.chart.read(cx).symbol().is_none()
            {
                slot.chart.update(cx, |chart, cx| {
                    chart.set_symbol(
                        symbol.id,
                        symbol.name,
                        symbol.digits,
                        symbol.pip_position,
                        cx,
                    );
                });
            }
        }
        self.active = self.active.min(self.slots.len() - 1);
        self.key = key;
        self.tell_layout_size(cx);
        let prefs = self.workspace.read(cx).preferences().clone();
        self.tree = tree_for(key, &prefs);
        self.persist(cx);
        cx.emit(MultiChartEvent::ActiveChanged);
        cx.notify();
    }

    /// Puts every line between the charts back where the grid has it.
    pub fn reset_splits(&mut self, cx: &mut Context<Self>) {
        self.tree.reset(layout(self.key));
        self.persist(cx);
        cx.notify();
    }

    pub fn toggle_link(&mut self, link: Link, cx: &mut Context<Self>) {
        link.toggle(&mut self.sync);
        match link {
            // Linking the interval brings every chart to the timeframe of the active one.
            Link::Interval if self.sync.interval => {
                let timeframe = self.active_timeframe(cx);
                self.set_timeframe(timeframe, cx);
            }
            // Linking the symbol brings every chart to the symbol of the active one.
            Link::Symbol if self.sync.symbol => {
                if let Some(symbol) = self.active_symbol(cx) {
                    let active = self.active;
                    self.set_symbol(active, symbol, cx);
                }
            }
            // Unlinking the crosshair clears the ones drawn from other charts.
            Link::Crosshair if !self.sync.crosshair => {
                for slot in &self.slots {
                    slot.chart
                        .update(cx, |chart, cx| chart.show_remote_pointer(None, cx));
                }
            }
            Link::Studies if self.sync.studies => self.share_studies(self.active, cx),
            _ => {}
        }
        self.persist(cx);
        cx.notify();
    }

    pub(super) fn on_area_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(mut drag) = self.split_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.split_drag = None;
            self.persist(cx);
            cx.notify();
            return;
        }
        let Some(area) = self.area.get() else {
            return;
        };
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let (w, h) = (
            f32::from(area.size.width).max(1.0),
            f32::from(area.size.height).max(1.0),
        );
        let delta = if drag.divider.vertical {
            (y - drag.last.1) / h
        } else {
            (x - drag.last.0) / w
        };
        drag.last = (x, y);
        self.split_drag = Some(drag);
        self.tree.drag(
            drag.divider.split,
            drag.divider.index,
            delta,
            drag.divider.span,
        );
        cx.notify();
    }

    pub(super) fn end_split_drag(&mut self, cx: &mut Context<Self>) {
        if self.split_drag.take().is_some() {
            self.persist(cx);
            cx.notify();
        }
    }
}

/// The split tree of a layout, with the weights saved for it.
pub(super) fn tree_for(key: LayoutKey, prefs: &Preferences) -> Node {
    let mut tree = super::split::tree(layout(key));
    if let Some(saved) = prefs.splits.get(&Preferences::split_key(key)) {
        tree.apply_weights(saved);
    }
    tree
}
