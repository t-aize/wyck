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
    ///
    /// Explicitly clears the terminal before returning it. Entering the alternate
    /// screen is not reliably a blank slate on every terminal/platform combination —
    /// notably on Windows, a prior process that left its own alternate-screen content
    /// behind (e.g. one killed hard enough to skip its own `Drop`/restore, or a
    /// preceding `cargo run`'s build-progress output) can otherwise bleed through
    /// ratatui's diff-based redraw on the very first frame, since that diff is computed
    /// against ratatui's own empty starting buffer, not against whatever bytes actually
    /// happen to be sitting in the terminal. An explicit clear forces every cell to be
    /// written on the first draw regardless of what was there before.
    ///
    /// # Errors
    ///
    /// Returns the backend I/O error if the initial clear fails.
    pub fn enter() -> std::io::Result<(Self, ratatui::DefaultTerminal)> {
        let mut terminal = ratatui::init();
        // Constructed before the fallible `clear()` call so that if it fails, this
        // guard is already live and its `Drop` still restores the terminal on the way
        // out via `?` — rather than leaving raw mode / the alternate screen entered
        // with nothing left to clean it up.
        let guard = Self { _private: () };
        terminal.clear()?;
        Ok((guard, terminal))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        ratatui::restore();
    }
}
