//! Menus that open on a right click or under a button: a card of entries with icons, hints and
//! check marks, in the app's look.
//!
//! `gpui-component` has a popup menu, but its entries never ask for the pointing hand and the
//! hover color is the theme's own, so they read differently from every other menu of the app. This
//! one gives an enabled entry the pointing hand and the same hover as the other menus, and a
//! disabled entry the plain arrow.
//!
//! A [`Menu`] holds whether the card is open (and where a right click opened it). The owner builds
//! the entries again on every frame the card is open, so they always show the live state:
//!
//! ```ignore
//! let menu = Menu::new("chart-menu", window, cx);
//! div()
//!     .on_mouse_down(MouseButton::Right, { let m = menu.clone(); move |e, _, cx| m.open(Some(e.position), cx) })
//!     .children(menu.popup(entries, Placement::Cursor, cx))
//! ```
//!
//! While it is open, the arrow keys move over the entries, Enter runs the chosen one, and Escape
//! or a click outside closes it.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ElementId, Entity, KeyDownEvent, MouseButton, Pixels, Point, SharedString,
    Window, anchored, canvas, deferred, div, px,
};
use gpui_kit::assets::IconName;

use super::connection::ui;
use super::{theme, widgets};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Above the dialogs, which can hold a menu too.
const PRIORITY: usize = 100;

/// How long after a click outside closed the card a click on its button does not open it again:
/// that click is the one that closed it.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// What a card holds.
#[derive(Clone)]
pub enum Item {
    Entry(Entry),
    Separator,
    /// A small heading that does nothing.
    Title(SharedString),
}

impl From<Entry> for Item {
    fn from(entry: Entry) -> Self {
        Self::Entry(entry)
    }
}

/// A line of a card that can be picked.
#[derive(Clone)]
pub struct Entry {
    label: SharedString,
    icon: Option<IconName>,
    hint: Option<SharedString>,
    checked: bool,
    disabled: bool,
    danger: bool,
    on_click: Option<Handler>,
}

impl Entry {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            hint: None,
            checked: false,
            disabled: false,
            danger: false,
            on_click: None,
        }
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// A word on the right, muted: a shortcut or a value.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    #[must_use]
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Written in the color of a destructive action.
    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    #[must_use]
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

/// Where a card shows.
#[derive(Debug, Clone, Copy)]
pub enum Placement {
    /// At the pointer of the right click that opened it.
    Cursor,
    /// Under its button, this far below the top of it.
    Below(f32),
}

#[derive(Default)]
struct State {
    open: bool,
    at: Option<Point<Pixels>>,
    /// The entry the keyboard is on.
    selected: Option<usize>,
    /// When a click outside last closed the card.
    dismissed: Option<Instant>,
}

/// Whether a card is open, and where. Cheap to clone: it is a handle.
#[derive(Clone)]
pub struct Menu {
    id: ElementId,
    state: Entity<State>,
}

impl Menu {
    /// The menu of `id`, which lives as long as the element that asks for it keeps rendering.
    pub fn new(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id: ElementId = id.into();
        let state = window.use_keyed_state(id.clone(), cx, |_, _| State::default());
        Self { id, state }
    }

    pub fn is_open(&self, cx: &App) -> bool {
        self.state.read(cx).open
    }

    /// Where the pointer was when a right click opened the card, in window coordinates.
    pub fn position(&self, cx: &App) -> Option<Point<Pixels>> {
        self.state.read(cx).at
    }

    pub fn open(&self, at: Option<Point<Pixels>>, cx: &mut App) {
        self.state.update(cx, |state, cx| {
            *state = State {
                open: true,
                at,
                selected: None,
                dismissed: None,
            };
            cx.notify();
        });
    }

    pub fn close(&self, cx: &mut App) {
        if !self.is_open(cx) {
            return;
        }
        self.state.update(cx, |state, cx| {
            state.open = false;
            state.selected = None;
            cx.notify();
        });
    }

    /// Opens the card, or closes it when it is open.
    pub fn toggle(&self, cx: &mut App) {
        let (open, dismissed) = {
            let state = self.state.read(cx);
            (state.open, state.dismissed)
        };
        if open {
            self.close(cx);
        } else if !dismissed.is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
            self.open(None, cx);
        }
    }

    /// A click outside the card closes it.
    fn dismiss(&self, cx: &mut App) {
        if !self.is_open(cx) {
            return;
        }
        self.state.update(cx, |state, cx| {
            state.open = false;
            state.selected = None;
            state.dismissed = Some(Instant::now());
            cx.notify();
        });
    }

    /// The card, when it is open. Add it as a child of the element that opens it.
    pub fn popup(
        &self,
        items: Vec<Item>,
        placement: Placement,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let (open, at, selected) = {
            let state = self.state.read(cx);
            (state.open, state.at, state.selected)
        };
        if !open || items.is_empty() {
            return None;
        }
        let card = self.card(&items, selected, placement);
        let items = Rc::new(items);
        let keys = {
            let (menu, items) = (self.clone(), items.clone());
            canvas(
                |_, _, _| (),
                move |_, (), window, _| {
                    let (menu, items) = (menu.clone(), items.clone());
                    window.on_key_event(move |event: &KeyDownEvent, phase, window, cx| {
                        if phase != gpui::DispatchPhase::Capture || !menu.is_open(cx) {
                            return;
                        }
                        if menu.on_key(&event.keystroke.key, &items, window, cx) {
                            cx.stop_propagation();
                        }
                    });
                },
            )
            .absolute()
            .size_0()
        };
        let placed = match (placement, at) {
            // A deferred element is drawn from where its parent is, so a position in window
            // coordinates goes in a box as big as the window, which starts at its corner.
            (Placement::Cursor, Some(at)) => {
                let size = window.viewport_size();
                anchored().child(
                    div().w(size.width).h(size.height).child(
                        anchored()
                            .position(at)
                            .snap_to_window_with_margin(px(8.))
                            .child(card),
                    ),
                )
            }
            (Placement::Below(offset), _) => anchored()
                .snap_to_window_with_margin(px(8.))
                .child(div().pt(px(offset)).child(card)),
            (Placement::Cursor, None) => anchored().snap_to_window_with_margin(px(8.)).child(card),
        };
        Some(
            deferred(div().absolute().size_0().child(keys).child(placed))
                .with_priority(PRIORITY)
                .into_any_element(),
        )
    }

    /// Handles a key while the card is open. Returns whether it was one the card uses.
    fn on_key(&self, key: &str, items: &[Item], window: &mut Window, cx: &mut App) -> bool {
        match key {
            "escape" => {
                self.close(cx);
                true
            }
            "down" | "up" => {
                let from = self.state.read(cx).selected;
                let next = step(items, from, key == "down");
                self.state.update(cx, |state, cx| {
                    state.selected = next;
                    cx.notify();
                });
                true
            }
            "enter" => {
                let chosen = self
                    .state
                    .read(cx)
                    .selected
                    .and_then(|i| match items.get(i) {
                        Some(Item::Entry(entry)) if !entry.disabled => Some(entry.clone()),
                        _ => None,
                    });
                if let Some(entry) = chosen {
                    self.close(cx);
                    if let Some(action) = &entry.on_click {
                        action(window, cx);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn card(&self, items: &[Item], selected: Option<usize>, placement: Placement) -> AnyElement {
        let min_w = match placement {
            Placement::Cursor => 230.,
            Placement::Below(_) => 190.,
        };
        let close = self.clone();
        let mut card = div()
            .id(widgets::child_id(&self.id, usize::MAX))
            .min_w(px(min_w))
            .max_h(px(520.))
            .overflow_y_scroll()
            .p_1()
            .flex()
            .flex_col()
            .gap_0p5()
            .rounded_lg()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border_subtle())
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(move |_, _, cx| close.dismiss(cx));
        for (index, item) in items.iter().enumerate() {
            card = card.child(match item {
                Item::Separator => div()
                    .my_0p5()
                    .h(px(1.))
                    .bg(theme::border_hairline())
                    .into_any_element(),
                Item::Title(text) => div()
                    .px_2()
                    .pt_1p5()
                    .pb_0p5()
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::muted_fg())
                    .child(text.to_uppercase())
                    .into_any_element(),
                Item::Entry(entry) => self.entry(index, entry, selected == Some(index)),
            });
        }
        card.into_any_element()
    }

    fn entry(&self, index: usize, entry: &Entry, keyboard: bool) -> AnyElement {
        let enabled = !entry.disabled;
        let tone = if !enabled {
            theme::muted_fg()
        } else if entry.danger {
            theme::destructive()
        } else {
            theme::fg()
        };
        let (menu, action) = (self.clone(), entry.on_click.clone());
        div()
            .id(widgets::child_id(&self.id, index))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_2()
            .rounded_md()
            .text_size(px(12.))
            .text_color(tone)
            .when(!enabled, |el| el.cursor_default().opacity(0.55))
            .when(enabled, |el| {
                el.cursor_pointer()
                    .hover(|s| s.bg(theme::surface_hover()))
                    .when(keyboard, |el| el.bg(theme::surface_hover()))
                    // The press belongs to the entry, not to what is under the card.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, window, cx| {
                        menu.close(cx);
                        if let Some(action) = &action {
                            action(window, cx);
                        }
                    })
            })
            .children(entry.icon.map(|icon| ui::icon_colored(icon, 15., tone)))
            .child(div().flex_1().min_w_0().child(entry.label.clone()))
            .children(entry.hint.clone().map(|hint| {
                div()
                    .pl_3()
                    .text_size(px(11.))
                    .text_color(theme::muted_fg())
                    .child(hint)
            }))
            .children(
                entry
                    .checked
                    .then(|| ui::icon_colored(IconName::Check, 13., theme::accent())),
            )
            .into_any_element()
    }
}

/// The next entry that can be picked, after `from` going forward or back, wrapping round.
fn step(items: &[Item], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = items.len();
    let mut at = from;
    for _ in 0..count {
        let next = match (at, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(i), true) => (i + 1) % count,
            (Some(i), false) => (i + count - 1) % count,
        };
        at = Some(next);
        if matches!(&items[next], Item::Entry(entry) if !entry.disabled) {
            return at;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Item> {
        vec![
            Item::Title("Group".into()),
            Entry::new("a").into(),
            Item::Separator,
            Entry::new("b").disabled(true).into(),
            Entry::new("c").into(),
        ]
    }

    #[test]
    fn the_keyboard_skips_what_cannot_be_picked() {
        let items = items();
        assert_eq!(step(&items, None, true), Some(1));
        assert_eq!(step(&items, Some(1), true), Some(4));
        assert_eq!(step(&items, Some(4), true), Some(1), "wraps round");
        assert_eq!(step(&items, Some(1), false), Some(4));
        assert_eq!(step(&items, None, false), Some(4));
    }

    #[test]
    fn a_card_with_nothing_to_pick_stops_nowhere() {
        let items = vec![Item::Separator, Entry::new("x").disabled(true).into()];
        assert_eq!(step(&items, None, true), None);
    }
}
