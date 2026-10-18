//! What sits over the canvas: the legend of each band (the symbol, the prices under the pointer,
//! the indicators with their buttons), the toolbar, its menus, the labels of the order and alert
//! lines, the right-click menu, and the loading, failed and empty states.

pub(crate) mod headline;
pub(crate) mod legend;
pub(crate) mod menus;

use super::{Chart, paint};
use gpui::prelude::*;
use gpui::{Context, MouseButton, Window, div};
use std::rc::Rc;
use wyck_ui_kit::menu::{self as popup};

fn rgb(color: u32) -> gpui::Rgba {
    gpui::rgb(color)
}

impl Render for Chart {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.open_pending_settings(window, cx);
        self.schedule_customs(cx);
        let entity = cx.entity();
        let compact = self.is_compact();
        let geometry = self.geometry();
        let menu = popup::Menu::new(("chart-context-menu", self.id), window, cx);
        let context = if menu.is_open(cx) {
            self.context_items(&entity, menu.position(cx), cx)
        } else {
            Vec::new()
        };
        let opener = menu.clone();

        div()
            .id(("chart", self.id))
            .relative()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .bg(self.palette().bg)
            .child(paint::surface(&entity, Rc::clone(&self.bounds)))
            .children(self.legends(&geometry, compact, cx))
            .children(self.line_labels(&geometry, cx))
            .children(self.corner(cx))
            .children(self.menu_popup(&geometry, cx))
            .children(self.drawing_hint(&geometry, cx))
            .children(self.replay_banner(&geometry))
            .children(self.marquee())
            .children(self.status(cx))
            .on_mouse_down(MouseButton::Right, move |event, _window, cx| {
                opener.open(Some(event.position), cx);
            })
            .children(menu.popup(context, popup::Placement::Cursor, window, cx))
    }
}
