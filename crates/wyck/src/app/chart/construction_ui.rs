//! What the panel of a chart adds for the price based types beyond the size of their boxes: which
//! prices of a bar they read, the reversal of Renko and its wicks, the width of Kagi lines, the
//! look of the X and O, and the look of the bricks. Every change applies at once.

use gpui::{AnyElement, Context, Entity, Subscription, Window};
use gpui_kit::component::input::InputState;

use super::Chart;
use super::settings::ChartKind;
use super::settings_rows::{Numbers, choice, named, number, switch};
use super::transform::{PricePath, TransformSettings};

/// The numbers that are typed in.
pub(super) struct Inputs {
    renko_reversal: Entity<InputState>,
    brick_width: Entity<InputState>,
    brick_opacity: Entity<InputState>,
    kagi_thick: Entity<InputState>,
    kagi_thin: Entity<InputState>,
    pnf_glyph: Entity<InputState>,
    pnf_line: Entity<InputState>,
}

/// The inputs of the numbers, with the subscriptions that apply what is typed to `chart`.
pub(super) fn inputs<T: 'static>(
    chart: &Entity<Chart>,
    window: &mut Window,
    cx: &mut Context<T>,
) -> (Inputs, Vec<Subscription>) {
    let t = chart.read(cx).settings.transform;
    let mut numbers = Numbers::new(chart, window, cx);
    let renko_reversal = numbers.add(f64::from(t.renko_reversal), (1.0, 10.0), 0, |s, v| {
        s.transform.renko_reversal = v.round() as u32;
    });
    let brick_width = numbers.add(
        f64::from(t.brick_width) * 100.0,
        (20.0, 100.0),
        0,
        |s, v| {
            s.transform.brick_width = (v / 100.0) as f32;
        },
    );
    let brick_opacity = numbers.add(
        f64::from(t.brick_opacity) * 100.0,
        (10.0, 100.0),
        0,
        |s, v| s.transform.brick_opacity = (v / 100.0) as f32,
    );
    let kagi_thick = numbers.add(f64::from(t.kagi_thick), (0.5, 8.0), 1, |s, v| {
        s.transform.kagi_thick = v as f32;
    });
    let kagi_thin = numbers.add(f64::from(t.kagi_thin), (0.5, 8.0), 1, |s, v| {
        s.transform.kagi_thin = v as f32;
    });
    let pnf_glyph = numbers.add(f64::from(t.pnf_glyph) * 100.0, (30.0, 100.0), 0, |s, v| {
        s.transform.pnf_glyph = (v / 100.0) as f32;
    });
    let pnf_line = numbers.add(f64::from(t.pnf_line), (0.0, 6.0), 1, |s, v| {
        s.transform.pnf_line = v as f32;
    });
    (
        Inputs {
            renko_reversal,
            brick_width,
            brick_opacity,
            kagi_thick,
            kagi_thin,
            pnf_glyph,
            pnf_line,
        },
        numbers.subscriptions,
    )
}

const PATH_HINT: &str = "Open, high, low and close follows the price inside each bar, which keeps \
                         intraday charts close to a tick chart. Close only is quicker";

/// The rows to add to the construction group of `kind`, after its size.
pub(super) fn rows(
    kind: ChartKind,
    chart: &Entity<Chart>,
    inputs: &Inputs,
    t: &TransformSettings,
) -> Vec<AnyElement> {
    let paths = named(&PricePath::ALL, PricePath::label);
    let bricks = |rows: &mut Vec<AnyElement>| {
        rows.push(number(
            "Brick width (%)",
            Some("Out of the room each brick has"),
            &inputs.brick_width,
        ));
        rows.push(number(
            "Brick opacity (%)",
            Some("How solid the body is"),
            &inputs.brick_opacity,
        ));
        rows.push(switch(
            chart,
            "brick-border",
            "Line around bricks",
            None,
            t.brick_border,
            |s, on| s.transform.brick_border = on,
        ));
    };
    let mut rows = Vec::new();
    match kind {
        ChartKind::Renko => {
            rows.push(choice(
                chart,
                "renko-path",
                "Prices read",
                Some(PATH_HINT),
                paths,
                t.renko_path,
                |s, v| s.transform.renko_path = v,
            ));
            rows.push(number(
                "Reversal (boxes)",
                Some("How far back the price goes for a brick of the other color. 2 is the usual"),
                &inputs.renko_reversal,
            ));
            rows.push(switch(
                chart,
                "renko-wicks",
                "Wicks",
                Some("The highest and lowest price seen while a brick formed"),
                t.renko_wicks,
                |s, on| s.transform.renko_wicks = on,
            ));
            bricks(&mut rows);
        }
        ChartKind::LineBreak => {
            rows.push(choice(
                chart,
                "line-break-path",
                "Prices read",
                Some(PATH_HINT),
                paths,
                t.line_break_path,
                |s, v| s.transform.line_break_path = v,
            ));
            bricks(&mut rows);
        }
        ChartKind::Kagi => {
            rows.push(choice(
                chart,
                "kagi-path",
                "Prices read",
                Some(PATH_HINT),
                paths,
                t.kagi_path,
                |s, v| s.transform.kagi_path = v,
            ));
            rows.push(number(
                "Thick line (px)",
                Some("Yang: the price is above the last shoulder"),
                &inputs.kagi_thick,
            ));
            rows.push(number(
                "Thin line (px)",
                Some("Yin: the price is below the last waist"),
                &inputs.kagi_thin,
            ));
        }
        ChartKind::PointFigure => {
            rows.push(choice(
                chart,
                "pnf-path",
                "Prices read",
                Some(PATH_HINT),
                paths,
                t.pnf_path,
                |s, v| s.transform.pnf_path = v,
            ));
            rows.push(number(
                "Size of the X and O (%)",
                Some("How much of its box each one fills"),
                &inputs.pnf_glyph,
            ));
            rows.push(number(
                "Stroke (px)",
                Some("0 follows the size of the boxes"),
                &inputs.pnf_line,
            ));
        }
        ChartKind::Range => {
            rows.push(choice(
                chart,
                "range-path",
                "Prices read",
                Some(PATH_HINT),
                paths,
                t.range_path,
                |s, v| s.transform.range_path = v,
            ));
        }
        _ => {}
    }
    rows
}
