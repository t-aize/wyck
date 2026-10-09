//! The drawings on a chart: presses, moves and releases on the plot go to the shared drawing book
//! with this chart's projection, so a drawing made here shows at once on every chart of the
//! symbol.

use gpui::{App, Context, Entity, Window};
use wyck_openapi::market::PRICE_SCALE;

use super::drawing::Drawings;
use super::projection::ChartProjection;
use super::study::StudyKind;
use super::{
    Chart, ChartAction, ChartEvent, PlanState, PositionLink, PositionPlan, drawing_props,
    object_tree,
};
use wyck_chart::drawing::book::{Book, Order, Press};
use wyck_chart::drawing::model::Tool;
use wyck_chart::study::atr_stop::{AtrStop, Smoothing};

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
        self._drawings_observe = Some(cx.observe(&drawings, |this, _drawings, cx| {
            this.request_drawing_atr(cx);
            cx.notify();
        }));
        self.drawings = Some(drawings);
        self.request_drawing_atr(cx);
    }

    pub(super) fn request_drawing_atr(&mut self, cx: &mut Context<Self>) {
        let (Some(drawings), Some(symbol)) = (&self.drawings, self.symbol_name()) else {
            return;
        };
        let configs: Vec<_> = drawings
            .read(cx)
            .book()
            .drawings(&symbol)
            .iter()
            .filter_map(|d| {
                d.tool
                    .is_position()
                    .then(|| d.style.position.atr_stop.clone())
                    .flatten()
            })
            .collect();
        for config in configs {
            self.request_atr(&config, cx);
        }
    }

    pub(super) fn resolved_position(
        &self,
        drawing: &wyck_chart::drawing::model::Drawing,
    ) -> Option<wyck_chart::drawing::model::Drawing> {
        if !drawing.tool.is_position() || drawing.points.len() < 3 {
            return Some(drawing.clone());
        }
        let mut result = drawing.clone();
        let settings = &drawing.style.position;
        let entry = drawing.points[0].p / PRICE_SCALE as f64;
        let buy = drawing.tool == Tool::LongPosition;
        if let Some(config) = &settings.atr_stop {
            let atr = self.atr_value(config)?;
            let stop = config.stop(entry, buy, atr)?;
            result.points[1].p = stop * PRICE_SCALE as f64;
        }
        if let Some(rr) = settings.target_rr {
            let distance = (entry - result.points[1].p / PRICE_SCALE as f64).abs();
            if !distance.is_finite() || distance <= 0.0 {
                return None;
            }
            let target = entry + if buy { rr * distance } else { -rr * distance };
            result.points[2].p = target * PRICE_SCALE as f64;
        }
        Some(result)
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

    pub(super) fn atr_seed(&self) -> Option<AtrStop> {
        let study = self
            .settings()
            .studies
            .iter()
            .find(|study| study.kind == StudyKind::Atr)?;
        Some(AtrStop {
            length: (study.input("length") as usize).clamp(1, 1_000),
            smoothing: match study.input("smoothing") as usize {
                1 => Smoothing::Sma,
                2 => Smoothing::Ema,
                3 => Smoothing::Wma,
                _ => Smoothing::Rma,
            },
            ..AtrStop::default()
        })
    }

    /// A press on the prices. Returns whether a drawing took it, in which case the chart does not
    /// scroll.
    pub(super) fn drawing_press(
        &mut self,
        x: f32,
        y: f32,
        add: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return false;
        };
        let at_limit = {
            let drawings = drawings.read(cx);
            let book = drawings.book();
            book.tool().is_some() && book.count(&symbol) >= book.drawing_limit()
        };
        if at_limit {
            wyck_ui::toast::show(
                cx,
                wyck_ui::toast::Kind::Warning,
                "Drawing limit reached",
                "Change the drawings per symbol limit in Settings (Ctrl+,).",
            );
            return true;
        }
        let timeframe = self.timeframe.code();
        let taken = self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| {
                    book.press_adding(&symbol, &timeframe, projection, x, y, add, &|d| {
                        self.resolved_position(d)
                    })
                })
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
                    drawings.read(cx).book().hover_resolved(
                        &symbol,
                        &timeframe,
                        projection,
                        x,
                        y,
                        &|d| self.resolved_position(d),
                    )
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

    /// Selects the drawings a box dragged over the plot touches, added to the selection.
    pub(super) fn select_in_box(
        &mut self,
        from: (f32, f32),
        to: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return;
        };
        let timeframe = self.timeframe.code();
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| {
                    book.select_in_box(&symbol, &timeframe, projection, from, to, true)
                })
            })
        });
    }

    /// Ctrl+A: selects every drawing of the symbol that shows on this timeframe.
    pub fn select_all_drawings(&mut self, cx: &mut Context<Self>) {
        let timeframe = self.timeframe.code();
        self.edit_drawings(cx, |book, symbol| book.select_all(symbol, &timeframe));
    }

    /// Keeps a copy of the selected drawings. Returns whether there was anything selected.
    pub fn copy_selected_drawings(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drawings) = self.drawings.clone() else {
            return false;
        };
        let Some(symbol) = self.symbol_name() else {
            return false;
        };
        drawings.update(cx, |drawings, cx| {
            drawings.edit(cx, |book| book.copy_selection(&symbol))
        })
    }

    /// Pastes the copied drawings on this chart's symbol. Returns whether anything was copied.
    pub fn paste_drawings(&mut self, cx: &mut Context<Self>) -> bool {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return false;
        };
        if !drawings.read(cx).book().can_paste() {
            return false;
        }
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.paste(&symbol, projection))
            })
        });
        true
    }

    /// The arrow keys with something selected: moves it by `bars` bars and `steps` hundredths of the
    /// price span. Returns whether there was a selection, in which case the arrow does not pan.
    pub fn nudge_selected(&mut self, bars: f64, steps: f64, cx: &mut Context<Self>) -> bool {
        let (Some(drawings), Some(symbol)) = (self.drawings.clone(), self.symbol_name()) else {
            return false;
        };
        {
            let drawings = drawings.read(cx);
            let book = drawings.book();
            if book.selected_count() == 0 || book.tool().is_some() {
                return false;
            }
        }
        self.with_projection(|projection| {
            drawings.update(cx, |drawings, cx| {
                drawings.edit(cx, |book| book.nudge(&symbol, projection, bars, steps))
            })
        });
        true
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
            drawings.read(cx).book().drawing_at_resolved(
                &symbol,
                &timeframe,
                projection,
                x,
                y,
                &|d| self.resolved_position(d),
            )
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

    /// Removes every drawing of the symbol that is not locked, as one undo step.
    pub fn clear_drawings(&mut self, cx: &mut Context<Self>) {
        self.edit_drawings(cx, |book, symbol| book.clear(symbol));
    }

    /// Selects a drawing, as a click on it would.
    pub fn select_drawing(&self, id: u64, cx: &mut Context<Self>) {
        self.edit_drawings(cx, |book, _| {
            book.select(Some(id));
            true
        });
    }

    /// A right click while a drawing tool is active: drops what is half made and gives up the tool,
    /// back to the normal cursor. Returns whether there was anything to give up, in which case the
    /// click has done its work and opens no menu.
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

    /// The position drawing `id` of `symbol` as it stands now.
    pub fn position_plan(&self, id: u64, symbol: &str, cx: &App) -> PlanState {
        let (Some(drawings), Some(own)) = (self.drawings.as_ref(), self.symbol_name()) else {
            return PlanState::Waiting;
        };
        // Drawings belong to a symbol: another chart says nothing about this one.
        if own != symbol {
            return PlanState::Waiting;
        }
        let Some(raw) = drawings.read(cx).book().get(&own, id) else {
            return PlanState::Gone;
        };
        if !raw.tool.is_position() || raw.points.len() < 3 {
            return PlanState::Gone;
        }
        let Some(drawing) = self.resolved_position(raw) else {
            return PlanState::Waiting;
        };
        let real = |raw: f64| raw / PRICE_SCALE as f64;
        PlanState::Ready(PositionPlan {
            drawing: id,
            buy: drawing.tool == Tool::LongPosition,
            entry: real(drawing.points[0].p),
            stop_loss: real(drawing.points[1].p),
            take_profit: real(drawing.points[2].p),
            atr: raw.style.position.atr_stop.clone(),
            rr: raw.style.position.target_rr,
        })
    }

    /// The order a long or short position drawing stands for, for the ticket.
    pub fn position_order(&self, id: u64, cx: &App) -> Option<ChartAction> {
        let symbol = self.symbol_name()?;
        let PlanState::Ready(plan) = self.position_plan(id, &symbol, cx) else {
            return None;
        };
        Some(ChartAction::Ticket {
            buy: plan.buy,
            entry: Some(plan.entry),
            stop_loss: Some(plan.stop_loss),
            take_profit: Some(plan.take_profit),
            link: Some(PositionLink {
                drawing: id,
                atr: plan.atr,
                rr: plan.rr,
            }),
        })
    }

    /// Opens the order ticket filled from a position drawing.
    pub fn trade_drawing(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(action) = self.position_order(id, cx) {
            cx.emit(ChartEvent::Action(action));
        } else if let (Some(drawings), Some(symbol)) = (&self.drawings, self.symbol_name())
            && drawings
                .read(cx)
                .book()
                .get(&symbol, id)
                .is_some_and(|drawing| drawing.style.position.atr_stop.is_some())
        {
            wyck_ui::toast::Toast::warning(
                "ATR unavailable",
                "The selected chart has no current ATR value for this drawing.",
            )
            .hint("Wait for the bars to load, or give the drawing an ATR timeframe with data.")
            .sticky(false)
            .show(cx);
        }
    }
}

/// Opens the settings of drawing `id` of the symbol of `chart`.
pub fn open_drawing_settings(chart: &Entity<Chart>, id: u64, window: &mut Window, cx: &mut App) {
    if let Some((drawings, symbol, zone, digits)) = chart.read(cx).drawing_context() {
        let atr_seed = chart.read(cx).atr_seed();
        drawing_props::open(
            drawings,
            symbol,
            id,
            drawing_props::PropsContext {
                zone,
                digits,
                atr_seed,
                chart: Some(chart.clone()),
            },
            window,
            cx,
        );
    }
}

/// Opens the list of the drawings of the symbol of `chart`.
pub fn open_object_tree(chart: &Entity<Chart>, window: &mut Window, cx: &mut App) {
    if let Some((drawings, symbol, zone, digits)) = chart.read(cx).drawing_context() {
        object_tree::open(
            drawings,
            symbol,
            zone,
            digits,
            Some(chart.clone()),
            window,
            cx,
        );
    }
}
