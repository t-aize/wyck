//! Favorites and watchlists in the symbol picker: the row of list chips under the class chips,
//! the star on each symbol, the panel in the details sheet that puts a symbol in lists, and the
//! small editor that names a new list or renames one.
//!
//! The lists themselves live in the [`Workspace`](crate::app::workspace::Workspace), which saves
//! them; this file only shows them and turns clicks into edits of them.

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, Focusable, MouseButton, SharedString, Subscription, Window, div,
    px,
};
use gpui_kit::assets::IconName;

use super::Dashboard;
use crate::app::connection::ui;
use crate::app::text_input::TextInput;
use crate::app::theme;
use crate::app::workspace::{NameError, Watchlists};

/// Which symbols the picker is limited to, besides the asset class and the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scope {
    Favorites,
    /// The watchlist at this position.
    List(usize),
}

impl Scope {
    /// Whether `symbol` is in the scope.
    pub(super) fn contains(self, lists: &Watchlists, symbol: &str) -> bool {
        match self {
            Self::Favorites => lists.is_favorite(symbol),
            Self::List(index) => lists.in_list(index, symbol),
        }
    }
}

/// The field that names a new list, or renames one.
pub(super) struct ListEditor {
    pub input: Entity<TextInput>,
    /// The list being renamed, or `None` for a new one.
    pub renaming: Option<usize>,
    pub error: Option<NameError>,
    _observe: Subscription,
}

impl Dashboard {
    /// Limits the picker to a scope, or lifts the limit.
    pub(super) fn set_scope(&mut self, scope: Option<Scope>, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.as_mut() {
            picker.scope = scope;
            picker.list_editor = None;
            cx.notify();
        }
    }

    /// Opens the field that names a list: a new one, or the one at `renaming`.
    pub(super) fn open_list_editor(
        &mut self,
        renaming: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = renaming.and_then(|index| {
            self.workspace
                .read(cx)
                .watchlists()
                .lists
                .get(index)
                .map(|list| list.name.clone())
        });
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let input = cx.new(|cx| TextInput::new(cx, "Name of the list"));
        if let Some(name) = current {
            input.update(cx, |input, cx| input.set_text(name, cx));
        }
        let observe = cx.observe(&input, |this, _input, cx| {
            // Typing clears an old complaint.
            if let Some(editor) = this.picker.as_mut().and_then(|p| p.list_editor.as_mut()) {
                editor.error = None;
            }
            cx.notify();
        });
        window.focus(&input.focus_handle(cx), cx);
        picker.list_editor = Some(ListEditor {
            input,
            renaming,
            error: None,
            _observe: observe,
        });
        cx.notify();
    }

    /// Closes the list field without saving, giving the search field the keyboard back.
    pub(super) fn close_list_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.as_mut()
            && picker.list_editor.take().is_some()
        {
            window.focus(&picker.input.focus_handle(cx), cx);
            cx.notify();
        }
    }

    /// Enter in the list field: creates the list (and shows it), or renames it.
    pub(super) fn commit_list_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((name, renaming)) = self
            .picker
            .as_ref()
            .and_then(|p| p.list_editor.as_ref())
            .map(|editor| (editor.input.read(cx).text().to_owned(), editor.renaming))
        else {
            return;
        };
        let outcome = self.workspace.update(cx, |workspace, cx| {
            workspace.edit_watchlists(cx, |lists| match renaming {
                Some(index) => lists.rename_list(index, &name).map(|()| index),
                None => lists.create_list(&name),
            })
        });
        match outcome {
            Ok(index) => {
                self.close_list_editor(window, cx);
                self.set_scope(Some(Scope::List(index)), cx);
            }
            Err(error) => {
                if let Some(editor) = self.picker.as_mut().and_then(|p| p.list_editor.as_mut()) {
                    editor.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn delete_list(&mut self, index: usize, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_watchlists(cx, |lists| lists.delete_list(index));
        });
        // The lists after it moved up, and the one on screen may be gone.
        self.set_scope(None, cx);
        self.recompute_results();
    }

    pub(super) fn toggle_favorite_symbol(&mut self, symbol: &str, cx: &mut Context<Self>) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_watchlists(cx, |lists| lists.toggle_favorite(symbol));
        });
        self.recompute_results();
        cx.notify();
    }

    pub(super) fn toggle_symbol_in_list(
        &mut self,
        index: usize,
        symbol: &str,
        cx: &mut Context<Self>,
    ) {
        self.workspace.update(cx, |workspace, cx| {
            workspace.edit_watchlists(cx, |lists| lists.toggle_in_list(index, symbol));
        });
        self.recompute_results();
        cx.notify();
    }

    /// Makes the picker work its results out again on the next frame.
    pub(super) fn recompute_results(&mut self) {
        if let Some(picker) = self.picker.as_mut() {
            picker.computed_for = None;
        }
    }
}

/// A rounded chip, like the asset class ones.
fn chip(
    id: impl Into<gpui::ElementId>,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .px_3()
        .py_1()
        .rounded_full()
        .border_1()
        .border_color(if selected {
            theme::accent()
        } else {
            theme::border_subtle()
        })
        .bg(if selected {
            theme::accent_selected()
        } else {
            theme::bg()
        })
        .text_size(px(12.))
        .text_color(if selected {
            theme::fg()
        } else {
            theme::muted_fg()
        })
        .cursor_pointer()
        .when(!selected, |el| {
            el.hover(|style| style.bg(theme::surface_hover()).text_color(theme::fg()))
        })
        .on_click(on_click)
}

/// A small round icon button that sits next to a chip.
fn icon_button(
    id: impl Into<gpui::ElementId>,
    icon: IconName,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(px(26.))
        .rounded_full()
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface_hover()))
        .on_click(on_click)
        .child(ui::icon_colored(icon, 14., theme::muted_fg()))
}

/// The row of chips under the class chips: favorites, each watchlist, and a way to make one.
pub(super) fn list_chips(
    lists: &Watchlists,
    scope: Option<Scope>,
    editor: Option<&ListEditor>,
    cx: &mut Context<Dashboard>,
) -> Vec<AnyElement> {
    let mut out: Vec<AnyElement> = Vec::new();

    out.push(
        chip(
            "scope-favorites",
            scope == Some(Scope::Favorites),
            cx.listener(|this, _event, _window, cx| {
                let next = (this.picker.as_ref().and_then(|p| p.scope) != Some(Scope::Favorites))
                    .then_some(Scope::Favorites);
                this.set_scope(next, cx);
            }),
        )
        .child(ui::icon_colored(IconName::Star, 12., theme::amber()))
        .child(format!("Favorites {}", lists.favorites.len()))
        .into_any_element(),
    );

    for (index, list) in lists.lists.iter().enumerate() {
        let selected = scope == Some(Scope::List(index));
        out.push(
            chip(
                ("scope-list", index as u64),
                selected,
                cx.listener(move |this, _event, _window, cx| {
                    let current = this.picker.as_ref().and_then(|p| p.scope);
                    let next = (current != Some(Scope::List(index))).then_some(Scope::List(index));
                    this.set_scope(next, cx);
                }),
            )
            .child(list.name.clone())
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(list.symbols.len().to_string()),
            )
            .into_any_element(),
        );
        if selected && editor.is_none() {
            out.push(
                icon_button(
                    ("rename-list", index as u64),
                    IconName::Pencil,
                    cx.listener(move |this, _event, window, cx| {
                        this.open_list_editor(Some(index), window, cx);
                    }),
                )
                .into_any_element(),
            );
            out.push(
                icon_button(
                    ("delete-list", index as u64),
                    IconName::Trash,
                    cx.listener(move |this, _event, _window, cx| this.delete_list(index, cx)),
                )
                .into_any_element(),
            );
        }
    }

    match editor {
        None => out.push(
            chip(
                "new-list",
                false,
                cx.listener(|this, _event, window, cx| this.open_list_editor(None, window, cx)),
            )
            .child(ui::icon_colored(IconName::Plus, 12., theme::muted_fg()))
            .child("New list")
            .into_any_element(),
        ),
        Some(editor) => out.push(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(div().w(px(200.)).child(editor.input.clone()))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(match editor.error {
                            Some(_) => theme::destructive(),
                            None => theme::muted_fg(),
                        })
                        .child(match editor.error {
                            Some(error) => error.to_string(),
                            None => "Enter to save, Esc to cancel".to_owned(),
                        }),
                )
                .into_any_element(),
        ),
    }
    out
}

/// The star at the end of a symbol row.
pub(super) fn star(
    row: usize,
    favorite: bool,
    cx: &mut Context<Dashboard>,
    symbol: SharedString,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(("symbol-star", row as u64))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(30.))
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(theme::surface_pressed()))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.toggle_favorite_symbol(&symbol, cx);
        }))
        .child(ui::icon_colored(
            IconName::Star,
            15.,
            if favorite {
                theme::amber()
            } else {
                theme::border_strong()
            },
        ))
}

/// The panel of the details sheet that puts the symbol in lists.
pub(super) fn membership_panel(
    symbol: &str,
    lists: &Watchlists,
    cx: &mut Context<Dashboard>,
) -> AnyElement {
    let row = |id: (&'static str, u64), title: SharedString, on: bool, icon: IconName, tone| {
        (id, title, on, icon, tone)
    };
    let mut rows = vec![row(
        ("member-favorite", 0),
        "Favorites".into(),
        lists.is_favorite(symbol),
        IconName::Star,
        theme::amber(),
    )];
    for (index, list) in lists.lists.iter().enumerate() {
        rows.push(row(
            ("member-list", index as u64),
            list.name.clone().into(),
            lists.in_list(index, symbol),
            IconName::ListPlus,
            theme::accent(),
        ));
    }

    let mut panel = div()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(theme::border_subtle())
        .bg(theme::surface())
        .overflow_hidden();
    let last = rows.len() - 1;
    for (position, (id, title, on, icon, tone)) in rows.into_iter().enumerate() {
        let name = symbol.to_owned();
        let is_favorite = id.0 == "member-favorite";
        let list_index = id.1 as usize;
        panel = panel.child(
            div()
                .id(id)
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .cursor_pointer()
                .hover(|style| style.bg(theme::surface_hover()))
                .when(position != last, |el| {
                    el.border_b_1().border_color(theme::border_hairline())
                })
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if is_favorite {
                        this.toggle_favorite_symbol(&name, cx);
                    } else {
                        this.toggle_symbol_in_list(list_index, &name, cx);
                    }
                }))
                .child(ui::icon_colored(
                    icon,
                    14.,
                    if on { tone } else { theme::muted_fg() },
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(theme::fg())
                        .child(title),
                )
                .child(ui::icon_colored(
                    if on { IconName::Check } else { IconName::Plus },
                    14.,
                    if on {
                        theme::accent()
                    } else {
                        theme::muted_fg()
                    },
                )),
        );
    }
    if lists.lists.is_empty() {
        panel = panel.child(
            div()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(theme::border_hairline())
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child("Make a list with New list, above, then add symbols to it here."),
        );
    }
    div()
        .child(
            div()
                .mt_4()
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child("SAVE TO"),
        )
        .child(panel.mt_2())
        .into_any_element()
}
