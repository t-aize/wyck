//! How the app looks, as the user set it: the font, the size of the interface, whether things
//! move, and the colors of the candles. It is saved in a document of its own, so it can be
//! exported, shared and reset apart from everything else.
//!
//! The interface has one theme, the dark palette in [`Colors::WYCK_DARK`]. On top of it come a few
//! overrides for the colors of the chart (the candles, the line, the background).
//! [`resolve`](Appearance::resolve) is the whole decision, with no window in it, so it is tested on
//! its own; [`update`] puts the result in force and saves it.

pub mod candles;

use gpui::{App, Global, SharedString};
use serde::{Deserialize, Serialize};
use wyck_storage::DocumentStore;

use crate::workspace::Saver;

use wyck_core::appearance::{Colors, Look, SCALE_MAX, SCALE_MIN};

/// The name of the document.
pub const DOCUMENT: &str = "appearance";
/// The font the app ships with.
pub const DEFAULT_FONT: &str = "Inter";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    /// Overrides of the colors of the chart. `None` leaves the palette's own color.
    #[serde(default)]
    pub candle_up: Option<u32>,
    #[serde(default)]
    pub candle_down: Option<u32>,
    #[serde(default)]
    pub chart_line: Option<u32>,
    #[serde(default)]
    pub chart_background: Option<u32>,
    /// The font of the interface: a family name the system has, or the one that ships with the app.
    #[serde(default = "default_font")]
    pub font: String,
    /// Whether screens and panels move as they appear.
    #[serde(default = "wyck_core::defaults::yes")]
    pub animations: bool,
    /// The size of the interface, in percent of the size it was drawn at: text, controls and
    /// fields. For eyes that need larger text, or a small screen.
    #[serde(default = "default_ui_scale")]
    pub ui_scale: u32,
}

fn default_ui_scale() -> u32 {
    100
}

fn default_font() -> String {
    DEFAULT_FONT.to_owned()
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            candle_up: None,
            candle_down: None,
            chart_line: None,
            chart_background: None,
            font: default_font(),
            animations: true,
            ui_scale: default_ui_scale(),
        }
    }
}

impl Appearance {
    /// The settings repaired: colors in range, a size in range, a font that says something. A file
    /// from before there was one theme still loads: what it said about themes is ignored.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        for color in [
            &mut self.candle_up,
            &mut self.candle_down,
            &mut self.chart_line,
            &mut self.chart_background,
        ] {
            *color = color.map(|c| c & 0xff_ffff);
        }
        self.ui_scale = self.ui_scale.clamp(SCALE_MIN, SCALE_MAX);
        self.font = self.font.trim().chars().take(80).collect();
        if self.font.is_empty() {
            self.font = default_font();
        }
        self
    }

    /// The palette in force: the dark palette, with the colors of the chart the user picked.
    pub fn resolve(&self) -> Colors {
        let mut colors = Colors::WYCK_DARK;
        if let Some(up) = self.candle_up {
            colors.up = up;
        }
        if let Some(down) = self.candle_down {
            colors.down = down;
        }
        if let Some(line) = self.chart_line {
            colors.line = line;
        }
        if let Some(background) = self.chart_background {
            colors.chart_bg = background;
        }
        colors
    }

    /// Whether anything is changed from what a first launch looks like.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

struct State {
    /// Puts a look in force on screen; given by the UI at startup.
    apply: fn(&Look, &mut App),
    appearance: Appearance,
    /// Writes the look a moment after the last change, and when the app closes.
    saver: Saver<Appearance>,
}

impl Global for State {}

/// Loads the saved look and puts it in force. Call once at startup, before a window opens, so the
/// first frame is already the right one.
pub fn init(store: DocumentStore, apply: fn(&Look, &mut App), cx: &mut App) {
    let appearance = store.load_or_default::<Appearance>(DOCUMENT).normalized();
    cx.set_global(State {
        apply,
        appearance,
        saver: Saver::new(store, DOCUMENT),
    });
    // A change made in the last moments is written when the app closes.
    cx.on_app_quit(|cx| {
        save_now(cx);
        async {}
    })
    .detach();
    put_in_force(cx);
}

/// The settings in force.
pub fn get(cx: &App) -> Appearance {
    cx.try_global::<State>()
        .map(|state| state.appearance.clone())
        .unwrap_or_default()
}

/// The font of the interface.
pub fn font(cx: &App) -> SharedString {
    cx.try_global::<State>().map_or_else(
        || SharedString::from(DEFAULT_FONT),
        |s| s.appearance.font.clone().into(),
    )
}

/// Changes the settings, puts the result in force and saves it a moment later.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Appearance)) {
    if !cx.has_global::<State>() {
        return;
    }
    let state = cx.global_mut::<State>();
    let mut next = state.appearance.clone();
    change(&mut next);
    let next = next.normalized();
    if next == state.appearance {
        return;
    }
    state.saver.schedule(next.clone());
    state.appearance = next;
    put_in_force(cx);
}

fn put_in_force(cx: &mut App) {
    let (look, apply) = {
        let state = cx.global::<State>();
        (
            Look {
                colors: state.appearance.resolve(),
                animations: state.appearance.animations,
                font: state.appearance.font.clone(),
                scale: state.appearance.ui_scale,
            },
            state.apply,
        )
    };
    apply(&look, cx);
}

/// Writes the settings now, for what is about to read the file (a backup).
pub fn save_now(cx: &mut App) {
    if let Some(state) = cx.try_global::<State>() {
        state.saver.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_size_is_kept_in_range_and_old_files_get_the_default() {
        let old: Appearance = toml::from_str("mode = \"light\"").unwrap();
        assert_eq!(old.normalized().ui_scale, 100);
        let huge = Appearance {
            ui_scale: 900,
            ..Appearance::default()
        };
        assert_eq!(huge.normalized().ui_scale, SCALE_MAX);
        let tiny = Appearance {
            ui_scale: 3,
            ..Appearance::default()
        };
        assert_eq!(tiny.normalized().ui_scale, SCALE_MIN);
    }

    #[test]
    fn a_file_from_the_days_of_themes_still_loads_with_what_it_kept() {
        let old: Appearance = toml::from_str(
            "mode = \"light\"\ndark_theme = \"nord\"\naccent = 255\ncandle_up = 65280\nfont = \"Segoe UI\"\nanimations = false\n[[custom_themes]]\nid = \"custom-x\"\nname = \"X\"\n",
        )
        .unwrap();
        let old = old.normalized();
        assert_eq!(old.candle_up, Some(65280));
        assert_eq!(old.font, "Segoe UI");
        assert!(!old.animations);
        assert_eq!(old.resolve().bg, Colors::WYCK_DARK.bg, "the one theme");
        assert_eq!(old.resolve().accent, Colors::WYCK_DARK.accent);
    }

    #[test]
    fn the_chart_overrides_sit_on_top_of_the_palette() {
        let mut a = Appearance::default();
        assert_eq!(a.resolve(), Colors::WYCK_DARK);
        a.candle_up = Some(0x00ff00);
        a.candle_down = Some(0x0000ff);
        a.chart_line = Some(0x123456);
        a.chart_background = Some(0x101010);
        let c = a.resolve();
        assert_eq!(
            (c.up, c.down, c.line, c.chart_bg),
            (0x00ff00, 0x0000ff, 0x123456, 0x101010)
        );
        // The rest is the palette's.
        assert_eq!((c.bg, c.fg), (Colors::WYCK_DARK.bg, Colors::WYCK_DARK.fg));
    }

    #[test]
    fn normalizing_repairs_what_a_file_can_hold() {
        let a = Appearance {
            candle_up: Some(0xff12_3456),
            font: "   ".to_owned(),
            ..Appearance::default()
        }
        .normalized();
        assert_eq!(a.candle_up, Some(0x12_3456), "a color keeps its six digits");
        assert_eq!(a.font, DEFAULT_FONT);
        let long = Appearance {
            font: "x".repeat(300),
            ..Appearance::default()
        }
        .normalized();
        assert_eq!(long.font.len(), 80);
    }

    #[test]
    fn an_empty_or_partial_file_gives_the_defaults() {
        let empty: Appearance = toml::from_str("").unwrap();
        assert_eq!(empty, Appearance::default());
        let partial: Appearance = toml::from_str("font = \"Segoe UI\"").unwrap();
        assert_eq!(partial.font, "Segoe UI");
        assert!(partial.animations);
        assert_eq!(partial.ui_scale, 100);
    }

    #[test]
    fn a_look_survives_the_saved_file() {
        let a = Appearance {
            candle_up: Some(0x112233),
            chart_background: Some(0x000000),
            font: "Segoe UI".to_owned(),
            animations: false,
            ui_scale: 130,
            ..Appearance::default()
        };
        let text = toml::to_string(&a).unwrap();
        let back: Appearance = toml::from_str(&text).unwrap();
        assert_eq!(back.normalized(), a);
    }
}
