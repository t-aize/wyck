//! The format and presets pages of the export dialog.

use super::ExportDialog;
use crate::chart::chart::settings_rows::named;
use crate::kit::icon::IconName;
use crate::kit::{form, theme, tokens};
use gpui::prelude::*;
use gpui::{AnyElement, Context, SharedString, div, px};
use wyck_chart::ColumnKey;
use wyck_chart::ExportOptions;
use wyck_chart::builtin_presets;
use wyck_chart::{
    Decimal, Delimiter, Empty, ExportZone, Format, HeaderCase, LineEnding, PriceDigits, Quote,
    TimeFormat,
};

impl ExportDialog {
    pub(super) fn format_page(&self, cx: &mut Context<Self>) -> AnyElement {
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
            file.push(Self::switch(
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
        file.push(Self::switch(
            cx,
            "export-bom",
            "Byte order mark",
            Some("Some spreadsheets need it to read accents"),
            o.bom,
            |o, on| o.bom = on,
        ));
        file.push(Self::switch(
            cx,
            "export-notes",
            "Say where the data comes from",
            Some("Comment lines first, or a meta object in JSON"),
            o.notes,
            |o, on| o.notes = on,
        ));
        if o.format == Format::Json {
            file.push(Self::switch(
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
            times.push(Self::switch(
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
        numbers.push(Self::switch(
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
        form::page()
            .child(form::group(IconName::FileText, "File", file))
            .child(form::group(IconName::Clock, "Times", times))
            .child(form::group(IconName::Hash, "Numbers", numbers))
            .child(form::group(IconName::Type, "Name of the file", name))
            .into_any_element()
    }

    pub(super) fn presets_page(&self, cx: &mut Context<Self>) -> AnyElement {
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
                .py_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(tokens::text::emphasis()))
                        .truncate()
                        .child(name),
                )
                .child(
                    crate::kit::button::quiet(SharedString::from(format!("preset-use-{id}")))
                        .label("Use")
                        .on_click(move |_, window, cx| {
                            let options = options.clone();
                            use_it.update(cx, |d, cx| d.apply(&options, window, cx));
                        }),
                )
                .children(deletable.then(|| {
                    crate::kit::button::quiet(SharedString::from(format!("preset-del-{id}")))
                        .icon(IconName::Trash)
                        .tooltip("Delete this preset")
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
        let built_in: Vec<AnyElement> = builtin_presets()
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
                    .text_size(px(tokens::text::body()))
                    .text_color(theme::muted_fg())
                    .child("Nothing saved yet.")
                    .into_any_element(),
            );
        }
        let save = this.clone();
        let name_state = self.inputs.preset_name.clone();
        let save_row = form::field(
            "Save the current choices",
            Some("A preset of the same name is replaced"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(form::text_field(
                    &self.inputs.preset_name,
                    tokens::field::text(),
                ))
                .child(
                    crate::kit::button::accent("preset-save")
                        .label("Save")
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
        form::page()
            .child(form::group(IconName::Bookmark, "Your presets", yours))
            .child(form::group(IconName::Save, "Save", [save_row]))
            .child(form::group(IconName::Sparkles, "Built in", built_in))
            .into_any_element()
    }
}
