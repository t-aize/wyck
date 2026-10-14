//! Building the export panel and keeping its text fields in step with the options.

use super::super::Chart;
use super::{ExportDialog, Inputs, Page, Preview, Snapshot, number_input, text_input};
use crate::app::chart_export as store;
use crate::ui::kit::input::InputState;
use crate::ui::kit::number;
use gpui::prelude::*;
use gpui::{Context, Entity, SharedString, Window};
use std::collections::BTreeMap;
use wyck_chart::export::choices::Content;
use wyck_chart::export::columns::ColumnKey;
use wyck_chart::export::options::ExportOptions;

impl ExportDialog {
    pub(super) fn new(chart: &Entity<Chart>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let snapshot = Snapshot::of(chart.read(cx));
        let store = crate::app_paths().map(crate::app::storage::AppPaths::documents);
        let saved = store.as_ref().map(store::read).unwrap_or_default();
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
            store,
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

    pub(super) fn ensure_name_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                number::watch_parsed(&state, cx, |_, text| Some(text.trim().to_owned()), {
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
    pub(super) fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
    pub(super) fn apply(
        &mut self,
        options: &ExportOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
}
