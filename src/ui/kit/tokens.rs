//! The sizes every screen shares: text, control heights, field widths and menus.
//!
//! A screen picks one of these instead of writing a number, so two controls that do the same job
//! have the same size wherever they are. Corners follow one rule too: `rounded_md` for a control,
//! `rounded_lg` for a card, a menu or a popover, `rounded_xl` for a dialog, `rounded_full` for a
//! pill or a dot. The edge of a card, a menu or a popover is `theme::border_subtle`; a line
//! between two parts of one surface is `theme::border_hairline`.
//!
//! Text, heights and widths follow the size of the interface the user chose (see [`set_scale`]):
//! they are functions that give the base size times that scale, so the whole app grows or shrinks
//! together and a person who needs larger text gets it everywhere.

use std::sync::atomic::{AtomicU32, Ordering};

/// The scale in force, in percent.
static SCALE: AtomicU32 = AtomicU32::new(100);

pub use crate::domain::appearance::{SCALE_MAX, SCALE_MIN};

/// Sets the size of the interface, in percent (100 is the size the app was drawn at). It is kept
/// between [`SCALE_MIN`] and [`SCALE_MAX`]. Screens read it as they paint.
pub fn set_scale(percent: u32) {
    SCALE.store(percent.clamp(SCALE_MIN, SCALE_MAX), Ordering::Relaxed);
}

/// The size of the interface, in percent.
pub fn scale_percent() -> u32 {
    SCALE.load(Ordering::Relaxed)
}

/// A base size at the scale in force.
pub fn scaled(base: f32) -> f32 {
    base * scale_percent() as f32 / 100.0
}

/// Text sizes, in pixels.
pub mod text {
    use super::scaled;

    /// Badges, keys, the headings of a menu.
    pub fn caption() -> f32 {
        scaled(10.0)
    }
    /// Secondary text: hints, labels under a value, timestamps.
    pub fn small() -> f32 {
        scaled(11.0)
    }
    /// The text of controls, menus and lists.
    pub fn body() -> f32 {
        scaled(12.0)
    }
    /// The label of a field, the text of a dialog.
    pub fn emphasis() -> f32 {
        scaled(13.0)
    }
    /// The title of a panel or of a card.
    pub fn title() -> f32 {
        scaled(14.0)
    }
    /// The head of a block: the symbol of the order ticket, a price, a notice.
    pub fn heading() -> f32 {
        scaled(16.0)
    }
    /// The title of a screen, a large figure.
    pub fn display() -> f32 {
        scaled(20.0)
    }
    /// The title of the welcome screen.
    pub fn hero() -> f32 {
        scaled(26.0)
    }
}

/// Heights of controls and rows, in pixels.
pub mod height {
    use super::scaled;

    /// The smallest button (gpui-kit's `xsmall`): a tool in a panel's own bar.
    pub fn tiny() -> f32 {
        scaled(20.0)
    }
    /// A control in a dense strip: a toolbar, the order ticket.
    pub fn compact() -> f32 {
        scaled(24.0)
    }
    /// A button, a field, a row of a menu or of a list.
    pub fn control() -> f32 {
        scaled(28.0)
    }
    /// A tab of a rail, the head of a group.
    pub fn large() -> f32 {
        scaled(34.0)
    }
}

/// Space between and around things, in pixels. Use these for gaps and padding written with
/// `px(..)`; the gpui scale classes (`gap_2`, `p_3`) stay for plain layout.
pub mod space {
    use super::scaled;

    /// Between an icon and its label, inside a tight chip.
    pub fn xs() -> f32 {
        scaled(4.0)
    }
    /// Between controls of a row, inside a button group.
    pub fn sm() -> f32 {
        scaled(8.0)
    }
    /// Between a label and its field, around a list row.
    pub fn md() -> f32 {
        scaled(12.0)
    }
    /// Around the content of a card or a panel.
    pub fn lg() -> f32 {
        scaled(16.0)
    }
    /// Between the blocks of a page.
    pub fn xl() -> f32 {
        scaled(24.0)
    }
}

/// Heights of the bars that frame a screen, in pixels.
pub mod bar {
    use super::scaled;

    /// A strip of tools inside a panel: the editor toolbar, the drawing options.
    pub fn toolbar() -> f32 {
        scaled(40.0)
    }
    /// The header of the dashboard.
    pub fn header() -> f32 {
        scaled(48.0)
    }
}

/// Widths of fields, in pixels.
pub mod field {
    use super::scaled;

    /// A short number: a count, a width, a percent.
    pub fn narrow() -> f32 {
        scaled(84.0)
    }
    /// A number with decimals: a price, a level, an amount.
    pub fn number() -> f32 {
        scaled(110.0)
    }
    /// A long number.
    pub fn wide() -> f32 {
        scaled(130.0)
    }
    /// A line of text: a name, a comment, a path.
    pub fn text() -> f32 {
        scaled(220.0)
    }
}

/// Menus and popovers.
pub mod menu {
    /// A menu opened by a right click.
    pub const CONTEXT_WIDTH: f32 = 240.0;
    /// A list that opens beside a rail, longer names than a context menu.
    pub const FLYOUT_WIDTH: f32 = 280.0;
    /// The least width of a menu opened by a button; it grows to fit its entries.
    pub const DROPDOWN_WIDTH: f32 = 200.0;
    /// A popover that holds controls rather than entries.
    pub const PANEL_WIDTH: f32 = 320.0;
    /// The space between a button and what opens under it.
    pub const GAP: f32 = 4.0;
    /// How far a card stays from the edges of the window.
    pub const MARGIN: f32 = 8.0;
    /// The tallest a card grows before it scrolls.
    pub const MAX_HEIGHT: f32 = 520.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_follow_the_scale_and_the_scale_stays_in_range() {
        // The scale is one for the whole process: this test is the only one that sets it.
        set_scale(100);
        assert_eq!(text::body(), 12.0);
        set_scale(150);
        assert_eq!(text::body(), 18.0);
        assert_eq!(height::control(), 42.0);
        assert_eq!(space::lg(), 24.0);
        assert_eq!(bar::header(), 72.0);
        set_scale(10_000);
        assert_eq!(scale_percent(), SCALE_MAX);
        set_scale(1);
        assert_eq!(scale_percent(), SCALE_MIN);
        set_scale(100);
    }
}
