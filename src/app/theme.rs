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

/// The hairline separating the titlebar from the content below it.
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

/// Background tint for an emerald badge or icon well.
pub fn emerald_bg() -> Rgba {
    rgba(0x00d49226)
}

/// The height of the custom titlebar, in logical pixels.
pub const TITLEBAR_HEIGHT: f32 = 44.0;
