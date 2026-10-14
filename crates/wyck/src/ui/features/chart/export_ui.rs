//! The panel that exports the data of a chart: what to export and from when, which columns and
//! under which names, how the file is written (the format, the delimiter, the times, the
//! numbers), presets to keep a set of choices, and a preview of the first rows that follows every
//! change. The file is written, or the text copied, from a snapshot taken when the panel opened,
//! so the numbers do not move under it while the chart goes on receiving prices.
//!
//! What the export contains is decided in `wyck-chart` (its export code); this is only the window on it.

mod build;
pub mod format;
pub mod output;
pub mod pages;

use super::Chart;
use crate::app::chart_export as store;
use crate::app::chart_export::Saved;
use crate::app::storage::DocumentStore;
use crate::ui::kit::icon::IconName;
use crate::ui::kit::input::InputState;
use crate::ui::kit::{button, controls, form, form::Head, form::Tab, modal, number, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, App, Context, Entity, Subscription, Window, div, px};
use std::collections::BTreeMap;
use wyck_chart::ColumnKey;
use wyck_chart::Display;
use wyck_chart::ExportOptions;
use wyck_chart::Series;
use wyck_chart::Source;
use wyck_chart::Zone;
use wyck_chart::{ChartKind, ChartSettings};

/// The most rows put on the clipboard: past it a file is the better way.
const MAX_COPY_ROWS: usize = 250_000;

/// Opens the export panel of `chart`.
pub fn open(chart: Entity<Chart>, window: &mut Window, cx: &mut App) {
    // Opened once the chart that asked is no longer being updated, since the panel reads it.
    window.defer(cx, move |window, cx| {
        let dialog = cx.new(|cx| ExportDialog::new(&chart, window, cx));
        let keep = dialog.clone();
        modal::open(
            dialog,
            modal::Options::new(900.0, 700.0)
                .label("Export the chart")
                .on_dismiss(move |_window, cx| {
                    keep.update(cx, |d, _| d.persist());
                }),
            window,
            cx,
        );
    });
}

/// A fixed width font that the system has.
fn mono() -> &'static str {
    if cfg!(target_os = "windows") {
        "Consolas"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else {
        "DejaVu Sans Mono"
    }
}

/// A size as `830 B`, `12.4 KB`, `3.1 MB`.
fn human_bytes(bytes: usize) -> String {
    let b = bytes as f64;
    if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1_024.0 {
        format!("{:.1} KB", b / 1_024.0)
    } else {
        format!("{bytes} B")
    }
}

/// What the chart held when the panel opened.
#[derive(Clone)]
struct Snapshot {
    symbol: String,
    timeframe: String,
    bar_ms: Option<i64>,
    kind: ChartKind,
    digits: u32,
    zone: Zone,
    raw: Series,
    display: Display,
    settings: ChartSettings,
    visible: (usize, usize),
    now_ms: i64,
}

impl Snapshot {
    fn of(chart: &Chart) -> Self {
        let shown = chart.display.shown(&chart.series).len();
        let plot_w = chart.geometry().plot_w();
        Self {
            symbol: chart
                .symbol()
                .map_or_else(|| "symbol".to_owned(), |s| s.name.to_string()),
            timeframe: chart.timeframe().label(),
            bar_ms: chart.timeframe().bar_ms(),
            kind: chart.settings.kind,
            digits: chart.digits(),
            zone: chart.settings.zone,
            raw: chart.series.clone(),
            display: chart.display.clone(),
            settings: chart.settings.clone(),
            visible: chart.view.visible(shown, plot_w),
            now_ms: crate::app::market_data::now_ms(),
        }
    }

    fn source(&self) -> Source<'_> {
        Source {
            symbol: &self.symbol,
            timeframe: &self.timeframe,
            bar_ms: self.bar_ms,
            kind: self.kind,
            digits: self.digits,
            zone: self.zone,
            raw: &self.raw,
            display: &self.display,
            settings: &self.settings,
            visible: self.visible,
            now_ms: self.now_ms,
        }
    }
}

/// What the panel says about the export as it is set now.
#[derive(Default)]
struct Preview {
    /// The first rows, as they would be written.
    lines: Vec<String>,
    /// How many rows the export has, and about how big the file is.
    rows: usize,
    bytes: usize,
    file_name: String,
    /// What is wrong or was repaired, in words.
    problems: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Data,
    Columns,
    Format,
    Presets,
    Preview,
}

impl Page {
    const ALL: [Self; 5] = [
        Self::Data,
        Self::Columns,
        Self::Format,
        Self::Presets,
        Self::Preview,
    ];

    fn tab(self) -> Tab {
        let (label, icon) = match self {
            Self::Data => ("Data", IconName::Layers),
            Self::Columns => ("Columns", IconName::Rows3),
            Self::Format => ("Format", IconName::Type),
            Self::Presets => ("Presets", IconName::Bookmark),
            Self::Preview => ("Preview", IconName::Eye),
        };
        Tab { label, icon }
    }
}

/// The fields that are typed in.
struct Inputs {
    from: Entity<InputState>,
    to: Entity<InputState>,
    last: Entity<InputState>,
    every: Entity<InputState>,
    limit: Entity<InputState>,
    custom_delimiter: Entity<InputState>,
    time_pattern: Entity<InputState>,
    date_pattern: Entity<InputState>,
    clock_pattern: Entity<InputState>,
    fixed_digits: Entity<InputState>,
    value_digits: Entity<InputState>,
    table_name: Entity<InputState>,
    file_name: Entity<InputState>,
    preset_name: Entity<InputState>,
}

struct ExportDialog {
    snapshot: Snapshot,
    saved: Saved,
    /// Where the presets are kept; `None` when the settings folder could not be found.
    store: Option<DocumentStore>,
    options: ExportOptions,
    page: Page,
    inputs: Inputs,
    /// The fields where a column is renamed, by the code of the column.
    names: BTreeMap<String, Entity<InputState>>,
    preview: Preview,
    /// Whether a file is being written.
    busy: bool,
    _subscriptions: Vec<Subscription>,
}

/// A text field that applies what is typed to the options.
fn text_input(
    window: &mut Window,
    cx: &mut Context<ExportDialog>,
    subscriptions: &mut Vec<Subscription>,
    value: &str,
    placeholder: &'static str,
    apply: fn(&mut ExportOptions, &str),
) -> Entity<InputState> {
    let state = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value.to_owned())
    });
    subscriptions.push(number::watch_parsed(
        &state,
        cx,
        |_, text| Some(text.to_owned()),
        move |this, text: String, cx| {
            apply(&mut this.options, &text);
            this.edited(cx);
        },
    ));
    state
}

/// A number field that applies what is typed (when it is at least the smallest) to the options.
fn number_input(
    window: &mut Window,
    cx: &mut Context<ExportDialog>,
    subscriptions: &mut Vec<Subscription>,
    value: f64,
    (low, high): (f64, f64),
    apply: fn(&mut ExportOptions, f64),
) -> Entity<InputState> {
    let state = cx.new(|cx| {
        number::state(number::Kind::Count, value, window, cx)
            .min(low)
            .max(high)
    });
    subscriptions.push(number::watch(&state, cx, move |this, value, cx| {
        if value >= low {
            apply(&mut this.options, value);
            this.edited(cx);
        }
    }));
    state
}

impl ExportDialog {
    /// Keeps the options for the next time the panel opens.
    fn persist(&mut self) {
        self.saved.last = self.options.clone().normalized();
        if let Some(store) = &self.store {
            let _ = store::write(store, &self.saved);
        }
    }

    /// Something in the options changed: the preview follows.
    fn edited(&mut self, cx: &mut Context<Self>) {
        self.preview = self.build_preview();
        cx.notify();
    }

    /// The columns the panel offers for what is exported now.
    fn available(&self) -> Vec<ColumnKey> {
        self.snapshot.source().available(self.options.content)
    }

    fn choice<T: Copy + PartialEq + 'static>(
        cx: &mut Context<Self>,
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        options: Vec<(T, &'static str)>,
        current: T,
        set: fn(&mut ExportOptions, T),
    ) -> AnyElement {
        let names: Vec<&'static str> = options.iter().map(|(_, name)| *name).collect();
        let index = options.iter().position(|(v, _)| *v == current).unwrap_or(0);
        let this = cx.entity();
        form::field(
            label,
            hint,
            controls::segmented(id, &names, index, move |chosen, _window, cx| {
                this.update(cx, |d, cx| {
                    set(&mut d.options, options[chosen].0);
                    d.edited(cx);
                });
            }),
        )
    }

    fn switch(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        label: &'static str,
        hint: Option<&'static str>,
        on: bool,
        set: fn(&mut ExportOptions, bool),
    ) -> AnyElement {
        let this = cx.entity();
        form::field(
            label,
            hint,
            controls::toggle(id, on, move |on, _window, cx| {
                this.update(cx, |d, cx| {
                    set(&mut d.options, on);
                    d.edited(cx);
                });
            }),
        )
    }

    fn number(
        label: &'static str,
        hint: Option<&'static str>,
        state: &Entity<InputState>,
    ) -> AnyElement {
        form::field(
            label,
            hint,
            div().child(number::field(state, tokens::field::number())),
        )
    }

    fn text(
        label: &'static str,
        hint: Option<&'static str>,
        state: &Entity<InputState>,
        width: f32,
    ) -> AnyElement {
        form::field(label, hint, form::text_field(state, width))
    }
}

impl Render for ExportDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_name_inputs(window, cx);
        let head = Head {
            icon: IconName::Download,
            title: "Export chart data".into(),
            subtitle: format!(
                "{} - {} - {}",
                self.snapshot.symbol,
                self.snapshot.timeframe,
                self.snapshot.kind.label()
            )
            .into(),
        };
        let body = match self.page {
            Page::Data => self.data_page(cx),
            Page::Columns => self.columns_page(cx),
            Page::Format => self.format_page(cx),
            Page::Presets => self.presets_page(cx),
            Page::Preview => self.preview_page(),
        };
        let tabs: Vec<Tab> = Page::ALL.iter().map(|p| p.tab()).collect();
        let active = Page::ALL.iter().position(|p| *p == self.page).unwrap_or(0);
        let this = cx.entity();
        let (defaults, copy, write) = (cx.entity(), cx.entity(), cx.entity());
        let ready = !self.options.columns.is_empty() && !self.busy;
        let summary = div()
            .text_size(px(tokens::text::small()))
            .text_color(theme::muted_fg())
            .child(format!(
                "{} rows, about {}",
                self.preview.rows,
                human_bytes(self.preview.bytes)
            ))
            .into_any_element();
        let footer = form::footer(
            vec![
                button::action(
                    "export-defaults",
                    "Defaults",
                    Some(IconName::RotateCcw),
                    false,
                    move |window, cx| {
                        defaults.update(cx, |d, cx| {
                            d.apply(&ExportOptions::default(), window, cx);
                        });
                    },
                )
                .into_any_element(),
                summary,
            ],
            vec![
                button::action("export-close", "Close", None, false, modal::dismiss)
                    .into_any_element(),
                button::action(
                    "export-copy",
                    "Copy",
                    Some(IconName::Copy),
                    false,
                    move |_window, cx| {
                        if ready {
                            copy.update(cx, |d, cx| d.copy(cx));
                        }
                    },
                )
                .into_any_element(),
                button::action(
                    "export-write",
                    if self.busy { "Writing..." } else { "Export..." },
                    Some(IconName::Download),
                    true,
                    move |_window, cx| {
                        if ready {
                            write.update(cx, |d, cx| d.export(cx));
                        }
                    },
                )
                .into_any_element(),
            ],
        );
        form::frame(
            head,
            &tabs,
            active,
            move |index, _window, cx| {
                this.update(cx, |d, cx| {
                    d.page = Page::ALL[index];
                    cx.notify();
                });
            },
            modal::dismiss,
            body,
            footer,
        )
    }
}
