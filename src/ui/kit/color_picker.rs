//! The color panel that opens from a swatch: a square for saturation and brightness, a bar for
//! the hue, the color as hex and as red, green and blue, the colors of the theme to click, and the
//! colors the user saved (kept with the preferences).
//!
//! Dragging in the square or on the bar, typing a value or clicking a preset all move the same
//! thing (the color), so the marker in the square and the thumb on the bar always show where the
//! color is, whichever way it was chosen. Colors are `0xRRGGBB`; the drawings and studies keep
//! their opacity apart from their color, so a color has no alpha. A swatch whose owner has an
//! opacity of its own (a drawing) can still show a bar for it, told apart from the color: see
//! [`panel_with_opacity`].
//!
//! The panel is an entity kept per swatch (by the swatch's id) so that a hue survives a drag
//! through white, gray or black, where a color no longer says which hue it came from.

pub mod math;
pub mod render;

use crate::ui::kit::shape::Shape as _;
use crate::ui::kit::theme;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, DragMoveEvent, ElementId, Entity, Global, MouseButton, MouseDownEvent,
    Pixels, Point, Subscription, Window, canvas, div, linear_color_stop, linear_gradient, px, rgb,
};
use gpui_kit::component::input::{InputEvent, InputState};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// The size of the square, and the height of the hue bar.
const AREA_W: f32 = 232.0;
const AREA_H: f32 = 152.0;
const HUE_H: f32 = 12.0;
/// The size of a preset and the gap between them: eight of them make the width of the square.
const CELL: f32 = 25.0;
const GAP: f32 = 4.0;
/// How many colors can be saved.
const MAX_SAVED: usize = 24;

/// What keeps the saved colors.
type SaveColors = Rc<dyn Fn(&[u32], &mut App)>;
type Change = Rc<dyn Fn(u32, &mut Window, &mut App)>;
type Close = Rc<dyn Fn(&mut Window, &mut App)>;
/// What to call with the opacity, from 0 to 1, when the bar for it is dragged.
pub type ChangeOpacity = Rc<dyn Fn(f32, &mut Window, &mut App)>;

/// An opacity a color panel shows a bar for.
pub struct Opacity {
    /// The opacity now, from 0 to 1.
    pub value: f32,
    /// What it is the opacity of, in a word: the panel writes it beside the bar.
    pub label: &'static str,
    pub change: ChangeOpacity,
}

/// What a drag in progress moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drag {
    Area,
    Hue,
    Alpha,
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
            return math::parse_hex(text);
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
            Self::Hex => math::hex_text(color),
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
    /// The opacity of what the swatch colors, when the owner has one to show a bar for.
    opacity: Option<f32>,
    opacity_label: &'static str,
    on_opacity: Option<ChangeOpacity>,
    alpha: Rc<Cell<Option<Bounds<Pixels>>>>,
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
            hsv: math::rgb_to_hsv(color),
            original: color,
            open: false,
            on_change: None,
            on_close: None,
            opacity: None,
            opacity_label: "",
            on_opacity: None,
            alpha: Rc::new(Cell::new(None)),
            swatch_hovered: false,
            area: Rc::new(Cell::new(None)),
            bar: Rc::new(Cell::new(None)),
            fields: None,
            synced: None,
            editing: None,
        }
    }

    fn rgb(&self) -> u32 {
        math::hsv_to_rgb(self.hsv.0, self.hsv.1, self.hsv.2)
    }

    /// Follows the color the owner holds, and remembers what to tell it.
    fn attach(
        &mut self,
        color: u32,
        open: bool,
        on_change: Change,
        on_close: Close,
        opacity: Option<Opacity>,
    ) {
        if open && !self.open {
            self.original = color;
        }
        self.open = open;
        if self.rgb() != color {
            let (hue, saturation, value) = math::rgb_to_hsv(color);
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
        (self.opacity, self.on_opacity) = match opacity {
            Some(Opacity {
                value,
                label,
                change,
            }) => {
                self.opacity_label = label;
                (Some(value.clamp(0.0, 1.0)), Some(change))
            }
            None => (None, None),
        };
    }

    fn emit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(on_change) = self.on_change.clone() {
            on_change(self.rgb(), window, cx);
        }
        cx.notify();
    }

    /// Sets the color, from a preset or a typed value: the marker and the thumb move to it.
    fn set_rgb(&mut self, color: u32, window: &mut Window, cx: &mut Context<Self>) {
        let (hue, saturation, value) = math::rgb_to_hsv(color);
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
            Drag::Alpha => &self.alpha,
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
            Drag::Alpha => {
                // Whole percents, so the number beside the bar is what is set.
                let value = (x * 100.0).round() / 100.0;
                self.opacity = Some(value);
                self.editing = None;
                if let Some(change) = self.on_opacity.clone() {
                    change(value, window, cx);
                }
                cx.notify();
                return;
            }
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
    panel_with_opacity(id, color, None, open, cx, on_change, on_close)
}

/// Like [`panel`], with a bar for the opacity of what the color is for, from 0 to 1, told the
/// value the owner holds now. Each drag on the bar calls `on_opacity`. The color itself stays
/// `0xRRGGBB`: the opacity travels apart from it.
pub fn panel_with_opacity(
    id: &ElementId,
    color: u32,
    opacity: Option<Opacity>,
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
        panel.attach(color, open, on_change, on_close, opacity)
    });
    panel
}

/// Tells the panel whether the pointer is on its swatch.
pub fn set_swatch_hovered(panel: &Entity<ColorPanel>, hovered: bool, cx: &mut App) {
    panel.update(cx, |panel, _| panel.swatch_hovered = hovered);
}

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
        .floats()
}

fn small_label(text: &'static str) -> gpui::Div {
    div()
        .text_size(px(crate::ui::kit::tokens::text::small()))
        .text_color(theme::muted_fg())
        .child(text)
}

impl ColorPanel {
    /// The bar for the opacity of what is colored, with the percent beside it, or nothing when the
    /// owner has no opacity to show. It runs from clear to the color, over a checker-free gray so
    /// the fade reads on any theme.
    #[inline(never)]
    fn opacity_row(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let value = self.opacity?;
        let color = self.rgb();
        let (r, g, b) = (
            ((color >> 16) & 0xff) as f32 / 255.0,
            ((color >> 8) & 0xff) as f32 / 255.0,
            (color & 0xff) as f32 / 255.0,
        );
        // The bar as last drawn, for the thumb; a guess until it has been drawn once.
        let width = self
            .alpha
            .get()
            .map_or(120.0, |bounds| f32::from(bounds.size.width));
        let clear = gpui::Rgba { r, g, b, a: 0.0 };
        let solid = gpui::Rgba { r, g, b, a: 1.0 };
        let track = div()
            .relative()
            .flex_1()
            .h(px(HUE_H))
            .child(
                div()
                    .size_full()
                    .rounded_full()
                    .overflow_hidden()
                    .bg(theme::surface_hover())
                    .child(div().size_full().bg(linear_gradient(
                        90.,
                        linear_color_stop(clear, 0.),
                        linear_color_stop(solid, 1.),
                    ))),
            )
            .child(
                marker(16.0, color)
                    .left(px(value * width - 8.0))
                    .top(px(-2.0)),
            )
            .child(surface("color-alpha", Drag::Alpha, self.alpha.clone(), cx).cursor_pointer());
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .w(px(AREA_W))
                .child(small_label(self.opacity_label))
                .child(track)
                .child(
                    div()
                        .w(px(34.))
                        .flex()
                        .justify_end()
                        .text_size(px(crate::ui::kit::tokens::text::small()))
                        .text_color(theme::fg())
                        .child(format!("{}%", (value * 100.0).round() as u32)),
                ),
        )
    }
}
