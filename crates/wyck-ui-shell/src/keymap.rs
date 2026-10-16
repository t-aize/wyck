//! Every keyboard shortcut of the app: the ones the user can change, the ones that are fixed, and
//! the keys that screens handle by hand.
//!
//! The table [`DEFS`] is the one place where a binding is declared, whether it can be changed
//! ([`Kind::Custom`]) or not ([`Kind::Fixed`]: the keys of a text field, of a dialog, of the symbol
//! picker). [`HANDLED`] lists the keys that a screen reads itself, with no action behind them (the
//! arrows of a menu, of the rows of the account panel); `keymap_guard` fails when a file reads keys
//! by hand without being declared there. The settings page shows all three.
//!
//! What the user changed is kept in the preferences ([`ShortcutPrefs`]) and put over the defaults
//! by [`apply`], which binds the result in gpui: the bindings of this table are taken out of the
//! keymap and bound again, so the bindings of the widget library are left alone. The combinations
//! themselves, their rules and their clashes are in [`wyck_app::shortcuts`].

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{Action, App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, Keystroke};

use crate::dashboard as d;
use crate::sign_in_gate::Dismiss;
use wyck_app::shortcuts::{Chord, Resolved, ShortcutPrefs, canonical};
use wyck_ui_chart::chart as c;
use wyck_ui_editor as e;
use wyck_ui_kit::modal::CloseModal;
use wyck_ui_kit::text_input as t;

/// Where the actions of the dashboard work: the key context of the dashboard.
const DASHBOARD: &str = "Dashboard";
/// The key context of the symbol picker.
const PICKER: &str = "SymbolPicker";
/// The key context of a text field.
const TEXT: &str = "TextInput";

/// How the shortcuts are grouped on the page that edits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Navigation,
    Trading,
    Chart,
    Drawing,
    Editor,
    /// The keys of the text fields. Fixed.
    Text,
    /// Closing and choosing in dialogs, menus and the sign-in window. Fixed.
    Dialogs,
    /// The symbol picker. Fixed.
    Picker,
}

impl Group {
    pub const ALL: [Self; 8] = [
        Self::Navigation,
        Self::Trading,
        Self::Chart,
        Self::Drawing,
        Self::Editor,
        Self::Text,
        Self::Dialogs,
        Self::Picker,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Navigation => "Windows and navigation",
            Self::Trading => "Trading",
            Self::Chart => "Chart",
            Self::Drawing => "Drawings",
            Self::Editor => "Indicator editor",
            Self::Text => "Text fields",
            Self::Dialogs => "Dialogs, menus and sign-in",
            Self::Picker => "Symbol picker",
        }
    }
}

/// Whether the user can change the keys of an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Custom,
    Fixed,
}

/// One action a shortcut can run.
pub struct Def {
    /// Written in the preferences, so it never changes.
    pub id: &'static str,
    pub label: &'static str,
    pub group: Group,
    /// The key context the keys work in. Two actions only clash in one context.
    pub context: &'static str,
    /// The keys it has until the user changes them. May be none.
    pub keys: &'static [&'static str],
    pub kind: Kind,
    build: fn() -> Box<dyn Action>,
}

/// Keys a screen reads itself, with no action behind them. Only for the settings page to show.
pub struct Handled {
    pub label: &'static str,
    pub group: Group,
    pub context: &'static str,
    /// As they are read on a screen.
    pub keys: &'static [&'static str],
}

macro_rules! def {
    ($id:literal, $label:literal, $group:ident, $context:expr, [$($key:literal),*], $action:expr) => {
        Def {
            id: $id,
            label: $label,
            group: Group::$group,
            context: $context,
            keys: &[$($key),*],
            kind: Kind::Custom,
            build: || -> Box<dyn Action> { Box::new($action) },
        }
    };
}

macro_rules! fixed {
    ($id:literal, $label:literal, $group:ident, $context:expr, [$($key:literal),*], $action:expr) => {
        Def {
            id: $id,
            label: $label,
            group: Group::$group,
            context: $context,
            keys: &[$($key),*],
            kind: Kind::Fixed,
            build: || -> Box<dyn Action> { Box::new($action) },
        }
    };
}

/// Every shortcut the user can change.
#[rustfmt::skip]
pub static DEFS: &[Def] = &[
    // Windows and navigation.
    def!("search_symbol", "Search a symbol", Navigation, DASHBOARD, ["secondary-k"], d::OpenPicker),
    def!("open_settings", "Open the settings", Navigation, DASHBOARD, ["secondary-,"], d::OpenSettings),
    def!("open_shortcuts", "Open the keyboard shortcuts", Navigation, DASHBOARD, ["secondary-/"], d::OpenShortcuts),
    def!("toggle_indicator_editor", "Show or hide the indicator editor", Navigation, DASHBOARD, ["secondary-shift-e"], d::ToggleIndicatorEditor),
    def!("toggle_order_ticket", "Show or hide the order ticket", Navigation, DASHBOARD, ["f9"], d::ToggleTicket),
    def!("toggle_account_panel", "Show or hide the account panel", Navigation, DASHBOARD, ["secondary-w"], d::ToggleAccountPanel),
    def!("cancel_all_orders", "Cancel all working orders", Trading, DASHBOARD, ["secondary-alt-x"], d::CancelAllOrders),
    def!("toggle_fullscreen", "Full screen", Navigation, DASHBOARD, ["f11"], d::ToggleFullscreen),
    def!("next_chart", "Next chart", Navigation, DASHBOARD, ["secondary-tab"], d::NextChart),
    def!("previous_chart", "Previous chart", Navigation, DASHBOARD, ["secondary-shift-tab"], d::PreviousChart),
    def!("next_timeframe", "Longer timeframe", Navigation, DASHBOARD, ["alt-up"], d::NextTimeframe),
    def!("previous_timeframe", "Shorter timeframe", Navigation, DASHBOARD, ["alt-down"], d::PreviousTimeframe),
    // Trading.
    def!("ticket_buy", "Buy: open the ticket on the buy side", Trading, DASHBOARD, ["alt-b"], d::TicketBuy),
    def!("ticket_sell", "Sell: open the ticket on the sell side", Trading, DASHBOARD, ["alt-s"], d::TicketSell),
    def!("ticket_send", "Send the order of the ticket", Trading, DASHBOARD, ["secondary-enter"], d::TicketSend),
    def!("close_all_positions", "Close all positions", Trading, DASHBOARD, ["secondary-shift-x"], d::CloseAllPositions),
    def!("toggle_kill_switch", "Kill switch: stop all trading", Trading, DASHBOARD, ["secondary-shift-k"], d::ToggleKillSwitch),
    def!("add_alert", "Add a price alert", Trading, DASHBOARD, ["alt-a"], c::ChartAddAlert),
    // The chart.
    def!("pan_back", "Move the chart back", Chart, DASHBOARD, ["left"], c::ChartPanBack),
    def!("pan_forward", "Move the chart forward", Chart, DASHBOARD, ["right"], c::ChartPanForward),
    def!("zoom_in", "Zoom in", Chart, DASHBOARD, ["secondary-=", "secondary-+"], c::ChartZoomIn),
    def!("zoom_out", "Zoom out", Chart, DASHBOARD, ["secondary--"], c::ChartZoomOut),
    def!("go_to_latest", "Go to the latest bar", Chart, DASHBOARD, ["end"], c::ChartLatest),
    def!("toggle_log_scale", "Logarithmic scale on or off", Chart, DASHBOARD, ["alt-l"], d::ToggleLogScale),
    def!("toggle_percent_scale", "Percentage scale on or off", Chart, DASHBOARD, ["alt-shift-p"], d::TogglePercentScale),
    def!("toggle_grid", "Grid on or off", Chart, DASHBOARD, [], d::ToggleGrid),
    def!("reset_price_scale", "Reset the price scale", Chart, DASHBOARD, ["alt-r"], c::ChartResetScale),
    def!("screenshot", "Take a picture of the chart", Chart, DASHBOARD, ["secondary-shift-s"], c::ChartScreenshot),
    def!("screenshot_all", "Take a picture of every chart", Chart, DASHBOARD, ["secondary-alt-shift-s"], c::ChartScreenshotAll),
    def!("copy_indicators", "Copy the indicators of the chart", Chart, DASHBOARD, ["secondary-c"], c::ChartCopyIndicators),
    def!("paste_to_chart", "Paste indicators or settings", Chart, DASHBOARD, ["secondary-v"], c::ChartPaste),
    // Drawings.
    def!("find_tool", "Find a drawing tool", Drawing, DASHBOARD, ["secondary-shift-f"], c::FindTool),
    def!("undo_drawing", "Undo", Drawing, DASHBOARD, ["secondary-z"], c::UndoDrawing),
    def!("redo_drawing", "Redo", Drawing, DASHBOARD, ["secondary-shift-z", "secondary-y"], c::RedoDrawing),
    def!("duplicate_drawing", "Duplicate the selection", Drawing, DASHBOARD, ["secondary-d"], c::DuplicateDrawing),
    def!("select_all_drawings", "Select all drawings", Drawing, DASHBOARD, ["secondary-a"], c::SelectAllDrawings),
    def!("delete_drawing", "Delete the selection", Drawing, DASHBOARD, ["delete", "backspace"], c::DeleteDrawing),
    def!("nudge_up", "Move the selection up", Drawing, DASHBOARD, ["up"], c::NudgeUp),
    def!("nudge_down", "Move the selection down", Drawing, DASHBOARD, ["down"], c::NudgeDown),
    def!("nudge_up_fast", "Move the selection up, ten times more", Drawing, DASHBOARD, ["shift-up"], c::NudgeUpFast),
    def!("nudge_down_fast", "Move the selection down, ten times more", Drawing, DASHBOARD, ["shift-down"], c::NudgeDownFast),
    def!("nudge_back_fast", "Move the selection back, ten bars", Drawing, DASHBOARD, ["shift-left"], c::NudgeBackFast),
    def!("nudge_forward_fast", "Move the selection forward, ten bars", Drawing, DASHBOARD, ["shift-right"], c::NudgeForwardFast),
    def!("tool_trend_line", "Tool: trend line", Drawing, DASHBOARD, ["alt-t"], c::ToolTrendLine),
    def!("tool_horizontal", "Tool: horizontal line", Drawing, DASHBOARD, ["alt-h"], c::ToolHorizontal),
    def!("tool_vertical", "Tool: vertical line", Drawing, DASHBOARD, ["alt-v"], c::ToolVertical),
    def!("tool_fib", "Tool: Fibonacci retracement", Drawing, DASHBOARD, ["alt-f"], c::ToolFib),
    def!("tool_rectangle", "Tool: rectangle", Drawing, DASHBOARD, ["alt-x"], c::ToolRectangle),
    def!("tool_text", "Tool: text", Drawing, DASHBOARD, ["alt-n"], c::ToolText),
    def!("tool_measure", "Tool: measure", Drawing, DASHBOARD, ["alt-m"], c::ToolMeasure),
    def!("tool_brush", "Tool: brush", Drawing, DASHBOARD, ["alt-p"], c::ToolBrush),
    def!("favorite_1", "Favorite tool 1", Drawing, DASHBOARD, ["alt-1"], c::Favorite1),
    def!("favorite_2", "Favorite tool 2", Drawing, DASHBOARD, ["alt-2"], c::Favorite2),
    def!("favorite_3", "Favorite tool 3", Drawing, DASHBOARD, ["alt-3"], c::Favorite3),
    def!("favorite_4", "Favorite tool 4", Drawing, DASHBOARD, ["alt-4"], c::Favorite4),
    def!("favorite_5", "Favorite tool 5", Drawing, DASHBOARD, ["alt-5"], c::Favorite5),
    def!("favorite_6", "Favorite tool 6", Drawing, DASHBOARD, ["alt-6"], c::Favorite6),
    def!("favorite_7", "Favorite tool 7", Drawing, DASHBOARD, ["alt-7"], c::Favorite7),
    def!("favorite_8", "Favorite tool 8", Drawing, DASHBOARD, ["alt-8"], c::Favorite8),
    def!("favorite_9", "Favorite tool 9", Drawing, DASHBOARD, ["alt-9"], c::Favorite9),
    def!("toggle_magnet", "Magnet on or off", Drawing, DASHBOARD, ["alt-g"], d::ToggleMagnet),
    def!("toggle_keep_drawing", "Stay in drawing mode on or off", Drawing, DASHBOARD, ["alt-k"], d::ToggleKeepDrawing),
    def!("toggle_favorites_bar", "Show or hide the favorites bar", Drawing, DASHBOARD, [], d::ToggleFavoritesBar),
    // The indicator editor.
    def!("editor_save", "Save the script", Editor, e::CONTEXT, ["secondary-s"], e::SaveScript),
    def!("editor_add_to_chart", "Add the script to the chart", Editor, e::CONTEXT, ["secondary-enter", "f5"], e::AddToChart),
    def!("editor_close_tab", "Close the tab", Editor, e::CONTEXT, ["secondary-w"], e::CloseTab),
    def!("editor_reference", "Show or hide the reference", Editor, e::CONTEXT, ["secondary-alt-r"], e::ToggleReference),
    def!("editor_next_problem", "Next problem", Editor, e::CONTEXT, ["f8"], e::NextProblem),
    def!("editor_previous_problem", "Previous problem", Editor, e::CONTEXT, ["shift-f8"], e::PreviousProblem),
    def!("editor_next_tab", "Next tab", Editor, e::CONTEXT, ["secondary-tab"], e::NextTab),
    def!("editor_previous_tab", "Previous tab", Editor, e::CONTEXT, ["secondary-shift-tab"], e::PreviousTab),
    // Keys that cannot be changed.
    fixed!("finish_drawing", "Finish an arrow path", Drawing, "DrawingInProgress", ["enter"], c::FinishDrawing),
    fixed!("close_overlays", "Close the picker or a menu, or give up a drawing", Dialogs, DASHBOARD, ["escape"], d::ClosePicker),
    fixed!("close_dialog", "Close the window", Dialogs, "Modal", ["escape"], CloseModal),
    fixed!("dismiss_sign_in", "Close the sign-in window", Dialogs, "SignInGate", ["escape"], Dismiss),
    fixed!("picker_up", "Highlight the symbol above", Picker, PICKER, ["up", "ctrl-p"], d::PickerUp),
    fixed!("picker_down", "Highlight the symbol below", Picker, PICKER, ["down", "ctrl-n"], d::PickerDown),
    fixed!("picker_page_up", "Highlight a page up", Picker, PICKER, ["pageup"], d::PickerPageUp),
    fixed!("picker_page_down", "Highlight a page down", Picker, PICKER, ["pagedown"], d::PickerPageDown),
    fixed!("picker_choose", "Choose the highlighted symbol", Picker, PICKER, ["enter"], d::PickerConfirm),
    fixed!("text_left", "Move the cursor left", Text, TEXT, ["left"], t::TextInputLeft),
    fixed!("text_right", "Move the cursor right", Text, TEXT, ["right"], t::TextInputRight),
    fixed!("text_home", "Go to the start of the line", Text, TEXT, ["home"], t::TextInputHome),
    fixed!("text_end", "Go to the end of the line", Text, TEXT, ["end"], t::TextInputEnd),
    fixed!("text_select_left", "Select one character to the left", Text, TEXT, ["shift-left"], t::TextInputSelectLeft),
    fixed!("text_select_right", "Select one character to the right", Text, TEXT, ["shift-right"], t::TextInputSelectRight),
    fixed!("text_select_all", "Select all the text", Text, TEXT, ["secondary-a"], t::TextInputSelectAll),
    fixed!("text_backspace", "Delete before the cursor", Text, TEXT, ["backspace"], t::TextInputBackspace),
    fixed!("text_delete", "Delete after the cursor", Text, TEXT, ["delete"], t::TextInputDelete),
    fixed!("text_copy", "Copy", Text, TEXT, ["secondary-c"], t::TextInputCopy),
    fixed!("text_cut", "Cut", Text, TEXT, ["secondary-x"], t::TextInputCut),
    fixed!("text_paste", "Paste", Text, TEXT, ["secondary-v"], t::TextInputPaste),
];

/// The keys that screens read by hand. They are not bound, they are told here so that the settings
/// page shows every key of the app.
#[rustfmt::skip]
pub static HANDLED: &[Handled] = &[
    Handled { label: "Move the focus to the next or previous control", group: Group::Dialogs, context: "Everywhere", keys: &["Tab", "Shift+Tab"] },
    Handled { label: "Press the control that has the focus", group: Group::Dialogs, context: "Everywhere", keys: &["Enter", "Space"] },
    Handled { label: "Close a menu", group: Group::Dialogs, context: "Menus", keys: &["Esc"] },
    Handled { label: "Move over the entries of a menu", group: Group::Dialogs, context: "Menus", keys: &["Up", "Down"] },
    Handled { label: "Choose the entry of a menu", group: Group::Dialogs, context: "Menus", keys: &["Enter"] },
    Handled { label: "Choose an account", group: Group::Dialogs, context: "Sign-in", keys: &["Up", "Down"] },
    Handled { label: "Connect to the chosen account", group: Group::Dialogs, context: "Sign-in", keys: &["Enter"] },
    Handled { label: "Give up naming a script", group: Group::Dialogs, context: "Indicator editor", keys: &["Esc"] },
];

fn mac() -> bool {
    cfg!(target_os = "macos")
}

/// The definition of an action.
pub fn def(id: &str) -> Option<&'static Def> {
    DEFS.iter().find(|def| def.id == id)
}

/// The actions with the keys they have now.
pub fn resolved(prefs: &ShortcutPrefs) -> Vec<Resolved> {
    DEFS.iter()
        .map(|def| Resolved {
            id: def.id.to_owned(),
            context: def.context.to_owned(),
            keys: keys_of(def, prefs),
        })
        .collect()
}

/// The keys an action has now: the user's for one that can be changed, the declared ones for one
/// that cannot.
pub fn keys_of(def: &Def, prefs: &ShortcutPrefs) -> Vec<String> {
    match def.kind {
        Kind::Custom => prefs.keys_of(def.id, def.keys),
        Kind::Fixed => def.keys.iter().map(|k| (*k).to_owned()).collect(),
    }
}

/// The bindings of the table with the keys the user set, and the first combination of each
/// action as it is read on a screen. A combination gpui cannot read is left out.
fn build(prefs: &ShortcutPrefs) -> (Vec<KeyBinding>, HashMap<&'static str, String>) {
    let mut bound = Vec::new();
    let mut hints = HashMap::new();
    for def in DEFS {
        let predicate = KeyBindingContextPredicate::parse(def.context)
            .ok()
            .map(Rc::new);
        for (index, keys) in keys_of(def, prefs).iter().enumerate() {
            // gpui takes any word for a key, so what the domain refuses is left out here. A fixed
            // key may be a bare Enter or Escape, which a user may not choose, so only its
            // spelling is checked.
            let checked = match def.kind {
                Kind::Custom => canonical(keys).map(|_| ()),
                Kind::Fixed => Chord::parse(keys).map(|_| ()),
            };
            if let Err(error) = checked {
                tracing::warn!(id = def.id, keys, %error, "a shortcut was left out");
                continue;
            }
            match KeyBinding::load(
                keys,
                (def.build)(),
                predicate.clone(),
                false,
                None,
                &DummyKeyboardMapper,
            ) {
                Ok(binding) => {
                    bound.push(binding);
                    if index == 0
                        && let Ok(chord) = Chord::parse(keys)
                    {
                        hints.insert(def.id, chord.label(mac()));
                    }
                }
                Err(error) => {
                    tracing::warn!(id = def.id, keys, %error, "a shortcut could not be bound");
                }
            }
        }
    }
    (bound, hints)
}

/// Binds the keys of the table as the user set them. The bindings of the table that were bound
/// before are taken out first; the others are not touched.
pub fn apply(prefs: &ShortcutPrefs, cx: &mut App) {
    let ours = ours();
    let keymap = cx.key_bindings();
    let kept: Vec<KeyBinding> = keymap
        .borrow()
        .bindings()
        .filter(|binding| !ours.contains(binding.action().name()))
        .cloned()
        .collect();
    let (bound, hints) = build(prefs);
    {
        let mut map = keymap.borrow_mut();
        map.clear();
        map.add_bindings(kept);
        map.add_bindings(bound);
    }
    wyck_ui_kit::hints::set(hints);
    cx.refresh_windows();
}

/// A binding of the widget library: not ours, so not changeable, but it works in the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryKey {
    /// Where it works, or `Everywhere` for a binding with no context.
    pub context: String,
    /// As it is read on a screen.
    pub keys: String,
    /// What it does, from the name of the action: `Select up`.
    pub action: String,
}

/// `SelectUp` -> `Select up`, `list::SelectUp` -> `Select up`.
fn humanize(action: &str) -> String {
    let name = action.rsplit("::").next().unwrap_or(action);
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push(' ');
            out.extend(ch.to_lowercase());
        } else if i == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A keystroke as it is read on a screen.
fn pretty(keystroke: &Keystroke) -> String {
    let m = keystroke.modifiers;
    let mac = mac();
    let mut parts: Vec<String> = Vec::new();
    if m.control {
        parts.push("Ctrl".into());
    }
    if m.platform {
        parts.push(if mac { "Cmd" } else { "Win" }.into());
    }
    if m.alt {
        parts.push(if mac { "Option" } else { "Alt" }.into());
    }
    if m.shift {
        parts.push("Shift".into());
    }
    parts.push(wyck_app::shortcuts::key_label(&keystroke.key));
    parts.join("+")
}

/// The names of the actions of the table.
fn ours() -> HashSet<&'static str> {
    DEFS.iter().map(|def| (def.build)().name()).collect()
}

/// Every binding the keymap holds that is not in the table, which means the ones of the widget
/// library, sorted by where they work.
pub fn library_keys(cx: &App) -> Vec<LibraryKey> {
    let ours = ours();
    let keymap = cx.key_bindings();
    let mut found: Vec<LibraryKey> = keymap
        .borrow()
        .bindings()
        .filter(|b| !ours.contains(b.action().name()))
        .map(|b| LibraryKey {
            context: b
                .predicate()
                .map_or_else(|| "Everywhere".to_owned(), |p| p.to_string()),
            keys: b
                .keystrokes()
                .iter()
                .map(|k| pretty(k.inner()))
                .collect::<Vec<_>>()
                .join(" "),
            action: humanize(b.action().name()),
        })
        .collect();
    found.sort_by(|a, b| (&a.context, &a.action, &a.keys).cmp(&(&b.context, &b.action, &b.keys)));
    found.dedup();
    found
}

/// The binding of the widget library that works everywhere with `keys`, if there is one: a key
/// that would be taken from it in every place of the app.
pub fn reserved_by_library(keys: &str, cx: &App) -> Option<LibraryKey> {
    let input = Keystroke::parse(keys).ok()?;
    let ours = ours();
    cx.all_bindings_for_input(&[input])
        .into_iter()
        .find(|b| !ours.contains(b.action().name()) && b.predicate().is_none())
        .map(|b| LibraryKey {
            context: "Everywhere".to_owned(),
            keys: pretty(b.keystrokes()[0].inner()),
            action: humanize(b.action().name()),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_app::shortcuts::clashes;

    #[test]
    fn every_action_has_its_own_id_and_a_label() {
        let mut seen = HashSet::new();
        for def in DEFS {
            assert!(seen.insert(def.id), "{} is declared twice", def.id);
            assert!(!def.label.is_empty(), "{} has no label", def.id);
        }
    }

    #[test]
    fn every_default_key_is_a_valid_combination_the_domain_accepts() {
        for def in DEFS {
            for keys in def.keys {
                let chord =
                    Chord::parse(keys).unwrap_or_else(|e| panic!("{}: {keys}: {e}", def.id));
                assert_eq!(
                    chord.keys(),
                    *keys,
                    "{}: {keys} is not written the canonical way",
                    def.id
                );
                // A key the user may choose must be one the domain accepts; a fixed key may be
                // a bare Enter or Escape.
                if def.kind == Kind::Custom {
                    assert_eq!(canonical(keys).as_deref(), Ok(*keys), "{}", def.id);
                }
            }
        }
    }

    #[test]
    fn no_two_actions_share_a_default_key_in_one_context() {
        let found = clashes(&resolved(&ShortcutPrefs::default()));
        assert!(found.is_empty(), "default keys clash: {found:?}");
    }

    #[test]
    fn the_default_keys_all_bind_and_each_action_reads_its_first_key() {
        let (bound, hints) = build(&ShortcutPrefs::default());
        let wanted: usize = DEFS.iter().map(|d| d.keys.len()).sum();
        assert_eq!(bound.len(), wanted, "a default key could not be bound");
        assert!(hints.contains_key("undo_drawing"));
        assert!(
            !hints.contains_key("toggle_favorites_bar"),
            "no key by default"
        );
    }

    #[test]
    fn the_keys_the_user_chose_replace_the_defaults_and_none_leaves_an_action_unbound() {
        let mut prefs = ShortcutPrefs::default();
        let undo = def("undo_drawing").unwrap();
        prefs.set("undo_drawing", vec!["alt-u".into()], undo.keys);
        let kill = def("toggle_kill_switch").unwrap();
        prefs.set("toggle_kill_switch", Vec::new(), kill.keys);
        let (bound, hints) = build(&prefs);
        let (default_bound, _) = build(&ShortcutPrefs::default());
        // One key replaced by one, one key removed.
        assert_eq!(bound.len(), default_bound.len() - 1);
        assert!(hints["undo_drawing"].ends_with('U'));
        assert!(!hints.contains_key("toggle_kill_switch"));
    }

    #[test]
    fn a_key_the_domain_refuses_is_dropped_without_breaking_the_others() {
        let mut prefs = ShortcutPrefs::default();
        prefs
            .changed
            .insert("undo_drawing".into(), vec!["banana".into()]);
        let (bound, _) = build(&prefs);
        let (default_bound, _) = build(&ShortcutPrefs::default());
        assert_eq!(bound.len(), default_bound.len() - 1);
    }

    #[test]
    fn a_fixed_key_is_not_changed_by_the_preferences() {
        let mut prefs = ShortcutPrefs::default();
        prefs
            .changed
            .insert("text_copy".into(), vec!["alt-u".into()]);
        let copy = def("text_copy").unwrap();
        assert_eq!(copy.kind, Kind::Fixed);
        assert_eq!(keys_of(copy, &prefs), ["secondary-c"]);
        let (bound, _) = build(&prefs);
        let (default_bound, _) = build(&ShortcutPrefs::default());
        assert_eq!(bound.len(), default_bound.len());
    }

    #[test]
    fn a_fixed_key_counts_in_the_clashes_of_its_context() {
        // Text fields keep Ctrl+C: a changeable action in the same context may not take it.
        let all = resolved(&ShortcutPrefs::default());
        let taken = wyck_app::shortcuts::taken_by(&all, "someone", TEXT, "secondary-c");
        assert_eq!(taken.map(|r| r.id.as_str()), Some("text_copy"));
    }

    #[test]
    fn the_name_of_a_library_action_is_made_readable_and_a_keystroke_too() {
        assert_eq!(humanize("list::SelectUp"), "Select up");
        assert_eq!(humanize("Confirm"), "Confirm");
        let keystroke = Keystroke::parse("ctrl-shift-pageup").unwrap();
        assert_eq!(pretty(&keystroke), "Ctrl+Shift+Page Up");
    }

    #[test]
    fn every_group_has_actions_and_the_label_of_an_unknown_id_is_none() {
        for group in Group::ALL {
            assert!(
                DEFS.iter().any(|d| d.group == group) || HANDLED.iter().any(|h| h.group == group),
                "{group:?}"
            );
        }
        assert!(def("nope").is_none());
        assert!(def("undo_drawing").is_some());
    }
}
