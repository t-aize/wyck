//! The GPUI shell: the window and the plumbing around it.
//!
//! [`run`] opens the main window without a native title bar (the view draws its own, see
//! [`crate::ui::titlebar`]), applies the theme and the fonts, follows the engine's state into an
//! [`AppModel`], registers the global shortcuts and routes them to the [`AppController`], and
//! quits when the main window closes. What the window shows is the view built by the function
//! passed to [`run`], given a [`Shell`] and the connection to open at startup: the application
//! passes [`AppView`](crate::ui::AppView).
//!
//! ```ignore
//! wyck_app::shell::run(args, Hooks::default(), |shell, connection, window, cx| {
//!     cx.new(|cx| AppView::new(shell, connection, None, window, cx))
//! });
//! ```
//!
//! Nothing here decides anything: the rules live in [`crate::controller`], [`crate::flow`],
//! [`crate::presentation`] and [`crate::messages`], which are tested without a window.

use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::Root;
use gpui_kit::{App, AppContext as _, Entity, Render, Window};
use wyck_engine::broker::ConnectRequest;
use wyck_engine::domain::{Side, now_millis};

use crate::controller::AppController;
use crate::hotkeys::{Debounce, HotkeyAction, Hotkeys, forward_presses, parse_bindings};
use crate::messages::Notice;
use crate::model::AppModel;
use crate::presentation::session_badge;
use crate::settings::AppSettings;
use crate::startup::StartupError;
use crate::ui::{assets, theme, titlebar};

/// Minimum time between two accepted presses of the same shortcut: drops key repeat.
const DEBOUNCE_MS: i64 = 400;

/// Everything [`run`] needs, prepared before any window exists.
pub struct AppArgs {
    /// The settings (the traded symbol, the shortcuts, the watch list).
    pub settings: AppSettings,
    /// The engine and its use cases.
    pub controller: Arc<AppController>,
    /// The connection to open, or why there is none.
    pub connection: Result<ConnectRequest, StartupError>,
    /// Notices that must stay visible from the start (a crashed previous session, say).
    pub banners: Vec<Notice>,
}

/// A callback that gets the shell and the app context.
pub type ShellCallback = Rc<dyn Fn(&Shell, &mut App)>;

/// Callbacks for what the shell cannot decide.
#[derive(Default)]
pub struct Hooks {
    /// Called when the "toggle the panel" shortcut is pressed. The shell has no panel of its own.
    pub on_toggle_panel: Option<ShellCallback>,
}

/// What a view holds on to: the shared data and the things a user can do.
#[derive(Clone)]
pub struct Shell {
    /// The data every window renders. Observe it to redraw when the engine's state changes.
    pub model: Entity<AppModel>,
    /// The engine and its use cases.
    pub controller: Arc<AppController>,
}

impl Shell {
    /// Plans and submits a hotkey order (a dry run) and records what came back as a notice.
    pub fn order(&self, side: Side, cx: &mut App) {
        let shell = self.clone();
        cx.spawn(async move |cx| {
            let notice = shell.controller.hotkey_order(side).await;
            tracing::info!(title = %notice.title, level = ?notice.level, "order request finished");
            let now = now_millis();
            shell.model.update(cx, |model, cx| {
                model.push_notice(notice, now);
                cx.notify();
            });
        })
        .detach();
    }

    /// Dismisses a warning, if the user is allowed to.
    pub fn dismiss(&self, id: &str, cx: &mut App) {
        if self.controller.dismiss_warning(id) {
            self.model.update(cx, |_, cx| cx.notify());
        }
    }
}

/// Runs the application until the main window is closed. `build_root` makes the content of the
/// main window; it is called once, when the window exists.
pub fn run<V: Render + 'static>(
    args: AppArgs,
    hooks: Hooks,
    build_root: impl FnOnce(
        Shell,
        Result<ConnectRequest, StartupError>,
        &mut Window,
        &mut App,
    ) -> Entity<V>
    + 'static,
) {
    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(move |cx| start(args, hooks, build_root, cx));
}

fn start<V: Render + 'static>(
    args: AppArgs,
    hooks: Hooks,
    build_root: impl FnOnce(
        Shell,
        Result<ConnectRequest, StartupError>,
        &mut Window,
        &mut App,
    ) -> Entity<V>
    + 'static,
    cx: &mut App,
) {
    gpui_kit::init(cx);
    // A trading screen is dark first. The fonts come before the theme that names them.
    assets::load_fonts(cx);
    theme::install(cx);
    // The system's "animation effects" setting was read by `gpui_kit::init`; an explicit choice wins.
    if let Some(reduce) = args.settings.reduce_motion {
        cx.set_reduce_motion(reduce);
    }

    let AppArgs {
        settings,
        controller,
        connection,
        mut banners,
    } = args;

    let model = cx.new(|_| AppModel::new(controller.handle().state(), Vec::new()));
    let shell = Shell {
        model: model.clone(),
        controller: Arc::clone(&controller),
    };

    // Global shortcuts. A problem here must not stop the app: it becomes a banner.
    let hotkeys = register_hotkeys(&settings, &mut banners);
    model.update(cx, |m, _| m.banners = banners);

    let opened = cx.open_window(titlebar::window_options(cx), {
        let shell = shell.clone();
        move |window, cx| {
            let view = build_root(shell, connection, window, cx);
            cx.new(|cx| Root::new(view, window, cx))
        }
    });
    let main_window = match opened {
        Ok(handle) => handle,
        Err(error) => {
            tracing::error!(%error, "could not open the main window");
            cx.quit();
            return;
        }
    };
    let main_id = main_window.window_id();
    cx.on_window_closed(move |cx, id| {
        if id == main_id {
            cx.quit();
        }
    })
    .detach();

    follow_engine(&shell, cx);
    if let Some(hotkeys) = hotkeys {
        route_hotkeys(shell, hooks, hotkeys, cx);
    }
}

/// Registers the shortcuts. Failures are appended to `banners` and logged; the app carries on.
fn register_hotkeys(settings: &AppSettings, banners: &mut Vec<Notice>) -> Option<Hotkeys> {
    let bindings = match parse_bindings(&settings.hotkeys) {
        Ok(b) => b,
        Err(error) => {
            tracing::warn!(%error, "shortcuts disabled");
            banners.push(Notice::warning(
                "Global shortcuts are off",
                error.to_string(),
            ));
            return None;
        }
    };
    let (hotkeys, failures) = match Hotkeys::register(&bindings, &settings.hotkeys) {
        Ok(ok) => ok,
        Err(error) => {
            tracing::warn!(%error, "shortcuts disabled");
            banners.push(Notice::warning(
                "Global shortcuts are off",
                error.to_string(),
            ));
            return None;
        }
    };
    for failure in &failures {
        tracing::warn!(error = %failure, "a shortcut could not be registered");
        banners.push(Notice::warning(
            "A shortcut could not be registered",
            failure.to_string(),
        ));
    }
    tracing::info!(registered = hotkeys.len(), "global shortcuts registered");
    Some(hotkeys)
}

/// Mirrors the engine's state into the model, on every change.
fn follow_engine(shell: &Shell, cx: &mut App) {
    let shell = shell.clone();
    let handle = shell.controller.handle();
    let mut states = handle.watch_state();
    cx.spawn(async move |cx| {
        let mut last_session = String::new();
        loop {
            // Clone out of the receiver before awaiting anything: never hold its lock across a suspension.
            let state = states.borrow_and_update().clone();
            let events = handle.recent_events();
            let session = session_badge(&state.session).text;
            if session != last_session {
                tracing::info!(%session, "session changed");
                last_session = session;
            }
            shell.model.update(cx, |model, cx| {
                model.apply(state, &events);
                cx.notify();
            });
            if states.changed().await.is_err() {
                break;
            }
        }
    })
    .detach();
}

/// Listens for shortcut presses for as long as the application runs.
fn route_hotkeys(shell: Shell, hooks: Hooks, hotkeys: Hotkeys, cx: &mut App) {
    let (tx, rx) = async_channel::unbounded::<u32>();
    // The handler runs inside the Windows message loop: it must never block.
    forward_presses(move |id| {
        let _ = tx.try_send(id);
    });
    let hotkeys = Rc::new(hotkeys);
    cx.spawn(async move |cx| {
        let mut debounce = Debounce::new(DEBOUNCE_MS);
        while let Ok(id) = rx.recv().await {
            let Some(action) = hotkeys.action_for(id) else {
                continue;
            };
            if !debounce.accept(action, now_millis()) {
                continue;
            }
            tracing::info!(action = action.label(), "shortcut pressed");
            let shell = shell.clone();
            let on_panel = hooks.on_toggle_panel.clone();
            cx.update(move |cx| match action {
                HotkeyAction::Buy => shell.order(Side::Buy, cx),
                HotkeyAction::Sell => shell.order(Side::Sell, cx),
                HotkeyAction::TogglePanel => {
                    if let Some(f) = on_panel {
                        f(&shell, cx);
                    }
                }
            });
        }
    })
    .detach();
}
