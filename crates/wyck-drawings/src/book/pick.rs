//! What is under the pointer: which drawing, and which part of it.

use super::{Book, Grab};
use crate::geometry::{self, P, Part, Projection};
use crate::model::Drawing;

impl Book {
    /// The topmost drawing shown on `timeframe` under `at`, and the part of it.
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
        let selected = self.selected;
        self.drawings(symbol)
            .iter()
            .rev()
            .filter(|drawing| drawing.shows_on(timeframe))
            .find_map(|drawing| {
                resolve(drawing)
                    .and_then(|resolved| {
                        geometry::hit::hit(&resolved, proj, at, selected == Some(drawing.id))
                    })
                    .map(|part| (drawing.id, part))
            })
    }

    /// The id of the topmost drawing shown on `timeframe` under `(x, y)`.
    pub fn drawing_at(
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
        if self.tool.is_some() {
            return None;
        }
        let (id, part) = self.hit_resolved(symbol, timeframe, proj, (x, y), resolve)?;
        let Part::Handle(index) = part else {
            return Some(Grab::Body);
        };
        let Some(drawing) = self.get(symbol, id).and_then(resolve) else {
            return Some(Grab::Body);
        };
        if drawing.tool.is_position() && drawing.points.len() == 4 {
            return Some(match index {
                1 | 2 => Grab::GripVertical,
                3 => Grab::GripHorizontal,
                _ => Grab::Grip,
            });
        }
        if drawing.tool.is_box() && drawing.points.len() == 2 {
            let grips = geometry::hit::handles(&drawing, proj);
            if index >= 4 {
                return Some(if index < 6 {
                    Grab::GripVertical
                } else {
                    Grab::GripHorizontal
                });
            }
            // A corner: the diagonal it lies on, seen from the middle of the box.
            if let (Some(corner), Some(a), Some(b)) =
                (grips.get(index), grips.first(), grips.get(1))
            {
                let centre = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                let same_side = (corner.0 - centre.0) * (corner.1 - centre.1) >= 0.0;
                return Some(if same_side {
                    Grab::GripDiagonalDown
                } else {
                    Grab::GripDiagonalUp
                });
            }
        }
        Some(Grab::Grip)
    }
}
