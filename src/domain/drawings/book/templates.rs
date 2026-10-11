//! The templates a tool starts from, and the named ones the user saved.

use super::Book;
use crate::domain::drawings::model::tool::Tool;
use crate::domain::drawings::model::{
    MAX_NAMED_TEMPLATES, MAX_TEMPLATE_NAME, NamedTemplate, Template,
};

impl Book {
    /// Makes the look and levels of this drawing what new drawings of its tool start with.
    pub fn save_template(&mut self, symbol: &str, id: u64) -> bool {
        let Some(drawing) = self.get(symbol, id) else {
            return false;
        };
        let template = Template {
            style: drawing.style.clone(),
            levels: drawing.levels.clone(),
        };
        self.templates.insert(drawing.tool.code(), template);
        self.revision += 1;
        true
    }

    /// Forgets the saved look of a tool, so new drawings start with the built-in one.
    pub fn forget_template(&mut self, tool: Tool) -> bool {
        let removed = self.templates.remove(&tool.code()).is_some();
        if removed {
            self.revision += 1;
        }
        removed
    }

    pub fn has_template(&self, tool: Tool) -> bool {
        self.templates.contains_key(&tool.code())
    }

    /// The looks saved under a name for `tool`, in the order they were saved.
    pub fn named_templates(&self, tool: Tool) -> Vec<&NamedTemplate> {
        let code = tool.code();
        self.named_templates
            .iter()
            .filter(|t| t.tool == code)
            .collect()
    }

    /// Saves the look of a drawing under `name`, replacing the look of the same name for the same
    /// tool. Returns whether it was saved: a name that says nothing is refused, and so is one more
    /// than the book holds.
    pub fn save_named_template(&mut self, symbol: &str, id: u64, name: &str) -> bool {
        let name: String = name.trim().chars().take(MAX_TEMPLATE_NAME).collect();
        let Some(drawing) = self.get(symbol, id) else {
            return false;
        };
        if name.is_empty() {
            return false;
        }
        let template = NamedTemplate {
            name,
            tool: drawing.tool.code(),
            style: drawing.style.clone(),
            levels: drawing.levels.clone(),
        };
        let same = |t: &NamedTemplate| t.tool == template.tool && t.name == template.name;
        if let Some(slot) = self.named_templates.iter_mut().find(|t| same(t)) {
            *slot = template;
        } else if self.named_templates.len() < MAX_NAMED_TEMPLATES {
            self.named_templates.push(template);
        } else {
            return false;
        }
        self.revision += 1;
        true
    }

    /// Forgets the look saved under `name` for `tool`.
    pub fn delete_named_template(&mut self, tool: Tool, name: &str) -> bool {
        let code = tool.code();
        let before = self.named_templates.len();
        self.named_templates
            .retain(|t| !(t.tool == code && t.name == name));
        let removed = self.named_templates.len() != before;
        if removed {
            self.revision += 1;
        }
        removed
    }

    /// Gives the selected drawings of `tool` the look saved under `name` for that tool.
    pub fn apply_named_template(&mut self, symbol: &str, tool: Tool, name: &str) -> bool {
        let code = tool.code();
        let Some(template) = self
            .named_templates
            .iter()
            .find(|t| t.tool == code && t.name == name)
            .cloned()
        else {
            return false;
        };
        self.edit_selected(symbol, |d| {
            if d.tool == tool {
                d.style = template.style.clone();
                d.levels = template.levels.clone();
            }
        })
    }

    /// Gives the selected drawings of `tool` the look new ones of the tool start with.
    pub fn apply_starting_style(&mut self, symbol: &str, tool: Tool) -> bool {
        let (style, levels) = self.starting_style(tool);
        self.edit_selected(symbol, |d| {
            if d.tool == tool {
                d.style = style.clone();
                d.levels = levels.clone();
            }
        })
    }

    /// The look a new drawing of `tool` starts with: the saved one, or the built-in one.
    pub fn starting_style(
        &self,
        tool: Tool,
    ) -> (
        crate::domain::drawings::model::style::Style,
        Vec<crate::domain::drawings::model::style::Level>,
    ) {
        match self.templates.get(&tool.code()) {
            Some(template) => (template.style.clone(), template.levels.clone()),
            None => (tool.default_style(), Vec::new()),
        }
    }
}
