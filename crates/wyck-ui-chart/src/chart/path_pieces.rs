//! Cutting a line down to what a GPU path can hold.
//!
//! A path is tessellated into at most 65 535 vertices, and a dashed line with a few thousand
//! dashes has more than that: the path is then refused, and the line vanishes from the frame until
//! the view changes. A line that is far longer than the screen (a ray, an extended line, a level
//! of a drawing zoomed in) has exactly that many dashes. So a line is cut to the part that shows,
//! and a dashed line is cut into its dashes, which keep the place they have along the whole line,
//! so they do not creep when the view moves.

use wyck_chart::P;

/// A rectangle, as left, top, right, bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub(crate) left: f32,
    pub(crate) top: f32,
    pub(crate) right: f32,
    pub(crate) bottom: f32,
}

impl Rect {
    /// The rectangle grown by `by` on every side.
    pub(crate) fn grown(self, by: f32) -> Self {
        Self {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }

    /// The length of its diagonal.
    fn diagonal(self) -> f32 {
        (self.right - self.left).hypot(self.bottom - self.top)
    }
}

/// The part of the segment `a` to `b` inside `rect`, as the two fractions of the segment that bound
/// it (Liang-Barsky), or `None` when none of it is inside.
fn clip_segment(a: P, b: P, rect: Rect) -> Option<(f32, f32)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    for (p, q) in [
        (-dx, a.0 - rect.left),
        (dx, rect.right - a.0),
        (-dy, a.1 - rect.top),
        (dy, rect.bottom - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

fn lerp(a: P, b: P, t: f32) -> P {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn finite(p: P) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

/// The parts of a line that show in `rect`, each a polyline. A point that is not a number cuts the
/// line there.
#[expect(
    clippy::float_cmp,
    reason = "`clip_segment` starts at exactly 1.0 and only lowers it, so 1.0 means the end was not clipped"
)]
pub(crate) fn solid(points: &[P], rect: Rect) -> Vec<Vec<P>> {
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut open = false;
    for pair in points.windows(2) {
        let &[a, b] = pair else {
            continue;
        };
        let clipped = if finite(a) && finite(b) {
            clip_segment(a, b, rect)
        } else {
            None
        };
        let Some((t0, t1)) = clipped else {
            open = false;
            continue;
        };
        let (from, to) = (lerp(a, b, t0), lerp(a, b, t1));
        // A segment that starts where the last one ended goes on the same polyline.
        match out.last_mut() {
            Some(line) if open && t0 == 0.0 => line.push(to),
            _ => out.push(vec![from, to]),
        }
        open = t1 == 1.0;
    }
    out
}

/// The dashes of a line that show in `rect`, each a polyline. `on` and `off` are the lengths of a
/// dash and of the gap after it, from the start of the line. Nothing is made for what is off
/// screen, so the work does not grow with the length of the line.
pub(crate) fn dashes(points: &[P], on: f32, off: f32, rect: Rect) -> Vec<Vec<P>> {
    let period = on + off;
    if !(on.is_finite() && off.is_finite() && on > 0.0 && off >= 0.0 && period > 0.0) {
        return solid(points, rect);
    }
    // A dash shorter than a fraction of a pixel is not worth the vertices it costs: the line
    // is drawn solid when the dashes would be more than the screen can show.
    if rect.diagonal() / period > 20_000.0 {
        return solid(points, rect);
    }
    let mut out: Vec<Vec<P>> = Vec::new();
    // Where the line is in the pattern: the length from the start of the line, modulo the period.
    let mut phase = 0.0_f32;
    let mut current: Option<Vec<P>> = None;
    for pair in points.windows(2) {
        let &[a, b] = pair else {
            continue;
        };
        if !(finite(a) && finite(b)) {
            if let Some(dash) = current.take() {
                out.push(dash);
            }
            continue;
        }
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        if length <= 0.0 {
            continue;
        }
        let Some((t0, t1)) = clip_segment(a, b, rect) else {
            // Off screen: the pattern moves on, and a dash that was open ends.
            if let Some(dash) = current.take() {
                out.push(dash);
            }
            phase = (phase + length) % period;
            continue;
        };
        // Enter the pattern where the visible part starts.
        let mut at = length * t0;
        let end = length * t1;
        if t0 > 0.0
            && let Some(dash) = current.take()
        {
            out.push(dash);
        }
        let mut in_pattern = (phase + at) % period;
        while at < end {
            if in_pattern < on {
                let stop = (at + (on - in_pattern)).min(end);
                let (p0, p1) = (lerp(a, b, at / length), lerp(a, b, stop / length));
                match current.as_mut() {
                    // A dash that went round a corner goes on from the corner.
                    Some(dash) => dash.push(p1),
                    None => current = Some(vec![p0, p1]),
                }
                if in_pattern + (stop - at) >= on {
                    // The dash is over (it does not go on in the next segment).
                    if let Some(dash) = current.take() {
                        out.push(dash);
                    }
                }
                in_pattern = (in_pattern + (stop - at)) % period;
                at = stop;
            } else {
                // In a gap: a dash cannot be open.
                if let Some(dash) = current.take() {
                    out.push(dash);
                }
                let stop = (at + (period - in_pattern)).min(end);
                in_pattern = (in_pattern + (stop - at)) % period;
                at = stop;
            }
        }
        phase = (phase + length) % period;
    }
    if let Some(dash) = current.take() {
        out.push(dash);
    }
    out
}

/// A polygon cut to `rect` (Sutherland-Hodgman). What shows of it is the same, and its points
/// cannot be far enough to spoil the tessellation. A point that is not a number is dropped.
#[expect(
    clippy::suspicious_operation_groupings,
    reason = "an edge on a vertical side divides by the horizontal span, one on a horizontal side by the vertical span"
)]
pub(crate) fn clip_polygon(points: &[P], rect: Rect) -> Vec<P> {
    type Inside = fn(P, Rect) -> bool;
    type Cross = fn(P, P, Rect) -> P;
    let mut poly: Vec<P> = points.iter().copied().filter(|p| finite(*p)).collect();
    // Each edge of the rectangle keeps what is on its inner side.
    let edges: [(Inside, Cross); 4] = [
        (
            |p, r| p.0 >= r.left,
            |a, b, r| (r.left, a.1 + (b.1 - a.1) * (r.left - a.0) / (b.0 - a.0)),
        ),
        (
            |p, r| p.0 <= r.right,
            |a, b, r| (r.right, a.1 + (b.1 - a.1) * (r.right - a.0) / (b.0 - a.0)),
        ),
        (
            |p, r| p.1 >= r.top,
            |a, b, r| (a.0 + (b.0 - a.0) * (r.top - a.1) / (b.1 - a.1), r.top),
        ),
        (
            |p, r| p.1 <= r.bottom,
            |a, b, r| (a.0 + (b.0 - a.0) * (r.bottom - a.1) / (b.1 - a.1), r.bottom),
        ),
    ];
    for (inside, cross) in edges {
        if poly.is_empty() {
            break;
        }
        let input = std::mem::take(&mut poly);
        let Some(&last) = input.last() else {
            break;
        };
        let mut previous = last;
        for current in input {
            match (inside(previous, rect), inside(current, rect)) {
                (true, true) => poly.push(current),
                (true, false) => poly.push(cross(previous, current, rect)),
                (false, true) => {
                    poly.push(cross(previous, current, rect));
                    poly.push(current);
                }
                (false, false) => {}
            }
            previous = current;
        }
    }
    poly
}

/// Splits `pieces` into groups holding at most `budget` points each, to be one path each.
pub(crate) fn batches(pieces: Vec<Vec<P>>, budget: usize) -> Vec<Vec<Vec<P>>> {
    let mut out: Vec<Vec<Vec<P>>> = Vec::new();
    let mut count = 0;
    for piece in pieces {
        // A piece of its own longer than the budget is cut, its parts sharing an end point.
        if piece.len() > budget {
            let mut start = 0;
            while start + 1 < piece.len() {
                let end = (start + budget).min(piece.len());
                if let Some(part) = piece.get(start..end) {
                    out.push(vec![part.to_vec()]);
                }
                start = end - 1;
            }
            count = budget;
            continue;
        }
        if out.is_empty() || count + piece.len() > budget {
            out.push(Vec::new());
            count = 0;
        }
        count += piece.len();
        if let Some(group) = out.last_mut() {
            group.push(piece);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{PathBuilder, point, px};

    const SCREEN: Rect = Rect {
        left: 0.0,
        top: 0.0,
        right: 1_000.0,
        bottom: 600.0,
    };

    fn near(a: P, b: P) -> bool {
        (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5
    }

    fn same(a: &[P], b: &[P]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| near(*x, *y))
    }

    /// Whether a GPU path takes these polylines, the way the canvas builds them.
    fn builds(groups: &[Vec<Vec<P>>], dashed: Option<[f32; 2]>) -> bool {
        groups.iter().all(|group| {
            let mut builder = PathBuilder::stroke(px(1.5));
            if let Some([on, off]) = dashed {
                builder = builder.dash_array(&[px(on), px(off)]);
            }
            for piece in group {
                builder.move_to(point(px(piece[0].0), px(piece[0].1)));
                for p in &piece[1..] {
                    builder.line_to(point(px(p.0), px(p.1)));
                }
            }
            builder.build().is_ok()
        })
    }

    #[test]
    fn a_line_far_longer_than_the_screen_is_refused_whole_but_taken_in_pieces() {
        // The line a ray makes when the view is zoomed in: a million pixels, dashed.
        let line = [(-500_000.0, -149_700.0), (500_000.0, 150_300.0)];
        let whole = vec![vec![line.to_vec()]];
        assert!(
            !builds(&whole, Some([2.0, 3.0])),
            "too many vertices as it is"
        );
        let pieces = dashes(&line, 2.0, 3.0, SCREEN.grown(8.0));
        assert_ne!(pieces, Vec::<Vec<P>>::new());
        let groups = batches(pieces, 3_000);
        assert!(builds(&groups, None), "each batch of dashes is taken");
    }

    #[test]
    fn a_solid_line_is_cut_to_the_screen() {
        let line = [(-1e6, 300.0), (1e6, 300.0)];
        let parts = solid(&line, SCREEN);
        assert!(
            parts.len() == 1 && same(&parts[0], &[(0.0, 300.0), (1_000.0, 300.0)]),
            "{parts:?}"
        );
        // Outside altogether: nothing.
        assert_eq!(
            solid(&[(-5.0, -5.0), (-1.0, -10.0)], SCREEN),
            Vec::<Vec<P>>::new()
        );
        // Inside: unchanged.
        let inside = [(10.0, 10.0), (20.0, 50.0), (400.0, 20.0)];
        assert_eq!(solid(&inside, SCREEN), vec![inside.to_vec()]);
    }

    #[test]
    fn a_polyline_that_leaves_and_comes_back_is_two_parts() {
        let line = [
            (100.0, 100.0),
            (100.0, -500.0),
            (300.0, -500.0),
            (300.0, 100.0),
        ];
        let parts = solid(&line, SCREEN);
        assert_eq!(parts.len(), 2, "{parts:?}");
        assert!(
            same(&parts[0], &[(100.0, 100.0), (100.0, 0.0)]),
            "{parts:?}"
        );
        assert!(
            same(&parts[1], &[(300.0, 0.0), (300.0, 100.0)]),
            "{parts:?}"
        );
    }

    #[test]
    fn a_point_that_is_not_a_number_cuts_the_line_instead_of_spoiling_it() {
        let line = [
            (10.0, 10.0),
            (50.0, 10.0),
            (f32::NAN, 10.0),
            (90.0, 10.0),
            (99.0, 10.0),
        ];
        let parts = solid(&line, SCREEN);
        assert_eq!(parts.len(), 2, "{parts:?}");
        assert!(parts.iter().flatten().all(|p| finite(*p)));
    }

    #[test]
    fn dashes_keep_their_place_along_the_whole_line_as_the_view_moves() {
        // Dashes of 4 with gaps of 6, along x, from x = -1000.
        let line = [(-1_000.0, 50.0), (9_000.0, 50.0)];
        let wide = dashes(&line, 4.0, 6.0, SCREEN);
        let narrow = dashes(
            &line,
            4.0,
            6.0,
            Rect {
                left: 200.0,
                right: 500.0,
                ..SCREEN
            },
        );
        // A dash of the narrow view is a dash of the wide one (but for being cut at the edge).
        for dash in &narrow[1..narrow.len() - 1] {
            assert!(
                wide.iter()
                    .any(|d| near(d[0], dash[0]) && near(d[d.len() - 1], dash[dash.len() - 1])),
                "{dash:?}"
            );
        }
        // Every dash starts at a multiple of the period from the start of the line.
        for dash in &wide[1..] {
            let along = dash[0].0 + 1_000.0;
            assert!((along % 10.0).abs() < 1e-2 || (10.0 - along % 10.0).abs() < 1e-2);
            assert!((dash[dash.len() - 1].0 - dash[0].0 - 4.0).abs() < 1e-2);
        }
        // About one dash in ten pixels over 1000 pixels.
        assert!((99..=101).contains(&wide.len()), "{}", wide.len());
    }

    #[test]
    fn a_dash_goes_round_a_corner() {
        let line = [(100.0, 100.0), (110.0, 100.0), (110.0, 110.0)];
        let found = dashes(&line, 15.0, 5.0, SCREEN);
        assert_eq!(found.len(), 1);
        assert!(same(
            &found[0],
            &[(100.0, 100.0), (110.0, 100.0), (110.0, 105.0)]
        ));
    }

    #[test]
    fn dashes_too_fine_to_see_are_drawn_solid() {
        let line = [(0.0, 10.0), (900.0, 10.0)];
        let fine = dashes(&line, 0.01, 0.01, SCREEN);
        assert_eq!(fine, solid(&line, SCREEN));
        // And a bad pattern is a solid line, not a hang.
        assert_eq!(dashes(&line, 0.0, 0.0, SCREEN), solid(&line, SCREEN));
        assert_eq!(dashes(&line, f32::NAN, 1.0, SCREEN), solid(&line, SCREEN));
    }

    #[test]
    fn a_polygon_is_cut_to_the_screen_and_keeps_what_shows() {
        // A box much larger than the screen, with the screen inside it: the screen is all that shows.
        let big = [(-1e6, -1e6), (1e6, -1e6), (1e6, 1e6), (-1e6, 1e6)];
        let cut = clip_polygon(&big, SCREEN);
        let (xs, ys): (Vec<f32>, Vec<f32>) = cut.iter().copied().unzip();
        let min = |v: &[f32]| v.iter().copied().fold(f32::INFINITY, f32::min);
        let max = |v: &[f32]| v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert_eq!(
            (min(&xs), max(&xs), min(&ys), max(&ys)),
            (0.0, 1_000.0, 0.0, 600.0)
        );
        // A triangle inside is unchanged, one outside is nothing.
        let inside = [(10.0, 10.0), (100.0, 10.0), (50.0, 90.0)];
        assert_eq!(clip_polygon(&inside, SCREEN).len(), 3);
        assert_eq!(
            clip_polygon(&[(-50.0, -50.0), (-10.0, -50.0), (-30.0, -10.0)], SCREEN),
            Vec::new()
        );
        // A point that is not a number is left out.
        let spoiled = [(10.0, 10.0), (f32::NAN, 5.0), (100.0, 10.0), (50.0, 90.0)];
        assert!(clip_polygon(&spoiled, SCREEN).iter().all(|p| finite(*p)));
    }

    #[test]
    fn batches_hold_at_most_their_budget_and_lose_nothing() {
        let pieces: Vec<Vec<P>> = (0..10_u8)
            .map(|i| vec![(f32::from(i), 0.0), (f32::from(i), 1.0)])
            .collect();
        let groups = batches(pieces.clone(), 6);
        assert_eq!(groups.len(), 4);
        assert!(
            groups
                .iter()
                .all(|g| g.iter().map(Vec::len).sum::<usize>() <= 6)
        );
        let flat: Vec<Vec<P>> = groups.into_iter().flatten().collect();
        assert_eq!(flat, pieces);
        // One polyline longer than the budget is cut into parts that share their end points.
        let long: Vec<P> = (0..10_u8).map(|i| (f32::from(i), 0.0)).collect();
        let groups = batches(vec![long], 4);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0][0].last(), groups[1][0].first());
    }
}
