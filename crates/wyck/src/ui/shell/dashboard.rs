//! The dashboard: the screen the app lands on once an account is connected.
//!
//! That is the header bar (the symbol of the active chart, its live price, the timeframes, the
//! connection state, the account and the window controls), the symbol picker that opens from it,
//! and the charts.
//!
//! A [`Dashboard`] owns the [`Session`] for its account: the session reconnects, renews the
//! tokens and restores the price subscription by itself, and the dashboard only follows its events.
//! The two things the rest of the app has to act on come out as a [`DashboardEvent`].

mod catalog;
mod chart_buttons;
mod details;
mod editor_dock;
mod header;
mod layout_menu;
mod lists;
mod marks;
mod notice;
mod picker;
pub mod session;
mod trade;
pub mod view;

use self::catalog::{Catalog, Entry};
use self::picker::Picker;
use crate::ui::features::indicators::editor::IndicatorEditor;
use crate::ui::features::trading::panel::AccountPanel;
use crate::ui::features::trading::ticket::OrderTicket;
use gpui::prelude::*;
use gpui::{App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Window};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use wyck_app::account::{Account, AccountEvent};
use wyck_app::alerts::Alerts;
use wyck_app::broker::Error as ApiError;
use wyck_app::broker::session::Session;
use wyck_app::drawings::Drawings;
use wyck_app::market_data::live::LiveHub;
use wyck_app::workspace::{Documents, Workspace};
use wyck_drawings::model::tool::Tool;
use wyck_trading::guard::Mode;
use wyck_ui_chart::chart;
use wyck_ui_chart::multichart::{MultiChart, MultiChartEvent};
use wyck_ui_kit::toast;

gpui::actions!(
    wyck_dashboard,
    [
        OpenPicker,
        OpenSettings,
        ClosePicker,
        PickerUp,
        PickerDown,
        PickerPageUp,
        PickerPageDown,
        PickerConfirm,
        ToggleIndicatorEditor,
        CloseAllPositions,
        ToggleKillSwitch,
        TicketBuy,
        TicketSell,
        TicketSend,
        OpenShortcuts,
        ToggleTicket,
        ToggleAccountPanel,
        ToggleFullscreen,
        NextChart,
        PreviousChart,
        NextTimeframe,
        PreviousTimeframe,
        ToggleMagnet,
        ToggleKeepDrawing,
        ToggleFavoritesBar,
        CancelAllOrders,
        ToggleLogScale,
        TogglePercentScale,
        ToggleGrid,
    ]
);

/// Who the dashboard is connected as.
#[derive(Clone)]
pub struct AccountInfo {
    pub label: SharedString,
    pub is_live: bool,
}

/// What the rest of the app has to act on.
pub enum DashboardEvent {
    /// The user picked another symbol; remember it for the next start.
    SymbolChosen(String),
    /// The user asked to disconnect this account and forget its sign-in.
    Disconnect,
    /// The session ended for a reason only a new sign-in fixes.
    SignInAgain,
}

impl EventEmitter<DashboardEvent> for Dashboard {}

/// Where the connection stands, as the header shows it.
enum Conn {
    Connecting,
    Ready,
    Reconnecting,
    Failed(SharedString),
}

/// The list of symbols.
enum Load {
    Loading,
    Ready(Arc<Catalog>),
    Failed(SharedString),
}

/// The symbol the dashboard is on.
#[derive(Clone)]
struct Active {
    entry: Entry,
    /// Decimals the symbol is quoted with, and where its pip sits. Until the broker's details for
    /// the symbol arrive these are the common forex values.
    digits: u32,
    pip_position: i64,
}

#[derive(Default, Clone, Copy)]
struct Quote {
    bid: Option<i64>,
    ask: Option<i64>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tick {
    Up,
    Down,
}

/// The symbol the picker is showing, and the prices it has had since: the dashboard follows its
/// price for as long as the picker highlights it.
struct Peek {
    id: i64,
    quote: Quote,
}

/// The menus of the dashboard that open over it. One is open at a time, and a click anywhere else
/// closes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    /// The account: figures, settings, disconnect.
    Account,
    /// The list of all timeframes.
    Timeframes,
    /// The layout picker.
    Layouts,
}

impl Dashboard {
    fn menu_is(&self, kind: MenuKind) -> bool {
        self.open_menu == Some(kind)
    }

    /// Opens `kind`, or closes it when it is the one open.
    fn toggle_menu(&mut self, kind: MenuKind, cx: &mut Context<Self>) {
        self.open_menu = if self.menu_is(kind) { None } else { Some(kind) };
        cx.notify();
    }

    fn close_menus(&mut self) {
        self.open_menu = None;
    }
}

pub struct Dashboard {
    session: Session,
    /// Every price and live bar subscription goes through it.
    hub: Rc<LiveHub>,
    /// The chart the picker chooses a symbol for.
    picker_target: Option<usize>,
    /// The last bid and ask of every symbol seen.
    quotes: HashMap<i64, Quote>,
    account: AccountInfo,
    focus_handle: FocusHandle,
    focused: bool,
    conn: Conn,
    catalog: Load,
    catalog_requested: bool,
    /// The symbol to open on once the list is loaded: the one remembered from last time.
    initial_symbol: Option<String>,
    active: Option<Active>,
    quote: Quote,
    tick: Option<Tick>,
    picker: Option<Picker>,
    /// What the broker said about the symbols the picker highlighted, kept while the app runs.
    details: HashMap<i64, details::Detail>,
    peek: Option<Peek>,
    /// How many times the picker has been opened, so its entrance animation replays.
    picker_opens: u64,
    /// The menu that is open, if any: one at a time.
    open_menu: Option<MenuKind>,
    multi: Entity<MultiChart>,
    workspace: Entity<Workspace>,
    /// The field of the menu where a custom timeframe is typed, made with the window, and the
    /// unit a bare number is in.
    tf_custom: Option<Entity<wyck_ui_kit::input::InputState>>,
    tf_unit: chart::Unit,
    /// A chart asked for the picker: it opens at the next render, which has the window.
    pending_picker: bool,
    /// The trading account: positions, orders, balance.
    trading: Entity<Account>,
    alerts: Entity<Alerts>,
    /// The order ticket, made at the first render (its fields need the window).
    ticket: Option<Entity<OrderTicket>>,
    panel: Option<Entity<AccountPanel>>,
    ticket_open: bool,
    panel_open: bool,
    panel_height: f32,
    /// Where the pointer was while the panel's top edge is dragged.
    panel_drag: Option<f32>,
    /// The editor of the indicator scripts, made when it is first opened.
    editor: Option<Entity<IndicatorEditor>>,
    editor_open: bool,
    /// How much of the window the editor drawer covers, from the right edge.
    editor_share: f32,
    /// Where the pointer was while the left edge of the editor drawer is dragged.
    editor_drag: Option<f32>,
    /// The size of the window as last drawn, for the drawer that is a share of it.
    viewport_width: f32,
    viewport_height: f32,
    /// Where the pointer was, and how wide the ticket was, when its edge was grabbed.
    ticket_drag: Option<(f32, f32)>,
    /// What waits for the window.
    pending: Vec<trade::Pending>,
}

impl Dashboard {
    pub fn new(
        session: Session,
        account: AccountInfo,
        initial_symbol: Option<String>,
        documents: Documents,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = cx.new(|cx| Workspace::new(&documents, cx));
        // The colors saved in the color panel live with the preferences.
        wyck_ui_kit::color_picker::connect(
            workspace.read(cx).preferences().saved_colors.clone(),
            {
                let workspace = workspace.clone();
                move |colors, cx| {
                    workspace.update(cx, |workspace, cx| {
                        workspace
                            .edit_preferences(cx, |prefs| prefs.saved_colors = colors.to_vec());
                    });
                }
            },
            cx,
        );
        // The header shows the favorite timeframes, so it follows the workspace.
        cx.observe(&workspace, |this, _workspace, cx| {
            this.sync_risk(cx);
            cx.notify();
        })
        .detach();
        let drawings = cx.new(|cx| Drawings::new(documents.account.clone(), cx));
        let hub = Rc::new(LiveHub::new(session.clone()));
        let trading = cx.new(|cx| {
            Account::new(
                session.clone(),
                hub.clone(),
                Mode::from_live(account.is_live),
                cx,
            )
        });
        cx.subscribe(&trading, |_, account, event: &AccountEvent, cx| {
            if let AccountEvent::Notice(notice) = event {
                notice::show(&account, notice.clone(), cx);
            }
        })
        .detach();
        let alerts = cx.new(|cx| {
            Alerts::new(
                documents.account.clone(),
                hub.clone(),
                session.clone(),
                drawings.clone(),
                cx,
            )
        });
        cx.subscribe(&alerts, |this, _alerts, event, cx| {
            this.on_alert_event(event, cx)
        })
        .detach();
        let panel_prefs = workspace.read(cx).preferences().account_panel;
        let panel =
            cx.new(|cx| AccountPanel::new(trading.clone(), alerts.clone(), panel_prefs, cx));
        cx.subscribe(&panel, |this, _panel, event, cx| {
            this.on_panel_event(event, cx)
        })
        .detach();
        // The lines on the charts follow the account and the alerts.
        cx.observe(&trading, |this, _trading, cx| {
            this.push_lines(cx);
            this.feed_alerts(cx);
        })
        .detach();
        cx.observe(&alerts, |this, _alerts, cx| this.push_lines(cx))
            .detach();
        let prefs = workspace.read(cx).preferences().clone();
        // The keys the user chose, over the defaults bound at the start.
        crate::ui::shell::keymap::apply(&prefs.shortcuts, cx);
        let multi = cx.new(|cx| {
            MultiChart::new(
                session.clone(),
                hub.clone(),
                workspace.clone(),
                drawings,
                cx,
            )
        });
        cx.subscribe(&multi, |this, _multi, event: &MultiChartEvent, cx| {
            this.on_multi_event(event, cx);
        })
        .detach();
        let mut dashboard = Self {
            session,
            hub,
            picker_target: None,
            quotes: HashMap::new(),
            account,
            focus_handle: cx.focus_handle(),
            focused: false,
            conn: Conn::Connecting,
            catalog: Load::Loading,
            catalog_requested: false,
            initial_symbol,
            active: None,
            quote: Quote::default(),
            tick: None,
            picker: None,
            details: HashMap::new(),
            peek: None,
            picker_opens: 0,
            open_menu: None,
            multi,
            workspace,
            tf_custom: None,
            tf_unit: chart::Unit::Minutes,
            pending_picker: false,
            trading,
            alerts,
            ticket: None,
            panel: Some(panel),
            ticket_open: prefs.ticket_open,
            panel_open: prefs.panel_open,
            panel_height: prefs.panel_height,
            panel_drag: None,
            editor: None,
            editor_open: false,
            editor_share: editor_dock::SHARE_DEFAULT,
            editor_drag: None,
            viewport_width: 1400.0,
            viewport_height: 800.0,
            ticket_drag: None,
            pending: Vec::new(),
        };
        dashboard.follow_session(cx);
        dashboard
    }

    /// Runs `f` on the chart the user is working on.
    /// An arrow key with drawings selected on the charts moves them; otherwise it goes on.
    fn nudge(&mut self, window: &Window, bars: f64, steps: f64, cx: &mut Context<Self>) {
        let moved = self.multi.read(cx).has_chart_focus(window)
            && self
                .multi
                .update(cx, |multi, cx| multi.nudge_drawings(bars, steps, cx));
        if !moved {
            cx.propagate();
        }
    }

    /// The keys of the drawing tools: select all, the arrows, the finder and the tool letters.
    /// Kept out of `render`, whose frame grows with every handler chained onto it.
    #[inline(never)]
    fn drawing_actions(root: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        root.on_action(cx.listener(|this, _: &chart::FindTool, window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.open_tool_search(window, cx));
        }))
        .on_action(
            cx.listener(|this, _: &chart::SelectAllDrawings, window, cx| {
                if this.multi.read(cx).has_chart_focus(window) {
                    this.multi
                        .update(cx, |multi, cx| multi.select_all_drawings(cx));
                } else {
                    cx.propagate();
                }
            }),
        )
        .on_action(cx.listener(|this, _: &chart::NudgeUp, window, cx| {
            this.nudge(window, 0.0, 0.2, cx);
        }))
        .on_action(cx.listener(|this, _: &chart::NudgeDown, window, cx| {
            this.nudge(window, 0.0, -0.2, cx);
        }))
        .on_action(cx.listener(|this, _: &chart::NudgeBackFast, window, cx| {
            this.nudge(window, -10.0, 0.0, cx);
        }))
        .on_action(
            cx.listener(|this, _: &chart::NudgeForwardFast, window, cx| {
                this.nudge(window, 10.0, 0.0, cx);
            }),
        )
        .on_action(cx.listener(|this, _: &chart::NudgeUpFast, window, cx| {
            this.nudge(window, 0.0, 2.0, cx);
        }))
        .on_action(cx.listener(|this, _: &chart::NudgeDownFast, window, cx| {
            this.nudge(window, 0.0, -2.0, cx);
        }))
        .on_action(cx.listener(|this, _: &chart::ToolTrendLine, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_tool_key(Tool::TrendLine, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ToolHorizontal, _window, cx| {
            this.multi.update(cx, |multi, cx| {
                multi.pick_tool_key(Tool::HorizontalLine, cx)
            });
        }))
        .on_action(cx.listener(|this, _: &chart::ToolVertical, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_tool_key(Tool::VerticalLine, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ToolFib, _window, cx| {
            this.multi.update(cx, |multi, cx| {
                multi.pick_tool_key(Tool::FibRetracement, cx)
            });
        }))
        .on_action(cx.listener(|this, _: &chart::ToolRectangle, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_tool_key(Tool::Rectangle, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ToolText, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_tool_key(Tool::Text, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ToolMeasure, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_tool_key(Tool::Measure, cx));
        }))
        .on_action(cx.listener(|this, _: &chart::ToolBrush, _window, cx| {
            this.multi
                .update(cx, |multi, cx| multi.pick_tool_key(Tool::Brush, cx));
        }))
    }
}

/// Saves a picture of a chart in the pictures folder (under `Wyck`) and puts it on the
/// clipboard, then says where it went.
fn save_picture(png: Vec<u8>, name: String, cx: &mut Context<Dashboard>) {
    cx.write_to_clipboard(gpui::ClipboardItem::new_image(&gpui::Image::from_bytes(
        gpui::ImageFormat::Png,
        png.clone(),
    )));
    let folder = wyck_app::storage::AppPaths::pictures_dir().map(|dir| dir.join("Wyck"));
    let Some(folder) = folder else {
        toast::show(
            cx,
            toast::Kind::Success,
            "Picture copied",
            "The chart is on the clipboard.",
        );
        return;
    };
    cx.spawn(async move |_this, cx| {
        let path = folder.join(&name);
        let written = cx
            .background_executor()
            .spawn(async move {
                wyck_app::storage::atomic_write(&path, &png)?;
                Ok::<_, std::io::Error>(path)
            })
            .await;
        cx.update(|cx| match written {
            Ok(path) => toast::show(
                cx,
                toast::Kind::Success,
                "Picture saved and copied",
                path.display().to_string(),
            ),
            Err(error) => toast::show(
                cx,
                toast::Kind::Warning,
                "Picture copied, not saved",
                error.to_string(),
            ),
        });
    })
    .detach();
}

/// What to tell the user about a session that ended for good.
fn failure_message(error: &ApiError) -> String {
    format!("{error}. Sign in again to keep going.")
}

impl Focusable for Dashboard {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
