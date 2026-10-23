//! Colors used by chart drawing commands, independent of the desktop renderer.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsla {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}

pub(crate) fn rgb(hex: u32) -> Rgba {
    let [_, red, green, blue] = hex.to_be_bytes();
    Rgba {
        r: f32::from(red) / 255.0,
        g: f32::from(green) / 255.0,
        b: f32::from(blue) / 255.0,
        a: 1.0,
    }
}

pub(crate) fn color_hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla {
        h: h.clamp(0.0, 1.0),
        s: s.clamp(0.0, 1.0),
        l: l.clamp(0.0, 1.0),
        a: a.clamp(0.0, 1.0),
    }
}

pub(crate) fn transparent_black() -> Hsla {
    Hsla {
        h: 0.0,
        s: 0.0,
        l: 0.0,
        a: 0.0,
    }
}

impl From<Rgba> for Hsla {
    #[expect(
        clippy::float_cmp,
        reason = "`max` is one of `r`, `g` and `b` picked with `f32::max`, and `l` is exactly 0 or 1 only for black and white, so these comparisons are exact"
    )]
    fn from(color: Rgba) -> Self {
        let Rgba { r, g, b, a } = color;
        let max = r.max(g.max(b));
        let min = r.min(g.min(b));
        let delta = max - min;
        let l = f32::midpoint(max, min);
        let s = if l == 0.0 || l == 1.0 {
            0.0
        } else if l < 0.5 {
            delta / (2.0 * l)
        } else {
            delta / (2.0 - 2.0 * l)
        };
        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            ((g - b) / delta).rem_euclid(6.0) / 6.0
        } else if max == g {
            ((b - r) / delta + 2.0) / 6.0
        } else {
            ((r - g) / delta + 4.0) / 6.0
        };
        Self { h, s, l, a }
    }
}
