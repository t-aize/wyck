//! The dashboard: the screen the application lands on once it is connected.
//!
//! The header on top and the price chart under it. The header follows the design: the traded
//! symbol with its tile and name, its price, the time frame selector, and the controls on the
//! right. Where the design showed sample values, this shows the engine's:
//!
//! - **The price** is the latest bid, colored for a moment in the direction it just moved, with a
//!   small arrow that stays. It says "No quote yet" until the first quote arrives.
//! - **The spread** in pips replaces the design's daily change, which needs candles the engine does
//!   not have yet. A number made up to fill the space would be worse than none.
//! - **The account kind and the mode** (demo, live or unknown; dry run or armed) are added on the
//!   right, and the session state too when it is not ready. The design had no room for them, but a
//!   trading screen must never hide which account it is on or whether orders are real.
//!
//! Buttons whose feature does not exist yet (indicators, the full workspace layout) are drawn
//! dimmed, do nothing, and say so in their tooltip. Full screen and "switch connection" work.

use gpui_kit::base::TransitionId;
use gpui_kit::base::transition;
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, Context, Div, FontWeight, Hsla, SharedString,
    Stateful, Window, div,
};
use wyck_engine::SessionState;
use wyck_engine::broker::ServiceKind;

use super::app_view::AppView;
use super::chart::Target;
use super::motion::{self, Hover, blend};
use super::theme::{self, sz};
use super::widgets::{Glyph, badge, glyph, icon_button_sized, rise, symbol_icon, tip};
use crate::dashboard::{SymbolHeader, Tick, Timeframe, symbol_header};
use crate::presentation::{self, Header};

/// The whole screen: the header on top, the chart area under it.
pub(super) fn dashboard(
    view: &mut AppView,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> impl IntoElement {
    let state = view.shell.model.read(cx).state.clone();
    let active = view.shell.controller.symbol();
    let listed = view
        .catalog
        .as_ref()
        .and_then(|c| c.find(&active).and_then(|i| c.entry(i)))
        .map(|e| e.info.clone());
    let symbol = symbol_header(&state, &active, listed.as_ref());
    let info = presentation::header(&state);
    let ready = matches!(state.session, SessionState::Ready);

    let bar = header(view, &symbol, &info, ready, window, cx);
    let namespace = match state.service {
        Some(ServiceKind::CtraderLocal) => "local",
        _ => "remote",
    };
    view.chart.update(cx, |chart, cx| {
        chart.retarget(
            Target {
                symbol: active.clone(),
                timeframe: view.timeframe,
                namespace,
                ready,
            },
            cx,
        );
    });
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .flex_col()
        .child(rise(0, bar.into_any_element(), window, cx))
        .child(view.chart.clone())
}

/// The header bar: symbol, price, time frames, then status and controls.
fn header(
    view: &mut AppView,
    symbol: &SymbolHeader,
    info: &Header,
    ready: bool,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Div {
    let divider = div().flex_none().w(sz(1.)).h(sz(24.)).bg(theme::border());
    let timeframes = timeframe_selector(view.timeframe, window, cx);
    let controls = controls(window, cx);
    let status = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(sz(8.))
        .child(badge(&info.kind))
        .child(badge(&info.mode))
        .when(!ready, |el| el.child(badge(&info.session)));

    div()
        .w_full()
        .flex_none()
        .h(sz(38.))
        .flex()
        .flex_row()
        .items_center()
        .px(sz(18.))
        .gap(sz(16.))
        .border_b_1()
        .border_color(theme::border())
        .child(symbol_block(symbol, window, cx))
        .child(price_block(view, symbol))
        .child(divider)
        .child(timeframes)
        .child(div().flex_1())
        .child(status)
        .child(controls)
}

/// The symbol: its icon, the ticker and the long name. It is a button: it opens the symbol picker
/// (Ctrl+K does too), and shows a chevron pair for that. Its ground fades in under the pointer.
fn symbol_block(
    symbol: &SymbolHeader,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    let hover = Hover::track("dashboard-symbol", window, cx);
    div()
        .id("dashboard-symbol")
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(sz(10.))
        .my(sz(-4.))
        .py(sz(4.))
        .pl(sz(6.))
        .pr(sz(8.))
        .ml(sz(-6.))
        .rounded(sz(8.))
        .bg(hover.mix(theme::alpha(theme::muted(), 0.0), theme::muted()))
        .cursor_pointer()
        .on_hover(hover.handler())
        .tooltip(tip("Change symbol (Ctrl+K)"))
        .on_click(cx.listener(|this, _, window, cx| this.open_picker(window, cx)))
        .child(symbol_icon(&symbol.icon, 26., theme::bg()))
        .child(
            div()
                .child(
                    div()
                        .text_size(sz(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::fg())
                        .child(symbol.symbol.clone()),
                )
                .child(
                    div()
                        .max_w(sz(220.))
                        .truncate()
                        .text_size(sz(9.5))
                        .text_color(theme::dim())
                        .child(symbol.name.clone()),
                ),
        )
        .child(glyph(
            Glyph::ChevronsUpDown,
            13.,
            hover.mix(theme::dim(), theme::fg()),
        ))
}

/// The price, the direction of its last move, and the spread.
fn price_block(view: &AppView, symbol: &SymbolHeader) -> Div {
    let tone = match view.tick_dir {
        Some(Tick::Up) => theme::green(),
        Some(Tick::Down) => theme::red(),
        None => theme::fg(),
    };
    let block = div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(sz(8.));

    let Some(price) = symbol.price.clone() else {
        return block.child(
            div()
                .text_size(sz(12.))
                .text_color(theme::dim())
                .child("No quote yet"),
        );
    };

    let figure = div()
        .text_size(sz(16.5))
        .font_weight(FontWeight::SEMIBOLD)
        .font_features(theme::tabular())
        .text_color(theme::fg())
        .child(price);
    // For half a second after a move the figure has the color of the move, then settles.
    let figure: AnyElement = if view.tick > 0 {
        figure
            .with_animation(
                gpui_kit::ElementId::from(("tick", view.tick as usize)),
                Animation::new(std::time::Duration::from_millis(700)),
                move |el, t| {
                    let settle = motion::exit().sample(t);
                    el.text_color(blend(tone, theme::fg(), settle))
                },
            )
            .into_any_element()
    } else {
        figure.into_any_element()
    };
    let arrow = view.tick_dir.map(|dir| {
        glyph(
            match dir {
                Tick::Up => Glyph::TickUp,
                Tick::Down => Glyph::TickDown,
            },
            7.,
            tone,
        )
    });
    let spread = symbol.spread_pips.clone().map(|pips| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(sz(6.))
            .px(sz(9.))
            .py(sz(4.))
            .rounded(sz(6.))
            .bg(theme::alpha(theme::fg(), 0.06))
            .text_size(sz(11.5))
            .text_color(theme::dim())
            .child("Spread")
            .child(
                div()
                    .font_features(theme::tabular())
                    .text_color(theme::fg())
                    .child(format!("{pips} pips")),
            )
    });
    block.child(figure).children(arrow).children(spread)
}

/// The segmented control that picks the time frame.
fn timeframe_selector(selected: Timeframe, window: &mut Window, cx: &mut Context<AppView>) -> Div {
    let pills: Vec<Stateful<Div>> = Timeframe::ALL
        .into_iter()
        .map(|timeframe| timeframe_pill(timeframe, timeframe == selected, window, cx))
        .collect();
    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(sz(2.))
        .p(sz(2.))
        .rounded(sz(7.))
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border())
        .children(pills)
}

/// One time frame. Its ground and text fade to the selected look when it is picked, and to a
/// lighter version of it under the pointer.
fn timeframe_pill(
    timeframe: Timeframe,
    selected: bool,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    let id = SharedString::from(format!("timeframe-{}", timeframe.label()));
    let hover = Hover::track(id.clone(), window, cx);
    let chosen = transition(
        TransitionId::from((id.clone(), "selected")),
        if selected { 1.0_f32 } else { 0.0 },
        motion::quick(),
        window,
        cx,
    );
    let amount = chosen.max(hover.amount);
    div()
        .id(id)
        .px(sz(9.))
        .py(sz(4.))
        .rounded(sz(6.))
        .bg(blend(
            theme::alpha(theme::muted(), 0.0),
            theme::muted(),
            amount,
        ))
        .text_color(blend(theme::dim(), theme::fg(), amount))
        .text_size(sz(11.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .on_hover(hover.handler())
        .on_click(cx.listener(move |this, _, _, cx| this.set_timeframe(timeframe, cx)))
        .child(timeframe.label())
}

/// A control whose feature is not built: dimmed, inert, and honest in its tooltip.
fn unavailable(button: Stateful<Div>, label: &'static str) -> Stateful<Div> {
    button.opacity(0.4).cursor_default().tooltip(tip(label))
}

/// The buttons on the right: indicators, layout, full screen, switch connection.
fn controls(window: &mut Window, cx: &mut Context<AppView>) -> Div {
    let dim: Hsla = theme::dim();
    let indicators = unavailable(
        icon_button_sized("dashboard-indicators", 28., 7., window, cx).child(glyph(
            Glyph::Sliders,
            14.,
            dim,
        )),
        "Indicators (coming soon)",
    );
    let workspace = unavailable(
        icon_button_sized("dashboard-workspace", 26., 5., window, cx)
            .h(sz(24.))
            .child(glyph(Glyph::LayoutGrid, 13., dim)),
        "Full workspace (coming soon)",
    );
    let chart_only = div()
        .id("dashboard-chart-only")
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(sz(26.))
        .h(sz(24.))
        .rounded(sz(5.))
        .bg(theme::muted())
        .tooltip(tip("Chart only"))
        .child(glyph(Glyph::Square, 13., theme::fg()));
    let layouts = div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(sz(2.))
        .p(sz(2.))
        .rounded(sz(7.))
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border())
        .child(workspace)
        .child(chart_only);
    let fullscreen = icon_button_sized("dashboard-fullscreen", 28., 7., window, cx)
        .tooltip(tip("Full screen"))
        .on_click(|_, window, _| window.toggle_fullscreen())
        .child(glyph(Glyph::Scan, 14., dim));
    let switch = icon_button_sized("dashboard-switch", 28., 7., window, cx)
        .tooltip(tip("Switch connection"))
        .on_click(cx.listener(|this, _, window, cx| this.switch_connection(window, cx)))
        .child(glyph(Glyph::LogOut, 14., dim));
    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(sz(8.))
        .child(indicators)
        .child(layouts)
        .child(fullscreen)
        .child(switch)
}
