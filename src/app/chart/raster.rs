//! A picture of a chart: the same drawing commands the screen gets ([`super::scene`]), painted
//! into pixels in memory with `tiny-skia`, text included (the app's own Inter font, rasterized by
//! `ab_glyph`), and encoded as PNG.
//!
//! It does not read the window back (gpui offers no way to on every platform): the frame is
//! drawn again, off screen, at twice the size for a crisp image.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use gpui::Hsla;
use tiny_skia::{
    FillRule, Mask, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Stroke, StrokeDash,
    Transform,
};

use super::scene::{Align, Cmd, LINE};
use crate::app::assets;

/// A clip rectangle, in logical pixels.
#[derive(Debug, Clone, Copy)]
struct Clip {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}

impl Clip {
    fn intersect(self, other: Clip) -> Clip {
        Clip {
            l: self.l.max(other.l),
            t: self.t.max(other.t),
            r: self.r.min(other.r),
            b: self.b.min(other.b),
        }
    }

    fn is_empty(&self) -> bool {
        self.r <= self.l || self.b <= self.t
    }
}

struct Canvas<'a> {
    pixmap: Pixmap,
    scale: f32,
    font: FontRef<'a>,
}

fn color(hsla: Hsla) -> tiny_skia::Color {
    let rgba: gpui::Rgba = hsla.into();
    tiny_skia::Color::from_rgba(
        rgba.r.clamp(0.0, 1.0),
        rgba.g.clamp(0.0, 1.0),
        rgba.b.clamp(0.0, 1.0),
        rgba.a.clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::BLACK)
}

fn paint(hsla: Hsla) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color(hsla));
    paint.anti_alias = true;
    paint
}

/// A rectangle with rounded corners as a path.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r <= 0.01 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?));
    }
    // A cubic with this handle length approximates a quarter circle.
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

impl Canvas<'_> {
    fn transform(&self) -> Transform {
        Transform::from_scale(self.scale, self.scale)
    }

    fn mask(&self, clip: Clip) -> Option<Mask> {
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
        let rect = Rect::from_ltrb(clip.l, clip.t, clip.r, clip.b)?;
        mask.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            false,
            self.transform(),
        );
        Some(mask)
    }

    /// The width of `text` at `size` logical pixels.
    fn measure(&self, text: &str, size: f32) -> f32 {
        let font = self.font.as_scaled(PxScale::from(size));
        let mut width = 0.0;
        let mut previous = None;
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            if let Some(prev) = previous {
                width += font.kern(prev, id);
            }
            width += font.h_advance(id);
            previous = Some(id);
        }
        width
    }

    /// Draws `text` with its top-left at `(x, y)` (logical pixels).
    fn text(&mut self, text: &str, x: f32, y: f32, size: f32, fill: Hsla, clip: Clip, bold: bool) {
        let px_size = size * self.scale;
        let font = self.font.as_scaled(PxScale::from(px_size));
        let c = color(fill);
        let line_top = y * self.scale + (LINE * self.scale - px_size * 1.2) / 2.0;
        let baseline = line_top + font.ascent();
        let mut pen = x * self.scale;
        let mut previous = None;
        let (cl, ct, cr, cb) = (
            (clip.l * self.scale).floor() as i32,
            (clip.t * self.scale).floor() as i32,
            (clip.r * self.scale).ceil() as i32,
            (clip.b * self.scale).ceil() as i32,
        );
        let (width, height) = (self.pixmap.width() as i32, self.pixmap.height() as i32);
        for ch in text.chars() {
            let id = font.glyph_id(ch);
            if let Some(prev) = previous {
                pen += font.kern(prev, id);
            }
            previous = Some(id);
            let glyph = id.with_scale_and_position(px_size, ab_glyph::point(pen, baseline));
            pen += font.h_advance(id);
            let Some(outlined) = self.font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outlined.px_bounds();
            let pixels = self.pixmap.pixels_mut();
            let mut plot = |gx: u32, gy: u32, coverage: f32| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < cl.max(0) || py < ct.max(0) || px >= cr.min(width) || py >= cb.min(height) {
                    return;
                }
                // A little extra weight for bold text.
                let coverage = if bold {
                    (coverage * 1.25).min(1.0)
                } else {
                    coverage
                };
                let alpha = coverage * c.alpha();
                if alpha <= 0.0 {
                    return;
                }
                let index = (py * width + px) as usize;
                let dst = pixels[index];
                let blend = |d: u8, s: f32| -> u8 {
                    (s * 255.0 * alpha + f32::from(d) * (1.0 - alpha))
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                let a = (alpha * 255.0 + f32::from(dst.alpha()) * (1.0 - alpha))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let (r, g, b) = (
                    blend(dst.red(), c.red()),
                    blend(dst.green(), c.green()),
                    blend(dst.blue(), c.blue()),
                );
                if let Some(pixel) =
                    PremultipliedColorU8::from_rgba(r.min(a), g.min(a), b.min(a), a)
                {
                    pixels[index] = pixel;
                }
            };
            outlined.draw(|gx, gy, coverage| plot(gx, gy, coverage));
        }
    }

    fn run(&mut self, cmds: &[Cmd], clip: Clip) {
        if clip.is_empty() {
            return;
        }
        let mask = self.mask(clip);
        let transform = self.transform();
        for cmd in cmds {
            match cmd {
                Cmd::Rect {
                    x,
                    y,
                    w,
                    h,
                    fill,
                    border,
                    radius,
                } => {
                    let Some(path) = rounded_rect(*x, *y, w.max(0.0), h.max(0.0), *radius) else {
                        continue;
                    };
                    if fill.a > 0.0 {
                        self.pixmap.fill_path(
                            &path,
                            &paint(*fill),
                            FillRule::Winding,
                            transform,
                            mask.as_ref(),
                        );
                    }
                    if let Some((width, edge)) = border {
                        let inset = width / 2.0;
                        if let Some(edge_path) = rounded_rect(
                            x + inset,
                            y + inset,
                            (w - width).max(0.0),
                            (h - width).max(0.0),
                            (radius - inset).max(0.0),
                        ) {
                            let stroke = Stroke {
                                width: *width,
                                ..Stroke::default()
                            };
                            self.pixmap.stroke_path(
                                &edge_path,
                                &paint(*edge),
                                &stroke,
                                transform,
                                mask.as_ref(),
                            );
                        }
                    }
                }
                Cmd::Stroke {
                    points,
                    width,
                    color: stroke_color,
                    dash,
                } => {
                    if points.len() < 2 {
                        continue;
                    }
                    let mut pb = PathBuilder::new();
                    pb.move_to(points[0].0, points[0].1);
                    for (px, py) in &points[1..] {
                        pb.line_to(*px, *py);
                    }
                    let Some(path) = pb.finish() else { continue };
                    let stroke = Stroke {
                        width: *width,
                        dash: dash.and_then(|[on, off]| StrokeDash::new(vec![on, off], 0.0)),
                        ..Stroke::default()
                    };
                    self.pixmap.stroke_path(
                        &path,
                        &paint(*stroke_color),
                        &stroke,
                        transform,
                        mask.as_ref(),
                    );
                }
                Cmd::Fill {
                    points,
                    color: fill_color,
                } => {
                    if points.len() < 3 {
                        continue;
                    }
                    let mut pb = PathBuilder::new();
                    pb.move_to(points[0].0, points[0].1);
                    for (px, py) in &points[1..] {
                        pb.line_to(*px, *py);
                    }
                    pb.close();
                    if let Some(path) = pb.finish() {
                        self.pixmap.fill_path(
                            &path,
                            &paint(*fill_color),
                            FillRule::EvenOdd,
                            transform,
                            mask.as_ref(),
                        );
                    }
                }
                Cmd::Text {
                    text,
                    x,
                    y,
                    size,
                    color: text_color,
                    align,
                    bold,
                } => {
                    let width = self.measure(text, *size);
                    let x = aligned(*x, width, *align);
                    self.text(text, x, *y, *size, *text_color, clip, *bold);
                }
                Cmd::Tag {
                    text,
                    x,
                    y,
                    height,
                    pad,
                    bg,
                    fg,
                    align,
                    fixed_width,
                    within,
                } => {
                    let text_w = self.measure(text, super::scene::FONT);
                    let width = fixed_width.unwrap_or(text_w + pad * 2.0);
                    let mut left = aligned(*x, width, *align);
                    if let Some((l, r)) = within {
                        left = left.clamp(*l, (r - width).max(*l));
                    }
                    if let Some(path) = rounded_rect(left, *y, width, *height, 3.0) {
                        self.pixmap.fill_path(
                            &path,
                            &paint(*bg),
                            FillRule::Winding,
                            transform,
                            mask.as_ref(),
                        );
                    }
                    self.text(
                        text,
                        left + pad,
                        y + (height - LINE) / 2.0,
                        super::scene::FONT,
                        *fg,
                        clip,
                        false,
                    );
                }
                Cmd::Clip { x, y, w, h, inner } => {
                    let inner_clip = clip.intersect(Clip {
                        l: *x,
                        t: *y,
                        r: x + w,
                        b: y + h,
                    });
                    self.run(inner, inner_clip);
                }
            }
        }
    }
}

fn aligned(x: f32, width: f32, align: Align) -> f32 {
    match align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
        Align::Right => x - width,
    }
}

/// A line of text over the picture: what it says, its size and color, and whether it is bold.
pub struct Caption {
    pub text: String,
    pub size: f32,
    pub color: Hsla,
    pub bold: bool,
}

/// Paints `cmds` (for a chart of `w` by `h` logical pixels, drawn from the origin) at `scale`
/// device pixels per logical pixel on `background`, with `captions` stacked at the top left, and
/// encodes the result as PNG.
pub fn render_png(
    cmds: &[Cmd],
    w: f32,
    h: f32,
    scale: f32,
    background: Hsla,
    captions: &[Caption],
) -> Result<Vec<u8>, String> {
    let font = FontRef::try_from_slice(assets::FONT).map_err(|e| e.to_string())?;
    let (pw, ph) = ((w * scale).round() as u32, (h * scale).round() as u32);
    let mut pixmap = Pixmap::new(pw.max(1), ph.max(1)).ok_or("the picture is too large")?;
    pixmap.fill(color(background));
    let mut canvas = Canvas {
        pixmap,
        scale,
        font,
    };
    let everything = Clip {
        l: 0.0,
        t: 0.0,
        r: w,
        b: h,
    };
    canvas.run(cmds, everything);
    let mut y = 8.0;
    for caption in captions {
        canvas.text(
            &caption.text,
            12.0,
            y,
            caption.size,
            caption.color,
            everything,
            caption.bold,
        );
        y += caption.size * 1.45;
    }
    canvas.pixmap.encode_png().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chart::scene::{FONT, rgb_alpha};

    #[test]
    fn a_frame_becomes_a_png_of_the_right_size() {
        let cmds = vec![
            Cmd::Rect {
                x: 10.0,
                y: 10.0,
                w: 50.0,
                h: 20.0,
                fill: rgb_alpha(0x26a69a, 1.0),
                border: Some((1.0, rgb_alpha(0xffffff, 1.0))),
                radius: 4.0,
            },
            Cmd::Stroke {
                points: vec![(0.0, 0.0), (100.0, 50.0), (200.0, 10.0)],
                width: 1.5,
                color: rgb_alpha(0x5b8def, 1.0),
                dash: Some([4.0, 2.0]),
            },
            Cmd::Clip {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
                inner: vec![Cmd::Fill {
                    points: vec![(0.0, 0.0), (300.0, 0.0), (0.0, 300.0)],
                    color: rgb_alpha(0xef5350, 0.5),
                }],
            },
            Cmd::Text {
                text: "1.08412".into(),
                x: 50.0,
                y: 60.0,
                size: FONT,
                color: rgb_alpha(0xffffff, 1.0),
                align: Align::Center,
                bold: false,
            },
            Cmd::Tag {
                text: "UTC".into(),
                x: 150.0,
                y: 80.0,
                height: 18.0,
                pad: 6.0,
                bg: rgb_alpha(0x363a45, 1.0),
                fg: rgb_alpha(0xffffff, 1.0),
                align: Align::Left,
                fixed_width: None,
                within: Some((0.0, 200.0)),
            },
        ];
        let png = render_png(
            &cmds,
            200.0,
            100.0,
            2.0,
            rgb_alpha(0x0a0a0a, 1.0),
            &[Caption {
                text: "EURUSD M5".into(),
                size: 14.0,
                color: rgb_alpha(0xffffff, 1.0),
                bold: true,
            }],
        )
        .unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let decoded = Pixmap::decode_png(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (400, 200));
        // Something other than the background was painted.
        let background = decoded.pixel(399, 199).unwrap();
        assert!(decoded.pixels().iter().any(|p| *p != background));
    }

    #[test]
    fn text_is_measured_with_the_font() {
        let font = FontRef::try_from_slice(assets::FONT).unwrap();
        let canvas = Canvas {
            pixmap: Pixmap::new(1, 1).unwrap(),
            scale: 1.0,
            font,
        };
        let short = canvas.measure("1.0", FONT);
        let long = canvas.measure("1.08412", FONT);
        assert!(short > 0.0 && long > short * 1.8);
    }
}
