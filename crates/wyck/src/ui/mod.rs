//! Screens: one module per distinct full-terminal view. [`crate::app::App`] owns
//! exactly one active [`Screen`] at a time and dispatches drawing/input to it.

mod dashboard;
mod first_run;

pub use dashboard::{ConnectionStatus, DashboardScreen};
pub use first_run::{FirstRunOutcome, FirstRunScreen};

/// The single screen the app is currently showing.
///
/// `FirstRunScreen` is boxed purely to keep this enum small: it holds four
/// [`tui_input::Input`] buffers and is meaningfully larger than `DashboardScreen`, and
/// `Screen` is moved around (e.g. replaced wholesale on submit) often enough that the
/// size difference is worth flattening.
pub enum Screen {
    FirstRun(Box<FirstRunScreen>),
    Dashboard(DashboardScreen),
}
