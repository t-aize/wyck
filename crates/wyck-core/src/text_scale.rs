//! How big the text of a chart is, as a share of its usual size.
//!
//! The scale of the interface (see the settings) makes the text of the panels bigger or smaller.
//! The text drawn on a chart (the prices on the axis, the labels of the drawings, the names of the
//! indicators) is part of the scene the chart builds, not of the widgets, so it follows this
//! setting on its own. The value is process wide, like the size of the widgets: a picture of a
//! chart is made at the size in force when it is taken.

use std::sync::atomic::{AtomicU32, Ordering};

use super::appearance::{SCALE_MAX, SCALE_MIN};

static PERCENT: AtomicU32 = AtomicU32::new(100);

/// A scale kept between the least and the most the interface can be scaled to.
fn clamped(percent: u32) -> u32 {
    percent.clamp(SCALE_MIN, SCALE_MAX)
}

/// Sets the scale, in percent, kept between [`SCALE_MIN`] and [`SCALE_MAX`].
pub fn set(percent: u32) {
    PERCENT.store(clamped(percent), Ordering::Relaxed);
}

/// The scale in force, in percent.
fn percent() -> u32 {
    PERCENT.load(Ordering::Relaxed)
}

/// A percent as an `f32`. The scale is kept between [`SCALE_MIN`] and [`SCALE_MAX`], far below
/// `u16::MAX`, so the conversion is exact.
fn percent_f32(percent: u32) -> f32 {
    f32::from(u16::try_from(percent).unwrap_or(u16::MAX))
}

/// The scale in force as a factor: 1.0 at 100 percent.
pub fn factor() -> f32 {
    percent_f32(percent()) / 100.0
}

/// A size at the scale in force.
pub fn scaled(base: f32) -> f32 {
    at(base, percent())
}

/// A size at a scale given in percent.
fn at(base: f32, percent: u32) -> f32 {
    base * percent_f32(clamped(percent)) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // The scale itself is shared by the whole process, and the tests of the scene read it: no test
    // sets it. What can be checked without it is checked here.
    #[test]
    fn the_scale_is_kept_in_range_and_sizes_follow_it() {
        assert_eq!(clamped(10), SCALE_MIN);
        assert_eq!(clamped(900), SCALE_MAX);
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
