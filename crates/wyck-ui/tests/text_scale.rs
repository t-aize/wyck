//! The sizes of the kit follow the text scale. The scale is one value for the whole process, so
//! this is the only test that sets it, in a test binary of its own.

use wyck_domain::base::appearance::{SCALE_MAX, SCALE_MIN};
use wyck_domain::base::math::f32_of;
use wyck_domain::base::text_scale;
use wyck_ui::kit::tokens::{bar, height, scaled, space, text};

#[test]
fn sizes_follow_the_scale_and_the_scale_stays_in_range() {
    text_scale::set(100);
    assert_eq!(text::body(), 12.0);
    text_scale::set(150);
    assert_eq!(text::body(), 18.0);
    assert_eq!(height::control(), 42.0);
    assert_eq!(space::lg(), 24.0);
    assert_eq!(bar::header(), 72.0);
    text_scale::set(10_000);
    assert_eq!(scaled(100.0), f32_of(f64::from(SCALE_MAX)));
    text_scale::set(1);
    assert_eq!(scaled(100.0), f32_of(f64::from(SCALE_MIN)));
    text_scale::set(100);
}
