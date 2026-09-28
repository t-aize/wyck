//! The sizes every screen shares: text, control heights, field widths and menus.
//!
//! A screen picks one of these instead of writing a number, so two controls that do the same job
//! have the same size wherever they are. Corners follow one rule too: `rounded_md` for a control,
//! `rounded_lg` for a card, a menu or a popover, `rounded_xl` for a dialog, `rounded_full` for a
//! pill or a dot. The edge of a card, a menu or a popover is `theme::border_subtle`; a line
//! between two parts of one surface is `theme::border_hairline`.

/// Text sizes, in pixels.
pub mod text {
    /// Badges, keys, the headings of a menu.
    pub const CAPTION: f32 = 10.0;
    /// Secondary text: hints, labels under a value, timestamps.
    pub const SMALL: f32 = 11.0;
    /// The text of controls, menus and lists.
    pub const BODY: f32 = 12.0;
    /// The label of a field, the text of a dialog.
    pub const EMPHASIS: f32 = 13.0;
    /// The title of a panel or of a card.
    pub const TITLE: f32 = 14.0;
    /// A large figure or the title of a screen.
    pub const DISPLAY: f32 = 20.0;
}

/// Heights of controls and rows, in pixels.
pub mod height {
    /// A control in a dense strip: a toolbar, the order ticket.
    pub const COMPACT: f32 = 24.0;
    /// A button, a field, a row of a menu or of a list.
    pub const CONTROL: f32 = 28.0;
    /// A tab of a rail, the head of a group.
    pub const LARGE: f32 = 34.0;
}

/// Widths of fields, in pixels.
pub mod field {
    /// A short number: a count, a width, a percent.
    pub const NARROW: f32 = 84.0;
    /// A number with decimals: a price, a level, an amount.
    pub const NUMBER: f32 = 110.0;
    /// A long number.
    pub const WIDE: f32 = 130.0;
    /// A line of text: a name, a comment, a path.
    pub const TEXT: f32 = 200.0;
}

/// Menus and popovers.
pub mod menu {
    /// A menu opened by a right click.
    pub const CONTEXT_WIDTH: f32 = 240.0;
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
