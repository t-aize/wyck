//! The look of the app as plain data: the palette of colors and what is put in force together
//! with it. The themes (their names, what the user changed) are in `crate::app::appearance`; the
//! screens read the palette through `crate::ui::kit::theme`.

/// The colors of the interface and of the chart, as `0xRRGGBB` (the tint of a selection carries
/// its own alpha, as `0xRRGGBBAA`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// The dark palette the app started with.
    pub const WYCK_DARK: Self = Self {
        bg: 0x0a0a0a,
        surface: 0x171717,
        hover: 0x1f1f1f,
        pressed: 0x282828,
        fg: 0xfafafa,
        muted: 0xa1a1a1,
        accent: 0x7c86ff,
        accent_fg: 0x0a0a0a,
        selected: 0x615fff73,
        danger: 0xff6467,
        amber: 0xffb900,
        emerald: 0x00d492,
        up: 0x26a69a,
        down: 0xef5350,
        line: 0x5b8def,
        tag: 0x363a45,
        chart_bg: 0x0a0a0a,
    };
}

impl Colors {
    /// Whether this is a light palette: the background is lighter than the text.
    pub fn is_light(&self) -> bool {
        luminance(self.bg) > luminance(self.fg)
    }

    /// The text colors for what is drawn straight on the chart: the text, the secondary text and
    /// the background of the crosshair's tags. The chart can have a background of its own, so a
    /// dark theme can sit on a white chart: the theme's text is kept while it reads on that
    /// background, and a dark or light one takes over when it does not.
    pub fn chart_text(&self) -> (u32, u32, u32) {
        if contrast(self.fg, self.chart_bg) >= 4.5 && contrast(self.muted, self.chart_bg) >= 3.0 {
            (self.fg, self.muted, self.tag)
        } else {
            let bg = self.chart_bg;
            let light = (0xf0f0f0, 0x9aa0a6, 0x363a45);
            let dark = (0x1f2328, 0x57606a, 0xd0d7de);
            let best = if contrast(dark.0, bg) >= contrast(light.0, bg) {
                dark
            } else {
                light
            };
            if contrast(best.0, bg) >= 4.5 {
                best
            } else if contrast(0x000000, bg) >= contrast(0xffffff, bg) {
                // A background in the middle of the range: plain black or white reads best.
                (0x000000, 0x3a3a3a, dark.2)
            } else {
                (0xffffff, 0xd0d0d0, light.2)
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
    let channel = |shift: u32| {
        let c = ((color >> shift) & 0xff) as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// The least and the most the interface can be scaled to, in percent.
pub const SCALE_MIN: u32 = 80;
/// See [`SCALE_MIN`].
pub const SCALE_MAX: u32 = 160;

/// Everything the interface takes from the user's appearance settings, ready to be put in force.
#[derive(Debug, Clone, PartialEq)]
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
