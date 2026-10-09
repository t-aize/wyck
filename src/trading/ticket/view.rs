//! How the order ticket looks: a header that stays on top, the blocks in the order the user
//! chose (each a card, like the groups of the settings panels), and a footer that stays at the
//! bottom with what stops the order and the button that sends it.
//!
//! Every size comes from [`crate::ui::kit::tokens`]. The density only changes the space between and
//! inside the cards, never the size of a text or of a control.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Div, SharedString, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, NumberInput};
use gpui_kit::component::{Disableable, Selectable, Sizable, StyledExt as _};

use super::prefs::{Density, Kind, Line, Section, Slot, Span, Tif};
use super::{OrderTicket, Plan, TicketEvent, customize, nice, plan, side_of, stop_limit_price};
use crate::chart_core::study::atr_stop::Smoothing;
use crate::trading::account::Busy;
use crate::trading::book::is_buy;
use crate::trading::math::{self, Contract, Limit, Offset, SizeMode};
use crate::ui::kit::{
    confirm::confirm,
    controls,
    focus::Keyboard,
    form, icon,
    menu::{self as popup, Entry, Item},
    number, theme, tokens,
};

mod exits;
mod options;
mod order;
mod plans;
mod positions;
mod protection;

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

    // ---- the blocks ----

    fn summary(&self, f: &Frame) -> AnyElement {
        let plan = &f.plan;
        let ratio = plan
            .risk
            .zip(plan.reward)
            .filter(|(r, _)| *r > 0.0)
            .map(|(r, w)| format!("1 : {:.2}", w / r));
        let mut card = card(f.m).child(card_head("Summary", None));
        // What is risked against what may be won, drawn to scale.
        if let Some((risk, reward)) = plan
            .risk
            .zip(plan.reward)
            .filter(|(r, w)| *r > 0.0 && *w > 0.0)
        {
            card = card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_0p5()
                            .h(px(6.))
                            .rounded_full()
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative((risk / (risk + reward)) as f32))
                                    .bg(theme::chart_down()),
                            )
                            .child(div().h_full().flex_1().bg(theme::chart_up())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(px(tokens::text::small()))
                            .child(
                                div()
                                    .text_color(theme::chart_down())
                                    .child(format!("Risk {}", f.money(risk))),
                            )
                            .child(
                                div()
                                    .text_color(theme::chart_up())
                                    .child(format!("Reward {}", f.money(reward))),
                            ),
                    ),
            );
        }
        let mut rows = div().flex().flex_col().gap_1();
        for shown in self.layout.visible_lines() {
            let text = match shown {
                Line::Volume => f.lots.zip(f.units).map_or_else(
                    || "-".into(),
                    |(l, u)| format!("{} lots, {} units", math::format_lots(l), format_units(u)),
                ),
                Line::Notional => plan.sized.zip(plan.entry).zip(plan.rate).map_or_else(
                    || "-".into(),
                    |((s, e), r)| f.money(s.volume as f64 / 100.0 * e * r),
                ),
                Line::Risk => match plan.risk {
                    Some(r) => format!("{}  {}", f.money(r), f.share(r)),
                    None if self.stop_on => "-".into(),
                    None => "No stop loss".into(),
                },
                Line::Reward => plan
                    .reward
                    .map_or_else(|| "-".into(), |r| format!("{}  {}", f.money(r), f.share(r))),
                Line::RiskReward => ratio.clone().unwrap_or_else(|| "-".into()),
                Line::Margin => f.margin.map_or_else(|| "-".into(), |m| f.money(m)),
                Line::PipValue => match (f.units, plan.rate) {
                    (Some(u), Some(rate)) => f.money(f.contract.pip() * u * rate),
                    (Some(u), None) => math::format_money(f.contract.pip() * u, &f.quote_currency),
                    _ => "-".into(),
                },
                Line::SpreadCost => f
                    .bid
                    .zip(f.ask)
                    .zip(f.units)
                    .zip(plan.rate)
                    .map_or_else(|| "-".into(), |(((b, a), u), r)| f.money((a - b) * u * r)),
            };
            rows = rows.child(summary_row(shown.label(), text));
        }
        card.child(rows).into_any_element()
    }

    /// The warnings that do not stop the order, then what does.
    fn warnings(&self, f: &Frame) -> Vec<String> {
        let mut warnings: Vec<String> = Vec::new();
        match f.plan.sized.and_then(|s| s.limit) {
            Some(Limit::Min) => warnings.push(format!(
                "Raised to the least volume, {} lots: more at stake than asked",
                math::format_lots(f.contract.lots_of_volume(f.contract.min_volume))
            )),
            Some(Limit::Max) => warnings.push("Cut to the most volume the broker takes".into()),
            None => {}
        }
        if let Some(m) = f.margin
            && m > f.free_margin
        {
            warnings.push("Not enough free margin".into());
        }
        let high = self.layout.high_risk;
        if let Some(risk) = f.plan.risk
            && f.balance > 0.0
            && risk / f.balance * 100.0 > high
        {
            warnings.push(format!(
                "Risks more than {}% of the balance",
                number::format(high, 2)
            ));
        }
        warnings.extend(f.plan.problem.clone());
        warnings
    }

    /// What the safety limits say now: a lock that stops new orders (with the way out), a request
    /// that got no answer, and how much of the day's loss limit is used.
    fn safety_banners(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let account_entity = self.account.clone();
        let (lock, uncertain, standing, limit, positions, currency, text) = {
            let account = self.account.read(cx);
            let standing = account.standing();
            let lock = account.lock();
            (
                lock,
                account.is_uncertain(),
                standing,
                account.risk().daily_loss_limit(standing.day_start_balance),
                account.book.positions.len(),
                account.book.currency.clone(),
                lock.map(|lock| {
                    lock.text(
                        &|m| math::format_money(m, &account.book.currency),
                        standing.now,
                    )
                }),
            )
        };
        let mut out: Vec<AnyElement> = Vec::new();
        if let (Some(_), Some(text)) = (lock, text) {
            let close = account_entity.clone();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::destructive())
                    .bg(theme::destructive_bg())
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .gap_2()
                            .text_size(px(tokens::text::body()))
                            .text_color(theme::destructive())
                            .child(div().flex_none().pt(px(1.)).child(icon::tinted(
                                IconName::ShieldAlert,
                                13.,
                                theme::destructive(),
                            )))
                            .child(div().flex_1().min_w_0().child(text)),
                    )
                    .when(positions > 0, |el| {
                        el.child(
                            text_button("ticket-lock-close-all", "Close all positions").on_click(
                                move |_, window, cx| {
                                    let close = close.clone();
                                    confirm(
                                        window,
                                        cx,
                                        "Close every position?",
                                        "All open positions are closed at market.",
                                        move |_, cx| {
                                            close.update(cx, |a, cx| a.close_all(None, cx))
                                        },
                                    );
                                },
                            ),
                        )
                    })
                    .into_any_element(),
            );
        }
        if uncertain {
            let refresh = account_entity.clone();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(theme::amber_bg())
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::amber())
                    .child("A request got no answer. It may have gone through: check the positions before sending again.")
                    .child(text_button("ticket-uncertain-refresh", "Check the account").on_click(
                        move |_, _, cx| refresh.update(cx, |a, cx| a.on_ready(cx)),
                    ))
                    .into_any_element(),
            );
        }
        if let Some(limit) = limit.filter(|l| *l > 0.0) {
            let used = (-standing.day_pnl).max(0.0);
            let share = (used / limit).clamp(0.0, 1.0) as f32;
            let color = if share >= 1.0 {
                theme::destructive()
            } else if share >= 0.7 {
                theme::amber()
            } else {
                theme::emerald()
            };
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(px(tokens::text::small()))
                            .text_color(theme::muted_fg())
                            .child("Daily loss")
                            .child(format!(
                                "{} of {}",
                                math::format_money(used, &currency),
                                math::format_money(limit, &currency)
                            )),
                    )
                    .child(
                        div()
                            .h(px(4.))
                            .w_full()
                            .rounded_full()
                            .bg(theme::fg_alpha(0.12))
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(color)
                                    .w(gpui::relative(share.max(0.02))),
                            ),
                    )
                    .into_any_element(),
            );
        }
        out
    }

    fn send_button(&self, f: &Frame, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.account.read(cx).lock().is_some();
        let blocked = f.plan.problem.is_some() || f.busy || locked;
        Button::new("ticket-send")
            .cursor_pointer()
            .when(blocked, |button| button.cursor_not_allowed())
            .label(self.describe(&f.plan, cx))
            .with_size(gpui_kit::component::Size::Large)
            .disabled(blocked)
            .loading(f.busy)
            .bg(if self.buy {
                theme::chart_up()
            } else {
                theme::chart_down()
            })
            .text_color(theme::bg())
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
            .children(warning_rows(warnings))
            .when(with_send, |el| el.child(self.send_button(f, cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
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
                                    confirm(
                                        window,
                                        cx,
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

fn warning_rows(warnings: &[String]) -> Vec<AnyElement> {
    warnings
        .iter()
        .map(|message| {
            div()
                .flex()
                .flex_row()
                .items_start()
                .gap_2()
                .p_2()
                .rounded_md()
                .bg(theme::amber_bg())
                .text_size(px(tokens::text::body()))
                .text_color(theme::amber())
                .child(div().flex_none().pt(px(1.)).child(icon::tinted(
                    IconName::TriangleAlert,
                    13.,
                    theme::amber(),
                )))
                .child(div().flex_1().min_w_0().child(message.clone()))
                .into_any_element()
        })
        .collect()
}

/// The deposit currency, or a word for it before it is known.
fn or_money(currency: &str) -> &str {
    if currency.is_empty() {
        "money"
    } else {
        currency
    }
}

fn summary_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .gap_2()
        .text_size(px(tokens::text::body()))
        .child(div().text_color(theme::muted_fg()).child(label))
        .child(div().text_color(theme::fg()).child(value))
}

fn format_units(units: f64) -> String {
    math::format_money(units, "")
        .trim_end_matches("00")
        .trim_end_matches('.')
        .to_owned()
}
