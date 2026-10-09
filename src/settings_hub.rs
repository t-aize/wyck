//! The settings of the whole app, opened from the account menu: how it looks, the colors of the
//! charts, how the tools behave, and the backup of everything the user made.
//!
//! Every change applies at once and is saved as it is made, so there is nothing to confirm: the
//! app behind the panel shows the result live.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px, rgb};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Disableable, Sizable};

use super::appearance::presets::CANDLE_SETS;
use super::appearance::{self, ColorField, Mode, contrast};
use super::build_info::{BuildMode, VERSION};
use super::indicators::{self, prefs};
use super::multichart::MultiChart;
use super::updates;
use super::workspace::{MAX_SAVED_ALERTS, UsageLimits, Workspace};
use crate::chart_core::drawing::model::MAX_DRAWINGS_PER_SYMBOL;
use crate::chart_core::settings::MAX_STUDIES;
use wyck_config::backup::{self, BackupEntry};
use crate::ui::kit::{
    button, confirm, controls,
    font_picker::{FontChosen, FontPicker},
    form,
    form::Head,
    icon, modal, number, theme,
    theme::Colors,
    toast, tokens,
};

mod about;
mod alerts;
mod behaviour;
mod charts;
mod data;
mod look;
mod safety;
mod scripts;

/// Opens the settings.
pub fn open(
    workspace: Entity<Workspace>,
    multi: Entity<MultiChart>,
    window: &mut Window,
    cx: &mut App,
) {
    open_at(workspace, multi, Page::Appearance, window, cx);
}

/// Opens the settings on a page.
pub fn open_at(
    workspace: Entity<Workspace>,
    multi: Entity<MultiChart>,
    page: Page,
    window: &mut Window,
    cx: &mut App,
) {
    // Opened once whatever asked is done updating.
    window.defer(cx, move |window, cx| {
        let hub = cx.new(|cx| SettingsHub::new(workspace, multi, page, window, cx));
        modal::open(
            hub,
            modal::Options::new(920.0, 700.0).label("Settings"),
            window,
            cx,
        );
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Appearance,
    Charts,
    Indicators,
    Behaviour,
    Alerts,
    Safety,
    Data,
    About,
}

impl Page {
    const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Charts,
        Self::Indicators,
        Self::Behaviour,
        Self::Alerts,
        Self::Safety,
        Self::Data,
        Self::About,
    ];

    fn tab(self) -> form::Tab {
        let (label, icon) = match self {
            Self::Appearance => ("Appearance", IconName::Palette),
            Self::Charts => ("Charts", IconName::ChartCandlestick),
            Self::Indicators => ("Indicators", IconName::CodeXml),
            Self::Behaviour => ("Behavior", IconName::SlidersHorizontal),
            Self::Alerts => ("Alerts", IconName::Bell),
            Self::Safety => ("Safety", IconName::ShieldCheck),
            Self::Data => ("Data and backup", IconName::Database),
            Self::About => ("About", IconName::Info),
        };
        form::Tab { label, icon }
    }
}

/// Which color panel is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    Accent,
    CandleUp,
    CandleDown,
    ChartLine,
    ChartBackground,
    Theme(ColorField),
}

#[derive(Clone, Copy)]
enum UsageLimitKind {
    Indicators,
    Alerts,
    Drawings,
}

/// The colors offered for the accent.
const ACCENTS: [u32; 10] = [
    0x7c86ff, 0x3b82f6, 0x06b6d4, 0x10b981, 0x84cc16, 0xeab308, 0xf97316, 0xef4444, 0xec4899,
    0xa855f7,
];

/// What a backup step last said.
struct Notice {
    ok: bool,
    text: String,
}

struct SettingsHub {
    workspace: Entity<Workspace>,
    multi: Entity<MultiChart>,
    page: Page,
    pick: Option<Pick>,
    /// The theme of the user's whose colors are being edited.
    editing: Option<String>,
    new_theme: Entity<InputState>,
    rename: Entity<InputState>,
    study_limit: Entity<InputState>,
    alert_limit: Entity<InputState>,
    drawing_limit: Entity<InputState>,
    /// The number fields of the Safety page.
    risk_inputs: Vec<(safety::RiskField, Entity<InputState>)>,
    /// The list of fonts, once it was opened.
    font_picker: Option<Entity<FontPicker>>,
    font_open: bool,
    _font_subscription: Option<Subscription>,
    notice: Option<Notice>,
    /// What the backup waiting to be applied holds, once one was chosen.
    waiting: Option<Vec<String>>,
    /// The copies kept in the backups folder, newest first, as last listed.
    copies: Vec<BackupEntry>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsHub {
    fn new(
        workspace: Entity<Workspace>,
        multi: Entity<MultiChart>,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let limits = workspace.read(cx).preferences().limits;
        let new_theme =
            cx.new(|cx| InputState::new(window, cx).placeholder("Name of the new theme"));
        let rename = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let study_limit = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                limits.studies_per_chart as f64,
                window,
                cx,
            )
            .min(1.0)
            .max(MAX_STUDIES as f64)
        });
        let alert_limit = cx.new(|cx| {
            number::state(number::Kind::Count, limits.alerts as f64, window, cx)
                .min(1.0)
                .max(MAX_SAVED_ALERTS as f64)
        });
        let drawing_limit = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                limits.drawings_per_symbol as f64,
                window,
                cx,
            )
            .min(1.0)
            .max(MAX_DRAWINGS_PER_SYMBOL as f64)
        });
        let mut subscriptions = vec![
            cx.observe(&workspace, |_this, _workspace, cx| cx.notify()),
            indicators::observe(cx, |_this: &mut Self, cx| cx.notify()),
            updates::observe(cx, |_this: &mut Self, cx| cx.notify()),
            cx.subscribe(&rename, |this, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur)
                    && let Some(id) = this.editing.clone()
                {
                    let name = state.read(cx).value().to_string();
                    appearance::update(cx, |a| {
                        a.rename_theme(&id, &name);
                    });
                }
            }),
        ];
        subscriptions.push(number::watch(&study_limit, cx, |this, value, cx| {
            this.set_usage_limit(UsageLimitKind::Indicators, value, cx);
        }));
        subscriptions.push(number::watch(&alert_limit, cx, |this, value, cx| {
            this.set_usage_limit(UsageLimitKind::Alerts, value, cx);
        }));
        subscriptions.push(number::watch(&drawing_limit, cx, |this, value, cx| {
            this.set_usage_limit(UsageLimitKind::Drawings, value, cx);
        }));
        let risk = workspace.read(cx).preferences().risk.clone();
        let mut risk_inputs = Vec::new();
        for field in safety::RiskField::ALL {
            let state = cx.new(|cx| number::state(field.kind(), field.value(&risk), window, cx));
            subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
                this.set_risk_field(field, value, cx);
            }));
            risk_inputs.push((field, state));
        }
        let waiting = cx
            .try_global::<PendingSummary>()
            .map(|p| p.0.clone())
            .filter(|_| crate::app_paths().is_some_and(backup::import_pending));
        Self {
            workspace,
            multi,
            page,
            pick: None,
            editing: None,
            new_theme,
            rename,
            study_limit,
            alert_limit,
            drawing_limit,
            risk_inputs,
            font_picker: None,
            font_open: false,
            _font_subscription: None,
            notice: None,
            waiting,
            copies: data::list_copies(),
            _subscriptions: subscriptions,
        }
    }

    fn set_usage_limit(&self, kind: UsageLimitKind, value: f64, cx: &mut Context<Self>) {
        let mut limits: UsageLimits = self.workspace.read(cx).preferences().limits;
        match kind {
            UsageLimitKind::Indicators => limits.studies_per_chart = value as usize,
            UsageLimitKind::Alerts => limits.alerts = value as usize,
            UsageLimitKind::Drawings => limits.drawings_per_symbol = value as usize,
        }
        let limits = limits.normalized();
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_preferences(cx, |prefs| prefs.limits = limits);
        });
        self.multi
            .update(cx, |multi, cx| multi.set_usage_limits(limits, cx));
    }

    fn toggle_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        self.pick = if self.pick == Some(pick) {
            None
        } else {
            Some(pick)
        };
        cx.notify();
    }

    /// A color swatch for a setting the look holds, with the panel that opens under it.
    fn swatch(
        &self,
        pick: Pick,
        color: u32,
        id: &str,
        cx: &mut Context<Self>,
        set: impl Fn(&mut appearance::Appearance, u32) + Clone + 'static,
    ) -> AnyElement {
        let (toggle, choose) = (cx.entity(), cx.entity());
        let _ = &choose;
        controls::color_swatch(
            SharedString::from(id.to_owned()),
            color,
            self.pick == Some(pick),
            cx,
            move |_window, cx| toggle.update(cx, |e, cx| e.toggle_pick(pick, cx)),
            move |color, _window, cx| {
                let set = set.clone();
                appearance::update(cx, |a| set(a, color));
            },
        )
    }

    // ---- appearance ----

    // ---- charts ----

    // ---- behavior ----

    // ---- data ----

    fn say(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(Notice {
            ok,
            text: text.into(),
        });
        cx.notify();
    }

    // ---- about ----

    // ---- indicators ----
}

/// What the backup that waits holds, kept for as long as the app runs so that the panel can show
/// it again after being closed.
struct PendingSummary(Vec<String>);

impl gpui::Global for PendingSummary {}

impl Render for SettingsHub {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs: Vec<form::Tab> = Page::ALL.iter().map(|p| p.tab()).collect();
        let active = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let this = cx.entity();
        let body = match self.page {
            Page::Appearance => self.appearance_page(cx),
            Page::Charts => self.charts_page(cx),
            Page::Indicators => self.indicators_page(cx),
            Page::Behaviour => self.behaviour_page(cx),
            Page::Alerts => self.alerts_page(cx),
            Page::Safety => self.safety_page(cx),
            Page::Data => self.data_page(cx),
            Page::About => self.about_page(cx),
        };
        let head = Head {
            icon: IconName::Settings,
            title: "Settings".into(),
            subtitle: "Look, charts, behavior, and the backup of everything you made".into(),
        };
        let footer = form::footer(
            vec![],
            vec![
                button::action("settings-hub-close", "Close", None, true, |window, cx| {
                    modal::close(window, cx);
                })
                .into_any_element(),
            ],
        );
        form::frame(
            head,
            &tabs,
            active,
            move |index, _window, cx| {
                let page = Page::ALL[index];
                this.update(cx, |e, cx| {
                    e.page = page;
                    e.pick = None;
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
        .into_any_element()
    }
}
