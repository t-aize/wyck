//! Global keyboard shortcuts: work while another application has the focus, which is the
//! point of an app that sits on top of a charting platform.
//!
//! Windowing toolkits rarely offer system-wide hotkeys, so this uses the `global-hotkey`
//! crate. On Windows that is `RegisterHotKey`, whose messages go to the thread that
//! registered. A front end must register on a thread that pumps Windows messages, which any GUI
//! toolkit's main loop does, and then no extra thread and no `unsafe` is needed. Checked on
//! Windows 11 with GPUI's loop: the keys were pressed while another window had the focus and
//! the events arrived.
//!
//! Three concerns are kept apart:
//!
//! - [`parse_bindings`] turns the configured text into bindings and refuses duplicates. Pure,
//!   tested without registering anything.
//! - [`Hotkeys::register`] registers them with the system. A key that another program already
//!   owns fails alone; the others still work, and every failure is reported.
//! - [`Debounce`] drops the repeats a held key produces, so a held shortcut is one order
//!   request, not a stream (the engine also refuses two orders in a row, but the UI should not
//!   even ask).

use std::collections::HashMap;
use std::str::FromStr;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::settings::HotkeyText;

/// What a shortcut does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Plan and submit a buy with the default size and stop (a dry run).
    Buy,
    /// Plan and submit a sell (a dry run).
    Sell,
    /// Show or hide the front end's trade panel.
    TogglePanel,
}

impl HotkeyAction {
    /// A name for messages.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
            Self::TogglePanel => "show or hide the panel",
        }
    }
}

/// A shortcut that could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    /// The text is not a valid shortcut.
    #[error("the shortcut for {action} (`{text}`) is not valid: {reason}")]
    Parse {
        /// What it was for.
        action: &'static str,
        /// What was typed.
        text: String,
        /// Why it failed.
        reason: String,
    },
    /// Two actions were given the same keys.
    #[error("`{text}` is bound to both {first} and {second}")]
    Duplicate {
        /// The shared shortcut.
        text: String,
        /// The first action.
        first: &'static str,
        /// The second action.
        second: &'static str,
    },
    /// The system refused the registration, usually because another program owns the keys.
    #[error("could not register `{text}` for {action}: {reason}")]
    Register {
        /// What it was for.
        action: &'static str,
        /// The shortcut.
        text: String,
        /// The system's answer.
        reason: String,
    },
    /// The hotkey manager itself could not start.
    #[error("global shortcuts are unavailable: {0}")]
    Manager(String),
}

/// A parsed shortcut and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// The action.
    pub action: HotkeyAction,
    /// The parsed keys.
    pub hotkey: HotKey,
}

/// Parses the configured shortcuts and refuses duplicates.
///
/// # Errors
///
/// [`HotkeyError::Parse`] for the first invalid shortcut, or [`HotkeyError::Duplicate`] when two
/// actions share the same keys.
pub fn parse_bindings(text: &HotkeyText) -> Result<Vec<Binding>, HotkeyError> {
    let wanted = [
        (HotkeyAction::Buy, &text.buy),
        (HotkeyAction::Sell, &text.sell),
        (HotkeyAction::TogglePanel, &text.panel),
    ];
    let mut out: Vec<(Binding, &String)> = Vec::new();
    for (action, typed) in wanted {
        let hotkey = HotKey::from_str(typed).map_err(|e| HotkeyError::Parse {
            action: action.label(),
            text: typed.clone(),
            reason: e.to_string(),
        })?;
        if let Some((earlier, _)) = out.iter().find(|(b, _)| b.hotkey.id() == hotkey.id()) {
            return Err(HotkeyError::Duplicate {
                text: typed.clone(),
                first: earlier.action.label(),
                second: action.label(),
            });
        }
        out.push((Binding { action, hotkey }, typed));
    }
    Ok(out.into_iter().map(|(b, _)| b).collect())
}

/// The registered shortcuts. Keep it alive for as long as they should work: dropping it
/// unregisters them. It is tied to the thread that created it.
pub struct Hotkeys {
    _manager: GlobalHotKeyManager,
    actions: HashMap<u32, HotkeyAction>,
}

impl Hotkeys {
    /// Registers `bindings` with the system. Returns the registered set and one error per
    /// shortcut that could not be registered.
    ///
    /// # Errors
    ///
    /// [`HotkeyError::Manager`] when the system offers no global shortcuts at all.
    pub fn register(
        bindings: &[Binding],
        typed: &HotkeyText,
    ) -> Result<(Self, Vec<HotkeyError>), HotkeyError> {
        let manager =
            GlobalHotKeyManager::new().map_err(|e| HotkeyError::Manager(e.to_string()))?;
        let mut actions = HashMap::new();
        let mut failures = Vec::new();
        for b in bindings {
            match manager.register(b.hotkey) {
                Ok(()) => {
                    actions.insert(b.hotkey.id(), b.action);
                }
                Err(e) => failures.push(HotkeyError::Register {
                    action: b.action.label(),
                    text: match b.action {
                        HotkeyAction::Buy => typed.buy.clone(),
                        HotkeyAction::Sell => typed.sell.clone(),
                        HotkeyAction::TogglePanel => typed.panel.clone(),
                    },
                    reason: e.to_string(),
                }),
            }
        }
        Ok((
            Self {
                _manager: manager,
                actions,
            },
            failures,
        ))
    }

    /// The action of a registered shortcut, by the id its events carry.
    #[must_use]
    pub fn action_for(&self, id: u32) -> Option<HotkeyAction> {
        self.actions.get(&id).copied()
    }

    /// How many shortcuts are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether no shortcut is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// Forwards every key **press** (not the release) to `sink`. Replaces any earlier handler.
///
/// The handler runs on the thread that pumps the Windows messages, so `sink` must not block:
/// send on an unbounded channel.
pub fn forward_presses(sink: impl Fn(u32) + Send + Sync + 'static) {
    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
        if event.state == HotKeyState::Pressed {
            sink(event.id);
        }
    }));
}

/// Drops the repeats of a held key: an action is accepted again only after `min_gap_ms`.
#[derive(Debug, Clone)]
pub struct Debounce {
    min_gap_ms: i64,
    last: HashMap<HotkeyAction, i64>,
}

impl Debounce {
    /// A debouncer that accepts each action at most once per `min_gap_ms`.
    #[must_use]
    pub fn new(min_gap_ms: i64) -> Self {
        Self {
            min_gap_ms,
            last: HashMap::new(),
        }
    }

    /// Whether `action` at time `now_ms` should be acted on.
    pub fn accept(&mut self, action: HotkeyAction, now_ms: i64) -> bool {
        match self.last.get(&action) {
            Some(&t) if now_ms.saturating_sub(t) < self.min_gap_ms => false,
            _ => {
                self.last.insert(action, now_ms);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(buy: &str, sell: &str, panel: &str) -> HotkeyText {
        HotkeyText {
            buy: buy.to_owned(),
            sell: sell.to_owned(),
            panel: panel.to_owned(),
        }
    }

    #[test]
    fn the_defaults_parse_and_are_distinct() {
        let bindings = parse_bindings(&HotkeyText::default()).unwrap();
        assert_eq!(bindings.len(), 3);
        let actions: Vec<_> = bindings.iter().map(|b| b.action).collect();
        assert_eq!(
            actions,
            [
                HotkeyAction::Buy,
                HotkeyAction::Sell,
                HotkeyAction::TogglePanel
            ]
        );
    }

    #[test]
    fn a_shortcut_that_is_not_valid_is_refused_with_its_name() {
        let error =
            parse_bindings(&text("ctrl+alt+b", "ctrl+alt+nonsense", "ctrl+alt+p")).unwrap_err();
        assert!(
            matches!(error, HotkeyError::Parse { action: "sell", .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains("sell"));
    }

    #[test]
    fn two_actions_cannot_share_keys_even_when_written_differently() {
        let error = parse_bindings(&text("ctrl+alt+b", "Ctrl+Alt+KeyB", "ctrl+alt+p")).unwrap_err();
        assert!(
            matches!(
                error,
                HotkeyError::Duplicate {
                    first: "buy",
                    second: "sell",
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn a_held_key_is_one_request() {
        let mut d = Debounce::new(400);
        assert!(d.accept(HotkeyAction::Buy, 1_000));
        assert!(!d.accept(HotkeyAction::Buy, 1_050), "auto-repeat");
        assert!(!d.accept(HotkeyAction::Buy, 1_399));
        assert!(
            d.accept(HotkeyAction::Buy, 1_400),
            "a deliberate second press"
        );
        // Another action is not affected.
        assert!(d.accept(HotkeyAction::Sell, 1_401));
    }
}
