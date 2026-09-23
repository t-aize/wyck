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

use std::ops::Range;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    Context, Entity, Focusable, FontWeight, MouseButton, MouseMoveEvent, ScrollStrategy, Stateful,
    Subscription, UniformListScrollHandle, Window, div, px, rgba, uniform_list,
};
use gpui_kit::assets::IconName;

use super::catalog::{Class, Entry};
use super::details::{self, Detail};
use super::marks;
use super::{Dashboard, Load, PickerConfirm, PickerDown, PickerPageDown, PickerPageUp, PickerUp};
use crate::app::connection::ui;
use crate::app::text_input::TextInput;
use crate::app::{anim, runtime, theme};
use wyck::openapi::OpenApiError;

const ROW_HEIGHT: f32 = 52.;
/// How many rows a page key moves.
const PAGE: isize = 8;

pub(super) struct Picker {
    input: Entity<TextInput>,
    /// Re-renders the dashboard when the search text changes.
    _observe: Subscription,
    class: Option<Class>,
    highlighted: usize,
    /// The catalog indices matching the current search, best first.
    results: Vec<usize>,
    /// The search the results were computed for.
    computed_for: Option<(String, Option<Class>)>,
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
        self.menu_open = false;
        let input = cx.new(|cx| TextInput::new(cx, "Search symbols, e.g. EURUSD or gold"));
        let observe = cx.observe(&input, |_this, _input, cx| cx.notify());
        window.focus(&input.focus_handle(cx), cx);
        self.picker_opens += 1;
        self.picker = Some(Picker {
            input,
            _observe: observe,
            class: None,
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
        self.picker = None;
        self.menu_open = false;
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let Load::Ready(catalog) = &self.catalog else {
            return None;
        };
        let catalog = catalog.clone();
        let picker = self.picker.as_mut()?;

        let text = picker.input.read(cx).text().to_owned();
        let search = (text.clone(), picker.class);
        if picker.computed_for.as_ref() != Some(&search) {
            picker.results = catalog.query(&text, picker.class);
            picker.highlighted = 0;
            picker.computed_for = Some(search);
            picker.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        let input = picker.input.clone();
        let scroll = picker.scroll.clone();
        let selected_class = picker.class;
        let count = picker.results.len();
        let highlighted: Option<Entry> = picker
            .results
            .get(picker.highlighted)
            .and_then(|index| catalog.entry(*index))
            .cloned();
        let highlighted_id = highlighted.as_ref().map(|entry| entry.id);
        self.schedule_details(highlighted_id, cx);
        let live = highlighted_id
            .filter(|id| self.active.as_ref().map(|a| a.entry.id) == Some(*id))
            .and_then(|_| self.price_text());
        let sheet = details::render_details(
            highlighted.as_ref(),
            highlighted_id.and_then(|id| self.details.get(&id)),
            live,
            self.picker_opens,
        );

        let chips = std::iter::once(None)
            .chain(catalog.classes().into_iter().map(Some))
            .map(|class| class_chip(class, class == selected_class, cx))
            .collect::<Vec<_>>();

        let list = if count == 0 {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(theme::muted_fg())
                .child("No symbol matches your search")
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
                    range
                        .filter_map(|row| {
                            let entry = catalog.entry(*picker.results.get(row)?)?;
                            Some(symbol_row(
                                row,
                                entry,
                                row == picker.highlighted,
                                active == Some(entry.id),
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
                let row = this.picker.as_ref().map(|p| p.highlighted).unwrap_or(0);
                this.choose(row, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .w(px(940.))
            .h(px(560.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded_xl()
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
                    .child(ui::icon_colored(IconName::Search, 18., theme::muted_fg()))
                    .child(div().flex_1().child(input))
                    .child(key_hint("Esc")),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_2()
                    .px_4()
                    .pb_3()
                    .children(chips),
            )
            .child(div().flex_none().h(px(1.)).bg(theme::border_hairline()))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(div().flex_1().min_w_0().flex().flex_col().child(list))
                    .child(sheet),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_2p5()
                    .border_t_1()
                    .border_color(theme::border_hairline())
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(format!(
                        "{count} symbol{}",
                        if count == 1 { "" } else { "s" }
                    ))
                    .child("Up/Down to move, Enter to choose, Esc to close"),
            );

        let overlay = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(72.))
            .bg(rgba(0x000000a6))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, window, cx| this.close_overlays(window, cx)),
            )
            .child(anim::enter(panel, ("symbol-picker", self.picker_opens), 0));
        Some(overlay.into_any_element())
    }
}

/// A filter chip: one asset class, or `None` for all of them.
fn class_chip(
    class: Option<Class>,
    selected: bool,
    cx: &mut Context<Dashboard>,
) -> Stateful<gpui::Div> {
    let label = class.map_or("All", Class::label);
    div()
        .id(("class-chip", class.map_or(0, |c| c as u64 + 1)))
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
        .on_click(cx.listener(move |this, _event, _window, cx| {
            if let Some(picker) = this.picker.as_mut() {
                picker.class = class;
                cx.notify();
            }
        }))
        .child(label)
}

fn symbol_row(
    row: usize,
    entry: &super::catalog::Entry,
    highlighted: bool,
    active: bool,
    cx: &mut Context<Dashboard>,
) -> Stateful<gpui::Div> {
    div()
        .id(("symbol-row", row as u64))
        .w_full()
        .h(px(ROW_HEIGHT))
        .px_4()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .cursor_pointer()
        .when(highlighted, |el| el.bg(theme::surface_hover()))
        .active(|style| style.bg(theme::surface_pressed()))
        .on_mouse_move(
            cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                if let Some(picker) = this.picker.as_mut()
                    && picker.highlighted != row
                {
                    picker.highlighted = row;
                    cx.notify();
                }
            }),
        )
        .on_click(cx.listener(move |this, _event, window, cx| this.choose(row, window, cx)))
        .child(marks::render(
            &entry.icon,
            38.,
            if highlighted {
                theme::surface_hover()
            } else {
                theme::surface()
            },
        ))
        .child(
            div()
                .w(px(120.))
                .flex_none()
                .truncate()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::fg())
                .child(entry.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(entry.description.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_size(px(11.))
                .text_color(theme::muted_fg())
                .child(entry.class.label()),
        )
        .child(
            div()
                .w(px(16.))
                .flex_none()
                .children(active.then(|| ui::icon_colored(IconName::Check, 16., theme::accent()))),
        )
}

fn key_hint(label: &'static str) -> impl IntoElement {
    div()
        .flex_none()
        .px_2()
        .py_0p5()
        .rounded_md()
        .border_1()
        .border_color(theme::border_subtle())
        .text_size(px(11.))
        .text_color(theme::muted_fg())
        .child(label)
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

            let fetched = runtime::spawn(async move {
                let client = session.client().ok_or(OpenApiError::Closed)?;
                let account = client.account(session.account_id());
                let details = account.market().symbol_details(&[id]).await?;
                details
                    .into_iter()
                    .find(|symbol| symbol.symbol_id == id)
                    .ok_or_else(|| OpenApiError::Protocol("the broker sent no details".into()))
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
