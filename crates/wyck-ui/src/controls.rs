//! Controls that pick a value: segmented choices, switches, color swatches and the pickers for
//! line width, line style and height.
//!
//! They are plain functions returning elements, so a view owns its state (which choice, which
//! swatch popover is open) and passes it in; the callbacks say what the user picked.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, App, Div, ElementId, Rgba, SharedString, Window, div, px};
use gpui_kit::component::Sizable;
use gpui_kit::component::switch::Switch;

use crate::{color_picker, menu, theme, tokens};

/// The id of child `n` of an element.
pub fn child_id(id: &ElementId, n: usize) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(id.clone()), n.to_string().into())
}

/// Buttons side by side, one of them chosen, each as wide as its label.
pub fn segmented(
    id: impl Into<ElementId>,
    options: &[&str],
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    strip(id.into(), options, selected, false, Rc::new(on_select))
}

/// [`segmented`] with the buttons sharing the whole width, for a choice that heads a panel (buy or
/// sell, market or pending).
pub fn segmented_fill(
    id: impl Into<ElementId>,
    options: &[&str],
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    strip(id.into(), options, selected, true, Rc::new(on_select))
}

type OnSelect = Rc<dyn Fn(usize, &mut Window, &mut App)>;

fn strip(id: ElementId, options: &[&str], selected: usize, fill: bool, on_select: OnSelect) -> Div {
    let mut strip = div()
        .flex()
        .flex_row()
        .items_center()
        .p_0p5()
        .gap_0p5()
        .rounded_md()
        .bg(theme::bg())
        .border_1()
        .border_color(theme::border_subtle());
    for (index, label) in options.iter().enumerate() {
        let chosen = index == selected;
        let on_select = on_select.clone();
        strip = strip.child(
            div()
                .id(child_id(&id, index))
                .when(fill, |el| el.flex_1().justify_center())
                .h(px(tokens::height::COMPACT))
                .px_2p5()
                .flex()
                .items_center()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(tokens::text::BODY))
                .text_color(ink(chosen))
                .when(chosen, |el| el.bg(theme::accent_selected()))
                .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
                .on_click(move |_, window, cx| on_select(index, window, cx))
                .child(SharedString::from((*label).to_owned())),
        );
    }
    strip
}

/// A chip: a small toggle with an edge, lit when `chosen`. The caller adds what it shows (a label,
/// an icon) and the click.
pub fn chip(id: impl Into<ElementId>, chosen: bool) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .h(px(tokens::height::COMPACT))
        .px_2p5()
        .rounded_md()
        .border_1()
        .border_color(if chosen {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .when(chosen, |el| el.bg(theme::accent_selected()))
        .cursor_pointer()
        .text_size(px(tokens::text::BODY))
        .text_color(ink(chosen))
        .hover(|s| s.bg(theme::surface_hover()).text_color(theme::fg()))
}

/// [`chip`]s that wrap, any number of them chosen. `on_click` gets the index of the one clicked.
pub fn chips(
    id: impl Into<ElementId>,
    labels: &[&str],
    selected: &[usize],
    on_click: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> Div {
    let id: ElementId = id.into();
    let on_click = Rc::new(on_click);
    let mut row = div().flex().flex_row().flex_wrap().gap_1();
    for (index, label) in labels.iter().enumerate() {
        let on_click = on_click.clone();
        row = row.child(
            chip(child_id(&id, index), selected.contains(&index))
                .on_click(move |_, window, cx| on_click(index, window, cx))
                .child(SharedString::from((*label).to_owned())),
        );
    }
    row
}

/// A swatch showing `color`; clicking it calls `on_toggle`. With `open`, the color panel shows
/// under it (see [`crate::color_picker`]), and every change of the color calls `on_pick` while the
/// panel stays open: a click outside it, or on the swatch, calls `on_toggle` to close it.
pub fn color_swatch(
    id: impl Into<ElementId>,
    color: u32,
    open: bool,
    cx: &mut App,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    on_pick: impl Fn(u32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let id: ElementId = id.into();
    let on_toggle = Rc::new(on_toggle);
    let close = on_toggle.clone();
    let panel = color_picker::panel(&id, color, open, cx, Rc::new(on_pick), close);
    let hover = panel.clone();
    let swatch = div()
        .id(id.clone())
        .size(px(tokens::height::COMPACT))
        .p(px(3.))
        .rounded_md()
        .border_1()
        .border_color(if open {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .cursor_pointer()
        .hover(|s| s.border_color(theme::border_strong()))
        .on_hover(move |hovered, _window, cx| {
            color_picker::set_swatch_hovered(&hover, *hovered, cx);
        })
        .on_click(move |_, window, cx| on_toggle(window, cx))
        .child(div().size_full().rounded_sm().bg(gpui::rgb(color)));
    if !open {
        return swatch.into_any_element();
    }
    div()
        .relative()
        .child(swatch)
        // Above the dialogs of gpui-component, which hold the color fields of the settings.
        .child(menu::below(panel, tokens::height::COMPACT, 100))
        .into_any_element()
}

/// A switch in the look of the panels: small, with the pointer of a button. The caller adds what
/// it needs (a label, `disabled`, the click).
pub fn switch(id: impl Into<ElementId>, on: bool) -> Switch {
    Switch::new(id).cursor_pointer().small().checked(on)
}

/// A switch for a row: `on_change` gets the new state.
pub fn toggle(
    id: impl Into<ElementId>,
    on: bool,
    on_change: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> Switch {
    switch(id, on).on_click(move |checked, window, cx| on_change(*checked, window, cx))
}

/// One option of a picker: a box holding `glyph`, lit when chosen.
fn option_box(id: ElementId, chosen: bool, glyph: impl IntoElement) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w(px(40.))
        .h(px(crate::tokens::height::CONTROL))
        .rounded_md()
        .border_1()
        .border_color(if chosen {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .cursor_pointer()
        .when(chosen, |el| el.bg(theme::accent_selected()))
        .when(!chosen, |el| el.hover(|s| s.bg(theme::surface_hover())))
        .child(glyph)
}

/// The text color of an option: full when chosen, muted otherwise.
pub(crate) fn ink(chosen: bool) -> Rgba {
    if chosen {
        theme::fg()
    } else {
        theme::muted_fg()
    }
}

/// Line widths shown as lines of that thickness, one chosen (`None` when the value is none of
/// them).
pub fn width_picker(
    id: impl Into<ElementId>,
    widths: &[f32],
    selected: Option<usize>,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: ElementId = id.into();
    let on_select = Rc::new(on_select);
    let mut row = div().flex().flex_row().gap_1();
    for (index, width) in widths.iter().enumerate() {
        let chosen = selected == Some(index);
        let on_select = on_select.clone();
        row = row.child(
            option_box(
                child_id(&id, index),
                chosen,
                div()
                    .w(px(20.))
                    .h(px(width.clamp(1.0, 6.0)))
                    .rounded_full()
                    .bg(ink(chosen)),
            )
            .tooltip(tooltip(format!("{width}")))
            .on_click(move |_, window, cx| on_select(index, window, cx)),
        );
    }
    row
}

/// A sample of a line style, drawn as shapes so every style sits on the same middle line and has
/// the same width (text dots sit on the baseline and drift off center). `style` is 0 for solid, 1
/// for dashed and 2 for dotted.
pub fn dash_glyph(style: usize, ink: Rgba) -> impl IntoElement {
    let row = div().flex().flex_row().items_center().justify_center();
    match style {
        0 => row.child(div().w(px(22.)).h(px(2.)).rounded_full().bg(ink)),
        1 => row
            .gap(px(3.))
            .children((0..3).map(|_| div().w(px(6.)).h(px(2.)).rounded_full().bg(ink))),
        _ => row
            .gap(px(4.))
            .children((0..4).map(|_| div().size(px(2.)).rounded_full().bg(ink))),
    }
}

/// The three line styles as samples, one chosen.
pub fn dash_picker(
    id: impl Into<ElementId>,
    selected: usize,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let id: ElementId = id.into();
    let on_select = Rc::new(on_select);
    let mut row = div().flex().flex_row().gap_1();
    for (index, name) in ["Solid", "Dashed", "Dotted"].into_iter().enumerate() {
        let chosen = selected == index;
        let on_select = on_select.clone();
        row = row.child(
            option_box(child_id(&id, index), chosen, dash_glyph(index, ink(chosen)))
                .tooltip(tooltip(name))
                .on_click(move |_, window, cx| on_select(index, window, cx)),
        );
    }
    row
}

/// Heights by name (a name and a weight against the rest), one chosen when `current` is the
/// weight of one of them. `on_pick` gets the weight.
pub fn height_picker(
    id: impl Into<ElementId>,
    presets: &'static [(&'static str, f32)],
    current: f32,
    on_pick: impl Fn(f32, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let names: Vec<&str> = presets.iter().map(|(name, _)| *name).collect();
    let selected = presets
        .iter()
        .position(|(_, weight)| (weight - current).abs() < 0.05)
        .unwrap_or(usize::MAX);
    segmented(id, &names, selected, move |choice, window, cx| {
        on_pick(presets[choice].1, window, cx);
    })
    .into_any_element()
}

/// A tooltip with `text`, in the shape `.tooltip(...)` of a gpui element takes. Every hand-built
/// control uses this one, so tooltips look the same everywhere.
pub fn tooltip(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| {
        gpui_kit::component::tooltip::Tooltip::new(text.clone())
            .m_1()
            .build(window, cx)
    }
}
