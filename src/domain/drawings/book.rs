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

use super::geometry::{P, Projection};
use super::model::{
    DEFAULT_DRAWINGS_PER_SYMBOL, Drawing, DrawingsDoc, MAX_DRAWINGS_PER_SYMBOL, NamedTemplate,
    Point, Template, Tool,
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
}

mod actions;
mod create;
mod edit;
mod history;
mod pick;
mod templates;

use create::drawing_has_text;

#[cfg(test)]
#[path = "../../../tests/unit/domain/drawings/book.rs"]
mod tests;
