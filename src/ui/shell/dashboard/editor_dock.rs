//! The editor of the indicator scripts as a drawer on the right, over the charts, and the button in
//! the header that opens it. The drawer floats above the charts and the ticket without moving
//! them; it covers three quarters of the width at first, and its left edge is dragged to make it
//! wider or narrower. It is made the first time it is asked for (it needs the window).

use crate::ui::kit::icon::IconName;
use crate::ui::kit::prelude::Selectable;
use gpui::prelude::*;
use gpui::{Context, MouseButton, MouseMoveEvent, Window, div, px};

use super::Dashboard;
use crate::app::scripts as indicators;
use crate::ui::features::indicators::editor::{EditorEvent, IndicatorEditor};
use crate::ui::kit::{
    menu::{Entry, Item, Menu, Placement},
    theme, tokens,
};

/// The share of the window the drawer covers at first, and the least and the most it can cover.
pub(super) const SHARE_DEFAULT: f32 = 0.75;
const SHARE_MIN: f32 = 0.35;
const SHARE_MAX: f32 = 0.98;
/// The narrowest the drawer gets, in pixels, whatever the share says: the explorer and the code
/// need room.
const WIDTH_MIN: f32 = 640.0;

impl Dashboard {
    /// Makes the editor (it needs the window) once it is wanted.
    pub(super) fn editor_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewport_height = f32::from(window.viewport_size().height);
        self.viewport_width = f32::from(window.viewport_size().width);
        if !self.editor_open || self.editor.is_some() {
            return;
        }
        let multi = self.multi.clone();
        let editor = cx.new(|cx| IndicatorEditor::new(multi, window, cx));
        cx.subscribe(
            &editor,
            |this, editor, event: &EditorEvent, cx| match event {
                EditorEvent::Close => this.set_editor_open(false, cx),
                EditorEvent::ToggleMaximize => {
                    editor.update(cx, |editor, cx| {
                        let next = !editor.is_maximized();
                        editor.set_maximized(next, cx);
                    });
                    cx.notify();
                }
            },
        )
        .detach();
        self.editor = Some(editor);
    }

    pub(super) fn set_editor_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.editor_open = open;
        cx.notify();
    }

    /// Opens the editor (or closes it) from a key or a button.
    pub(super) fn toggle_editor(&mut self, cx: &mut Context<Self>) {
        let open = !self.editor_open;
        self.set_editor_open(open, cx);
    }

    /// Opens the editor and runs `then` on it once it exists.
    pub(super) fn with_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut IndicatorEditor, &mut Window, &mut Context<IndicatorEditor>),
    ) {
        self.editor_open = true;
        self.editor_frame(window, cx);
        if let Some(editor) = self.editor.clone() {
            editor.update(cx, |editor, cx| then(editor, window, cx));
        }
        cx.notify();
    }

    /// The width the drawer has now, in pixels.
    fn dock_width(&self, cx: &gpui::App) -> f32 {
        let maximized = self
            .editor
            .as_ref()
            .is_some_and(|e| e.read(cx).is_maximized());
        let share = if maximized { 1.0 } else { self.editor_share };
        (share * self.viewport_width)
            .max(WIDTH_MIN)
            .min(self.viewport_width.max(WIDTH_MIN))
    }

    /// The drawer, when the editor is open: laid over the right of the charts, with its left edge
    /// to drag and the editor.
    pub(super) fn editor_dock(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let editor = self.editor.clone().filter(|_| self.editor_open)?;
        let dragging = self.editor_drag.is_some();
        Some(
            div()
                .id("editor-drawer")
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(self.dock_width(cx)))
                .max_w_full()
                .flex()
                .flex_row()
                // It sits over the charts: nothing under it hears the pointer.
                .occlude()
                // Being over it, the pointer never reaches the layout behind, which follows the
                // drag elsewhere: the drawer follows it here.
                .when(dragging, |el| {
                    el.on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                        this.drag_editor(event, cx);
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_editor_drag(cx)),
                    )
                    .on_mouse_up_out(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.end_editor_drag(cx)),
                    )
                })
                .bg(theme::bg())
                .border_l_1()
                .border_color(theme::border_subtle())
                .shadow_lg()
                .child(
                    div()
                        .id("editor-resize")
                        .flex_none()
                        .w(px(5.))
                        .h_full()
                        .cursor_col_resize()
                        .bg(if dragging {
                            theme::accent_alpha(0.5)
                        } else {
                            gpui::rgba(0x00000000)
                        })
                        .hover(|s| s.bg(theme::accent_alpha(0.5)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.editor_drag = Some(f32::from(event.position.x));
                                cx.notify();
                            }),
                        ),
                )
                .child(div().flex_1().min_w_0().h_full().child(editor))
                .into_any_element(),
        )
    }

    pub(super) fn drag_editor(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(last) = self.editor_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.editor_drag = None;
            cx.notify();
            return;
        }
        let x = f32::from(event.position.x);
        let width = self.viewport_width.max(1.0);
        // Dragging the left edge to the left makes the drawer wider.
        let least = (WIDTH_MIN / width).clamp(SHARE_MIN, SHARE_MAX);
        self.editor_share = (self.editor_share - (x - last) / width).clamp(least, SHARE_MAX);
        self.editor_drag = Some(x);
        cx.notify();
    }

    pub(super) fn end_editor_drag(&mut self, cx: &mut Context<Self>) {
        if self.editor_drag.take().is_some() {
            cx.notify();
        }
    }

    /// The button of the header for the indicators: it opens the menu of what can be done with
    /// them, the first being to open their folder.
    pub(super) fn indicators_button(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let unsaved = self
            .editor
            .as_ref()
            .is_some_and(|e| e.read(cx).has_unsaved());
        let menu = Menu::new("header-indicators-menu", window, cx);
        let toggle = menu.clone();
        let entity = cx.entity();
        let run =
            |label: &'static str,
             icon: IconName,
             hint: Option<gpui::SharedString>,
             action: fn(&mut Dashboard, &mut Window, &mut Context<Dashboard>)| {
                let (entity, menu) = (entity.clone(), menu.clone());
                let mut entry = Entry::new(label).icon(icon).on_click(move |window, cx| {
                    menu.close(cx);
                    entity.update(cx, |dashboard, cx| action(dashboard, window, cx));
                });
                if let Some(hint) = hint {
                    entry = entry.hint(hint);
                }
                Item::from(entry)
            };
        let items = vec![
            run(
                "Open the indicators folder",
                IconName::FolderOpen,
                None,
                |_, _, cx| {
                    indicators::open_folder(cx);
                },
            ),
            Item::Separator,
            run(
                "Indicator editor",
                IconName::CodeXml,
                Some(crate::ui::kit::shortcut::text("Ctrl+Shift+E")),
                |d, _, cx| {
                    d.toggle_editor(cx);
                },
            ),
            run(
                "New indicator...",
                IconName::FilePlus,
                None,
                |d, window, cx| {
                    d.with_editor(window, cx, |editor, window, cx| {
                        editor.ask_new(0, window, cx)
                    });
                },
            ),
            run(
                "Import indicators...",
                IconName::FileUp,
                None,
                |d, window, cx| {
                    d.with_editor(window, cx, |editor, window, cx| editor.import(window, cx));
                },
            ),
            run(
                "Export every indicator...",
                IconName::FileDown,
                None,
                |d, window, cx| {
                    d.with_editor(window, cx, |editor, _, cx| editor.export_all(cx));
                },
            ),
            Item::Separator,
            run(
                "Read the folder again",
                IconName::RefreshCw,
                None,
                |_, _, cx| {
                    indicators::reload(cx).detach();
                },
            ),
            run(
                "Indicator settings...",
                IconName::Settings2,
                None,
                |d, window, cx| {
                    d.open_settings_page(
                        crate::ui::shell::settings_hub::Page::Indicators,
                        window,
                        cx,
                    );
                },
            ),
        ];
        div()
            .relative()
            .flex_none()
            .child(
                crate::ui::kit::button::quiet("open-indicators")
                    .selected(self.editor_open)
                    .icon(IconName::CodeXml)
                    .tooltip("Indicator scripts: the folder, the editor, import and export")
                    .on_click(move |_, _window, cx| toggle.toggle(cx)),
            )
            .children(unsaved.then(|| {
                div()
                    .absolute()
                    .top(px(4.))
                    .right(px(4.))
                    .size(px(7.))
                    .rounded_full()
                    .bg(theme::amber())
            }))
            .children(menu.popup(
                items,
                Placement::Below(tokens::height::compact()),
                window,
                cx,
            ))
    }
}

impl Dashboard {
    /// Opens the settings on `page`.
    pub(super) fn open_settings_page(
        &mut self,
        page: crate::ui::shell::settings_hub::Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::ui::shell::settings_hub::open_at(
            self.workspace.clone(),
            self.multi.clone(),
            page,
            window,
            cx,
        );
    }
}
