//! A picture of a chart: the same drawing commands the screen gets (`wyck_chart::build`), painted
//! into pixels in memory with `tiny-skia`, text included (the app's own Inter font, rasterized by
//! `ab_glyph`), and encoded as PNG.
//!
//! It does not read the window back (gpui offers no way to on every platform): the frame is
//! drawn again, off screen, at twice the size for a crisp image.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use tiny_skia::{
    FillRule, Mask, Paint, PathBuilder, Pixmap, PremultipliedColorU8, Rect, Stroke, StrokeDash,
    Transform,
};
use wyck_chart::Hsla;
use wyck_chart::{fitted_origin, fitted_size};

use wyck_chart::Cmd;
use wyck_core::math::{f32_to_i32, f32_to_u32};
use wyck_ui_kit::assets;

/// A clip rectangle, in logical pixels.
#[derive(Debug, Clone, Copy)]
struct Clip {
    l: f32,
    t: f32,
    r: f32,
    b: f32,
}

impl Clip {
    fn intersect(self, other: Self) -> Self {
        Self {
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
    let rgba: gpui::Rgba = super::palette::gpui_hsla(hsla).into();
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
fn rounded_rect(
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> Option<tiny_skia::Path> {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    if radius <= 0.01 {
        return Some(PathBuilder::from_rect(Rect::from_xywh(
            left, top, width, height,
        )?));
    }
    let (right, bottom) = (left + width, top + height);
    // A cubic with this handle length approximates a quarter circle.
    let k = radius * 0.552_284_8;
    let mut path = PathBuilder::new();
    path.move_to(left + radius, top);
    path.line_to(right - radius, top);
    path.cubic_to(
        right - radius + k,
        top,
        right,
        top + radius - k,
        right,
        top + radius,
    );
    path.line_to(right, bottom - radius);
    path.cubic_to(
        right,
        bottom - radius + k,
        right - radius + k,
        bottom,
        right - radius,
        bottom,
    );
    path.line_to(left + radius, bottom);
    path.cubic_to(
        left + radius - k,
        bottom,
        left,
        bottom - radius + k,
        left,
        bottom - radius,
    );
    path.line_to(left, top + radius);
    path.cubic_to(
        left,
        top + radius - k,
        left + radius - k,
        top,
        left + radius,
        top,
    );
    path.close();
    path.finish()
}

/// How a text is drawn: its size, color and weight.
#[derive(Clone, Copy)]
struct Ink {
    size: f32,
    fill: Hsla,
    bold: bool,
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

    /// Draws `text` with its top-left at `(left, top)` (logical pixels).
    fn text(&mut self, text: &str, left: f32, top: f32, ink: Ink, clip: Clip) {
        let Ink { size, fill, bold } = ink;
        let px_size = size * self.scale;
        let font = self.font.as_scaled(PxScale::from(px_size));
        let ink_color = color(fill);
        let line_top = top * self.scale + (size * 1.3 * self.scale - px_size * 1.2) / 2.0;
        let baseline = line_top + font.ascent();
        let mut pen = left * self.scale;
        let mut previous = None;
        let (clip_left, clip_top, clip_right, clip_bottom) = (
            f32_to_i32((clip.l * self.scale).floor()),
            f32_to_i32((clip.t * self.scale).floor()),
            f32_to_i32((clip.r * self.scale).ceil()),
            f32_to_i32((clip.b * self.scale).ceil()),
        );
        let (width, height) = (signed(self.pixmap.width()), signed(self.pixmap.height()));
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
                let px = f32_to_i32(bounds.min.x) + signed(gx);
                let py = f32_to_i32(bounds.min.y) + signed(gy);
                if px < clip_left.max(0)
                    || py < clip_top.max(0)
                    || px >= clip_right.min(width)
                    || py >= clip_bottom.min(height)
                {
                    return;
                }
                // A little extra weight for bold text.
                let coverage = if bold {
                    (coverage * 1.25).min(1.0)
                } else {
                    coverage
                };
                let alpha = coverage * ink_color.alpha();
                if alpha <= 0.0 {
                    return;
                }
                let Some(slot) = usize::try_from(py * width + px)
                    .ok()
                    .and_then(|index| pixels.get_mut(index))
                else {
                    return;
                };
                let behind = *slot;
                let blend = |below: u8, source: f32| -> u8 {
                    channel(source * 255.0 * alpha + f32::from(below) * (1.0 - alpha))
                };
                let opacity = channel(alpha * 255.0 + f32::from(behind.alpha()) * (1.0 - alpha));
                let (red, green, blue) = (
                    blend(behind.red(), ink_color.red()),
                    blend(behind.green(), ink_color.green()),
                    blend(behind.blue(), ink_color.blue()),
                );
                if let Some(pixel) = PremultipliedColorU8::from_rgba(
                    red.min(opacity),
                    green.min(opacity),
                    blue.min(opacity),
                    opacity,
                ) {
                    *slot = pixel;
                }
            };
            outlined.draw(&mut plot);
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
                    let Some((&(x, y), rest)) = points.split_first() else {
                        continue;
                    };
                    let mut pb = PathBuilder::new();
                    pb.move_to(x, y);
                    for (px, py) in rest {
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
                    let Some((&(x, y), rest)) = points.split_first() else {
                        continue;
                    };
                    let mut pb = PathBuilder::new();
                    pb.move_to(x, y);
                    for (px, py) in rest {
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
                    ..
                } => {
                    let width = self.measure(text, *size);
                    let x = align.left_of(*x, width);
                    self.text(
                        text,
                        x,
                        *y,
                        Ink {
                            size: *size,
                            fill: *text_color,
                            bold: *bold,
                        },
                        clip,
                    );
                }
                Cmd::FittedText {
                    text,
                    x,
                    y,
                    w,
                    h,
                    size,
                    color: text_color,
                    background,
                    align,
                    valign,
                    bold,
                    ..
                } => {
                    let Some(font_size) =
                        fitted_size(text, *size, *w, *h, |s| self.measure(text, s))
                    else {
                        continue;
                    };
                    let text_w = self.measure(text, font_size);
                    let (left, top) =
                        fitted_origin((*x, *y, *w, *h), text_w, font_size, *align, *valign);
                    let label_clip = clip.intersect(Clip {
                        l: *x,
                        t: *y,
                        r: x + w,
                        b: y + h,
                    });
                    if let Some(bg) = background
                        && let Some(path) = rounded_rect(
                            left - 3.0,
                            top - 1.0,
                            text_w + 6.0,
                            font_size * 1.3 + 2.0,
                            3.0,
                        )
                    {
                        let label_mask = self.mask(label_clip);
                        self.pixmap.fill_path(
                            &path,
                            &paint(*bg),
                            FillRule::Winding,
                            transform,
                            label_mask.as_ref(),
                        );
                    }
                    self.text(
                        text,
                        left,
                        top,
                        Ink {
                            size: font_size,
                            fill: *text_color,
                            bold: *bold,
                        },
                        label_clip,
                    );
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
                    size,
                    bold,
                    ..
                } => {
                    let text_w = self.measure(text, *size);
                    let width = fixed_width.unwrap_or(text_w + pad * 2.0);
                    let mut left = align.left_of(*x, width);
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
                        y + (height - size * 1.3) / 2.0,
                        Ink {
                            size: *size,
                            fill: *fg,
                            bold: *bold,
                        },
                        clip,
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

/// A size in pixels as the signed integer the glyph arithmetic works in.
fn signed(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// A color channel from a blended value, kept between 0 and 255.
fn channel(value: f32) -> u8 {
    u8::try_from(f32_to_u32(value.round().clamp(0.0, 255.0))).unwrap_or(u8::MAX)
}

/// Why a picture of a chart could not be made.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RasterError {
    #[error("the font of the picture cannot be read: {0}")]
    Font(#[from] ab_glyph::InvalidFont),
    #[error("the picture is too large")]
    TooLarge,
    #[error("the picture cannot be encoded: {0}")]
    Encode(String),
}

impl RasterError {
    fn encode(error: impl std::fmt::Display) -> Self {
        Self::Encode(error.to_string())
    }
}

/// Device pixels per logical pixel in a picture: twice the screen, for a crisp image.
pub(crate) const PICTURE_SCALE: f32 = 2.0;

/// A line of text over the picture: what it says, its size and color, and whether it is bold.
pub(crate) struct Caption {
    pub(crate) text: String,
    pub(crate) size: f32,
    pub(crate) color: Hsla,
    pub(crate) bold: bool,
}

/// Paints `cmds` (for a chart of `w` by `h` logical pixels, drawn from the origin) at `scale`
/// device pixels per logical pixel on `background`, with `captions` stacked at the top left, and
/// encodes the result as PNG.
#[cfg(test)]
pub(crate) fn render_png(
    cmds: &[Cmd],
    w: f32,
    h: f32,
    scale: f32,
    background: Hsla,
    captions: &[Caption],
) -> Result<Vec<u8>, RasterError> {
    render_pixmap(cmds, w, h, scale, background, captions)?
        .encode_png()
        .map_err(RasterError::encode)
}

/// `render_png` without the encoding: the pixels, to be put in a larger picture (see
/// [`compose_png`]).
pub(crate) fn render_pixmap(
    cmds: &[Cmd],
    w: f32,
    h: f32,
    scale: f32,
    background: Hsla,
    captions: &[Caption],
) -> Result<Pixmap, RasterError> {
    let font = FontRef::try_from_slice(assets::FONT)?;
    let (pw, ph) = (
        f32_to_u32((w * scale).round()),
        f32_to_u32((h * scale).round()),
    );
    let mut pixmap = Pixmap::new(pw.max(1), ph.max(1)).ok_or(RasterError::TooLarge)?;
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
            Ink {
                size: caption.size,
                fill: caption.color,
                bold: caption.bold,
            },
            everything,
        );
        y += caption.size * 1.45;
    }
    Ok(canvas.pixmap)
}

/// Puts pictures side by side in one: a picture of `w` by `h` logical pixels at `scale` on
/// `background`, with each tile drawn at its own top left corner (in logical pixels), and encodes
/// it as PNG. What a tile does not cover shows the background; a tile that reaches past the edge
/// is cut.
pub(crate) fn compose_png(
    w: f32,
    h: f32,
    scale: f32,
    background: Hsla,
    tiles: &[(f32, f32, Pixmap)],
) -> Result<Vec<u8>, RasterError> {
    let (pw, ph) = (
        f32_to_u32((w * scale).round()),
        f32_to_u32((h * scale).round()),
    );
    let mut pixmap = Pixmap::new(pw.max(1), ph.max(1)).ok_or(RasterError::TooLarge)?;
    pixmap.fill(color(background));
    for (x, y, tile) in tiles {
        pixmap.draw_pixmap(
            f32_to_i32((x * scale).round()),
            f32_to_i32((y * scale).round()),
            tile.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    pixmap.encode_png().map_err(RasterError::encode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyck_chart::Align;
    use wyck_core::math::{f64_to_i64, i64_to_f64};
    use wyck_drawings::look::Face;

    /// A tile of one flat color.
    fn tile(w: u32, h: u32, red: u8) -> Pixmap {
        let mut pixmap = Pixmap::new(w, h).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(red, 0, 0, 255));
        pixmap
    }

    #[test]
    fn tiles_are_put_where_they_say_in_a_picture_of_their_own_size() {
        let background = rgb_alpha(0x0000_0000, 1.0);
        // Two tiles of 20 by 10 logical pixels side by side, at twice the size.
        let tiles = vec![
            (0.0, 0.0, tile(40, 20, 200)),
            (20.0, 0.0, tile(40, 20, 100)),
        ];
        let png = compose_png(40.0, 10.0, 2.0, background, &tiles).unwrap();
        let picture = Pixmap::decode_png(&png).unwrap();
        assert_eq!((picture.width(), picture.height()), (80, 20));
        assert_eq!(picture.pixel(5, 5).unwrap().red(), 200);
        assert_eq!(picture.pixel(60, 5).unwrap().red(), 100);
    }

    #[test]
    fn a_tile_past_the_edge_is_cut_and_a_gap_shows_the_background() {
        let background = rgb_alpha(0x0000_0000, 1.0);
        let tiles = vec![(10.0, 0.0, tile(40, 20, 255))];
        let png = compose_png(20.0, 10.0, 2.0, background, &tiles).unwrap();
        let picture = Pixmap::decode_png(&png).unwrap();
        assert_eq!((picture.width(), picture.height()), (40, 20));
        assert_eq!(picture.pixel(2, 5).unwrap().red(), 0, "the gap");
        assert_eq!(picture.pixel(39, 5).unwrap().red(), 255, "the cut tile");
    }
    use wyck_chart::{font, rgb_alpha};

    #[test]
    #[ignore = "writes a chart preview for visual review"]
    fn indicator_families_preview() {
        use wyck_chart::ChartSettings;
        use wyck_chart::Display;
        use wyck_chart::Palette;
        use wyck_chart::Series;
        use wyck_chart::View;
        use wyck_chart::{Frame, build};
        use wyck_indicators::config::StudyConfig;
        use wyck_indicators::kind::StudyKind;
        use wyck_market::Bar;
        use wyck_market::Timeframe;

        let bars: Vec<Bar> = (0..240)
            .map(|i| {
                let base = 100_000
                    + f64_to_i64(
                        (i64_to_f64(i) * 0.17)
                            .sin()
                            .mul_add(350.0, i64_to_f64(i) * 3.0),
                    );
                Bar {
                    time_ms: 1_767_571_200_000 + i * 60_000,
                    open: base,
                    high: base + 55,
                    low: base - 50,
                    close: base + 12,
                    volume: 10 + i % 8,
                }
            })
            .collect();
        let raw = Series::Bars(bars);
        let mut settings = ChartSettings::default();
        for kind in [
            StudyKind::Bollinger,
            StudyKind::Vwap,
            StudyKind::Rsi,
            StudyKind::Macd,
            StudyKind::Volume,
        ] {
            settings.studies.push(StudyConfig::new(kind));
        }
        settings.studies[1]
            .plots
            .get_mut("upper_1")
            .unwrap()
            .visible = true;
        settings.studies[1]
            .plots
            .get_mut("lower_1")
            .unwrap()
            .visible = true;
        let display = Display::build(&raw, &settings, 1);
        let view = View::new(5.0);
        let frame = Frame {
            raw: &raw,
            display: &display,
            settings: &settings,
            view: &view,
            timeframe: Timeframe::DEFAULT,
            digits: 5,
            pip_position: Some(4),
            origin: (0.0, 0.0),
            w: 1_400.0,
            h: 900.0,
            scale: 1.0,
            hover: None,
            remote: None,
            ask: None,
            now_ms: 1_767_571_200_000,
            palette: Palette::new(),
            drawings: None,
            marks: &[],
            flow: None,
            watermark: None,
        };
        let cmds = build(&frame);
        let png = render_png(&cmds, 1_400.0, 900.0, 1.0, rgb_alpha(0x000a_0a0a, 1.0), &[]).unwrap();
        let path = std::env::temp_dir().join("wyck-indicator-families.png");
        std::fs::write(&path, png).unwrap();
        println!("preview written to {}", path.display());
    }

    #[test]
    fn a_frame_becomes_a_png_of_the_right_size() {
        let cmds = vec![
            Cmd::Rect {
                x: 10.0,
                y: 10.0,
                w: 50.0,
                h: 20.0,
                fill: rgb_alpha(0x0026_a69a, 1.0),
                border: Some((1.0, rgb_alpha(0x00ff_ffff, 1.0))),
                radius: 4.0,
            },
            Cmd::Stroke {
                points: vec![(0.0, 0.0), (100.0, 50.0), (200.0, 10.0)],
                width: 1.5,
                color: rgb_alpha(0x005b_8def, 1.0),
                dash: Some([4.0, 2.0]),
            },
            Cmd::Clip {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
                inner: vec![Cmd::Fill {
                    points: vec![(0.0, 0.0), (300.0, 0.0), (0.0, 300.0)],
                    color: rgb_alpha(0x00ef_5350, 0.5),
                }],
            },
            Cmd::Text {
                text: "1.08412".into(),
                x: 50.0,
                y: 60.0,
                size: font(),
                color: rgb_alpha(0x00ff_ffff, 1.0),
                align: Align::Center,
                bold: false,
                face: Face::default(),
            },
            Cmd::Tag {
                text: "UTC".into(),
                x: 150.0,
                y: 80.0,
                height: 18.0,
                pad: 6.0,
                bg: rgb_alpha(0x0036_3a45, 1.0),
                fg: rgb_alpha(0x00ff_ffff, 1.0),
                align: Align::Left,
                fixed_width: None,
                within: Some((0.0, 200.0)),
                size: font(),
                bold: false,
                face: Face::default(),
            },
        ];
        let png = render_png(
            &cmds,
            200.0,
            100.0,
            2.0,
            rgb_alpha(0x000a_0a0a, 1.0),
            &[Caption {
                text: "EURUSD M5".into(),
                size: 14.0,
                color: rgb_alpha(0x00ff_ffff, 1.0),
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
        let short = canvas.measure("1.0", wyck_chart::font());
        let long = canvas.measure("1.08412", wyck_chart::font());
        assert!(short > 0.0 && long > short * 1.8);
    }

    #[test]
    fn fitted_label_in_png_stays_inside_its_box() {
        let png = render_png(
            &[Cmd::FittedText {
                text: "FVG".to_owned(),
                x: 10.0,
                y: 10.0,
                w: 40.0,
                h: 20.0,
                size: 14.0,
                color: rgb_alpha(0x00ff_ffff, 1.0),
                background: Some(rgb_alpha(0x0022_6666, 1.0)),
                align: Align::Center,
                valign: wyck_drawings::look::VAlign::Auto,
                bold: false,
                face: Face::default(),
            }],
            80.0,
            50.0,
            2.0,
            rgb_alpha(0x0000_0000, 1.0),
            &[],
        )
        .unwrap();
        let image = Pixmap::decode_png(&png).unwrap();
        let background = image.pixel(0, 0).unwrap();
        assert_ne!(image.pixel(60, 40).unwrap(), background);
        for y in 0..100 {
            for x in 0..160 {
                if !(20..100).contains(&x) || !(20..60).contains(&y) {
                    assert_eq!(image.pixel(x, y).unwrap(), background);
                }
            }
        }
    }
}
