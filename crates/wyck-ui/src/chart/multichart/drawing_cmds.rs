//! What the drawing tools do for the active chart: pick a tool, edit the selection, undo.

use super::{MAX_RECENT_COLORS, MAX_RECENT_TOOLS, MultiChart};
use gpui::{Context, Window};
use wyck_domain::drawings::model::style::{Dash, MIN_LINE_OPACITY};
use wyck_domain::drawings::model::tool::{Group, Tool};

impl MultiChart {
    pub(crate) fn pick_tool(&mut self, tool: Option<Tool>, cx: &mut Context<Self>) {
        self.flyout = None;
        self.tool_search = false;
        if let Some(tool) = tool {
            self.last_tool.insert(tool.group(), tool);
            self.recent_tools.retain(|other| *other != tool);
            self.recent_tools.insert(0, tool);
            self.recent_tools.truncate(MAX_RECENT_TOOLS);
        }
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_tool(tool));
        });
        cx.notify();
    }

    /// Opens the list of tools of a family beside the rail, or closes it when it is the one open.
    pub(crate) fn toggle_flyout(&mut self, group: Group, cx: &mut Context<Self>) {
        self.tool_search = false;
        self.flyout = if self.flyout == Some(group) {
            None
        } else {
            Some(group)
        };
        cx.notify();
    }

    /// Opens the search over every tool, with the keyboard in its field.
    pub fn open_tool_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.flyout = None;
        self.tool_search = true;
        self.search_input
            .update(cx, |input, cx| input.set_text("", cx));
        window.focus(&gpui::Focusable::focus_handle(&self.search_input, cx), cx);
        cx.notify();
    }

    pub fn toggle_magnet(&mut self, cx: &mut Context<Self>) {
        let magnet = !self.drawings.read(cx).book().magnet();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_magnet(magnet));
        });
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.magnet = magnet);
        });
    }

    pub fn toggle_keep_drawing(&mut self, cx: &mut Context<Self>) {
        let keep = !self.drawings.read(cx).book().keep_tool();
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.set_keep_tool(keep));
        });
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.keep_drawing = keep);
        });
    }

    /// Writes what waits to be saved now: the drawings and the workspace, for a backup.
    pub fn flush_documents(&self, cx: &gpui::App) {
        self.drawings.read(cx).flush();
        self.workspace.read(cx).flush();
    }

    /// Whether a drawing is being made on any chart, for the keys that only mean something then.
    pub fn drawing_in_progress(&self, cx: &gpui::App) -> bool {
        self.drawings.read(cx).book().is_creating()
    }

    /// Enter: ends an arrow path with the points it has. Returns whether it did.
    pub fn finish_drawing(&mut self, cx: &mut Context<Self>) -> bool {
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, wyck_domain::drawings::book::Book::finish)
        })
    }

    /// Escape: closes the menu a chart has open (the time zone one). Returns whether one was.
    pub fn close_menus(&mut self, cx: &mut Context<Self>) -> bool {
        let charts: Vec<_> = self.slots.iter().map(|slot| slot.chart.clone()).collect();
        let mut closed = false;
        for chart in charts {
            closed |= chart.update(cx, super::super::chart::Chart::close_menu);
        }
        closed
    }

    /// Escape: gives up what the drawing tools have in progress. Returns whether there was any.
    pub fn cancel_drawing(&mut self, cx: &mut Context<Self>) -> bool {
        if self.flyout.take().is_some() || std::mem::take(&mut self.tool_search) {
            cx.notify();
            return true;
        }
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, wyck_domain::drawings::book::Book::cancel)
        })
    }

    /// Opens the order ticket filled from a position drawing of the active chart.
    pub(crate) fn trade_drawing(&mut self, id: u64, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.trade_drawing(id, cx));
    }

    pub(crate) fn drawing_command(
        &mut self,
        id: u64,
        command: crate::chart::chart::DrawingCommand,
        cx: &mut Context<Self>,
    ) {
        let chart = self.active_chart().clone();
        chart.update(cx, |chart, cx| chart.drawing_command(id, command, cx));
    }

    pub(crate) fn edit_book(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut wyck_domain::drawings::book::Book, &str) -> bool,
    ) {
        let Some(symbol) = self.symbol_name(cx) else {
            return;
        };
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| change(book, &symbol))
        });
    }

    /// Delete and Backspace: take back the last point of a drawing being made, or else delete the
    /// selected drawing.
    pub fn delete_drawing(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| {
            book.remove_last_point() || book.delete_selected(symbol)
        });
    }

    pub fn undo_drawing(&mut self, cx: &mut Context<Self>) {
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, wyck_domain::drawings::book::Book::undo)
        });
    }

    pub fn redo_drawing(&mut self, cx: &mut Context<Self>) {
        self.drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, wyck_domain::drawings::book::Book::redo)
        });
    }

    pub fn duplicate_drawing(&mut self, cx: &mut Context<Self>) {
        let chart = self.active_chart().clone();
        chart.update(cx, super::super::chart::Chart::duplicate_selected);
    }

    pub(crate) fn clear_drawings(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, wyck_domain::drawings::book::Book::clear);
    }

    pub(crate) fn set_drawing_color(&mut self, color: u32, cx: &mut Context<Self>) {
        self.recent_colors.retain(|other| *other != color);
        self.recent_colors.insert(0, color);
        self.recent_colors.truncate(MAX_RECENT_COLORS);
        self.edit_book(cx, |book, symbol| book.set_color(symbol, color));
    }

    /// The opacity bar of the color panel: the lines of the selected drawings, from 0 to 1.
    pub(crate) fn set_drawing_opacity(&mut self, opacity: f32, cx: &mut Context<Self>) {
        let opacity = opacity.clamp(MIN_LINE_OPACITY, 1.0);
        self.edit_book(cx, |book, symbol| {
            book.edit_style(symbol, |s| s.opacity = opacity)
        });
    }

    pub(crate) fn set_drawing_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.set_width(symbol, width));
    }

    pub(crate) fn set_drawing_dash(&mut self, dash: Dash, cx: &mut Context<Self>) {
        self.edit_book(cx, |book, symbol| book.set_dash(symbol, dash));
    }

    pub(crate) fn toggle_drawing_fill(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, wyck_domain::drawings::book::Book::toggle_fill);
    }

    pub(crate) fn toggle_drawing_lock(&mut self, cx: &mut Context<Self>) {
        self.edit_book(cx, wyck_domain::drawings::book::Book::toggle_lock);
    }

    /// The words field changed: the selected text drawing says what it says.
    pub(super) fn on_text_edited(&mut self, cx: &mut Context<Self>) {
        let text = self.text_input.read(cx).text().to_owned();
        self.edit_book(cx, |book, symbol| book.set_text(symbol, &text));
    }

    /// The drawings changed: keep the words field in step with the selected text drawing.
    pub(super) fn on_drawings_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(symbol) = self.symbol_name(cx) {
            let book = self.drawings.read(cx).book();
            let words = book
                .selected()
                .and_then(|id| book.get(&symbol, id))
                .filter(|drawing| drawing.tool.has_text())
                .map(|drawing| drawing.text.clone());
            if let Some(words) = words
                && self.text_input.read(cx).text() != words
            {
                self.text_input
                    .update(cx, |input, cx| input.set_text(words, cx));
            }
        }
        cx.notify();
    }
}
