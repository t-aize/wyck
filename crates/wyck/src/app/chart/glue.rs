//! The drawings on a chart: presses, moves and releases on the plot go to the shared drawing book
//! with this chart's projection, so a drawing made here shows at once on every chart of the
//! symbol.

use gpui::{App, Context, Entity, Window};
use wyck_openapi::market::PRICE_SCALE;

use super::drawing::Drawings;
use super::drawing::book::{Book, Order, Press};
use super::drawing::model::Tool;
use super::projection::ChartProjection;
use super::{Chart, ChartAction, ChartEvent, drawing_props, object_tree};

/// Something done to one drawing from its menu or the bar over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawingCommand {
    Duplicate,
    Order(Order),
    Hidden(bool),
    Lock(bool),
    Delete,
    Flip,
}

impl Chart {
    /// Shows and edits the drawings of this entity, redrawing when they change.
    pub fn attach_drawings(&mut self, drawings: Entity<Drawings>, cx: &mut Context<Self>) {
        self._drawings_observe = Some(cx.observe(&drawings, |_this, _drawings, cx| cx.notify()));
        self.drawings = Some(drawings);
    }

    /// Runs `f` with the projection of the prices band as it is now, when it has data.
    pub(super) fn with_projection<R>(
        &self,
        f: impl FnOnce(&ChartProjection<'_>) -> R,
    ) -> Option<R> {
        let map = self.main_map()?;
        let geometry = self.geometry();
        let projection = ChartProjection {
            series: self.shown(),
            view: &self.view,
            map,
            plot_w: geometry.plot_w(),
            plot_h: geometry.main().h,
            step_ms: self.step_ms(),
            digits: self.digits(),
            pip_position: self.symbol.as_ref().and_then(|symbol| symbol.pip_position),
        };
        Some(f(&projection))
    }

    pub(super) fn symbol_name(&self) -> Option<String> {
        self.symbol.as_ref().map(|s| s.name.to_string())
    }

    /// A press on the prices. Returns whether a drawing took it, in which case the chart does not
    /// scroll.
    pub(super) fn drawing_press(&mut self, x: f32, y: f32, cx: &mut Context<Self>) -> bool {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return false;
        };
        let at_limit = {
            let drawings = drawings.read(cx);
            let book = drawings.book();
            book.tool().is_some() && book.count(&symbol) >= book.drawing_limit()
        };
        if at_limit {
            crate::app::toast::show(
                cx,
                crate::app::toast::Kind::Warning,
                "Drawing limit reached",
                "Change the drawings per symbol limit in Settings (Ctrl+,).",
            );
            return true;
        }
        let timeframe = self.timeframe.code();
        let taken = self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.press(&symbol, &timeframe, projection, x, y))
            })
        });
        taken == Some(Press::Taken)
    }

    /// The pointer moved: a drawing being made or moved follows it.
    pub(super) fn drawing_moved(
        &mut self,
        x: f32,
        y: f32,
        constrain: bool,
        cx: &mut Context<Self>,
    ) {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        let timeframe = self.timeframe.code();
        if !drawings.read(cx).book().is_busy() {
            let over = self
                .with_projection(|projection| {
                    drawings
                        .read(cx)
                        .book()
                        .hover(&symbol, &timeframe, projection, x, y)
                })
                .flatten();
            if over != self.over_drawing {
                self.over_drawing = over;
                cx.notify();
            }
            return;
        }
        self.over_drawing = None;
        let changed = self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| {
                    book.pointer_moved(&symbol, projection, x, y, constrain)
                })
            })
        });
        if changed == Some(true) {
            cx.notify();
        }
    }

    /// Duplicates the selected drawing, a little to the side.
    pub fn duplicate_selected(&mut self, cx: &mut Context<Self>) {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.duplicate(&symbol, projection))
            })
        });
    }

    pub(super) fn drawing_released(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        self.drawing_drag = false;
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.release(&symbol, projection, x, y))
            })
        });
        cx.notify();
    }

    /// The drawing under `(x, y)` on the prices, if any.
    pub fn drawing_under(&self, x: f32, y: f32, cx: &App) -> Option<u64> {
        let (drawings, symbol) = (self.drawings.as_ref()?, self.symbol_name()?);
        let timeframe = self.timeframe.code();
        self.with_projection(|projection| {
            drawings
                .read(cx)
                .book()
                .drawing_at(&symbol, &timeframe, projection, x, y)
        })
        .flatten()
    }

    fn edit_drawings(&self, cx: &mut Context<Self>, change: impl FnOnce(&mut Book, &str) -> bool) {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| change(book, &symbol))
        });
    }

    /// Selects a drawing, as a click on it would.
    pub fn select_drawing(&self, id: u64, cx: &mut Context<Self>) {
        self.edit_drawings(cx, |book, _| {
            book.select(Some(id));
            true
        });
    }

    /// A right click: drops the drawing being made here, if there is one. Returns whether it did,
    /// in which case the click has done its work and opens no menu.
    pub(super) fn abort_drawing(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return false;
        };
        let aborted = drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.abort(&symbol))
        });
        if aborted {
            self.drawing_drag = false;
            cx.notify();
        }
        aborted
    }

    pub fn drawing_command(&mut self, id: u64, command: DrawingCommand, cx: &mut Context<Self>) {
        match command {
            DrawingCommand::Duplicate => {
                self.select_drawing(id, cx);
                self.duplicate_selected(cx);
            }
            DrawingCommand::Order(order) => {
                self.edit_drawings(cx, |book, symbol| book.reorder(symbol, id, order));
            }
            DrawingCommand::Hidden(hidden) => {
                self.edit_drawings(cx, |book, symbol| book.set_hidden(symbol, id, hidden));
            }
            DrawingCommand::Lock(locked) => {
                self.edit_drawings(cx, |book, symbol| book.set_locked(symbol, id, locked));
            }
            DrawingCommand::Delete => {
                self.edit_drawings(cx, |book, symbol| book.delete(symbol, id));
            }
            DrawingCommand::Flip => {
                self.edit_drawings(cx, |book, symbol| book.flip_position(symbol, id));
            }
        }
        cx.notify();
    }

    /// What the drawing dialogs need: the drawings, the symbol, the zone and the decimals.
    fn drawing_context(&self) -> Option<(Entity<Drawings>, String, super::zone::Zone, u32)> {
        Some((
            self.drawings.clone()?,
            self.symbol_name()?,
            self.settings.zone,
            self.digits(),
        ))
    }

    /// How many drawings the symbol has, or `None` when the chart has no drawings or symbol.
    pub(super) fn drawing_count(&self, cx: &App) -> Option<usize> {
        let drawings = self.drawings.as_ref()?;
        let symbol = self.symbol_name()?;
        Some(drawings.read(cx).book().drawings(&symbol).len())
    }

    /// A settings dialog asked for by a double click opens now that the window is at hand.
    pub(super) fn open_pending_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.settings_for.take() {
            let entity = cx.entity();
            // Opened after this render, so the dialog is not made while the chart is borrowed.
            window.defer(cx, move |window, cx| {
                open_drawing_settings(&entity, id, window, cx);
            });
        }
    }

    /// The order a long or short position drawing stands for, for the ticket.
    pub fn position_order(&self, id: u64, cx: &App) -> Option<ChartAction> {
        let (drawings, symbol) = (self.drawings.as_ref()?, self.symbol_name()?);
        let drawing = drawings.read(cx).book().get(&symbol, id)?.clone();
        if !drawing.tool.is_position() || drawing.points.len() < 3 {
            return None;
        }
        let real = |raw: f64| raw / PRICE_SCALE as f64;
        Some(ChartAction::Ticket {
            buy: drawing.tool == Tool::LongPosition,
            entry: Some(real(drawing.points[0].p)),
            stop_loss: Some(real(drawing.points[1].p)),
            take_profit: Some(real(drawing.points[2].p)),
        })
    }

    /// Opens the order ticket filled from a position drawing.
    pub fn trade_drawing(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(action) = self.position_order(id, cx) {
            cx.emit(ChartEvent::Action(action));
        }
    }
}

/// Opens the settings of drawing `id` of the symbol of `chart`.
pub fn open_drawing_settings(chart: &Entity<Chart>, id: u64, window: &mut Window, cx: &mut App) {
    if let Some((drawings, symbol, zone, digits)) = chart.read(cx).drawing_context() {
        drawing_props::open(drawings, symbol, id, zone, digits, window, cx);
    }
}

/// Opens the list of the drawings of the symbol of `chart`.
pub fn open_object_tree(chart: &Entity<Chart>, window: &mut Window, cx: &mut App) {
    if let Some((drawings, symbol, zone, digits)) = chart.read(cx).drawing_context() {
        object_tree::open(drawings, symbol, zone, digits, window, cx);
    }
}
