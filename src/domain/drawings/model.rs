//! What a drawing is: a tool, the points it hangs on, and how it looks.
//!
//! A point is a time and a price, never a screen position, so a drawing stays where the user put
//! it when the chart is scrolled, zoomed, resized or shown on another timeframe. These types are
//! also what is saved: every field has a default and unknown tools are tolerated, so a file from
//! another version still loads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod style;
mod tool;

pub use style::{DASHES, Dash, Level, MIN_LINE_OPACITY, PALETTE, Style, WIDTHS};
pub use tool::{Group, Tool};

mod capabilities;
mod defaults;

const SCHEMA_VERSION: u32 = 1;

/// The most drawings kept for one symbol, and the most points in one brush stroke. A guard
/// against a runaway file more than a limit anyone meets.
pub const MAX_DRAWINGS_PER_SYMBOL: usize = 5_000;
pub const DEFAULT_DRAWINGS_PER_SYMBOL: usize = 2_000;
pub const MAX_BRUSH_POINTS: usize = 2_000;
/// The most points an arrow path can have.
pub const MAX_PATH_POINTS: usize = 64;
/// The most levels a drawing can have.
pub const MAX_LEVELS: usize = 24;

/// A place on the chart: a time in Unix milliseconds and a raw price.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub t: i64,
    pub p: f64,
}

/// How the points of an Elliott wave are named, from the largest degree to the smallest.
pub const DEGREES: [&str; 7] = [
    "Supercycle",
    "Cycle",
    "Primary",
    "Intermediate",
    "Minor",
    "Minute",
    "Minuette",
];

/// The degree new waves start with: Minor, plain `1 2 3` and `A B C`.
pub const DEFAULT_DEGREE: u8 = 4;

/// The name of wave `name` (a digit or a capital letter) at `degree`.
pub fn wave_label(name: &str, degree: u8) -> String {
    let lower = name.to_ascii_lowercase();
    let roman = |text: &str, upper: bool| -> String {
        let r = match text {
            "1" => "i",
            "2" => "ii",
            "3" => "iii",
            "4" => "iv",
            "5" => "v",
            other => return other.to_owned(),
        };
        if upper {
            r.to_ascii_uppercase()
        } else {
            r.to_owned()
        }
    };
    match degree {
        0 => format!("({})", roman(name, true)),
        1 => roman(name, true),
        2 => format!("(({name}))"),
        3 => format!("({name})"),
        4 => name.to_owned(),
        5 => roman(&lower, false),
        _ => format!("({})", roman(&lower, false)),
    }
}

/// A look (and levels) saved as what a tool starts with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub levels: Vec<Level>,
}

/// The most looks a user can save under a name, and the longest a name can be.
pub const MAX_NAMED_TEMPLATES: usize = 100;
pub const MAX_TEMPLATE_NAME: usize = 40;

/// A look (and levels) saved under a name, to apply to any drawing of the same tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedTemplate {
    pub name: String,
    /// The tool it is for, by code.
    pub tool: String,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub levels: Vec<Level>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub id: u64,
    pub tool: Tool,
    pub points: Vec<Point>,
    #[serde(default)]
    pub style: Style,
    /// The words of a text drawing.
    #[serde(default)]
    pub text: String,
    /// A locked drawing can be selected but not moved, edited or deleted by mistake.
    #[serde(default)]
    pub locked: bool,
    /// A hidden drawing is kept but not shown.
    #[serde(default)]
    pub hidden: bool,
    /// The levels, when the tool has some; empty means the tool's own.
    #[serde(default)]
    pub levels: Vec<Level>,
    /// The timeframes (by code) the drawing shows on; `None` for every one.
    #[serde(default)]
    pub timeframes: Option<Vec<String>>,
    /// A name the user gave it, for the list of drawings.
    #[serde(default)]
    pub name: String,
    /// Whether the levels run the other way.
    #[serde(default)]
    pub reverse: bool,
    /// The degree of an Elliott wave, an index in [`DEGREES`].
    #[serde(default = "default_degree")]
    pub degree: u8,
}

fn default_degree() -> u8 {
    DEFAULT_DEGREE
}

impl Drawing {
    /// A new drawing of `tool` with its default look.
    pub fn new(id: u64, tool: Tool, points: Vec<Point>) -> Self {
        Self {
            id,
            tool,
            points,
            style: tool.default_style(),
            text: String::new(),
            locked: false,
            hidden: false,
            levels: Vec::new(),
            timeframes: None,
            name: String::new(),
            reverse: false,
            degree: DEFAULT_DEGREE,
        }
    }

    /// Whether the drawing shows on a chart of this timeframe (by its saved code).
    pub fn shows_on(&self, timeframe: &str) -> bool {
        !self.hidden
            && self
                .timeframes
                .as_ref()
                .is_none_or(|list| list.iter().any(|code| code == timeframe))
    }

    /// The levels the drawing draws: its own, or the tool's.
    pub fn levels(&self) -> Vec<Level> {
        if self.levels.is_empty() {
            self.tool.default_levels()
        } else {
            self.levels.clone()
        }
    }

    /// What the list of drawings calls it.
    pub fn title(&self) -> String {
        if !self.name.trim().is_empty() {
            self.name.trim().to_owned()
        } else if self.tool == Tool::Text && !self.text.trim().is_empty() {
            let text: String = self.text.trim().chars().take(24).collect();
            format!("Text: {text}")
        } else {
            self.tool.label().to_owned()
        }
    }

    /// Whether the drawing is well formed for its tool: the right number of finite points.
    pub fn is_valid(&self) -> bool {
        let count_ok = match self.tool {
            Tool::Unknown => false,
            Tool::Brush | Tool::Highlighter => (2..=MAX_BRUSH_POINTS).contains(&self.points.len()),
            Tool::ArrowPath => (2..=MAX_PATH_POINTS).contains(&self.points.len()),
            tool => self.points.len() == tool.anchors(),
        };
        count_ok
            && self.points.iter().all(|p| p.p.is_finite())
            && self.style.width.is_finite()
            && self.style.width > 0.0
    }

    /// The drawing with its style in range and its levels sane.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.style = self.style.normalized();
        self.levels.retain(|l| l.value.is_finite());
        self.levels.truncate(MAX_LEVELS);
        for level in &mut self.levels {
            level.color &= 0xff_ffff;
        }
        if !self.tool.has_levels() {
            self.levels.clear();
        }
        self.degree = self.degree.min(DEGREES.len() as u8 - 1);
        if let Some(list) = &mut self.timeframes {
            list.sort();
            list.dedup();
        }
        self
    }
}

/// The saved document: every symbol's drawings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrawingsDoc {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// The next id to give, so ids stay unique across runs.
    #[serde(default)]
    pub next_id: u64,
    #[serde(default)]
    pub symbols: BTreeMap<String, Vec<Drawing>>,
    /// What each tool starts with, by the tool's code, when the user saved a look as default.
    #[serde(default)]
    pub templates: BTreeMap<String, Template>,
    /// Looks saved under a name, each for one tool.
    #[serde(default)]
    pub named_templates: Vec<NamedTemplate>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for DrawingsDoc {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            next_id: 1,
            symbols: BTreeMap::new(),
            templates: BTreeMap::new(),
            named_templates: Vec::new(),
        }
    }
}

impl DrawingsDoc {
    /// The document repaired: malformed drawings and empty symbols dropped, ids made unique, and
    /// `next_id` past every id in use.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = SCHEMA_VERSION;
        let mut seen = std::collections::HashSet::new();
        let mut highest = 0;
        for drawings in self.symbols.values_mut() {
            *drawings = std::mem::take(drawings)
                .into_iter()
                .filter(Drawing::is_valid)
                .take(MAX_DRAWINGS_PER_SYMBOL)
                .map(Drawing::normalized)
                .collect();
            for drawing in drawings.iter_mut() {
                if drawing.id == 0 || !seen.insert(drawing.id) {
                    // A repeated or missing id gets a fresh one below.
                    drawing.id = 0;
                }
                highest = highest.max(drawing.id);
            }
        }
        let mut next = self.next_id.max(highest + 1).max(1);
        for drawings in self.symbols.values_mut() {
            for drawing in drawings.iter_mut() {
                if drawing.id == 0 {
                    drawing.id = next;
                    next += 1;
                }
            }
        }
        self.next_id = next;
        self.symbols.retain(|_, drawings| !drawings.is_empty());
        let known: std::collections::HashSet<String> = Tool::ALL.iter().map(|t| t.code()).collect();
        self.templates.retain(|code, _| known.contains(code));
        for template in self.templates.values_mut() {
            template.style = template.style.clone().normalized();
            template.levels.retain(|l| l.value.is_finite());
            template.levels.truncate(MAX_LEVELS);
        }
        // The named looks: a known tool, a name that says something, each name once per tool.
        let mut kept: Vec<NamedTemplate> = Vec::new();
        for mut template in std::mem::take(&mut self.named_templates) {
            template.name = template
                .name
                .trim()
                .chars()
                .take(MAX_TEMPLATE_NAME)
                .collect();
            if template.name.is_empty() || !known.contains(&template.tool) {
                continue;
            }
            template.style = template.style.normalized();
            template.levels.retain(|l| l.value.is_finite());
            template.levels.truncate(MAX_LEVELS);
            kept.retain(|t| !(t.tool == template.tool && t.name == template.name));
            kept.push(template);
        }
        kept.truncate(MAX_NAMED_TEMPLATES);
        self.named_templates = kept;
        self
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/domain/drawings/model.rs"]
mod tests;
