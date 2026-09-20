//! Motion: how things move between screens and react to the pointer.
//!
//! One place holds the durations, the curves and the distances, so every animation in the
//! application speaks the same language. The numbers come from desktop motion guidelines and from
//! the defaults of the component library:
//!
//! - **Duration.** Micro-interactions (hover, focus) 120 ms, fades and small moves 180 ms, a
//!   screen entering 280 ms. **What leaves is faster than what arrives**: a screen exits in 110 ms
//!   because the user has already read it and is waiting for the next one.
//! - **Curves.** What enters decelerates ([`enter`]), what leaves accelerates ([`exit`]), what
//!   moves while staying on screen eases both ways ([`standard`]). Nothing spatial is linear.
//! - **Distance.** A screen slides 18 px in and 12 px out: enough to give the change a direction,
//!   little enough not to be a show. A card's content rises 8 px, one element after the other, 32 ms
//!   apart, so the whole card is in place well under half a second.
//! - **Direction.** Going deeper into the flow slides the new screen in from the right, going back
//!   slides it in from the left ([`direction`]).
//! - **Reduced motion.** Every transition here goes through GPUI's motion functions, which show
//!   the end state at once when the system's "animation effects" switch is off, or when
//!   `WYCK_REDUCE_MOTION=on` is set. Nothing else has to be done for it.
//!
//! The pure parts (the depth of a screen, the direction of a change, the easing curves) are tested
//! without a window. [`Hover`] and the helpers that take a `Window` are used by the widgets.

use std::time::Duration;

use gpui_kit::base::{Easing, Transition, TransitionId, transition};
use gpui_kit::{App, ElementId, Entity, Hsla, Rgba, Window};

use crate::flow::Screen;

/// A hover or focus change.
pub const FAST: Duration = Duration::from_millis(120);
/// A fade or a small move.
pub const NORMAL: Duration = Duration::from_millis(180);
/// A screen entering, a toast arriving.
pub const SLOW: Duration = Duration::from_millis(280);
/// A screen leaving, a toast leaving: half of what it took to arrive, or less.
pub const LEAVE: Duration = Duration::from_millis(110);

/// How long a leaving screen is gone before the next one starts to arrive, so the two are never
/// both readable at once.
pub const SCREEN_GAP: Duration = Duration::from_millis(70);
/// How far a screen slides in, in pixels.
pub const SLIDE_IN: f32 = 18.0;
/// How far a screen slides out, in pixels.
pub const SLIDE_OUT: f32 = 12.0;
/// How far a piece of a card rises into place, in pixels.
pub const RISE: f32 = 8.0;
/// The delay between one piece of a card and the next.
pub const STAGGER: Duration = Duration::from_millis(32);
/// The delay before the first piece of a card starts, after its screen begins to arrive.
pub const RISE_START: Duration = Duration::from_millis(90);
/// How long one piece of a card takes to rise.
pub const RISE_TIME: Duration = Duration::from_millis(240);

/// The curve of something arriving: fast start, long gentle landing.
#[must_use]
pub fn enter() -> Easing {
    bezier(0.16, 1.0, 0.3, 1.0)
}

/// The curve of something leaving: slow start, quick departure.
#[must_use]
pub fn exit() -> Easing {
    bezier(0.4, 0.0, 1.0, 1.0)
}

/// The curve of something that stays on screen and changes: eases both ways.
#[must_use]
pub fn standard() -> Easing {
    bezier(0.2, 0.0, 0.0, 1.0)
}

fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Easing {
    // The control points are constants inside the valid range, so this cannot fail.
    Easing::cubic_bezier(x1, y1, x2, y2).unwrap_or(Easing::EaseOut)
}

/// A curve that overshoots its target a little and settles back: for a mark that pops.
///
/// It runs from 0 to 1, peaks near 1.1 at about two thirds of the way and lands on 1.
#[must_use]
pub fn overshoot(t: f32) -> f32 {
    const C1: f32 = 1.4;
    const C3: f32 = C1 + 1.0;
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + C3 * t * t * t + C1 * t * t
}

/// How long the shake of a refused field lasts.
pub const SHAKE_TIME: Duration = Duration::from_millis(380);

/// The sideways offset, in pixels, of a field that says "no", `t` running from 0 to 1: three
/// swings of at most 8 px that die out.
#[must_use]
pub fn shake(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let decay = (1.0 - t) * (1.0 - t);
    (t * std::f32::consts::TAU * 3.0).sin() * 8.0 * decay
}

/// The transition of a screen arriving.
#[must_use]
pub fn screen_in() -> Transition {
    Transition::new(SLOW).delay(SCREEN_GAP).easing(enter())
}

/// The transition of a screen leaving.
#[must_use]
pub fn screen_out() -> Transition {
    Transition::new(LEAVE).easing(exit())
}

/// The transition of a hover or a focus.
#[must_use]
pub fn quick() -> Transition {
    Transition::new(FAST).easing(standard())
}

/// Which way a change of screen goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Deeper into the flow: the new screen comes from the right.
    Forward,
    /// Back out of it: the new screen comes from the left.
    Back,
}

impl Direction {
    /// `1.0` for forward, `-1.0` for back: the side a screen comes from.
    #[must_use]
    pub fn sign(self) -> f32 {
        match self {
            Self::Forward => 1.0,
            Self::Back => -1.0,
        }
    }
}

/// How deep a screen is in the flow. A screen at a greater depth is "further".
#[must_use]
pub fn depth(screen: &Screen) -> u8 {
    match screen {
        Screen::Choose => 0,
        Screen::Searching | Screen::Token { .. } => 1,
        Screen::Verifying { .. } | Screen::LocalFound(_) | Screen::LocalNotFound(_) => 2,
        Screen::Connected => 3,
    }
}

/// Whether two states are the same page, so that a change between them is an update of that page
/// and not a change of screen.
#[must_use]
pub fn same_page(a: &Screen, b: &Screen) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// The direction of the change from `from` to `to`. A change between screens of the same depth
/// (from the token form to the local search, say) goes forward.
#[must_use]
pub fn direction(from: &Screen, to: &Screen) -> Direction {
    if depth(to) < depth(from) {
        Direction::Back
    } else {
        Direction::Forward
    }
}

/// The horizontal offset of a screen at `progress` (0 gone, 1 settled), in pixels.
///
/// An arriving screen starts on the side it comes from and lands at 0. A leaving screen goes
/// toward the opposite side, less far.
#[must_use]
pub fn slide(direction: Direction, arriving: bool, progress: f32) -> f32 {
    let distance = if arriving {
        SLIDE_IN * direction.sign()
    } else {
        -SLIDE_OUT * direction.sign()
    };
    distance * (1.0 - progress)
}

/// `from` faded toward `to` by `t` (0 to 1), through colors that look like both.
///
/// The colors are mixed as RGB weighted by their opacity (premultiplied), not channel by channel
/// in HSL: fading an opaque dark ground toward "white at 3 percent" channel by channel goes
/// through a light gray half way, which shows as a flash. Mixed this way it stays dark, and a
/// fade from transparent only changes the opacity.
#[must_use]
pub fn blend(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    let (a, b) = (Rgba::from(from), Rgba::from(to));
    let alpha = a.a + (b.a - a.a) * t;
    if alpha <= f32::EPSILON {
        return Hsla { a: 0.0, ..to };
    }
    let (ra, rb) = ((a.r, a.g, a.b), (b.r, b.g, b.b));
    let premul = |from: f32, to: f32| from * a.a + (to * b.a - from * a.a) * t;
    Rgba {
        r: premul(ra.0, rb.0) / alpha,
        g: premul(ra.1, rb.1) / alpha,
        b: premul(ra.2, rb.2) / alpha,
        a: alpha,
    }
    .into()
}

/// A pointer-over state that fades instead of flipping.
///
/// [`Hover::track`] reads whether the element under `id` is hovered and returns how far along the
/// fade is: `amount` goes from 0 to 1 over [`FAST`] when the pointer arrives, and back when it
/// leaves, and can reverse half way. Give [`Hover::handler`] to the element's `on_hover`.
pub struct Hover {
    /// 0 with the pointer away, 1 with the pointer over, in between while it fades.
    pub amount: f32,
    flag: Entity<bool>,
}

impl Hover {
    /// Reads the hover state of `id` and its fade.
    pub fn track(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        let flag = window.use_keyed_state(
            ElementId::NamedChild(id.clone().into(), "hover-flag".into()),
            cx,
            |_, _| false,
        );
        let hovered = *flag.read(cx);
        let amount = transition(
            TransitionId::from((id, "hover")),
            if hovered { 1.0_f32 } else { 0.0 },
            quick(),
            window,
            cx,
        );
        Self { amount, flag }
    }

    /// The function to pass to `on_hover`.
    pub fn handler(&self) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        let flag = self.flag.clone();
        move |hovered, window, cx| {
            flag.update(cx, |flag, _| *flag = *hovered);
            window.refresh();
        }
    }

    /// `from` faded toward `to` by the hover amount.
    #[must_use]
    pub fn mix(&self, from: Hsla, to: Hsla) -> Hsla {
        blend(from, to, self.amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{Failure, LocalSession};
    use crate::presentation::{Badge, Tone};

    fn session() -> LocalSession {
        LocalSession {
            endpoint: String::new(),
            server_version: None,
            account_id: "1".to_owned(),
            kind: Badge {
                text: "DEMO".to_owned(),
                tone: Tone::Good,
            },
        }
    }

    fn all() -> Vec<Screen> {
        vec![
            Screen::Choose,
            Screen::Searching,
            Screen::LocalFound(session()),
            Screen::LocalNotFound(Failure::other("x")),
            Screen::Token { refused: None },
            Screen::Verifying {
                hint: String::new(),
            },
            Screen::Connected,
        ]
    }

    #[test]
    fn going_deeper_goes_forward_and_coming_out_goes_back() {
        assert_eq!(
            direction(&Screen::Choose, &Screen::Searching),
            Direction::Forward
        );
        assert_eq!(
            direction(&Screen::Searching, &Screen::LocalFound(session())),
            Direction::Forward
        );
        assert_eq!(
            direction(&Screen::LocalFound(session()), &Screen::Connected),
            Direction::Forward
        );
        assert_eq!(
            direction(&Screen::Token { refused: None }, &Screen::Choose),
            Direction::Back
        );
        assert_eq!(
            direction(
                &Screen::Verifying {
                    hint: String::new()
                },
                &Screen::Token { refused: None }
            ),
            Direction::Back,
            "a refused token returns to the form"
        );
        assert_eq!(
            direction(&Screen::Connected, &Screen::Choose),
            Direction::Back
        );
    }

    #[test]
    fn a_change_between_neighbours_goes_forward() {
        assert_eq!(
            direction(&Screen::Token { refused: None }, &Screen::Searching),
            Direction::Forward
        );
    }

    #[test]
    fn every_screen_has_a_depth_and_the_first_is_the_shallowest() {
        for screen in all() {
            assert!(depth(&screen) >= depth(&Screen::Choose));
        }
        assert!(depth(&Screen::Connected) > depth(&Screen::Searching));
    }

    #[test]
    fn a_change_of_data_on_the_same_page_is_not_a_change_of_screen() {
        let failure = Failure::other("x");
        assert!(same_page(
            &Screen::Token { refused: None },
            &Screen::Token {
                refused: Some(failure)
            }
        ));
        assert!(!same_page(&Screen::Choose, &Screen::Searching));
    }

    #[test]
    fn a_screen_arrives_from_its_side_and_lands_at_zero() {
        assert!((slide(Direction::Forward, true, 0.0) - SLIDE_IN).abs() < 1e-4);
        assert!((slide(Direction::Back, true, 0.0) + SLIDE_IN).abs() < 1e-4);
        assert_eq!(slide(Direction::Forward, true, 1.0), 0.0);
    }

    #[test]
    fn a_screen_leaves_toward_the_other_side_and_less_far() {
        let out = slide(Direction::Forward, false, 0.0);
        assert!(out < 0.0);
        assert!(out.abs() < SLIDE_IN);
        assert_eq!(slide(Direction::Forward, false, 1.0), 0.0);
    }

    #[test]
    fn leaving_is_faster_than_arriving() {
        assert!(LEAVE * 2 <= SLOW);
        assert!(FAST <= NORMAL && NORMAL <= SLOW);
    }

    #[test]
    fn a_card_is_in_place_well_before_half_a_second() {
        let pieces = 7_u32;
        let last_start = SCREEN_GAP + RISE_START + STAGGER * (pieces - 1);
        assert!(last_start + RISE_TIME < Duration::from_millis(600));
    }

    #[test]
    fn the_curves_start_at_zero_and_end_at_one() {
        for curve in [enter(), exit(), standard()] {
            assert!(curve.sample(0.0).abs() < 1e-4, "{curve:?}");
            assert!((curve.sample(1.0) - 1.0).abs() < 1e-4, "{curve:?}");
        }
    }

    #[test]
    fn an_arrival_covers_most_of_its_distance_early_and_a_departure_late() {
        assert!(enter().sample(0.3) > 0.6, "decelerates");
        assert!(exit().sample(0.3) < 0.25, "accelerates");
    }

    #[test]
    fn fading_a_dark_ground_toward_faint_white_never_flashes_light() {
        let ground = Rgba {
            r: 0.04,
            g: 0.04,
            b: 0.04,
            a: 1.0,
        };
        let faint = Rgba::from(crate::ui::theme::over(
            ground.into(),
            crate::ui::theme::fg(),
            0.04,
        ));
        for i in 0..=20 {
            let mixed = Rgba::from(blend(ground.into(), faint.into(), i as f32 / 20.0));
            assert!(mixed.r < 0.10, "step {i}: {mixed:?} is too light");
            assert!((mixed.a - 1.0).abs() < 0.05, "stays opaque");
        }
    }

    #[test]
    fn a_fade_from_transparent_only_changes_the_opacity() {
        let clear = Hsla {
            a: 0.0,
            ..theme_gray()
        };
        let solid = theme_gray();
        let half = Rgba::from(blend(clear, solid, 0.5));
        let want = Rgba::from(solid);
        assert!((half.r - want.r).abs() < 1e-3 && (half.g - want.g).abs() < 1e-3);
        assert!((half.a - 0.5).abs() < 1e-3);
    }

    #[test]
    fn a_blend_starts_at_its_first_color_and_ends_at_its_second() {
        let (a, b) = (
            theme_gray(),
            Hsla {
                l: 0.9,
                ..theme_gray()
            },
        );
        assert_eq!(Rgba::from(blend(a, b, 0.0)), Rgba::from(a));
        let end = Rgba::from(blend(a, b, 1.0));
        assert!((end.r - Rgba::from(b).r).abs() < 1e-4);
    }

    fn theme_gray() -> Hsla {
        Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.15,
            a: 1.0,
        }
    }

    #[test]
    fn the_shake_is_bounded_dies_out_and_starts_and_ends_at_rest() {
        assert_eq!(shake(0.0), 0.0);
        assert!(shake(1.0).abs() < 1e-4);
        let peak = (0..=100)
            .map(|i| shake(i as f32 / 100.0).abs())
            .fold(0.0_f32, f32::max);
        assert!(peak > 4.0 && peak <= 8.0, "peak {peak}");
        assert!(shake(0.9).abs() < 1.0, "quiet at the end");
    }

    #[test]
    fn the_overshoot_goes_past_one_then_lands_on_it() {
        assert!(overshoot(0.0).abs() < 1e-4);
        assert!((overshoot(1.0) - 1.0).abs() < 1e-4);
        let peak = (1..100)
            .map(|i| overshoot(i as f32 / 100.0))
            .fold(0.0_f32, f32::max);
        assert!(peak > 1.02 && peak < 1.2, "peak {peak}");
    }
}
