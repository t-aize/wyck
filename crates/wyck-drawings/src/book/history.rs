//! The undo history of the book, and how a drawing is added to it.

use super::create::drawing_has_text;
use super::{Book, Creating, Snapshot, UNDO_DEPTH};
use crate::model::tool::Tool;
use crate::model::{Drawing, Point};

impl Book {
    pub(super) fn snapshot(&self, symbol: &str) -> Snapshot {
        Snapshot {
            symbol: symbol.to_owned(),
            drawings: self.drawings(symbol).to_vec(),
        }
    }

    pub(super) fn remember(&mut self, before: Snapshot) {
        self.undo.push(before);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.revision += 1;
    }

    pub(super) fn restore(&mut self, snapshot: Snapshot) -> Snapshot {
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

    pub(super) fn new_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(1);
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub(super) fn blank(&mut self, tool: Tool, points: Vec<Point>) -> Drawing {
        let mut drawing = Drawing::new(self.new_id(), tool, points);
        if let Some(template) = self.templates.get(&tool.code()) {
            drawing.style = template.style.clone();
            drawing.levels.clone_from(&template.levels);
        }
        drawing
    }

    /// Puts a finished drawing in its symbol, selects it and goes back to the pointer.
    pub(super) fn commit(&mut self, symbol: &str, drawing: Drawing) {
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
    pub(super) fn finish_path(&mut self, mut creating: Creating) {
        if creating.drawing.tool != Tool::ArrowPath || creating.placed < 2 {
            return;
        }
        creating.drawing.points.truncate(creating.placed);
        self.commit(&creating.symbol, creating.drawing);
    }
}
