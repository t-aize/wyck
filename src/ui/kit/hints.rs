//! The keys of the actions, as the screens write them in tooltips and menus.
//!
//! The shortcuts can be changed, so a hint cannot be typed into the screen that shows it: it asks
//! here, and gets the keys in force now. `ui::shell::keymap` fills this in whenever it binds the
//! keys, and an action with no key has no hint.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::domain::drawings::model::tool::Tool;

static HINTS: RwLock<Option<HashMap<&'static str, String>>> = RwLock::new(None);

/// Replaces what is known about the keys.
pub fn set(hints: HashMap<&'static str, String>) {
    if let Ok(mut slot) = HINTS.write() {
        *slot = Some(hints);
    }
}

/// The first combination of an action as it is read on a screen (`Ctrl+Shift+Z`), or `None` when
/// it has no key.
pub fn hint(id: &str) -> Option<String> {
    HINTS.read().ok()?.as_ref()?.get(id).cloned()
}

/// ` (Ctrl+Z)` for a tooltip, or nothing when the action has no key.
pub fn suffix(id: &str) -> String {
    hint(id)
        .map(|keys| format!(" ({keys})"))
        .unwrap_or_default()
}

/// The keys that pick a drawing tool, or `None` for a tool with none.
pub fn tool_hint(tool: Tool) -> Option<String> {
    let id = match tool {
        Tool::TrendLine => "tool_trend_line",
        Tool::HorizontalLine => "tool_horizontal",
        Tool::VerticalLine => "tool_vertical",
        Tool::FibRetracement => "tool_fib",
        Tool::Rectangle => "tool_rectangle",
        Tool::Text => "tool_text",
        Tool::Measure => "tool_measure",
        Tool::Brush => "tool_brush",
        _ => return None,
    };
    hint(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hint_is_what_was_set_and_an_action_with_no_key_has_none() {
        set(HashMap::from([("tool_trend_line", "Alt+T".to_owned())]));
        assert_eq!(hint("tool_trend_line").as_deref(), Some("Alt+T"));
        assert_eq!(suffix("tool_trend_line"), " (Alt+T)");
        assert_eq!(tool_hint(Tool::TrendLine).as_deref(), Some("Alt+T"));
        assert_eq!(tool_hint(Tool::Text), None);
        assert_eq!(suffix("nothing"), "");
    }
}
