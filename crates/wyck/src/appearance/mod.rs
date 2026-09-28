//! How the app looks, as the user set it: the theme, the accent, the colors of the candles, the
//! font, whether things move. It is saved in a document of its own, so it can be exported, shared
//! and reset apart from everything else.
//!
//! A theme is a [`Colors`] palette. The ones that come with the app are in [`presets`]; the user
//! can also copy one, rename it and change any of its colors, and those copies are kept here.
//! On top of the theme come a few overrides that hold across themes (the accent, the candle colors,
//! the background of the chart), so changing the theme does not lose the candles the user picked.
//!
//! The mode says which theme is in force: a dark one, a light one, or the one the system asks for.
//! [`resolve`](Appearance::resolve) is the whole decision, with no window in it, so it is tested
//! on its own; [`update`] puts the result in force and saves it.

pub mod presets;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{App, Global, SharedString, Task};
use serde::{Deserialize, Serialize};
use wyck_config::DocumentStore;

use super::theme::{self, Colors};
use presets::{DEFAULT_DARK, DEFAULT_LIGHT, PRESETS};

/// The name of the document.
pub const DOCUMENT: &str = "appearance";
/// How many themes of their own a user can keep.
pub const MAX_CUSTOM_THEMES: usize = 30;
/// The font the app ships with.
pub const DEFAULT_FONT: &str = "Inter";
/// How long a change waits for another before it is written.
const SAVE_DELAY: Duration = Duration::from_millis(400);

/// Which theme is in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The theme chosen for what the system is set to: dark at night, light by day.
    System,
    Light,
    #[default]
    Dark,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

/// A theme the user made from another one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub colors: Colors,
}

/// One color of a palette, for the editor of a theme to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorField {
    Bg,
    Surface,
    Hover,
    Pressed,
    Fg,
    Muted,
    Accent,
    Danger,
    Amber,
    Emerald,
    Up,
    Down,
    Line,
    Tag,
    ChartBg,
}

impl ColorField {
    pub const ALL: [Self; 15] = [
        Self::Bg,
        Self::Surface,
        Self::Hover,
        Self::Pressed,
        Self::Fg,
        Self::Muted,
        Self::Accent,
        Self::Danger,
        Self::Amber,
        Self::Emerald,
        Self::Up,
        Self::Down,
        Self::Line,
        Self::Tag,
        Self::ChartBg,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bg => "Background",
            Self::Surface => "Panels",
            Self::Hover => "Hovered row",
            Self::Pressed => "Pressed row",
            Self::Fg => "Text",
            Self::Muted => "Secondary text",
            Self::Accent => "Accent",
            Self::Danger => "Error",
            Self::Amber => "Warning",
            Self::Emerald => "Success",
            Self::Up => "Rising candle",
            Self::Down => "Falling candle",
            Self::Line => "Chart line",
            Self::Tag => "Crosshair tag",
            Self::ChartBg => "Chart background",
        }
    }

    pub fn get(self, colors: &Colors) -> u32 {
        match self {
            Self::Bg => colors.bg,
            Self::Surface => colors.surface,
            Self::Hover => colors.hover,
            Self::Pressed => colors.pressed,
            Self::Fg => colors.fg,
            Self::Muted => colors.muted,
            Self::Accent => colors.accent,
            Self::Danger => colors.danger,
            Self::Amber => colors.amber,
            Self::Emerald => colors.emerald,
            Self::Up => colors.up,
            Self::Down => colors.down,
            Self::Line => colors.line,
            Self::Tag => colors.tag,
            Self::ChartBg => colors.chart_bg,
        }
    }

    /// Sets the color. The accent brings the text on it and the tint of a selection along.
    pub fn set(self, colors: &mut Colors, value: u32) {
        let value = value & 0xff_ffff;
        match self {
            Self::Bg => colors.bg = value,
            Self::Surface => colors.surface = value,
            Self::Hover => colors.hover = value,
            Self::Pressed => colors.pressed = value,
            Self::Fg => colors.fg = value,
            Self::Muted => colors.muted = value,
            Self::Accent => with_accent(colors, value),
            Self::Danger => colors.danger = value,
            Self::Amber => colors.amber = value,
            Self::Emerald => colors.emerald = value,
            Self::Up => colors.up = value,
            Self::Down => colors.down = value,
            Self::Line => colors.line = value,
            Self::Tag => colors.tag = value,
            Self::ChartBg => colors.chart_bg = value,
        }
    }
}

/// Puts `accent` in a palette, with the text that reads on it and the tint of a selection.
fn with_accent(colors: &mut Colors, accent: u32) {
    colors.accent = accent;
    colors.accent_fg = if theme::luminance(accent) > 0.4 {
        0x0a0a0a
    } else {
        0xffffff
    };
    colors.selected = (accent << 8) | 0x66;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Appearance {
    #[serde(default)]
    pub mode: Mode,
    /// The theme in force in dark mode, and in light mode, by id. Any theme can go in either.
    #[serde(default = "default_dark")]
    pub dark_theme: String,
    #[serde(default = "default_light")]
    pub light_theme: String,
    /// Overrides that hold across themes. `None` leaves the theme's own color.
    #[serde(default)]
    pub accent: Option<u32>,
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
    #[serde(default = "yes")]
    pub animations: bool,
    /// The themes the user made.
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
}

fn default_dark() -> String {
    DEFAULT_DARK.to_owned()
}

fn default_light() -> String {
    DEFAULT_LIGHT.to_owned()
}

fn default_font() -> String {
    DEFAULT_FONT.to_owned()
}

fn yes() -> bool {
    true
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            dark_theme: default_dark(),
            light_theme: default_light(),
            accent: None,
            candle_up: None,
            candle_down: None,
            chart_line: None,
            chart_background: None,
            font: default_font(),
            animations: true,
            custom_themes: Vec::new(),
        }
    }
}

impl Appearance {
    /// The settings repaired: themes that exist, each id once, colors in range, a font that
    /// says something.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let mut seen: Vec<String> = Vec::new();
        let mut kept = Vec::new();
        for mut theme in std::mem::take(&mut self.custom_themes) {
            theme.name = theme.name.trim().chars().take(40).collect();
            theme.id = theme.id.trim().to_owned();
            if theme.name.is_empty()
                || theme.id.is_empty()
                || seen.contains(&theme.id)
                || presets::preset(&theme.id).is_some()
            {
                continue;
            }
            for field in ColorField::ALL {
                let value = field.get(&theme.colors);
                field.set(&mut theme.colors, value);
            }
            theme.colors.selected &= u32::MAX;
            seen.push(theme.id.clone());
            kept.push(theme);
            if kept.len() >= MAX_CUSTOM_THEMES {
                break;
            }
        }
        self.custom_themes = kept;
        if self.find(&self.dark_theme).is_none() {
            self.dark_theme = default_dark();
        }
        if self.find(&self.light_theme).is_none() {
            self.light_theme = default_light();
        }
        for color in [
            &mut self.accent,
            &mut self.candle_up,
            &mut self.candle_down,
            &mut self.chart_line,
            &mut self.chart_background,
        ] {
            *color = color.map(|c| c & 0xff_ffff);
        }
        self.font = self.font.trim().chars().take(80).collect();
        if self.font.is_empty() {
            self.font = default_font();
        }
        self
    }

    /// The name and colors of the theme under `id`: one of the user's, or one that comes with
    /// the app.
    pub fn find(&self, id: &str) -> Option<(String, Colors)> {
        if let Some(custom) = self.custom_themes.iter().find(|t| t.id == id) {
            return Some((custom.name.clone(), custom.colors));
        }
        presets::preset(id).map(|p| (p.name.to_owned(), p.colors))
    }

    /// The id of the theme in force, given what the system is set to.
    pub fn active_id(&self, system_dark: bool) -> &str {
        match self.mode {
            Mode::Dark => &self.dark_theme,
            Mode::Light => &self.light_theme,
            Mode::System if system_dark => &self.dark_theme,
            Mode::System => &self.light_theme,
        }
    }

    /// The palette in force: the theme in force, with the overrides on top.
    pub fn resolve(&self, system_dark: bool) -> Colors {
        let mut colors = self
            .find(self.active_id(system_dark))
            .map_or_else(Colors::default, |(_, colors)| colors);
        if let Some(accent) = self.accent {
            with_accent(&mut colors, accent);
        }
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

    /// Makes a theme of the user's from the one under `from`, under `name`. Returns its id, or
    /// `None` when there is no such theme, the name says nothing, or the user has as many as
    /// they can keep.
    pub fn copy_theme(&mut self, from: &str, name: &str) -> Option<String> {
        let name: String = name.trim().chars().take(40).collect();
        let (_, colors) = self.find(from)?;
        if name.is_empty() || self.custom_themes.len() >= MAX_CUSTOM_THEMES {
            return None;
        }
        let id = self.unused_id(&name);
        self.custom_themes.push(CustomTheme {
            id: id.clone(),
            name,
            colors,
        });
        Some(id)
    }

    /// An id made from `name` that no theme has.
    fn unused_id(&self, name: &str) -> String {
        let base: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_owned();
        let base = if base.is_empty() {
            "theme".to_owned()
        } else {
            base
        };
        let base = format!("custom-{base}");
        let mut id = base.clone();
        let mut n = 2;
        while self.find(&id).is_some() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Deletes a theme of the user's. A slot that used it goes back to the default. Returns
    /// whether there was one.
    pub fn delete_theme(&mut self, id: &str) -> bool {
        let before = self.custom_themes.len();
        self.custom_themes.retain(|t| t.id != id);
        if self.custom_themes.len() == before {
            return false;
        }
        if self.dark_theme == id {
            self.dark_theme = default_dark();
        }
        if self.light_theme == id {
            self.light_theme = default_light();
        }
        true
    }

    /// Renames a theme of the user's.
    pub fn rename_theme(&mut self, id: &str, name: &str) -> bool {
        let name: String = name.trim().chars().take(40).collect();
        if name.is_empty() {
            return false;
        }
        match self.custom_themes.iter_mut().find(|t| t.id == id) {
            Some(theme) if theme.name != name => {
                theme.name = name;
                true
            }
            _ => false,
        }
    }

    /// Changes one color of a theme of the user's (the ones that come with the app are not
    /// edited: copy one first).
    pub fn set_theme_color(&mut self, id: &str, field: ColorField, value: u32) -> bool {
        match self.custom_themes.iter_mut().find(|t| t.id == id) {
            Some(theme) if field.get(&theme.colors) != value & 0xff_ffff => {
                field.set(&mut theme.colors, value);
                true
            }
            _ => false,
        }
    }

    /// Every theme, the user's first, as id and name.
    pub fn themes(&self) -> Vec<(String, String)> {
        self.custom_themes
            .iter()
            .map(|t| (t.id.clone(), t.name.clone()))
            .chain(PRESETS.iter().map(|p| (p.id.to_owned(), p.name.to_owned())))
            .collect()
    }

    /// Whether anything is changed from what a first launch looks like.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

// ---- in force ----

/// Whether things move. Read by the animation helpers, which have no window at hand.
static ANIMATIONS: AtomicBool = AtomicBool::new(true);

/// Whether screens and panels move as they appear.
pub fn animations() -> bool {
    ANIMATIONS.load(Ordering::Relaxed)
}

struct State {
    appearance: Appearance,
    system_dark: bool,
    store: Option<DocumentStore>,
    /// Counts the changes, so a write that waited knows whether a newer change is coming.
    revision: u64,
    _save: Option<Task<()>>,
}

impl Global for State {}

/// Loads the saved look and puts it in force. Call once at startup, before a window opens, so the
/// first frame is already the right one.
pub fn init(store: DocumentStore, cx: &mut App) {
    let appearance = store.load_or_default::<Appearance>(DOCUMENT).normalized();
    let system_dark = system_is_dark(cx);
    cx.set_global(State {
        appearance,
        system_dark,
        store: Some(store),
        revision: 0,
        _save: None,
    });
    put_in_force(cx);
}

fn system_is_dark(cx: &App) -> bool {
    !matches!(
        cx.window_appearance(),
        gpui::WindowAppearance::Light | gpui::WindowAppearance::VibrantLight
    )
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

/// The id of the theme in force.
pub fn active_id(cx: &App) -> String {
    cx.try_global::<State>().map_or_else(default_dark, |state| {
        state.appearance.active_id(state.system_dark).to_owned()
    })
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
    state.appearance = next;
    state.revision += 1;
    put_in_force(cx);
    schedule_save(cx);
}

/// The system went from dark to light or back: with the mode on System, the theme follows.
pub fn set_system_dark(cx: &mut App, dark: bool) {
    if !cx.has_global::<State>() {
        return;
    }
    let state = cx.global_mut::<State>();
    if state.system_dark == dark {
        return;
    }
    state.system_dark = dark;
    if state.appearance.mode == Mode::System {
        put_in_force(cx);
    }
}

/// Reads the system's appearance again, for the window that just learned it changed.
pub fn refresh_system(cx: &mut App) {
    let dark = system_is_dark(cx);
    set_system_dark(cx, dark);
}

fn put_in_force(cx: &mut App) {
    let (colors, animations) = {
        let state = cx.global::<State>();
        (
            state.appearance.resolve(state.system_dark),
            state.appearance.animations,
        )
    };
    ANIMATIONS.store(animations, Ordering::Relaxed);
    theme::set_colors(colors);
    theme::apply(cx);
}

fn schedule_save(cx: &mut App) {
    let revision = cx.global::<State>().revision;
    let task = cx.spawn(async move |cx| {
        cx.background_executor().timer(SAVE_DELAY).await;
        cx.update(|cx| {
            if cx.has_global::<State>() && cx.global::<State>().revision == revision {
                save_now(cx);
            }
        });
    });
    cx.global_mut::<State>()._save = Some(task);
}

/// Writes the settings now, for what is about to read the file (a backup).
pub fn save_now(cx: &mut App) {
    let Some(state) = cx.try_global::<State>() else {
        return;
    };
    let Some(store) = state.store.clone() else {
        return;
    };
    if let Err(error) = store.save(DOCUMENT, &state.appearance) {
        tracing::warn!(%error, "could not save the appearance");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dark() -> Colors {
        presets::preset(DEFAULT_DARK).unwrap().colors
    }

    #[test]
    fn the_mode_and_the_system_choose_the_theme() {
        let mut a = Appearance {
            dark_theme: "nord".to_owned(),
            light_theme: "paper".to_owned(),
            ..Appearance::default()
        };
        assert_eq!(
            a.active_id(false),
            "nord",
            "dark by default, whatever the system says"
        );
        a.mode = Mode::Light;
        assert_eq!(a.active_id(true), "paper");
        a.mode = Mode::System;
        assert_eq!(a.active_id(true), "nord");
        assert_eq!(a.active_id(false), "paper");
        assert!(!a.resolve(true).is_light());
        assert!(a.resolve(false).is_light());
    }

    #[test]
    fn the_overrides_sit_on_top_of_any_theme() {
        let mut a = Appearance::default();
        assert_eq!(a.resolve(true), dark());
        a.accent = Some(0xff0000);
        a.candle_up = Some(0x00ff00);
        a.candle_down = Some(0x0000ff);
        a.chart_line = Some(0x123456);
        a.chart_background = Some(0x101010);
        let c = a.resolve(true);
        assert_eq!(
            (c.accent, c.up, c.down, c.line, c.chart_bg),
            (0xff0000, 0x00ff00, 0x0000ff, 0x123456, 0x101010)
        );
        assert_eq!(c.selected, 0xff00_0066, "the tint follows the accent");
        // The rest is the theme's.
        assert_eq!((c.bg, c.fg), (dark().bg, dark().fg));
        // And they hold when the theme changes.
        a.dark_theme = "dracula".to_owned();
        let c = a.resolve(true);
        assert_eq!(c.up, 0x00ff00);
        assert_eq!(c.bg, presets::preset("dracula").unwrap().colors.bg);
        // The text on a light accent is dark, on a dark one it is light.
        a.accent = Some(0xffffcc);
        assert_eq!(a.resolve(true).accent_fg, 0x0a0a0a);
        a.accent = Some(0x001133);
        assert_eq!(a.resolve(true).accent_fg, 0xffffff);
    }

    #[test]
    fn a_theme_can_be_copied_edited_renamed_and_deleted() {
        let mut a = Appearance::default();
        let id = a.copy_theme("nord", "  My Nord  ").unwrap();
        assert_eq!(id, "custom-my-nord");
        assert_eq!(a.find(&id).unwrap().0, "My Nord");
        assert_eq!(
            a.find(&id).unwrap().1,
            presets::preset("nord").unwrap().colors
        );
        // The same name again gets its own id.
        assert_eq!(a.copy_theme("nord", "My Nord").unwrap(), "custom-my-nord-2");
        assert!(a.copy_theme("no-such-theme", "x").is_none());
        assert!(a.copy_theme("nord", "   ").is_none());

        assert!(a.set_theme_color(&id, ColorField::Bg, 0x010203));
        assert!(
            !a.set_theme_color(&id, ColorField::Bg, 0x010203),
            "no change"
        );
        assert!(
            !a.set_theme_color("nord", ColorField::Bg, 0x111111),
            "a preset is not edited"
        );
        assert_eq!(a.find(&id).unwrap().1.bg, 0x010203);
        assert!(a.set_theme_color(&id, ColorField::Accent, 0xffffff));
        assert_eq!(a.find(&id).unwrap().1.accent_fg, 0x0a0a0a);

        assert!(a.rename_theme(&id, "Renamed"));
        assert!(!a.rename_theme(&id, "  "));
        a.dark_theme = id.clone();
        assert!(a.delete_theme(&id));
        assert!(!a.delete_theme(&id));
        assert_eq!(
            a.dark_theme, DEFAULT_DARK,
            "the slot goes back to the default"
        );
    }

    #[test]
    fn a_user_has_a_limited_number_of_themes() {
        let mut a = Appearance::default();
        for n in 0..MAX_CUSTOM_THEMES {
            assert!(a.copy_theme("nord", &format!("theme {n}")).is_some());
        }
        assert!(a.copy_theme("nord", "one more").is_none());
    }

    #[test]
    fn normalizing_repairs_what_a_file_can_hold() {
        let mut a = Appearance {
            dark_theme: "gone".to_owned(),
            light_theme: "gone-too".to_owned(),
            accent: Some(0xff12_3456),
            font: "   ".to_owned(),
            ..Appearance::default()
        };
        a.custom_themes = vec![
            CustomTheme {
                id: "a".into(),
                name: "A".into(),
                colors: dark(),
            },
            CustomTheme {
                id: "a".into(),
                name: "Again".into(),
                colors: dark(),
            },
            CustomTheme {
                id: "b".into(),
                name: "  ".into(),
                colors: dark(),
            },
            CustomTheme {
                id: "nord".into(),
                name: "Clashes with a preset".into(),
                colors: dark(),
            },
        ];
        let a = a.normalized();
        assert_eq!(
            a.custom_themes.len(),
            1,
            "one theme per id, with a name, not a preset's"
        );
        assert_eq!(a.dark_theme, DEFAULT_DARK);
        assert_eq!(a.light_theme, DEFAULT_LIGHT);
        assert_eq!(a.accent, Some(0x12_3456));
        assert_eq!(a.font, DEFAULT_FONT);
    }

    #[test]
    fn an_empty_or_partial_file_gives_the_defaults() {
        let empty: Appearance = toml::from_str("").unwrap();
        assert_eq!(empty, Appearance::default());
        assert!(empty.is_default());
        let some: Appearance = toml::from_str("mode = \"system\"\nanimations = false\n").unwrap();
        assert_eq!(some.mode, Mode::System);
        assert!(!some.animations);
        assert_eq!(some.dark_theme, DEFAULT_DARK);
    }

    #[test]
    fn a_look_survives_the_saved_file() {
        let mut a = Appearance {
            mode: Mode::System,
            accent: Some(0xabcdef),
            candle_up: Some(0x111111),
            font: "Segoe UI".to_owned(),
            animations: false,
            ..Appearance::default()
        };
        let id = a.copy_theme("dracula", "Mine").unwrap();
        a.dark_theme = id;
        let text = toml::to_string_pretty(&a).unwrap();
        let back: Appearance = toml::from_str(&text).unwrap();
        assert_eq!(back.normalized(), a.normalized());
    }

    #[test]
    fn every_theme_is_listed_the_users_first() {
        let mut a = Appearance::default();
        a.copy_theme("nord", "Mine").unwrap();
        let themes = a.themes();
        assert_eq!(themes[0].1, "Mine");
        assert_eq!(themes.len(), PRESETS.len() + 1);
    }
}
