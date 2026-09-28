//! Defaults for `#[serde(default = "...")]`, shared by every saved setting of the app.

/// A switch that is on when a saved file does not mention it.
pub fn yes() -> bool {
    true
}
