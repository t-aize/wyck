//! How the order ticket looks: a header that stays on top, the blocks in the order the user
//! chose (each a card, like the groups of the settings panels), and a footer that stays at the
//! bottom with what stops the order and the button that sends it.
//!
//! Every size comes from [`wyck_ui_kit::tokens`]. The density only changes the space between and
//! inside the cards, never the size of a text or of a control.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Div, SharedString, Window, div, px};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::prelude::{Disableable, StyledExt as _};

use super::helpers::{nice, side_of, stop_limit_price};
use super::{OrderTicket, Plan, TicketEvent, customize, plan};
use wyck_app::account::Busy;
use wyck_app::prefs::ticket::{Density, Kind, Line, Section, Slot, Span, Tif};
use wyck_trading::atr_stop::Smoothing;
use wyck_trading::math;
use wyck_trading::math::{Contract, Limit, Offset, SizeMode};
use wyck_ui_kit::{
    confirm::{Tone, confirm_as},
    controls,
    focus::Keyboard,
    form, icon,
    menu::{self as popup, Entry, Item},
    number, theme, tokens,
};

/// The height of the bar that shows how a total is shared.
const METER_HEIGHT: f32 = 6.0;
/// The height of a thin progress bar.
const THIN_METER_HEIGHT: f32 = 4.0;

mod exits;
mod options;
mod order;
mod plans;
mod positions;
mod protection;
mod safety;
mod summary;

/// What a button of a position row does.
type Action = Rc<dyn Fn(&mut Window, &mut App)>;

/// The space the density gives the panel. Text and controls do not change with it.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    /// Between two cards.
    gap: f32,
    /// Around the cards, at the edge of the panel.
    pad: f32,
    /// Between the rows of a card, and half of what surrounds them.
    inner: f32,
    compact: bool,
}

impl Metrics {
    fn of(density: Density) -> Self {
        match density {
            Density::Comfortable => Self {
                gap: 12.,
                pad: 12.,
                inner: 8.,
                compact: false,
            },
            Density::Compact => Self {
                gap: 8.,
                pad: 8.,
                inner: 6.,
                compact: true,
            },
        }
    }
}

/// What the blocks are drawn from, worked out once per frame.
struct Frame {
    m: Metrics,
    contract: Contract,
    plan: Plan,
    bid: Option<f64>,
    ask: Option<f64>,
    busy: bool,
    balance: f64,
    free_margin: f64,
    currency: String,
    quote_currency: String,
    name: SharedString,
    spread: String,
    lots: Option<f64>,
    units: Option<f64>,
    /// The margin of the order, when the server has said it for this volume.
    margin: Option<f64>,
    /// The ATR the stop loss is set from, when it is one.
    atr: Option<f64>,
}

impl Frame {
    fn money(&self, amount: f64) -> String {
        math::format_money(amount, &self.currency)
    }

    /// An amount and its share of the balance.
    fn share(&self, amount: f64) -> String {
        if self.balance > 0.0 {
            format!("{:.2}%", amount / self.balance * 100.0)
        } else {
            String::new()
        }
    }

    fn price(&self, price: Option<f64>) -> String {
        price.map_or_else(|| "-".to_owned(), |p| self.contract.format_price(p))
    }
}

/// A card of the ticket: what the groups of the settings panels are, made for a narrow column.
fn card(m: Metrics) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(m.inner))
        .p(px(m.inner + 4.))
        .rounded_lg()
        .border_1()
        .border_color(theme::border_subtle())
        .bg(theme::fg_alpha(0.025))
}

/// The title of a card, with what sits at its right (a switch, a count, a menu).
fn card_head(title: impl Into<SharedString>, right: Option<AnyElement>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
        .h(px(tokens::height::compact()))
        .child(
            div()
                .text_size(px(tokens::text::body()))
                .font_semibold()
                .text_color(theme::muted_fg())
                .child(title.into()),
        )
        .children(right)
}

/// A label with its control at the right, on one row.
fn line(label: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_2()
        .min_h(px(tokens::height::compact()))
        .child(
            div()
                .text_size(px(tokens::text::body()))
                .text_color(theme::muted_fg())
                .child(label.into()),
        )
        .child(control)
}

/// A line of secondary text.
fn hint(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(tokens::text::small()))
        .text_color(theme::muted_fg())
        .child(text.into())
}

/// A small text button: an action that is not the point of the block.
fn text_button(id: &'static str, text: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .keyboard()
        .cursor_pointer()
        .text_size(px(tokens::text::body()))
        .text_color(theme::accent())
        .hover(|s| s.underline())
        .child(text)
}

/// The tint of a color, for the background of a warning or of a side.
fn tint(color: gpui::Rgba, alpha: f32) -> gpui::Hsla {
    let mut hsla: gpui::Hsla = color.into();
    hsla.a = alpha;
    hsla
}

impl OrderTicket {
    fn size_label(mode: SizeMode, currency: &str) -> String {
        match mode {
            SizeMode::Lots => "Lots".to_owned(),
            SizeMode::Units => "Units".to_owned(),
            SizeMode::RiskBalance => "Risk % of balance".to_owned(),
            SizeMode::RiskEquity => "Risk % of equity".to_owned(),
            SizeMode::RiskMoney => format!("Risk in {}", or_money(currency)),
            SizeMode::FreeMargin => "% of free margin".to_owned(),
        }
    }

    fn unit_label(unit: Offset, currency: &str) -> String {
        match unit {
            Offset::Price => "Price".to_owned(),
            Offset::Pips => "Pips".to_owned(),
            Offset::Money => or_money(currency).to_owned(),
            Offset::Percent => "% balance".to_owned(),
            Offset::Ratio => "R".to_owned(),
        }
    }

    fn send_button(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.account.read(cx).lock().is_some();
        let blocked = f.plan.problem.is_some() || f.busy || locked;
        wyck_ui_kit::button::trade("ticket-send", self.buy)
            .when(blocked, |button| button.cursor_not_allowed())
            .label(self.describe(&f.plan, cx))
            .disabled(blocked)
            .loading(f.busy)
            .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
            .into_any_element()
    }

    /// The bottom of the panel, which does not scroll: what is wrong with the order, the button
    /// that sends it and the switch of one-click trading.
    fn footer(
        &self,
        f: &Frame,
        warnings: &[String],
        with_send: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(f.m.pad))
            .border_t_1()
            .border_color(theme::border_hairline())
            .bg(theme::bg())
            .children(self.safety_banners(cx))
            .children(summary::warning_rows(warnings))
            .when(with_send, |el| el.child(self.send_button(f, cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        controls::switch("ticket-one-click", self.one_click)
                            .label("One-click trading")
                            .on_click(cx.listener(|this, checked: &bool, window, cx| {
                                if *checked && !this.one_click {
                                    // Turning it on says what it does, and asks once.
                                    let ticket = cx.entity();
                                    let live = this.account.read(cx).is_live();
                                    let text = if live {
                                        "This is a LIVE account. Buy and Sell will send real orders as soon as you click, with no confirmation."
                                    } else {
                                        "Buy and Sell will send orders as soon as you click, with no confirmation."
                                    };
                                    confirm_as(
                                        window,
                                        cx,
                                        if live { Tone::Danger } else { Tone::Caution },
                                        "Turn on one-click trading?",
                                        text,
                                        move |_, cx| {
                                            ticket.update(cx, |t, cx| {
                                                t.one_click = true;
                                                cx.emit(TicketEvent::LinesChanged);
                                                cx.notify();
                                            });
                                        },
                                    );
                                    return;
                                }
                                this.one_click = *checked;
                                cx.emit(TicketEvent::LinesChanged);
                                cx.notify();
                            })),
                    )
                    .when(self.one_click, |el| {
                        el.child(
                            div()
                                .text_size(px(tokens::text::small()))
                                .text_color(theme::amber())
                                .child(
                                    "Orders are sent without asking. The send button sends at once.",
                                ),
                        )
                    }),
            )
            .into_any_element()
    }
}

impl Render for OrderTicket {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A followed drawing is read first: everything below works from what it asks.
        self.sync_link(window, cx);
        self.request_atr(cx);
        let contract = self.contract(cx);
        let mut plan = self.plan(cx);
        // The protections the defaults ask for start as soon as there is a price to start from.
        if self.autofill && plan.entry.is_some() {
            self.apply_defaults(window, cx);
            plan = self.plan(cx);
        }
        // The margin follows the volume.
        if let Some(symbol) = self.symbol.as_ref().map(|s| s.id) {
            let volume = plan.sized.map_or(contract.min_volume, |s| s.volume);
            if self.margin_asked != Some((symbol, volume)) {
                self.request_margin(symbol, volume, cx);
            }
        }
        let (bid, ask) = self.quote(cx);
        let (busy, summary, currency, quote_currency) = {
            let account = self.account.read(cx);
            (
                account.is_busy(Busy::Placing),
                account.summary(),
                account.book.currency.clone(),
                self.symbol
                    .as_ref()
                    .and_then(|s| account.book.quote_currency.get(&s.id).cloned())
                    .unwrap_or_default(),
            )
        };
        let margin = self
            .margin
            .filter(|m| Some(m.volume) == plan.sized.map(|s| s.volume))
            .map(|m| side_of(m.order, self.buy));
        let atr = self
            .chart
            .as_ref()
            .filter(|_| self.stop_atr)
            .and_then(|chart| chart.read(cx).atr_value(&self.atr));
        let frame = Frame {
            m: Metrics::of(self.layout.density),
            spread: bid
                .zip(ask)
                .map(|(b, a)| format!("{:.1}", contract.pips(a - b)))
                .unwrap_or_default(),
            lots: plan.sized.map(|s| contract.lots_of_volume(s.volume)),
            units: plan.sized.map(|s| s.volume as f64 / 100.0),
            name: self
                .symbol
                .as_ref()
                .map_or_else(|| SharedString::from("No symbol"), |s| s.name.clone()),
            contract,
            plan,
            bid,
            ask,
            busy,
            balance: summary.balance,
            free_margin: summary.free_margin,
            currency,
            quote_currency,
            margin,
            atr,
        };
        let m = frame.m;
        let has_symbol = self.symbol.is_some();
        let warnings = if has_symbol {
            self.warnings(&frame)
        } else {
            Vec::new()
        };

        let mut blocks: Vec<AnyElement> = Vec::new();
        if has_symbol {
            for section in self.layout.clone().visible_sections() {
                blocks.push(match section {
                    Section::Sides => self.sides(&frame, cx),
                    Section::Order => self.order_block(&frame, cx),
                    Section::Size => self.size_block(&frame, window, cx),
                    Section::StopLoss => self.protection(true, &frame, window, cx),
                    Section::TakeProfit => self.protection(false, &frame, window, cx),
                    Section::Exits => self.exits_block(&frame, cx),
                    Section::TimeStop => self.time_stop_block(&frame, window, cx),
                    Section::Plans => self.plans_block(&frame, cx),
                    Section::Options => self.options(&frame, window, cx),
                    Section::Positions => self.positions(&frame, cx),
                    Section::Summary => self.summary(&frame),
                    // The button has a place of its own, at the bottom.
                    Section::Send => continue,
                });
            }
        }
        let with_send = self.layout.shows(Section::Send);

        div()
            .id("order-ticket")
            .flex()
            .flex_col()
            .size_full()
            .child(self.header(&frame, cx))
            .child(
                div()
                    .id("ticket-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(m.gap))
                    .p(px(m.pad))
                    .children(blocks)
                    .when(!has_symbol, |el| {
                        el.child(form::empty(
                            IconName::Info,
                            "Pick a symbol on a chart to trade it.",
                        ))
                    }),
            )
            .when(has_symbol, |el| {
                el.child(self.footer(&frame, &warnings, with_send, cx))
            })
    }
}

/// The deposit currency, or a word for it before it is known.
fn or_money(currency: &str) -> &str {
    if currency.is_empty() {
        "money"
    } else {
        currency
    }
}

fn format_units(units: f64) -> String {
    math::format_money(units, "")
        .trim_end_matches("00")
        .trim_end_matches('.')
        .to_owned()
}
