//! The colors a chart is drawn with.

use super::color::{Rgba, rgb};
use super::{GRID_ALPHA, TPO_IB, TPO_MARKS, TPO_POC, TPO_SINGLE, TPO_VALUE_AREA};
use crate::options::ChartColors;

/// The colors the chart draws with.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub up: Rgba,
    pub down: Rgba,
    pub line: Rgba,
    pub grid: Rgba,
    pub text: Rgba,
    pub text_strong: Rgba,
    pub bg: Rgba,
    pub tag: Rgba,
    pub border: Rgba,
    pub crosshair: Rgba,
    pub accent: Rgba,
    /// The thick and thin lines of Kagi.
    pub kagi_yang: Rgba,
    pub kagi_yin: Rgba,
    /// The X and the O of point and figure.
    pub pnf_up: Rgba,
    pub pnf_down: Rgba,
    /// The marks, the point of control, the value area, the initial balance and the single
    /// prints of a TPO profile.
    pub tpo: Rgba,
    pub tpo_poc: Rgba,
    pub tpo_value_area: Rgba,
    pub tpo_ib: Rgba,
    pub tpo_single: Rgba,
}

impl Palette {
    pub fn new() -> Self {
        Self {
            up: rgb(0x26_a69a),
            down: rgb(0xef_5350),
            line: rgb(0x5b_8def),
            grid: rgb(0x30_343b),
            text: rgb(0x9a_a0a6),
            text_strong: rgb(0xf0_f0f0),
            bg: rgb(0x0a_0a0a),
            tag: rgb(0x36_3a45),
            border: rgb(0x30_343b),
            crosshair: rgb(0x9a_a0a6),
            accent: rgb(0x7c_86ff),
            kagi_yang: rgb(0x26_a69a),
            kagi_yin: rgb(0xef_5350),
            pnf_up: rgb(0x26_a69a),
            pnf_down: rgb(0xef_5350),
            tpo: rgb(TPO_MARKS),
            tpo_poc: rgb(TPO_POC),
            tpo_value_area: rgb(TPO_VALUE_AREA),
            tpo_ib: rgb(TPO_IB),
            tpo_single: rgb(TPO_SINGLE),
        }
    }

    /// Applies per-chart colors to a frontend-supplied palette.
    pub fn for_chart_with_base(colors: &ChartColors, mut palette: Self) -> Self {
        let pick = |color: u32| rgb(color);
        if let Some(c) = colors.up {
            palette.up = pick(c);
        }
        if let Some(c) = colors.down {
            palette.down = pick(c);
        }
        // What follows the rising and falling colors, unless it has a color of its own.
        (palette.kagi_yang, palette.kagi_yin) = (palette.up, palette.down);
        (palette.pnf_up, palette.pnf_down) = (palette.up, palette.down);
        if let Some(c) = colors.kagi_yang {
            palette.kagi_yang = pick(c);
        }
        if let Some(c) = colors.kagi_yin {
            palette.kagi_yin = pick(c);
        }
        if let Some(c) = colors.pnf_up {
            palette.pnf_up = pick(c);
        }
        if let Some(c) = colors.pnf_down {
            palette.pnf_down = pick(c);
        }
        for (own, slot) in [
            (colors.tpo, &mut palette.tpo),
            (colors.tpo_poc, &mut palette.tpo_poc),
            (colors.tpo_value_area, &mut palette.tpo_value_area),
            (colors.tpo_ib, &mut palette.tpo_ib),
            (colors.tpo_single, &mut palette.tpo_single),
        ] {
            if let Some(c) = own {
                *slot = pick(c);
            }
        }
        if let Some(c) = colors.line {
            palette.line = pick(c);
        }
        if let Some(c) = colors.background {
            palette.bg = pick(c);
        }
        if let Some(c) = colors.grid {
            // The grid sits behind the prices: a chosen color is drawn faint, like the theme's.
            palette.grid = Rgba {
                a: GRID_ALPHA,
                ..pick(c)
            };
        }
        if let Some(c) = colors.crosshair {
            palette.crosshair = pick(c);
        }
        if let Some(c) = colors.text {
            palette.text = pick(c);
            palette.text_strong = pick(c);
        }
        palette
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {

    use crate::scene::color::rgb;

    use super::*;

    use crate::options::ChartColors;

    #[test]
    fn trading_lines_that_are_hidden_are_not_marks() {
        let settings = crate::options::TradingLines {
            alerts: false,
            ..crate::options::TradingLines::default()
        };
        assert!(!settings.shows(crate::lines::LineId::Alert(3)));
        assert!(settings.shows(crate::lines::LineId::Order(3)));
    }

    #[test]
    fn kagi_and_point_figure_take_their_own_colors_and_follow_the_rising_ones_otherwise() {
        let plain = Palette::for_chart_with_base(&ChartColors::default(), Palette::new());
        assert_eq!(plain.kagi_yang, plain.up);
        assert_eq!(plain.kagi_yin, plain.down);
        assert_eq!(plain.pnf_up, plain.up);
        let colors = ChartColors {
            up: Some(0x11_2233),
            kagi_yin: Some(0x44_5566),
            pnf_up: Some(0x77_8899),
            ..ChartColors::default()
        };
        let p = Palette::for_chart_with_base(&colors, Palette::new());
        assert_eq!(p.kagi_yang, rgb(0x11_2233), "follows the rising color");
        assert_eq!(p.kagi_yin, rgb(0x44_5566));
        assert_eq!(p.pnf_up, rgb(0x77_8899));
        assert_eq!(p.pnf_down, p.down);
    }
}
