//! What the user does to the selected drawings: style, text, copy and paste, move, delete, order.

use super::{Book, Order, Snapshot};
use crate::domain::drawings::geometry::Projection;
use crate::domain::drawings::model::Drawing;
use crate::domain::drawings::model::tool::Tool;

impl Book {
    pub(super) fn edit_selected(
        &mut self,
        symbol: &str,
        mut change: impl FnMut(&mut Drawing),
    ) -> bool {
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

    pub fn set_dash(
        &mut self,
        symbol: &str,
        dash: crate::domain::drawings::model::style::Dash,
    ) -> bool {
        self.edit_selected(symbol, |d| d.style.dash = dash)
    }

    /// Changes the look of the selected drawing as one undo step, for the quick controls of the
    /// style bar that have no setter of their own.
    pub fn edit_style(
        &mut self,
        symbol: &str,
        mut change: impl FnMut(&mut crate::domain::drawings::model::style::Style),
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
    pub(super) fn add_copies(
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

    pub(super) fn edit_one(
        &mut self,
        symbol: &str,
        id: u64,
        change: impl FnOnce(&mut Drawing),
    ) -> bool {
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
}
