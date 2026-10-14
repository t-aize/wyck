//! Moving a drawing, one of its handles, or a new point under the pointer.

use super::{BRUSH_STEP, Book, DRAG_START, EditKind, Editing, Press, Snapshot};
use crate::domain::drawings::geometry::{self, Part, Projection};
use crate::domain::drawings::model::tool::Tool;
use crate::domain::drawings::model::{Drawing, MAX_BRUSH_POINTS, Point};

impl Book {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn press_to_edit(
        &mut self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        add: bool,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Press {
        let at = (x, y);
        // The selected drawing's grips come first, wherever it is in the stack.
        let mut found: Option<(u64, Part)> = None;
        if let Some(id) = self.selected
            && let Some(drawing) = self.get(symbol, id).filter(|d| d.shows_on(timeframe))
            && let Some(resolved) = resolve(drawing)
            && let Some(part @ Part::Handle(_)) = geometry::hit::hit(&resolved, proj, at, true)
        {
            found = Some((id, part));
        }
        if found.is_none() {
            found = self.hit_resolved(symbol, timeframe, proj, at, resolve);
        }
        let Some((id, part)) = found else {
            // Shift keeps the selection, so a box can be dragged from empty space to add to it.
            if !add {
                self.selected = None;
                self.also.clear();
            }
            return Press::Ignored;
        };
        if add {
            self.toggle_select(id);
            return Press::Taken;
        }
        // A press on a drawing of a group keeps the group, and moves it whole.
        let in_group = self.selected_count() > 1 && self.is_selected(id);
        if !in_group {
            self.selected = Some(id);
            self.also.clear();
        }
        let Some(original) = self.get(symbol, id).and_then(resolve) else {
            return Press::Taken;
        };
        if original.locked {
            return Press::Taken;
        }
        let kind = match part {
            Part::Handle(index) => EditKind::Handle(index),
            Part::Body => match proj.point_at(x, y, false) {
                Some(start) => EditKind::Move(start),
                None => return Press::Taken,
            },
        };
        let group = if in_group && matches!(kind, EditKind::Move(_)) {
            self.selection()
                .into_iter()
                .filter(|other| *other != id)
                .filter_map(|other| self.get(symbol, other).cloned())
                .filter(|d| !d.locked)
                .collect()
        } else {
            Vec::new()
        };
        self.editing = Some(Editing {
            symbol: symbol.to_owned(),
            id,
            original,
            group,
            before: self.drawings(symbol).to_vec(),
            kind,
        });
        Press::Taken
    }

    /// The pointer moved to `(x, y)`. With `constrain` (Shift held) a line being drawn keeps to
    /// a multiple of 45 degrees. Returns whether something on screen changed.
    pub fn pointer_moved(
        &mut self,
        symbol: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        constrain: bool,
    ) -> bool {
        if self.creating.as_ref().is_some_and(|c| c.symbol == symbol) {
            let (x, y) = if constrain {
                self.constrained(proj, x, y)
            } else {
                (x, y)
            };
            return self.move_creating(proj, x, y);
        }
        if self.editing.as_ref().is_some_and(|e| e.symbol == symbol) {
            return self.move_editing(proj, x, y);
        }
        false
    }

    /// Where the pointer is taken to be when Shift keeps the line from the last point placed to
    /// a multiple of 45 degrees: along that direction, as far as the pointer is.
    pub(super) fn constrained(&self, proj: &dyn Projection, x: f32, y: f32) -> (f32, f32) {
        let Some(creating) = self.creating.as_ref() else {
            return (x, y);
        };
        if creating.drawing.tool.is_freehand() || creating.placed == 0 {
            return (x, y);
        }
        let Some(from) = creating
            .drawing
            .points
            .get(creating.placed - 1)
            .and_then(|p| proj.to_screen(*p))
        else {
            return (x, y);
        };
        let (dx, dy) = (x - from.0, y - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length < 1.0 {
            return (x, y);
        }
        let step = std::f32::consts::FRAC_PI_4;
        let angle = (dy.atan2(dx) / step).round() * step;
        (from.0 + length * angle.cos(), from.1 + length * angle.sin())
    }

    pub(super) fn move_creating(&mut self, proj: &dyn Projection, x: f32, y: f32) -> bool {
        let magnet = self.magnet;
        let Some(point) = proj.point_at(x, y, magnet) else {
            return false;
        };
        let Some(creating) = self.creating.as_mut() else {
            return false;
        };
        if creating.drawing.tool.is_freehand() {
            // The stroke grows by pointer distance, whatever the bars are: `press` holds the
            // last position that added a point.
            let far_enough = creating.press.is_none_or(|last| {
                ((x - last.0).powi(2) + (y - last.1).powi(2)).sqrt() >= BRUSH_STEP
            });
            if far_enough && creating.drawing.points.len() < MAX_BRUSH_POINTS {
                creating.drawing.points.push(point);
                creating.press = Some((x, y));
                return true;
            }
            return false;
        }
        for later in creating.placed..creating.drawing.points.len() {
            creating.drawing.points[later] = point;
        }
        if let Some(press) = creating.press
            && ((x - press.0).powi(2) + (y - press.1).powi(2)).sqrt() >= DRAG_START
        {
            creating.dragged = true;
        }
        true
    }

    pub(super) fn move_editing(&mut self, proj: &dyn Projection, x: f32, y: f32) -> bool {
        let Some(editing) = self.editing.as_ref() else {
            return false;
        };
        let symbol = editing.symbol.clone();
        let Some(now) = proj.point_at(
            x,
            y,
            self.magnet && matches!(editing.kind, EditKind::Handle(_)),
        ) else {
            return false;
        };
        let mut updated = editing.original.clone();
        let mut moved_group: Vec<Drawing> = Vec::new();
        match editing.kind {
            EditKind::Handle(index) => apply_handle(&mut updated, index, now),
            EditKind::Move(start) => {
                let (Some(from), Some(to)) = (proj.index_of(start.t), proj.index_of(now.t)) else {
                    return false;
                };
                let (bars, price) = (to - from, now.p - start.p);
                let shift = |drawing: &mut Drawing| {
                    for point in &mut drawing.points {
                        if let Some(t) = proj.shift_bars(point.t, bars) {
                            point.t = t;
                            point.p += price;
                        }
                    }
                };
                shift(&mut updated);
                moved_group = editing.group.clone();
                for other in &mut moved_group {
                    shift(other);
                }
            }
        }
        let Some(list) = self.symbols.get_mut(&symbol) else {
            return false;
        };
        let mut changed = false;
        for next in std::iter::once(updated).chain(moved_group) {
            if let Some(slot) = list.iter_mut().find(|d| d.id == next.id)
                && *slot != next
            {
                *slot = next;
                changed = true;
            }
        }
        changed
    }

    /// The left button was released at `(x, y)`. Returns whether something changed.
    pub fn release(&mut self, symbol: &str, proj: &dyn Projection, x: f32, y: f32) -> bool {
        if self.creating.as_ref().is_some_and(|c| c.symbol == symbol) {
            let brush = self
                .creating
                .as_ref()
                .is_some_and(|c| c.drawing.tool.is_freehand());
            if brush {
                if let Some(done) = self.creating.take()
                    && done.drawing.points.len() >= 2
                {
                    self.commit(symbol, done.drawing);
                }
                return true;
            }
            // A point lands where the button is let go, whether the press was a click or a drag:
            // that is the one rule for every tool.
            let magnet = self.magnet;
            let Some(creating) = self.creating.as_mut() else {
                return false;
            };
            if !std::mem::take(&mut creating.holding) {
                return false;
            }
            let (origin, dragged) = (creating.origin, creating.dragged);
            creating.origin = false;
            creating.dragged = false;
            creating.press = None;
            // The press that began the drawing already placed its point. Unless it was dragged,
            // it was a click, and the next point waits for the next one.
            if origin && !dragged {
                return false;
            }
            // A click on the last point of an arrow path (a double click) ends it. Deciding it
            // here, and not on the press, leaves a drag that starts on the last point free to go
            // on to the next.
            let on_last = creating
                .last
                .is_some_and(|last| ((x - last.0).powi(2) + (y - last.1).powi(2)).sqrt() < 4.0);
            if creating.drawing.tool == Tool::ArrowPath && !dragged && on_last {
                if let Some(done) = self.creating.take() {
                    self.finish_path(done);
                }
                return true;
            }
            // Off the plot there is no point under the pointer: the drawing keeps the last one it
            // followed.
            let point = proj
                .point_at(x, y, magnet)
                .or_else(|| creating.drawing.points.get(creating.placed).copied());
            if let Some(point) = point {
                self.place_point(symbol, point, (x, y));
            }
            return true;
        }
        if let Some(editing) = self.editing.take() {
            let changed = self
                .get(&editing.symbol, editing.id)
                .is_some_and(|now| *now != editing.original);
            if changed {
                self.remember(Snapshot {
                    symbol: editing.symbol,
                    drawings: editing.before,
                });
            }
            return changed;
        }
        false
    }
}

/// Moves one point of a drawing, keeping the shape the tool needs: a position's stop and target
/// move only up and down, its right edge only sideways.
pub(super) fn apply_handle(drawing: &mut Drawing, index: usize, to: Point) {
    if drawing.tool.is_position() && drawing.points.len() == 4 {
        let left = drawing.points[0].t;
        match index {
            0 => {
                let (dt, dp) = (to.t - drawing.points[0].t, to.p - drawing.points[0].p);
                for point in &mut drawing.points {
                    point.t += dt;
                    point.p += dp;
                }
            }
            1 | 2 => {
                drawing.points[index] = Point { t: left, p: to.p };
                if index == 1 {
                    drawing.style.position.atr_stop = None;
                }
                if index == 2 {
                    drawing.style.position.target_rr = None;
                }
            }
            _ => {
                drawing.points[3] = Point {
                    t: to.t.max(left + 1),
                    p: drawing.points[0].p,
                };
            }
        }
        return;
    }
    if drawing.tool.is_box() && drawing.points.len() == 2 {
        let points = &mut drawing.points;
        match index {
            0 | 1 => points[index] = to,
            2 => {
                points[0].t = to.t;
                points[1].p = to.p;
            }
            3 => {
                points[1].t = to.t;
                points[0].p = to.p;
            }
            4 => points[0].p = to.p,
            5 => points[1].p = to.p,
            6 => points[0].t = to.t,
            _ => points[1].t = to.t,
        }
        return;
    }
    if let Some(point) = drawing.points.get_mut(index) {
        *point = to;
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::geometry::tests::Linear;

    use super::super::edit::apply_handle;

    use super::super::*;

    #[test]
    fn a_box_is_reshaped_from_any_corner_or_side() {
        let mut d = Drawing::new(
            1,
            Tool::Rectangle,
            vec![
                Point { t: 0, p: 100.0 },
                Point {
                    t: 600_000,
                    p: 200.0,
                },
            ],
        );
        apply_handle(
            &mut d,
            2,
            Point {
                t: 60_000,
                p: 250.0,
            },
        );
        assert_eq!(
            d.points[0],
            Point {
                t: 60_000,
                p: 100.0
            }
        );
        assert_eq!(
            d.points[1],
            Point {
                t: 600_000,
                p: 250.0
            }
        );
        apply_handle(&mut d, 7, Point { t: 900_000, p: 1.0 });
        assert_eq!(d.points[1].t, 900_000);
        assert_eq!(d.points[1].p, 250.0, "a side moves one way only");
        assert_eq!(
            super::super::super::geometry::hit::handles(&d, &Linear).len(),
            8
        );
    }
}
