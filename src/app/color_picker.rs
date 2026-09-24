//! The color panel that opens from a swatch: a square for saturation and brightness, a bar for
//! the hue, the color as hex and as red, green and blue, the colors of the theme to click, and the
//! colors the user saved (kept with the preferences).
//!
//! Dragging in the square or on the bar, typing a value or clicking a preset all move the same
//! thing (the color), so the marker in the square and the thumb on the bar always show where the
//! color is, whichever way it was chosen. Colors are `0xRRGGBB`; the drawings and studies keep
//! their opacity apart from their color, so there is no alpha here.
//!
//! The panel is an entity kept per swatch (by the swatch's id) so that a hue survives a drag
//! through white, gray or black, where a color no longer says which hue it came from.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, DragMoveEvent, ElementId, Entity, Global, MouseButton, MouseDownEvent,
    Pixels, Point, SharedString, Subscription, Window, canvas, div, hsla, linear_color_stop,
    linear_gradient, px, rgb,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable, Sizable};

use super::theme;

/// The colors of the theme offered to click: a row of grays, then eight hues in four shades.
pub const PRESETS: [u32; 40] = [
    0xffffff, 0xd1d4dc, 0x9598a1, 0x787b86, 0x5d606b, 0x434651, 0x2a2e39, 0x000000, 0xf23645,
    0xff9800, 0xffeb3b, 0x4caf50, 0x089981, 0x00bcd4, 0x2962ff, 0x9c27b0, 0xfccbcd, 0xffe0b2,
    0xfff9c4, 0xc8e6c9, 0xace5dc, 0xb2ebf2, 0xbbd9fb, 0xe1bee7, 0xf7525f, 0xffb74d, 0xfff176,
    0x81c784, 0x22ab94, 0x4dd0e1, 0x5b9cf6, 0xba68c8, 0xb22833, 0xf57c00, 0xfbc02d, 0x388e3c,
    0x056656, 0x0097a7, 0x1848cc, 0x7b1fa2,
];

/// The size of the square, and the height of the hue bar.
const AREA_W: f32 = 232.0;
const AREA_H: f32 = 152.0;
const HUE_H: f32 = 12.0;
/// The size of a preset and the gap between them: eight of them make the width of the square.
const CELL: f32 = 25.0;
const GAP: f32 = 4.0;
/// How many colors can be saved.
const MAX_SAVED: usize = 24;

// ---- colors ----

/// The hue (0 to 360), saturation and value (0 to 1) of a color.
pub fn rgb_to_hsv(color: u32) -> (f32, f32, f32) {
    let r = ((color >> 16) & 0xff) as f32 / 255.0;
    let g = ((color >> 8) & 0xff) as f32 / 255.0;
    let b = (color & 0xff) as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta < 1e-6 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max < 1e-6 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// The color of a hue (0 to 360), saturation and value (0 to 1).
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> u32 {
    let h = hue.rem_euclid(360.0) / 60.0;
    let s = saturation.clamp(0.0, 1.0);
    let v = value.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |channel: f32| ((channel + m) * 255.0).round().clamp(0.0, 255.0) as u32;
    (byte(r) << 16) | (byte(g) << 8) | byte(b)
}

/// `#rrggbb` as text.
pub fn hex_text(color: u32) -> String {
    format!("{:06x}", color & 0xff_ffff)
}

/// A color written as `rgb`, `#rgb`, `rrggbb` or `#rrggbb`.
pub fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.trim().trim_start_matches('#');
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match digits.len() {
        3 => {
            let mut color = 0;
            for c in digits.chars() {
                let d = c.to_digit(16)?;
                color = (color << 8) | (d << 4) | d;
            }
            Some(color)
        }
        6 => u32::from_str_radix(digits, 16).ok(),
        _ => None,
    }
}

// ---- the panel ----

/// What keeps the saved colors.
type SaveColors = Rc<dyn Fn(&[u32], &mut App)>;
type Change = Rc<dyn Fn(u32, &mut Window, &mut App)>;
type Close = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a drag in progress moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
    Area,
    Hue,
}

/// The typed fields of the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Hex,
    Red,
    Green,
    Blue,
}

impl Field {
    /// The color the text makes, given the color as it is, or `None` while it is not a value yet.
    fn parse(self, text: &str, now: u32) -> Option<u32> {
        if self == Self::Hex {
            return parse_hex(text);
        }
        let value: u32 = text.trim().parse().ok().filter(|v| *v <= 255)?;
        let shift = match self {
            Self::Red => 16,
            Self::Green => 8,
            _ => 0,
        };
        Some((now & !(0xff << shift)) | (value << shift))
    }

    /// What the field shows for a color.
    fn text(self, color: u32) -> String {
        match self {
            Self::Hex => hex_text(color),
            Self::Red => ((color >> 16) & 0xff).to_string(),
            Self::Green => ((color >> 8) & 0xff).to_string(),
            Self::Blue => (color & 0xff).to_string(),
        }
    }
}

struct Fields {
    hex: Entity<InputState>,
    red: Entity<InputState>,
    green: Entity<InputState>,
    blue: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl Fields {
    fn get(&self, field: Field) -> &Entity<InputState> {
        match field {
            Field::Hex => &self.hex,
            Field::Red => &self.red,
            Field::Green => &self.green,
            Field::Blue => &self.blue,
        }
    }
}

/// The colors the user saved, oldest first, for every panel.
#[derive(Default)]
struct Saved(Vec<u32>);

impl Global for Saved {}

/// What to call with the saved colors when they change, to keep them.
struct Persist(SaveColors);

impl Global for Persist {}

/// Sets the colors saved so far, and what keeps them when they change.
pub fn connect(saved: Vec<u32>, persist: impl Fn(&[u32], &mut App) + 'static, cx: &mut App) {
    let mut colors: Vec<u32> = Vec::new();
    for color in saved.into_iter().map(|c| c & 0xff_ffff) {
        if !colors.contains(&color) && colors.len() < MAX_SAVED {
            colors.push(color);
        }
    }
    cx.set_global(Saved(colors));
    cx.set_global(Persist(Rc::new(persist)));
}

/// The panels, by the id of the swatch that opens them.
#[derive(Default)]
struct Panels(HashMap<ElementId, Entity<ColorPanel>>);

impl Global for Panels {}

pub struct ColorPanel {
    /// Hue, saturation and value of the color. Kept rather than worked out from the color, so a
    /// hue is not lost when the color goes gray or black.
    hsv: (f32, f32, f32),
    /// The color when the panel opened, shown beside the new one.
    original: u32,
    open: bool,
    on_change: Option<Change>,
    on_close: Option<Close>,
    /// Whether the pointer is on the swatch, whose click closes the panel by itself.
    swatch_hovered: bool,
    area: Rc<Cell<Option<Bounds<Pixels>>>>,
    bar: Rc<Cell<Option<Bounds<Pixels>>>>,
    fields: Option<Fields>,
    /// The color the fields were last written for.
    synced: Option<u32>,
    /// The field being typed in, which is not written over while the user types.
    editing: Option<Field>,
}

impl ColorPanel {
    fn new(color: u32) -> Self {
        Self {
            hsv: rgb_to_hsv(color),
            original: color,
            open: false,
            on_change: None,
            on_close: None,
            swatch_hovered: false,
            area: Rc::new(Cell::new(None)),
            bar: Rc::new(Cell::new(None)),
            fields: None,
            synced: None,
            editing: None,
        }
    }

    fn rgb(&self) -> u32 {
        hsv_to_rgb(self.hsv.0, self.hsv.1, self.hsv.2)
    }

    /// Follows the color the owner holds, and remembers what to tell it.
    fn attach(&mut self, color: u32, open: bool, on_change: Change, on_close: Close) {
        if open && !self.open {
            self.original = color;
        }
        self.open = open;
        if self.rgb() != color {
            let (hue, saturation, value) = rgb_to_hsv(color);
            // A gray or a black has no hue of its own: keep the one there was.
            let hue = if saturation < 1e-4 || value < 1e-4 {
                self.hsv.0
            } else {
                hue
            };
            self.hsv = (hue, saturation, value);
        }
        self.on_change = Some(on_change);
        self.on_close = Some(on_close);
    }

    fn emit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(on_change) = self.on_change.clone() {
            on_change(self.rgb(), window, cx);
        }
        cx.notify();
    }

    /// Sets the color, from a preset or a typed value: the marker and the thumb move to it.
    fn set_rgb(&mut self, color: u32, window: &mut Window, cx: &mut Context<Self>) {
        let (hue, saturation, value) = rgb_to_hsv(color);
        let hue = if saturation < 1e-4 || value < 1e-4 {
            self.hsv.0
        } else {
            hue
        };
        self.hsv = (hue, saturation, value);
        self.emit(window, cx);
    }

    /// Keeps the color with the saved ones, if it is not there yet and there is room.
    fn save_current(&self, cx: &mut Context<Self>) {
        let color = self.rgb();
        let saved = cx.default_global::<Saved>();
        if saved.0.contains(&color) || saved.0.len() >= MAX_SAVED {
            return;
        }
        saved.0.push(color);
        Self::persist(cx);
        cx.notify();
    }

    fn forget(&self, color: u32, cx: &mut Context<Self>) {
        cx.default_global::<Saved>().0.retain(|c| *c != color);
        Self::persist(cx);
        cx.notify();
    }

    fn persist(cx: &mut Context<Self>) {
        let colors = cx.default_global::<Saved>().0.clone();
        if let Some(Persist(persist)) = cx.try_global::<Persist>().map(|p| Persist(p.0.clone())) {
            persist(&colors, cx);
        }
    }

    fn pick_preset(&mut self, color: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = None;
        self.set_rgb(color, window, cx);
    }

    /// The pointer is at `position` while dragging `drag`.
    fn drag_to(
        &mut self,
        drag: Drag,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cell = match drag {
            Drag::Area => &self.area,
            Drag::Hue => &self.bar,
        };
        let Some(bounds) = cell.get() else {
            return;
        };
        let across = |offset: Pixels, size: Pixels| {
            (f32::from(offset) / f32::from(size).max(1.0)).clamp(0.0, 1.0)
        };
        let x = across(position.x - bounds.origin.x, bounds.size.width);
        let y = across(position.y - bounds.origin.y, bounds.size.height);
        match drag {
            Drag::Area => {
                self.hsv.1 = x;
                self.hsv.2 = 1.0 - y;
            }
            Drag::Hue => self.hsv.0 = (x * 360.0).min(359.99),
        }
        self.editing = None;
        self.emit(window, cx);
    }

    fn make_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Fields {
        let color = self.rgb();
        let make = |field: Field, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).default_value(field.text(color)))
        };
        let hex = make(Field::Hex, window, cx);
        let red = make(Field::Red, window, cx);
        let green = make(Field::Green, window, cx);
        let blue = make(Field::Blue, window, cx);
        let mut subscriptions = Vec::new();
        for (field, state) in [
            (Field::Hex, &hex),
            (Field::Red, &red),
            (Field::Green, &green),
            (Field::Blue, &blue),
        ] {
            subscriptions.push(cx.subscribe_in(
                state,
                window,
                move |this, state, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let text = state.read(cx).value().to_string();
                        let Some(color) = field.parse(&text, this.rgb()) else {
                            return;
                        };
                        // Writing the field from the color says the same again: not a change.
                        if color != this.rgb() {
                            this.editing = Some(field);
                            this.set_rgb(color, window, cx);
                        }
                    }
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.editing = None;
                        this.synced = None;
                        cx.notify();
                    }
                    InputEvent::Focus => {}
                },
            ));
        }
        Fields {
            hex,
            red,
            green,
            blue,
            _subscriptions: subscriptions,
        }
    }

    /// Writes the color in the fields, except the one being typed in.
    fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let color = self.rgb();
        let Some(fields) = self.fields.as_ref() else {
            return;
        };
        for field in [Field::Hex, Field::Red, Field::Green, Field::Blue] {
            if self.editing == Some(field) {
                continue;
            }
            let text = field.text(color);
            fields.get(field).update(cx, |state, cx| {
                if state.value().as_ref() != text {
                    state.set_value(text, window, cx);
                }
            });
        }
    }
}

/// The panel of the swatch `id`, told the color the owner holds now.
pub fn panel(
    id: &ElementId,
    color: u32,
    open: bool,
    cx: &mut App,
    on_change: Change,
    on_close: Close,
) -> Entity<ColorPanel> {
    let known = cx.default_global::<Panels>().0.get(id).cloned();
    let panel = known.unwrap_or_else(|| {
        let panel = cx.new(|_| ColorPanel::new(color));
        cx.global_mut::<Panels>()
            .0
            .insert(id.clone(), panel.clone());
        panel
    });
    panel.update(cx, |panel, _| {
        panel.attach(color, open, on_change, on_close)
    });
    panel
}

/// Tells the panel whether the pointer is on its swatch.
pub fn set_swatch_hovered(panel: &Entity<ColorPanel>, hovered: bool, cx: &mut App) {
    panel.update(cx, |panel, _| panel.swatch_hovered = hovered);
}

// ---- drawing it ----

/// What is being dragged from a surface, carried by the drag itself.
#[derive(Clone)]
struct DragMarker(Drag);

impl Render for DragMarker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// A surface over the area or the bar that turns the pointer into a color: a press picks the color
/// under it, and dragging goes on picking, even when the pointer leaves the surface.
fn surface(
    id: &'static str,
    drag: Drag,
    cell: Rc<Cell<Option<Bounds<Pixels>>>>,
    cx: &mut Context<ColorPanel>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        // Anchored to the corner: an absolute box with no offset stays where it would have been
        // in the flow, which under the hue bar's own strip is a full bar height too low.
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            canvas(
                move |bounds, _window, _cx| cell.set(Some(bounds)),
                |_bounds, _state, _window, _cx| {},
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.drag_to(drag, event.position, window, cx);
            }),
        )
        .on_drag(DragMarker(drag), |marker, _offset, _window, cx| {
            cx.stop_propagation();
            cx.new(|_| marker.clone())
        })
        .on_drag_move(
            cx.listener(move |this, event: &DragMoveEvent<DragMarker>, window, cx| {
                if event.drag(cx).0 == drag {
                    this.drag_to(drag, event.event.position, window, cx);
                }
            }),
        )
}

/// A marker on a circle of `size`, filled with `fill`.
fn marker(size: f32, fill: u32) -> gpui::Div {
    div()
        .absolute()
        .size(px(size))
        .rounded_full()
        .bg(rgb(fill))
        .border_2()
        .border_color(rgb(0xffffff))
        .shadow_md()
}

fn small_label(text: &'static str) -> gpui::Div {
    div()
        .text_size(px(10.5))
        .text_color(theme::muted_fg())
        .child(text)
}

impl Render for ColorPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.fields.is_none() {
            self.fields = Some(self.make_fields(window, cx));
        }
        let color = self.rgb();
        if self.synced != Some(color) {
            self.sync_fields(window, cx);
            self.synced = Some(color);
        }
        let (hue, saturation, value) = self.hsv;
        let this = cx.entity();
        let pure = hsv_to_rgb(hue, 1.0, 1.0);
        let clear_white = hsla(0.0, 0.0, 1.0, 0.0);
        let clear_black = hsla(0.0, 0.0, 0.0, 0.0);

        // The square: the hue, whitened to the left and darkened downwards.
        let area = div()
            .relative()
            .w(px(AREA_W))
            .h(px(AREA_H))
            .rounded_md()
            .overflow_hidden()
            .bg(rgb(pure))
            .child(div().absolute().size_full().bg(linear_gradient(
                90.,
                linear_color_stop(hsla(0.0, 0.0, 1.0, 1.0), 0.),
                linear_color_stop(clear_white, 1.),
            )))
            .child(div().absolute().size_full().bg(linear_gradient(
                180.,
                linear_color_stop(clear_black, 0.),
                linear_color_stop(hsla(0.0, 0.0, 0.0, 1.0), 1.),
            )))
            .child(
                marker(14.0, color)
                    .left(px(saturation * AREA_W - 7.0))
                    .top(px((1.0 - value) * AREA_H - 7.0)),
            )
            .child(surface("color-area", Drag::Area, self.area.clone(), cx).cursor_crosshair());

        // The bar: every hue, in six steps of two colors each.
        let strip = (0..6).map(|i| {
            let (from, to) = (i as f32 * 60.0, (i + 1) as f32 * 60.0);
            div().flex_1().h_full().bg(linear_gradient(
                90.,
                linear_color_stop(rgb(hsv_to_rgb(from, 1.0, 1.0)), 0.),
                linear_color_stop(rgb(hsv_to_rgb(to, 1.0, 1.0)), 1.),
            ))
        });
        let bar = div()
            .relative()
            .w(px(AREA_W))
            .h(px(HUE_H))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_row()
                    .rounded_full()
                    .overflow_hidden()
                    .children(strip),
            )
            .child(
                marker(16.0, pure)
                    .left(px(hue / 360.0 * AREA_W - 8.0))
                    .top(px(-2.0)),
            )
            .child(surface("color-hue", Drag::Hue, self.bar.clone(), cx).cursor_pointer());

        let fields = self.fields.as_ref();
        let preview = div()
            .flex_none()
            .flex()
            .flex_row()
            .w(px(44.))
            .h(px(28.))
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(theme::border_subtle())
            .child(div().flex_1().h_full().bg(rgb(self.original)))
            .child(div().flex_1().h_full().bg(rgb(color)));
        let typed = fields.map(|fields| {
            let channel = |label: &'static str, state: &Entity<InputState>| {
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(small_label(label))
                    .child(div().w(px(56.)).child(Input::new(state).small()))
            };
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(preview)
                        .child(small_label("HEX"))
                        .child(div().flex_1().child(Input::new(&fields.hex).small())),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .child(channel("R", &fields.red))
                        .child(channel("G", &fields.green))
                        .child(channel("B", &fields.blue)),
                )
        });

        let swatch = |id: String, shade: u32, panel: &Entity<Self>| {
            let panel = panel.clone();
            div()
                .id(SharedString::from(id))
                .size(px(CELL))
                .rounded_sm()
                .bg(rgb(shade))
                .border_2()
                .border_color(if shade == color {
                    theme::fg()
                } else {
                    theme::border_subtle()
                })
                .cursor_pointer()
                .hover(|style| style.border_color(theme::border_strong()))
                .on_click(move |_, window, cx| {
                    panel.update(cx, |panel, cx| panel.pick_preset(shade, window, cx));
                })
        };
        let presets = div().flex().flex_row().flex_wrap().gap(px(GAP)).children(
            PRESETS
                .iter()
                .enumerate()
                .map(|(i, shade)| swatch(format!("color-preset-{i}"), *shade, &this)),
        );
        let saved: Vec<u32> = cx.default_global::<Saved>().0.clone();
        let can_save = !saved.contains(&color) && saved.len() < MAX_SAVED;
        let saved_row = div().flex().flex_row().flex_wrap().gap(px(GAP)).children(
            saved.iter().enumerate().map(|(i, shade)| {
                let forget = this.clone();
                let shade = *shade;
                swatch(format!("color-saved-{i}"), shade, &this).on_mouse_down(
                    MouseButton::Right,
                    move |_, _window, cx| {
                        forget.update(cx, |panel, cx| panel.forget(shade, cx));
                    },
                )
            }),
        );
        let save_this = this.clone();

        div()
            .w(px(AREA_W + 24.0))
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_lg()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _event, window, cx| {
                // A click on the swatch itself closes the panel through the swatch.
                if !this.swatch_hovered
                    && let Some(close) = this.on_close.clone()
                {
                    close(window, cx);
                }
            }))
            .child(area)
            .child(bar)
            .children(typed)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(small_label("PRESETS"))
                    .child(presets),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .child(small_label("SAVED"))
                            .child(
                                Button::new("color-save")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Plus)
                                    .label("Save this color")
                                    .disabled(!can_save)
                                    .cursor_pointer()
                                    .when(!can_save, |button| button.cursor_not_allowed())
                                    .on_click(move |_, _window, cx| {
                                        save_this.update(cx, |panel, cx| panel.save_current(cx));
                                    }),
                            ),
                    )
                    .child(if saved.is_empty() {
                        div()
                            .h(px(CELL))
                            .flex()
                            .items_center()
                            .text_size(px(11.5))
                            .text_color(theme::muted_fg())
                            .child("Colors you save show here")
                    } else {
                        saved_row
                    })
                    .children(
                        (!saved.is_empty())
                            .then(|| small_label("Right click a saved color to remove it")),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_goes_to_hsv_and_back() {
        for color in [
            0x000000, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0x4f8dff, 0x123456,
        ] {
            let (h, s, v) = rgb_to_hsv(color);
            assert_eq!(hsv_to_rgb(h, s, v), color, "{color:06x}");
        }
        for color in PRESETS {
            let (h, s, v) = rgb_to_hsv(color);
            assert_eq!(hsv_to_rgb(h, s, v), color, "{color:06x}");
        }
    }

    #[test]
    fn primary_colors_have_their_hues() {
        assert_eq!(rgb_to_hsv(0xff0000).0, 0.0);
        assert_eq!(rgb_to_hsv(0x00ff00).0, 120.0);
        assert_eq!(rgb_to_hsv(0x0000ff).0, 240.0);
        assert_eq!(rgb_to_hsv(0x808080).1, 0.0, "a gray has no saturation");
        assert_eq!(hsv_to_rgb(60.0, 1.0, 1.0), 0xffff00);
    }

    #[test]
    fn hex_is_read_in_its_short_and_long_forms() {
        assert_eq!(parse_hex("#ff8800"), Some(0xff8800));
        assert_eq!(parse_hex("FF8800"), Some(0xff8800));
        assert_eq!(parse_hex("f80"), Some(0xff8800));
        assert_eq!(parse_hex(" #abc "), Some(0xaabbcc));
        assert_eq!(parse_hex("ff88"), None);
        assert_eq!(parse_hex("gg0000"), None);
        assert_eq!(parse_hex(""), None);
        assert_eq!(hex_text(0x00ff88), "00ff88");
    }

    #[test]
    fn a_channel_replaces_only_its_own_part() {
        assert_eq!(Field::Red.parse("18", 0x112233), Some(0x122233));
        assert_eq!(Field::Green.parse("255", 0x112233), Some(0x11ff33));
        assert_eq!(Field::Blue.parse("0", 0x112233), Some(0x112200));
        assert_eq!(Field::Blue.parse("256", 0x112233), None);
        assert_eq!(Field::Red.parse("", 0x112233), None);
        assert_eq!(Field::Hex.parse("#000", 0x112233), Some(0));
        assert_eq!(Field::Green.text(0x112233), "34");
    }

    #[test]
    fn the_presets_have_no_repeats() {
        let mut seen = std::collections::HashSet::new();
        assert!(PRESETS.iter().all(|c| seen.insert(*c)));
    }
}
