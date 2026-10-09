//! How big the text of a chart is, as a share of its usual size.
//!
//! The scale of the interface (see the settings) makes the text of the panels bigger or smaller.
//! The text drawn on a chart (the prices on the axis, the labels of the drawings, the names of the
//! indicators) is part of the scene the chart builds, not of the widgets, so it follows this
//! setting on its own. The value is process wide, like the size of the widgets: a picture of a
//! chart is made at the size in force when it is taken.

use std::sync::atomic::{AtomicU32, Ordering};

/// The smallest and the largest scale, in percent. The same as the interface's.
pub const MIN: u32 = 80;
pub const MAX: u32 = 160;

static PERCENT: AtomicU32 = AtomicU32::new(100);

/// A scale kept between [`MIN`] and [`MAX`].
fn clamped(percent: u32) -> u32 {
    percent.clamp(MIN, MAX)
}

/// Sets the scale, in percent, kept between [`MIN`] and [`MAX`].
pub fn set(percent: u32) {
    PERCENT.store(clamped(percent), Ordering::Relaxed);
}

/// The scale in force, in percent.
pub fn percent() -> u32 {
    PERCENT.load(Ordering::Relaxed)
}

/// The scale in force as a factor: 1.0 at 100 percent.
pub fn factor() -> f32 {
    percent() as f32 / 100.0
}

/// A size at the scale in force.
pub fn scaled(base: f32) -> f32 {
    at(base, percent())
}

/// A size at a scale given in percent.
fn at(base: f32, percent: u32) -> f32 {
    base * clamped(percent) as f32 / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // The scale itself is shared by the whole process, and the tests of the scene read it: no test
    // sets it. What can be checked without it is checked here.
    #[test]
    fn the_scale_is_kept_in_range_and_sizes_follow_it() {
        assert_eq!(clamped(10), MIN);
        assert_eq!(clamped(900), MAX);
        assert_eq!(clamped(120), 120);
        assert!((at(10.0, 150) - 15.0).abs() < 1e-4);
        assert!((at(10.0, 100) - 10.0).abs() < 1e-6);
        assert!(
            (at(10.0, 5) - 8.0).abs() < 1e-4,
            "under the least it is the least"
        );
        // Left alone, the scale is 100 percent and changes nothing.
        assert_eq!(percent(), 100);
        assert!((scaled(11.0) - 11.0).abs() < 1e-6);
    }
}
