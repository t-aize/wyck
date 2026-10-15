//! How the alert editor is drawn.

use super::alert_editor::{AlertEditor, watchable};
use gpui::prelude::*;
use gpui::{Context, Window, div, px};
use wyck_app::alerts::model::source::{PnlScope, PriceKind};
use wyck_app::alerts::sound::SoundKind;
use wyck_app::alerts::{Condition, Source, Trigger};
use wyck_indicators::config::StudyConfig;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::menu::{Entry, Item};
use wyck_ui_kit::{button, controls, form, form::Head, modal, number, theme, tokens};

impl Render for AlertEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let symbol = if self.draft.source.has_symbol() {
            self.draft.symbol.clone()
        } else {
            "the account".to_owned()
        };
        let is_indicator = self.draft.source.is_indicator();
        let measure = self.draft.source.is_measure();
        let source_index = match &self.draft.source {
            Source::Indicator { .. } => 1,
            Source::Spread { .. } => 2,
            Source::Pnl { .. } => 3,
            _ => 0,
        };
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
                &["Price", "Indicator", "Spread", "Profit"],
                source_index,
                move |choice, window, cx| {
                    source_this.update(cx, |e, cx| {
                        e.choose_source(choice, window, cx);
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
            Source::Spread { .. } => {}
            Source::Pnl { scope } => {
                // The choices: the whole account, the symbol, and the one position this alert
                // was made for (it cannot be chosen here, only kept).
                let mut labels = vec!["Account"];
                if self.draft.symbol_id > 0 {
                    labels.push("This symbol");
                }
                let position = format!("Position {}", self.id_of_position());
                if matches!(scope, PnlScope::Position { .. }) {
                    labels.push(&position);
                }
                let at = match scope {
                    PnlScope::Account => 0,
                    PnlScope::Symbol => 1,
                    PnlScope::Position { .. } => labels.len() - 1,
                };
                let scope_this = cx.entity();
                watch.push(form::field(
                    "Profit of",
                    Some("In the money of the account, net of swap and commission"),
                    controls::segmented("alert-scope", &labels, at, move |choice, _w, cx| {
                        scope_this.update(cx, |e, cx| {
                            if let Source::Pnl { scope } = &mut e.draft.source {
                                match choice {
                                    0 => *scope = PnlScope::Account,
                                    1 => *scope = PnlScope::Symbol,
                                    _ => {}
                                }
                            }
                            cx.notify();
                        });
                    }),
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
                let unit = match &self.draft.source {
                    Source::Spread { .. } => Some("In pips"),
                    Source::Pnl { .. } => Some("In the money of the account. A loss is negative"),
                    _ => None,
                };
                when.push(form::field(
                    if condition.is_zone() {
                        "Zone from"
                    } else {
                        "Level"
                    },
                    unit,
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
            .filter(|t| !(measure && **t == Trigger::OncePerBarClose))
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
            let frames: Vec<wyck_market::Timeframe> = wyck_market::QUICK
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
                wyck_market::Timeframe::from_code(&current).map_or(current.clone(), |t| t.label());
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

        // "Default" is the sound chosen in the settings, then one entry for each sound.
        let sound_this = cx.entity();
        let mut sound_labels = vec!["Default"];
        sound_labels.extend(SoundKind::ALL.iter().map(|k| k.label()));
        let sound_at = self
            .draft
            .sound
            .and_then(|own| SoundKind::ALL.iter().position(|k| *k == own))
            .map_or(0, |i| i + 1);
        often.push(form::field_below(
            "Sound",
            Some("Default is the one of the settings"),
            controls::chips(
                "alert-sound",
                &sound_labels,
                &[sound_at],
                move |index, _w, cx| {
                    sound_this.update(cx, |e, cx| {
                        e.draft.sound = index.checked_sub(1).map(|i| SoundKind::ALL[i]);
                        cx.notify();
                    });
                },
            ),
        ));

        let message = vec![
            form::block(wyck_ui_kit::input::text(&self.message)),
            form::block(wyck_ui_kit::input::text(&self.tag)),
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
