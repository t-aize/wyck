//! Pictures of the charts, and what each chart tells the group.

use super::links::Follow;
use super::{MultiChart, MultiChartEvent, SymbolRef};
use crate::chart::chart::Chart;
use crate::chart::chart::events::ChartEvent;
use crate::chart::chart::raster::PICTURE_SCALE;
use gpui::{Context, Entity};
use wyck_core::math::u32_to_f32;

impl MultiChart {
    pub fn picture(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        match chart.read(cx).picture(cx) {
            Ok((png, name)) => cx.emit(MultiChartEvent::Picture(png, name)),
            Err(error) => cx.emit(MultiChartEvent::PictureFailed(error.to_string())),
        }
    }

    /// Takes a picture of every chart of the layout, in one image laid out as on screen.
    pub fn picture_all(&mut self, cx: &mut Context<Self>) {
        if self.slots.len() < 2 {
            self.picture(cx);
            return;
        }
        let (rects, _) = self.tree.place(self.slots.len());
        let mut tiles = Vec::new();
        let mut background = None;
        let (mut width, mut height) = self.area.get().map_or((0.0, 0.0), |a| {
            (f32::from(a.size.width), f32::from(a.size.height))
        });
        for (slot, rect) in self.slots.iter().zip(&rects) {
            let chart = slot.chart.read(cx);
            let (pixmap, bg) = match chart.picture_pixels(cx) {
                Ok(done) => done,
                Err(error) => {
                    cx.emit(MultiChartEvent::PictureFailed(error.to_string()));
                    return;
                }
            };
            background.get_or_insert(bg);
            let (tile_w, tile_h) = (
                u32_to_f32(pixmap.width()) / PICTURE_SCALE,
                u32_to_f32(pixmap.height()) / PICTURE_SCALE,
            );
            // The area was not measured yet (nothing was drawn): the charts say how big it is.
            width = width.max(tile_w / rect.w.max(f32::EPSILON));
            height = height.max(tile_h / rect.h.max(f32::EPSILON));
            tiles.push((rect.x, rect.y, pixmap));
        }
        let tiles: Vec<_> = tiles
            .into_iter()
            .map(|(x, y, pixmap)| (x * width, y * height, pixmap))
            .collect();
        let Some(background) = background else { return };
        match crate::chart::chart::raster::compose_png(
            width,
            height,
            PICTURE_SCALE,
            background,
            &tiles,
        ) {
            Ok(png) => {
                let stamp = wyck_app::market_data::now_ms();
                let name = format!("wyck-layout-{}-charts-{stamp}.png", self.slots.len());
                cx.emit(MultiChartEvent::Picture(png, name));
            }
            Err(error) => cx.emit(MultiChartEvent::PictureFailed(error.to_string())),
        }
    }

    pub(super) fn on_chart_event(
        &mut self,
        source: &Entity<Chart>,
        event: &ChartEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(from) = self.slots.iter().position(|s| &s.chart == source) else {
            return;
        };
        match event {
            ChartEvent::Activated => self.activate(from, cx),
            ChartEvent::Hover(_) | ChartEvent::ViewChanged(_) | ChartEvent::TimePicked(_) => {
                let symbols: Vec<Option<i64>> = self
                    .slots
                    .iter()
                    .map(|slot| slot.chart.read(cx).symbol().map(|s| s.id))
                    .collect();
                for (index, follow) in super::links::route(self.sync, from, &symbols, event) {
                    let Some(slot) = self.slots.get(index) else {
                        continue;
                    };
                    slot.chart.update(cx, |chart, cx| match follow {
                        Follow::Pointer(pointer) => chart.show_remote_pointer(pointer, cx),
                        Follow::Span(span) => chart.follow_span(span, cx),
                        Follow::RightEdge(right) => chart.follow_right_edge(right, cx),
                        Follow::PickedTime(time) => chart.follow_picked_time(time, cx),
                    });
                }
            }
            ChartEvent::LookChanged(look) => {
                // Kept for the next start, and handed to the other charts.
                let look = (**look).clone();
                self.workspace.update(cx, |workspace, cx| {
                    workspace.edit_preferences(cx, |prefs| prefs.chart = look.clone());
                });
                for (index, slot) in self.slots.iter().enumerate() {
                    if index != from {
                        slot.chart
                            .update(cx, |chart, cx| chart.adopt_look(&look, cx));
                    }
                }
            }
            ChartEvent::SettingsChanged => {
                if self.sync.studies && !self.syncing_studies {
                    self.share_studies(from, cx);
                }
                self.persist(cx);
                if from == self.active {
                    cx.emit(MultiChartEvent::ActiveChanged);
                }
            }
            ChartEvent::CopyIndicators => self.copy_chart(from, cx),
            ChartEvent::Paste => self.paste_chart(from, cx),
            ChartEvent::PickSymbol => {
                self.activate(from, cx);
                cx.emit(MultiChartEvent::PickSymbol(from));
            }
            ChartEvent::Action(_) if self.replay.read(cx).is_on() => {
                cx.emit(MultiChartEvent::ReplayBlocked);
            }
            ChartEvent::Action(action) => {
                if let Some(symbol) = source.read(cx).symbol() {
                    let symbol = SymbolRef {
                        id: symbol.id,
                        name: symbol.name.clone(),
                        digits: symbol.digits,
                        pip_position: symbol.pip_position,
                    };
                    cx.emit(MultiChartEvent::Action(symbol, action.clone()));
                }
            }
            ChartEvent::LineMoved(id, price) => cx.emit(MultiChartEvent::LineMoved(*id, *price)),
            ChartEvent::IndicatorEditor(request) => {
                cx.emit(MultiChartEvent::IndicatorEditor(request.clone()));
            }
            ChartEvent::LineClosed(id) => cx.emit(MultiChartEvent::LineClosed(*id)),
            ChartEvent::Screenshot => {
                self.activate(from, cx);
                self.picture(cx);
            }
            ChartEvent::ScreenshotAll => {
                self.activate(from, cx);
                self.picture_all(cx);
            }
            ChartEvent::ReplayStale => self.replay_changed(cx),
            ChartEvent::ReplayPicked(time_ms) => self.replay_picked(*time_ms, cx),
        }
    }
}
