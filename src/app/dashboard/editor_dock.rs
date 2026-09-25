//! The editor of the indicator scripts as a panel under the charts, and the button in the header
//! that opens it. The panel is made the first time it is asked for (it needs the window), and is
//! resized by dragging its top edge, like the account panel.

use gpui::prelude::*;
use gpui::{Context, MouseButton, MouseMoveEvent, Window, div, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Selectable, Sizable};

use super::Dashboard;
use crate::app::indicators::{self, editor::EditorEvent, editor::IndicatorEditor};
use crate::app::menu::{Entry, Item, Menu, Placement};
use crate::app::theme;

/// The least and the most height of the panel that is not maximized.
const DOCK_MIN: f32 = 220.0;
const DOCK_MAX: f32 = 900.0;
pub(super) const DOCK_DEFAULT: f32 = 420.0;

/// What is left of the window for the charts when the editor is maximized.
const MAXIMIZED_MARGIN: f32 = 190.0;

impl Dashboard {
    /// Makes the editor (it needs the window) once it is wanted.
    pub(super) fn editor_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewport_height = f32::from(window.viewport_size().height);
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

    /// The height the panel has now.
    fn dock_height(&self, cx: &gpui::App) -> f32 {
        let maximized = self
            .editor
            .as_ref()
            .is_some_and(|e| e.read(cx).is_maximized());
        if maximized {
            (self.viewport_height - MAXIMIZED_MARGIN).max(DOCK_MIN)
        } else {
            self.editor_height
        }
    }

    /// The panel, when the editor is open: its top edge to drag, and the editor.
    pub(super) fn editor_dock(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let editor = self.editor.clone().filter(|_| self.editor_open)?;
        let dragging = self.editor_drag.is_some();
        Some(
            div()
                .flex_none()
                .h(px(self.dock_height(cx)))
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("editor-resize")
                        .flex_none()
                        .h(px(5.))
                        .w_full()
                        .cursor_row_resize()
                        .border_t_1()
                        .border_color(if dragging {
                            theme::accent()
                        } else {
                            theme::border_hairline()
                        })
                        .hover(|s| s.border_color(theme::accent()))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                                this.editor_drag = Some(f32::from(event.position.y));
                                cx.notify();
                            }),
                        ),
                )
                .child(div().flex_1().min_h_0().child(editor))
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
        let y = f32::from(event.position.y);
        let room = (self.viewport_height - MAXIMIZED_MARGIN).max(DOCK_MIN);
        self.editor_height = (self.editor_height - (y - last)).clamp(DOCK_MIN, DOCK_MAX.min(room));
        self.editor_drag = Some(y);
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
             hint: Option<&'static str>,
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
                Some("Ctrl+Shift+E"),
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
                    d.open_settings_page(crate::app::settings_hub::Page::Indicators, window, cx);
                },
            ),
        ];
        div()
            .relative()
            .flex_none()
            .child(
                Button::new("open-indicators")
                    .ghost()
                    .small()
                    .selected(self.editor_open)
                    .icon(IconName::CodeXml)
                    .tooltip("Indicator scripts: the folder, the editor, import and export")
                    .cursor_pointer()
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
            .children(menu.popup(items, Placement::Below(34.), window, cx))
    }
}

impl Dashboard {
    /// Opens the settings on `page`.
    pub(super) fn open_settings_page(
        &mut self,
        page: crate::app::settings_hub::Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        crate::app::settings_hub::open_at(
            self.workspace.clone(),
            self.multi.clone(),
            page,
            window,
            cx,
        );
    }
}
