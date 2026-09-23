//! Design tokens for the desktop app: colors, radii and type scale.
//!
//! Extracted from the approved dark-theme mockup (a shadcn/ui "zinc" dark palette with a violet
//! accent swapped in) by rendering it and reading its computed styles back, not eyeballed from a
//! screenshot. Keep this the single source of truth for anything visual: a screen should read
//! `theme::` values, never spell out a hex color inline.

use gpui::{Rgba, rgb, rgba};

/// Window and page background. Also the color of an unfocused text input.
pub fn bg() -> Rgba {
    rgb(0x0a0a0a)
}

/// Raised surface: cards, panels, number badges, list rows.
pub fn surface() -> Rgba {
    rgb(0x171717)
}

/// Primary text.
pub fn fg() -> Rgba {
    rgb(0xfafafa)
}

/// Secondary text: descriptions, hints, timestamps.
pub fn muted_fg() -> Rgba {
    rgb(0xa1a1a1)
}

/// A subtle divider between stacked sections.
pub fn border_hairline() -> Rgba {
    rgba(0xffffff1a)
}

/// A slightly stronger border: input outlines, badge rings, card edges.
pub fn border_subtle() -> Rgba {
    rgba(0xffffff26)
}

/// Primary action color: buttons, active icons, links, focus rings.
pub fn accent() -> Rgba {
    rgb(0x7c86ff)
}

/// Text/icon color to place on top of [`accent`].
pub fn accent_fg() -> Rgba {
    rgb(0x0a0a0a)
}

/// The tint used for the selected segment of a segmented control.
pub fn accent_selected() -> Rgba {
    rgba(0x615fff73)
}

/// Error text, icons and borders.
pub fn destructive() -> Rgba {
    rgb(0xff6467)
}

/// Background tint for an error banner or an invalid field.
pub fn destructive_bg() -> Rgba {
    rgba(0xff64671a)
}

/// The "LIVE" account badge.
pub fn amber() -> Rgba {
    rgb(0xffb900)
}

/// Background tint for an amber badge.
pub fn amber_bg() -> Rgba {
    rgba(0xffb90026)
}

/// Success state: the connected checkmark, "Connected" text.
pub fn emerald() -> Rgba {
    rgb(0x00d492)
}

/// Points gpui-component's theme at this palette, so its buttons, spinners and tooltips match
/// the hand-styled parts of the UI. Call once at startup, after `gpui_kit::init`.
pub fn apply(cx: &mut gpui::App) {
    use gpui::Hsla;
    use gpui_kit::component::{Theme, ThemeMode};

    Theme::change(ThemeMode::Dark, None, cx);
    let colors = &mut Theme::global_mut(cx).colors;
    let hsla = |color: Rgba| -> Hsla { color.into() };

    colors.background = hsla(bg());
    colors.foreground = hsla(fg());
    colors.border = hsla(border_subtle());
    colors.input = hsla(border_subtle());
    colors.ring = hsla(accent());
    colors.muted = hsla(surface());
    colors.muted_foreground = hsla(muted_fg());
    colors.popover = hsla(surface());
    colors.popover_foreground = hsla(fg());

    colors.primary = hsla(accent());
    colors.primary_hover = hsla(accent()).opacity(0.88);
    colors.primary_active = hsla(accent()).opacity(0.75);
    colors.primary_foreground = hsla(accent_fg());

    colors.secondary = hsla(surface());
    colors.secondary_hover = hsla(rgb(0x262626));
    colors.secondary_active = hsla(rgb(0x2e2e2e));
    colors.secondary_foreground = hsla(fg());

    colors.accent = hsla(surface());
    colors.accent_foreground = hsla(fg());

    colors.danger = hsla(destructive());
    colors.danger_hover = hsla(destructive()).opacity(0.88);
    colors.danger_active = hsla(destructive()).opacity(0.75);
    colors.danger_foreground = hsla(accent_fg());
    colors.success = hsla(emerald());
    colors.warning = hsla(amber());
    colors.link = hsla(accent());

    // Buttons read their own set of colors, which default to unrelated values.
    colors.button = colors.secondary;
    colors.button_hover = colors.secondary_hover;
    colors.button_active = colors.secondary_active;
    colors.button_foreground = colors.secondary_foreground;
    colors.button_primary = colors.primary;
    colors.button_primary_hover = colors.primary_hover;
    colors.button_primary_active = colors.primary_active;
    colors.button_primary_foreground = colors.primary_foreground;
    colors.button_secondary = colors.secondary;
    colors.button_secondary_hover = colors.secondary_hover;
    colors.button_secondary_active = colors.secondary_active;
    colors.button_secondary_foreground = colors.secondary_foreground;
    colors.button_danger = colors.danger;
    colors.button_danger_hover = colors.danger_hover;
    colors.button_danger_active = colors.danger_active;
    colors.button_danger_foreground = colors.danger_foreground;

    // Components read a second, derived copy of the palette; rebuild it from the edited colors.
    let theme = Theme::global_mut(cx);
    theme.tokens = theme.colors.into();
}

/// Row and option background while the pointer is over it.
pub fn surface_hover() -> Rgba {
    rgb(0x1f1f1f)
}

/// Row and option background while pressed.
pub fn surface_pressed() -> Rgba {
    rgb(0x282828)
}

/// A card or row edge while the pointer is over it.
pub fn border_strong() -> Rgba {
    rgba(0xffffff40)
}
