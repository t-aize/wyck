//! What is under the pointer: which drawing, and which part of it.

use super::{Book, Grab};
use crate::drawings::geometry::{self, P, Part, Projection};
use crate::drawings::model::Drawing;

impl Book {
    /// The topmost drawing shown on `timeframe` under `at`, and the part of it.
    #[cfg(test)]
    pub(super) fn hit(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        at: P,
    ) -> Option<(u64, Part)> {
        self.hit_resolved(symbol, timeframe, proj, at, &|d| Some(d.clone()))
    }

    pub(super) fn hit_resolved(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        at: P,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Option<(u64, Part)> {
        // Every drawing offers its grips to the pointer that is over it, not only the selected
        // one: they show on hover, and a press on one takes it.
        self.drawings(symbol)
            .iter()
            .rev()
            .filter(|drawing| self.shows(drawing, timeframe))
            .find_map(|drawing| {
                resolve(drawing)
                    .and_then(|resolved| geometry::hit::hit(&resolved, proj, at, true))
                    .map(|part| (drawing.id, part))
            })
    }

    /// What a press at `at` would take: a grip of the selected drawing wherever it is in the
    /// stack, else the topmost drawing under the pointer. Hover and press both ask this, so the
    /// cursor and the grips shown never disagree with what a press does.
    pub(super) fn pick_resolved(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        at: P,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Option<(u64, Part)> {
        if let Some(id) = self.selected
            && let Some(drawing) = self.get(symbol, id).filter(|d| self.shows(d, timeframe))
            && let Some(resolved) = resolve(drawing)
            && let Some(part @ Part::Handle(_)) = geometry::hit::hit(&resolved, proj, at, true)
        {
            return Some((id, part));
        }
        self.hit_resolved(symbol, timeframe, proj, at, resolve)
    }

    /// The id of the topmost drawing shown on `timeframe` under `(x, y)`.
    #[cfg(test)]
    pub(crate) fn drawing_at(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
    ) -> Option<u64> {
        self.hit(symbol, timeframe, proj, (x, y)).map(|(id, _)| id)
    }

    pub fn drawing_at_resolved(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Option<u64> {
        self.hit_resolved(symbol, timeframe, proj, (x, y), resolve)
            .map(|(id, _)| id)
    }

    /// What is under the pointer, told finely enough to pick a cursor: the drawing itself, a grip
    /// that moves freely, or a grip that only slides one way or along a diagonal.
    pub fn hover(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
    ) -> Option<Grab> {
        self.hover_resolved(symbol, timeframe, proj, x, y, &|d| Some(d.clone()))
    }

    pub fn hover_resolved(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Option<Grab> {
        self.hover_target_resolved(symbol, timeframe, proj, x, y, resolve)
            .map(|(_, grab)| grab)
    }

    /// [`Book::hover_resolved`] with the id of the drawing under the pointer, which shows its
    /// grips while the pointer is over it.
    pub fn hover_target_resolved(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
        resolve: &dyn Fn(&Drawing) -> Option<Drawing>,
    ) -> Option<(u64, Grab)> {
        if self.tool.is_some() {
            return None;
        }
        let (id, part) = self.pick_resolved(symbol, timeframe, proj, (x, y), resolve)?;
        let Part::Handle(index) = part else {
            return Some((id, Grab::Body));
        };
        let Some(drawing) = self.get(symbol, id).and_then(resolve) else {
            return Some((id, Grab::Body));
        };
        Some((id, grab_of_grip(&drawing, index, proj)))
    }
}

/// The cursor a grip asks for: free, sliding up and down, sideways, or along a diagonal.
fn grab_of_grip(drawing: &Drawing, index: usize, proj: &dyn Projection) -> Grab {
    if drawing.tool.is_position() && drawing.points.len() == 4 {
        return match index {
            1 | 2 => Grab::GripVertical,
            3 => Grab::GripHorizontal,
            _ => Grab::Grip,
        };
    }
    if drawing.tool.is_box() && drawing.points.len() == 2 {
        let grips = geometry::hit::handles(drawing, proj);
        if index >= 4 {
            return if index < 6 {
                Grab::GripVertical
            } else {
                Grab::GripHorizontal
            };
        }
        // A corner: the diagonal it lies on, seen from the middle of the box.
        if let (Some(corner), Some(a), Some(b)) = (grips.get(index), grips.first(), grips.get(1)) {
            let centre = (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
            let same_side = (corner.0 - centre.0) * (corner.1 - centre.1) >= 0.0;
            return if same_side {
                Grab::GripDiagonalDown
            } else {
                Grab::GripDiagonalUp
            };
        }
    }
    Grab::Grip
}
