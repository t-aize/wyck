//! The bar of favorite drawing tools: which tools are pinned and in what order.

use super::Preferences;
use wyck_drawings::model::tool::Tool;

/// The most tools the bar of favorites holds.
pub const MAX_FAVORITE_TOOLS: usize = 16;

/// The tools pinned the first time: the ones drawn most.
pub(super) fn default_favorite_tools() -> Vec<String> {
    [
        Tool::TrendLine,
        Tool::HorizontalLine,
        Tool::Rectangle,
        Tool::FibRetracement,
        Tool::LongPosition,
        Tool::ShortPosition,
        Tool::Text,
    ]
    .into_iter()
    .map(Tool::code)
    .collect()
}

impl Preferences {
    /// Keeps the known tools, each once, in the order saved, no more than the bar holds. An empty
    /// list is kept: it is what the user chose, not a reason for the defaults.
    pub(super) fn normalize_favorite_tools(&mut self) {
        let mut tools: Vec<Tool> = Vec::new();
        for code in &self.favorite_tools {
            if let Some(tool) = Tool::from_code(code)
                && !tools.contains(&tool)
                && tools.len() < MAX_FAVORITE_TOOLS
            {
                tools.push(tool);
            }
        }
        self.favorite_tools = tools.into_iter().map(Tool::code).collect();
    }

    /// The favorite tools, in the order the bar shows them.
    pub fn favorite_tool_list(&self) -> Vec<Tool> {
        self.favorite_tools
            .iter()
            .filter_map(|code| Tool::from_code(code))
            .collect()
    }

    /// Pins the tool, or unpins it when it is pinned. Returns whether it is a favorite now. A full
    /// bar takes no more, and the tool is not pinned.
    pub fn toggle_favorite_tool(&mut self, tool: Tool) -> bool {
        let code = tool.code();
        if let Some(at) = self.favorite_tools.iter().position(|c| *c == code) {
            self.favorite_tools.remove(at);
            return false;
        }
        if self.favorite_tools.len() >= MAX_FAVORITE_TOOLS {
            return false;
        }
        self.favorite_tools.push(code);
        true
    }

    /// Moves the favorite at `index` by `delta` places (negative is towards the start), stopping
    /// at the ends.
    pub fn move_favorite_tool(&mut self, index: usize, delta: isize) {
        let len = self.favorite_tools.len();
        if index >= len {
            return;
        }
        let target = index
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        if target != index {
            let code = self.favorite_tools.remove(index);
            self.favorite_tools.insert(target, code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_favorite_tools_start_with_the_usual_ones_and_old_files_get_them() {
        let fresh = Preferences::default();
        let list = fresh.favorite_tool_list();
        assert_eq!(list[0], Tool::TrendLine);
        assert!(list.contains(&Tool::LongPosition));
        assert!(fresh.favorites_bar && !fresh.favorites_labels);

        // A file from before the bar has none of these keys.
        let old: Preferences = toml::from_str(
            "magnet = true
    ",
        )
        .unwrap();
        assert_eq!(old.favorite_tool_list(), list);
        assert!(old.favorites_bar);
    }

    #[test]
    fn a_tool_is_pinned_and_unpinned_by_the_same_call() {
        let mut prefs = Preferences::default();
        assert!(
            !prefs.toggle_favorite_tool(Tool::TrendLine),
            "it was pinned: now it is not"
        );
        assert!(!prefs.favorite_tool_list().contains(&Tool::TrendLine));
        assert!(prefs.toggle_favorite_tool(Tool::Ellipse));
        assert_eq!(prefs.favorite_tool_list().last(), Some(&Tool::Ellipse));
        assert!(!prefs.toggle_favorite_tool(Tool::Ellipse));
    }

    #[test]
    fn the_bar_holds_a_limited_number_and_an_empty_bar_stays_empty() {
        let mut prefs = Preferences::default();
        prefs.favorite_tools.clear();
        for tool in Tool::ALL.into_iter().filter(|t| *t != Tool::Unknown) {
            prefs.toggle_favorite_tool(tool);
        }
        assert_eq!(prefs.favorite_tools.len(), MAX_FAVORITE_TOOLS);
        let extra = Tool::ALL
            .into_iter()
            .find(|t| !prefs.favorite_tool_list().contains(t))
            .unwrap();
        assert!(
            !prefs.toggle_favorite_tool(extra),
            "a full bar takes no more"
        );
        assert!(!prefs.favorite_tool_list().contains(&extra));

        prefs.favorite_tools.clear();
        let prefs = prefs.normalized();
        assert!(
            prefs.favorite_tools.is_empty(),
            "the choice of none is kept"
        );
    }

    #[test]
    fn normalizing_drops_unknown_and_repeated_favorites_and_keeps_the_order() {
        let prefs = Preferences {
            favorite_tools: vec![
                Tool::Rectangle.code(),
                "no_such_tool".to_owned(),
                Tool::TrendLine.code(),
                Tool::Rectangle.code(),
            ],
            ..Preferences::default()
        }
        .normalized();
        assert_eq!(
            prefs.favorite_tool_list(),
            vec![Tool::Rectangle, Tool::TrendLine]
        );
    }

    #[test]
    fn favorites_move_along_the_bar_and_stop_at_its_ends() {
        let mut prefs = Preferences {
            favorite_tools: vec![Tool::Ray.code(), Tool::Arrow.code(), Tool::Text.code()],
            ..Preferences::default()
        };
        prefs.move_favorite_tool(2, -1);
        assert_eq!(
            prefs.favorite_tool_list(),
            vec![Tool::Ray, Tool::Text, Tool::Arrow]
        );
        prefs.move_favorite_tool(0, -1);
        prefs.move_favorite_tool(2, 1);
        prefs.move_favorite_tool(9, 1);
        assert_eq!(
            prefs.favorite_tool_list(),
            vec![Tool::Ray, Tool::Text, Tool::Arrow]
        );
        prefs.move_favorite_tool(0, 5);
        assert_eq!(prefs.favorite_tool_list().last(), Some(&Tool::Ray));
    }
}
