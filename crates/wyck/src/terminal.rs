//! Terminal lifecycle: enter raw mode + the alternate screen, and guarantee restoration
//! on every exit path — normal return, an early `?`, or a panic.

/// An RAII guard that puts the terminal back into its normal (cooked, main-screen) mode
/// when dropped.
///
/// [`ratatui::init`] itself installs a panic hook that restores the terminal before
/// forwarding to the previous hook, so a panic is already covered. This guard covers
/// the other two exit paths ratatui's hook doesn't: a normal return from [`App::run`],
/// and an early return via `?` while resolving a startup error. Because [`Drop`] runs
/// during unwinding for both of those, holding an instance of this guard for the
/// lifetime of the TUI session is enough to guarantee [`ratatui::restore`] always runs
/// exactly once before the process's next line of output (e.g. `color_eyre`'s error
/// report, if `main` returns `Err`).
///
/// [`App::run`]: crate::app::App::run
pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    /// Enters raw mode + the alternate screen and returns both the guard and the ready-
    /// to-draw [`ratatui::DefaultTerminal`]. Keep the guard bound to a variable for as
    /// long as the terminal should stay in TUI mode; drop it (or let it go out of
    /// scope) to restore.
    pub fn enter() -> (Self, ratatui::DefaultTerminal) {
        let terminal = ratatui::init();
        (Self { _private: () }, terminal)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
