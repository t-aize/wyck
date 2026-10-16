//! How the measure and the position tools are drawn.

use super::{Anchor, P, Prim, Projection, lead, seg};
use crate::extras;
use crate::model::Drawing;
use crate::model::style::Dash;
use crate::model::tool::Tool;

pub(super) fn measure_prims(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    out: &mut Vec<Prim>,
) {
    let style = &drawing.style;
    let (Some([a, b]), Some([(x0, y0), (x1, y1)])) = (lead(&drawing.points), lead(pts)) else {
        return;
    };
    let up = b.p - a.p >= 0.0;
    // The color of the drawing is for a move up, the one of the look for a move down.
    let color = if up {
        style.color
    } else {
        style.measure.down_color
    };
    let width = style.width;
    out.push(Prim::Rect {
        a: (x0, y0),
        b: (x1, y1),
        fill: (style.fill && style.fill_opacity > 0.0)
            .then(|| (style.fill_color.unwrap_or(color), style.fill_opacity)),
        stroke: Some((color, width)),
    });
    // A cross through the middle.
    let (mx, my) = (f32::midpoint(x0, x1), f32::midpoint(y0, y1));
    out.push(seg((mx, y0), (mx, y1), color, 0.9, width, style.dash));
    out.push(seg((x0, my), (x1, my), color, 0.9, width, style.dash));

    if !style.labels {
        return;
    }
    let text = [
        extras::move_text(proj, a, b, &style.measure),
        extras::span_text(proj, a, b, &style.measure),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("   ");
    let top = y0.min(y1);
    out.push(Prim::Label {
        at: (mx, top - 14.0),
        text,
        color: style.text_color.unwrap_or(0xff_ffff),
        background: Some((color, 0.9)),
        anchor: Anchor::Center,
        size: style.text_size,
        bold: style.bold,
        face: style.face(),
    });
}

pub(super) fn position_prims(
    drawing: &Drawing,
    pts: &[P],
    proj: &dyn Projection,
    selected: bool,
    out: &mut Vec<Prim>,
) {
    let long = drawing.tool == Tool::LongPosition;
    let style = &drawing.style;
    let cfg = &style.position;
    let (Some([entry_point, stop_point, target_point]), Some([p0, p1, p2, p3])) =
        (lead(&drawing.points), lead(pts))
    else {
        return;
    };
    let entry = entry_point.p;
    let (stop, target) = (stop_point.p, target_point.p);
    let (left, right) = (p0.0, p3.0.max(p0.0 + 1.0));
    let (y_entry, y_stop, y_target) = (p0.1, p1.1, p2.1);
    let width = style.width;

    if style.fill && style.fill_opacity > 0.0 {
        out.push(Prim::Rect {
            a: (left, y_entry),
            b: (right, y_target),
            fill: Some((cfg.target_color, style.fill_opacity)),
            stroke: None,
        });
        out.push(Prim::Rect {
            a: (left, y_entry),
            b: (right, y_stop),
            fill: Some((cfg.stop_color, style.fill_opacity)),
            stroke: None,
        });
    }
    out.push(seg(
        (left, y_entry),
        (right, y_entry),
        cfg.entry_color,
        0.9,
        width,
        style.dash,
    ));
    out.push(seg(
        (left, y_target),
        (right, y_target),
        cfg.target_color,
        1.0,
        width,
        Dash::Solid,
    ));
    out.push(seg(
        (left, y_stop),
        (right, y_stop),
        cfg.stop_color,
        1.0,
        width,
        Dash::Solid,
    ));
    if !style.labels || !(selected || cfg.always_stats) {
        return;
    }

    let pct = |price: f64| {
        if entry == 0.0 {
            0.0
        } else {
            (price - entry) / entry * 100.0
        }
    };
    let reward = (target - entry).abs();
    let risk = (entry - stop).abs();
    let ratio = if risk > 0.0 { reward / risk } else { 0.0 };
    // A long profits when the target is above the entry; the drawing is only right if the stop is
    // on the other side. Say so when it is not, instead of showing a nonsense ratio.
    let sensible = if long {
        target > entry && stop < entry
    } else {
        target < entry && stop > entry
    };
    let tick = proj.tick();
    let pip = proj.pip();
    let stats = sensible
        .then(|| {
            cfg.stats(
                proj.real_price(entry),
                proj.real_price(stop),
                proj.real_price(target),
                tick,
            )
        })
        .flatten();
    // The stats are the amounts and the size: shown all the time, or only while it is selected.
    let details = selected || cfg.always_stats;
    let ticks_of = |price: f64| {
        if tick > 0.0 {
            (proj.real_price(price) - proj.real_price(entry)).abs() / tick
        } else {
            0.0
        }
    };
    let pips_of = |price: f64| {
        if pip > 0.0 {
            (proj.real_price(price) - proj.real_price(entry)).abs() / pip
        } else {
            0.0
        }
    };

    // The words of a level tag: what the switches ask for, or one short figure when compact.
    let level_text = |name: &str, price: f64, amount: Option<f64>| -> String {
        let amount = amount
            .filter(|_| cfg.show_amounts && details)
            .map(|a| cfg.format_amount(a));
        if cfg.compact {
            let mut text = amount
                .or_else(|| cfg.show_percent.then(|| format!("{:+.2}%", pct(price))))
                .unwrap_or_else(|| proj.format_price(price));
            if cfg.show_pips && pip > 0.0 {
                text = format!("{text} | {:.1} pips", pips_of(price));
            }
            if cfg.show_ticks && tick > 0.0 {
                text = format!("{text} | {} ticks", ticks_of(price).round());
            }
            return text;
        }
        let mut parts = vec![name.to_owned()];
        if cfg.show_price {
            parts.push(proj.format_price(price));
        }
        if cfg.show_percent {
            parts.push(format!("({:+.2}%)", pct(price)));
        }
        if cfg.show_ticks && tick > 0.0 {
            parts.push(format!("{} ticks", ticks_of(price).round()));
        }
        if cfg.show_pips && pip > 0.0 {
            parts.push(format!("{:.1} pips", pips_of(price)));
        }
        parts.extend(amount);
        parts.join(" ")
    };
    let ink = style.text_color;
    let (size, bold, face) = (style.text_size, style.bold, style.face());
    let middle = f32::midpoint(left, right);

    out.push(Prim::Label {
        at: (
            middle,
            y_target + if y_target < y_entry { -12.0 } else { 12.0 },
        ),
        text: level_text("Target", target, stats.map(|s| s.profit)),
        color: ink.unwrap_or(0x0a_0a0a),
        background: Some((cfg.target_color, 0.95)),
        anchor: Anchor::Center,
        size,
        bold,
        face,
    });
    out.push(Prim::Label {
        at: (middle, y_stop + if y_stop < y_entry { -12.0 } else { 12.0 }),
        text: level_text("Stop", stop, stats.map(|s| -s.loss)),
        color: ink.unwrap_or(0x0a_0a0a),
        background: Some((cfg.stop_color, 0.95)),
        anchor: Anchor::Center,
        size,
        bold,
        face,
    });

    let name = if long { "Long" } else { "Short" };
    let entry_text = if sensible {
        let mut parts = vec![name.to_owned()];
        if let Some(stats) = stats.filter(|_| details) {
            if cfg.show_qty {
                let capped = if stats.capped && !cfg.compact {
                    " (leverage)"
                } else {
                    ""
                };
                parts.push(if cfg.compact {
                    cfg.format_qty(stats.qty)
                } else {
                    format!("Qty {}{capped}", cfg.format_qty(stats.qty))
                });
            }
            if cfg.show_ratio {
                parts.push(if cfg.compact {
                    format!("{ratio:.1}R")
                } else {
                    format!("R/R {ratio:.2}")
                });
            }
            if cfg.show_risk && !cfg.compact {
                parts.push(format!("Risk {}", cfg.format_plain(stats.risk_amount)));
            }
        } else if cfg.show_ratio {
            parts.push(if cfg.compact {
                format!("{ratio:.1}R")
            } else {
                format!("R/R {ratio:.2}")
            });
        }
        parts.join(" | ")
    } else {
        "Stop and target are on the wrong sides".to_owned()
    };
    out.push(Prim::Label {
        at: (middle, y_entry),
        text: entry_text,
        color: ink.unwrap_or(0xff_ffff),
        background: Some((0x2a_2a2a, 0.95)),
        anchor: Anchor::Center,
        size,
        bold,
        face,
    });
}
