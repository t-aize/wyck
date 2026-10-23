//! The look of the app as plain data: the palette of colors and what is put in force together
//! with it.

/// The colors of the interface and of the chart, as `0xRRGGBB` (the tint of a selection carries
/// its own alpha, as `0xRRGGBBAA`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    /// Window and page background. Also the color of an unfocused text input.
    pub bg: u32,
    /// Raised surface: cards, panels, list rows.
    pub surface: u32,
    /// A row or an option while the pointer is over it, and while pressed.
    pub hover: u32,
    pub pressed: u32,
    /// Primary text, and secondary text (descriptions, hints, timestamps).
    pub fg: u32,
    pub muted: u32,
    /// The action color: buttons, active icons, links, focus rings. And what sits on top of it.
    pub accent: u32,
    pub accent_fg: u32,
    /// The tint of the selected segment of a control, with its alpha.
    pub selected: u32,
    /// Error, warning and success.
    pub danger: u32,
    pub amber: u32,
    pub emerald: u32,
    /// The chart: a rising and a falling candle, a line, the background of the crosshair's tags,
    /// and the background of the chart itself.
    pub up: u32,
    pub down: u32,
    pub line: u32,
    pub tag: u32,
    pub chart_bg: u32,
}

impl Colors {
    /// The one palette of the interface: dark.
    pub const WYCK_DARK: Self = Self {
        bg: 0x0a_0a0a,
        surface: 0x17_1717,
        hover: 0x1f_1f1f,
        pressed: 0x28_2828,
        fg: 0xfa_fafa,
        muted: 0xa1_a1a1,
        accent: 0x7c_86ff,
        accent_fg: 0x0a_0a0a,
        selected: 0x615f_ff73,
        danger: 0xff_6467,
        amber: 0xff_b900,
        emerald: 0x00_d492,
        up: 0x26_a69a,
        down: 0xef_5350,
        line: 0x5b_8def,
        tag: 0x36_3a45,
        chart_bg: 0x0a_0a0a,
    };

    /// The text colors for what is drawn straight on the chart: the text, the secondary text and
    /// the background of the crosshair's tags. The chart can have a background of its own, so a
    /// dark theme can sit on a white chart: the theme's text is kept while it reads on that
    /// background, and a dark or light one takes over when it does not.
    pub fn chart_text(&self) -> (u32, u32, u32) {
        if contrast(self.fg, self.chart_bg) >= 4.5 && contrast(self.muted, self.chart_bg) >= 3.0 {
            (self.fg, self.muted, self.tag)
        } else {
            let bg = self.chart_bg;
            let light = (0xf0_f0f0, 0x9a_a0a6, 0x36_3a45);
            let dark = (0x1f_2328, 0x57_606a, 0xd0_d7de);
            let best = if contrast(dark.0, bg) >= contrast(light.0, bg) {
                dark
            } else {
                light
            };
            if contrast(best.0, bg) >= 4.5 {
                best
            } else if contrast(0x00_0000, bg) >= contrast(0xff_ffff, bg) {
                // A background in the middle of the range: plain black or white reads best.
                (0x00_0000, 0x3a_3a3a, dark.2)
            } else {
                (0xff_ffff, 0xd0_d0d0, light.2)
            }
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self::WYCK_DARK
    }
}

/// The contrast ratio of two `0xRRGGBB` colors, from 1 to 21.
pub fn contrast(a: u32, b: u32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The relative luminance of a `0xRRGGBB` color, from 0 (black) to 1 (white).
pub fn luminance(color: u32) -> f32 {
    let [_, red, green, blue] = color.to_be_bytes();
    let channel = |byte: u8| {
        let c = f32::from(byte) / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
}

/// The least and the most the interface can be scaled to, in percent.
pub const SCALE_MIN: u32 = 80;
/// See [`SCALE_MIN`].
pub const SCALE_MAX: u32 = 160;

/// Everything the interface takes from the user's appearance settings, ready to be put in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    /// The palette.
    pub colors: Colors,
    /// Whether things fade and slide.
    pub animations: bool,
    /// The font family of the interface.
    pub font: String,
    /// The size of the interface, in percent, between [`SCALE_MIN`] and [`SCALE_MAX`].
    pub scale: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luminance_runs_from_black_to_white() {
        assert!(luminance(0x00_0000) < 0.001);
        assert!(luminance(0xff_ffff) > 0.99);
        assert!(luminance(0x80_8080) > luminance(0x20_2020));
    }

    #[test]
    fn contrast_is_one_for_equal_colors_and_twenty_one_for_black_on_white() {
        assert!((contrast(0x33_6699, 0x33_6699) - 1.0).abs() < 1e-6);
        assert!((contrast(0x00_0000, 0xff_ffff) - 21.0).abs() < 1e-3);
        assert_eq!(
            contrast(0x00_0000, 0xff_ffff),
            contrast(0xff_ffff, 0x00_0000),
            "the order does not matter"
        );
    }
}
