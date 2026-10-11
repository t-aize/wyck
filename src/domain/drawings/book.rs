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
    DEFAULT_DRAWINGS_PER_SYMBOL, Drawing, DrawingsDoc, MAX_BRUSH_POINTS, MAX_DRAWINGS_PER_SYMBOL,
    MAX_NAMED_TEMPLATES, MAX_PATH_POINTS, MAX_TEMPLATE_NAME, NamedTemplate, Point, Template, Tool,
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
    /// The other drawings moving with this one, as they were when the press began.
    group: Vec<Drawing>,
    before: Vec<Drawing>,
    kind: EditKind,
}

#[derive(Default)]
pub struct Book {
    symbols: BTreeMap<String, Vec<Drawing>>,
    drawing_limit: Option<usize>,
    /// What each tool starts with, by the tool's code.
    templates: BTreeMap<String, Template>,
    /// Looks saved under a name, each for one tool.
    named_templates: Vec<NamedTemplate>,
    next_id: u64,
    tool: Option<Tool>,
    magnet: bool,
    /// Whether a tool stays picked once a drawing is finished, to draw several in a row.
    keep_tool: bool,
    selected: Option<u64>,
    /// The drawings selected along with `selected`, by Shift and a click, a box or Ctrl+A.
    also: Vec<u64>,
    /// What was copied, to paste on any symbol. Not saved.
    clipboard: Vec<Drawing>,
    /// How many times the clipboard was pasted in a row, so each paste lands a step further.
    pasted: u32,
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
            named_templates: doc.named_templates,
            next_id: doc.next_id,
            ..Self::default()
        }
    }

    pub fn to_doc(&self) -> DrawingsDoc {
        DrawingsDoc {
            next_id: self.next_id.max(1),
            symbols: self.symbols.clone(),
            templates: self.templates.clone(),
            named_templates: self.named_templates.clone(),
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

    /// The drawings selected along with the main one.
    pub fn also(&self) -> &[u64] {
        &self.also
    }

    /// Every selected drawing, the main one first.
    pub fn selection(&self) -> Vec<u64> {
        self.selected
            .into_iter()
            .chain(self.also.iter().copied())
            .collect()
    }

    pub fn is_selected(&self, id: u64) -> bool {
        self.selected == Some(id) || self.also.contains(&id)
    }

    /// How many drawings are selected.
    pub fn selected_count(&self) -> usize {
        usize::from(self.selected.is_some()) + self.also.len()
    }

    /// Adds the drawing to the selection, or takes it out when it is in.
    pub fn toggle_select(&mut self, id: u64) {
        if self.selected == Some(id) {
            self.selected = if self.also.is_empty() {
                None
            } else {
                Some(self.also.remove(0))
            };
        } else if let Some(at) = self.also.iter().position(|other| *other == id) {
            self.also.remove(at);
        } else if self.selected.is_none() {
            self.selected = Some(id);
        } else {
            self.also.push(id);
        }
    }

    /// Selects every drawing of the symbol that shows on `timeframe`.
    pub fn select_all(&mut self, symbol: &str, timeframe: &str) -> bool {
        let ids: Vec<u64> = self
            .drawings(symbol)
            .iter()
            .filter(|d| d.shows_on(timeframe) && !d.hidden)
            .map(|d| d.id)
            .collect();
        if ids.is_empty() {
            return false;
        }
        self.tool = None;
        self.selected = Some(ids[0]);
        self.also = ids[1..].to_vec();
        true
    }

    /// Selects the drawings that have a point inside the box from `a` to `b` (screen positions),
    /// added to the selection when `add` is set.
    pub fn select_in_box(
        &mut self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        a: P,
        b: P,
        add: bool,
    ) -> bool {
        let (left, right) = (a.0.min(b.0), a.0.max(b.0));
        let (top, bottom) = (a.1.min(b.1), a.1.max(b.1));
        let inside = |d: &Drawing| {
            d.points.iter().any(|point| {
                proj.to_screen(*point).is_some_and(|at| {
                    at.0 >= left && at.0 <= right && at.1 >= top && at.1 <= bottom
                })
            })
        };
        let ids: Vec<u64> = self
            .drawings(symbol)
            .iter()
            .filter(|d| d.shows_on(timeframe) && !d.hidden && inside(d))
            .map(|d| d.id)
            .collect();
        let mut all = if add { self.selection() } else { Vec::new() };
        for id in ids {
            if !all.contains(&id) {
                all.push(id);
            }
        }
        let changed = all != self.selection();
        self.selected = all.first().copied();
        self.also = all.into_iter().skip(1).collect();
        changed
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

    pub fn set_drawing_limit(&mut self, limit: usize) {
        self.drawing_limit = Some(limit.clamp(1, MAX_DRAWINGS_PER_SYMBOL));
    }

    pub fn drawing_limit(&self) -> usize {
        self.drawing_limit.unwrap_or(DEFAULT_DRAWINGS_PER_SYMBOL)
    }

    /// Picks a tool (or, with `None`, the plain pointer), dropping a drawing half made.
    pub fn set_tool(&mut self, tool: Option<Tool>) {
        self.creating = None;
        self.tool = tool;
        if tool.is_some() {
            self.selected = None;
            self.also.clear();
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
        let had = self.selected.take().is_some();
        let had_more = !self.also.is_empty();
        self.also.clear();
        had || had_more
    }

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
        let symbol = snapshot.symbol.clone();
        let alive: Vec<u64> = self
            .also
            .iter()
            .copied()
            .filter(|id| self.get(&symbol, *id).is_some())
            .collect();
        self.also = alive;
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
        if self.count(symbol) >= self.drawing_limit() {
            self.creating = None;
            return;
        }
        let before = self.snapshot(symbol);
        self.remember(before);
        self.selected = Some(drawing.id);
        self.also.clear();
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

    #[allow(clippy::too_many_arguments)]
    fn press_to_edit(
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
            && let Some(part @ Part::Handle(_)) = geometry::hit(&resolved, proj, at, true)
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

    /// The topmost drawing shown on `timeframe` under `at`, and the part of it.
    fn hit(
        &self,
        symbol: &str,
        timeframe: &str,
        proj: &dyn Projection,
        at: P,
    ) -> Option<(u64, Part)> {
        self.hit_resolved(symbol, timeframe, proj, at, &|d| Some(d.clone()))
    }

    fn hit_resolved(
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
                        geometry::hit(&resolved, proj, at, selected == Some(drawing.id))
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
            let grips = geometry::handles(&drawing, proj);
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

    fn edit_selected(&mut self, symbol: &str, mut change: impl FnMut(&mut Drawing)) -> bool {
        let ids = self.selection();
        if ids.is_empty() {
            return false;
        }
        let before = self.snapshot(symbol);
        let mut changed = false;
        if let Some(list) = self.symbols.get_mut(symbol) {
            for drawing in list.iter_mut().filter(|d| ids.contains(&d.id)) {
                let original = drawing.clone();
                change(drawing);
                changed |= *drawing != original;
            }
        }
        if changed {
            self.remember(before);
        }
        changed
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

    /// Changes the look of the selected drawing as one undo step, for the quick controls of the
    /// style bar that have no setter of their own.
    pub fn edit_style(
        &mut self,
        symbol: &str,
        mut change: impl FnMut(&mut super::model::Style),
    ) -> bool {
        self.edit_selected(symbol, |d| change(&mut d.style))
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

    /// Duplicates the selected drawings, a little to the side, and selects the copies.
    pub fn duplicate(&mut self, symbol: &str, proj: &dyn Projection) -> bool {
        let originals: Vec<Drawing> = self
            .selection()
            .into_iter()
            .filter_map(|id| self.get(symbol, id).cloned())
            .collect();
        self.add_copies(symbol, originals, proj, 1)
    }

    /// Puts copies of `originals` on the symbol, `step` steps to the side and down, and selects
    /// them. The drawing limit stops it early.
    fn add_copies(
        &mut self,
        symbol: &str,
        originals: Vec<Drawing>,
        proj: &dyn Projection,
        step: u32,
    ) -> bool {
        if originals.is_empty() {
            return false;
        }
        let room = self.drawing_limit().saturating_sub(self.count(symbol));
        if room == 0 {
            return false;
        }
        let bars = 3.0 * f64::from(step);
        let down = proj.price_span() * 0.03 * f64::from(step);
        let before = self.snapshot(symbol);
        let mut ids = Vec::new();
        for original in originals.into_iter().take(room) {
            let mut copy = original;
            copy.id = self.new_id();
            copy.locked = false;
            for point in &mut copy.points {
                point.t = proj.shift_bars(point.t, bars).unwrap_or(point.t);
                point.p -= down;
            }
            ids.push(copy.id);
            self.symbols
                .entry(symbol.to_owned())
                .or_default()
                .push(copy);
        }
        self.remember(before);
        self.selected = ids.first().copied();
        self.also = ids.into_iter().skip(1).collect();
        true
    }

    /// Keeps a copy of the selected drawings, to paste.
    pub fn copy_selection(&mut self, symbol: &str) -> bool {
        let copied: Vec<Drawing> = self
            .selection()
            .into_iter()
            .filter_map(|id| self.get(symbol, id).cloned())
            .collect();
        if copied.is_empty() {
            return false;
        }
        self.clipboard = copied;
        self.pasted = 0;
        true
    }

    /// Whether something was copied.
    pub fn can_paste(&self) -> bool {
        !self.clipboard.is_empty()
    }

    /// Pastes what was copied, on any symbol. Each paste in a row lands a step further along.
    pub fn paste(&mut self, symbol: &str, proj: &dyn Projection) -> bool {
        self.pasted += 1;
        let copies = self.clipboard.clone();
        let done = self.add_copies(symbol, copies, proj, self.pasted);
        if !done {
            self.pasted -= 1;
        }
        done
    }

    /// Moves the selected drawings by `bars` bars and `steps` hundredths of the price span, which
    /// the arrow keys ask for. Locked drawings stay.
    pub fn nudge(&mut self, symbol: &str, proj: &dyn Projection, bars: f64, steps: f64) -> bool {
        let rise = proj.price_span() * 0.01 * steps;
        self.edit_selected(symbol, |d| {
            if d.locked {
                return;
            }
            for point in &mut d.points {
                if bars != 0.0 {
                    point.t = proj.shift_bars(point.t, bars).unwrap_or(point.t);
                }
                point.p += rise;
            }
        })
    }

    /// Deletes the selected drawings, except the locked ones.
    pub fn delete_selected(&mut self, symbol: &str) -> bool {
        let ids: Vec<u64> = self
            .selection()
            .into_iter()
            .filter(|id| self.get(symbol, *id).is_some_and(|d| !d.locked))
            .collect();
        if ids.is_empty() {
            return false;
        }
        let before = self.snapshot(symbol);
        self.remember(before);
        if let Some(list) = self.symbols.get_mut(symbol) {
            list.retain(|d| !ids.contains(&d.id));
            if list.is_empty() {
                self.symbols.remove(symbol);
            }
        }
        self.selected = None;
        self.also.clear();
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
        self.also.clear();
        true
    }

    /// Selects a drawing (or nothing), as a click on it would.
    pub fn select(&mut self, id: Option<u64>) {
        self.selected = id;
        self.also.clear();
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
        self.also.retain(|other| *other != id);
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

    /// The looks saved under a name for `tool`, in the order they were saved.
    pub fn named_templates(&self, tool: Tool) -> Vec<&NamedTemplate> {
        let code = tool.code();
        self.named_templates
            .iter()
            .filter(|t| t.tool == code)
            .collect()
    }

    /// Saves the look of a drawing under `name`, replacing the look of the same name for the same
    /// tool. Returns whether it was saved: a name that says nothing is refused, and so is one more
    /// than the book holds.
    pub fn save_named_template(&mut self, symbol: &str, id: u64, name: &str) -> bool {
        let name: String = name.trim().chars().take(MAX_TEMPLATE_NAME).collect();
        let Some(drawing) = self.get(symbol, id) else {
            return false;
        };
        if name.is_empty() {
            return false;
        }
        let template = NamedTemplate {
            name,
            tool: drawing.tool.code(),
            style: drawing.style.clone(),
            levels: drawing.levels.clone(),
        };
        let same = |t: &NamedTemplate| t.tool == template.tool && t.name == template.name;
        if let Some(slot) = self.named_templates.iter_mut().find(|t| same(t)) {
            *slot = template;
        } else if self.named_templates.len() < MAX_NAMED_TEMPLATES {
            self.named_templates.push(template);
        } else {
            return false;
        }
        self.revision += 1;
        true
    }

    /// Forgets the look saved under `name` for `tool`.
    pub fn delete_named_template(&mut self, tool: Tool, name: &str) -> bool {
        let code = tool.code();
        let before = self.named_templates.len();
        self.named_templates
            .retain(|t| !(t.tool == code && t.name == name));
        let removed = self.named_templates.len() != before;
        if removed {
            self.revision += 1;
        }
        removed
    }

    /// Gives the selected drawings of `tool` the look saved under `name` for that tool.
    pub fn apply_named_template(&mut self, symbol: &str, tool: Tool, name: &str) -> bool {
        let code = tool.code();
        let Some(template) = self
            .named_templates
            .iter()
            .find(|t| t.tool == code && t.name == name)
            .cloned()
        else {
            return false;
        };
        self.edit_selected(symbol, |d| {
            if d.tool == tool {
                d.style = template.style.clone();
                d.levels = template.levels.clone();
            }
        })
    }

    /// Gives the selected drawings of `tool` the look new ones of the tool start with.
    pub fn apply_starting_style(&mut self, symbol: &str, tool: Tool) -> bool {
        let (style, levels) = self.starting_style(tool);
        self.edit_selected(symbol, |d| {
            if d.tool == tool {
                d.style = style.clone();
                d.levels = levels.clone();
            }
        })
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
#[path = "../../../tests/unit/domain/drawings/book.rs"]
mod tests;
