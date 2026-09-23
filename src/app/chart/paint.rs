//! The canvas of a chart: it measures itself, draws the frame the scene builds, and listens to
//! the pointer.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, ContentMask, Entity, FontWeight, Hitbox, HitboxBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, PathBuilder, PinchEvent, Pixels,
    ScrollWheelEvent, SharedString, TextAlign, TextRun, Window, canvas, point, px, size,
};

use super::Chart;
use super::scene::{self, Align, Cmd, DrawingView, Frame, LINE, P, Palette};

/// Most points in one stroked path: a GPU path may only hold so many vertices.
const PATH_CHUNK: usize = 1_500;

impl Chart {
    /// Builds the frame for the canvas at `bounds`.
    fn scene(&self, cx: &App, bounds: Bounds<Pixels>, scale: f32, with_pointer: bool) -> Vec<Cmd> {
        let timeframe = self.timeframe.code();
        let visible = move |d: &super::drawing::model::Drawing| d.shows_on(&timeframe);
        let drawings =
            self.drawings
                .as_ref()
                .zip(self.symbol.as_ref())
                .and_then(|(drawings, symbol)| {
                    let book = drawings.read(cx).book();
                    let name = symbol.name.as_ref();
                    let (list, creating) = (book.drawings(name), book.creating(name));
                    (!list.is_empty() || creating.is_some()).then_some((
                        list,
                        creating,
                        book.selected(),
                    ))
                });
        let marks: Vec<scene::PriceMark> = self
            .lines
            .iter()
            .map(|line| {
                let mut mark = line.mark();
                mark.price = self.line_price(line.id, mark.price);
                mark
            })
            .collect();
        scene::build(&Frame {
            raw: &self.series,
            display: &self.display,
            settings: &self.settings,
            view: &self.view,
            timeframe: self.timeframe,
            digits: self.digits(),
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
            now_ms: super::now_ms(),
            palette: Palette::new(),
            drawings: drawings.map(|(list, creating, selected)| DrawingView {
                list,
                creating,
                selected,
                visible: &visible,
            }),
            marks: &marks,
        })
    }
}

impl Chart {
    /// A PNG picture of the chart as it is on screen, at twice its size, with the symbol and
    /// timeframe written at the top left. Also returns a file name for it.
    pub fn picture(&self, cx: &App) -> Result<(Vec<u8>, String), String> {
        let (w, h) = self.size();
        let scale = 2.0;
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(w as f32), px(h as f32)));
        // No crosshair in a picture: it is drawn for the pointer, which is not in it.
        let cmds = self.scene(cx, bounds, scale, false);
        let symbol = self
            .symbol
            .as_ref()
            .map_or_else(|| "Chart".to_owned(), |s| s.name.to_string());
        let mut title = format!("{symbol}  {}", self.timeframe.name());
        if self.settings.kind != super::ChartKind::Candles {
            title.push_str(&format!("  {}", self.settings.kind.label()));
        }
        let when = super::axis::full_time(super::now_ms(), self.settings.zone, false, false);
        let captions = [
            super::raster::Caption {
                text: title,
                size: 15.0,
                color: crate::app::theme::fg().into(),
                bold: true,
            },
            super::raster::Caption {
                text: format!("{when}  {}", self.settings.zone.label(super::now_ms())),
                size: 11.0,
                color: crate::app::theme::muted_fg().into(),
                bold: false,
            },
        ];
        let png = super::raster::render_png(
            &cmds,
            w as f32,
            h as f32,
            scale,
            crate::app::theme::bg().into(),
            &captions,
        )?;
        let stamp: String = when.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let name = format!(
            "wyck-{}-{}-{stamp}.png",
            symbol.replace(|c: char| !c.is_ascii_alphanumeric(), ""),
            self.timeframe.code()
        );
        Ok((png, name))
    }
}

/// Draws the commands of a frame.
pub fn execute(cmds: Vec<Cmd>, window: &mut Window, cx: &mut App) {
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
                    fill,
                );
                quad.corner_radii = px(radius).into();
                if let Some((width, color)) = border {
                    quad.border_widths = px(width).into();
                    quad.border_color = color;
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
                if points.len() < 3 {
                    continue;
                }
                let polygon: Vec<gpui::Point<Pixels>> =
                    points.iter().map(|(x, y)| point(px(*x), px(*y))).collect();
                let mut builder = PathBuilder::fill();
                builder.add_polygon(&polygon, true);
                if let Ok(path) = builder.build() {
                    window.paint_path(path, color);
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
            } => {
                let line = shape(window, &text, font_size, color, bold);
                let x = aligned(x, f32::from(line.width), align);
                let _ = line.paint(
                    point(px(x), px(y)),
                    px(font_size * 1.3),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
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
            } => {
                let line = shape(window, &text, scene::FONT, fg, false);
                let width = fixed_width.unwrap_or_else(|| f32::from(line.width) + pad * 2.0);
                let mut x = aligned(x, width, align);
                if let Some((left, right)) = within {
                    x = x.clamp(left, (right - width).max(left));
                }
                let mut quad: PaintQuad = gpui::fill(
                    Bounds::new(point(px(x), px(y)), size(px(width), px(height))),
                    bg,
                );
                quad.corner_radii = px(3.0).into();
                window.paint_quad(quad);
                let _ = line.paint(
                    point(px(x + pad), px(y + (height - LINE) / 2.0)),
                    px(LINE),
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }
    }
}

/// Strokes a polyline in pieces a GPU path can hold, each sharing its end point with the next so
/// the line has no break.
fn stroke(
    window: &mut Window,
    points: &[P],
    width: f32,
    color: gpui::Hsla,
    dash: Option<[f32; 2]>,
) {
    let mut start = 0;
    while start + 1 < points.len() {
        let end = (start + PATH_CHUNK).min(points.len());
        let mut builder = PathBuilder::stroke(px(width));
        if let Some([on, off]) = dash {
            builder = builder.dash_array(&[px(on), px(off)]);
        }
        builder.move_to(point(px(points[start].0), px(points[start].1)));
        for (x, y) in &points[start + 1..end] {
            builder.line_to(point(px(*x), px(*y)));
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
        start = end - 1;
    }
}

fn aligned(x: f32, width: f32, align: Align) -> f32 {
    match align {
        Align::Left => x,
        Align::Center => x - width / 2.0,
        Align::Right => x - width,
    }
}

fn shape(
    window: &Window,
    text: &str,
    size_px: f32,
    color: gpui::Hsla,
    bold: bool,
) -> gpui::ShapedLine {
    let style = window.text_style();
    let mut font = style.font();
    if bold {
        font.weight = FontWeight::SEMIBOLD;
    }
    let run = TextRun {
        len: text.len(),
        font,
        color,
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
pub fn surface(
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
                e.update(cx, |chart, cx| chart.on_mouse_down(x, y, clicks, cx));
            }
            MouseButton::Right => e.update(cx, |chart, cx| chart.on_right_down(x, y, cx)),
            _ => {}
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
