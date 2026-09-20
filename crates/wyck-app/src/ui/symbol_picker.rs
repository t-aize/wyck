//! The symbol picker: a palette in the middle of the window that lists every symbol the account
//! offers, with its icon and everything the broker says about it, and switches the application to
//! the one chosen.
//!
//! It opens from the symbol block of the dashboard header, or with Ctrl+K (Cmd+K on macOS). The
//! list is the account's own (the engine reads it once per connection, see
//! [`wyck_engine::EngineHandle::symbol_catalog`]); [`crate::symbols::Catalog`] classifies, ranks and
//! groups it. Choosing a symbol makes it the one the application is on
//! ([`AppController::set_symbol`](crate::controller::AppController::set_symbol)): the header, the
//! quotes the engine follows and, later, the chart all follow it, and it is remembered for the next
//! start.
//!
//! # Layout
//!
//! A search field on top, filters by asset class, then the list on the left and the details of the
//! highlighted symbol on the right, then a line of key hints. The list is virtualized, so an account
//! with thousands of shares scrolls as smoothly as one with a hundred forex pairs.
//!
//! # Keys
//!
//! Up and Down (or Ctrl+N and Ctrl+P) move the highlight, Page Up and Page Down move it by a page,
//! Enter chooses, Tab and Shift+Tab step through the filters, Escape closes. They are read by a
//! keystroke interceptor while the picker is open, ahead of the search field, which would otherwise
//! keep the arrow keys for itself. The mouse works the same: hovering highlights, clicking chooses,
//! clicking outside closes.
//!
//! # No blur
//!
//! The window behind the palette is dimmed, not blurred: GPUI cannot blur what is drawn behind an
//! element (it only blurs shadows and, on some platforms, the window's own background).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::base::{Presence, Transition, TransitionId, transition};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, Context, Div, ElementId, Entity, FontWeight,
    Keystroke, MouseButton, ScrollStrategy, SharedString, Stateful, Subscription, Task,
    UniformListScrollHandle, WeakEntity, Window, div, uniform_list,
};
use wyck_engine::EngineError;
use wyck_engine::domain::{Instrument, Quote};

use super::app_view::AppView;
use super::motion::{self, Hover, blend};
use super::theme::{self, sz};
use super::widgets::{Glyph, glyph, panel, pill, primary_button, row, symbol_icon};
use crate::flow::Screen;
use crate::messages::describe_error;
use crate::startup::{open_user_config, remember_symbol};
use crate::symbols::{AssetClass, Catalog, DetailsView, Row, describe};

/// The height of a line of the list, in design pixels.
const ROW_HEIGHT: f32 = 50.;
/// How long the keyboard has the highlight after a key: the pointer resting over the list must not
/// take it back when the list scrolls under it.
const KEYBOARD_GRACE: Duration = Duration::from_millis(400);
/// How long the highlight rests on a symbol before its details are fetched.
const DETAILS_DELAY: Duration = Duration::from_millis(140);
/// How many lines a page key moves.
const PAGE: isize = 8;

/// Where the list of symbols is.
pub(super) enum Load {
    /// Being read from the broker.
    Loading,
    /// Ready.
    Ready(Arc<Catalog>),
    /// The broker could not give it: what to tell the user.
    Failed(String),
}

/// What is known of one symbol beyond the list: its instrument details and a quote.
#[derive(Clone, Default)]
struct Fetched {
    instrument: Option<Instrument>,
    quote: Option<Quote>,
    loading: bool,
    error: Option<String>,
    /// A quote was asked for and the answer has come back, with or without one.
    quote_checked: bool,
    /// Why a quote could not be read.
    quote_error: Option<String>,
}

/// The open picker.
pub(super) struct Picker {
    /// Unique per opening: scopes the animation state.
    key: usize,
    /// It has been asked to close and is playing its exit.
    closing: bool,
    search: Entity<InputState>,
    load: Load,
    /// The class filter, `None` for all.
    class: Option<AssetClass>,
    /// The catalog indices that match the search and the filter, best first.
    matches: Vec<usize>,
    /// The lines of the list.
    rows: Arc<Vec<Row>>,
    /// The highlighted symbol, as a catalog index.
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    fetched: HashMap<String, Fetched>,
    fetch_task: Option<Task<()>>,
    /// When a key last moved the highlight.
    keyboard_at: Option<Instant>,
    /// The pointer has moved since the picker opened. Until then a pointer resting over the list
    /// must not take the highlight from the current symbol.
    pointer_moved: bool,
    _subscription: Subscription,
}

impl AppView {
    // ---- opening, closing ----

    /// Opens the picker on the dashboard, with the search field focused and the current symbol
    /// highlighted.
    pub(super) fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.flow.screen(), Screen::Dashboard)
            || self.picker.as_ref().is_some_and(|p| !p.closing)
        {
            return;
        }
        self.picker_opens += 1;
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Search a symbol, a name, a country...")
        });
        let subscription =
            cx.subscribe_in(&search, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.picker_refresh(cx);
                }
            });
        let load = match &self.catalog {
            Some(catalog) => Load::Ready(Arc::clone(catalog)),
            None => Load::Loading,
        };
        let ready = matches!(load, Load::Ready(_));
        self.picker = Some(Picker {
            key: self.picker_opens,
            closing: false,
            search: search.clone(),
            load,
            class: None,
            matches: Vec::new(),
            rows: Arc::new(Vec::new()),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            fetched: HashMap::new(),
            fetch_task: None,
            keyboard_at: None,
            pointer_moved: false,
            _subscription: subscription,
        });
        search.update(cx, |state, cx| state.focus(window, cx));
        if ready {
            self.picker_refresh(cx);
        } else {
            self.load_catalog(cx);
        }
        cx.notify();
    }

    /// Starts closing the picker: it fades out, then goes.
    pub(super) fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.as_mut()
            && !picker.closing
        {
            picker.closing = true;
            picker.fetch_task = None;
            self.settle_focus(window, cx);
            cx.notify();
        }
    }

    /// Chooses the symbol at `index` of the catalog, and closes the picker.
    pub(super) fn pick_symbol(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .catalog
            .as_ref()
            .and_then(|c| c.entry(index))
            .map(|e| e.info.symbol.clone())
        else {
            return;
        };
        if self.shell.controller.set_symbol(&name) {
            tracing::info!(symbol = %name, "symbol chosen");
            // The price of another symbol is not a move of this one.
            self.reset_price_tracking();
            let saved = name.clone();
            cx.spawn(async move |_, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { remember_symbol(&saved, open_user_config) })
                    .await;
                if let Err(error) = result {
                    tracing::warn!(%error, "the symbol could not be remembered");
                }
            })
            .detach();
        }
        self.close_picker(window, cx);
    }

    // ---- the catalog ----

    /// Reads the list of symbols from the engine, in the background.
    pub(super) fn load_catalog(&mut self, cx: &mut Context<Self>) {
        if let Some(picker) = self.picker.as_mut() {
            picker.load = Load::Loading;
        }
        let handle = self.shell.controller.handle();
        cx.spawn(async move |this, cx| {
            let result = match handle.symbol_catalog().await {
                Ok(list) => Ok(cx
                    .background_executor()
                    .spawn(async move { Catalog::new(&list) })
                    .await),
                Err(error) => Err(error),
            };
            this.update(cx, |this, cx| match result {
                Ok(catalog) => this.catalog_loaded(Arc::new(catalog), cx),
                Err(error) => this.catalog_failed(&error, cx),
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn catalog_loaded(&mut self, catalog: Arc<Catalog>, cx: &mut Context<Self>) {
        self.catalog = Some(Arc::clone(&catalog));
        if let Some(picker) = self.picker.as_mut() {
            picker.load = Load::Ready(catalog);
            self.picker_refresh(cx);
        }
        cx.notify();
    }

    fn catalog_failed(&mut self, error: &EngineError, cx: &mut Context<Self>) {
        let notice = describe_error(error);
        tracing::warn!(%error, "the symbol list could not be read");
        if let Some(picker) = self.picker.as_mut() {
            picker.load = Load::Failed(notice.title);
        }
        cx.notify();
    }

    // ---- searching and choosing ----

    /// Recomputes the list after the text or the filter changed, and puts the highlight on the best
    /// match while searching, on the current symbol otherwise.
    fn picker_refresh(&mut self, cx: &mut Context<Self>) {
        let current = self.shell.controller.symbol();
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let Load::Ready(catalog) = &picker.load else {
            return;
        };
        let text = picker.search.read(cx).value();
        let matches = catalog.query(&text, picker.class);
        let grouped = text.trim().is_empty() && picker.class.is_none();
        let rows = catalog.rows(&matches, grouped);
        // While searching, the best match is highlighted; with nothing typed, the current symbol.
        let searching = !text.trim().is_empty();
        let selected = (!searching)
            .then(|| catalog.find(&current).filter(|c| matches.contains(c)))
            .flatten()
            .or_else(|| matches.first().copied());
        picker.matches = matches;
        picker.rows = Arc::new(rows);
        picker.selected = selected;
        if let Some(ix) = selected.and_then(|s| row_of(&picker.rows, s)) {
            picker.scroll.scroll_to_item(
                ix,
                if searching {
                    ScrollStrategy::Top
                } else {
                    ScrollStrategy::Center
                },
            );
        }
        self.request_details(cx);
        cx.notify();
    }

    /// Moves the highlight by `delta` symbols, wrapping around, skipping group headings.
    fn picker_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let symbols: Vec<usize> = picker
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Symbol(i) => Some(*i),
                Row::Header(..) => None,
            })
            .collect();
        if symbols.is_empty() {
            return;
        }
        let at = picker
            .selected
            .and_then(|s| symbols.iter().position(|&i| i == s));
        let last = symbols.len() as isize - 1;
        let next = match at {
            None if delta >= 0 => 0,
            None => last,
            Some(at) => {
                let moved = at as isize + delta;
                // A step wraps around the ends; a page stops at them.
                if delta.abs() == 1 {
                    moved.rem_euclid(last + 1)
                } else {
                    moved.clamp(0, last)
                }
            }
        };
        let chosen = symbols[next as usize];
        picker.keyboard_at = Some(Instant::now());
        self.picker_highlight(chosen, true, cx);
    }

    /// Highlights a symbol, scrolling to it when the keyboard did it.
    fn picker_highlight(&mut self, index: usize, scroll: bool, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.selected == Some(index) {
            return;
        }
        picker.selected = Some(index);
        if scroll && let Some(ix) = row_of(&picker.rows, index) {
            picker.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
        self.request_details(cx);
        cx.notify();
    }

    /// The pointer moved over the palette: from now on hovering a symbol highlights it.
    fn picker_pointer_moved(&mut self) {
        if let Some(picker) = self.picker.as_mut() {
            picker.pointer_moved = true;
        }
    }

    /// The pointer went over a symbol.
    fn picker_hover(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.picker.as_ref().is_some_and(|p| p.pointer_moved) {
            return;
        }
        let held = self
            .picker
            .as_ref()
            .and_then(|p| p.keyboard_at)
            .is_some_and(|at| at.elapsed() < KEYBOARD_GRACE);
        if !held {
            self.picker_highlight(index, false, cx);
        }
    }

    /// Chooses another class filter and shows the list again.
    fn picker_set_class(
        &mut self,
        class: Option<AssetClass>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.class == class {
            return;
        }
        picker.class = class;
        picker
            .search
            .update(cx, |state, cx| state.focus(window, cx));
        self.picker_refresh(cx);
    }

    /// Steps to the next (or, backwards, the previous) filter: All, then each class present.
    fn picker_cycle_class(&mut self, backwards: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let Load::Ready(catalog) = &picker.load else {
            return;
        };
        let mut steps: Vec<Option<AssetClass>> = vec![None];
        steps.extend(catalog.counts().into_iter().map(|(class, _)| Some(class)));
        let at = steps.iter().position(|c| *c == picker.class).unwrap_or(0);
        let next = if backwards {
            (at + steps.len() - 1) % steps.len()
        } else {
            (at + 1) % steps.len()
        };
        self.picker_set_class(steps[next], window, cx);
    }

    // ---- details of the highlighted symbol ----

    /// Fetches what the broker says about the highlighted symbol (its instrument once, a quote every
    /// time), after a short rest so that scrolling through the list does not ask for each symbol.
    fn request_details(&mut self, cx: &mut Context<Self>) {
        let handle = self.shell.controller.handle();
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let Load::Ready(catalog) = &picker.load else {
            return;
        };
        let Some(name) = picker
            .selected
            .and_then(|s| catalog.entry(s))
            .map(|e| e.info.symbol.clone())
        else {
            picker.fetch_task = None;
            return;
        };
        let known = picker.fetched.entry(name.clone()).or_default();
        let have_instrument = known.instrument.is_some();
        known.loading = !have_instrument;
        known.error = None;
        picker.fetch_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DETAILS_DELAY).await;
            if !have_instrument {
                let instrument = handle.instrument(&name).await;
                let symbol = name.clone();
                this.update(cx, |this, cx| {
                    this.store_instrument(&symbol, instrument, cx)
                })
                .ok();
            }
            let quote = handle.quote(&name).await;
            this.update(cx, |this, cx| this.store_quote(&name, quote, cx))
                .ok();
        }));
    }

    fn store_instrument(
        &mut self,
        symbol: &str,
        result: Result<Instrument, EngineError>,
        cx: &mut Context<Self>,
    ) {
        if let Some(picker) = self.picker.as_mut() {
            let entry = picker.fetched.entry(symbol.to_owned()).or_default();
            entry.loading = false;
            match result {
                Ok(instrument) => entry.instrument = Some(instrument),
                Err(error) => {
                    tracing::debug!(%error, symbol, "the details of a symbol could not be read");
                    entry.error = Some(describe_error(&error).title);
                }
            }
            cx.notify();
        }
    }

    fn store_quote(
        &mut self,
        symbol: &str,
        result: Result<Option<Quote>, EngineError>,
        cx: &mut Context<Self>,
    ) {
        if let Some(picker) = self.picker.as_mut() {
            let entry = picker.fetched.entry(symbol.to_owned()).or_default();
            match result {
                Ok(quote) => {
                    entry.quote = quote;
                    entry.quote_error = None;
                }
                Err(error) => {
                    tracing::debug!(%error, symbol, "a quote could not be read");
                    entry.quote_error = Some(describe_error(&error).title);
                }
            }
            entry.quote_checked = true;
            cx.notify();
        }
    }

    // ---- the keyboard ----

    /// Reads a key before anything else does. Returns whether the picker took it.
    pub(super) fn on_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = keystroke.key.as_str();
        let modifiers = &keystroke.modifiers;
        let open = self.picker.as_ref().is_some_and(|p| !p.closing);
        if !open {
            if modifiers.secondary()
                && key == "k"
                && matches!(self.flow.screen(), Screen::Dashboard)
            {
                self.open_picker(window, cx);
                return true;
            }
            return false;
        }
        let control = modifiers.control;
        match key {
            "escape" => self.close_picker(window, cx),
            "k" if modifiers.secondary() => self.close_picker(window, cx),
            "down" => self.picker_move(1, cx),
            "up" => self.picker_move(-1, cx),
            "n" if control => self.picker_move(1, cx),
            "p" if control => self.picker_move(-1, cx),
            "pagedown" => self.picker_move(PAGE, cx),
            "pageup" => self.picker_move(-PAGE, cx),
            "tab" => self.picker_cycle_class(modifiers.shift, window, cx),
            "enter" => {
                if let Some(index) = self.picker.as_ref().and_then(|p| p.selected) {
                    self.pick_symbol(index, window, cx);
                }
            }
            _ => return false,
        }
        true
    }

    // ---- drawing ----

    /// The picker over the window, or `None` when it is closed (or has finished closing).
    pub(super) fn picker_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let picker = self.picker.as_ref()?;
        let closing = picker.closing;
        let key = picker.key;
        let transition = if closing {
            motion::screen_out()
        } else {
            Transition::new(motion::SLOW).easing(motion::enter())
        };
        let sample = Presence::new(TransitionId::from((key, "picker")), !closing)
            .transition(transition)
            .sample(window, cx);
        if !sample.should_render() {
            self.picker = None;
            return None;
        }
        let progress = sample.progress;

        let picker = self.picker.as_ref()?;
        let view = cx.entity().downgrade();
        let current = self.shell.controller.symbol();
        let query = picker.search.read(cx).value().to_string();
        let selected = picker.selected;
        let catalog = match &picker.load {
            Load::Ready(catalog) => Some(Arc::clone(catalog)),
            _ => None,
        };
        let sheet = catalog
            .as_ref()
            .zip(selected)
            .and_then(|(catalog, index)| catalog.entry(index))
            .map(|entry| {
                let fetched = picker
                    .fetched
                    .get(&entry.info.symbol)
                    .cloned()
                    .unwrap_or_default();
                let details = describe(entry, fetched.instrument.as_ref(), fetched.quote.as_ref());
                (entry.icon.clone(), details, fetched)
            });

        let body = match &picker.load {
            Load::Loading => {
                centered_message(loading_line("Loading the symbols of this account..."))
            }
            Load::Failed(reason) => failed_list(reason.clone(), window, cx),
            Load::Ready(catalog) if catalog.is_empty() => {
                centered_message(text_line("This account offers no symbols."))
            }
            Load::Ready(_) if picker.rows.is_empty() => centered_message(text_line(format!(
                "No symbol matches \"{}\".",
                query.trim()
            ))),
            Load::Ready(catalog) => list(
                Arc::clone(catalog),
                Arc::clone(&picker.rows),
                selected,
                current.clone(),
                picker.scroll.clone(),
                view.clone(),
            ),
        };
        let search = picker.search.clone();
        let class = picker.class;
        let total = catalog.as_ref().map_or(0, |c| c.len());
        let shown = picker.matches.len();
        let counts = catalog.as_ref().map(|c| c.counts()).unwrap_or_default();

        let header = search_bar(&search);
        let chips = filter_chips(class, total, &counts, window, cx);
        let side = details_pane(sheet, &current, selected, window, cx);
        let footer = footer(shown, total);

        let panel = div()
            .id("symbol-picker")
            .relative()
            .top(sz((1.0 - progress) * 14.0))
            .opacity(progress)
            .w(sz(940.))
            .max_w(gpui_kit::relative(0.94))
            .h(sz(620.))
            .max_h(gpui_kit::relative(0.9))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(sz(14.))
            .bg(theme::card())
            .border_1()
            .border_color(theme::alpha(theme::fg(), 0.14))
            .shadow(vec![gpui_kit::BoxShadow {
                color: gpui_kit::hsla(0., 0., 0., 0.55),
                offset: gpui_kit::point(gpui_kit::px(0.), sz(24.)),
                blur_radius: sz(64.),
                spread_radius: sz(-12.),
                inset: false,
            }])
            // A click inside the palette is not a click on the dimmed window behind it.
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_move(cx.listener(|this, _, _, _| this.picker_pointer_moved()))
            .child(header)
            .child(chips)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(div().flex_1().min_w_0().flex().flex_col().child(body))
                    .child(side),
            )
            .child(footer);

        Some(
            div()
                .id("symbol-picker-overlay")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .occlude()
                .bg(theme::alpha(theme::bg(), 0.76 * progress))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_picker(window, cx)),
                )
                .child(panel)
                .into_any_element(),
        )
    }
}

/// The line of the list that shows the symbol at catalog index `index`.
fn row_of(rows: &[Row], index: usize) -> Option<usize> {
    rows.iter().position(|r| *r == Row::Symbol(index))
}

// ---- pieces ----

fn text_line(text: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(sz(13.))
        .text_color(theme::dim())
        .text_center()
        .child(text.into())
        .into_any_element()
}

fn loading_line(text: &'static str) -> AnyElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(sz(10.))
        .child(small_spinner(16.))
        .child(text_line(text))
        .into_any_element()
}

/// A small spinning arc.
fn small_spinner(size: f32) -> Div {
    div()
        .relative()
        .flex_none()
        .size(sz(size))
        .child(
            gpui_kit::svg()
                .path("wyck/ring.svg")
                .absolute()
                .size_full()
                .text_color(theme::muted()),
        )
        .child(
            gpui_kit::svg()
                .path("wyck/arc.svg")
                .absolute()
                .size_full()
                .text_color(theme::fg())
                .with_animation(
                    "picker-spin",
                    Animation::new(Duration::from_secs(1)).repeat(),
                    |el, delta| {
                        el.with_transformation(gpui_kit::Transformation::rotate(
                            gpui_kit::percentage(delta),
                        ))
                    },
                ),
        )
}

fn centered_message(content: AnyElement) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .p(sz(24.))
        .child(content)
        .into_any_element()
}

fn failed_list(reason: String, window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let retry = primary_button("picker-retry", "Try again", window, cx)
        .w(sz(160.))
        .on_click(cx.listener(|this, _, _, cx| this.load_catalog(cx)));
    centered_message(
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(sz(14.))
            .child(
                div()
                    .text_size(sz(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::fg())
                    .child("The list of symbols could not be read."),
            )
            .child(text_line(reason))
            .child(retry)
            .into_any_element(),
    )
}

/// A small key cap, for the hints and the search field.
fn key_cap(content: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .min_w(sz(20.))
        .h(sz(20.))
        .px(sz(5.))
        .rounded(sz(5.))
        .bg(theme::alpha(theme::fg(), 0.07))
        .border_1()
        .border_color(theme::border())
        .text_size(sz(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::dim())
        .child(content)
}

fn search_bar(search: &Entity<InputState>) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(sz(12.))
        .h(sz(58.))
        .px(sz(20.))
        .child(glyph(Glyph::Search, 18., theme::dim()))
        .child(
            div().flex_1().min_w_0().text_size(sz(16.)).child(
                Input::new(search)
                    .large()
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false),
            ),
        )
        .child(key_cap("esc"))
}

/// The filters: All, then each class the account has, with how many symbols it has.
fn filter_chips(
    selected: Option<AssetClass>,
    total: usize,
    counts: &[(AssetClass, usize)],
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    let mut chips: Vec<AnyElement> = Vec::with_capacity(counts.len() + 1);
    chips.push(chip("All", total, None, selected.is_none(), window, cx).into_any_element());
    for (class, count) in counts {
        chips.push(
            chip(
                class.label(),
                *count,
                Some(*class),
                selected == Some(*class),
                window,
                cx,
            )
            .into_any_element(),
        );
    }
    div()
        .id("picker-chips")
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap(sz(6.))
        .h(sz(46.))
        .px(sz(20.))
        .overflow_x_scroll()
        .children(chips)
}

fn chip(
    label: &'static str,
    count: usize,
    class: Option<AssetClass>,
    selected: bool,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Stateful<Div> {
    let id = SharedString::from(format!("picker-chip-{label}"));
    let hover = Hover::track(id.clone(), window, cx);
    let chosen = transition(
        TransitionId::from((id.clone(), "selected")),
        if selected { 1.0_f32 } else { 0.0 },
        motion::quick(),
        window,
        cx,
    );
    let amount = chosen.max(hover.amount * 0.6);
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(sz(6.))
        .h(sz(28.))
        .px(sz(12.))
        .rounded_full()
        .border_1()
        .border_color(blend(
            theme::border(),
            theme::alpha(theme::fg(), 0.22),
            chosen,
        ))
        .bg(blend(
            theme::alpha(theme::muted(), 0.0),
            theme::muted(),
            amount,
        ))
        .text_color(blend(theme::dim(), theme::fg(), amount))
        .text_size(sz(12.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .on_hover(hover.handler())
        .on_click(cx.listener(move |this, _, window, cx| this.picker_set_class(class, window, cx)))
        .child(label)
        .child(
            div()
                .font_features(theme::tabular())
                .text_size(sz(11.))
                .text_color(theme::dim())
                .child(count.to_string()),
        )
}

/// The virtualized list of symbols.
fn list(
    catalog: Arc<Catalog>,
    rows: Arc<Vec<Row>>,
    selected: Option<usize>,
    current: String,
    scroll: UniformListScrollHandle,
    view: WeakEntity<AppView>,
) -> AnyElement {
    let count = rows.len();
    uniform_list("picker-list", count, move |range, _window, _cx| {
        range
            .filter_map(|ix| rows.get(ix).map(|row| (ix, *row)))
            .map(|(ix, row)| match row {
                Row::Header(class, count) => group_heading(class, count).into_any_element(),
                Row::Symbol(index) => symbol_row(
                    &catalog,
                    index,
                    ix,
                    selected == Some(index),
                    &current,
                    view.clone(),
                )
                .into_any_element(),
            })
            .collect::<Vec<_>>()
    })
    .track_scroll(&scroll)
    .flex_1()
    .min_h_0()
    .into_any_element()
}

/// A line of the list for a symbol: the row, inset from the edges of the list.
fn symbol_row(
    catalog: &Catalog,
    index: usize,
    line: usize,
    highlighted: bool,
    current: &str,
    view: WeakEntity<AppView>,
) -> Div {
    div().w_full().px(sz(8.)).child(symbol_row_inner(
        catalog,
        index,
        line,
        highlighted,
        current,
        view,
    ))
}

fn group_heading(class: AssetClass, count: usize) -> Div {
    div()
        .h(sz(ROW_HEIGHT))
        .flex()
        .flex_row()
        .items_end()
        .gap(sz(8.))
        .px(sz(20.))
        .pb(sz(8.))
        .text_size(sz(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::dim())
        .child(class.label().to_uppercase())
        .child(
            div()
                .font_features(theme::tabular())
                .text_color(theme::alpha(theme::dim(), 0.7))
                .child(count.to_string()),
        )
}

fn symbol_row_inner(
    catalog: &Catalog,
    index: usize,
    line: usize,
    highlighted: bool,
    current: &str,
    view: WeakEntity<AppView>,
) -> Stateful<Div> {
    let Some(entry) = catalog.entry(index) else {
        return div().id(ElementId::from(("symbol-row", line)));
    };
    let info = &entry.info;
    let is_current = info.symbol.eq_ignore_ascii_case(current);
    let side = info
        .category
        .clone()
        .unwrap_or_else(|| entry.class.label().to_owned());
    let hover_view = view.clone();
    div()
        .id(ElementId::from(("symbol-row", line)))
        .w_full()
        .h(sz(ROW_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .gap(sz(12.))
        .px(sz(12.))
        .rounded(sz(9.))
        .cursor_pointer()
        .bg(if highlighted {
            theme::alpha(theme::fg(), 0.07)
        } else {
            theme::alpha(theme::fg(), 0.0)
        })
        .on_hover(move |hovered, _, cx| {
            if *hovered {
                hover_view
                    .update(cx, |this, cx| this.picker_hover(index, cx))
                    .ok();
            }
        })
        .on_click(move |_, window, cx| {
            view.update(cx, |this, cx| this.pick_symbol(index, window, cx))
                .ok();
        })
        .child(symbol_icon(&entry.icon, 30., theme::card()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(sz(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::fg())
                        .child(info.symbol.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(sz(11.5))
                        .text_color(theme::dim())
                        .child(info.description.clone().unwrap_or_default()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_none()
                .items_center()
                .gap(sz(10.))
                .child(
                    div()
                        .text_size(sz(11.))
                        .text_color(theme::dim())
                        .child(side),
                )
                .when(!info.enabled, |el| {
                    el.child(pill(
                        "Closed",
                        theme::red(),
                        theme::alpha(theme::red(), 0.14),
                    ))
                })
                .when(is_current, |el| {
                    el.child(glyph(Glyph::Check, 14., theme::green()))
                }),
        )
}

/// The right side: the symbol the highlight is on.
fn details_pane(
    sheet: Option<(crate::symbols::SymbolIcon, DetailsView, Fetched)>,
    current: &str,
    selected: Option<usize>,
    window: &mut Window,
    cx: &mut Context<AppView>,
) -> Div {
    let pane = div()
        .flex_none()
        .w(sz(330.))
        .flex()
        .flex_col()
        .border_l_1()
        .border_color(theme::border())
        .bg(theme::alpha(theme::bg(), 0.5));
    let Some((icon, details, fetched)) = sheet else {
        return pane
            .items_center()
            .justify_center()
            .child(text_line("Highlight a symbol to see its details."));
    };
    let is_current = details.symbol.eq_ignore_ascii_case(current);

    let mut badges = div().flex().flex_row().flex_wrap().gap(sz(6.)).mt(sz(12.));
    badges = badges.child(pill(details.class, theme::dim(), theme::muted()));
    if let Some(category) = &details.category {
        badges = badges.child(pill(category.clone(), theme::dim(), theme::muted()));
    }
    if is_current {
        badges = badges.child(pill(
            "Current",
            theme::green(),
            theme::alpha(theme::green(), 0.14),
        ));
    }
    if !details.enabled {
        badges = badges.child(pill(
            "Trading disabled",
            theme::red(),
            theme::alpha(theme::red(), 0.14),
        ));
    }

    let live: Option<Div> = (!details.live.is_empty()).then(|| {
        div()
            .flex()
            .flex_row()
            .gap(sz(8.))
            .mt(sz(16.))
            .children(details.live.iter().map(|d| {
                div()
                    .flex_1()
                    .min_w_0()
                    .px(sz(10.))
                    .py(sz(8.))
                    .rounded(sz(8.))
                    .bg(theme::alpha(theme::fg(), 0.05))
                    .child(
                        div()
                            .text_size(sz(10.5))
                            .text_color(theme::dim())
                            .child(d.label),
                    )
                    .child(
                        div()
                            .truncate()
                            .font_features(theme::tabular())
                            .text_size(sz(13.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::fg())
                            .child(d.value.clone()),
                    )
            }))
    });

    let last = details.rows.len().saturating_sub(1);
    let table = panel()
        .mt(sz(16.))
        .children(details.rows.iter().enumerate().map(|(i, d)| {
            row(
                None,
                d.label,
                div()
                    .font_features(theme::tabular())
                    .text_size(sz(12.))
                    .text_color(theme::fg())
                    .child(d.value.clone()),
                i != last,
            )
        }));

    // Said out loud: silence would read as a bug. The forex market is closed on weekends, and
    // some symbols have no quote outside their session.
    let no_quote: Option<Div> = (fetched.quote_checked && fetched.quote.is_none()).then(|| {
        div()
            .mt(sz(12.))
            .text_size(sz(11.5))
            .line_height(gpui_kit::relative(1.5))
            .text_color(theme::dim())
            .child(match &fetched.quote_error {
                Some(reason) => format!("No quote could be read: {reason}"),
                None => "No quote right now. The market may be closed for this symbol.".to_owned(),
            })
    });

    let status: Option<Div> = if let Some(error) = &fetched.error {
        Some(
            div()
                .mt(sz(12.))
                .text_size(sz(11.5))
                .text_color(theme::red_text())
                .child(format!("Could not load the details: {error}")),
        )
    } else if fetched.loading {
        Some(
            div()
                .mt(sz(12.))
                .flex()
                .flex_row()
                .items_center()
                .gap(sz(8.))
                .text_size(sz(11.5))
                .text_color(theme::dim())
                .child(small_spinner(12.))
                .child("Loading the details..."),
        )
    } else {
        None
    };

    let choose = selected.map(|index| {
        primary_button(
            "picker-use",
            if is_current {
                format!("Stay on {}", details.symbol)
            } else {
                format!("Use {}", details.symbol)
            },
            window,
            cx,
        )
        .on_click(cx.listener(move |this, _, window, cx| this.pick_symbol(index, window, cx)))
    });

    pane.child(
        div()
            .id("picker-details")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(sz(20.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(sz(12.))
                    .child(symbol_icon(&icon, 46., theme::bg()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(sz(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child(details.symbol.clone()),
                            )
                            .child(
                                div()
                                    .text_size(sz(12.))
                                    .line_height(gpui_kit::relative(1.4))
                                    .text_color(theme::dim())
                                    .child(details.description.clone().unwrap_or_default()),
                            ),
                    ),
            )
            .child(badges)
            .children(live)
            .children(no_quote)
            .child(table)
            .children(status),
    )
    .children(choose.map(|button| div().flex_none().px(sz(20.)).pb(sz(16.)).child(button)))
}

/// The line of key hints and the count.
fn footer(shown: usize, total: usize) -> Div {
    let hint = |caps: Vec<AnyElement>, label: &'static str| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(sz(6.))
            .children(caps)
            .child(
                div()
                    .text_size(sz(11.5))
                    .text_color(theme::dim())
                    .child(label),
            )
    };
    let count = if shown == total {
        format!("{} symbols", group(total))
    } else {
        format!("{} of {} symbols", group(shown), group(total))
    };
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .justify_between()
        .h(sz(40.))
        .px(sz(20.))
        .border_t_1()
        .border_color(theme::border())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(sz(16.))
                .child(hint(
                    vec![
                        key_cap(glyph(Glyph::ArrowUp, 11., theme::dim())).into_any_element(),
                        key_cap(glyph(Glyph::ArrowDown, 11., theme::dim())).into_any_element(),
                    ],
                    "Navigate",
                ))
                .child(hint(
                    vec![key_cap(glyph(Glyph::Enter, 11., theme::dim())).into_any_element()],
                    "Select",
                ))
                .child(hint(vec![key_cap("tab").into_any_element()], "Filter"))
                .child(hint(vec![key_cap("esc").into_any_element()], "Close")),
        )
        .child(
            div()
                .font_features(theme::tabular())
                .text_size(sz(11.5))
                .text_color(theme::dim())
                .child(count),
        )
}

/// A number with its thousands separated: `1,234`.
fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_symbol_is_found_in_the_lines_of_the_list() {
        let rows = [
            Row::Header(AssetClass::Forex, 2),
            Row::Symbol(4),
            Row::Symbol(9),
        ];
        assert_eq!(row_of(&rows, 9), Some(2));
        assert_eq!(row_of(&rows, 1), None);
    }

    #[test]
    fn counts_are_grouped_by_thousands() {
        assert_eq!(group(0), "0");
        assert_eq!(group(999), "999");
        assert_eq!(group(1_000), "1,000");
        assert_eq!(group(12_345), "12,345");
        assert_eq!(group(1_234_567), "1,234,567");
    }
}
