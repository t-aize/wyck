//! Drawing the TPO chart type: one profile per session, each in the room of one element of the
//! series, with its marks as letters or blocks, the point of control, the value area, the initial
//! balance, the single prints, the poor ends, and where the session opened and closed.
//!
//! Zoomed out until a profile is too narrow to read, the sessions are drawn as plain bars, so the
//! work of a frame stays bounded by the screen, like the other chart types.

use wyck::openapi::market::Bar;

use super::super::study::ValueFormat;
use super::super::tpo::{Profile, TpoColor, TpoDisplay, letter};
use super::cmd::{Align, Cmd, hsla, with_alpha};
use super::{Ctx, series};

/// Profiles in a slot narrower than this are drawn as bars.
const COMPACT_PX: f64 = 10.0;
/// The shortest row that holds a letter, and the narrowest column.
const LETTER_ROW: f32 = 9.0;
const LETTER_COL: f32 = 7.0;
/// The widest a column of marks gets, so a lone profile does not have huge letters.
const MAX_COL: f32 = 14.0;
/// Most marks drawn one by one; past it a row is one block.
const MAX_CELLS: usize = 6_000;
/// Most marks drawn as separate blocks.
const MAX_BLOCKS: usize = 20_000;
/// Most rows on screen before the sessions are drawn as bars.
const MAX_ROWS: usize = 12_000;
/// The room kept on the right of a profile for the prices of its levels.
const LABEL_W: f32 = 56.0;
/// The narrowest slot that has that room.
const LABEL_SLOT: f32 = 130.0;
/// The size of the text of the levels.
const LABEL_SIZE: f32 = 9.0;

/// Draws the profiles of `first..last`.
pub(super) fn draw(cx: &Ctx<'_>, bars: &[Bar], first: usize, last: usize, out: &mut Vec<Cmd>) {
    let profiles = &cx.f.display.tpo;
    let last = last.min(profiles.len()).min(bars.len());
    let visible = &profiles[first.min(last)..last];
    let rows: usize = visible.iter().map(|p| p.rows.len()).sum();
    if cx.f.view.bar_px < COMPACT_PX || visible.is_empty() || rows > MAX_ROWS {
        series::solid_candles(cx, bars, first, last, out);
        return;
    }
    let settings = &cx.f.settings.tpo;
    let slot = cx.f.view.bar_px as f32;
    let pad = slot * 0.04;
    let labels = settings.labels && slot >= LABEL_SLOT;
    let usable = slot - 2.0 * pad - if labels { LABEL_W } else { 0.0 };
    // One column width for all, so the profiles compare.
    let widest = visible
        .iter()
        .map(Profile::widest)
        .max()
        .unwrap_or(1)
        .max(1);
    let col_w = (usable / widest as f32).min(MAX_COL);
    let marks: usize = visible.iter().map(|p| p.marks).sum();
    let look = Look {
        col_w,
        usable,
        labels,
        letters: match settings.display {
            TpoDisplay::Letters => true,
            TpoDisplay::Blocks => false,
            TpoDisplay::Auto => col_w >= LETTER_COL,
        } && marks <= MAX_CELLS,
        blocks_apart: marks <= MAX_BLOCKS && col_w >= 3.0,
        widest,
    };
    for (n, profile) in visible.iter().enumerate() {
        let i = first + n;
        let left = cx.x(i) - slot / 2.0 + pad;
        draw_profile(cx, profile, left, &look, out);
    }
}

/// What is the same for every profile of a frame.
struct Look {
    /// The width of one mark.
    col_w: f32,
    /// The width of the room of the marks, from the left of the profile.
    usable: f32,
    /// Whether the prices of the levels are written on the right.
    labels: bool,
    /// Whether marks are letters (else blocks).
    letters: bool,
    /// Whether blocks are one for each mark (else one for each row).
    blocks_apart: bool,
    /// The most marks in a row of any profile on screen, for the colors that follow activity.
    widest: usize,
}

/// The rows a set of single prints covers.
fn single_rows(profile: &Profile, shown: bool) -> Vec<bool> {
    let mut rows = vec![false; profile.rows.len()];
    if shown {
        for print in &profile.singles {
            for row in &mut rows[print.first..=print.last] {
                *row = true;
            }
        }
    }
    rows
}

/// The colors of the marks of a row.
fn mark_color(
    cx: &Ctx<'_>,
    profile: &Profile,
    mark: u16,
    count: usize,
    widest: usize,
) -> gpui::Hsla {
    let p = cx.f.palette;
    match cx.f.settings.tpo.color {
        TpoColor::Single => with_alpha(p.tpo, 0.95),
        TpoColor::ByPeriod => {
            let hue = (f32::from(mark) * 47.0 % 360.0) / 360.0;
            gpui::hsla(hue, 0.55, 0.62, 0.95)
        }
        TpoColor::ByTime => {
            let share = if profile.periods > 1 {
                (usize::from(mark) % profile.periods) as f32 / (profile.periods - 1) as f32
            } else {
                1.0
            };
            with_alpha(p.tpo, 0.3 + 0.7 * share.min(1.0))
        }
        TpoColor::ByCount => with_alpha(p.tpo, 0.3 + 0.7 * count as f32 / widest as f32),
    }
}

fn draw_profile(cx: &Ctx<'_>, profile: &Profile, left: f32, look: &Look, out: &mut Vec<Cmd>) {
    let settings = &cx.f.settings.tpo;
    let p = cx.f.palette;
    let (top_edge, bottom_edge) = (cx.oy + cx.band.top as f32, cx.oy + cx.band.bottom() as f32);
    let right = left + look.usable;
    // The y of the top and of the bottom of a row, upward first whatever the scale does.
    let span = |row: usize| {
        let (a, b) = (
            cx.y(profile.row_top(row) as f64),
            cx.y(profile.row_bottom(row) as f64),
        );
        (a.min(b), a.max(b))
    };
    let on_screen = |top: f32, bottom: f32| bottom >= top_edge && top <= bottom_edge;
    let last_row = profile.rows.len() - 1;
    let (va_low, va_high) = profile.value_area;

    // Under the marks: the value area, the single prints and the initial balance.
    if settings.value_area {
        let (top, _) = span(va_high);
        let (_, bottom) = span(va_low);
        out.push(cx.rect(
            left,
            top,
            look.usable,
            bottom - top,
            with_alpha(p.tpo_value_area, 0.11),
        ));
    }
    let singles = single_rows(profile, settings.single_prints);
    for print in profile.singles.iter().filter(|_| settings.single_prints) {
        let (top, _) = span(print.last);
        let (_, bottom) = span(print.first);
        out.push(cx.rect(
            left,
            top,
            look.usable,
            bottom - top,
            with_alpha(p.tpo_single, if print.tail { 0.10 } else { 0.18 }),
        ));
    }
    if settings.initial_balance
        && let Some((low, high)) = profile.initial_balance
    {
        let (top, bottom) = (cx.y(high as f64), cx.y(low as f64));
        let (top, bottom) = (top.min(bottom), top.max(bottom));
        out.push(Cmd::Rect {
            x: left - 1.0,
            y: top,
            w: look.usable + 2.0,
            h: bottom - top,
            fill: gpui::transparent_black(),
            border: Some((1.0, with_alpha(p.tpo_ib, 0.75))),
            radius: 0.0,
        });
    }

    // The marks.
    for (row, marks) in profile.rows.iter().enumerate() {
        let (top, bottom) = span(row);
        if !on_screen(top, bottom) {
            continue;
        }
        let h = bottom - top;
        let is_poc = settings.poc && row == profile.poc;
        let in_va = !settings.value_area || (va_low..=va_high).contains(&row);
        let single = singles[row];
        let tone = |mark: u16| {
            let mut c = if is_poc {
                with_alpha(p.tpo_poc, 1.0)
            } else if single {
                with_alpha(p.tpo_single, 0.95)
            } else {
                mark_color(cx, profile, mark, marks.len(), look.widest)
            };
            // Outside the value area the marks are fainter, so it stands out.
            if !in_va && !is_poc && !single {
                c.a *= 0.6;
            }
            c
        };
        if look.letters && h >= LETTER_ROW {
            let size = (h - 2.0).clamp(7.0, 13.0);
            for (k, mark) in marks.iter().enumerate() {
                out.push(Cmd::Text {
                    text: letter(*mark).to_string(),
                    x: left + (k as f32 + 0.5) * look.col_w,
                    y: (top + bottom) / 2.0 - size * 0.65,
                    size,
                    color: tone(*mark),
                    align: Align::Center,
                    bold: is_poc,
                });
            }
        } else if look.blocks_apart {
            for (k, mark) in marks.iter().enumerate() {
                let mut c = tone(*mark);
                c.a *= 0.75;
                out.push(cx.rect(
                    left + k as f32 * look.col_w + 0.5,
                    top + 0.5,
                    look.col_w - 1.0,
                    (h - 1.0).max(1.0),
                    c,
                ));
            }
        } else if let Some(mark) = marks.first() {
            let mut c = tone(*mark);
            c.a *= 0.75;
            out.push(cx.rect(
                left,
                top + 0.5,
                marks.len() as f32 * look.col_w,
                (h - 1.0).max(1.0),
                c,
            ));
        }
    }

    // Over the marks: the lines across the profile.
    let line = |y: f32, color: gpui::Hsla, width: f32, dash: Option<[f32; 2]>| Cmd::Stroke {
        points: vec![(left - 2.0, y), (right + 2.0, y)],
        width,
        color,
        dash,
    };
    if settings.poc && settings.poc_line {
        let (top, bottom) = span(profile.poc);
        out.push(line(
            cx.snap((top + bottom) / 2.0) + 0.5,
            with_alpha(p.tpo_poc, 0.9),
            1.2,
            None,
        ));
    }
    if settings.value_area {
        for row in [va_high, va_low] {
            let (top, bottom) = span(row);
            let y = cx.snap(if row == va_high { top } else { bottom }) + 0.5;
            out.push(line(
                y,
                with_alpha(p.tpo_value_area, 0.85),
                1.0,
                Some([2.0, 3.0]),
            ));
        }
    }
    if settings.midpoint {
        let y = cx.snap(cx.y(profile.midpoint() as f64)) + 0.5;
        out.push(line(y, with_alpha(p.text, 0.5), 1.0, Some([4.0, 4.0])));
    }
    if settings.poor_extremes {
        for (poor, row, up) in [
            (profile.poor_high, last_row, true),
            (profile.poor_low, 0, false),
        ] {
            if !poor {
                continue;
            }
            let (top, bottom) = span(row);
            let y = if up { top } else { bottom };
            out.push(line(y, with_alpha(p.tpo_single, 0.95), 2.0, None));
            if look.letters || look.labels {
                out.push(Cmd::Text {
                    text: if up { "poor high" } else { "poor low" }.to_owned(),
                    x: left,
                    y: if up { y - 11.0 } else { y + 2.0 },
                    size: 8.0,
                    color: with_alpha(p.tpo_single, 0.95),
                    align: Align::Left,
                    bold: false,
                });
            }
        }
    }
    if settings.open_close {
        let center = |row: usize| {
            let (top, bottom) = span(row);
            (top + bottom) / 2.0
        };
        let y = center(profile.open_row);
        out.push(Cmd::Rect {
            x: left - 6.0,
            y: y - 2.5,
            w: 5.0,
            h: 5.0,
            fill: hsla(p.text_strong),
            border: None,
            radius: 2.5,
        });
        let y = center(profile.close_row);
        let end = profile.rows[profile.close_row].len() as f32 * look.col_w;
        out.push(Cmd::Rect {
            x: left + end + 1.0,
            y: y - 2.5,
            w: 5.0,
            h: 5.0,
            fill: gpui::transparent_black(),
            border: Some((1.2, hsla(p.text_strong))),
            radius: 2.5,
        });
    }
    if look.labels {
        let x = right + 8.0;
        let digits = cx.f.digits;
        let text = |price: i64, name: &str| {
            format!(
                "{name} {}",
                cx.map.label(price as f64, ValueFormat::Price, digits)
            )
        };
        let mut written: Vec<f32> = Vec::new();
        let mut put = |y: f32, text: String, color: gpui::Hsla| {
            // Levels that are close would write over each other: the first one stays.
            if written.iter().all(|w| (w - y).abs() >= LABEL_SIZE + 2.0) {
                written.push(y);
                out.push(Cmd::Text {
                    text,
                    x,
                    y: y - LABEL_SIZE * 0.65,
                    size: LABEL_SIZE,
                    color,
                    align: Align::Left,
                    bold: false,
                });
            }
        };
        if settings.poc {
            let (top, bottom) = span(profile.poc);
            let price = (profile.row_bottom(profile.poc) + profile.row_top(profile.poc)) / 2;
            put(
                (top + bottom) / 2.0,
                text(price, "POC"),
                with_alpha(p.tpo_poc, 1.0),
            );
        }
        if settings.value_area {
            put(
                span(va_high).0,
                text(profile.row_top(va_high), "VAH"),
                with_alpha(p.tpo_value_area, 1.0),
            );
            put(
                span(va_low).1,
                text(profile.row_bottom(va_low), "VAL"),
                with_alpha(p.tpo_value_area, 1.0),
            );
        }
    }
}
