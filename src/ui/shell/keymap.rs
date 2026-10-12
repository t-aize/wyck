//! The keyboard shortcuts the user can change: every action with its default keys, how they are
//! bound, and how a screen asks what the keys of an action are now.
//!
//! The table [`DEFS`] is the one place where a shortcut is declared. What the user changed is kept
//! in the preferences ([`ShortcutPrefs`]) and put over the defaults by [`apply`], which binds the
//! result in gpui: the bindings of this table are taken out of the keymap and bound again, so
//! the other bindings (text fields, dialogs, the symbol picker) are left alone. The combinations
//! themselves, their rules and their clashes are in [`crate::domain::shortcuts`].

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{Action, App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};

use crate::domain::shortcuts::{Chord, Resolved, ShortcutPrefs, canonical};
use crate::ui::features::chart as c;
use crate::ui::features::indicators::editor as e;
use crate::ui::shell::dashboard as d;

/// Where the actions of the dashboard work: the key context of the dashboard.
const DASHBOARD: &str = "Dashboard";

/// How the shortcuts are grouped on the page that edits them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Navigation,
    Trading,
    Chart,
    Drawing,
    Editor,
}

impl Group {
    pub const ALL: [Self; 5] = [
        Self::Navigation,
        Self::Trading,
        Self::Chart,
        Self::Drawing,
        Self::Editor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Navigation => "Windows and navigation",
            Self::Trading => "Trading",
            Self::Chart => "Chart",
            Self::Drawing => "Drawings",
            Self::Editor => "Indicator editor",
        }
    }
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
    build: fn() -> Box<dyn Action>,
}

macro_rules! def {
    ($id:literal, $label:literal, $group:ident, $context:expr, [$($key:literal),*], $action:expr) => {
        Def {
            id: $id,
            label: $label,
            group: Group::$group,
            context: $context,
            keys: &[$($key),*],
            build: || -> Box<dyn Action> { Box::new($action) },
        }
    };
}

/// Every shortcut the user can change.
pub static DEFS: &[Def] = &[
    // Windows and navigation.
    def!(
        "search_symbol",
        "Search a symbol",
        Navigation,
        DASHBOARD,
        ["secondary-k"],
        d::OpenPicker
    ),
    def!(
        "open_settings",
        "Open the settings",
        Navigation,
        DASHBOARD,
        ["secondary-,"],
        d::OpenSettings
    ),
    def!(
        "open_shortcuts",
        "Open the keyboard shortcuts",
        Navigation,
        DASHBOARD,
        ["secondary-/"],
        d::OpenShortcuts
    ),
    def!(
        "toggle_indicator_editor",
        "Show or hide the indicator editor",
        Navigation,
        DASHBOARD,
        ["secondary-shift-e"],
        d::ToggleIndicatorEditor
    ),
    def!(
        "toggle_order_ticket",
        "Show or hide the order ticket",
        Navigation,
        DASHBOARD,
        ["f9"],
        d::ToggleTicket
    ),
    def!(
        "toggle_account_panel",
        "Show or hide the account panel",
        Navigation,
        DASHBOARD,
        ["secondary-w"],
        d::ToggleAccountPanel
    ),
    def!(
        "toggle_fullscreen",
        "Full screen",
        Navigation,
        DASHBOARD,
        ["f11"],
        d::ToggleFullscreen
    ),
    def!(
        "next_chart",
        "Next chart",
        Navigation,
        DASHBOARD,
        ["secondary-tab"],
        d::NextChart
    ),
    def!(
        "previous_chart",
        "Previous chart",
        Navigation,
        DASHBOARD,
        ["secondary-shift-tab"],
        d::PreviousChart
    ),
    def!(
        "next_timeframe",
        "Longer timeframe",
        Navigation,
        DASHBOARD,
        ["alt-up"],
        d::NextTimeframe
    ),
    def!(
        "previous_timeframe",
        "Shorter timeframe",
        Navigation,
        DASHBOARD,
        ["alt-down"],
        d::PreviousTimeframe
    ),
    // Trading.
    def!(
        "ticket_buy",
        "Buy: open the ticket on the buy side",
        Trading,
        DASHBOARD,
        ["alt-b"],
        d::TicketBuy
    ),
    def!(
        "ticket_sell",
        "Sell: open the ticket on the sell side",
        Trading,
        DASHBOARD,
        ["alt-s"],
        d::TicketSell
    ),
    def!(
        "ticket_send",
        "Send the order of the ticket",
        Trading,
        DASHBOARD,
        ["secondary-enter"],
        d::TicketSend
    ),
    def!(
        "close_all_positions",
        "Close all positions",
        Trading,
        DASHBOARD,
        ["secondary-shift-x"],
        d::CloseAllPositions
    ),
    def!(
        "toggle_kill_switch",
        "Kill switch: stop all trading",
        Trading,
        DASHBOARD,
        ["secondary-shift-k"],
        d::ToggleKillSwitch
    ),
    def!(
        "add_alert",
        "Add a price alert",
        Trading,
        DASHBOARD,
        ["alt-a"],
        c::ChartAddAlert
    ),
    // The chart.
    def!(
        "pan_back",
        "Move the chart back",
        Chart,
        DASHBOARD,
        ["left"],
        c::ChartPanBack
    ),
    def!(
        "pan_forward",
        "Move the chart forward",
        Chart,
        DASHBOARD,
        ["right"],
        c::ChartPanForward
    ),
    def!(
        "zoom_in",
        "Zoom in",
        Chart,
        DASHBOARD,
        ["secondary-=", "secondary-+"],
        c::ChartZoomIn
    ),
    def!(
        "zoom_out",
        "Zoom out",
        Chart,
        DASHBOARD,
        ["secondary--"],
        c::ChartZoomOut
    ),
    def!(
        "go_to_latest",
        "Go to the latest bar",
        Chart,
        DASHBOARD,
        ["end"],
        c::ChartLatest
    ),
    def!(
        "reset_price_scale",
        "Reset the price scale",
        Chart,
        DASHBOARD,
        ["alt-r"],
        c::ChartResetScale
    ),
    def!(
        "screenshot",
        "Take a picture of the chart",
        Chart,
        DASHBOARD,
        ["secondary-shift-s"],
        c::ChartScreenshot
    ),
    def!(
        "screenshot_all",
        "Take a picture of every chart",
        Chart,
        DASHBOARD,
        ["secondary-alt-shift-s"],
        c::ChartScreenshotAll
    ),
    def!(
        "copy_indicators",
        "Copy the indicators of the chart",
        Chart,
        DASHBOARD,
        ["secondary-c"],
        c::ChartCopyIndicators
    ),
    def!(
        "copy_settings",
        "Copy the settings of the chart",
        Chart,
        DASHBOARD,
        ["secondary-shift-c"],
        c::ChartCopySettings
    ),
    def!(
        "paste_to_chart",
        "Paste indicators or settings",
        Chart,
        DASHBOARD,
        ["secondary-v"],
        c::ChartPaste
    ),
    // Drawings.
    def!(
        "find_tool",
        "Find a drawing tool",
        Drawing,
        DASHBOARD,
        ["secondary-shift-f"],
        c::FindTool
    ),
    def!(
        "undo_drawing",
        "Undo",
        Drawing,
        DASHBOARD,
        ["secondary-z"],
        c::UndoDrawing
    ),
    def!(
        "redo_drawing",
        "Redo",
        Drawing,
        DASHBOARD,
        ["secondary-shift-z", "secondary-y"],
        c::RedoDrawing
    ),
    def!(
        "duplicate_drawing",
        "Duplicate the selection",
        Drawing,
        DASHBOARD,
        ["secondary-d"],
        c::DuplicateDrawing
    ),
    def!(
        "select_all_drawings",
        "Select all drawings",
        Drawing,
        DASHBOARD,
        ["secondary-a"],
        c::SelectAllDrawings
    ),
    def!(
        "delete_drawing",
        "Delete the selection",
        Drawing,
        DASHBOARD,
        ["delete", "backspace"],
        c::DeleteDrawing
    ),
    def!(
        "nudge_up",
        "Move the selection up",
        Drawing,
        DASHBOARD,
        ["up"],
        c::NudgeUp
    ),
    def!(
        "nudge_down",
        "Move the selection down",
        Drawing,
        DASHBOARD,
        ["down"],
        c::NudgeDown
    ),
    def!(
        "nudge_up_fast",
        "Move the selection up, ten times more",
        Drawing,
        DASHBOARD,
        ["shift-up"],
        c::NudgeUpFast
    ),
    def!(
        "nudge_down_fast",
        "Move the selection down, ten times more",
        Drawing,
        DASHBOARD,
        ["shift-down"],
        c::NudgeDownFast
    ),
    def!(
        "nudge_back_fast",
        "Move the selection back, ten bars",
        Drawing,
        DASHBOARD,
        ["shift-left"],
        c::NudgeBackFast
    ),
    def!(
        "nudge_forward_fast",
        "Move the selection forward, ten bars",
        Drawing,
        DASHBOARD,
        ["shift-right"],
        c::NudgeForwardFast
    ),
    def!(
        "tool_trend_line",
        "Tool: trend line",
        Drawing,
        DASHBOARD,
        ["alt-t"],
        c::ToolTrendLine
    ),
    def!(
        "tool_horizontal",
        "Tool: horizontal line",
        Drawing,
        DASHBOARD,
        ["alt-h"],
        c::ToolHorizontal
    ),
    def!(
        "tool_vertical",
        "Tool: vertical line",
        Drawing,
        DASHBOARD,
        ["alt-v"],
        c::ToolVertical
    ),
    def!(
        "tool_fib",
        "Tool: Fibonacci retracement",
        Drawing,
        DASHBOARD,
        ["alt-f"],
        c::ToolFib
    ),
    def!(
        "tool_rectangle",
        "Tool: rectangle",
        Drawing,
        DASHBOARD,
        ["alt-x"],
        c::ToolRectangle
    ),
    def!(
        "tool_text",
        "Tool: text",
        Drawing,
        DASHBOARD,
        ["alt-n"],
        c::ToolText
    ),
    def!(
        "tool_measure",
        "Tool: measure",
        Drawing,
        DASHBOARD,
        ["alt-m"],
        c::ToolMeasure
    ),
    def!(
        "tool_brush",
        "Tool: brush",
        Drawing,
        DASHBOARD,
        ["alt-p"],
        c::ToolBrush
    ),
    def!(
        "favorite_1",
        "Favorite tool 1",
        Drawing,
        DASHBOARD,
        ["alt-1"],
        c::Favorite1
    ),
    def!(
        "favorite_2",
        "Favorite tool 2",
        Drawing,
        DASHBOARD,
        ["alt-2"],
        c::Favorite2
    ),
    def!(
        "favorite_3",
        "Favorite tool 3",
        Drawing,
        DASHBOARD,
        ["alt-3"],
        c::Favorite3
    ),
    def!(
        "favorite_4",
        "Favorite tool 4",
        Drawing,
        DASHBOARD,
        ["alt-4"],
        c::Favorite4
    ),
    def!(
        "favorite_5",
        "Favorite tool 5",
        Drawing,
        DASHBOARD,
        ["alt-5"],
        c::Favorite5
    ),
    def!(
        "favorite_6",
        "Favorite tool 6",
        Drawing,
        DASHBOARD,
        ["alt-6"],
        c::Favorite6
    ),
    def!(
        "favorite_7",
        "Favorite tool 7",
        Drawing,
        DASHBOARD,
        ["alt-7"],
        c::Favorite7
    ),
    def!(
        "favorite_8",
        "Favorite tool 8",
        Drawing,
        DASHBOARD,
        ["alt-8"],
        c::Favorite8
    ),
    def!(
        "favorite_9",
        "Favorite tool 9",
        Drawing,
        DASHBOARD,
        ["alt-9"],
        c::Favorite9
    ),
    def!(
        "toggle_magnet",
        "Magnet on or off",
        Drawing,
        DASHBOARD,
        ["alt-g"],
        d::ToggleMagnet
    ),
    def!(
        "toggle_keep_drawing",
        "Stay in drawing mode on or off",
        Drawing,
        DASHBOARD,
        ["alt-k"],
        d::ToggleKeepDrawing
    ),
    def!(
        "toggle_favorites_bar",
        "Show or hide the favorites bar",
        Drawing,
        DASHBOARD,
        [],
        d::ToggleFavoritesBar
    ),
    // The indicator editor.
    def!(
        "editor_save",
        "Save the script",
        Editor,
        e::CONTEXT,
        ["secondary-s"],
        e::SaveScript
    ),
    def!(
        "editor_add_to_chart",
        "Add the script to the chart",
        Editor,
        e::CONTEXT,
        ["secondary-enter", "f5"],
        e::AddToChart
    ),
    def!(
        "editor_close_tab",
        "Close the tab",
        Editor,
        e::CONTEXT,
        ["secondary-w"],
        e::CloseTab
    ),
    def!(
        "editor_reference",
        "Show or hide the reference",
        Editor,
        e::CONTEXT,
        ["secondary-alt-r"],
        e::ToggleReference
    ),
    def!(
        "editor_next_problem",
        "Next problem",
        Editor,
        e::CONTEXT,
        ["f8"],
        e::NextProblem
    ),
    def!(
        "editor_previous_problem",
        "Previous problem",
        Editor,
        e::CONTEXT,
        ["shift-f8"],
        e::PreviousProblem
    ),
    def!(
        "editor_next_tab",
        "Next tab",
        Editor,
        e::CONTEXT,
        ["secondary-tab"],
        e::NextTab
    ),
    def!(
        "editor_previous_tab",
        "Previous tab",
        Editor,
        e::CONTEXT,
        ["secondary-shift-tab"],
        e::PreviousTab
    ),
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
            keys: prefs.keys_of(def.id, def.keys),
        })
        .collect()
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
        for (index, keys) in prefs.keys_of(def.id, def.keys).iter().enumerate() {
            // gpui takes any word for a key, so what the domain refuses is left out here.
            if let Err(error) = canonical(keys) {
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
    let ours: HashSet<&'static str> = DEFS.iter().map(|def| (def.build)().name()).collect();
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
    crate::ui::kit::hints::set(hints);
    cx.refresh_windows();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::shortcuts::clashes;

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
                assert_eq!(
                    canonical(keys).as_deref(),
                    Ok(*keys),
                    "{}: {keys} is not written the canonical way",
                    def.id
                );
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
    fn every_group_has_actions_and_the_label_of_an_unknown_id_is_none() {
        for group in Group::ALL {
            assert!(DEFS.iter().any(|d| d.group == group), "{group:?}");
        }
        assert!(def("nope").is_none());
        assert!(def("undo_drawing").is_some());
    }
}
