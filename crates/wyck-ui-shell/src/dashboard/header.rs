//! The bar across the top of the dashboard, and the account menu that hangs from it.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{Context, FontFeatures, FontWeight, Window, div, px};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::prelude::Selectable;

use super::marks;
pub(crate) mod figures;
pub(crate) mod timeframes;
use super::{Conn, Dashboard, MenuKind, Tick};
use wyck_core::math::count_f32;
use wyck_ui_kit::{anim, icon, layout, theme, tokens};

/// The widest the subtitle of the symbol grows before it is cut.
const SUBTITLE_MAX: f32 = 170.0;
/// The width kept for the arrow that shows the last price move.
const TICK_WIDTH: f32 = 14.0;
const CUSTOM_TIMEFRAME_WIDTH: f32 = 78.0;

/// Which parts of the bar the window is wide enough for.
#[derive(Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one flag per part of the bar that can be left out"
)]
struct Fit {
    chart_labels: bool,
    ask: bool,
    equity: bool,
    subtitle: bool,
}

impl Dashboard {
    /// The bar: the symbol (flush with the left edge, over the drawing tools), its price, the
    /// timeframes and the layout of the charts on the left; the account panels, the account and
    /// the window on the right.
    pub(super) fn render_header(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // What the bar leaves out as the window narrows, least needed first.
        let width = f32::from(window.viewport_size().width);
        let fit = Fit {
            chart_labels: width >= 1_180.0,
            ask: width >= 1_100.0,
            equity: width >= 1_240.0,
            subtitle: width >= 1_360.0,
        };
        let divider = || layout::rule_v().h(px(tokens::height::compact()));
        div()
            .flex_none()
            .w_full()
            .h(px(tokens::bar::header()))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .border_b_1()
            .border_color(theme::border_hairline())
            .child(self.symbol_button(fit, cx))
            .child(self.price_block(fit))
            .child(divider())
            .child(self.timeframe_strip(window, cx))
            .child(self.layout_button(window, cx))
            .child(self.replay_button(fit.chart_labels, window, cx))
            .child(self.chart_buttons(fit.chart_labels, window, cx))
            .child(div().flex_1().min_w_0())
            .child(self.indicators_button(window, cx))
            .child(self.panel_toggles(cx))
            .child(self.account_pill(fit, cx))
            .child(Self::fullscreen_button(window))
    }

    /// The symbol: its tile, ticker and name. It is a button that opens the picker.
    fn symbol_button(&self, fit: Fit, cx: &mut Context<Self>) -> impl IntoElement {
        let (tile, title, subtitle) = match &self.active {
            Some(active) => (
                marks::render(&active.entry.icon, 30., theme::bg()),
                active.entry.name.clone(),
                active.entry.description.clone(),
            ),
            None => (
                layout::icon_tile(
                    IconName::Search,
                    30.,
                    16.,
                    theme::surface(),
                    theme::muted_fg(),
                )
                .into_any_element(),
                match &self.catalog {
                    super::Load::Loading => "Loading symbols...".to_owned(),
                    super::Load::Failed(_) => "Symbols unavailable".to_owned(),
                    super::Load::Ready(_) => "Select a symbol".to_owned(),
                },
                match &self.catalog {
                    super::Load::Failed(message) => format!("{message}. Click to try again."),
                    _ => String::new(),
                },
            ),
        };

        // The mark sits over the rail of drawing tools, centred on it, with no gap to the edge.
        let subtitle = if fit.subtitle {
            subtitle
        } else {
            String::new()
        };
        div()
            .id("symbol-button")
            .flex_none()
            .h_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .pl(px(tokens::space::sm()))
            .pr_3()
            .border_r_1()
            .border_color(theme::border_hairline())
            .cursor_pointer()
            .hover(|style| style.bg(theme::surface_hover()))
            .active(|style| style.bg(theme::surface_pressed()))
            .on_click(cx.listener(|this, _event, window, cx| this.open_picker(window, cx)))
            .child(tile)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(tokens::text::title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg())
                            .child(title),
                    )
                    .when(!subtitle.is_empty(), |el| {
                        el.child(
                            div()
                                .max_w(px(SUBTITLE_MAX))
                                .truncate()
                                .text_size(px(tokens::text::small()))
                                .text_color(theme::muted_fg())
                                .child(subtitle),
                        )
                    }),
            )
            .child(icon::tinted(
                IconName::ChevronsUpDown,
                15.,
                theme::muted_fg(),
            ))
    }

    /// The bid and the direction of its last move, with the ask and the spread stacked beside it.
    fn price_block(&self, fit: Fit) -> impl IntoElement {
        let block = div().flex_none().flex().flex_row().items_center().gap_2();
        let Some((bid, ask, spread)) = self.price_text() else {
            return block.child(
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child(if self.active.is_some() {
                        "Waiting for a price..."
                    } else {
                        ""
                    }),
            );
        };
        let tone = match self.tick {
            Some(Tick::Up) => theme::emerald(),
            Some(Tick::Down) => theme::destructive(),
            None => theme::fg(),
        };
        // The block keeps its width while the price moves, so what follows it in the bar stays
        // put: the width comes from how many characters the prices have (which changes only
        // with a new symbol, or a price that gains a digit), the digits are all as wide, and the
        // arrow has its place whether it shows or not.
        let digits = || FontFeatures(Arc::new(vec![("tnum".into(), 1)]));
        let chars = |text: &str| count_f32(text.chars().count());
        let bid_w = chars(&bid) * 10.5;
        let side_chars = ask
            .as_deref()
            .map_or(0.0, |a| chars(a) + 4.0)
            .max(spread.as_deref().map_or(0.0, |p| chars(p) + 7.0));
        let line = |label: &'static str, value: String| {
            div()
                .flex()
                .flex_row()
                .gap_1()
                .child(div().text_color(theme::muted_fg()).child(label))
                .child(div().text_color(theme::fg()).child(value))
        };
        block
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .w(px(bid_w))
                            .text_size(px(tokens::text::heading()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(digits())
                            .text_color(tone)
                            .child(bid),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(TICK_WIDTH))
                            .children(self.tick.map(|tick| {
                                icon::tinted(
                                    match tick {
                                        Tick::Up => IconName::ArrowUp,
                                        Tick::Down => IconName::ArrowDown,
                                    },
                                    13.,
                                    tone,
                                )
                            })),
                    ),
            )
            .when(fit.ask, |el| {
                el.child(
                    div()
                        .w(px(side_chars * 6.6))
                        .flex()
                        .flex_col()
                        .text_size(px(tokens::text::small()))
                        .font_features(digits())
                        .children(ask.map(|ask| line("Ask", ask)))
                        .children(spread.map(|pips| line("Spread", pips))),
                )
            })
    }

    /// The switches of the panel under the charts and of the ticket beside them.
    fn panel_toggles(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ticket_on_left = self
            .ticket
            .as_ref()
            .is_some_and(|t| t.read(cx).layout().dock == wyck_app::prefs::ticket::Dock::Left);
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                wyck_ui_kit::button::quiet("toggle-panel")
                    .selected(self.panel_open)
                    .icon(IconName::PanelBottom)
                    .tooltip("Positions, orders and alerts")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let open = !this.panel_open;
                        this.set_panel_open(open, cx);
                    })),
            )
            .child(
                wyck_ui_kit::button::quiet("toggle-ticket")
                    .selected(self.ticket_open)
                    .icon(if ticket_on_left {
                        IconName::PanelLeft
                    } else {
                        IconName::PanelRight
                    })
                    .tooltip("Order ticket")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let open = !this.ticket_open;
                        this.set_ticket_open(open, cx);
                    })),
            )
    }

    /// The account in one place: the state of the connection, demo or live, the equity and the
    /// open profit. It opens the account menu.
    fn account_pill(&self, fit: Fit, cx: &mut Context<Self>) -> impl IntoElement {
        let account = self.trading.read(cx);
        let ready = account.status == wyck_app::account::Status::Ready;
        let summary = account.summary();
        let currency = account.book.currency.clone();
        let profit_color = if summary.unrealized > 0.0 {
            theme::chart_up()
        } else if summary.unrealized < 0.0 {
            theme::chart_down()
        } else {
            theme::muted_fg()
        };
        // The connection only speaks up when something is wrong.
        let (dot, problem): (gpui::AnyElement, Option<&str>) = match &self.conn {
            Conn::Ready => (
                layout::status_dot(theme::emerald()).into_any_element(),
                None,
            ),
            Conn::Connecting => (
                anim::spin(
                    icon::tinted(IconName::LoaderCircle, 12., theme::muted_fg()),
                    "header-connecting",
                )
                .into_any_element(),
                Some("Connecting..."),
            ),
            Conn::Reconnecting => (
                layout::status_dot(theme::amber()).into_any_element(),
                Some("Reconnecting..."),
            ),
            Conn::Failed(_) => (
                layout::status_dot(theme::destructive()).into_any_element(),
                Some("Disconnected"),
            ),
        };
        let figure = |value: String, color: gpui::Rgba| {
            div()
                .text_size(px(tokens::text::body()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .child(value)
        };
        let open = self.menu_is(MenuKind::Account);
        div()
            .id("account-pill")
            .flex_none()
            .h(px(tokens::height::large()))
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_3()
            .rounded_lg()
            .border_1()
            .border_color(if open {
                theme::accent()
            } else {
                theme::border_hairline()
            })
            .cursor_pointer()
            .hover(|s| s.bg(theme::surface_hover()))
            .on_click(cx.listener(|this, _, _, cx| {
                this.toggle_menu(MenuKind::Account, cx);
            }))
            .child(dot)
            .child(layout::environment_badge(self.account.is_live))
            .children(problem.map(|text| {
                div()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::amber())
                    .child(text)
            }))
            .when(ready && problem.is_none(), |el| {
                el.when(fit.equity, |el| {
                    el.child(figure(
                        wyck_trading::amounts::format_money(summary.equity, &currency),
                        theme::fg(),
                    ))
                })
                .child(figure(
                    format!(
                        "{}{}",
                        if summary.unrealized > 0.0 { "+" } else { "" },
                        wyck_trading::amounts::format_money(summary.unrealized, &currency)
                    ),
                    profit_color,
                ))
            })
            .child(icon::tinted(IconName::ChevronDown, 13., theme::muted_fg()))
    }

    fn fullscreen_button(window: &mut Window) -> impl IntoElement {
        let fullscreen = window.is_fullscreen();
        wyck_ui_kit::button::quiet("toggle-fullscreen")
            .icon(if fullscreen {
                IconName::Minimize
            } else {
                IconName::Maximize
            })
            .tooltip(if fullscreen {
                "Exit full screen"
            } else {
                "Full screen"
            })
            .on_click(|_event, window, _cx| window.toggle_fullscreen())
    }
}
