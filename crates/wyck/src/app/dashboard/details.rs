//! The details sheet on the right of the symbol picker: what the broker says about the symbol
//! under the highlight, a live price when it is the symbol being followed, and the reason when the
//! details could not be loaded.

use gpui::prelude::*;
use gpui::{AnyElement, Div, FontWeight, SharedString, div, px};
use gpui_kit::assets::IconName;
use wyck_openapi::market::Symbol;

use super::catalog::{Class, Entry};
use super::marks;
use crate::app::connection::ui;
use crate::app::{anim, theme};

/// Where the details of one symbol stand.
pub(super) enum Detail {
    Loading,
    Ready(Symbol),
    Failed(SharedString),
}

/// The live prices of the symbol being followed: bid, ask and spread in pips.
pub(super) type Live = (String, Option<String>, Option<String>);

/// `1234567` as `1,234,567`.
fn grouped(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

/// A volume as the server gives it, in hundredths of a unit, as a count of units.
fn units(hundredths: i64) -> String {
    let whole = hundredths / 100;
    let rest = hundredths % 100;
    if rest == 0 {
        grouped(whole)
    } else {
        format!("{}.{:02}", grouped(whole), rest.abs())
    }
}

/// `1 unit`, `1,000 units`.
fn units_label(hundredths: i64) -> String {
    let count = units(hundredths);
    if count == "1" {
        "1 unit".to_owned()
    } else {
        format!("{count} units")
    }
}

/// A volume in lots, with the units it stands for: `0.01 lots (1,000 units)`. Without a known lot
/// size only the units are shown.
fn volume(hundredths: i64, lot_size: Option<i64>) -> String {
    match lot_size.filter(|lot| *lot > 0) {
        Some(lot) => {
            let lots = hundredths as f64 / lot as f64;
            let text = if lots >= 1000.0 {
                grouped(lots.round() as i64)
            } else {
                let text = format!("{lots:.4}");
                text.trim_end_matches('0').trim_end_matches('.').to_owned()
            };
            let plural = if (lots - 1.0).abs() < f64::EPSILON {
                ""
            } else {
                "s"
            };
            format!("{text} lot{plural} ({})", units_label(hundredths))
        }
        None => units_label(hundredths),
    }
}

/// The size of a pip: ten to the power of minus `pip_position` (`4` gives `0.0001`).
fn pip_size(pip_position: i64) -> String {
    match usize::try_from(pip_position) {
        Ok(0) => "1".to_owned(),
        Ok(places) if places <= 8 => format!("0.{}1", "0".repeat(places - 1)),
        _ => format!("1e-{pip_position}"),
    }
}

/// The lines of the table for a symbol whose details are known.
fn spec_rows(symbol: &Symbol) -> Vec<(&'static str, String)> {
    let mut rows = vec![
        ("Price digits", symbol.digits.to_string()),
        ("Pip size", pip_size(symbol.pip_position)),
    ];
    if let Some(lot) = symbol.lot_size {
        rows.push(("Lot size", units_label(lot)));
    }
    if let Some(min) = symbol.min_volume {
        rows.push(("Minimum volume", volume(min, symbol.lot_size)));
    }
    if let Some(step) = symbol.step_volume {
        rows.push(("Volume step", volume(step, symbol.lot_size)));
    }
    if let Some(max) = symbol.max_volume {
        rows.push(("Maximum volume", volume(max, symbol.lot_size)));
    }
    if let Some(zone) = symbol
        .schedule_time_zone
        .as_deref()
        .filter(|z| !z.is_empty())
    {
        rows.push(("Trading hours zone", zone.to_owned()));
    }
    rows
}

/// A small rounded tag: the class, the category, "Current".
fn pill(text: impl Into<SharedString>, color: gpui::Rgba, tint: gpui::Rgba) -> Div {
    div()
        .px_2p5()
        .py_1()
        .rounded_full()
        .bg(tint)
        .text_size(px(11.))
        .text_color(color)
        .child(text.into())
}

/// One live figure: bid, ask or spread.
fn tile(label: &'static str, value: String) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .px_2p5()
        .py_2()
        .rounded_lg()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border_hairline())
        .child(
            div()
                .text_size(px(10.5))
                .text_color(theme::muted_fg())
                .child(label),
        )
        .child(
            div()
                .truncate()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::fg())
                .child(value),
        )
}

/// A line of the table: the label on the left, the value on the right.
fn row(label: &'static str, value: String, divider: bool) -> Div {
    div()
        .flex()
        .flex_row()
        .items_start()
        .justify_between()
        .gap_4()
        .px_3()
        .py_2()
        .when(divider, |el| {
            el.border_t_1().border_color(theme::border_hairline())
        })
        .text_size(px(12.))
        .child(div().flex_none().text_color(theme::muted_fg()).child(label))
        .child(
            div()
                .min_w_0()
                .text_right()
                .text_color(theme::fg())
                .child(value),
        )
}

/// What the sheet shows.
pub(super) struct Sheet<'a> {
    /// The symbol under the highlight (`None` when the search matches nothing).
    pub entry: Option<&'a Entry>,
    pub detail: Option<&'a Detail>,
    /// Live prices, when there are some for this symbol.
    pub live: Option<Live>,
    /// Whether this is the symbol the dashboard is on.
    pub is_current: bool,
    /// The button that chooses the symbol, pinned at the bottom.
    pub action: Option<AnyElement>,
    /// The panel that puts the symbol in favorites and watchlists.
    pub lists: Option<AnyElement>,
    pub epoch: u64,
}

pub(super) fn render_details(sheet: Sheet<'_>) -> Div {
    let pane = div()
        .flex_none()
        .w(px(380.))
        .h_full()
        .flex()
        .flex_col()
        .border_l_1()
        .border_color(theme::border_hairline())
        .bg(theme::bg());

    let Some(entry) = sheet.entry else {
        return pane
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .text_color(theme::muted_fg())
            .child("Highlight a symbol to see its details.");
    };

    let header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .child(marks::render(&entry.icon, 52., theme::bg()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .truncate()
                        .text_size(px(19.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::fg())
                        .child(entry.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted_fg())
                        .child(entry.description.clone()),
                ),
        );

    let mut badges = div().flex().flex_row().flex_wrap().gap_1p5().mt_3();
    badges = badges.child(pill(
        entry.class.label(),
        theme::muted_fg(),
        theme::surface(),
    ));
    if let Some(category) = &entry.category {
        badges = badges.child(pill(category.clone(), theme::muted_fg(), theme::surface()));
    }
    if sheet.is_current {
        badges = badges.child(pill("Current", theme::emerald(), {
            let mut tint = theme::emerald();
            tint.a = 0.14;
            tint
        }));
    }

    let has_live = sheet.live.is_some();
    let live = sheet.live.map(|(bid, ask, spread)| {
        div()
            .flex()
            .flex_row()
            .gap_2()
            .mt_4()
            .child(tile("Bid", bid))
            .children(ask.map(|ask| tile("Ask", ask)))
            .children(spread.map(|pips| tile("Spread", format!("{pips} pips"))))
    });

    let mut lines: Vec<(&'static str, String)> = vec![("Asset class", entry.class.label().into())];
    if let Some(category) = &entry.category {
        lines.push(("Category", category.clone()));
    }
    match (entry.class, &entry.base, &entry.quote) {
        (Class::Forex, Some(base), Some(quote)) => {
            lines.push(("Pair", format!("{base} / {quote}")));
        }
        (Class::Forex, ..) => {}
        // For the rest the broker's base asset is an internal name; what matters is the currency.
        (_, _, Some(quote)) => lines.push(("Quoted in", quote.clone())),
        _ => {}
    }
    if let Some(Detail::Ready(symbol)) = sheet.detail {
        lines.extend(spec_rows(symbol));
    }
    let last = lines.len().saturating_sub(1);
    let table = div()
        .mt_4()
        .flex()
        .flex_col()
        .rounded_lg()
        .overflow_hidden()
        .border_1()
        .border_color(theme::border_subtle())
        .bg(theme::surface())
        .children(
            lines
                .into_iter()
                .enumerate()
                .map(|(index, (label, value))| row(label, value, index != 0 && index <= last)),
        );

    // Said out loud: silence would read as a bug. The forex market is closed on weekends, and
    // some symbols have no price outside their session.
    let no_price = (!has_live && matches!(sheet.detail, Some(Detail::Ready(_)))).then(|| {
        div()
            .mt_3()
            .text_size(px(12.))
            .text_color(theme::muted_fg())
            .child("No price yet. The market may be closed for this symbol.")
    });

    let status = match sheet.detail {
        Some(Detail::Loading) | None => Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .mt_3()
                .text_size(px(12.))
                .text_color(theme::muted_fg())
                .child(anim::spin(
                    ui::icon_colored(IconName::LoaderCircle, 13., theme::muted_fg()),
                    ("details-loading", sheet.epoch),
                ))
                .child("Loading the details..."),
        ),
        Some(Detail::Failed(message)) => Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .mt_3()
                .p_3()
                .rounded_lg()
                .bg(theme::destructive_bg())
                .border_1()
                .border_color(theme::destructive())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(12.))
                        .text_color(theme::destructive())
                        .child(ui::icon_colored(
                            IconName::TriangleAlert,
                            14.,
                            theme::destructive(),
                        ))
                        .child("Could not load the details"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::muted_fg())
                        .child(message.clone()),
                ),
        ),
        Some(Detail::Ready(_)) => None,
    };

    pane.child(
        div()
            .id("symbol-details")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .child(header)
            .child(badges)
            .children(live)
            .children(no_price)
            .children(sheet.lists)
            .child(table)
            .children(status),
    )
    .children(
        sheet
            .action
            .map(|button| div().flex_none().px_5().pb_4().child(button)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_grouped_by_thousands() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(10_000_000), "10,000,000");
        assert_eq!(grouped(-1_234_567), "-1,234,567");
    }

    #[test]
    fn a_volume_in_hundredths_is_shown_in_units() {
        assert_eq!(units(10_000_000), "100,000");
        assert_eq!(units(100_000), "1,000");
        assert_eq!(units(150), "1.50");
    }

    #[test]
    fn a_volume_is_shown_in_lots_with_its_units() {
        assert_eq!(volume(100_000, Some(10_000_000)), "0.01 lots (1,000 units)");
        assert_eq!(
            volume(10_000_000, Some(10_000_000)),
            "1 lot (100,000 units)"
        );
        assert_eq!(volume(100_000, None), "1,000 units");
    }

    #[test]
    fn a_pip_is_ten_to_the_minus_its_position() {
        assert_eq!(pip_size(4), "0.0001");
        assert_eq!(pip_size(2), "0.01");
        assert_eq!(pip_size(1), "0.1");
        assert_eq!(pip_size(0), "1");
    }
}
