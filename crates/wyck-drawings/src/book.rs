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

mod actions;
mod create;
mod edit;
mod history;
mod pick;
mod templates;

use super::geometry::{P, Projection};
use super::model::tool::Tool;
use super::model::{
    DEFAULT_DRAWINGS_PER_SYMBOL, Drawing, DrawingsDoc, MAX_DRAWINGS_PER_SYMBOL, NamedTemplate,
    Point, Template,
};
use std::collections::BTreeMap;

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

    pub(crate) fn is_selected(&self, id: u64) -> bool {
        self.selected == Some(id) || self.also.contains(&id)
    }

    /// How many drawings are selected.
    pub fn selected_count(&self) -> usize {
        usize::from(self.selected.is_some()) + self.also.len()
    }

    /// Adds the drawing to the selection, or takes it out when it is in.
    pub(crate) fn toggle_select(&mut self, id: u64) {
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
        let Some((first, rest)) = ids.split_first() else {
            return false;
        };
        self.tool = None;
        self.selected = Some(*first);
        self.also = rest.to_vec();
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

#[cfg(test)]
mod tests {
    use crate::geometry::tests::Linear;

    use super::*;
    use crate::model::{MAX_NAMED_TEMPLATES, MAX_TEMPLATE_NAME};
    use wyck_core::math::{count_f32, f32_of};

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

    /// Two trend lines, far apart, the way a user would draw them.
    fn apart_lines(book: &mut Book) {
        book.set_tool(Some(Tool::TrendLine));
        click(book, 60.0, 100.0);
        click(book, 240.0, 160.0);
        book.set_tool(Some(Tool::TrendLine));
        click(book, 60.0, 300.0);
        click(book, 240.0, 360.0);
        book.select(None);
    }

    /// Presses at `from`, moves to `to` in steps, and lets go there.
    fn drag(book: &mut Book, from: (f32, f32), to: (f32, f32)) {
        let (x0, y0) = at(from.0, from.1);
        book.press(SYMBOL, TF, &Linear, x0, y0);
        for step in 1..=4 {
            let k = count_f32(step) / 4.0;
            let (x, y) = at(from.0 + (to.0 - from.0) * k, from.1 + (to.1 - from.1) * k);
            book.pointer_moved(SYMBOL, &Linear, x, y, false);
        }
        let (x1, y1) = at(to.0, to.1);
        book.release(SYMBOL, &Linear, x1, y1);
    }

    fn with_a_line() -> Book {
        let mut book = book();
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 120.0, 100.0);
        click(&mut book, 480.0, 300.0);
        book
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
                    let (x, y) = at(60.0 + 12.0 * count_f32(i), 100.0 + 6.0 * count_f32(i));
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
                        60.0 + 180.0 * count_f32(i),
                        100.0 + 40.0 * count_f32(i % 3),
                    );
                }
            }
            assert_eq!(book.count(SYMBOL), 1, "{tool:?} is not made");
            assert!(book.drawings(SYMBOL)[0].is_valid(), "{tool:?} is not valid");
        }
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
    fn a_right_click_drops_the_drawing_being_made_and_gives_up_the_tool() {
        let mut book = book();
        assert!(!book.abort(SYMBOL), "no tool armed: the menu is free");

        book.set_tool(Some(Tool::ArrowPath));
        assert!(
            book.abort(SYMBOL),
            "a tool armed but idle still gives it up"
        );
        assert_eq!(book.tool(), None);

        book.set_tool(Some(Tool::ArrowPath));
        click(&mut book, 60.0, 100.0);
        click(&mut book, 200.0, 180.0);
        assert!(
            !book.abort("EURUSD"),
            "another chart's drawing still needs the tool, so this one is left alone"
        );
        assert_eq!(book.tool(), Some(Tool::ArrowPath));
        assert!(book.abort(SYMBOL));
        assert!(!book.is_creating());
        assert_eq!(
            book.count(SYMBOL),
            0,
            "dropped, not finished like Escape does"
        );
        assert_eq!(
            book.tool(),
            None,
            "back to the normal cursor, like TradingView"
        );
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
            let (mx, my) = at(60.0 + count_f32(step) * 20.0, 100.0 + count_f32(step) * 5.0);
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

    #[test]
    fn a_five_point_pattern_takes_five_clicks() {
        let mut book = book();
        book.set_tool(Some(Tool::Xabcd));
        for (i, price) in [100.0, 200.0, 150.0, 180.0, 120.0].iter().enumerate() {
            assert_eq!(book.count(SYMBOL), 0);
            click(&mut book, 60.0 * (count_f32(i) + 1.0), *price);
        }
        assert_eq!(book.count(SYMBOL), 1);
        let pattern = &book.drawings(SYMBOL)[0];
        assert_eq!(pattern.points.len(), 5);
        assert!(pattern.is_valid());
    }

    #[test]
    fn with_the_tool_kept_several_lines_are_drawn_in_a_row() {
        let mut book = book();
        book.set_keep_tool(true);
        book.set_tool(Some(Tool::TrendLine));
        for n in 0..3 {
            click(&mut book, 60.0, 100.0 + count_f32(n) * 20.0);
            assert_eq!(book.progress(SYMBOL).map(|p| p.1), Some(1));
            click(&mut book, 300.0, 150.0 + count_f32(n) * 20.0);
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
        let (sx, sy) = at(120.0, f32_of(before.points[1].p));
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
    fn a_right_click_finds_the_drawing_under_the_pointer() {
        let (book, _, second) = two_lines();
        // The second line runs from (60 s, 150) to (300 s, 250); its middle is (180, 200).
        let (x, y) = at(180.0, 200.0);
        assert_eq!(book.drawing_at(SYMBOL, TF, &Linear, x, y), Some(second));
        let (x, y) = at(180.0, 50.0);
        assert_eq!(book.drawing_at(SYMBOL, TF, &Linear, x, y), None);
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
    fn undo_and_redo_walk_through_every_change() {
        let mut book = with_a_line();
        assert_eq!(book.count(SYMBOL), 1);
        assert!(book.set_color(SYMBOL, 0xff_6467));
        assert_eq!(book.drawings(SYMBOL)[0].style.color, 0xff_6467);
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
        assert_eq!(book.drawings(SYMBOL)[0].style.color, 0xff_6467);
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
            let (nx, ny) = at(300.0 + count_f32(step) * 60.0, 200.0);
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
        book.set_drawing_limit(3);
        for _ in 0..5 {
            book.set_tool(Some(Tool::HorizontalLine));
            click(&mut book, 60.0, 150.0);
        }
        assert_eq!(book.count(SYMBOL), 3);
        let mut loaded = Book::from_doc(book.to_doc());
        loaded.set_drawing_limit(2);
        assert_eq!(
            loaded.count(SYMBOL),
            3,
            "lowering a limit keeps saved drawings"
        );
    }

    #[test]
    fn a_settings_dialog_previews_then_keeps_as_one_step_or_puts_back() {
        let (mut book, first, _) = two_lines();
        let before = book.drawings(SYMBOL).to_vec();
        let steps = book.undo.len();
        let mut changed = book.get(SYMBOL, first).unwrap().clone();
        changed.style.width = 4.0;
        assert!(book.preview(SYMBOL, changed.clone()));
        changed.style.color = 0x12_3456;
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
    fn a_new_drawing_leaves_the_selection_of_the_last_one_behind() {
        let (mut book, _, _) = two_lines();
        book.set_keep_tool(true);
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 300.0);
        click(&mut book, 300.0, 350.0);
        let made = book.selected();
        assert!(made.is_some(), "the drawing just made is selected");
        assert_eq!(book.tool(), Some(Tool::TrendLine), "the tool was kept");
        // The next drawing begins: the old one is no longer the selected one.
        click(&mut book, 80.0, 120.0);
        assert_eq!(book.selected(), None);
    }

    #[test]
    fn locking_every_drawing_at_once_can_be_undone() {
        let (mut book, first, second) = two_lines();
        assert!(book.set_all_locked(SYMBOL, true));
        assert!(book.drawings(SYMBOL).iter().all(|d| d.locked));
        assert!(!book.set_all_locked(SYMBOL, true), "already locked");
        assert!(!book.delete(SYMBOL, first) && !book.delete(SYMBOL, second));
        assert!(!book.set_all_locked("NOSUCH", true), "no drawings there");
        assert!(book.set_all_locked(SYMBOL, false));
        assert!(book.delete(SYMBOL, first));
        // The one step back puts the lock on again.
        assert!(book.undo());
        assert!(book.undo());
        assert!(book.drawings(SYMBOL).iter().all(|d| d.locked));
    }

    #[test]
    fn a_saved_look_is_what_new_drawings_of_the_tool_start_with() {
        let (mut book, first, _) = two_lines();
        let mut changed = book.get(SYMBOL, first).unwrap().clone();
        changed.style.color = 0xab_cdef;
        changed.style.extend_right = true;
        book.preview(SYMBOL, changed);
        assert!(book.save_template(SYMBOL, first));
        assert!(book.has_template(Tool::TrendLine));
        book.set_tool(Some(Tool::TrendLine));
        click(&mut book, 60.0, 300.0);
        click(&mut book, 300.0, 350.0);
        let made = book.drawings(SYMBOL).last().unwrap();
        assert_eq!(made.style.color, 0xab_cdef);
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
    fn a_look_can_be_saved_under_a_name_replaced_deleted_and_kept_in_the_file() {
        let (mut book, first, second) = two_lines();
        let mut red = book.get(SYMBOL, first).unwrap().clone();
        red.style.color = 0xff_0000;
        red.style.text_layout.background = true;
        book.preview(SYMBOL, red);
        let before = book.revision();
        assert!(book.save_named_template(SYMBOL, first, "  Resistance  "));
        assert!(book.revision() > before, "worth saving");
        assert!(
            !book.save_named_template(SYMBOL, first, "   "),
            "no name, no look"
        );
        assert!(
            !book.save_named_template(SYMBOL, 9_999, "Ghost"),
            "no such drawing"
        );

        let listed = book.named_templates(Tool::TrendLine);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "Resistance", "trimmed");
        assert_eq!(listed[0].style.color, 0xff_0000);
        assert!(
            book.named_templates(Tool::Rectangle).is_empty(),
            "each tool has its own"
        );

        // The same name replaces the look, another name adds one.
        let mut blue = book.get(SYMBOL, second).unwrap().clone();
        blue.style.color = 0x00_00ff;
        book.preview(SYMBOL, blue);
        assert!(book.save_named_template(SYMBOL, second, "Resistance"));
        assert!(book.save_named_template(SYMBOL, second, "Support"));
        let names: Vec<&str> = book
            .named_templates(Tool::TrendLine)
            .iter()
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(names, vec!["Resistance", "Support"]);
        assert_eq!(
            book.named_templates(Tool::TrendLine)[0].style.color,
            0x00_00ff
        );

        // They survive a save and a load, and a name is forgotten on demand.
        let reloaded = Book::from_doc(book.to_doc());
        assert_eq!(reloaded.named_templates(Tool::TrendLine).len(), 2);
        assert!(book.delete_named_template(Tool::TrendLine, "Support"));
        assert!(!book.delete_named_template(Tool::TrendLine, "Support"));
        assert_eq!(book.named_templates(Tool::TrendLine).len(), 1);
    }

    #[test]
    fn a_book_holds_a_limited_number_of_named_looks() {
        let (mut book, first, _) = two_lines();
        for n in 0..MAX_NAMED_TEMPLATES {
            assert!(book.save_named_template(SYMBOL, first, &format!("look {n}")));
        }
        assert!(!book.save_named_template(SYMBOL, first, "one too many"));
        assert!(
            book.save_named_template(SYMBOL, first, "look 0"),
            "a name that exists is replaced, not added"
        );
        let long = "x".repeat(200);
        assert!(book.delete_named_template(Tool::TrendLine, "look 1"));
        assert!(book.save_named_template(SYMBOL, first, &long));
        assert!(
            book.named_templates(Tool::TrendLine)
                .iter()
                .any(|t| t.name.chars().count() == MAX_TEMPLATE_NAME)
        );
    }

    #[test]
    fn shift_and_a_click_builds_a_selection_and_a_second_click_takes_one_out() {
        let mut book = book();
        apart_lines(&mut book);
        let ids: Vec<u64> = book.drawings(SYMBOL).iter().map(|d| d.id).collect();
        let (x, y) = at(150.0, 130.0);
        book.press_adding(SYMBOL, TF, &Linear, x, y, true, &|d| Some(d.clone()));
        book.release(SYMBOL, &Linear, x, y);
        let (x, y) = at(150.0, 330.0);
        book.press_adding(SYMBOL, TF, &Linear, x, y, true, &|d| Some(d.clone()));
        book.release(SYMBOL, &Linear, x, y);
        assert_eq!(book.selected_count(), 2);
        assert!(ids.iter().all(|id| book.is_selected(*id)));

        book.press_adding(SYMBOL, TF, &Linear, x, y, true, &|d| Some(d.clone()));
        book.release(SYMBOL, &Linear, x, y);
        assert_eq!(book.selected_count(), 1);
        assert!(!book.is_selected(ids[1]));
    }

    #[test]
    fn dragging_one_of_a_group_moves_all_of_it_as_one_undo_step() {
        let mut book = book();
        apart_lines(&mut book);
        assert!(book.select_all(SYMBOL, TF));
        let before: Vec<f64> = book
            .drawings(SYMBOL)
            .iter()
            .map(|d| d.points[0].p)
            .collect();
        let (x, y) = at(150.0, 130.0);
        book.press(SYMBOL, TF, &Linear, x, y);
        let (x2, y2) = at(150.0, 150.0);
        book.pointer_moved(SYMBOL, &Linear, x2, y2, false);
        book.release(SYMBOL, &Linear, x2, y2);
        let after: Vec<f64> = book
            .drawings(SYMBOL)
            .iter()
            .map(|d| d.points[0].p)
            .collect();
        let moved: Vec<f64> = before.iter().zip(&after).map(|(a, b)| b - a).collect();
        assert!(moved[0].abs() > 0.0);
        assert!((moved[0] - moved[1]).abs() < 1e-6, "both moved the same");
        assert_eq!(book.selected_count(), 2, "the group is kept");

        assert!(book.undo());
        let restored: Vec<f64> = book
            .drawings(SYMBOL)
            .iter()
            .map(|d| d.points[0].p)
            .collect();
        assert_eq!(restored, before);
    }

    #[test]
    fn a_box_selects_what_has_a_point_inside_it() {
        let mut book = book();
        apart_lines(&mut book);
        let low = book.select_in_box(SYMBOL, TF, &Linear, at(0.0, 80.0), at(300.0, 200.0), false);
        assert!(low);
        assert_eq!(book.selected_count(), 1);
        book.select_in_box(SYMBOL, TF, &Linear, at(0.0, 280.0), at(300.0, 400.0), true);
        assert_eq!(book.selected_count(), 2, "added to the selection");
    }

    #[test]
    fn delete_and_style_changes_reach_every_selected_drawing() {
        let mut book = book();
        apart_lines(&mut book);
        book.select_all(SYMBOL, TF);
        assert!(book.set_color(SYMBOL, 0x12_3456));
        assert!(
            book.drawings(SYMBOL)
                .iter()
                .all(|d| d.style.color == 0x12_3456)
        );
        assert!(book.delete_selected(SYMBOL));
        assert_eq!(book.count(SYMBOL), 0);
        assert!(book.undo());
        assert_eq!(book.count(SYMBOL), 2);
    }

    #[test]
    fn a_copy_pastes_on_another_symbol_and_each_paste_lands_further() {
        let mut book = book();
        apart_lines(&mut book);
        book.select_all(SYMBOL, TF);
        assert!(book.copy_selection(SYMBOL));
        assert!(book.paste("EURUSD", &Linear));
        assert_eq!(book.count("EURUSD"), 2);
        assert_eq!(book.selected_count(), 2, "the copies are selected");
        assert!(book.paste("EURUSD", &Linear));
        let list = book.drawings("EURUSD");
        assert_eq!(list.len(), 4);
        assert_ne!(list[0].points[0], list[2].points[0]);
        assert_eq!(book.count(SYMBOL), 2, "the originals stay");
    }

    #[test]
    fn the_arrows_move_the_selection_but_not_a_locked_drawing() {
        let mut book = book();
        apart_lines(&mut book);
        let id = book.drawings(SYMBOL)[0].id;
        book.select(Some(id));
        let start = book.drawings(SYMBOL)[0].points[0];
        assert!(book.nudge(SYMBOL, &Linear, 1.0, 1.0));
        let moved = book.drawings(SYMBOL)[0].points[0];
        assert!(moved.t > start.t && moved.p > start.p);
        assert!(book.toggle_lock(SYMBOL));
        assert!(
            !book.nudge(SYMBOL, &Linear, 1.0, 1.0),
            "a locked drawing stays"
        );
    }
}
