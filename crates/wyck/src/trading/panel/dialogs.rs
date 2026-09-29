//! The dialogs the account panel opens: the one that modifies a position or an order, and the one
//! that edits an alert.

use gpui::prelude::*;
use gpui::{App, Context, Entity, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::Sizable;
use gpui_kit::component::input::{Input, InputState};

use crate::alerts::model::PriceKind;
use crate::alerts::{Alert, Alerts, Condition, Source, Trigger};
use crate::trading::account::Account;
use wyck_chart::study::{StudyConfig, StudyKind};
use wyck_ui::focus::Keyboard;
use wyck_ui::menu::{self as popup, Entry, Item};
use wyck_ui::{button, controls, form, form::Head, icon, modal, number, theme, tokens};

// ---- modifying a position or an order ----

#[derive(Debug, Clone, Copy)]
pub enum Target {
    Position(i64),
    Order(i64),
}

struct ProtectionEditor {
    account: Entity<Account>,
    target: Target,
    price: Entity<InputState>,
    stop_loss: Entity<InputState>,
    take_profit: Entity<InputState>,
}

pub fn open_protection(
    account: Entity<Account>,
    target: Target,
    window: &mut Window,
    cx: &mut App,
) {
    let (price, stop_loss, take_profit, digits) = {
        let book = &account.read(cx).book;
        match target {
            Target::Position(id) => match book.positions.get(&id) {
                Some(p) => (
                    None,
                    p.stop_loss,
                    p.take_profit,
                    book.contract(p.trade_data.symbol_id),
                ),
                None => return,
            },
            Target::Order(id) => match book.orders.get(&id) {
                Some(o) => (
                    o.limit_price.or(o.stop_price),
                    o.stop_loss,
                    o.take_profit,
                    book.contract(o.trade_data.symbol_id),
                ),
                None => return,
            },
        }
    };
    let text = |v: Option<f64>| v.map(|v| digits.format_price(v)).unwrap_or_default();
    let editor = cx.new(|cx| ProtectionEditor {
        account,
        target,
        price: cx.new(|cx| InputState::new(window, cx).default_value(text(price))),
        stop_loss: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(text(stop_loss))
                .placeholder("None")
        }),
        take_profit: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(text(take_profit))
                .placeholder("None")
        }),
    });
    modal::open(editor, modal::Options::new(420.0, 400.0), window, cx);
}

impl ProtectionEditor {
    fn save(&self, cx: &mut App) {
        let read = |state: &Entity<InputState>| number::parse(&state.read(cx).value());
        let (price, sl, tp) = (
            read(&self.price),
            read(&self.stop_loss),
            read(&self.take_profit),
        );
        let target = self.target;
        self.account.update(cx, |account, cx| match target {
            Target::Position(id) => account.protect_position(id, sl, tp, cx),
            Target::Order(id) => {
                let current = account
                    .book
                    .orders
                    .get(&id)
                    .and_then(|o| o.limit_price.or(o.stop_price));
                if let Some(price) = price.filter(|p| Some(*p) != current) {
                    account.move_order(id, price, cx);
                }
                account.protect_order(id, sl, tp, cx);
            }
        });
    }
}

impl Render for ProtectionEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, subtitle) = match self.target {
            Target::Position(_) => ("Modify the position", "Stop loss and take profit"),
            Target::Order(_) => ("Modify the order", "Price, stop loss and take profit"),
        };
        let head = Head {
            icon: IconName::ShieldCheck,
            title: title.into(),
            subtitle: subtitle.into(),
        };
        let mut rows = Vec::new();
        if matches!(self.target, Target::Order(_)) {
            rows.push(form::field(
                "Price",
                None,
                form::text_field(&self.price, tokens::field::text()),
            ));
        }
        rows.push(form::field(
            "Stop loss",
            Some("Leave it empty to remove it"),
            form::text_field(&self.stop_loss, tokens::field::text()),
        ));
        rows.push(form::field(
            "Take profit",
            Some("Leave it empty to remove it"),
            form::text_field(&self.take_profit, tokens::field::text()),
        ));
        let body = form::page()
            .child(form::group(IconName::Target, "Levels", rows))
            .child(form::note("Lines can also be dragged on the chart."));
        let save = cx.entity();
        let footer = form::footer(
            Vec::new(),
            vec![
                button::action("protection-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                button::action("protection-save", "Save", None, true, move |window, cx| {
                    save.update(cx, |editor, cx| editor.save(cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        form::dialog(head, modal::dismiss, body, footer)
    }
}

// ---- editing an alert ----

struct AlertEditor {
    alerts: Entity<Alerts>,
    id: u64,
    digits: u32,
    /// The alert as it is being edited: what it watches, its condition and trigger.
    draft: Alert,
    price: Entity<InputState>,
    upper: Entity<InputState>,
    amount: Entity<InputState>,
    minutes: Entity<InputState>,
    /// How many hours until it stops watching; empty or 0 never.
    expiry: Entity<InputState>,
    period: Entity<InputState>,
    message: Entity<InputState>,
    tag: Entity<InputState>,
}

/// The indicators an alert can watch: the ones drawn from the bars, not scripts or profiles.
fn watchable() -> Vec<StudyKind> {
    StudyKind::ALL
        .into_iter()
        .filter(|kind| !matches!(kind, StudyKind::Custom | StudyKind::VolumeProfile))
        .collect()
}

pub fn open_alert(alerts: Entity<Alerts>, id: u64, window: &mut Window, cx: &mut App) {
    let Some(alert) = alerts.read(cx).book().get(id).cloned() else {
        return;
    };
    let digits = alerts
        .read(cx)
        .digits
        .get(&alert.symbol_id)
        .copied()
        .unwrap_or(5);
    let period_value = match &alert.source {
        Source::Indicator { study, .. } => study.input("length"),
        _ => 0.0,
    };
    let level = |value: f64| {
        if value == 0.0 {
            String::new()
        } else {
            format!("{:.*}", digits as usize, value)
        }
    };
    let editor = cx.new(|cx| AlertEditor {
        alerts,
        id,
        digits,
        price: cx.new(|cx| {
            InputState::new(window, cx).default_value(if alert.source.is_indicator() {
                format!("{}", alert.price)
            } else {
                level(alert.price)
            })
        }),
        upper: cx.new(|cx| InputState::new(window, cx).default_value(level(alert.upper))),
        amount: cx
            .new(|cx| number::state(number::Kind::Percent, alert.amount.max(0.5), window, cx)),
        minutes: cx.new(|cx| {
            number::state(
                number::Kind::Count,
                f64::from(alert.minutes.max(15)),
                window,
                cx,
            )
        }),
        expiry: cx.new(|cx| {
            let hours = alert
                .expires_at
                .map(|at| ((at - crate::chart::now_ms()) as f64 / 3_600_000.0).max(0.0))
                .filter(|h| *h > 0.0)
                .map_or(String::new(), |h| format!("{h:.1}"));
            InputState::new(window, cx)
                .default_value(hours)
                .placeholder("Never")
        }),
        period: cx.new(|cx| number::state(number::Kind::Count, period_value.max(1.0), window, cx)),
        message: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(alert.message.clone())
                .placeholder("What to say: {symbol} {value} {level}")
        }),
        tag: cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(alert.tag.clone())
                .placeholder("Optional tag")
        }),
        draft: alert,
    });
    modal::open(editor, modal::Options::new(520.0, 680.0), window, cx);
}

impl AlertEditor {
    fn read(&self, state: &Entity<InputState>, cx: &App) -> Option<f64> {
        number::parse(&state.read(cx).value())
    }

    /// The conditions that make sense for what is watched.
    fn conditions(&self) -> Vec<Condition> {
        Condition::ALL
            .into_iter()
            .filter(|c| match (&self.draft.source, &self.draft.versus) {
                (Source::Indicator { .. }, _) => *c != Condition::MovesBy,
                (_, Some(Source::Drawing { .. })) => {
                    !matches!(c, Condition::MovesBy | Condition::ChangesDirection)
                }
                _ => *c != Condition::ChangesDirection,
            })
            .collect()
    }

    /// Puts the draft back in a shape that means something after what it watches changed.
    fn mend(&mut self) {
        if !self.conditions().contains(&self.draft.condition) {
            self.draft.condition = Condition::Crossing;
        }
        if self.draft.source.is_indicator() && self.draft.trigger == Trigger::OncePerBar {
            self.draft.trigger = Trigger::EveryTime;
        }
    }

    fn save(&self, cx: &mut App) {
        let mut alert = self.draft.clone();
        if let Some(price) = self.read(&self.price, cx)
            && (price > 0.0 || alert.source.is_indicator())
        {
            alert.price = price;
        }
        alert.upper = self.read(&self.upper, cx).unwrap_or(alert.upper);
        alert.amount = self.read(&self.amount, cx).unwrap_or(alert.amount);
        alert.minutes = self
            .read(&self.minutes, cx)
            .map_or(alert.minutes, |m| m.max(1.0) as u32);
        alert.message = self.message.read(cx).value().to_string();
        alert.tag = self.tag.read(cx).value().trim().to_owned();
        let hours = self.read(&self.expiry, cx).filter(|h| *h > 0.0);
        alert.expires_at = hours.map(|h| crate::chart::now_ms() + (h * 3_600_000.0) as i64);
        if let Source::Indicator { study, .. } = &mut alert.source
            && let Some(length) = self.read(&self.period, cx)
            && study.kind.spec().inputs.iter().any(|i| i.key == "length")
        {
            study
                .inputs
                .insert("length".to_owned(), length.max(1.0).round());
        }
        let id = self.id;
        self.alerts.update(cx, |alerts, cx| {
            alerts.edit(cx, |book| {
                if let Some(stored) = book.get_mut(id) {
                    let (created, fired_at, fired_count) =
                        (stored.created_at, stored.fired_at, stored.fired_count);
                    *stored = alert;
                    stored.created_at = created;
                    stored.fired_at = fired_at;
                    stored.fired_count = fired_count;
                    stored.active = true;
                    stored.snoozed_until = None;
                    stored.bar_key = None;
                }
            });
        });
    }

    /// A button that opens a list of choices under it.
    fn select(
        &self,
        key: &'static str,
        text: String,
        items: Vec<Item>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let menu = popup::Menu::new(key, window, cx);
        let open = menu.is_open(cx);
        let toggle = menu.clone();
        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id(key)
                    .keyboard()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .h(px(tokens::height::control()))
                    .px_2()
                    .min_w(px(tokens::field::number()))
                    .rounded_md()
                    .border_1()
                    .border_color(if open {
                        theme::accent()
                    } else {
                        theme::border_subtle()
                    })
                    .cursor_pointer()
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::fg())
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, _, cx| toggle.toggle(cx))
                    .child(div().flex_1().child(text))
                    .child(icon::tinted(IconName::ChevronDown, 12., theme::muted_fg())),
            )
            .children(menu.popup(
                if open { items } else { Vec::new() },
                popup::Placement::Below(tokens::height::control()),
                window,
                cx,
            ))
            .into_any_element()
    }
}

impl Render for AlertEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let symbol = self.draft.symbol.clone();
        let is_indicator = self.draft.source.is_indicator();
        let drawing = self.draft.drawing();
        let condition = self.draft.condition;
        let (save, kind_this) = (cx.entity(), cx.entity());
        let head = Head {
            icon: IconName::BellRing,
            title: format!("Alert on {symbol}").into(),
            subtitle: "What to watch, when to fire, and how often".into(),
        };

        // What it watches.
        let source_this = cx.entity();
        let mut watch = vec![form::field(
            "Watch",
            None,
            controls::segmented(
                "alert-source",
                &["Price", "Indicator"],
                usize::from(is_indicator),
                move |choice, window, cx| {
                    source_this.update(cx, |e, cx| {
                        e.draft.source = if choice == 1 {
                            Source::Indicator {
                                study: Box::new(StudyConfig::new(StudyKind::Rsi)),
                                plot: 0,
                            }
                        } else {
                            Source::Price {
                                price: PriceKind::Bid,
                            }
                        };
                        if choice == 1 && e.draft.price <= 1.0 {
                            e.draft.price = 70.0;
                            e.price.update(cx, |s, cx| s.set_value("70", window, cx));
                        }
                        e.mend();
                        cx.notify();
                    });
                },
            ),
        )];
        match self.draft.source.clone() {
            Source::Indicator { study, plot } => {
                let spec = study.kind.spec();
                let items: Vec<Item> = watchable()
                    .into_iter()
                    .map(|kind| {
                        let this = kind_this.clone();
                        Entry::new(kind.spec().label)
                            .checked(kind == study.kind)
                            .on_click(move |_, cx| {
                                this.update(cx, |e, cx| {
                                    e.draft.source = Source::Indicator {
                                        study: Box::new(StudyConfig::new(kind)),
                                        plot: 0,
                                    };
                                    e.mend();
                                    cx.notify();
                                });
                            })
                            .into()
                    })
                    .collect();
                watch.push(form::field(
                    "Indicator",
                    None,
                    self.select("alert-indicator", spec.label.to_owned(), items, window, cx),
                ));
                if spec.plots.len() > 1 {
                    let labels: Vec<&str> = spec.plots.iter().take(4).map(|p| p.label).collect();
                    let plot_this = cx.entity();
                    watch.push(form::field(
                        "Line",
                        None,
                        controls::segmented(
                            "alert-plot",
                            &labels,
                            plot.min(labels.len() - 1),
                            move |choice, _window, cx| {
                                plot_this.update(cx, |e, cx| {
                                    if let Source::Indicator { plot, .. } = &mut e.draft.source {
                                        *plot = choice;
                                    }
                                    cx.notify();
                                });
                            },
                        ),
                    ));
                }
                if spec.inputs.iter().any(|i| i.key == "length") {
                    watch.push(form::field(
                        "Period",
                        None,
                        number::field(&self.period, tokens::field::narrow()),
                    ));
                }
            }
            Source::Price { price } => {
                let labels: Vec<&str> = PriceKind::ALL.iter().map(|p| p.label()).collect();
                let price_this = cx.entity();
                watch.push(form::field(
                    "Price",
                    None,
                    controls::segmented(
                        "alert-price-kind",
                        &labels,
                        PriceKind::ALL.iter().position(|p| *p == price).unwrap_or(0),
                        move |choice, _window, cx| {
                            price_this.update(cx, |e, cx| {
                                e.draft.source = Source::Price {
                                    price: PriceKind::ALL[choice],
                                };
                                cx.notify();
                            });
                        },
                    ),
                ));
            }
            Source::Drawing { .. } => {}
        }

        // The condition and what it is compared with.
        let allowed = self.conditions();
        let cond_this = cx.entity();
        let cond_items: Vec<Item> = allowed
            .iter()
            .map(|c| {
                let (this, c) = (cond_this.clone(), *c);
                Entry::new(c.label())
                    .checked(c == condition)
                    .on_click(move |_, cx| {
                        this.update(cx, |e, cx| {
                            e.draft.condition = c;
                            cx.notify();
                        });
                    })
                    .into()
            })
            .collect();
        let mut when = vec![form::field(
            "Fires when it",
            None,
            self.select(
                "alert-condition",
                condition.label().to_owned(),
                cond_items,
                window,
                cx,
            ),
        )];
        if condition.needs_level() {
            if let Some(id) = drawing {
                let level_this = cx.entity();
                when.push(form::field(
                    "Compared with",
                    Some("A drawing of this symbol, followed as it is moved"),
                    button::action(
                        "alert-use-level",
                        "Use a level instead",
                        None,
                        false,
                        move |_window, cx| {
                            level_this.update(cx, |e, cx| {
                                e.draft.versus = None;
                                e.mend();
                                cx.notify();
                            });
                        },
                    ),
                ));
                let _ = id;
            } else {
                when.push(form::field(
                    if condition.is_zone() {
                        "Zone from"
                    } else {
                        "Level"
                    },
                    None,
                    form::text_field(&self.price, tokens::field::text()),
                ));
                if condition.is_zone() {
                    when.push(form::field(
                        "Zone to",
                        None,
                        form::text_field(&self.upper, tokens::field::text()),
                    ));
                }
            }
        }
        if condition == Condition::MovesBy {
            when.push(form::field(
                "Move (percent)",
                None,
                number::field(&self.amount, tokens::field::narrow()),
            ));
            when.push(form::field(
                "Within (minutes)",
                None,
                number::field(&self.minutes, tokens::field::narrow()),
            ));
        }

        // How often, how long, and on which bars.
        let trigger = self.draft.trigger;
        let trig_this = cx.entity();
        let trig_items: Vec<Item> = Trigger::ALL
            .iter()
            .filter(|t| !(is_indicator && **t == Trigger::OncePerBar))
            .map(|t| {
                let (this, t) = (trig_this.clone(), *t);
                Entry::new(t.label())
                    .checked(t == trigger)
                    .on_click(move |_, cx| {
                        this.update(cx, |e, cx| {
                            e.draft.trigger = t;
                            cx.notify();
                        });
                    })
                    .into()
            })
            .collect();
        let mut often = vec![form::field(
            "How often",
            Some("Once per bar close judges the closed bars only"),
            self.select(
                "alert-trigger",
                trigger.label().to_owned(),
                trig_items,
                window,
                cx,
            ),
        )];
        if self.draft.needs_bars() || trigger == Trigger::OncePerBar {
            let tf_this = cx.entity();
            let current = self.draft.timeframe.clone();
            let frames: Vec<wyck_chart::Timeframe> = wyck_chart::QUICK
                .into_iter()
                .filter(|t| t.bar_ms().is_some())
                .collect();
            let tf_items: Vec<Item> = frames
                .iter()
                .map(|t| {
                    let (this, t) = (tf_this.clone(), *t);
                    Entry::new(t.label())
                        .checked(t.code() == current)
                        .on_click(move |_, cx| {
                            this.update(cx, |e, cx| {
                                e.draft.timeframe = t.code();
                                cx.notify();
                            });
                        })
                        .into()
                })
                .collect();
            let text =
                wyck_chart::Timeframe::from_code(&current).map_or(current.clone(), |t| t.label());
            often.push(form::field(
                "Timeframe",
                Some("The bars it is judged on"),
                self.select("alert-timeframe", text, tf_items, window, cx),
            ));
        }
        let sticky_this = cx.entity();
        often.push(form::field(
            "Expires in (hours)",
            Some("It stops watching by itself. Empty is never"),
            number::field(&self.expiry, tokens::field::narrow()),
        ));
        often.push(form::field(
            "Keep the notice on screen",
            Some("Until you close it"),
            controls::toggle("alert-sticky", self.draft.sticky, move |on, _window, cx| {
                sticky_this.update(cx, |e, cx| {
                    e.draft.sticky = on;
                    cx.notify();
                });
            }),
        ));

        let message = vec![
            form::block(Input::new(&self.message).small()),
            form::block(Input::new(&self.tag).small()),
        ];

        // What it did before.
        let digits = self.digits;
        let history: Vec<gpui::AnyElement> = self
            .alerts
            .read(cx)
            .book()
            .firings(self.id)
            .take(5)
            .map(|f| {
                form::block(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .text_size(px(tokens::text::small()))
                        .text_color(theme::muted_fg())
                        .child(wyck_chart::Zone::Local.format(f.at, "%Y-%m-%d %H:%M"))
                        .child(div().text_color(theme::fg()).child(f.text.clone())),
                )
            })
            .collect();
        let _ = digits;

        let mut body = form::page()
            .child(form::group(IconName::Eye, "Watch", watch))
            .child(form::group(IconName::Target, "Condition", when))
            .child(form::group(IconName::Timer, "Trigger", often))
            .child(form::group(IconName::MessageSquare, "Message", message));
        if !history.is_empty() {
            body = body.child(form::group(IconName::Info, "Last times it fired", history));
        }
        let body = body.child(form::note(
            "The line of a price alert can also be dragged on the chart. An alert on a drawing follows it when it moves.",
        ));
        let footer = form::footer(
            Vec::new(),
            vec![
                button::action("alert-cancel", "Cancel", None, false, modal::close)
                    .into_any_element(),
                button::action("alert-save", "Save", None, true, move |window, cx| {
                    save.update(cx, |editor, cx| editor.save(cx));
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        form::dialog(head, modal::dismiss, body, footer)
    }
}
