//! The panel that exports the data of a chart: what to export and from when, which columns and
//! under which names, how the file is written (the format, the delimiter, the times, the
//! numbers), presets to keep a set of choices, and a preview of the first rows that follows every
//! change. The file is written, or the text copied, from a snapshot taken when the panel opened,
//! so the numbers do not move under it while the chart goes on receiving prices.
//!
//! What the export contains is decided in [`super::export`]; this is only the window on it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClipboardItem, Context, Entity, SharedString, Subscription, Window, div, px,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable, Sizable};

use super::Chart;
use super::data::Series;
use super::display::Display;
use super::export::store::{self, Saved};
use super::export::{
    self, ColumnKey, Content, Decimal, Delimiter, Empty, ExportOptions, ExportZone, Format,
    HeaderCase, LineEnding, Order, PREVIEW_ROWS, PriceDigits, Quote, RangeKind, Source, TimeFormat,
};
use super::settings::{ChartKind, ChartSettings};
use super::settings_rows::named;
use super::zone::Zone;
use crate::app::settings_ui::{self as ui, Head, Tab};
use crate::app::toast::{self, Kind};
use crate::app::{modal, theme, widgets};

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
            modal::Options::new(900.0, 700.0).on_dismiss(move |_window, cx| {
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

/// The folder where the file dialog starts.
fn documents() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|dirs| dirs.document_dir().map(std::path::Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir)
}

/// The settings folder, where the presets are kept.
fn config_dir() -> Option<PathBuf> {
    wyck::config::AppPaths::discover()
        .ok()
        .map(|paths| paths.config_dir().to_path_buf())
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
            now_ms: super::now_ms(),
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
    dir: Option<PathBuf>,
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
    subscriptions.push(widgets::watch_parsed(
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
    let state = cx.new(|cx| widgets::number_state(value, low, high, 1.0, 0, window, cx));
    subscriptions.push(widgets::watch_number(&state, cx, move |this, value, cx| {
        if value >= low {
            apply(&mut this.options, value);
            this.edited(cx);
        }
    }));
    state
}

impl ExportDialog {
    fn new(chart: &Entity<Chart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let snapshot = Snapshot::of(chart.read(cx));
        let dir = config_dir();
        let saved = dir.as_deref().map(store::read).unwrap_or_default();
        let mut options = saved.last.clone().normalized();
        // A chart that draws what the prices are has nothing else to export.
        if !snapshot.display.is_derived() {
            options.content = Content::Prices;
        }
        // The columns the last export chose, kept to the ones this chart has.
        let available = snapshot.source().available(options.content);
        options.columns.retain(|c| available.contains(c));
        if options.columns.is_empty() {
            options.columns = ExportOptions::default()
                .columns
                .into_iter()
                .filter(|c| available.contains(c))
                .collect();
            if options.columns.is_empty() {
                options.columns = vec![ColumnKey::Time, ColumnKey::Price];
            }
        }
        let mut subs = Vec::new();
        let o = &options;
        let inputs = Inputs {
            from: text_input(
                window,
                cx,
                &mut subs,
                &o.from,
                "2026-01-05 09:30",
                |o, t| {
                    o.from = t.to_owned();
                },
            ),
            to: text_input(window, cx, &mut subs, &o.to, "2026-01-06", |o, t| {
                o.to = t.to_owned();
            }),
            last: number_input(
                window,
                cx,
                &mut subs,
                f64::from(o.last),
                (1.0, 1e7),
                |o, v| o.last = v.round() as u32,
            ),
            every: number_input(
                window,
                cx,
                &mut subs,
                f64::from(o.every),
                (1.0, 1e6),
                |o, v| o.every = v.round() as u32,
            ),
            limit: number_input(
                window,
                cx,
                &mut subs,
                f64::from(o.limit),
                (0.0, 1e8),
                |o, v| o.limit = v.round() as u32,
            ),
            custom_delimiter: text_input(
                window,
                cx,
                &mut subs,
                &o.custom_delimiter,
                "One character",
                |o, t| o.custom_delimiter = t.to_owned(),
            ),
            time_pattern: text_input(
                window,
                cx,
                &mut subs,
                &o.time_pattern,
                "%Y-%m-%d %H:%M:%S",
                |o, t| o.time_pattern = t.to_owned(),
            ),
            date_pattern: text_input(
                window,
                cx,
                &mut subs,
                &o.date_pattern,
                "%Y-%m-%d",
                |o, t| {
                    o.date_pattern = t.to_owned();
                },
            ),
            clock_pattern: text_input(
                window,
                cx,
                &mut subs,
                &o.clock_pattern,
                "%H:%M:%S",
                |o, t| {
                    o.clock_pattern = t.to_owned();
                },
            ),
            fixed_digits: number_input(
                window,
                cx,
                &mut subs,
                f64::from(o.fixed_digits),
                (0.0, 12.0),
                |o, v| o.fixed_digits = v.round() as u32,
            ),
            value_digits: number_input(
                window,
                cx,
                &mut subs,
                f64::from(o.value_digits),
                (0.0, 12.0),
                |o, v| o.value_digits = v.round() as u32,
            ),
            table_name: text_input(window, cx, &mut subs, &o.table_name, "bars", |o, t| {
                o.table_name = t.to_owned();
            }),
            file_name: text_input(
                window,
                cx,
                &mut subs,
                &o.file_name,
                "{symbol}_{timeframe}_{from}_{to}",
                |o, t| o.file_name = t.to_owned(),
            ),
            preset_name: cx.new(|cx| InputState::new(window, cx).placeholder("Name this preset")),
        };
        let mut dialog = Self {
            snapshot,
            saved,
            dir,
            options,
            page: Page::Data,
            inputs,
            names: BTreeMap::new(),
            preview: Preview::default(),
            busy: false,
            _subscriptions: subs,
        };
        dialog.preview = dialog.build_preview();
        dialog
    }

    /// Keeps the options for the next time the panel opens.
    fn persist(&mut self) {
        self.saved.last = self.options.clone().normalized();
        if let Some(dir) = &self.dir {
            let _ = store::write(dir, &self.saved);
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

    fn build_preview(&self) -> Preview {
        let source = self.snapshot.source();
        let options = self.options.clone().normalized();
        let table = export::table(&source, &options, Some(PREVIEW_ROWS));
        let text = export::render(&table, &options, source.digits, source.zone);
        let lines: Vec<String> = text
            .lines()
            .take(PREVIEW_ROWS + 4)
            .map(str::to_owned)
            .collect();
        // The size follows from the rows built and the rows there are.
        let built = table.rows.len().max(1);
        let bytes = if table.rows.is_empty() {
            text.len()
        } else {
            text.len() * table.total_rows.max(built) / built
        };
        let mut problems = Vec::new();
        let raw = &self.options;
        if raw.columns.is_empty() {
            problems.push("No column is chosen, so the file has no data.".to_owned());
        }
        if table.total_rows == 0 && !raw.columns.is_empty() {
            problems.push("No row matches the range. Check the dates.".to_owned());
        }
        if raw.range == RangeKind::Dates
            && (!export::date_is_valid(&raw.from) || !export::date_is_valid(&raw.to))
        {
            problems.push("A date is not read as 2026-01-05 or 2026-01-05 09:30.".to_owned());
        }
        if raw.time_format == TimeFormat::Custom && !export::pattern_is_valid(&raw.time_pattern) {
            problems.push("The time pattern is not valid, the default one is used.".to_owned());
        }
        if !export::pattern_is_valid(&raw.date_pattern)
            || !export::pattern_is_valid(&raw.clock_pattern)
        {
            problems.push(
                "The date or clock pattern is not valid, the default one is used.".to_owned(),
            );
        }
        if export::clashes(&options) {
            problems.push(
                "The decimal comma and the comma delimiter clash, and fields are never quoted."
                    .to_owned(),
            );
        }
        if raw.custom_delimiter.chars().count() > 1 {
            problems.push("Only the first character of the delimiter is used.".to_owned());
        }
        if table.total_rows > MAX_COPY_ROWS * 4 {
            problems.push("This is a very large file. Writing it may take a moment.".to_owned());
        }
        Preview {
            lines,
            rows: table.total_rows,
            bytes,
            file_name: export::file_name(&source, &options, &table),
            problems,
        }
    }

    /// Makes the field where each chosen column is renamed, for the columns that have none yet.
    fn ensure_name_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted: Vec<ColumnKey> = self.options.columns.clone();
        for key in wanted {
            let code = key.code();
            if self.names.contains_key(&code) {
                continue;
            }
            let current = self.options.renames.get(&code).cloned().unwrap_or_default();
            let placeholder: SharedString = self.snapshot.source().column_name(key).into();
            let state = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(current)
            });
            let subscription =
                widgets::watch_parsed(&state, cx, |_, text| Some(text.trim().to_owned()), {
                    let code = code.clone();
                    move |this, text: String, cx| {
                        if text.is_empty() {
                            this.options.renames.remove(&code);
                        } else {
                            this.options.renames.insert(code.clone(), text);
                        }
                        this.edited(cx);
                    }
                });
            self._subscriptions.push(subscription);
            self.names.insert(code, state);
        }
    }

    /// Puts the typed fields back in step with the options (after a preset or the defaults).
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let o = self.options.clone();
        let texts: [(&Entity<InputState>, String); 10] = [
            (&self.inputs.from, o.from.clone()),
            (&self.inputs.to, o.to.clone()),
            (&self.inputs.custom_delimiter, o.custom_delimiter.clone()),
            (&self.inputs.time_pattern, o.time_pattern.clone()),
            (&self.inputs.date_pattern, o.date_pattern.clone()),
            (&self.inputs.clock_pattern, o.clock_pattern.clone()),
            (&self.inputs.table_name, o.table_name.clone()),
            (&self.inputs.file_name, o.file_name.clone()),
            (&self.inputs.last, o.last.to_string()),
            (&self.inputs.every, o.every.to_string()),
        ];
        for (state, text) in texts {
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        let more: [(&Entity<InputState>, String); 3] = [
            (&self.inputs.limit, o.limit.to_string()),
            (&self.inputs.fixed_digits, o.fixed_digits.to_string()),
            (&self.inputs.value_digits, o.value_digits.to_string()),
        ];
        for (state, text) in more {
            state.update(cx, |state, cx| state.set_value(text, window, cx));
        }
        // The rename fields are made again from the new options.
        self.names.clear();
    }

    /// Uses the options of a preset, keeping to the columns this chart has.
    fn apply(&mut self, options: &ExportOptions, window: &mut Window, cx: &mut Context<Self>) {
        let mut options = options.clone().normalized();
        if !self.snapshot.display.is_derived() {
            options.content = Content::Prices;
        }
        let available = self.snapshot.source().available(options.content);
        options.columns.retain(|c| available.contains(c));
        self.options = options;
        self.sync_inputs(window, cx);
        self.edited(cx);
    }

    // ---- writing ----

    /// Asks where to write the file, then writes it off the interface thread.
    fn export(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.options.columns.is_empty() {
            return;
        }
        self.persist();
        let options = self.options.clone().normalized();
        let snapshot = self.snapshot.clone();
        let picked = cx.prompt_for_new_path(&documents(), Some(&self.preview.file_name));
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                let _ = this.update(cx, |d, cx| {
                    d.busy = false;
                    cx.notify();
                });
                return;
            };
            let target = path.clone();
            let done = cx
                .background_executor()
                .spawn(async move {
                    let source = snapshot.source();
                    let table = export::table(&source, &options, None);
                    let text = export::render(&table, &options, source.digits, source.zone);
                    std::fs::write(&target, text.as_bytes())
                        .map(|()| (table.total_rows, text.len()))
                })
                .await;
            let _ = this.update(cx, |d, cx| {
                d.busy = false;
                match done {
                    Ok((rows, bytes)) => toast::show(
                        cx,
                        Kind::Success,
                        "Exported",
                        format!("{rows} rows, {}, to {}", human_bytes(bytes), path.display()),
                    ),
                    Err(error) => toast::show(
                        cx,
                        Kind::Error,
                        "Export failed",
                        format!("{}: {error}", path.display()),
                    ),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts the export on the clipboard.
    fn copy(&mut self, cx: &mut Context<Self>) {
        if self.options.columns.is_empty() {
            return;
        }
        if self.preview.rows > MAX_COPY_ROWS {
            toast::show(
                cx,
                Kind::Warning,
                "Too many rows to copy",
                format!(
                    "{} rows are more than the {MAX_COPY_ROWS} that can go on the clipboard. Export to a file, or narrow the range.",
                    self.preview.rows
                ),
            );
            return;
        }
        self.persist();
        let options = self.options.clone().normalized();
        let source = self.snapshot.source();
        let table = export::table(&source, &options, None);
        let text = export::render(&table, &options, source.digits, source.zone);
        let (rows, bytes) = (table.total_rows, text.len());
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        toast::show(
            cx,
            Kind::Success,
            "Copied",
            format!("{rows} rows, {}, on the clipboard", human_bytes(bytes)),
        );
    }

    // ---- rows ----

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
        ui::field(
            label,
            hint,
            widgets::segmented(id, &names, index, move |chosen, _window, cx| {
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
        ui::field(
            label,
            hint,
            ui::toggle(id, on, move |on, _window, cx| {
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
        ui::field(label, hint, div().child(widgets::number_field(state, 120.)))
    }

    fn text(
        label: &'static str,
        hint: Option<&'static str>,
        state: &Entity<InputState>,
        width: f32,
    ) -> AnyElement {
        ui::field(label, hint, ui::text_field(state, width))
    }

    // ---- pages ----

    fn data_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let o = &self.options;
        let derived = self.snapshot.display.is_derived();
        let mut what = Vec::new();
        if derived {
            what.push(Self::choice(
                cx,
                "export-content",
                "Export",
                Some("The chart draws its own elements, which are not the prices"),
                export::Content::ALL
                    .iter()
                    .map(|c| {
                        (
                            *c,
                            if *c == Content::Shown {
                                self.snapshot.kind.label()
                            } else {
                                "Prices"
                            },
                        )
                    })
                    .collect(),
                o.content,
                |o, v| o.content = v,
            ));
        }
        what.push(Self::choice(
            cx,
            "export-range",
            "Rows",
            Some("Which part of the data goes in the file"),
            named(&RangeKind::ALL, RangeKind::label),
            o.range,
            |o, v| o.range = v,
        ));
        if o.range == RangeKind::Dates {
            what.push(Self::text(
                "From",
                Some("A date or a date and time, in the time zone chosen in Format. Empty is the start"),
                &self.inputs.from,
                190.,
            ));
            what.push(Self::text(
                "To",
                Some("A date alone takes that whole day. Empty is the end"),
                &self.inputs.to,
                190.,
            ));
        }
        if o.range == RangeKind::Last {
            what.push(Self::number("How many", None, &self.inputs.last));
        }
        let refine = vec![
            Self::choice(
                cx,
                "export-order",
                "Order",
                None,
                named(&Order::ALL, Order::label),
                o.order,
                |o, v| o.order = v,
            ),
            self.switch(
                cx,
                "export-forming",
                "Leave out the forming row",
                Some("The newest bar (or element) is not final yet"),
                o.skip_forming,
                |o, on| o.skip_forming = on,
            ),
            Self::number(
                "One row in",
                Some("1 keeps every row, 5 keeps one in five"),
                &self.inputs.every,
            ),
            Self::number("At most (rows)", Some("0 has no limit"), &self.inputs.limit),
        ];
        let summary = ui::field(
            "This export",
            None,
            div()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(format!(
                    "{} rows, about {}",
                    self.preview.rows,
                    human_bytes(self.preview.bytes)
                )),
        );
        ui::page()
            .child(ui::group(IconName::Layers, "What to export", what))
            .child(ui::group(IconName::ListFilter, "Which rows", refine))
            .child(ui::group(IconName::Info, "Result", [summary]))
            .into_any_element()
    }

    fn columns_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let source = self.snapshot.source();
        let this = cx.entity();
        let count = self.options.columns.len();
        let mut chosen: Vec<AnyElement> = Vec::new();
        for (index, key) in self.options.columns.iter().copied().enumerate() {
            let code = key.code();
            let up = this.clone();
            let down = this.clone();
            let gone = this.clone();
            let icon_button = |id: String, icon: IconName, tip: &'static str| {
                Button::new(SharedString::from(id))
                    .ghost()
                    .compact()
                    .icon(icon)
                    .tooltip(tip)
                    .cursor_pointer()
            };
            chosen.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py_1p5()
                    .child(
                        div()
                            .w(px(150.))
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(theme::muted_fg())
                            .truncate()
                            .child(source.column_name(key)),
                    )
                    .child(
                        div().flex_1().min_w_0().children(
                            self.names
                                .get(&code)
                                .map(|state| ui::text_field(state, 240.)),
                        ),
                    )
                    .child(
                        icon_button(format!("col-up-{code}"), IconName::ArrowUp, "Move up")
                            .disabled(index == 0)
                            .on_click(move |_, _, cx| {
                                up.update(cx, |d, cx| {
                                    d.options.columns.swap(index, index - 1);
                                    d.edited(cx);
                                });
                            }),
                    )
                    .child(
                        icon_button(format!("col-down-{code}"), IconName::ArrowDown, "Move down")
                            .disabled(index + 1 >= count)
                            .on_click(move |_, _, cx| {
                                down.update(cx, |d, cx| {
                                    d.options.columns.swap(index, index + 1);
                                    d.edited(cx);
                                });
                            }),
                    )
                    .child(
                        icon_button(format!("col-del-{code}"), IconName::X, "Leave out").on_click(
                            move |_, _, cx| {
                                gone.update(cx, |d, cx| {
                                    d.options.columns.remove(index);
                                    d.edited(cx);
                                });
                            },
                        ),
                    )
                    .into_any_element(),
            );
        }
        if chosen.is_empty() {
            chosen.push(
                div()
                    .py_2()
                    .text_size(px(12.))
                    .text_color(theme::muted_fg())
                    .child("No column: pick some below.")
                    .into_any_element(),
            );
        }
        let mut wrap = div().flex().flex_row().flex_wrap().gap_1p5();
        for key in self
            .available()
            .into_iter()
            .filter(|k| !self.options.columns.contains(k))
        {
            let add = this.clone();
            wrap = wrap.child(
                div()
                    .id(SharedString::from(format!("col-add-{}", key.code())))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .px_2p5()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::border_subtle())
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(theme::muted_fg())
                    .hover(|s| s.bg(theme::surface_hover()))
                    .on_click(move |_, _, cx| {
                        add.update(cx, |d, cx| {
                            d.options.columns.push(key);
                            d.edited(cx);
                        });
                    })
                    .child(format!("+ {}", source.column_name(key))),
            );
        }
        let reset = this.clone();
        let standard = Button::new("col-standard")
            .ghost()
            .small()
            .label("Standard columns")
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                reset.update(cx, |d, cx| {
                    let available = d.available();
                    d.options.columns = ExportOptions::default()
                        .columns
                        .into_iter()
                        .filter(|c| available.contains(c))
                        .collect();
                    d.edited(cx);
                });
            });
        ui::page()
            .child(ui::group(
                IconName::Rows3,
                "Columns in the file, in order",
                chosen,
            ))
            .child(ui::group(
                IconName::Plus,
                "Add a column",
                [ui::block(wrap), ui::block(standard)],
            ))
            .into_any_element()
    }

    fn format_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let o = &self.options;
        let mut file = vec![Self::choice(
            cx,
            "export-format",
            "Format",
            None,
            named(&Format::ALL, Format::label),
            o.format,
            |o, v| o.format = v,
        )];
        if o.format == Format::Delimited {
            file.push(Self::choice(
                cx,
                "export-delimiter",
                "Delimiter",
                None,
                named(&Delimiter::ALL, Delimiter::label),
                o.delimiter,
                |o, v| o.delimiter = v,
            ));
            if o.delimiter == Delimiter::Custom {
                file.push(Self::text(
                    "Other delimiter",
                    None,
                    &self.inputs.custom_delimiter,
                    120.,
                ));
            }
            file.push(Self::choice(
                cx,
                "export-quote",
                "Quotes",
                Some("Around a field that holds the delimiter or a quote"),
                named(&Quote::ALL, Quote::label),
                o.quote,
                |o, v| o.quote = v,
            ));
        }
        if matches!(o.format, Format::Delimited | Format::Markdown) {
            file.push(self.switch(
                cx,
                "export-header",
                "Header row",
                Some("The names of the columns on the first line"),
                o.header,
                |o, on| o.header = on,
            ));
        }
        file.push(Self::choice(
            cx,
            "export-case",
            "Names",
            Some("The case of the names of the columns"),
            named(&HeaderCase::ALL, HeaderCase::label),
            o.header_case,
            |o, v| o.header_case = v,
        ));
        file.push(Self::choice(
            cx,
            "export-eol",
            "Line ending",
            None,
            named(&LineEnding::ALL, LineEnding::label),
            o.line_ending,
            |o, v| o.line_ending = v,
        ));
        file.push(self.switch(
            cx,
            "export-bom",
            "Byte order mark",
            Some("Some spreadsheets need it to read accents"),
            o.bom,
            |o, on| o.bom = on,
        ));
        file.push(self.switch(
            cx,
            "export-notes",
            "Say where the data comes from",
            Some("Comment lines first, or a meta object in JSON"),
            o.notes,
            |o, on| o.notes = on,
        ));
        if o.format == Format::Json {
            file.push(self.switch(
                cx,
                "export-pretty",
                "Indented JSON",
                None,
                o.json_pretty,
                |o, on| o.json_pretty = on,
            ));
        }
        if o.format == Format::Sql {
            file.push(Self::text("Table", None, &self.inputs.table_name, 200.));
        }
        file.push(Self::choice(
            cx,
            "export-empty",
            "Missing values",
            Some("A value that is not there, such as an indicator before it has started"),
            named(&Empty::ALL, Empty::label),
            o.empty,
            |o, v| o.empty = v,
        ));

        let mut times = vec![
            Self::choice(
                cx,
                "export-time",
                "Time",
                None,
                named(&TimeFormat::ALL, TimeFormat::label),
                o.time_format,
                |o, v| o.time_format = v,
            ),
            Self::choice(
                cx,
                "export-zone",
                "Time zone",
                None,
                named(&ExportZone::ALL, ExportZone::label),
                o.zone,
                |o, v| o.zone = v,
            ),
        ];
        if o.time_format == TimeFormat::Custom {
            times.push(Self::text(
                "Time pattern",
                Some("With the codes of strftime: %Y %m %d %H %M %S"),
                &self.inputs.time_pattern,
                220.,
            ));
        }
        if matches!(o.time_format, TimeFormat::Iso8601 | TimeFormat::DateTime) {
            times.push(self.switch(
                cx,
                "export-millis",
                "Milliseconds",
                None,
                o.millis,
                |o, on| o.millis = on,
            ));
        }
        if o.columns.contains(&ColumnKey::Date) {
            times.push(Self::text(
                "Date pattern",
                Some("For the Date column"),
                &self.inputs.date_pattern,
                220.,
            ));
        }
        if o.columns.contains(&ColumnKey::Clock) {
            times.push(Self::text(
                "Clock pattern",
                Some("For the Clock column"),
                &self.inputs.clock_pattern,
                220.,
            ));
        }

        let mut numbers = vec![Self::choice(
            cx,
            "export-price",
            "Prices",
            None,
            named(&PriceDigits::ALL, PriceDigits::label),
            o.price_digits,
            |o, v| o.price_digits = v,
        )];
        if o.price_digits == PriceDigits::Fixed {
            numbers.push(Self::number("Decimals", None, &self.inputs.fixed_digits));
        }
        numbers.push(Self::choice(
            cx,
            "export-decimal",
            "Decimal mark",
            None,
            named(&Decimal::ALL, Decimal::label),
            o.decimal,
            |o, v| o.decimal = v,
        ));
        numbers.push(self.switch(
            cx,
            "export-trim",
            "Drop trailing zeros",
            Some("1.2300 becomes 1.23"),
            o.trim_zeros,
            |o, on| o.trim_zeros = on,
        ));
        numbers.push(Self::number(
            "Decimals of indicators",
            Some("For the values that are not prices"),
            &self.inputs.value_digits,
        ));

        let name = vec![Self::text(
            "File name",
            Some("With {symbol} {timeframe} {type} {from} {to} {date}"),
            &self.inputs.file_name,
            260.,
        )];
        ui::page()
            .child(ui::group(IconName::FileText, "File", file))
            .child(ui::group(IconName::Clock, "Times", times))
            .child(ui::group(IconName::Hash, "Numbers", numbers))
            .child(ui::group(IconName::Type, "Name of the file", name))
            .into_any_element()
    }

    fn presets_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.entity();
        let row = |id: String, name: String, options: ExportOptions, deletable: bool| {
            let use_it = this.clone();
            let remove = this.clone();
            let forget = name.clone();
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .py_1p5()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.))
                        .truncate()
                        .child(name),
                )
                .child(
                    Button::new(SharedString::from(format!("preset-use-{id}")))
                        .ghost()
                        .small()
                        .label("Use")
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            let options = options.clone();
                            use_it.update(cx, |d, cx| d.apply(&options, window, cx));
                        }),
                )
                .children(deletable.then(|| {
                    Button::new(SharedString::from(format!("preset-del-{id}")))
                        .ghost()
                        .compact()
                        .icon(IconName::Trash)
                        .tooltip("Delete this preset")
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            let forget = forget.clone();
                            remove.update(cx, |d, cx| {
                                d.saved.remove_preset(&forget);
                                d.persist();
                                cx.notify();
                            });
                        })
                }))
                .into_any_element()
        };
        let built_in: Vec<AnyElement> = export::builtin_presets()
            .into_iter()
            .enumerate()
            .map(|(n, p)| row(format!("b{n}"), p.name, p.options, false))
            .collect();
        let mut yours: Vec<AnyElement> = self
            .saved
            .presets
            .iter()
            .enumerate()
            .map(|(n, p)| row(format!("u{n}"), p.name.clone(), p.options.clone(), true))
            .collect();
        if yours.is_empty() {
            yours.push(
                div()
                    .py_2()
                    .text_size(px(12.))
                    .text_color(theme::muted_fg())
                    .child("Nothing saved yet.")
                    .into_any_element(),
            );
        }
        let save = this.clone();
        let name_state = self.inputs.preset_name.clone();
        let save_row = ui::field(
            "Save the current choices",
            Some("A preset of the same name is replaced"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(ui::text_field(&self.inputs.preset_name, 200.))
                .child(
                    Button::new("preset-save")
                        .primary()
                        .small()
                        .label("Save")
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            let name = name_state.read(cx).value().to_string();
                            let saved = save.update(cx, |d, cx| {
                                let options = d.options.clone();
                                let ok = d.saved.save_preset(&name, &options);
                                if ok {
                                    d.persist();
                                }
                                cx.notify();
                                ok
                            });
                            if saved {
                                name_state.update(cx, |s, cx| s.set_value("", window, cx));
                            }
                        }),
                ),
        );
        ui::page()
            .child(ui::group(IconName::Bookmark, "Your presets", yours))
            .child(ui::group(IconName::Save, "Save", [save_row]))
            .child(ui::group(IconName::Sparkles, "Built in", built_in))
            .into_any_element()
    }

    fn preview_page(&self) -> AnyElement {
        let p = &self.preview;
        let mut column = div().flex().flex_col().gap_0p5();
        for line in &p.lines {
            column = column.child(div().whitespace_nowrap().child(if line.is_empty() {
                SharedString::from(" ")
            } else {
                SharedString::from(line.clone())
            }));
        }
        let code = div()
            .id("export-preview")
            .w_full()
            .p_3()
            .rounded_md()
            .bg(theme::fg_alpha(0.05))
            .border_1()
            .border_color(theme::border_hairline())
            .overflow_x_scroll()
            .font_family(mono())
            .text_size(px(11.))
            .text_color(theme::fg())
            .child(column);
        let summary = ui::field(
            "File",
            Some("Where the dialog will offer to save it"),
            div()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(p.file_name.clone()),
        );
        let size = ui::field(
            "Size",
            None,
            div()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(format!("{} rows, about {}", p.rows, human_bytes(p.bytes))),
        );
        let mut page = ui::page().child(ui::group(IconName::Info, "The file", [summary, size]));
        if !p.problems.is_empty() {
            let rows = p.problems.iter().map(|text| {
                div()
                    .py_1p5()
                    .text_size(px(12.))
                    .text_color(theme::destructive())
                    .child(text.clone())
                    .into_any_element()
            });
            page = page.child(ui::group(IconName::TriangleAlert, "To look at", rows));
        }
        page.child(ui::group(
            IconName::Eye,
            format!("First {} rows", PREVIEW_ROWS),
            [ui::block(code)],
        ))
        .into_any_element()
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
            .text_size(px(11.))
            .text_color(theme::muted_fg())
            .child(format!(
                "{} rows, about {}",
                self.preview.rows,
                human_bytes(self.preview.bytes)
            ))
            .into_any_element();
        let footer = ui::footer(
            vec![
                ui::action(
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
                ui::action("export-close", "Close", None, false, modal::dismiss).into_any_element(),
                ui::action(
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
                ui::action(
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
        ui::frame(
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
