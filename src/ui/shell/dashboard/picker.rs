//! The symbol picker: a palette over the dashboard listing every symbol the account offers.
//!
//! It opens from the symbol button in the header, or with Ctrl+K (Cmd+K on macOS). A search field
//! on top, filters by asset class, then the list; choosing a symbol makes it the one the dashboard
//! follows. The list is virtualized, so an account with thousands of shares scrolls as smoothly as
//! one with a hundred forex pairs.
//!
//! Keys: Up and Down (or Ctrl+P and Ctrl+N) move the highlight, Page Up and Page Down move it by a
//! page, Enter chooses, Escape closes. The search field would keep the arrow keys for itself, but
//! it binds none of them, so they reach the picker's key context. The mouse works the same:
//! hovering highlights, clicking chooses, clicking outside closes.

mod footer;
mod rows;

use std::ops::Range;
use std::time::Duration;

use crate::ui::kit::icon::IconName;
use gpui::prelude::*;
use gpui::{
    Context, Entity, Focusable, MouseButton, ScrollStrategy, Subscription, UniformListScrollHandle,
    Window, div, px, relative, uniform_list,
};

use super::catalog::{Class, Entry};
use super::details::{self, Detail};
use super::lists::{self, ListEditor, Scope};
use super::{Dashboard, Load, PickerConfirm, PickerDown, PickerPageDown, PickerPageUp, PickerUp};
use crate::app::broker::Error as ApiError;
use crate::app::system::runtime;
use crate::ui::kit::{anim, button, icon, text_input::TextInput, theme, tokens};

/// The width of the symbol picker.
const PICKER_WIDTH: f32 = 1160.0;
/// The width of a column of the results of the picker.
const RESULT_COLUMN_WIDTH: f32 = 120.0;

const ROW_HEIGHT: f32 = 52.;
/// How many rows a page key moves.
const PAGE: isize = 8;

pub(super) struct Picker {
    pub(super) input: Entity<TextInput>,
    /// Re-renders the dashboard when the search text changes.
    _observe: Subscription,
    class: Option<Class>,
    /// Favorites or one watchlist, on top of the class and the search.
    pub(super) scope: Option<Scope>,
    /// The field that names a list, while it is open.
    pub(super) list_editor: Option<ListEditor>,
    highlighted: usize,
    /// The catalog indices matching the current search, best first.
    results: Vec<usize>,
    /// The search the results were computed for.
    pub(super) computed_for: Option<(String, Option<Class>, Option<Scope>)>,
    scroll: UniformListScrollHandle,
    /// The symbol whose details were last asked for, so a re-render does not ask again.
    requested: Option<i64>,
}

impl Dashboard {
    pub(super) fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.catalog {
            Load::Ready(_) => {}
            // A click on the button after a failed load asks again.
            Load::Failed(_) => {
                self.catalog = Load::Loading;
                self.load_catalog(cx);
                cx.notify();
                return;
            }
            Load::Loading => return,
        }
        self.close_menus();
        let input = cx.new(|cx| TextInput::new(cx, "Search symbols, e.g. EURUSD or gold"));
        let observe = cx.observe(&input, |_this, _input, cx| cx.notify());
        window.focus(&input.focus_handle(cx), cx);
        self.picker_opens += 1;
        self.picker = Some(Picker {
            input,
            _observe: observe,
            class: None,
            scope: None,
            list_editor: None,
            highlighted: 0,
            results: Vec::new(),
            computed_for: None,
            scroll: UniformListScrollHandle::new(),
            requested: None,
        });
        cx.notify();
    }

    /// Escape: closes the picker, or else the account menu.
    pub(super) fn close_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Escape in the list field only closes that field.
        if self
            .picker
            .as_ref()
            .is_some_and(|picker| picker.list_editor.is_some())
        {
            self.close_list_editor(window, cx);
            return;
        }
        self.drop_peek();
        self.picker = None;
        self.close_menus();
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn move_highlight(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let Some(last) = picker.results.len().checked_sub(1) else {
            return;
        };
        let next = picker.highlighted.saturating_add_signed(delta).min(last);
        picker.highlighted = next;
        picker.scroll.scroll_to_item(next, ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Chooses the symbol on `row` of the current results.
    fn choose(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Load::Ready(catalog) = &self.catalog else {
            return;
        };
        let entry = self
            .picker
            .as_ref()
            .and_then(|picker| picker.results.get(row))
            .and_then(|index| catalog.entry(*index))
            .cloned();
        if let Some(entry) = entry {
            self.select(entry, true, cx);
            self.close_overlays(window, cx);
        }
    }

    pub(super) fn render_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        // As tall as the window allows, with a margin above and below.
        let height = (f32::from(window.viewport_size().height) - 14.0).clamp(320.0, 820.0);
        let Load::Ready(catalog) = &self.catalog else {
            return None;
        };
        let catalog = catalog.clone();
        let picker = self.picker.as_mut()?;

        let text = picker.input.read(cx).text().to_owned();
        let search = (text.clone(), picker.class, picker.scope);
        let lists = self.workspace.read(cx).watchlists().clone();
        if picker.computed_for.as_ref() != Some(&search) {
            let mut results = catalog.query(&text, picker.class);
            if let Some(scope) = picker.scope {
                results.retain(|index| {
                    catalog
                        .entry(*index)
                        .is_some_and(|entry| scope.contains(&lists, &entry.name))
                });
            }
            picker.results = results;
            picker.highlighted = 0;
            picker.computed_for = Some(search);
            picker.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        let input = picker.input.clone();
        let scroll = picker.scroll.clone();
        let selected_class = picker.class;
        let scope = picker.scope;
        let count = picker.results.len();
        let highlighted_row = picker.highlighted;
        let highlighted: Option<Entry> = picker
            .results
            .get(picker.highlighted)
            .and_then(|index| catalog.entry(*index))
            .cloned();
        let highlighted_id = highlighted.as_ref().map(|entry| entry.id);
        self.schedule_details(highlighted_id, cx);
        let active_id = self.active.as_ref().map(|a| a.entry.id);
        let is_current = highlighted_id.is_some() && highlighted_id == active_id;
        let live = highlighted_id.and_then(|id| {
            if Some(id) == active_id {
                self.price_text()
            } else {
                self.peek_text(id)
            }
        });
        let action = highlighted.as_ref().map(|entry| {
            button::primary(
                "pick-symbol",
                if is_current {
                    format!("Stay on {}", entry.name)
                } else {
                    format!("Use {}", entry.name)
                },
                cx.listener(move |this, _event, window, cx| {
                    this.choose(highlighted_row, window, cx);
                }),
            )
            .into_any_element()
        });
        let sheet = details::render_details(details::Sheet {
            entry: highlighted.as_ref(),
            detail: highlighted_id.and_then(|id| self.details.get(&id)),
            live,
            is_current,
            action,
            lists: highlighted
                .as_ref()
                .map(|entry| lists::membership_panel(&entry.name, &lists, cx)),
            epoch: self.picker_opens,
        });
        let total = catalog.total();

        let chips = std::iter::once(None)
            .chain(catalog.classes().into_iter().map(Some))
            .map(|class| rows::class_chip(class, class == selected_class, cx))
            .collect::<Vec<_>>();

        let list_chips = lists::list_chips(
            &lists,
            scope,
            self.picker.as_ref().and_then(|p| p.list_editor.as_ref()),
            cx,
        );
        let empty_message = match scope {
            Some(Scope::Favorites) if lists.favorites.is_empty() => {
                "No favorites yet. Click the star of a symbol to add it."
            }
            Some(Scope::List(_)) if count == 0 && text.trim().is_empty() => {
                "This list is empty. Highlight a symbol and save it to the list."
            }
            _ => "No symbol matches your search",
        };

        let list = if count == 0 {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px_6()
                .text_center()
                .text_size(px(tokens::text::emphasis()))
                .text_color(theme::muted_fg())
                .child(empty_message)
                .into_any_element()
        } else {
            uniform_list(
                "symbol-list",
                count,
                cx.processor(move |this, range: Range<usize>, _window, cx| {
                    let Load::Ready(catalog) = &this.catalog else {
                        return Vec::new();
                    };
                    let Some(picker) = &this.picker else {
                        return Vec::new();
                    };
                    let active = this.active.as_ref().map(|a| a.entry.id);
                    let watchlists = this.workspace.read(cx).watchlists().clone();
                    range
                        .filter_map(|row| {
                            let entry = catalog.entry(*picker.results.get(row)?)?;
                            Some(rows::symbol_row(
                                row,
                                entry,
                                row == picker.highlighted,
                                active == Some(entry.id),
                                watchlists.is_favorite(&entry.name),
                                cx,
                            ))
                        })
                        .collect()
                }),
            )
            .track_scroll(&scroll)
            .flex_1()
            .into_any_element()
        };

        let panel = div()
            .key_context("SymbolPicker")
            .on_action(cx.listener(|this, _: &PickerUp, _window, cx| this.move_highlight(-1, cx)))
            .on_action(cx.listener(|this, _: &PickerDown, _window, cx| this.move_highlight(1, cx)))
            .on_action(
                cx.listener(|this, _: &PickerPageUp, _window, cx| this.move_highlight(-PAGE, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PickerPageDown, _window, cx| this.move_highlight(PAGE, cx)),
            )
            .on_action(cx.listener(|this, _: &PickerConfirm, window, cx| {
                // Enter in the list field saves the list, not the symbol.
                if this
                    .picker
                    .as_ref()
                    .is_some_and(|picker| picker.list_editor.is_some())
                {
                    this.commit_list_editor(window, cx);
                    return;
                }
                let row = this.picker.as_ref().map(|p| p.highlighted).unwrap_or(0);
                this.choose(row, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .w(px(PICKER_WIDTH))
            .max_w(relative(0.96))
            .h(px(height))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_xl()
            .shadow_lg()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .child(icon::tinted(IconName::Search, 18., theme::muted_fg()))
                    .child(div().flex_1().child(input))
                    .child(footer::key_cap("esc")),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .px_4()
                    .pb_2()
                    .children(chips),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .pb_3()
                    .children(list_chips),
            )
            .child(crate::ui::kit::layout::rule_h())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(div().flex_1().min_w_0().flex().flex_col().child(list))
                    .child(sheet),
            )
            .child(footer::footer(count, total));

        let overlay = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .py(px(crate::ui::kit::tokens::space::xl()))
            .bg(theme::veil())
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| this.close_overlays(window, cx)),
            )
            .child(anim::enter(panel, ("symbol-picker", self.picker_opens), 0));
        Some(overlay.into_any_element())
    }
}

impl Dashboard {
    /// Asks the broker for the details of the symbol under the highlight, once the highlight has
    /// rested on it for a moment: moving quickly through the list must not send a request per row.
    /// A symbol already known is not asked again; one that failed is asked again the next time it
    /// is highlighted.
    fn schedule_details(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.requested == id {
            return;
        }
        picker.requested = id;
        let Some(id) = id else {
            return;
        };
        if matches!(self.details.get(&id), Some(Detail::Ready(_))) {
            return;
        }
        self.details.insert(id, Detail::Loading);

        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(140))
                .await;
            let still_wanted = this
                .update(cx, |this, _cx| {
                    this.picker
                        .as_ref()
                        .is_some_and(|picker| picker.requested == Some(id))
                })
                .unwrap_or(false);
            if !still_wanted {
                let _ = this.update(cx, |this, _cx| {
                    if matches!(this.details.get(&id), Some(Detail::Loading)) {
                        this.details.remove(&id);
                    }
                });
                return;
            }

            // Follow the highlighted symbol's price while the picker shows it (the charts
            // follow their own).
            let _ = this.update(cx, |this, cx| {
                let shown = this.multi.read(cx).symbol_ids(cx);
                if shown.contains(&id) {
                    this.drop_peek();
                } else {
                    this.peek_at(id);
                }
            });

            let fetched = runtime::spawn(async move {
                let client = session.client().ok_or(ApiError::Closed)?;
                let account = client.account(session.account_id());
                let market = account.market();
                let details = market.symbol_details(&[id]).await?;
                details
                    .into_iter()
                    .find(|symbol| symbol.symbol_id == id)
                    .ok_or_else(|| ApiError::Protocol("the broker sent no details".into()))
            })
            .await;
            let _ = this.update(cx, |this, cx| {
                let detail = match fetched {
                    Ok(Ok(symbol)) => Detail::Ready(symbol),
                    Ok(Err(error)) => {
                        tracing::warn!(%error, symbol_id = id, "could not load a symbol's details");
                        Detail::Failed(error.to_string().into())
                    }
                    Err(_) => Detail::Failed("the background runtime stopped".into()),
                };
                this.details.insert(id, detail);
                cx.notify();
            });
        })
        .detach();
    }
}
