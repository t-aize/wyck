//! The drawings on a chart: presses, moves and releases on the plot go to the shared drawing book
//! with this chart's projection, so a drawing made here shows at once on every chart of the
//! symbol.

use gpui::{Context, Entity};

use super::Chart;
use super::drawing::Drawings;
use super::drawing::book::Press;
use super::projection::ChartProjection;

impl Chart {
    /// Shows and edits the drawings of this entity, redrawing when they change.
    pub fn attach_drawings(&mut self, drawings: Entity<Drawings>, cx: &mut Context<Self>) {
        self._drawings_observe = Some(cx.observe(&drawings, |_this, _drawings, cx| cx.notify()));
        self.drawings = Some(drawings);
    }

    /// Runs `f` with the projection of the prices band as it is now, when it has data.
    pub(super) fn with_projection<R>(
        &self,
        f: impl FnOnce(&ChartProjection<'_>) -> R,
    ) -> Option<R> {
        let map = self.main_map()?;
        let geometry = self.geometry();
        let projection = ChartProjection {
            series: self.shown(),
            view: &self.view,
            map,
            plot_w: geometry.plot_w(),
            plot_h: geometry.main().h,
            step_ms: self.step_ms(),
            digits: self.digits(),
        };
        Some(f(&projection))
    }

    fn symbol_name(&self) -> Option<String> {
        self.symbol.as_ref().map(|s| s.name.to_string())
    }

    /// A press on the prices. Returns whether a drawing took it, in which case the chart does not
    /// scroll.
    pub(super) fn drawing_press(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return false;
        };
        let timeframe = self.timeframe.code();
        let taken = self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.press(&symbol, &timeframe, projection, x, y))
            })
        });
        taken == Some(Press::Taken)
    }

    /// The pointer moved: a drawing being made or moved follows it.
    pub(super) fn drawing_moved(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        let timeframe = self.timeframe.code();
        if !drawings.read(cx).book().is_busy() {
            let over = self
                .with_projection(|projection| {
                    drawings
                        .read(cx)
                        .book()
                        .hover_part(&symbol, &timeframe, projection, x, y)
                        .is_some()
                })
                .unwrap_or(false);
            if over != self.over_drawing {
                self.over_drawing = over;
                cx.notify();
            }
            return;
        }
        self.over_drawing = false;
        let changed = self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.pointer_moved(&symbol, projection, x, y))
            })
        });
        if changed == Some(true) {
            cx.notify();
        }
    }

    /// Duplicates the selected drawing, a little to the side.
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.duplicate(&symbol, projection))
            })
        });
    }

    pub(super) fn drawing_released(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.drawing_drag = false;
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.release(&symbol, projection, x, y))
            })
        });
        cx.notify();
    }
}
