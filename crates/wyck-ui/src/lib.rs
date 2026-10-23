//! The screens of Wyck, one module per area: `kit` (widgets and look), `chart`, `trading`,
//! `editor` and `shell` (the window). A module names the ones before it in that list, never after.

pub mod chart;
pub mod editor;
pub mod kit;
pub mod shell;
pub mod trading;
