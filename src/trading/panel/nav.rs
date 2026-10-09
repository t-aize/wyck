//! Moving through the rows of a table with the keyboard: the arrows, Home and End, Page Up and
//! Page Down, and the key that opens the menu of a row. Plain functions of the key and the
//! position, so they are tested without a window.

/// Where a key sends the focus among the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

/// How many rows a page moves.
pub const PAGE: usize = 8;

/// The step a key stands for, if it stands for one. `key` is the name gpui gives the key.
pub fn step_for(key: &str) -> Option<Step> {
    match key {
        "up" => Some(Step::Up),
        "down" => Some(Step::Down),
        "pageup" => Some(Step::PageUp),
        "pagedown" => Some(Step::PageDown),
        "home" => Some(Step::First),
        "end" => Some(Step::Last),
        _ => None,
    }
}

/// The row the focus goes to from row `from` of `len`. It stops at the ends rather than going
/// round: a list that wraps loses the place of a user who cannot see it.
pub fn target(from: usize, len: usize, step: Step) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let from = from.min(last);
    match step {
        Step::Up => from.saturating_sub(1),
        Step::Down => (from + 1).min(last),
        Step::PageUp => from.saturating_sub(PAGE),
        Step::PageDown => (from + PAGE).min(last),
        Step::First => 0,
        Step::Last => last,
    }
}

/// Whether a key press asks for the menu of the row: the menu key, or Shift+F10, which is what
/// keyboards without a menu key use.
pub fn asks_for_menu(key: &str, shift: bool) -> bool {
    key == "menu" || (key == "f10" && shift)
}

/// The row that keeps the focus (the one Tab arrives on): the one that had it if it is still
/// there, else the first.
pub fn current(keys: &[String], remembered: Option<&str>) -> usize {
    remembered
        .and_then(|key| keys.iter().position(|k| k == key))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_of_a_list_are_the_ones_users_expect() {
        assert_eq!(step_for("down"), Some(Step::Down));
        assert_eq!(step_for("home"), Some(Step::First));
        assert_eq!(step_for("pagedown"), Some(Step::PageDown));
        assert_eq!(step_for("a"), None);
        assert_eq!(step_for("tab"), None, "Tab leaves the table");
    }

    #[test]
    fn the_focus_stops_at_the_ends_of_the_list() {
        assert_eq!(target(0, 5, Step::Up), 0);
        assert_eq!(target(4, 5, Step::Down), 4);
        assert_eq!(target(1, 5, Step::Down), 2);
        assert_eq!(target(3, 5, Step::Up), 2);
        assert_eq!(target(2, 5, Step::First), 0);
        assert_eq!(target(2, 5, Step::Last), 4);
        assert_eq!(target(1, 30, Step::PageDown), 1 + PAGE);
        assert_eq!(target(25, 30, Step::PageDown), 29);
        assert_eq!(target(3, 30, Step::PageUp), 0);
    }

    #[test]
    fn an_empty_list_and_a_stale_position_do_no_harm() {
        assert_eq!(target(0, 0, Step::Down), 0);
        assert_eq!(target(9, 0, Step::Last), 0);
        // The list shrank under the focus.
        assert_eq!(target(9, 3, Step::Up), 1);
        assert_eq!(target(9, 3, Step::Down), 2);
    }

    #[test]
    fn the_menu_has_two_keys() {
        assert!(asks_for_menu("menu", false));
        assert!(asks_for_menu("f10", true));
        assert!(!asks_for_menu("f10", false));
        assert!(!asks_for_menu("enter", true));
    }

    #[test]
    fn the_row_that_keeps_the_focus_is_the_remembered_one_or_the_first() {
        let keys: Vec<String> = ["a", "b", "c"].iter().map(|k| (*k).to_owned()).collect();
        assert_eq!(current(&keys, Some("c")), 2);
        assert_eq!(current(&keys, Some("gone")), 0);
        assert_eq!(current(&keys, None), 0);
        assert_eq!(current(&[], Some("a")), 0);
    }
}
