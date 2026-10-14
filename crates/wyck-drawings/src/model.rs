//! What a drawing is: a tool, the points it hangs on, and how it looks.
//!
//! A point is a time and a price, never a screen position, so a drawing stays where the user put
//! it when the chart is scrolled, zoomed, resized or shown on another timeframe. These types are
//! also what is saved: every field has a default and unknown tools are tolerated, so a file from
//! another version still loads.

mod capabilities;
mod defaults;
pub mod style;
pub mod tool;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 1;

/// The most drawings kept for one symbol, and the most points in one brush stroke. A guard
/// against a runaway file more than a limit anyone meets.
pub const MAX_DRAWINGS_PER_SYMBOL: usize = 5_000;
pub const DEFAULT_DRAWINGS_PER_SYMBOL: usize = 2_000;
pub const MAX_BRUSH_POINTS: usize = 2_000;
/// The most points an arrow path can have.
pub(crate) const MAX_PATH_POINTS: usize = 64;
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
pub(crate) const DEFAULT_DEGREE: u8 = 4;

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
    pub style: style::Style,
    #[serde(default)]
    pub levels: Vec<style::Level>,
}

/// The most looks a user can save under a name, and the longest a name can be.
pub(crate) const MAX_NAMED_TEMPLATES: usize = 100;
pub(crate) const MAX_TEMPLATE_NAME: usize = 40;

/// A look (and levels) saved under a name, to apply to any drawing of the same tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedTemplate {
    pub name: String,
    /// The tool it is for, by code.
    pub tool: String,
    #[serde(default)]
    pub style: style::Style,
    #[serde(default)]
    pub levels: Vec<style::Level>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing {
    pub id: u64,
    pub tool: tool::Tool,
    pub points: Vec<Point>,
    #[serde(default)]
    pub style: style::Style,
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
    pub levels: Vec<style::Level>,
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
    pub fn new(id: u64, tool: tool::Tool, points: Vec<Point>) -> Self {
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
    pub fn levels(&self) -> Vec<style::Level> {
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
        } else if self.tool == tool::Tool::Text && !self.text.trim().is_empty() {
            let text: String = self.text.trim().chars().take(24).collect();
            format!("Text: {text}")
        } else {
            self.tool.label().to_owned()
        }
    }

    /// Whether the drawing is well formed for its tool: the right number of finite points.
    pub fn is_valid(&self) -> bool {
        let count_ok = match self.tool {
            tool::Tool::Unknown => false,
            tool::Tool::Brush | tool::Tool::Highlighter => {
                (2..=MAX_BRUSH_POINTS).contains(&self.points.len())
            }
            tool::Tool::ArrowPath => (2..=MAX_PATH_POINTS).contains(&self.points.len()),
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
        let known: std::collections::HashSet<String> =
            tool::Tool::ALL.iter().map(|t| t.code()).collect();
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
mod tests {

    use super::style::*;
    use super::tool::*;
    use super::*;

    fn drawing(id: u64, tool: Tool, count: usize) -> Drawing {
        Drawing::new(
            id,
            tool,
            (0..count)
                .map(|i| Point {
                    t: i as i64 * 60_000,
                    p: 100.0 + i as f64,
                })
                .collect(),
        )
    }

    #[test]
    fn a_style_saved_before_opacity_is_fully_opaque_and_the_opacity_is_kept_in_range() {
        let old: Style = toml::from_str(
            "color = 255
    ",
        )
        .unwrap();
        assert_eq!(old.opacity, 1.0);
        let wild = Style {
            opacity: 7.0,
            ..Style::default()
        }
        .normalized();
        assert_eq!(wild.opacity, 1.0);
        let faint = Style {
            opacity: 0.0,
            ..Style::default()
        }
        .normalized();
        assert!(faint.opacity >= 0.05);
    }

    #[test]
    fn a_drawing_needs_the_points_its_tool_asks_for() {
        assert!(drawing(1, Tool::TrendLine, 2).is_valid());
        assert!(!drawing(1, Tool::TrendLine, 1).is_valid());
        assert!(drawing(1, Tool::LongPosition, 4).is_valid());
        assert!(!drawing(1, Tool::LongPosition, 3).is_valid());
        assert!(drawing(1, Tool::Brush, 40).is_valid());
        assert!(!drawing(1, Tool::Brush, 1).is_valid());
        assert!(!drawing(1, Tool::Unknown, 2).is_valid());
        let mut nan = drawing(1, Tool::HorizontalLine, 1);
        nan.points[0].p = f64::NAN;
        assert!(!nan.is_valid());
    }

    #[test]
    fn a_saved_document_reads_back_and_unknown_tools_do_not_break_it() {
        let mut doc = DrawingsDoc::default();
        doc.symbols.insert(
            "US100.cash".into(),
            vec![drawing(1, Tool::FibRetracement, 2)],
        );
        doc.next_id = 2;
        let text = toml::to_string_pretty(&doc).unwrap();
        let back: DrawingsDoc = toml::from_str(&text).unwrap();
        assert_eq!(back, doc);

        let with_future_tool = r#"
            [[symbols."EURUSD"]]
            id = 5
            tool = "future_tool"
            points = [{ t = 1, p = 1.0 }, { t = 2, p = 2.0 }]
            [[symbols."EURUSD"]]
            id = 6
            tool = "trend_line"
            points = [{ t = 1, p = 1.0 }, { t = 2, p = 2.0 }]
        "#;
        let parsed: DrawingsDoc = toml::from_str(with_future_tool).unwrap();
        let parsed = parsed.normalized();
        let kept = &parsed.symbols["EURUSD"];
        assert_eq!(
            kept.len(),
            1,
            "the tool this version does not know is dropped"
        );
        assert_eq!(kept[0].tool, Tool::TrendLine);
        assert!(parsed.next_id > 6);
    }

    #[test]
    fn normalizing_repairs_ids_and_drops_the_broken() {
        let mut doc = DrawingsDoc::default();
        doc.symbols.insert(
            "A".into(),
            vec![
                drawing(3, Tool::Rectangle, 2),
                drawing(3, Tool::Ellipse, 2),
                drawing(0, Tool::Ray, 2),
                drawing(9, Tool::Ray, 1),
            ],
        );
        doc.symbols.insert("Empty".into(), vec![]);
        doc.next_id = 1;
        let doc = doc.normalized();
        let ids: Vec<u64> = doc.symbols["A"].iter().map(|d| d.id).collect();
        assert_eq!(ids.len(), 3, "the one with too few points is gone");
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 3, "ids are unique: {ids:?}");
        assert!(doc.next_id > *ids.iter().max().unwrap());
        assert!(!doc.symbols.contains_key("Empty"));
    }

    #[test]
    fn an_empty_or_partial_file_still_loads() {
        let doc: DrawingsDoc = toml::from_str("").unwrap();
        assert!(doc.symbols.is_empty());
        let one: Drawing =
            toml::from_str("id = 1\ntool = \"text\"\npoints = [{ t = 5, p = 2.5 }]\n").unwrap();
        assert_eq!(one.style, Style::default());
        assert!(!one.locked);
        assert!(one.is_valid());
    }

    #[test]
    fn a_drawing_shows_on_the_timeframes_it_is_kept_for() {
        let mut d = drawing(1, Tool::TrendLine, 2);
        assert!(d.shows_on("M5"));
        d.timeframes = Some(vec!["H1".into(), "D1".into()]);
        assert!(!d.shows_on("M5"));
        assert!(d.shows_on("H1"));
        d.hidden = true;
        assert!(!d.shows_on("H1"));
    }

    #[test]
    fn levels_default_to_the_tools_and_are_kept_sane() {
        let mut d = drawing(1, Tool::FibRetracement, 2);
        assert_eq!(d.levels(), Tool::FibRetracement.default_levels());
        d.levels = vec![Level {
            value: f64::NAN,
            color: 0xff00_0000,
            visible: true,
            ..Level::default()
        }];
        let d = d.normalized();
        assert!(d.levels.is_empty(), "the broken level is dropped");
        let mut line = drawing(2, Tool::TrendLine, 2);
        line.levels = vec![Level {
            value: 1.0,
            color: 0,
            visible: true,
            ..Level::default()
        }];
        assert!(line.normalized().levels.is_empty(), "a line has no levels");
    }

    #[test]
    fn every_new_part_of_a_look_survives_the_saved_file() {
        use super::super::look::{Cap, HAlign, LabelSide, LevelText, VAlign};

        let mut d = drawing(1, Tool::FibRetracement, 2);
        d.text = "the label".to_owned();
        d.style.text_layout.align = HAlign::End;
        d.style.text_layout.valign = VAlign::Bottom;
        d.style.text_layout.background = true;
        d.style.text_layout.background_color = Some(0x123456);
        d.style.caps.start = Cap::Circle;
        d.style.caps.end = Cap::Arrow;
        d.style.measure.percent = false;
        d.style.measure.down_color = 0xabcdef;
        d.style.profile.rows = 60;
        d.style.profile.value_area = 0.5;
        d.style.level_text = LevelText::Percent;
        d.style.label_side = LabelSide::Right;
        d.style.scale = 2.5;
        d.style.position.risk = 3.0;
        let mut levels = d.levels();
        levels[0].width = 3.0;
        levels[1].dash = Some(Dash::Dotted);
        d.levels = levels;

        let mut doc = DrawingsDoc::default();
        doc.symbols.insert("EURUSD".to_owned(), vec![d.clone()]);
        doc.named_templates.push(NamedTemplate {
            name: "Mine".to_owned(),
            tool: Tool::FibRetracement.code(),
            style: d.style.clone(),
            levels: d.levels.clone(),
        });
        let text = toml::to_string_pretty(&doc).unwrap();
        let back: DrawingsDoc = toml::from_str(&text).unwrap();
        let back = back.normalized();
        assert_eq!(back.symbols["EURUSD"][0], d.clone().normalized());
        assert_eq!(back.named_templates.len(), 1);
        assert_eq!(back.named_templates[0].style, d.style.normalized());
        assert_eq!(back.named_templates[0].levels[0].width, 3.0);

        // A drawing left at the defaults writes none of it.
        let plain = drawing(2, Tool::TrendLine, 2);
        let mut doc = DrawingsDoc::default();
        doc.symbols.insert("EURUSD".to_owned(), vec![plain]);
        let text = toml::to_string_pretty(&doc).unwrap();
        for key in [
            "text_layout",
            "caps",
            "measure",
            "profile",
            "level_text",
            "scale",
            "position",
        ] {
            assert!(!text.contains(key), "{key} is written for nothing:\n{text}");
        }
    }

    #[test]
    fn every_tool_has_a_code_and_templates_of_unknown_tools_are_dropped() {
        assert_eq!(Tool::HeadAndShoulders.code(), "head_and_shoulders");
        let mut doc = DrawingsDoc::default();
        doc.templates.insert(
            "gann_fan".into(),
            Template {
                style: Style {
                    width: -3.0,
                    ..Style::default()
                },
                levels: Vec::new(),
            },
        );
        doc.templates.insert(
            "nothing".into(),
            Template {
                style: Style::default(),
                levels: Vec::new(),
            },
        );
        let text = toml::to_string_pretty(&doc).unwrap();
        let back: DrawingsDoc = toml::from_str(&text).unwrap();
        let back = back.normalized();
        assert_eq!(back.templates.len(), 1);
        assert_eq!(back.templates["gann_fan"].style.width, 1.5);
    }

    #[test]
    fn waves_are_named_by_their_degree() {
        assert_eq!(wave_label("3", DEFAULT_DEGREE), "3");
        assert_eq!(wave_label("3", 0), "(III)");
        assert_eq!(wave_label("4", 1), "IV");
        assert_eq!(wave_label("B", 2), "((B))");
        assert_eq!(wave_label("5", 5), "v");
        assert_eq!(wave_label("C", 6), "(c)");
    }
}
