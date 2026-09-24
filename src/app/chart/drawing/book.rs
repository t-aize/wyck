//! The drawings of every symbol, and the rules for making and changing them with the pointer.
//!
//! This is a state machine with no window in it: a press, a move and a release come in with a
//! [`Projection`], and drawings come out. That keeps the fiddly parts (which click places which
//! point, what a drag moves, what undo restores) testable without a screen.
//!
//! One rule makes a drawing, for every tool: a point lands where the pointer is when the button is
//! let go. A click places it there, and a press that is dragged and released places it at the end
//! of the drag, so each point of a many-point drawing can be either. The first point is the
//! exception in timing only: it lands where the button goes down, and a drag from it places the
//! second at the release. A single-point tool places on the press. A brush is drawn by holding the
//! button, since a stroke has no points to place. An arrow path takes as many points as it is
//! given and ends on a double click, Enter or Escape. Backspace takes back the last point. When
//! a drawing is finished it is selected and the tool goes back to the plain pointer.
//!
//! With no tool, a press picks up whatever drawing is under the pointer (a grip to reshape it,
//! anywhere else to move it whole). A press on nothing is left to the chart, which scrolls.

use std::collections::BTreeMap;

use super::geometry::{self, P, Part, Projection};
use super::model::{
    Drawing, DrawingsDoc, MAX_BRUSH_POINTS, MAX_DRAWINGS_PER_SYMBOL, MAX_PATH_POINTS, Point,
    Template, Tool,
};

/// How far a press must move before it counts as a drag, in pixels.
const DRAG_START: f32 = 6.0;
/// The least distance between two points of a brush stroke, in pixels.
const BRUSH_STEP: f32 = 3.0;
/// The most steps of undo kept.
const UNDO_DEPTH: usize = 200;
/// How wide a new position is, in bars, and how far its stop is, as a share of the price span.
const POSITION_BARS: f64 = 20.0;
const POSITION_RISK: f64 = 0.12;

/// What the pointer is over on a drawing, for choosing the mouse cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    /// The drawing itself: it can be picked and moved.
    Body,
    /// A grip that moves freely.
    Grip,
    /// A grip that slides up and down only.
    GripVertical,
    /// A grip that slides sideways only.
    GripHorizontal,
    /// A corner on the diagonal from top left to bottom right.
    GripDiagonalDown,
    /// A corner on the diagonal from bottom left to top right.
    GripDiagonalUp,
}

/// What a press did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// The press is for the chart (to scroll it).
    Ignored,
    /// A drawing took the press.
    Taken,
}

/// Where a drawing goes in the stack of its symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    Front,
    Forward,
    Backward,
    Back,
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
    /// Where the button went down, while it is held. A brush keeps here the last position that
    /// added a point.
    press: Option<P>,
    /// Where the last point was placed on the screen, to tell a double click on an arrow path.
    last: Option<P>,
    /// Whether the button is down. The next point lands where it is let go, so a click and a
    /// press-drag-release place a point the same way.
    holding: bool,
    /// Whether the held press is the one that began the drawing. Its point is placed at once, so
    /// letting go only places a second one if the press turned into a drag.
    origin: bool,
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
    /// What each tool starts with, by the tool's code.
    templates: BTreeMap<String, Template>,
    next_id: u64,
    tool: Option<Tool>,
    magnet: bool,
    /// Whether a tool stays picked once a drawing is finished, to draw several in a row.
    keep_tool: bool,
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
            templates: doc.templates,
            next_id: doc.next_id,
            ..Self::default()
        }
    }

    pub fn to_doc(&self) -> DrawingsDoc {
        DrawingsDoc {
            next_id: self.next_id.max(1),
            symbols: self.symbols.clone(),
            templates: self.templates.clone(),
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

    pub fn keep_tool(&self) -> bool {
        self.keep_tool
    }

    pub fn set_keep_tool(&mut self, keep: bool) {
        self.keep_tool = keep;
    }

    /// The tool of the drawing being made on `symbol`, how many of its points are placed, and
    /// how many it needs (none for a brush, which ends when the button is let go, or for an arrow
    /// path, which ends on a double click, Enter or Escape). While the button is down the next
    /// point is not placed yet: it lands where the button is let go.
    pub fn progress(&self, symbol: &str) -> Option<(Tool, usize, usize)> {
        let creating = self.creating.as_ref().filter(|c| c.symbol == symbol)?;
        let tool = creating.drawing.tool;
        let needed = if matches!(tool, Tool::Brush | Tool::Highlighter | Tool::ArrowPath) {
            0
        } else {
            tool.anchors()
        };
        Some((tool, creating.placed, needed))
    }

    pub fn set_magnet(&mut self, magnet: bool) {
        self.magnet = magnet;
    }

    /// Escape: gives up what is in progress, one level at a time (a half made drawing, then the
    /// tool, then the selection). Returns whether there was anything to give up. An arrow path
    /// with two points placed is finished instead: it has no last point to wait for.
    pub fn cancel(&mut self) -> bool {
        if let Some(creating) = self.creating.take() {
            self.finish_path(creating);
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
        let mut drawing = Drawing::new(self.new_id(), tool, points);
        if let Some(template) = self.templates.get(&tool.code()) {
            drawing.style = template.style.clone();
            drawing.levels = template.levels.clone();
        }
        drawing
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
        // A text is typed at once, so its tool always gives way; the others stay when asked.
        if !self.keep_tool || drawing_has_text(&self.symbols, symbol) {
            self.tool = None;
        }
    }

    /// Commits an arrow path that has at least two points placed, dropping the point that
    /// followed the pointer. Anything else is given up.
    fn finish_path(&mut self, mut creating: Creating) {
        if creating.drawing.tool != Tool::ArrowPath || creating.placed < 2 {
            return;
        }
        creating.drawing.points.truncate(creating.placed);
        self.commit(&creating.symbol, creating.drawing);
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
        // A drawing already begun: this press is for its next point, which lands where the button
        // is let go (see `release`), so the pointer can still be moved to it while it is down.
        if let Some(creating) = self.creating.as_mut().filter(|c| c.symbol == symbol) {
            creating.press = Some(at);
            creating.holding = true;
            creating.origin = false;
            creating.dragged = false;
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
    fn place_point(&mut self, symbol: &str, point: Point, at: P) {
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

    /// A right click while a drawing is being made on `symbol`: drops it, whatever it has, and
    /// keeps the tool. Returns whether there was one. With nothing to drop it does nothing, so the
    /// right click is left for its usual menu.
    pub fn abort(&mut self, symbol: &str) -> bool {
        if self.creating.as_ref().is_some_and(|c| c.symbol == symbol) {
            self.creating = None;
            return true;
        }
        false
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
    fn constrained(&self, proj: &dyn Projection, x: f32, y: f32) -> (f32, f32) {
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

    fn move_creating(&mut self, proj: &dyn Projection, x: f32, y: f32) -> bool {
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
        if self.tool.is_some() {
            return None;
        }
        let (id, part) = self.hit(symbol, timeframe, proj, (x, y))?;
        let Part::Handle(index) = part else {
            return Some(Grab::Body);
        };
        let Some(drawing) = self.get(symbol, id) else {
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
            let grips = geometry::handles(drawing, proj);
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

    // ---- changes from the settings dialog and the list of drawings ----

    /// Selects a drawing (or nothing), as a click on it would.
    pub fn select(&mut self, id: Option<u64>) {
        self.selected = id;
    }

    fn edit_one(&mut self, symbol: &str, id: u64, change: impl FnOnce(&mut Drawing)) -> bool {
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

    /// Shows the drawing as `drawing` says, without an undo step: the settings dialog shows its
    /// changes as they are made, and [`Book::settle`] or [`Book::revert`] ends them.
    pub fn preview(&mut self, symbol: &str, drawing: Drawing) -> bool {
        let drawing = drawing.normalized();
        let Some(slot) = self
            .symbols
            .get_mut(symbol)
            .and_then(|list| list.iter_mut().find(|d| d.id == drawing.id))
        else {
            return false;
        };
        if *slot == drawing {
            return false;
        }
        *slot = drawing;
        self.revision += 1;
        true
    }

    /// Keeps what was previewed, as one undo step back to `before` (the drawings of the symbol
    /// when the dialog opened).
    pub fn settle(&mut self, symbol: &str, before: Vec<Drawing>) -> bool {
        if self.drawings(symbol) == before.as_slice() {
            return false;
        }
        self.remember(Snapshot {
            symbol: symbol.to_owned(),
            drawings: before,
        });
        true
    }

    /// Puts back the drawings of the symbol as they were when the dialog opened.
    pub fn revert(&mut self, symbol: &str, before: Vec<Drawing>) -> bool {
        if self.drawings(symbol) == before.as_slice() {
            return false;
        }
        self.restore(Snapshot {
            symbol: symbol.to_owned(),
            drawings: before,
        });
        true
    }

    pub fn set_hidden(&mut self, symbol: &str, id: u64, hidden: bool) -> bool {
        self.edit_one(symbol, id, |d| d.hidden = hidden)
    }

    /// Turns a long position into a short one, or the other way: the same entry, with the stop and
    /// the target on the other side of it.
    pub fn flip_position(&mut self, symbol: &str, id: u64) -> bool {
        self.edit_one(symbol, id, |d| {
            if !d.tool.is_position() || d.points.len() != 4 {
                return;
            }
            let was = d.tool;
            let now = if was == Tool::LongPosition {
                Tool::ShortPosition
            } else {
                Tool::LongPosition
            };
            let entry = d.points[0].p;
            d.points[1].p = 2.0 * entry - d.points[1].p;
            d.points[2].p = 2.0 * entry - d.points[2].p;
            // A drawing still in its tool's own color takes the color of the other one.
            if d.style.color == was.default_style().color {
                d.style.color = now.default_style().color;
            }
            d.tool = now;
        })
    }

    pub fn set_locked(&mut self, symbol: &str, id: u64, locked: bool) -> bool {
        self.edit_one(symbol, id, |d| d.locked = locked)
    }

    /// Shows or hides every drawing of the symbol at once.
    pub fn set_all_hidden(&mut self, symbol: &str, hidden: bool) -> bool {
        let before = self.snapshot(symbol);
        let Some(list) = self.symbols.get_mut(symbol) else {
            return false;
        };
        let mut changed = false;
        for drawing in list.iter_mut().filter(|d| d.hidden != hidden) {
            drawing.hidden = hidden;
            changed = true;
        }
        if changed {
            self.remember(before);
        }
        changed
    }

    /// Deletes one drawing, unless it is locked.
    pub fn delete(&mut self, symbol: &str, id: u64) -> bool {
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
        if self.selected == Some(id) {
            self.selected = None;
        }
        true
    }

    /// Moves a drawing up or down the stack: the last one is drawn on top and picked first.
    pub fn reorder(&mut self, symbol: &str, id: u64, order: Order) -> bool {
        let before = self.snapshot(symbol);
        let Some(list) = self.symbols.get_mut(symbol) else {
            return false;
        };
        let Some(index) = list.iter().position(|d| d.id == id) else {
            return false;
        };
        let last = list.len() - 1;
        let target = match order {
            Order::Front => last,
            Order::Forward => (index + 1).min(last),
            Order::Backward => index.saturating_sub(1),
            Order::Back => 0,
        };
        if target == index {
            return false;
        }
        let drawing = list.remove(index);
        list.insert(target, drawing);
        self.remember(before);
        true
    }

    /// Makes the look and levels of this drawing what new drawings of its tool start with.
    pub fn save_template(&mut self, symbol: &str, id: u64) -> bool {
        let Some(drawing) = self.get(symbol, id) else {
            return false;
        };
        let template = Template {
            style: drawing.style.clone(),
            levels: drawing.levels.clone(),
        };
        self.templates.insert(drawing.tool.code(), template);
        self.revision += 1;
        true
    }

    /// Forgets the saved look of a tool, so new drawings start with the built-in one.
    pub fn forget_template(&mut self, tool: Tool) -> bool {
        let removed = self.templates.remove(&tool.code()).is_some();
        if removed {
            self.revision += 1;
        }
        removed
    }

    pub fn has_template(&self, tool: Tool) -> bool {
        self.templates.contains_key(&tool.code())
    }

    /// The look a new drawing of `tool` starts with: the saved one, or the built-in one.
    pub fn starting_style(&self, tool: Tool) -> (super::model::Style, Vec<super::model::Level>) {
        match self.templates.get(&tool.code()) {
            Some(template) => (template.style.clone(), template.levels.clone()),
            None => (tool.default_style(), Vec::new()),
        }
    }
}

/// Whether the drawing just added to `symbol` (the last one) has words to type.
fn drawing_has_text(symbols: &BTreeMap<String, Vec<Drawing>>, symbol: &str) -> bool {
    symbols
        .get(symbol)
        .and_then(|list| list.last())
        .is_some_and(|d| d.tool.has_text())
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
    fn an_arrow_path_takes_a_point_per_click_and_ends_on_a_double_click() {
        let mut book = book();
        book.set_tool(Some(Tool::ArrowPath));
        for (seconds, price) in [(60.0, 100.0), (180.0, 200.0), (300.0, 150.0)] {
            click(&mut book, seconds, price);
            let (x, y) = at(seconds + 40.0, price + 20.0);
            book.pointer_moved(SYMBOL, &Linear, x, y, false);
        }
        assert_eq!(book.count(SYMBOL), 0, "still being made");
        click(&mut book, 420.0, 250.0);
        click(&mut book, 420.0, 250.0);

        assert_eq!(book.count(SYMBOL), 1);
        let path = &book.drawings(SYMBOL)[0];
        assert_eq!(path.tool, Tool::ArrowPath);
        assert_eq!(path.points.len(), 4);
        assert!(path.is_valid());
    }

    #[test]
    fn every_tool_can_be_made_with_the_pointer() {
        for tool in Tool::ALL {
            let mut book = book();
            book.set_tool(Some(tool));
            if tool.is_freehand() {
                let (x, y) = at(60.0, 100.0);
                book.press(SYMBOL, TF, &Linear, x, y);
                for i in 1..8 {
                    let (x, y) = at(60.0 + 12.0 * i as f32, 100.0 + 6.0 * i as f32);
                    book.pointer_moved(SYMBOL, &Linear, x, y, false);
                }
                book.release(SYMBOL, &Linear, x, y);
            } else if tool == Tool::ArrowPath {
                click(&mut book, 60.0, 100.0);
                click(&mut book, 240.0, 200.0);
                assert!(book.cancel());
            } else {
                for i in 0..tool.anchors().max(1) {
                    click(
                        &mut book,
                        60.0 + 180.0 * i as f32,
                        100.0 + 40.0 * (i % 3) as f32,
                    );
                }
            }
            assert_eq!(book.count(SYMBOL), 1, "{tool:?} is not made");
            assert!(book.drawings(SYMBOL)[0].is_valid(), "{tool:?} is not valid");
        }
    }

    #[test]
    fn a_position_can_be_flipped_to_the_other_side() {
        let mut book = book();
        book.set_tool(Some(Tool::LongPosition));
        click(&mut book, 300.0, 200.0);
        let id = book.selected().unwrap();
        let before = book.get(SYMBOL, id).unwrap().clone();
        assert!(book.flip_position(SYMBOL, id));
        let flipped = book.get(SYMBOL, id).unwrap();
        assert_eq!(flipped.tool, Tool::ShortPosition);
        assert_eq!(flipped.points[0], before.points[0], "the entry stays");
        let entry = before.points[0].p;
        assert!((flipped.points[1].p - (2.0 * entry - before.points[1].p)).abs() < 1e-9);
        assert!((flipped.points[2].p - (2.0 * entry - before.points[2].p)).abs() < 1e-9);
        assert!(book.flip_position(SYMBOL, id));
        assert_eq!(book.get(SYMBOL, id).unwrap().points, before.points);
        assert!(book.undo() && book.undo());
        assert_eq!(book.get(SYMBOL, id).unwrap().tool, Tool::LongPosition);
        let line = {
            book.set_tool(Some(Tool::TrendLine));
            click(&mut book, 60.0, 100.0);
            click(&mut book, 180.0, 160.0);
            book.selected().unwrap()
        };
        assert!(!book.flip_position(SYMBOL, line), "only positions flip");
    }

    #[test]
    fn a_note_is_typed_at_once_and_a_marker_is_not() {
        let mut book = book();
        book.set_keep_tool(true);
        book.set_tool(Some(Tool::ArrowMarkUp));
        click(&mut book, 60.0, 100.0);
        click(&mut book, 180.0, 120.0);
        assert_eq!(book.count(SYMBOL), 2);
        assert_eq!(book.tool(), Some(Tool::ArrowMarkUp), "the tool stays");
        assert!(!book.wants_text_focus());

        book.set_tool(Some(Tool::Note));
        click(&mut book, 300.0, 150.0);
        assert!(book.wants_text_focus(), "a note asks for its words");
        assert_eq!(book.tool(), None, "and gives way to the pointer");
    }

    #[test]
    fn escape_finishes_an_arrow_path_with_two_points() {
        let mut book = book();
        book.set_tool(Some(Tool::ArrowPath));
        click(&mut book, 60.0, 100.0);
        assert!(book.cancel());
        assert_eq!(book.count(SYMBOL), 0, "one point is not a path");

        click(&mut book, 60.0, 100.0);
        click(&mut book, 200.0, 180.0);
        assert!(book.cancel());
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(book.drawings(SYMBOL)[0].points.len(), 2);
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
        assert!(book.pointer_moved(SYMBOL, &Linear, x, y, false));
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
        book.pointer_moved(SYMBOL, &Linear, x1, y1, false);
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
        book.pointer_moved(SYMBOL, &Linear, x + 2.0, y + 1.0, false);
        book.release(SYMBOL, &Linear, x + 2.0, y + 1.0);
        assert_eq!(book.count(SYMBOL), 0, "still waiting for the second click");
        assert!(book.creating(SYMBOL).is_some());
    }

    /// Presses at `from`, moves to `to` in steps, and lets go there.
    fn drag(book: &mut Book, from: (f32, f32), to: (f32, f32)) {
        let (x0, y0) = at(from.0, from.1);
        book.press(SYMBOL, TF, &Linear, x0, y0);
        for step in 1..=4 {
            let k = step as f32 / 4.0;
            let (x, y) = at(from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
            book.pointer_moved(SYMBOL, &Linear, x, y, false);
        }
        let (x1, y1) = at(to.0, to.1);
        book.release(SYMBOL, &Linear, x1, y1);
    }

    #[test]
    fn every_point_of_a_many_point_drawing_can_be_a_drag_or_a_click() {
        // Two drags: the first places two points, the second one more.
        let mut book = book();
        book.set_tool(Some(Tool::ParallelChannel));
        drag(&mut book, (60.0, 100.0), (360.0, 200.0));
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
        // The point is where the pointer is let go, not where it went down.
        drag(&mut book, (360.0, 200.0), (300.0, 40.0));
        assert_eq!(book.count(SYMBOL), 1);
        let channel = &book.drawings(SYMBOL)[0];
        assert_eq!(channel.points[1].t, 360_000);
        assert_eq!(
            channel.points[2],
            Point {
                t: 300_000,
                p: 40.0
            }
        );

        // A drag, then a click for the last point.
        let mut book = self::book();
        book.set_tool(Some(Tool::ParallelChannel));
        drag(&mut book, (60.0, 100.0), (360.0, 200.0));
        click(&mut book, 240.0, 30.0);
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(book.drawings(SYMBOL)[0].points[2].p, 30.0);
    }

    #[test]
    fn a_middle_point_is_not_placed_until_the_button_is_let_go() {
        let mut book = book();
        book.set_tool(Some(Tool::Abcd));
        click(&mut book, 60.0, 100.0);
        drag(&mut book, (150.0, 150.0), (200.0, 220.0));
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
        assert_eq!(book.creating(SYMBOL).unwrap().points[1].p, 220.0);

        // Held down, the point has not landed yet.
        let (x, y) = at(300.0, 120.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
        book.release(SYMBOL, &Linear, x, y);
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(3));
    }

    #[test]
    fn dragging_places_the_points_of_an_arrow_path_too() {
        let mut book = book();
        book.set_tool(Some(Tool::ArrowPath));
        drag(&mut book, (60.0, 100.0), (200.0, 180.0));
        drag(&mut book, (200.0, 180.0), (320.0, 120.0));
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(3));
        assert!(book.finish());
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(book.drawings(SYMBOL)[0].points.len(), 3);
    }

    #[test]
    fn enter_ends_an_arrow_path_and_leaves_the_other_tools_waiting() {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 100.0);
        assert!(!book.finish(), "a line still needs its second point");
        assert!(book.creating(SYMBOL).is_some());

        book.set_tool(Some(Tool::ArrowPath));
        click(&mut book, 60.0, 100.0);
        click(&mut book, 200.0, 180.0);
        assert!(book.finish());
        assert_eq!(book.count(SYMBOL), 1);
        assert!(!book.is_creating());
        assert!(!book.finish(), "nothing left to end");
    }

    #[test]
    fn backspace_takes_back_the_last_point_then_gives_up_the_drawing() {
        let mut book = book();
        book.set_tool(Some(Tool::ParallelChannel));
        assert!(!book.remove_last_point(), "nothing is being made");
        click(&mut book, 60.0, 100.0);
        click(&mut book, 360.0, 200.0);
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(2));
        assert!(book.remove_last_point());
        assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(1));
        // The drawing goes on from the first point.
        click(&mut book, 240.0, 150.0);
        click(&mut book, 300.0, 40.0);
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(book.drawings(SYMBOL)[0].points[1].t, 240_000);

        book.set_tool(Some(Tool::ArrowPath));
        click(&mut book, 60.0, 100.0);
        click(&mut book, 200.0, 180.0);
        click(&mut book, 320.0, 120.0);
        assert!(book.remove_last_point());
        assert!(book.finish());
        assert_eq!(book.drawings(SYMBOL)[1].points.len(), 2);

        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 100.0);
        assert!(book.remove_last_point(), "only the first point: gives up");
        assert!(!book.is_creating());
    }

    #[test]
    fn a_right_click_drops_the_drawing_being_made_and_only_that() {
        let mut book = book();
        book.set_tool(Some(Tool::ArrowPath));
        assert!(
            !book.abort(SYMBOL),
            "nothing is being made: the menu is free"
        );
        click(&mut book, 60.0, 100.0);
        click(&mut book, 200.0, 180.0);
        assert!(
            !book.abort("EURUSD"),
            "another symbol's drawing is not this one"
        );
        assert!(book.abort(SYMBOL));
        assert!(!book.is_creating());
        assert_eq!(
            book.count(SYMBOL),
            0,
            "dropped, not finished like Escape does"
        );
        assert_eq!(book.tool(), Some(Tool::ArrowPath), "the tool stays");
        assert!(!book.abort(SYMBOL), "the next right click is free again");
    }

    #[test]
    fn a_double_click_still_ends_an_arrow_path_made_by_dragging() {
        let mut book = book();
        book.set_tool(Some(Tool::ArrowPath));
        drag(&mut book, (60.0, 100.0), (200.0, 180.0));
        click(&mut book, 320.0, 120.0);
        click(&mut book, 320.0, 120.0);
        assert_eq!(book.count(SYMBOL), 1);
        assert_eq!(book.drawings(SYMBOL)[0].points.len(), 3);
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
            book.pointer_moved(SYMBOL, &Linear, mx, my, false);
        }
        // A tiny move adds no point.
        let (tx, ty) = at(260.0, 150.0);
        let moved = book.pointer_moved(SYMBOL, &Linear, tx + 1.0, ty, false);
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
        assert!(book.pointer_moved(SYMBOL, &Linear, nx, ny, false));
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
        book.pointer_moved(SYMBOL, &Linear, nx, ny, false);
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
        assert!(!book.pointer_moved(SYMBOL, &Linear, nx, ny, false));
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
        book.pointer_moved(SYMBOL, &Linear, nx, ny, false);
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
            book.pointer_moved(SYMBOL, &Linear, nx, ny, false);
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
        assert_eq!(book.hover("EURUSD", TF, &Linear, x, y), None);
        assert!(book.hover(SYMBOL, TF, &Linear, x, y).is_some());
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

    fn two_lines() -> (Book, u64, u64) {
        let mut book = book();
        for (a, b) in [(100.0, 200.0), (150.0, 250.0)] {
            book.set_tool(Some(Tool::TrendLine));
            click(&mut book, 60.0, a);
            click(&mut book, 300.0, b);
        }
        let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
        (book, ids[0], ids[1])
    }

    #[test]
    fn a_settings_dialog_previews_then_keeps_as_one_step_or_puts_back() {
        let (mut book, first, _) = two_lines();
        let before = book.drawings(SYMBOL).to_vec();
        let steps = book.undo.len();
        let mut changed = book.get(SYMBOL, first).unwrap().clone();
        changed.style.width = 4.0;
        assert!(book.preview(SYMBOL, changed.clone()));
        changed.style.color = 0x123456;
        assert!(book.preview(SYMBOL, changed));
        assert_eq!(book.undo.len(), steps, "previews are not undo steps");
        assert!(book.settle(SYMBOL, before.clone()));
        assert_eq!(book.undo.len(), steps + 1, "keeping is one step");
        assert!(book.undo());
        assert_eq!(book.drawings(SYMBOL), before.as_slice());

        let mut changed = book.get(SYMBOL, first).unwrap().clone();
        changed.style.width = 4.0;
        book.preview(SYMBOL, changed);
        assert!(book.revert(SYMBOL, before.clone()));
        assert_eq!(book.drawings(SYMBOL), before.as_slice());
    }

    #[test]
    fn a_drawing_moves_up_and_down_the_stack() {
        let (mut book, first, second) = two_lines();
        assert!(
            !book.reorder(SYMBOL, second, Order::Front),
            "already on top"
        );
        assert!(book.reorder(SYMBOL, first, Order::Front));
        let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
        assert_eq!(ids, vec![second, first]);
        assert!(book.reorder(SYMBOL, first, Order::Backward));
        assert_eq!(book.drawings(SYMBOL)[0].id, first);
        assert!(book.undo());
        assert_eq!(book.drawings(SYMBOL)[1].id, first);
    }

    #[test]
    fn hiding_locking_and_deleting_by_id() {
        let (mut book, first, second) = two_lines();
        assert!(book.set_all_hidden(SYMBOL, true));
        assert!(book.drawings(SYMBOL).iter().all(|d| d.hidden));
        assert!(!book.drawings(SYMBOL)[0].shows_on(TF));
        assert!(book.set_hidden(SYMBOL, first, false));
        assert!(book.set_locked(SYMBOL, first, true));
        assert!(!book.delete(SYMBOL, first), "a locked drawing stays");
        assert!(book.delete(SYMBOL, second));
        assert_eq!(book.count(SYMBOL), 1);
    }

    #[test]
    fn a_saved_look_is_what_new_drawings_of_the_tool_start_with() {
        let (mut book, first, _) = two_lines();
        let mut changed = book.get(SYMBOL, first).unwrap().clone();
        changed.style.color = 0xabcdef;
        changed.style.extend_right = true;
        book.preview(SYMBOL, changed);
        assert!(book.save_template(SYMBOL, first));
        assert!(book.has_template(Tool::TrendLine));
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 300.0);
        click(&mut book, 300.0, 350.0);
        let made = book.drawings(SYMBOL).last().unwrap();
        assert_eq!(made.style.color, 0xabcdef);
        assert!(made.style.extend_right);

        // The saved look survives a save and a load.
        let reloaded = Book::from_doc(book.to_doc());
        assert!(reloaded.has_template(Tool::TrendLine));
        assert!(book.forget_template(Tool::TrendLine));
        assert_eq!(
            book.starting_style(Tool::TrendLine).0,
            Tool::TrendLine.default_style()
        );
    }

    #[test]
    fn a_right_click_finds_the_drawing_under_the_pointer() {
        let (book, _, second) = two_lines();
        // The second line runs from (60 s, 150) to (300 s, 250); its middle is (180, 200).
        let (x, y) = at(180.0, 200.0);
        assert_eq!(book.drawing_at(SYMBOL, TF, &Linear, x, y), Some(second));
        let (x, y) = at(180.0, 50.0);
        assert_eq!(book.drawing_at(SYMBOL, TF, &Linear, x, y), None);
    }

    #[test]
    fn a_five_point_pattern_takes_five_clicks() {
        let mut book = book();
        book.set_tool(Some(Tool::Xabcd));
        for (i, price) in [100.0, 200.0, 150.0, 180.0, 120.0].iter().enumerate() {
            assert_eq!(book.count(SYMBOL), 0);
            click(&mut book, 60.0 * (i as f32 + 1.0), *price);
        }
        assert_eq!(book.count(SYMBOL), 1);
        let pattern = &book.drawings(SYMBOL)[0];
        assert_eq!(pattern.points.len(), 5);
        assert!(pattern.is_valid());
    }

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
        assert_eq!(super::super::geometry::handles(&d, &Linear).len(), 8);
    }

    #[test]
    fn with_the_tool_kept_several_lines_are_drawn_in_a_row() {
        let mut book = book();
        book.set_keep_tool(true);
        book.set_tool(Some(Tool::TrendLine));
        for n in 0..3 {
            click(&mut book, 60.0, 100.0 + n as f32 * 20.0);
            assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(1));
            click(&mut book, 300.0, 150.0 + n as f32 * 20.0);
        }
        assert_eq!(book.count(SYMBOL), 3);
        assert_eq!(
            book.tool(),
            Some(Tool::TrendLine),
            "the tool is still picked"
        );
        // A text gives way at once, so its words can be typed.
        book.set_tool(Some(Tool::Text));
        click(&mut book, 100.0, 100.0);
        assert_eq!(book.tool(), None);
    }

    #[test]
    fn shift_keeps_a_line_to_45_degrees() {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 100.0);
        // Nearly flat: it lies flat.
        let (x, y) = at(300.0, 104.0);
        book.pointer_moved(SYMBOL, &Linear, x, y, true);
        let line = book.creating(SYMBOL).unwrap();
        assert!((line.points[1].p - 100.0).abs() < 1.0, "{:?}", line.points);
    }
}
