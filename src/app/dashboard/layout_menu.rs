//! The layout button in the header and the picker it opens: how many charts, how they are
//! arranged, and what is linked between them.

use gpui::prelude::*;
use gpui::{Context, SharedString, Window, div, px};

use super::Dashboard;
use crate::app::multichart::icon::layout_icon;
use crate::app::multichart::layouts::{self, LayoutKey};
use crate::app::multichart::links::{Link, Links};
use crate::app::{menu, theme};

/// The links on offer, with what each one does.
const LINK_ROWS: [(Option<Link>, &str, &str); 5] = [
    (
        Some(Link::Symbol),
        "Symbol",
        "Every chart shows the same symbol",
    ),
    (
        Some(Link::Interval),
        "Interval",
        "One timeframe for every chart",
    ),
    (
        Some(Link::Crosshair),
        "Crosshair",
        "The pointer shows on every chart",
    ),
    (
        Some(Link::Time),
        "Time",
        "Scrolling moves every chart to the same time",
    ),
    (
        Some(Link::Range),
        "Date range",
        "Zooming and scrolling give every chart the same span",
    ),
];

impl Dashboard {
    /// The button that shows the current layout, and the picker under it when it is open.
    pub(super) fn layout_button(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let key = self.multi.read(cx).layout_key();
        let open = self.layout_menu_open;
        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id("layout-button")
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(28.))
                    .px_2()
                    .rounded_md()
                    .cursor_pointer()
                    .when(open, |el| el.bg(theme::accent_selected()))
                    .hover(|style| style.bg(theme::surface_hover()))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.layout_menu_open = !this.layout_menu_open;
                        this.tf_menu_open = false;
                        cx.notify();
                    }))
                    .child(layout_icon(layouts::layout(key), 20., 16., false)),
            )
            .children(open.then(|| self.layout_menu(key, window, cx)))
    }

    pub(super) fn pick_layout(&mut self, key: LayoutKey, cx: &mut Context<Self>) {
        self.layout_menu_open = false;
        self.multi.update(cx, |multi, cx| multi.set_layout(key, cx));
        cx.notify();
    }

    fn layout_menu(
        &self,
        current: LayoutKey,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // As tall as the window allows, so the links at the bottom are never cut off.
        let max_height = (f32::from(window.viewport_size().height) - 112.0).max(240.0);
        let links = self.multi.read(cx).sync();

        let mut arrangements = div().flex().flex_col();
        for (count, variants) in layouts::catalog() {
            let mut icons = Vec::new();
            for (variant, arrangement) in variants.iter().enumerate() {
                let key = LayoutKey {
                    count: *count,
                    variant,
                };
                icons.push(
                    div()
                        .id(("layout-option", count * 100 + variant))
                        .p(px(2.))
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|style| style.bg(theme::surface_hover()))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.pick_layout(key, cx);
                        }))
                        .child(layout_icon(arrangement, 24., 18., key == current)),
                );
            }
            arrangements = arrangements.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .py_1()
                    .border_b_1()
                    .border_color(theme::border_hairline())
                    .child(
                        div()
                            .w(px(20.))
                            .text_size(px(11.))
                            .text_color(theme::muted_fg())
                            .child(count.to_string()),
                    )
                    .child(div().flex().flex_row().flex_wrap().gap_1().children(icons)),
            );
        }

        let reset = div()
            .id("layout-reset-splits")
            .mt_2()
            .px_1()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(theme::muted_fg())
            .hover(|style| style.bg(theme::surface_hover()).text_color(theme::fg()))
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.multi.update(cx, |multi, cx| multi.reset_splits(cx));
            }))
            .child("Make the charts equal again (or double click a line between them)");
        let mut links_section = div().flex().flex_col().gap_1().pt_3().child(
            div()
                .pb_1()
                .text_size(px(10.5))
                .text_color(theme::muted_fg())
                .child("SYNC IN LAYOUT"),
        );
        for (index, (link, title, hint)) in LINK_ROWS.into_iter().enumerate() {
            links_section = links_section.child(link_row(index, link, title, hint, links, cx));
        }

        let card = div()
            .id("layout-menu")
            .w(px(380.))
            .max_h(px(max_height))
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .occlude()
            .child(arrangements)
            .child(reset)
            .child(links_section);

        menu::below(
            crate::app::anim::enter(card, "layout-menu-card", 0),
            menu::BELOW_BUTTON,
            1,
        )
    }
}

/// One line of the sync section: its name, what it does, and a switch.
fn link_row(
    index: usize,
    link: Option<Link>,
    title: &str,
    hint: &str,
    links: Links,
    cx: &mut Context<Dashboard>,
) -> impl IntoElement + use<> {
    let on = link.is_none_or(|link| link.is_on(&links));
    div()
        .id(SharedString::from(format!("layout-link-{index}")))
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .py_1()
        .px_1()
        .rounded_md()
        .when(link.is_some(), |el| {
            el.cursor_pointer()
                .hover(|style| style.bg(theme::surface_hover()))
        })
        .when_some(link, |el, link| {
            el.on_click(cx.listener(move |this, _event, _window, cx| {
                this.multi
                    .update(cx, |multi, cx| multi.toggle_link(link, cx));
                cx.notify();
            }))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme::fg())
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(hint.to_owned()),
                ),
        )
        .child(switch(on, link.is_none()))
}

/// A small on/off switch. A locked one is dimmed.
fn switch(on: bool, locked: bool) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(34.))
        .h(px(20.))
        .p(px(2.))
        .rounded_full()
        .flex()
        .flex_row()
        .bg(if on {
            theme::accent()
        } else {
            theme::surface_pressed()
        })
        .when(on, |el| el.justify_end())
        .when(locked, |el| el.opacity(0.5))
        .child(div().size(px(16.)).rounded_full().bg(theme::fg()))
}
