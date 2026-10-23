//! The canvas of a chart: it measures itself, draws the frame the scene builds, and listens to
//! the pointer.

use std::cell::Cell;
use std::rc::Rc;
use wyck_domain::chart::{Hsla, PriceMark, build, fitted_origin, fitted_size, full_time, hsla};

use gpui::prelude::*;
use gpui::{
    App, Bounds, ContentMask, Entity, FontStyle, FontWeight, Hitbox, HitboxBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathBuilder, PinchEvent, Pixels,
    ScrollWheelEvent, SharedString, TextAlign, TextRun, Window, canvas, point, px, size,
};

use wyck_domain::drawings::look::Face;

use super::Chart;
use super::path_pieces;
use wyck_domain::base::math::f32_of;
use wyck_domain::chart::{Cmd, DrawingView, Frame, P};

/// Most points in one path: a GPU path may only hold so many vertices (see [`super::path_pieces`]).
const PATH_BATCH: usize = 3_000;

impl Chart {
    /// Builds the frame for the canvas at `bounds`.
    fn scene(&self, cx: &App, bounds: Bounds<Pixels>, scale: f32, with_pointer: bool) -> Vec<Cmd> {
        let timeframe = self.timeframe.code();
        let horizon = self
            .drawings
            .as_ref()
            .and_then(|drawings| drawings.read(cx).book().horizon());
        let visible = move |d: &wyck_domain::drawings::model::Drawing| {
            d.shows_on(&timeframe)
                && horizon.is_none_or(|h| d.first_time().is_none_or(|time| time <= h))
        };
        let drawings =
            self.drawings
                .as_ref()
                .zip(self.symbol.as_ref())
                .and_then(|(drawings, symbol)| {
                    let book = drawings.read(cx).book();
                    let name = symbol.name.as_ref();
                    let (list, creating) = (book.drawings(name), book.creating(name));
                    let list: Vec<_> = list
                        .iter()
                        .filter_map(|d| self.resolved_position(d))
                        .collect();
                    let creating = creating.and_then(|d| self.resolved_position(d));
                    (!list.is_empty() || creating.is_some()).then_some((
                        list,
                        creating,
                        book.selected(),
                        book.also().to_vec(),
                    ))
                });
        let marks: Vec<PriceMark> = self
            .lines
            .iter()
            .filter(|line| self.replay.is_none() && self.settings.trading.shows(line.id))
            .map(|line| {
                let mut mark = line.mark();
                mark.price = self.line_price(line.id, mark.price);
                mark
            })
            .collect();
        build(&Frame {
            raw: &self.series,
            display: &self.display,
            settings: &self.settings,
            view: &self.view,
            timeframe: self.timeframe,
            digits: self.digits(),
            pip_position: self.symbol.as_ref().and_then(|symbol| symbol.pip_position),
            origin: (f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
            w: f64::from(f32::from(bounds.size.width)),
            h: f64::from(f32::from(bounds.size.height)),
            scale,
            hover: self.hover.filter(|_| with_pointer),
            remote: self
                .remote
                .filter(|_| with_pointer)
                .map(|r| (r.time_ms, r.price)),
            ask: self.ask,
            now_ms: self.clock_ms(),
            palette: super::palette::palette_for_chart(&self.settings.colors),
            drawings: drawings
                .as_ref()
                .map(|(list, creating, selected, also)| DrawingView {
                    list,
                    creating: creating.as_ref(),
                    selected: *selected,
                    also,
                    // What the pointer is over, or is dragging in this chart (the book is busy
                    // then and nothing is hovered).
                    hovered: self
                        .over_drawing
                        .map(|(id, _)| id)
                        .or(if self.drawing_drag { *selected } else { None }),
                    visible: &visible,
                }),
            marks: &marks,
            flow: Some(&self.flow),
            watermark: self.watermark(),
        })
    }
}

impl Chart {
    /// A PNG picture of the chart as it is on screen, at twice its size, with the symbol and
    /// timeframe written at the top left. Also returns a file name for it.
    pub(crate) fn picture(
        &self,
        cx: &App,
    ) -> Result<(Vec<u8>, String), super::raster::RasterError> {
        let (pixmap, _) = self.picture_pixels(cx)?;
        let png = pixmap
            .encode_png()
            .map_err(|error| super::raster::RasterError::Encode(error.to_string()))?;
        let symbol = self
            .symbol
            .as_ref()
            .map_or_else(|| "Chart".to_owned(), |s| s.name.to_string());
        let when = full_time(self.clock_ms(), self.settings.zone, false, false);
        let stamp: String = when.chars().filter(char::is_ascii_alphanumeric).collect();
        let name = format!(
            "wyck-{}-{}-{stamp}.png",
            symbol.replace(|c: char| !c.is_ascii_alphanumeric(), ""),
            self.timeframe.code()
        );
        Ok((png, name))
    }

    /// The pixels of [`Self::picture`], for a picture of several charts, with the background color
    /// of the chart. The scale is [`super::raster::PICTURE_SCALE`].
    pub(crate) fn picture_pixels(
        &self,
        cx: &App,
    ) -> Result<(tiny_skia::Pixmap, Hsla), super::raster::RasterError> {
        let (w, h) = self.size();
        let scale = super::raster::PICTURE_SCALE;
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(f32_of(w)), px(f32_of(h))));
        // No crosshair in a picture: it is drawn for the pointer, which is not in it.
        let cmds = self.scene(cx, bounds, scale, false);
        let palette = super::palette::palette_for_chart(&self.settings.colors);
        let symbol = self
            .symbol
            .as_ref()
            .map_or_else(|| "Chart".to_owned(), |s| s.name.to_string());
        let mut title = format!("{symbol}  {}", self.timeframe.name());
        if self.settings.kind != super::ChartKind::Candles {
            title.push_str("  ");
            title.push_str(self.settings.kind.label());
        }
        let when = full_time(self.clock_ms(), self.settings.zone, false, false);
        let captions = [
            super::raster::Caption {
                text: title,
                size: 15.0,
                color: hsla(palette.text_strong),
                bold: true,
            },
            super::raster::Caption {
                text: format!("{when}  {}", self.settings.zone.label(self.clock_ms())),
                size: 11.0,
                color: hsla(palette.text),
                bold: false,
            },
        ];
        let background = hsla(palette.bg);
        let pixmap = super::raster::render_pixmap(
            &cmds,
            f32_of(w),
            f32_of(h),
            super::raster::PICTURE_SCALE,
            background,
            &captions,
        )?;
        Ok((pixmap, background))
    }
}

/// Draws the commands of a frame.
pub(crate) fn execute(cmds: Vec<Cmd>, window: &mut Window, cx: &mut App) {
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
                let mut quad = gpui::fill(
                    Bounds::new(point(px(x), px(y)), size(px(w.max(0.0)), px(h.max(0.0)))),
                    super::palette::gpui_hsla(fill),
                );
                quad.corner_radii = px(radius).into();
                if let Some((width, color)) = border {
                    quad.border_widths = px(width).into();
                    quad.border_color = super::palette::gpui_hsla(color);
                }
                window.paint_quad(quad);
            }
            Cmd::Stroke {
                points,
                width,
                color,
                dash,
            } => stroke(window, &points, width, color, dash),
            Cmd::Fill { points, color } => {
                // Only what shows: a polygon far larger than the screen is cut to it, so it
                // cannot spoil the tessellation.
                let polygon: Vec<gpui::Point<Pixels>> =
                    path_pieces::clip_polygon(&points, visible(window, 2.0))
                        .into_iter()
                        .map(|(x, y)| point(px(x), px(y)))
                        .collect();
                if polygon.len() < 3 {
                    continue;
                }
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&polygon, true);
                match builder.build() {
                    Ok(path) => window.paint_path(path, super::palette::gpui_hsla(color)),
                    Err(error) => note_refused(&error, polygon.len()),
                }
            }
            Cmd::Clip { x, y, w, h, inner } => {
                let bounds = Bounds::new(point(px(x), px(y)), size(px(w.max(0.0)), px(h.max(0.0))));
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    execute(inner, window, cx);
                });
            }
            Cmd::Text {
                text,
                x,
                y,
                size: font_size,
                color,
                align,
                bold,
                face,
            } => {
                let line = shape(window, &text, font_size, color, bold, face);
                let x = align.left_of(x, f32::from(line.width));
                paint_line(&line, point(px(x), px(y)), font_size, window, cx);
            }
            Cmd::FittedText {
                text,
                x,
                y,
                w,
                h,
                size: preferred,
                color,
                background,
                align,
                valign,
                bold,
                face,
            } => {
                let Some(font_size) = fitted_size(&text, preferred, w, h, |font_size| {
                    f32::from(shape(window, &text, font_size, color, bold, face).width)
                }) else {
                    continue;
                };
                let line = shape(window, &text, font_size, color, bold, face);
                let text_w = f32::from(line.width);
                let (left, top) = fitted_origin((x, y, w, h), text_w, font_size, align, valign);
                if let Some(bg) = background {
                    let mut quad = gpui::fill(
                        Bounds::new(
                            point(px(left - 3.0), px(top - 1.0)),
                            size(px(text_w + 6.0), px(font_size * LINE_HEIGHT + 2.0)),
                        ),
                        super::palette::gpui_hsla(bg),
                    );
                    quad.corner_radii = px(3.0).into();
                    window.paint_quad(quad);
                }
                paint_line(&line, point(px(left), px(top)), font_size, window, cx);
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
                size: font_size,
                bold,
                face,
            } => {
                let line = shape(window, &text, font_size, fg, bold, face);
                let width = fixed_width.unwrap_or_else(|| f32::from(line.width) + pad * 2.0);
                let mut x = align.left_of(x, width);
                if let Some((left, right)) = within {
                    x = x.clamp(left, (right - width).max(left));
                }
                let mut quad: PaintQuad = gpui::fill(
                    Bounds::new(point(px(x), px(y)), size(px(width), px(height))),
                    super::palette::gpui_hsla(bg),
                );
                quad.corner_radii = px(3.0).into();
                window.paint_quad(quad);
                paint_line(
                    &line,
                    point(
                        px(x + pad),
                        px(y + (height - font_size * LINE_HEIGHT) / 2.0),
                    ),
                    font_size,
                    window,
                    cx,
                );
            }
        }
    }
}

/// The part of the window the next shapes show in, grown by `grow` so the width of a line at the
/// edge is not cut.
fn visible(window: &Window, grow: f32) -> path_pieces::Rect {
    let bounds = window.content_mask().bounds;
    let (left, top) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    path_pieces::Rect {
        left,
        top,
        right: left + f32::from(bounds.size.width),
        bottom: top + f32::from(bounds.size.height),
    }
    .grown(grow)
}

/// Says once that a shape was refused by the GPU path builder. It used to vanish without a word.
fn note_refused(error: &impl std::fmt::Display, points: usize) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SAID: AtomicBool = AtomicBool::new(false);
    if !SAID.swap(true, Ordering::Relaxed) {
        tracing::warn!(%error, points, "a shape of the chart was refused by the path builder and is not drawn");
    }
}

/// Strokes a polyline: the part of it that shows, in paths a GPU path can hold. A dashed line is
/// drawn dash by dash, so a line longer than the screen by a million pixels costs what the screen
/// shows of it (see [`super::path_pieces`]).
fn stroke(window: &mut Window, points: &[P], width: f32, color: Hsla, dash: Option<[f32; 2]>) {
    let rect = visible(window, width + 2.0);
    let pieces = match dash {
        Some([on, off]) => path_pieces::dashes(points, on, off, rect),
        None => path_pieces::solid(points, rect),
    };
    for group in path_pieces::batches(pieces, PATH_BATCH) {
        let mut builder = PathBuilder::stroke(px(width));
        let mut count = 0;
        for piece in &group {
            let Some((first, rest)) = piece.split_first() else {
                continue;
            };
            builder.move_to(point(px(first.0), px(first.1)));
            for (x, y) in rest {
                builder.line_to(point(px(*x), px(*y)));
            }
            count += piece.len();
        }
        match builder.build() {
            Ok(path) => window.paint_path(path, super::palette::gpui_hsla(color)),
            Err(error) => note_refused(&error, count),
        }
    }
}

/// The height of a line of text, as a multiple of its font size.
const LINE_HEIGHT: f32 = 1.3;

/// Paints a shaped line with its top left corner at `origin`. A line the text system cannot
/// paint is skipped and logged, so the rest of the frame is still drawn.
fn paint_line(
    line: &gpui::ShapedLine,
    origin: gpui::Point<Pixels>,
    font_size: f32,
    window: &mut Window,
    cx: &mut App,
) {
    if let Err(error) = line.paint(
        origin,
        px(font_size * LINE_HEIGHT),
        TextAlign::Left,
        None,
        window,
        cx,
    ) {
        tracing::warn!(%error, "could not paint a line of text on a chart");
    }
}

fn shape(
    window: &Window,
    text: &str,
    size_px: f32,
    color: Hsla,
    bold: bool,
    face: Face,
) -> gpui::ShapedLine {
    let style = window.text_style();
    let mut font = style.font();
    if bold {
        font.weight = FontWeight::SEMIBOLD;
    }
    if face.italic {
        font.style = FontStyle::Italic;
    }
    if let Some(family) = face.family {
        font.family = SharedString::from(family);
    }
    let run = TextRun {
        len: text.len(),
        font,
        color: super::palette::gpui_hsla(color),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        SharedString::from(text.to_owned()),
        px(size_px),
        &[run],
        None,
    )
}

/// The canvas: measures itself, draws a frame from the chart's data, and listens to the pointer.
pub(crate) fn surface(
    chart: &Entity<Chart>,
    bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> impl IntoElement {
    let entity = chart.clone();
    canvas(
        move |bounds, window, _cx| {
            bounds_cell.set(Some(bounds));
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        },
        move |bounds, hitbox: Hitbox, window, cx| {
            let scale = window.scale_factor();
            let cmds = entity.read(cx).scene(cx, bounds, scale, true);
            execute(cmds, window, cx);
            let cursor = entity.read(cx).cursor(cx);
            window.set_cursor_style(cursor, &hitbox);
            listen(&entity, bounds, hitbox, window);
        },
    )
    .absolute()
    .size_full()
}

/// Registers the pointer handlers for this frame. They are window wide, so a drag that leaves the
/// chart keeps working, and each one checks the hitbox so an overlay above the chart wins.
fn listen(entity: &Entity<Chart>, bounds: Bounds<Pixels>, hitbox: Hitbox, window: &mut Window) {
    let relative = move |position: gpui::Point<Pixels>| {
        (
            f32::from(position.x - bounds.origin.x),
            f32::from(position.y - bounds.origin.y),
        )
    };

    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble || !h.is_hovered(window) {
            return;
        }
        let (x, y) = relative(event.position);
        match event.button {
            MouseButton::Left => {
                let clicks = event.click_count;
                let shift = event.modifiers.shift;
                e.update(cx, |chart, cx| chart.on_mouse_down(x, y, clicks, shift, cx));
            }
            MouseButton::Right => e.update(cx, |chart, cx| chart.on_right_down(x, y, cx)),
            _ => {}
        }
    });

    // A right click while a drawing tool is active gives it up, back to the normal cursor, and
    // opens no menu. The menu listens on the way back up and would get the click first, so this
    // one takes it on the way down and stops it there. With no tool armed it lets the click
    // through, and the menu opens as usual.
    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Capture
            || event.button != MouseButton::Right
            || !h.is_hovered(window)
        {
            return;
        }
        if e.update(cx, Chart::abort_drawing) {
            cx.stop_propagation();
        }
    });

    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble {
            return;
        }
        let (x, y) = relative(event.position);
        let hovered = h.is_hovered(window);
        let left_down = event.pressed_button == Some(MouseButton::Left);
        let shift = event.modifiers.shift;
        e.update(cx, |chart, cx| {
            if chart.drag.is_some() || chart.drawing_drag || hovered {
                chart.on_mouse_move(x, y, left_down, shift, cx);
            } else {
                chart.on_pointer_left(cx);
            }
        });
    });

    let e = entity.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
        if phase == gpui::DispatchPhase::Bubble && event.button == MouseButton::Left {
            let (x, y) = relative(event.position);
            e.update(cx, |chart, cx| chart.on_mouse_up(x, y, cx));
        }
    });

    let (e, h) = (entity.clone(), hitbox.clone());
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble || !h.is_hovered(window) {
            return;
        }
        let (x, y) = relative(event.position);
        let delta = event.delta.pixel_delta(px(20.0));
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        let shift = event.modifiers.shift;
        e.update(cx, |chart, cx| chart.on_wheel(x, y, dx, dy, shift, cx));
    });

    let (e, h) = (entity.clone(), hitbox);
    window.on_mouse_event(move |event: &PinchEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble || !h.is_hovered(window) {
            return;
        }
        let (x, _) = relative(event.position);
        let factor = f64::from(1.0 + event.delta).max(0.1);
        e.update(cx, |chart, cx| chart.zoom_by(factor, x, cx));
    });
}
