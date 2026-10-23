//! Building the preview, and writing or copying the export.

use super::{ExportDialog, MAX_COPY_ROWS, Preview, human_bytes};
use crate::kit::toast::{self, Kind};
use gpui::{ClipboardItem, Context};
use wyck_app::lost::lost;
use wyck_domain::chart::PREVIEW_ROWS;
use wyck_domain::chart::{RangeKind, TimeFormat};
use wyck_domain::chart::{clashes, date_is_valid, file_name, pattern_is_valid, render, table};

impl ExportDialog {
    pub(super) fn build_preview(&self) -> Preview {
        let source = self.snapshot.source();
        let options = self.options.clone().normalized();
        let table = table(&source, &options, Some(PREVIEW_ROWS));
        let text = render(&table, &options, source.digits, source.zone);
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
        if raw.range == RangeKind::Dates && (!date_is_valid(&raw.from) || !date_is_valid(&raw.to)) {
            problems.push("A date is not read as 2026-01-05 or 2026-01-05 09:30.".to_owned());
        }
        if raw.time_format == TimeFormat::Custom && !pattern_is_valid(&raw.time_pattern) {
            problems.push("The time pattern is not valid, the default one is used.".to_owned());
        }
        if !pattern_is_valid(&raw.date_pattern) || !pattern_is_valid(&raw.clock_pattern) {
            problems.push(
                "The date or clock pattern is not valid, the default one is used.".to_owned(),
            );
        }
        if clashes(&options) {
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
            file_name: file_name(&source, &options, &table),
            problems,
        }
    }

    /// Makes the field where each chosen column is renamed, for the columns that have none yet.
    /// Asks where to write the file, then writes it off the interface thread.
    pub(super) fn export(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.options.columns.is_empty() {
            return;
        }
        self.persist();
        let options = self.options.clone().normalized();
        let snapshot = self.snapshot.clone();
        let picked = cx.prompt_for_new_path(
            &wyck_app::storage::AppPaths::documents_dir(),
            Some(&self.preview.file_name),
        );
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                lost(
                    "output: export",
                    this.update(cx, |d, cx| {
                        d.busy = false;
                        cx.notify();
                    }),
                );
                return;
            };
            let target = path.clone();
            let done = cx
                .background_executor()
                .spawn(async move {
                    let source = snapshot.source();
                    let table = table(&source, &options, None);
                    let text = render(&table, &options, source.digits, source.zone);
                    wyck_app::storage::atomic_write(&target, text.as_bytes())
                        .map(|()| (table.total_rows, text.len()))
                })
                .await;
            lost(
                "output: export",
                this.update(cx, |d, cx| {
                    d.busy = false;
                    match done {
                        Ok((rows, bytes)) => toast::show(
                            cx,
                            Kind::Success,
                            "Exported",
                            format!("{rows} rows, {}, to {}", human_bytes(bytes), path.display()),
                        ),
                        Err(error) => toast::Toast::error(
                            "The export failed",
                            format!("Nothing was written to {}.", path.display()),
                        )
                        .hint("Check that the folder exists and can be written to.")
                        .details(error.to_string())
                        .show(cx),
                    }
                    cx.notify();
                }),
            );
        })
        .detach();
    }

    /// Puts the export on the clipboard.
    pub(super) fn copy(&mut self, cx: &mut Context<Self>) {
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
        let table = table(&source, &options, None);
        let text = render(&table, &options, source.digits, source.zone);
        let (rows, bytes) = (table.total_rows, text.len());
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        toast::show(
            cx,
            Kind::Success,
            "Copied",
            format!("{rows} rows, {}, on the clipboard", human_bytes(bytes)),
        );
    }
}
