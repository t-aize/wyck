//! Placing the points of a new drawing.

use super::{Book, Creating, POSITION_BARS, POSITION_RISK, Press};
use crate::geometry::{P, Projection};
use crate::model::tool::Tool;
use crate::model::{Drawing, MAX_PATH_POINTS, Point};
use std::collections::BTreeMap;

impl Book {
    /// The four points of a new position: entry, stop and target at the same time, and the right
    /// edge. The stop is a share of the price span away and the target twice as far.
    pub(super) fn position_points(tool: Tool, at: Point, proj: &dyn Projection) -> Vec<Point> {
        let risk = proj.price_span() * POSITION_RISK;
        let (stop, target) = if tool == Tool::LongPosition {
            (at.p - risk, at.p + risk * 2.0)
        } else {
            (at.p + risk, at.p - risk * 2.0)
        };
        let right = proj.shift_bars(at.t, POSITION_BARS).unwrap_or(at.t);
        vec![
            at,
            Point { t: at.t, p: stop },
            Point { t: at.t, p: target },
            Point { t: right, p: at.p },
        ]
    }

    /// A press of the left button at `(x, y)` on the plot.
    pub fn press(
        &mut self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
    ) -> Press {
        self.press_resolved(symbol, timeframe, proj, x, y, &|d| Some(d.clone()))
    }

    pub fn press_resolved(
        &mut self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Press {
        self.press_adding(symbol, timeframe, proj, x, y, false, resolve)
    }

    /// A press, with `add` (Shift held) turning a click on a drawing into adding it to the
    /// selection, or taking it out, and leaving the selection alone on a click on nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn press_adding(
        &mut self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        add: bool,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Press {
        if let Some(tool) = self.tool {
            let Some(point) = proj.point_at(x, y, self.magnet) else {
                return Press::Taken;
            };
            return self.press_with_tool(symbol, tool, point, (x, y), proj);
        }
        self.press_to_edit(symbol, timeframe, proj, x, y, add, resolve)
    }

    pub(super) fn press_with_tool(
        &mut self,
        symbol: &str,
        tool: Tool,
        point: Point,
        at: P,
        proj: &dyn Projection,
    ) -> Press {
        // A drawing already begun: this press is for its next point, which lands where the button
        // is let go (see `release`), so the pointer can still be moved to it while it is down.
        if let Some(creating) = self.creating.as_mut().filter(|c| c.symbol == symbol) {
            creating.press = Some(at);
            creating.holding = true;
            creating.origin = false;
            creating.dragged = false;
            return Press::Taken;
        }
        // A new drawing starts: what was selected is left behind.
        self.selected = None;
        self.also.clear();
        if tool.is_position() {
            let points = Self::position_points(tool, point, proj);
            let drawing = self.blank(tool, points);
            self.commit(symbol, drawing);
        } else if tool.is_single_click() {
            let drawing = self.blank(tool, vec![point; tool.anchors()]);
            self.commit(symbol, drawing);
        } else {
            let points = if tool.is_freehand() {
                vec![point]
            } else {
                vec![point; tool.anchors()]
            };
            let drawing = self.blank(tool, points);
            self.creating = Some(Creating {
                symbol: symbol.to_owned(),
                drawing,
                placed: 1,
                press: Some(at),
                last: Some(at),
                holding: true,
                origin: true,
                dragged: false,
            });
        }
        Press::Taken
    }

    /// Fixes the next point of the drawing being made at `point`, and finishes the drawing when
    /// that was its last one. `at` is where it is on the screen.
    pub(super) fn place_point(&mut self, symbol: &str, point: Point, at: P) {
        let Some(creating) = self.creating.as_mut().filter(|c| c.symbol == symbol) else {
            return;
        };
        creating.drawing.points[creating.placed] = point;
        creating.placed += 1;
        creating.last = Some(at);
        if creating.drawing.tool == Tool::ArrowPath {
            // One more point to follow the pointer, until the path is ended.
            creating.drawing.points.push(point);
            if creating.placed >= MAX_PATH_POINTS
                && let Some(done) = self.creating.take()
            {
                self.finish_path(done);
            }
            return;
        }
        for later in creating.placed..creating.drawing.points.len() {
            creating.drawing.points[later] = point;
        }
        if creating.placed >= creating.drawing.tool.anchors()
            && let Some(done) = self.creating.take()
        {
            self.commit(symbol, done.drawing);
        }
    }

    /// Enter: ends an arrow path with the points it has. The other tools have a number of points
    /// to place, so they go on waiting. Returns whether it did anything.
    pub fn finish(&mut self) -> bool {
        if self
            .creating
            .as_ref()
            .is_some_and(|c| c.drawing.tool == Tool::ArrowPath)
            && let Some(done) = self.creating.take()
        {
            self.finish_path(done);
            return true;
        }
        false
    }

    /// Backspace while a drawing is being made: takes back the last point placed, or gives the
    /// drawing up when only its first is. Returns whether a drawing was being made.
    pub fn remove_last_point(&mut self) -> bool {
        let Some(creating) = self.creating.as_mut() else {
            return false;
        };
        if creating.placed <= 1 || creating.drawing.tool.is_freehand() {
            self.creating = None;
            return true;
        }
        creating.placed -= 1;
        creating.last = None;
        creating.holding = false;
        creating.origin = false;
        if creating.drawing.tool == Tool::ArrowPath {
            creating.drawing.points.pop();
        } else {
            // The point taken back follows the pointer again, like the ones after it.
            let follower = creating.drawing.points[creating.placed];
            for later in creating.placed..creating.drawing.points.len() {
                creating.drawing.points[later] = follower;
            }
        }
        true
    }

    /// Whether a drawing is being made, point by point or by a stroke.
    pub fn is_creating(&self) -> bool {
        self.creating.is_some()
    }

    /// A right click while a drawing tool is active on `symbol`: drops what is half made, if there
    /// is any, and gives up the tool itself, back to the normal cursor, the same as TradingView.
    /// Returns whether there was anything to give up. With no tool armed and nothing half made it
    /// does nothing, so the right click is left for its usual menu. A drawing half made on another
    /// chart's symbol is left alone, tool and all: that chart still needs it to keep going.
    pub fn abort(&mut self, symbol: &str) -> bool {
        if let Some(creating) = self.creating.as_ref() {
            if creating.symbol != symbol {
                return false;
            }
            self.creating = None;
            self.tool = None;
            return true;
        }
        self.tool.take().is_some()
    }
}

/// Whether the drawing just added to `symbol` (the last one) has words to type.
pub(super) fn drawing_has_text(symbols: &BTreeMap<String, Vec<Drawing>>, symbol: &str) -> bool {
    symbols
        .get(symbol)
        .and_then(|list| list.last())
        .is_some_and(|d| d.tool.has_text())
}
