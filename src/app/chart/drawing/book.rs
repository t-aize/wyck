//! The drawings of every symbol, and the rules for making and changing them with the pointer.
//!
//! This is a state machine with no window in it: a press, a move and a release come in with a
//! [`Projection`], and drawings come out. That keeps the fiddly parts (which click places which
//! point, what a drag moves, what undo restores) testable without a screen.
//!
//! Making a drawing takes one click for a single-point tool, and for the others either a click
//! per point or a press-drag-release for the first two. A brush is drawn by dragging. When a
//! drawing is finished it is selected and the tool goes back to the plain pointer.
//!
//! With no tool, a press picks up whatever drawing is under the pointer (a grip to reshape it,
//! anywhere else to move it whole). A press on nothing is left to the chart, which scrolls.

use std::collections::BTreeMap;

use super::geometry::{self, P, Part, Projection};
use super::model::{Drawing, DrawingsDoc, MAX_BRUSH_POINTS, MAX_DRAWINGS_PER_SYMBOL, Point, Tool};

/// How far a press must move before it counts as a drag, in pixels.
const DRAG_START: f32 = 6.0;
/// The least distance between two points of a brush stroke, in pixels.
const BRUSH_STEP: f32 = 3.0;
/// The most steps of undo kept.
const UNDO_DEPTH: usize = 200;
/// How wide a new position is, in bars, and how far its stop is, as a share of the price span.
const POSITION_BARS: f64 = 20.0;
const POSITION_RISK: f64 = 0.05;

/// What a press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// The press is for the chart (to scroll it).
    Ignored,
    /// A drawing took the press.
    Taken,
}

#[derive(Clone)]
struct Snapshot {
    symbol: String,
    drawings: Vec<Drawing>,
}

struct Creating {
    symbol: String,
    drawing: Drawing,
    /// How many points are fixed. The others follow the pointer.
    placed: usize,
    /// Where the press that began the drawing was, until it is released.
    press: Option<P>,
    dragged: bool,
}

enum EditKind {
    Handle(usize),
    /// Moving the whole drawing, from this point (where the press was).
    Move(Point),
}

struct Editing {
    symbol: String,
    id: u64,
    original: Drawing,
    before: Vec<Drawing>,
    kind: EditKind,
}

#[derive(Default)]
pub struct Book {
    symbols: BTreeMap<String, Vec<Drawing>>,
    next_id: u64,
    tool: Option<Tool>,
    magnet: bool,
    selected: Option<u64>,
    creating: Option<Creating>,
    editing: Option<Editing>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Counts the changes worth saving.
    revision: u64,
    /// Set when a text drawing was just made, so the field for its words can take the keyboard.
    wants_text_focus: bool,
}

impl Book {
    pub fn from_doc(doc: DrawingsDoc) -> Self {
        let doc = doc.normalized();
        Self {
            symbols: doc.symbols,
            next_id: doc.next_id,
            ..Self::default()
        }
    }

    pub fn to_doc(&self) -> DrawingsDoc {
        DrawingsDoc {
            next_id: self.next_id.max(1),
            symbols: self.symbols.clone(),
            ..DrawingsDoc::default()
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn tool(&self) -> Option<Tool> {
        self.tool
    }

    pub fn magnet(&self) -> bool {
        self.magnet
    }

    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    pub fn drawings(&self, symbol: &str) -> &[Drawing] {
        self.symbols.get(symbol).map_or(&[], Vec::as_slice)
    }

    pub fn get(&self, symbol: &str, id: u64) -> Option<&Drawing> {
        self.drawings(symbol).iter().find(|d| d.id == id)
    }

    /// The drawing being made, for drawing it while the pointer decides its last point.
    pub fn creating(&self, symbol: &str) -> Option<&Drawing> {
        self.creating
            .as_ref()
            .filter(|c| c.symbol == symbol)
            .map(|c| &c.drawing)
    }

    pub fn is_busy(&self) -> bool {
        self.tool.is_some() || self.creating.is_some() || self.editing.is_some()
    }

    /// Whether a text drawing was just made and its words are waiting for the keyboard.
    pub fn wants_text_focus(&self) -> bool {
        self.wants_text_focus
    }

    /// Whether the keyboard should go to the words of the drawing just made. Reading it clears it.
    pub fn take_text_focus(&mut self) -> bool {
        std::mem::take(&mut self.wants_text_focus)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn count(&self, symbol: &str) -> usize {
        self.drawings(symbol).len()
    }

    // ---- tools ----

    /// Picks a tool (or, with `None`, the plain pointer), dropping a drawing half made.
    pub fn set_tool(&mut self, tool: Option<Tool>) {
        self.creating = None;
        self.tool = tool;
        if tool.is_some() {
            self.selected = None;
        }
    }

    pub fn set_magnet(&mut self, magnet: bool) {
        self.magnet = magnet;
    }

    /// Escape: gives up what is in progress, one level at a time (a half made drawing, then the
    /// tool, then the selection). Returns whether there was anything to give up.
    pub fn cancel(&mut self) -> bool {
        if self.creating.take().is_some() {
            return true;
        }
        if self.editing.take().is_some() {
            return true;
        }
        if self.tool.take().is_some() {
            return true;
        }
        self.selected.take().is_some()
    }

    // ---- history ----

    fn snapshot(&self, symbol: &str) -> Snapshot {
        Snapshot {
            symbol: symbol.to_owned(),
            drawings: self.drawings(symbol).to_vec(),
        }
    }

    fn remember(&mut self, before: Snapshot) {
        self.undo.push(before);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
    }

    fn restore(&mut self, snapshot: Snapshot) -> Snapshot {
        let current = self.snapshot(&snapshot.symbol);
        if snapshot.drawings.is_empty() {
            self.symbols.remove(&snapshot.symbol);
        } else {
            self.symbols
                .insert(snapshot.symbol.clone(), snapshot.drawings);
        }
        if self
            .selected
            .is_some_and(|id| self.get(&snapshot.symbol, id).is_none())
        {
            self.selected = None;
        }
        self.revision += 1;
        current
    }

    pub fn undo(&mut self) -> bool {
        self.creating = None;
        self.editing = None;
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = self.restore(previous);
        self.redo.push(current);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = self.restore(next);
        self.undo.push(current);
        true
    }

    // ---- making a drawing ----

    fn new_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn blank(&mut self, tool: Tool, points: Vec<Point>) -> Drawing {
        Drawing {
            id: self.new_id(),
            tool,
            points,
            style: tool.default_style(),
            text: String::new(),
            locked: false,
            hidden: false,
        }
    }

    /// Puts a finished drawing in its symbol, selects it and goes back to the pointer.
    fn commit(&mut self, symbol: &str, drawing: Drawing) {
        if self.count(symbol) >= MAX_DRAWINGS_PER_SYMBOL {
            return;
        }
        let before = self.snapshot(symbol);
        self.remember(before);
        self.selected = Some(drawing.id);
        self.wants_text_focus = drawing.tool.has_text();
        self.symbols
            .entry(symbol.to_owned())
            .or_default()
            .push(drawing);
        self.creating = None;
        self.tool = None;
    }

    /// The four points of a new position: entry, stop and target at the same time, and the right
    /// edge. The stop is a share of the price span away and the target twice as far.
    fn position_points(tool: Tool, at: Point, proj: &dyn Projection) -> Vec<Point> {
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
        if let Some(tool) = self.tool {
            let Some(point) = proj.point_at(x, y, self.magnet) else {
                return Press::Taken;
            };
            return self.press_with_tool(symbol, tool, point, (x, y), proj);
        }
        self.press_to_edit(symbol, timeframe, proj, x, y)
    }

    fn press_with_tool(
        &mut self,
        symbol: &str,
        tool: Tool,
        point: Point,
        at: P,
        proj: &dyn Projection,
    ) -> Press {
        // A drawing already begun takes this click as its next point.
        if let Some(creating) = self.creating.as_mut().filter(|c| c.symbol == symbol) {
            creating.drawing.points[creating.placed] = point;
            creating.placed += 1;
            for later in creating.placed..creating.drawing.points.len() {
                creating.drawing.points[later] = point;
            }
            creating.press = Some(at);
            creating.dragged = false;
            if creating.placed >= tool.anchors()
                && let Some(done) = self.creating.take()
            {
                self.commit(symbol, done.drawing);
            }
            return Press::Taken;
        }
        if tool.is_position() {
            let points = Self::position_points(tool, point, proj);
            let drawing = self.blank(tool, points);
            self.commit(symbol, drawing);
        } else if tool.is_single_click() {
            let drawing = self.blank(tool, vec![point; tool.anchors()]);
            self.commit(symbol, drawing);
        } else {
            let points = if tool == Tool::Brush {
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
                dragged: false,
            });
        }
        Press::Taken
    }

    fn press_to_edit(
        &mut self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
    ) -> Press {
        let at = (x, y);
        // The selected drawing's grips come first, wherever it is in the stack.
        let mut found: Option<(u64, Part)> = None;
        if let Some(id) = self.selected
            && let Some(drawing) = self.get(symbol, id).filter(|d| d.shows_on(timeframe))
            && let Some(part @ Part::Handle(_)) = geometry::hit(drawing, proj, at, true)
        {
            found = Some((id, part));
        }
        if found.is_none() {
            found = self.hit(symbol, timeframe, proj, at);
        }
        let Some((id, part)) = found else {
            self.selected = None;
            return Press::Ignored;
        };
        self.selected = Some(id);
        let Some(original) = self.get(symbol, id).cloned() else {
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
        self.editing = Some(Editing {
            symbol: symbol.to_owned(),
            id,
            original,
            before: self.drawings(symbol).to_vec(),
            kind,
        });
        Press::Taken
    }

    /// The pointer moved to `(x, y)`. Returns whether something on screen changed.
    pub fn pointer_moved(&mut self, symbol: &str, proj: &dyn Projection, x: f32, y: f32) -> bool {
        if self.creating.as_ref().is_some_and(|c| c.symbol == symbol) {
            return self.move_creating(proj, x, y);
        }
        if self.editing.as_ref().is_some_and(|e| e.symbol == symbol) {
            return self.move_editing(proj, x, y);
        }
        false
    }

    fn move_creating(&mut self, proj: &dyn Projection, x: f32, y: f32) -> bool {
        let magnet = self.magnet;
        let Some(point) = proj.point_at(x, y, magnet) else {
            return false;
        };
        let Some(creating) = self.creating.as_mut() else {
            return false;
        };
        if creating.drawing.tool == Tool::Brush {
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

    fn move_editing(&mut self, proj: &dyn Projection, x: f32, y: f32) -> bool {
        let Some(editing) = self.editing.as_ref() else {
            return false;
        };
        let (symbol, id) = (editing.symbol.clone(), editing.id);
        let Some(now) = proj.point_at(
            x,
            y,
            self.magnet && matches!(editing.kind, EditKind::Handle(_)),
        ) else {
            return false;
        };
        let mut updated = editing.original.clone();
        match editing.kind {
            EditKind::Handle(index) => apply_handle(&mut updated, index, now),
            EditKind::Move(start) => {
                let (Some(from), Some(to)) = (proj.index_of(start.t), proj.index_of(now.t)) else {
                    return false;
                };
                let (bars, price) = (to - from, now.p - start.p);
                for point in &mut updated.points {
                    if let Some(t) = proj.shift_bars(point.t, bars) {
                        point.t = t;
                        point.p += price;
                    }
                }
            }
        }
        match self
            .symbols
            .get_mut(&symbol)
            .and_then(|list| list.iter_mut().find(|d| d.id == id))
        {
            Some(slot) if *slot != updated => {
                *slot = updated;
                true
            }
            _ => false,
        }
    }

    /// The left button was released at `(x, y)`. Returns whether something changed.
    pub fn release(&mut self, symbol: &str, proj: &dyn Projection, x: f32, y: f32) -> bool {
        if self.creating.as_ref().is_some_and(|c| c.symbol == symbol) {
            let brush = self
                .creating
                .as_ref()
                .is_some_and(|c| c.drawing.tool == Tool::Brush);
            if brush {
                if let Some(done) = self.creating.take()
                    && done.drawing.points.len() >= 2
                {
                    self.commit(symbol, done.drawing);
                }
                return true;
            }
            // A drag from the first point finishes the second, as a click on it would.
            let dragged = self
                .creating
                .as_ref()
                .is_some_and(|c| c.dragged && c.placed == 1);
            if dragged {
                self.move_creating(proj, x, y);
                if let Some(tool) = self.tool
                    && let Some(point) = proj.point_at(x, y, self.magnet)
                {
                    self.press_with_tool(symbol, tool, point, (x, y), proj);
                }
                return true;
            }
            if let Some(creating) = self.creating.as_mut() {
                creating.press = None;
            }
            return false;
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

    /// The topmost drawing shown on `timeframe` under `at`, and the part of it.
    fn hit(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        at: P,
    ) -> Option<(u64, Part)> {
        let selected = self.selected;
        self.drawings(symbol)
            .iter()
            .rev()
            .filter(|drawing| drawing.shows_on(timeframe))
            .find_map(|drawing| {
                geometry::hit(drawing, proj, at, selected == Some(drawing.id))
                    .map(|part| (drawing.id, part))
            })
    }

    /// What is under the pointer, for choosing the mouse cursor.
    pub fn hover_part(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        x: f32,
        y: f32,
    ) -> Option<Part> {
        if self.tool.is_some() {
            return None;
        }
        self.hit(symbol, timeframe, proj, (x, y))
            .map(|(_, part)| part)
    }

    // ---- changing what exists ----

    fn edit_selected(&mut self, symbol: &str, change: impl FnOnce(&mut Drawing)) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        let before = self.snapshot(symbol);
        let Some(drawing) = self
            .symbols
            .get_mut(symbol)
            .and_then(|list| list.iter_mut().find(|d| d.id == id))
        else {
            return false;
        };
        let original = drawing.clone();
        change(drawing);
        if *drawing == original {
            return false;
        }
        self.remember(before);
        true
    }

    pub fn set_color(&mut self, symbol: &str, color: u32) -> bool {
        self.edit_selected(symbol, |d| d.style.color = color)
    }

    pub fn set_width(&mut self, symbol: &str, width: f32) -> bool {
        self.edit_selected(symbol, |d| d.style.width = width)
    }

    pub fn set_dash(&mut self, symbol: &str, dash: super::model::Dash) -> bool {
        self.edit_selected(symbol, |d| d.style.dash = dash)
    }

    pub fn toggle_fill(&mut self, symbol: &str) -> bool {
        self.edit_selected(symbol, |d| d.style.fill = !d.style.fill)
    }

    pub fn toggle_lock(&mut self, symbol: &str) -> bool {
        self.edit_selected(symbol, |d| d.locked = !d.locked)
    }

    /// Sets the words of the selected text drawing. Typing is not an undo step of its own.
    pub fn set_text(&mut self, symbol: &str, text: &str) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        let Some(drawing) = self
            .symbols
            .get_mut(symbol)
            .and_then(|list| list.iter_mut().find(|d| d.id == id))
        else {
            return false;
        };
        if drawing.text == text {
            return false;
        }
        text.clone_into(&mut drawing.text);
        self.revision += 1;
        true
    }

    /// Duplicates the selected drawing, a little to the side, and selects the copy.
    pub fn duplicate(&mut self, symbol: &str, proj: &dyn Projection) -> bool {
        let Some(original) = self.selected.and_then(|id| self.get(symbol, id)).cloned() else {
            return false;
        };
        if self.count(symbol) >= MAX_DRAWINGS_PER_SYMBOL {
            return false;
        }
        let mut copy = original;
        copy.id = self.new_id();
        copy.locked = false;
        let step = proj.price_span() * 0.03;
        for point in &mut copy.points {
            point.t = proj.shift_bars(point.t, 3.0).unwrap_or(point.t);
            point.p -= step;
        }
        let before = self.snapshot(symbol);
        self.remember(before);
        self.selected = Some(copy.id);
        self.symbols
            .entry(symbol.to_owned())
            .or_default()
            .push(copy);
        true
    }

    /// Deletes the selected drawing, unless it is locked.
    pub fn delete_selected(&mut self, symbol: &str) -> bool {
        let Some(id) = self.selected else {
            return false;
        };
        if self.get(symbol, id).is_none_or(|d| d.locked) {
            return false;
        }
        let before = self.snapshot(symbol);
        self.remember(before);
        if let Some(list) = self.symbols.get_mut(symbol) {
            list.retain(|d| d.id != id);
            if list.is_empty() {
                self.symbols.remove(symbol);
            }
        }
        self.selected = None;
        true
    }

    /// Deletes every drawing of the symbol that is not locked. One undo step brings them back.
    pub fn clear(&mut self, symbol: &str) -> bool {
        if self.drawings(symbol).iter().all(|d| d.locked) {
            return false;
        }
        let before = self.snapshot(symbol);
        self.remember(before);
        if let Some(list) = self.symbols.get_mut(symbol) {
            list.retain(|d| d.locked);
            if list.is_empty() {
                self.symbols.remove(symbol);
            }
        }
        self.selected = None;
        true
    }
}

/// Moves one point of a drawing, keeping the shape the tool needs: a position's stop and target
/// move only up and down, its right edge only sideways.
fn apply_handle(drawing: &mut Drawing, index: usize, to: Point) {
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
            1 | 2 => drawing.points[index] = Point { t: left, p: to.p },
            _ => {
                drawing.points[3] = Point {
                    t: to.t.max(left + 1),
                    p: drawing.points[0].p,
                };
            }
        }
        return;
    }
    if let Some(point) = drawing.points.get_mut(index) {
        *point = to;
    }
}

#[cfg(test)]
mod tests {
    use super::super::geometry::tests::Linear;
    use super::*;

    const SYMBOL: &str = "US100.cash";
    const TF: &str = "M5";

    fn book() -> Book {
        Book::from_doc(DrawingsDoc::default())
    }

    /// Screen (x, y) for a time in seconds and a price on the test projection.
    fn at(seconds: f32, price: f32) -> (f32, f32) {
        (seconds, 500.0 - price)
    }

    fn click(book: &mut Book, seconds: f32, price: f32) {
        let (x, y) = at(seconds, price);
        book.press(SYMBOL, TF, &Linear, x, y);
        book.release(SYMBOL, &Linear, x, y);
    }

    #[test]
    fn a_trend_line_is_made_with_two_clicks_and_then_selected() {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 120.0, 100.0);
        assert_eq!(book.count(SYMBOL), 0, "one click is not enough");
        assert!(book.creating(SYMBOL).is_some());
        // The second point follows the pointer until it is clicked.
        let (x, y) = at(360.0, 200.0);
        assert!(book.pointer_moved(SYMBOL, &Linear, x, y));
        assert_eq!(book.creating(SYMBOL).unwrap().points[1].p, 200.0);
        click(&mut book, 360.0, 200.0);

        assert_eq!(book.count(SYMBOL), 1);
        let line = &book.drawings(SYMBOL)[0];
        assert_eq!(line.tool, Tool::TrendLine);
        assert_eq!(
            line.points[0],
            Point {
                t: 120_000,
                p: 100.0
            }
        );
        assert_eq!(
            line.points[1],
            Point {
                t: 360_000,
                p: 200.0
            }
        );
        assert_eq!(book.selected(), Some(line.id));
        assert_eq!(book.tool(), None, "back to the pointer");
    }

    #[test]
    fn dragging_from_the_first_point_finishes_a_two_point_drawing() {
        let mut book = book();
        book.set_tool(Some(Tool::Rectangle));
        let (x0, y0) = at(60.0, 100.0);
        book.press(SYMBOL, TF, &Linear, x0, y0);
        let (x1, y1) = at(300.0, 250.0);
        book.pointer_moved(SYMBOL, &Linear, x1, y1);
        book.release(SYMBOL, &Linear, x1, y1);
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(
            book.drawings(SYMBOL)[0].points[1],
            Point {
                t: 300_000,
                p: 250.0
            }
        );
    }

    #[test]
    fn a_small_wobble_on_the_first_click_is_not_a_drag() {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        let (x, y) = at(60.0, 100.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        book.pointer_moved(SYMBOL, &Linear, x + 2.0, y + 1.0);
        book.release(SYMBOL, &Linear, x + 2.0, y + 1.0);
        assert_eq!(book.count(SYMBOL), 0, "still waiting for the second click");
        assert!(book.creating(SYMBOL).is_some());
    }

    #[test]
    fn a_channel_takes_three_points() {
        let mut book = book();
        book.set_tool(Some(Tool::ParallelChannel));
        click(&mut book, 60.0, 100.0);
        click(&mut book, 360.0, 200.0);
        assert_eq!(book.count(SYMBOL), 0);
        click(&mut book, 180.0, 260.0);
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(book.drawings(SYMBOL)[0].points.len(), 3);
    }

    #[test]
    fn single_click_tools_finish_at_once_and_a_position_gets_sensible_defaults() {
        let mut book = book();
        book.set_tool(Some(Tool::HorizontalLine));
        click(&mut book, 60.0, 150.0);
        assert_eq!(book.count(SYMBOL), 1);

        book.set_tool(Some(Tool::LongPosition));
        click(&mut book, 120.0, 200.0);
        let long = &book.drawings(SYMBOL)[1];
        assert_eq!(long.points.len(), 4);
        let (entry, stop, target) = (long.points[0].p, long.points[1].p, long.points[2].p);
        assert!(stop < entry && target > entry, "{stop} {entry} {target}");
        assert!(
            ((target - entry) / (entry - stop) - 2.0).abs() < 1e-9,
            "reward is twice the risk"
        );
        assert!(long.points[3].t > long.points[0].t);

        book.set_tool(Some(Tool::ShortPosition));
        click(&mut book, 120.0, 200.0);
        let short = &book.drawings(SYMBOL)[2];
        assert!(short.points[1].p > short.points[0].p && short.points[2].p < short.points[0].p);
    }

    #[test]
    fn a_brush_collects_a_stroke_while_dragging() {
        let mut book = book();
        book.set_tool(Some(Tool::Brush));
        let (x, y) = at(60.0, 100.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        for step in 1..=10 {
            let (mx, my) = at(60.0 + step as f32 * 20.0, 100.0 + step as f32 * 5.0);
            book.pointer_moved(SYMBOL, &Linear, mx, my);
        }
        // A tiny move adds no point.
        let (tx, ty) = at(260.0, 150.0);
        let moved = book.pointer_moved(SYMBOL, &Linear, tx + 1.0, ty);
        assert!(!moved);
        book.release(SYMBOL, &Linear, tx, ty);
        assert_eq!(book.count(SYMBOL), 1);
        assert!(book.drawings(SYMBOL)[0].points.len() >= 8);
    }

    #[test]
    fn a_brush_that_never_moved_is_thrown_away() {
        let mut book = book();
        book.set_tool(Some(Tool::Brush));
        let (x, y) = at(60.0, 100.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        book.release(SYMBOL, &Linear, x, y);
        assert_eq!(book.count(SYMBOL), 0);
    }

    #[test]
    fn escape_gives_up_one_level_at_a_time() {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 100.0);
        assert!(book.cancel(), "the half made line");
        assert!(book.creating(SYMBOL).is_none());
        assert_eq!(book.tool(), Some(Tool::TrendLine));
        assert!(book.cancel(), "the tool");
        assert_eq!(book.tool(), None);
        assert!(!book.cancel(), "nothing left");
    }

    fn with_a_line() -> Book {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 120.0, 100.0);
        click(&mut book, 480.0, 300.0);
        book
    }

    #[test]
    fn a_press_on_a_drawing_selects_it_and_dragging_moves_it_whole() {
        let mut book = with_a_line();
        book.selected = None;
        // The middle of the line is at time 300 s and price 200.
        let (x, y) = at(300.0, 200.0);
        assert_eq!(book.press(SYMBOL, TF, &Linear, x, y), Press::Taken);
        assert_eq!(book.selected(), Some(book.drawings(SYMBOL)[0].id));
        let (nx, ny) = at(420.0, 260.0);
        assert!(book.pointer_moved(SYMBOL, &Linear, nx, ny));
        book.release(SYMBOL, &Linear, nx, ny);
        let line = &book.drawings(SYMBOL)[0];
        // Moved by 120 s (two bars) and +60 in price.
        assert_eq!(
            line.points[0],
            Point {
                t: 240_000,
                p: 160.0
            }
        );
        assert_eq!(
            line.points[1],
            Point {
                t: 600_000,
                p: 360.0
            }
        );
    }

    #[test]
    fn dragging_a_grip_reshapes_without_moving_the_other_end() {
        let mut book = with_a_line();
        let (gx, gy) = at(480.0, 300.0);
        book.press(SYMBOL, TF, &Linear, gx, gy);
        let (nx, ny) = at(660.0, 340.0);
        book.pointer_moved(SYMBOL, &Linear, nx, ny);
        book.release(SYMBOL, &Linear, nx, ny);
        let line = &book.drawings(SYMBOL)[0];
        assert_eq!(
            line.points[0],
            Point {
                t: 120_000,
                p: 100.0
            },
            "the first end stayed"
        );
        assert_eq!(
            line.points[1],
            Point {
                t: 660_000,
                p: 340.0
            }
        );
    }

    #[test]
    fn a_press_on_nothing_clears_the_selection_and_is_left_to_the_chart() {
        let mut book = with_a_line();
        assert!(book.selected().is_some());
        let (x, y) = at(900.0, 20.0);
        assert_eq!(book.press(SYMBOL, TF, &Linear, x, y), Press::Ignored);
        assert_eq!(book.selected(), None);
    }

    #[test]
    fn a_locked_drawing_is_selected_but_not_moved_or_deleted() {
        let mut book = with_a_line();
        assert!(book.toggle_lock(SYMBOL));
        let (x, y) = at(300.0, 200.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        let (nx, ny) = at(400.0, 300.0);
        assert!(!book.pointer_moved(SYMBOL, &Linear, nx, ny));
        book.release(SYMBOL, &Linear, nx, ny);
        assert_eq!(
            book.drawings(SYMBOL)[0].points[0],
            Point {
                t: 120_000,
                p: 100.0
            }
        );
        assert!(!book.delete_selected(SYMBOL));
        assert!(!book.clear(SYMBOL), "clearing leaves locked drawings alone");
        assert_eq!(book.count(SYMBOL), 1);
    }

    #[test]
    fn position_grips_keep_their_shape() {
        let mut book = book();
        book.set_tool(Some(Tool::LongPosition));
        click(&mut book, 120.0, 200.0);
        let before = book.drawings(SYMBOL)[0].clone();
        // Drag the stop grip sideways and down: only the price changes.
        let (sx, sy) = at(120.0, before.points[1].p as f32);
        book.press(SYMBOL, TF, &Linear, sx, sy);
        let (nx, ny) = at(300.0, 60.0);
        book.pointer_moved(SYMBOL, &Linear, nx, ny);
        book.release(SYMBOL, &Linear, nx, ny);
        let after = &book.drawings(SYMBOL)[0];
        assert_eq!(
            after.points[1],
            Point {
                t: before.points[0].t,
                p: 60.0
            }
        );
        assert_eq!(after.points[0], before.points[0]);
        assert_eq!(after.points[2], before.points[2]);
    }

    #[test]
    fn undo_and_redo_walk_through_every_change() {
        let mut book = with_a_line();
        assert_eq!(book.count(SYMBOL), 1);
        assert!(book.set_color(SYMBOL, 0xff6467));
        assert_eq!(book.drawings(SYMBOL)[0].style.color, 0xff6467);
        assert!(book.undo());
        assert_eq!(
            book.drawings(SYMBOL)[0].style.color,
            Tool::TrendLine.default_style().color
        );
        assert!(book.undo());
        assert_eq!(book.count(SYMBOL), 0);
        assert_eq!(book.selected(), None, "the selected drawing is gone");
        assert!(!book.undo());
        assert!(book.redo());
        assert_eq!(book.count(SYMBOL), 1);
        assert!(book.redo());
        assert_eq!(book.drawings(SYMBOL)[0].style.color, 0xff6467);
        assert!(!book.redo());
        // A new change forgets what could have been redone.
        book.undo();
        book.selected = book.drawings(SYMBOL).first().map(|d| d.id);
        assert!(book.set_width(SYMBOL, 4.0));
        assert!(!book.can_redo());
    }

    #[test]
    fn a_move_is_one_undo_step_however_long_the_drag() {
        let mut book = with_a_line();
        let steps_before = book.undo.len();
        let (x, y) = at(300.0, 200.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        for step in 1..=5 {
            let (nx, ny) = at(300.0 + step as f32 * 60.0, 200.0);
            book.pointer_moved(SYMBOL, &Linear, nx, ny);
        }
        let (nx, ny) = at(600.0, 200.0);
        book.release(SYMBOL, &Linear, nx, ny);
        assert_eq!(book.undo.len(), steps_before + 1);
        book.undo();
        assert_eq!(
            book.drawings(SYMBOL)[0].points[0],
            Point {
                t: 120_000,
                p: 100.0
            }
        );
    }

    #[test]
    fn a_press_and_release_without_moving_records_nothing() {
        let mut book = with_a_line();
        let steps_before = book.undo.len();
        let revision = book.revision();
        let (x, y) = at(300.0, 200.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        assert!(!book.release(SYMBOL, &Linear, x, y));
        assert_eq!(book.undo.len(), steps_before);
        assert_eq!(book.revision(), revision, "nothing to save");
    }

    #[test]
    fn deleting_clearing_and_duplicating() {
        let mut book = with_a_line();
        assert!(book.duplicate(SYMBOL, &Linear));
        assert_eq!(book.count(SYMBOL), 2);
        let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(book.selected(), Some(ids[1]), "the copy is selected");
        assert!(book.delete_selected(SYMBOL));
        assert_eq!(book.count(SYMBOL), 1);
        assert!(book.clear(SYMBOL));
        assert_eq!(book.count(SYMBOL), 0);
        assert!(book.undo(), "clearing is one step");
        assert_eq!(book.count(SYMBOL), 1);
    }

    #[test]
    fn drawings_belong_to_their_symbol() {
        let book = with_a_line();
        assert_eq!(book.count("EURUSD"), 0);
        assert!(book.creating("EURUSD").is_none());
        let (x, y) = at(300.0, 200.0);
        assert_eq!(book.hover_part("EURUSD", TF, &Linear, x, y), None);
        assert!(book.hover_part(SYMBOL, TF, &Linear, x, y).is_some());
    }

    #[test]
    fn text_is_edited_without_undo_steps_and_asks_for_the_keyboard() {
        let mut book = book();
        book.set_tool(Some(Tool::Text));
        click(&mut book, 120.0, 200.0);
        assert!(book.take_text_focus());
        assert!(!book.take_text_focus(), "asked once");
        let steps = book.undo.len();
        assert!(book.set_text(SYMBOL, "Breakout"));
        assert!(!book.set_text(SYMBOL, "Breakout"));
        assert_eq!(book.undo.len(), steps);
        assert_eq!(book.drawings(SYMBOL)[0].text, "Breakout");
    }

    #[test]
    fn the_book_round_trips_through_its_document() {
        let mut book = with_a_line();
        book.set_tool(Some(Tool::HorizontalLine));
        click(&mut book, 60.0, 150.0);
        let doc = book.to_doc();
        let text = toml::to_string_pretty(&doc).unwrap();
        let mut reloaded = Book::from_doc(toml::from_str(&text).unwrap());
        assert_eq!(reloaded.drawings(SYMBOL), book.drawings(SYMBOL));
        // New drawings do not reuse an id.
        reloaded.set_tool(Some(Tool::HorizontalLine));
        click(&mut reloaded, 60.0, 50.0);
        let ids: Vec<u64> = reloaded.drawings(SYMBOL).iter().map(|d| d.id).collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn a_symbol_cannot_hold_more_than_the_limit() {
        let mut book = book();
        for _ in 0..MAX_DRAWINGS_PER_SYMBOL + 5 {
            book.set_tool(Some(Tool::HorizontalLine));
            click(&mut book, 60.0, 150.0);
        }
        assert_eq!(book.count(SYMBOL), MAX_DRAWINGS_PER_SYMBOL);
    }
}
