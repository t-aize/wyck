//! What the dashboard draws: its body and the header over the charts.

use super::{
    CloseAllPositions, ClosePicker, Conn, Dashboard, DashboardEvent, MenuKind, OpenPicker,
    OpenSettings, Quote, TicketBuy, TicketSell, TicketSend, ToggleIndicatorEditor,
    ToggleKillSwitch,
};

use crate::domain::market::{PRICE_SCALE, format_price};
use crate::ui::features::chart;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::{button, icon, theme, tokens};
use gpui::prelude::*;
use gpui::{Context, Window, deferred, div, px};

impl Dashboard {
    pub(super) fn body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let centered = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .px_6();
        match &self.conn {
            Conn::Failed(message) => centered
                .child(icon::tinted(
                    IconName::CircleAlert,
                    28.,
                    theme::destructive(),
                ))
                .child(
                    div()
                        .text_size(px(tokens::text::heading()))
                        .text_color(theme::fg())
                        .child("The connection to cTrader ended"),
                )
                .child(
                    div()
                        .max_w(px(crate::ui::kit::tokens::measure::note()))
                        .text_center()
                        .text_size(px(tokens::text::emphasis()))
                        .text_color(theme::muted_fg())
                        .child(message.clone()),
                )
                .child(
                    div()
                        .pt_2()
                        .w(px(crate::ui::kit::tokens::measure::action()))
                        .child(button::primary(
                            "sign-in-again",
                            "Sign in again",
                            cx.listener(|_this, _event, _window, cx| {
                                cx.emit(DashboardEvent::SignInAgain);
                            }),
                        )),
                )
                .into_any_element(),
            _ => self
                .trading_layout(self.multi.clone().into_any_element(), cx)
                .into_any_element(),
        }
    }
}

impl Render for Dashboard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus_handle, cx);
        }
        if std::mem::take(&mut self.pending_picker) {
            self.open_picker(window, cx);
        }
        self.trading_frame(window, cx);
        self.editor_frame(window, cx);
        let picker = self.render_picker(window, cx);
        let header = self.render_header(window, cx).into_any_element();
        let body = self.body(cx).into_any_element();
        let menu = self.render_menu(cx);
        let menu_backdrop = (self.menu_is(MenuKind::Timeframes) || self.menu_is(MenuKind::Layouts))
            .then(|| {
                let this = cx.entity().downgrade();
                deferred(crate::ui::kit::menu::backdrop(move |_, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.close_menus();
                        cx.notify();
                    });
                }))
                .with_priority(0)
            });

        // A second context name while a drawing is being made turns on the keys for that.
        let context = if self.multi.read(cx).drawing_in_progress(cx) {
            "Dashboard DrawingInProgress"
        } else {
            "Dashboard"
        };
        Self::drawing_actions(
            div().key_context(context).track_focus(&self.focus_handle),
            cx,
        )
        .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
            crate::ui::shell::settings_hub::open(
                this.workspace.clone(),
                this.multi.clone(),
                window,
                cx,
            );
        }))
        .on_action(cx.listener(|this, _: &ToggleIndicatorEditor, _window, cx| {
            this.toggle_editor(cx);
        }))
        .on_action(cx.listener(|this, _: &CloseAllPositions, window, cx| {
            this.ask_close_all(window, cx);
        }))
        .on_action(cx.listener(|this, _: &ToggleKillSwitch, _window, cx| {
            this.toggle_kill_switch(cx);
        }))
        .on_action(cx.listener(|this, _: &TicketBuy, window, cx| {
            this.ticket_key(Some(true), window, cx);
        }))
        .on_action(cx.listener(|this, _: &TicketSell, window, cx| {
            this.ticket_key(Some(false), window, cx);
        }))
        .on_action(cx.listener(|this, _: &TicketSend, window, cx| {
            this.ticket_key(None, window, cx);
        }))
        .on_action(cx.listener(|this, _: &OpenPicker, window, cx| {
            this.open_picker(window, cx);
        }))
        .on_action(cx.listener(|this, _: &ClosePicker, window, cx| {
            // Escape gives up a drawing in progress before it closes anything else.
            let nothing_open = this.picker.is_none() && this.open_menu.is_none();
            // A menu of a chart (the time zone) closes before anything else.
            if nothing_open && this.multi.update(cx, |multi, cx| multi.close_menus(cx)) {
                return;
            }
            if nothing_open && this.multi.update(cx, |multi, cx| multi.cancel_drawing(cx)) {
                // The field that had the keyboard may be gone with the selection.
                window.focus(&this.focus_handle, cx);
                return;
            }
            this.close_overlays(window, cx);
        }))
        .on_action(cx.listener(|this, _: &chart::DeleteDrawing, window, cx| {
            this.multi.update(cx, |multi, cx| multi.delete_drawing(cx));
            window.focus(&this.focus_handle, cx);
        }))
        .on_action(cx.listener(|this, _: &chart::FinishDrawing, _window, cx| {
            // Only an arrow path ends on Enter: for anything else the key goes on.
            if !this.multi.update(cx, |multi, cx| multi.finish_drawing(cx)) {
                cx.propagate();
            }
        }))
        // Alt and a number picks the favorite tool at that place in the bar.
        .on_action(cx.listener(|this, _: &chart::Favorite1, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(0, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite2, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(1, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite3, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(2, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite4, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(3, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite5, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(4, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite6, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(5, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite7, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(6, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite8, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(7, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::Favorite9, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_favorite(8, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::UndoDrawing, _window, cx| {
            this.multi.update(cx, |multi, cx| multi.undo_drawing(cx));
        }))
        .on_action(cx.listener(|this, _: &chart::RedoDrawing, _window, cx| {
            this.multi.update(cx, |multi, cx| multi.redo_drawing(cx));
        }))
        .on_action(
            cx.listener(|this, _: &chart::DuplicateDrawing, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.duplicate_drawing(cx));
            }),
        )
        .on_action(cx.listener(|this, _: &chart::ChartPanBack, window, cx| {
            let moved = this.multi.read(cx).has_chart_focus(window)
                && this
                    .multi
                    .update(cx, |multi, cx| multi.nudge_drawings(-1.0, 0.0, cx));
            if !moved {
                this.on_active_chart(cx, |chart, cx| chart.pan_keys(false, cx));
            }
        }))
        .on_action(cx.listener(|this, _: &chart::ChartPanForward, window, cx| {
            let moved = this.multi.read(cx).has_chart_focus(window)
                && this
                    .multi
                    .update(cx, |multi, cx| multi.nudge_drawings(1.0, 0.0, cx));
            if !moved {
                this.on_active_chart(cx, |chart, cx| chart.pan_keys(true, cx));
            }
        }))
        .on_action(cx.listener(|this, _: &chart::ChartZoomIn, _window, cx| {
            this.on_active_chart(cx, |chart, cx| chart.zoom_keys(true, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ChartZoomOut, _window, cx| {
            this.on_active_chart(cx, |chart, cx| chart.zoom_keys(false, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ChartLatest, _window, cx| {
            this.on_active_chart(cx, |chart, cx| chart.jump_to_latest(cx));
        }))
        .on_action(
            cx.listener(|this, _: &chart::ChartResetScale, _window, cx| {
                this.on_active_chart(cx, |chart, cx| chart.reset_price_scale(cx));
            }),
        )
        .on_action(cx.listener(|this, _: &chart::ChartAddAlert, _window, cx| {
            this.add_alert_here(cx);
        }))
        .on_action(
            cx.listener(|this, _: &chart::ChartScreenshot, _window, cx| {
                this.multi.update(cx, |multi, cx| multi.picture(cx));
            }),
        )
        .on_action(
            cx.listener(|this, _: &chart::ChartScreenshotAll, _window, cx| {
                this.multi.update(cx, |multi, cx| multi.picture_all(cx));
            }),
        )
        .on_action(
            cx.listener(|this, _: &chart::ChartCopyIndicators, window, cx| {
                this.multi.update(cx, |multi, cx| {
                    if multi.has_chart_focus(window) {
                        multi.copy_active_indicators(cx);
                    }
                });
            }),
        )
        .on_action(
            cx.listener(|this, _: &chart::ChartCopySettings, window, cx| {
                this.multi.update(cx, |multi, cx| {
                    if multi.has_chart_focus(window) {
                        multi.copy_active_settings(cx);
                    }
                });
            }),
        )
        .on_action(cx.listener(|this, _: &chart::ChartPaste, window, cx| {
            this.multi.update(cx, |multi, cx| {
                if multi.has_chart_focus(window) {
                    multi.paste_active(cx);
                }
            });
        }))
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .w_full()
        .h_full()
        .bg(theme::bg())
        .child(header)
        .child(body)
        .children(menu)
        .children(picker)
        .children(menu_backdrop)
    }
}

/// The bid, the ask and the spread in pips of a quote, formatted for a symbol quoted with `digits`
/// decimals and a pip at `pip_position`.
pub(super) fn quote_text(
    quote: Quote,
    digits: u32,
    pip_position: i64,
) -> Option<super::details::Live> {
    let bid = quote.bid?;
    let spread = quote.ask.map(|ask| {
        let pips = (ask - bid) as f64
            / PRICE_SCALE as f64
            / crate::domain::market::price::pip_size(pip_position);
        format!("{pips:.1}")
    });
    Some((
        format_price(bid, digits),
        quote.ask.map(|ask| format_price(ask, digits)),
        spread,
    ))
}
