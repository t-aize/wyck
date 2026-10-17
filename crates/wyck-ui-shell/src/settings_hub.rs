//! The settings of the whole app, opened from the account menu: how it looks, the colors of the
//! charts, how the tools behave, and the backup of everything the user made.
//!
//! Every change applies at once and is saved as it is made, so there is nothing to confirm: the
//! app behind the panel shows the result live.

use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, SharedString, Subscription, Window, div, px, rgb};
use wyck_ui_kit::icon::IconName;
use wyck_ui_kit::input::InputState;

use wyck_app::appearance;
use wyck_app::appearance::candles::CANDLE_SETS;
use wyck_app::prefs::indicators as prefs;
use wyck_app::scripts as indicators;
use wyck_app::storage::backup;
use wyck_app::storage::backup::store::BackupEntry;
use wyck_app::system::build_info::{BuildMode, VERSION};
use wyck_app::updates;
use wyck_app::alerts::MAX_SAVED_ALERTS;
use wyck_app::workspace::{UsageLimits, Workspace};
use wyck_chart::MAX_STUDIES;
use wyck_core::math::{count_f64, f64_to_usize};
use wyck_drawings::model::MAX_DRAWINGS_PER_SYMBOL;
use wyck_ui_chart::multichart::MultiChart;
use wyck_ui_kit::{
    button, confirm, controls,
    font_picker::{FontChosen, FontPicker},
    form,
    form::Head,
    icon, modal, number, theme, toast, tokens,
};

mod about;

/// Tells the user an update can be installed from the settings.
pub fn announce_update(version: &str, cx: &mut App) {
    toast::show(
        cx,
        toast::Kind::Info,
        "Wyck update available",
        format!("Version {version} can be installed from Settings > About."),
    );
}

/// Tells the user what the start found or did about the folders of their data: a move that was
/// made or failed, a chosen folder that could not be reached, copies made by a sync service.
pub fn announce_folders(cx: &mut App) {
    use wyck_ui_kit::toast::{Kind, show};
    for note in wyck_app::locations::startup_notes() {
        let kind = if note.warning {
            Kind::Warning
        } else {
            Kind::Info
        };
        show(cx, kind, note.title, note.text);
    }
}

mod alerts;
mod behaviour;
mod charts;
mod data;
mod folders;
mod look;
mod safety;
mod scripts;
mod shortcuts;

/// Opens the settings.
pub(crate) fn open(
    workspace: Entity<Workspace>,
    multi: Entity<MultiChart>,
    window: &mut Window,
    cx: &mut App,
) {
    open_at(workspace, multi, Page::Interface, window, cx);
}

/// Opens the settings on a page.
pub(crate) fn open_at(
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
pub(crate) enum Page {
    Interface,
    Charts,
    Indicators,
    Behaviour,
    Shortcuts,
    Alerts,
    Safety,
    Folders,
    Data,
    About,
}

impl Page {
    const ALL: [Self; 10] = [
        Self::Interface,
        Self::Charts,
        Self::Indicators,
        Self::Behaviour,
        Self::Shortcuts,
        Self::Alerts,
        Self::Safety,
        Self::Folders,
        Self::Data,
        Self::About,
    ];

    fn tab(self) -> form::Tab {
        let (label, icon) = match self {
            Self::Interface => ("Interface", IconName::Settings2),
            Self::Charts => ("Charts", IconName::ChartCandlestick),
            Self::Indicators => ("Indicators", IconName::CodeXml),
            Self::Behaviour => ("Behavior", IconName::SlidersHorizontal),
            Self::Shortcuts => ("Shortcuts", IconName::Keyboard),
            Self::Alerts => ("Alerts", IconName::Bell),
            Self::Safety => ("Safety", IconName::ShieldCheck),
            Self::Folders => ("Folders", IconName::FolderOpen),
            Self::Data => ("Data and backup", IconName::Database),
            Self::About => ("About", IconName::Info),
        };
        form::Tab { label, icon }
    }
}

/// Which color panel is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    CandleUp,
    CandleDown,
    ChartLine,
    ChartBackground,
}

#[derive(Clone, Copy)]
enum UsageLimitKind {
    Indicators,
    Alerts,
    Drawings,
}

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
    study_limit: Entity<InputState>,
    alert_limit: Entity<InputState>,
    drawing_limit: Entity<InputState>,
    /// The number fields of the Safety page.
    risk_inputs: Vec<(safety::RiskField, Entity<InputState>)>,
    /// The list of fonts, once it was opened.
    font_picker: Option<Entity<FontPicker>>,
    font_open: bool,
    font_subscription: Option<Subscription>,
    notice: Option<Notice>,
    /// The shortcut waiting for its new keys, and the last thing said about one.
    capturing: Option<shortcuts::Capture>,
    shortcut_note: Option<(String, String)>,
    shortcut_search: Entity<InputState>,
    capture_focus: gpui::FocusHandle,
    /// A combination waiting to be taken from the action that has it.
    swap: Option<shortcuts::Swap>,
    /// Whether the keys of the widget library are listed.
    library_open: bool,
    /// What the backup waiting to be applied holds, once one was chosen.
    waiting: Option<Vec<String>>,
    /// The copies kept in the backups folder, newest first, as last listed.
    copies: Vec<BackupEntry>,
    /// Where each kind of data is kept, as last read (see the Folders page).
    folders: Option<wyck_app::locations::Overview>,
    /// What a move of folders that waits for the next start will do.
    folders_wait: Option<Vec<String>>,
    /// What the last step of the Folders page said.
    folders_notice: Option<Notice>,
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
        let study_limit = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                count_f64(limits.studies_per_chart),
                window,
                cx,
            )
            .min(1.0)
            .max(count_f64(MAX_STUDIES))
        });
        let alert_limit = cx.new(|cx| {
            number::state(number::Kind::Count, count_f64(limits.alerts), window, cx)
                .min(1.0)
                .max(count_f64(MAX_SAVED_ALERTS))
        });
        let drawing_limit = cx.new(|cx| {
            number::state(
                number::Kind::Count,
                count_f64(limits.drawings_per_symbol),
                window,
                cx,
            )
            .min(1.0)
            .max(count_f64(MAX_DRAWINGS_PER_SYMBOL))
        });
        let mut subscriptions = vec![
            cx.observe(&workspace, |_this, _workspace, cx| cx.notify()),
            indicators::observe(cx, |_this: &mut Self, cx| cx.notify()),
            updates::observe(cx, |_this: &mut Self, cx| cx.notify()),
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
        let folders = wyck_app::locations::overview();
        let folders_wait = folders.as_ref().and_then(|o| o.waiting.clone());
        let shortcut_search = Self::new_shortcut_search(window, cx);
        subscriptions.push(cx.observe(&shortcut_search, |_this, _search, cx| cx.notify()));
        let waiting = cx
            .try_global::<PendingSummary>()
            .map(|p| p.0.clone())
            .filter(|_| wyck_app::app_paths().is_some_and(backup::staged::import_pending));
        Self {
            workspace,
            multi,
            page,
            pick: None,
            study_limit,
            alert_limit,
            drawing_limit,
            risk_inputs,
            font_picker: None,
            font_open: false,
            font_subscription: None,
            notice: None,
            capturing: None,
            shortcut_note: None,
            shortcut_search,
            capture_focus: cx.focus_handle(),
            swap: None,
            library_open: false,
            waiting,
            copies: data::list_copies(),
            folders,
            folders_wait,
            folders_notice: None,
            _subscriptions: subscriptions,
        }
    }

    fn set_usage_limit(&self, kind: UsageLimitKind, value: f64, cx: &mut Context<Self>) {
        let mut limits: UsageLimits = self.workspace.read(cx).preferences().limits;
        match kind {
            UsageLimitKind::Indicators => limits.studies_per_chart = f64_to_usize(value),
            UsageLimitKind::Alerts => limits.alerts = f64_to_usize(value),
            UsageLimitKind::Drawings => limits.drawings_per_symbol = f64_to_usize(value),
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
        let toggle = cx.entity();
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

    fn say(&mut self, ok: bool, text: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = Some(Notice {
            ok,
            text: text.into(),
        });
        cx.notify();
    }
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
            Page::Interface => self.interface_page(cx),
            Page::Charts => self.charts_page(cx),
            Page::Indicators => Self::indicators_page(cx),
            Page::Behaviour => self.behaviour_page(cx),
            Page::Shortcuts => self.shortcuts_page(cx),
            Page::Alerts => self.alerts_page(cx),
            Page::Safety => self.safety_page(cx),
            Page::Folders => self.folders_page(cx),
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
                let Some(&page) = Page::ALL.get(index) else {
                    return;
                };
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
