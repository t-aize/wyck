//! Answers that come back after the part of the app waiting for them is gone.

use std::fmt::Display;

use tracing::warn;

/// Writes down that an answer could not be delivered because the entity waiting for it was
/// released (the window closed, the user signed out) while the call was away. The answer is lost,
/// and on the order, position and account paths that is worth knowing, so it is logged with where
/// it happened instead of dropped. `what` says which answer it was; it must hold no secret.
pub fn lost<T, E: Display>(what: &str, delivered: Result<T, E>) {
    if let Err(error) = delivered {
        warn!(what, %error, "an answer arrived after its owner was released");
    }
}
