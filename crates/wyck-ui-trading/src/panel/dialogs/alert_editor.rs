//! The dialog that edits an alert: its source, its conditions and how it tells the user.

use gpui::prelude::*;
use gpui::{App, Context, Entity, Window, div, px};
use wyck_app::alerts::model::source::{PnlScope, PriceKind};
use wyck_app::alerts::{Alert, Alerts, Condition, Source, Trigger};
use wyck_indicators::config::StudyConfig;
use wyck_indicators::kind::StudyKind;
use wyck_ui_kit::focus::Keyboard;
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::input::InputState;
use wyck_ui_kit::menu::{self as popup, Item};
use wyck_ui_kit::{icon, modal, number, theme, tokens};

pub(super) struct AlertEditor {
    pub(super) alerts: Entity<Alerts>,
    pub(super) id: u64,
    pub(super) digits: u32,
    /// The alert as it is being edited: what it watches, its condition and trigger.
    pub(super) draft: Alert,
    pub(super) price: Entity<InputState>,
    pub(super) upper: Entity<InputState>,
    pub(super) amount: Entity<InputState>,
    pub(super) minutes: Entity<InputState>,
    /// How many hours until it stops watching; empty or 0 never.
    pub(super) expiry: Entity<InputState>,
    pub(super) period: Entity<InputState>,
    pub(super) message: Entity<InputState>,
    pub(super) tag: Entity<InputState>,
}

/// The indicators an alert can watch: the ones drawn from the bars, not scripts or profiles.
pub(super) fn watchable() -> Vec<StudyKind> {
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
    let measure = alert.source.is_measure();
    let decimals = alert.source.decimals(digits);
    let level = |value: f64| {
        if value == 0.0 && !measure {
            String::new()
        } else {
            format!("{:.*}", decimals as usize, value)
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
                .map(|at| ((at - wyck_app::market_data::now_ms()) as f64 / 3_600_000.0).max(0.0))
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
    modal::open(
        editor,
        modal::Options::new(520.0, 680.0).label("Edit the alert"),
        window,
        cx,
    );
}

impl AlertEditor {
    pub(super) fn read(&self, state: &Entity<InputState>, cx: &App) -> Option<f64> {
        number::parse(&state.read(cx).value())
    }

    /// The conditions that make sense for what is watched.
    pub(super) fn conditions(&self) -> Vec<Condition> {
        Condition::ALL
            .into_iter()
            .filter(|c| match (&self.draft.source, &self.draft.versus) {
                (source, _) if source.is_measure() => source.allows(*c),
                (Source::Indicator { .. }, _) => *c != Condition::MovesBy,
                (_, Some(Source::Drawing { .. })) => {
                    !matches!(c, Condition::MovesBy | Condition::ChangesDirection)
                }
                _ => *c != Condition::ChangesDirection,
            })
            .collect()
    }

    /// The position a profit alert on one position follows.
    pub(super) fn id_of_position(&self) -> i64 {
        match &self.draft.source {
            Source::Pnl {
                scope: PnlScope::Position { id },
            } => *id,
            _ => 0,
        }
    }

    /// The watch has changed to the choice at `choice` of the strip: price, indicator, spread or
    /// profit. The level is set to something that fits the new unit when the old one does not.
    pub(super) fn choose_source(
        &mut self,
        choice: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (source, level) = match choice {
            1 => (
                Source::Indicator {
                    study: Box::new(StudyConfig::new(StudyKind::Rsi)),
                    plot: 0,
                },
                Some(70.0),
            ),
            2 => {
                let pip = self.alerts.read(cx).pip_of(self.draft.symbol_id);
                (Source::Spread { pip }, Some(2.0))
            }
            3 => (
                Source::Pnl {
                    scope: if self.draft.symbol_id > 0 {
                        PnlScope::Symbol
                    } else {
                        PnlScope::Account
                    },
                },
                Some(0.0),
            ),
            _ => (
                Source::Price {
                    price: PriceKind::Bid,
                },
                None,
            ),
        };
        let was_price = matches!(self.draft.source, Source::Price { .. });
        self.draft.source = source;
        // A price level means nothing for another unit, and the other way round.
        if let Some(level) = level
            && (was_price || self.draft.price <= 1.0 || self.draft.source.is_measure())
        {
            self.draft.price = level;
            let text = number::format(level, 2);
            self.price.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        self.mend();
        cx.notify();
    }

    /// Puts the draft back in a shape that means something after what it watches changed.
    pub(super) fn mend(&mut self) {
        if !self.conditions().contains(&self.draft.condition) {
            self.draft.condition = Condition::Crossing;
        }
        if self.draft.source.is_indicator() && self.draft.trigger == Trigger::OncePerBar {
            self.draft.trigger = Trigger::EveryTime;
        }
        if self.draft.source.is_measure() {
            self.draft.versus = None;
            if self.draft.trigger == Trigger::OncePerBarClose {
                self.draft.trigger = Trigger::EveryTime;
            }
        }
    }

    pub(super) fn save(&self, cx: &mut App) {
        let mut alert = self.draft.clone();
        if let Some(price) = self.read(&self.price, cx)
            && (price > 0.0 || alert.source.is_indicator() || alert.source.is_measure())
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
        alert.expires_at =
            hours.map(|h| wyck_app::market_data::now_ms() + (h * 3_600_000.0) as i64);
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
    pub(super) fn select(
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
