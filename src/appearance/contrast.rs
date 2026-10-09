//! Contrast checks for a palette, after WCAG 2.2: text needs 4.5:1 against what it sits on, and
//! the parts of a control and the marks of a chart 3:1. These are the rules every theme that comes
//! with the app is tested against (see [`super::presets`]); a theme of the user's is held to them
//! by showing what falls short and offering to put it right.
//!
//! Nothing here refuses a color: a user may want a low contrast theme, and it is theirs. The
//! backgrounds are never touched by [`fix`]: it moves the colors drawn on them.

use wyck_ui::theme::{Colors, contrast};

use super::ColorField;

/// One pair of colors that has to be told apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// What is drawn: `Text`, `Secondary text`...
    pub what: &'static str,
    pub fg: ColorField,
    pub bg: ColorField,
    /// The contrast it needs.
    pub least: f32,
}

const fn rule(what: &'static str, fg: ColorField, bg: ColorField, least: f32) -> Rule {
    Rule {
        what,
        fg,
        bg,
        least,
    }
}

use ColorField::{Accent, Amber, Bg, ChartBg, Danger, Down, Emerald, Fg, Line, Muted, Surface, Up};

/// Every pair checked. The text on the accent is not here: it is chosen (black or white) with the
/// accent, so it always reads.
pub const RULES: [Rule; 15] = [
    rule("Text on the background", Fg, Bg, 4.5),
    rule("Text on panels", Fg, Surface, 4.5),
    rule("Secondary text on the background", Muted, Bg, 4.5),
    rule("Secondary text on panels", Muted, Surface, 4.5),
    rule("Error text on the background", Danger, Bg, 4.5),
    rule("Error text on panels", Danger, Surface, 4.5),
    rule("Warning text on the background", Amber, Bg, 4.5),
    rule("Warning text on panels", Amber, Surface, 4.5),
    rule("Success text on the background", Emerald, Bg, 4.5),
    rule("Success text on panels", Emerald, Surface, 4.5),
    rule("The accent on the background", Accent, Bg, 3.0),
    rule("The accent on panels", Accent, Surface, 3.0),
    rule("Rising candles on the chart", Up, ChartBg, 3.0),
    rule("Falling candles on the chart", Down, ChartBg, 3.0),
    rule("The chart line", Line, ChartBg, 3.0),
];

/// What a rule found in a palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Finding {
    pub rule: Rule,
    pub ratio: f32,
}

impl Finding {
    pub fn ok(&self) -> bool {
        self.ratio >= self.rule.least
    }

    /// `Text on panels is 3.2:1, needs 4.5:1`.
    pub fn describe(&self) -> String {
        format!(
            "{} is {:.1}:1, needs {:.1}:1",
            self.rule.what, self.ratio, self.rule.least
        )
    }
}

/// Every rule, with the contrast the palette gives it.
pub fn check(colors: &Colors) -> Vec<Finding> {
    RULES
        .iter()
        .map(|rule| Finding {
            rule: *rule,
            ratio: contrast(rule.fg.get(colors), rule.bg.get(colors)),
        })
        .collect()
}

/// The rules the palette does not meet.
pub fn failing(colors: &Colors) -> Vec<Finding> {
    check(colors).into_iter().filter(|f| !f.ok()).collect()
}

/// The rules a color is part of that the palette does not meet: what to say under its swatch.
pub fn failing_for(colors: &Colors, field: ColorField) -> Vec<Finding> {
    failing(colors)
        .into_iter()
        .filter(|f| f.rule.fg == field || f.rule.bg == field)
        .collect()
}

/// A color moved `t` of the way to another, both as `0xRRGGBB`.
fn mix(from: u32, to: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let (a, b) = (
            f32::from(((from >> shift) & 0xff) as u8),
            f32::from(((to >> shift) & 0xff) as u8),
        );
        (a + (b - a) * t).round().clamp(0.0, 255.0) as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// The fewest steps of `t` (in hundredths) toward `target` that make `color` meet every rule
/// against the backgrounds given, or `None` when even the target does not.
fn steps_to_meet(color: u32, target: u32, needs: &[(u32, f32)]) -> Option<u32> {
    (0..=100).find(|step| {
        let moved = mix(color, target, *step as f32 / 100.0);
        needs
            .iter()
            .all(|(bg, least)| contrast(moved, *bg) >= *least)
    })
}

/// Moves the colors drawn on the backgrounds, as little as it takes, toward white or black until
/// they meet the rules. Returns which colors moved. The backgrounds stay as they are.
pub fn fix(colors: &mut Colors) -> Vec<ColorField> {
    let mut moved = Vec::new();
    let fields = [Fg, Muted, Danger, Amber, Emerald, Accent, Up, Down, Line];
    for field in fields {
        let needs: Vec<(u32, f32)> = RULES
            .iter()
            .filter(|r| r.fg == field)
            .map(|r| (r.bg.get(colors), r.least))
            .collect();
        let color = field.get(colors);
        if needs
            .iter()
            .all(|(bg, least)| contrast(color, *bg) >= *least)
        {
            continue;
        }
        // Toward whichever end gets there in the fewest steps; white on a tie when the
        // backgrounds are dark and black when they are light.
        let toward_white = steps_to_meet(color, 0xffffff, &needs);
        let toward_black = steps_to_meet(color, 0x000000, &needs);
        let dark_bg = needs
            .iter()
            .all(|(bg, _)| wyck_ui::theme::luminance(*bg) < 0.18);
        let target = match (toward_white, toward_black) {
            (Some(w), Some(b)) if w < b => Some((0xffffff, w)),
            (Some(w), Some(b)) if b < w => Some((0x000000, b)),
            (Some(w), Some(_)) => Some(if dark_bg {
                (0xffffff, w)
            } else {
                (0x000000, w)
            }),
            (Some(w), None) => Some((0xffffff, w)),
            (None, Some(b)) => Some((0x000000, b)),
            (None, None) => None,
        };
        if let Some((end, step)) = target {
            field.set(colors, mix(color, end, step as f32 / 100.0));
            moved.push(field);
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::presets::PRESETS;

    #[test]
    fn every_theme_that_comes_with_the_app_meets_every_rule() {
        for preset in PRESETS {
            let failures: Vec<String> = failing(&preset.colors)
                .iter()
                .map(Finding::describe)
                .collect();
            assert!(failures.is_empty(), "{}: {failures:?}", preset.name);
        }
    }

    fn poor() -> Colors {
        // Grey on grey: nothing reads.
        let mut colors = wyck_ui::theme::Colors::WYCK_DARK;
        colors.bg = 0x404040;
        colors.surface = 0x484848;
        colors.chart_bg = 0x404040;
        ColorField::Fg.set(&mut colors, 0x606060);
        colors.muted = 0x555555;
        colors.up = 0x505050;
        colors.line = 0x4a4a4a;
        colors
    }

    #[test]
    fn a_palette_that_does_not_read_is_reported_color_by_color() {
        let colors = poor();
        let failing = failing(&colors);
        assert!(failing.len() >= 4, "{failing:?}");
        assert!(failing.iter().all(|f| !f.ok()));
        let for_text = failing_for(&colors, ColorField::Fg);
        assert!(!for_text.is_empty());
        assert!(
            for_text
                .iter()
                .all(|f| f.rule.fg == ColorField::Fg || f.rule.bg == ColorField::Fg)
        );
        assert!(for_text[0].describe().contains(":1, needs"));
        // A color no rule mentions is never reported.
        assert!(failing_for(&colors, ColorField::Tag).is_empty());
    }

    #[test]
    fn fixing_moves_the_colors_drawn_and_never_the_backgrounds() {
        let mut colors = poor();
        let before = colors;
        let moved = fix(&mut colors);
        assert!(moved.contains(&ColorField::Fg));
        assert!(failing(&colors).is_empty(), "{:?}", failing(&colors));
        assert_eq!(
            (colors.bg, colors.surface, colors.chart_bg),
            (before.bg, before.surface, before.chart_bg)
        );
        // Nothing left to do the second time.
        assert!(fix(&mut colors).is_empty());
    }

    #[test]
    fn a_color_that_already_reads_is_left_alone() {
        let mut colors = wyck_ui::theme::Colors::WYCK_DARK;
        let before = colors;
        assert!(fix(&mut colors).is_empty());
        assert_eq!(colors, before);
    }

    #[test]
    fn a_moved_accent_brings_readable_text_along() {
        let mut colors = wyck_ui::theme::Colors::WYCK_DARK;
        colors.bg = 0x808080;
        colors.surface = 0x808080;
        ColorField::Accent.set(&mut colors, 0x858585);
        fix(&mut colors);
        assert!(contrast(colors.accent_fg, colors.accent) >= 4.5);
    }

    #[test]
    fn colors_mix_channel_by_channel() {
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
        assert_eq!(mix(0x102030, 0x102030, 1.0), 0x102030);
        assert_eq!(mix(0xff0000, 0x0000ff, 1.0), 0x0000ff);
    }
}
