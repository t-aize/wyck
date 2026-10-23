// DRAWINGS FROM A SCRIPT
//
// A script can put the same objects on the chart as the drawing tools: lines, a Fibonacci, a
// channel, a rectangle, an arrow, a text, a position... Here they follow the swings of the last
// bars. Change the look back in the settings and watch them move.
//
// draw(id, tool, points, options):
//  - the id is yours (letters, digits, _ and -), used once per run; at most 500 drawings;
//  - the tool is a name of drawing_tools(): the tool names in snake_case; each needs a number of
//    points (1 for a horizontal line, 2 for a trend line, 3 for a channel, 4 for a position);
//  - a point comes from bar_point(bar, price) (an existing bar) or time_point(unix_ms, price)
//    (any time, even in the future);
//  - the options are the ones of the style bar of a drawing: color, width, dash, opacity, fill,
//    fill_color, fill_opacity, extend_left, extend_right, text, text_size, text_color, bold,
//    labels, middle, scale, text_layout, levels... A mistake in an option is reported by name.
//
// Script drawings have no way to be an alert, and nothing is drawn on the run that declares the
// indicator (n is 0), so every draw is inside `if n > ...`.

indicator(#{
    name: "Drawings",
    short: "DRAW",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "Lines, Fibonacci, channel, rectangle, arrow, text and a position, drawn by a script.",
});

let lookback = input_int("lookback", 80, #{ label: "Look back (bars)", min: 10, max: 1000 });
let avg_length = input_int("average", 20, #{ label: "Average length", min: 2, max: 300 });
let accent = input_color("accent", "orange", #{ label: "Accent color" });
let show_fib = input_bool("show_fib", true, #{ label: "Fibonacci" });
let show_plan = input_bool("show_plan", true, #{ label: "Example position" });

if n > lookback {
    let first = n - lookback;
    let hi = highest(high, lookback).last();
    let lo = lowest(low, lookback).last();

    // Where the extremes of the window are. A loop is fine for a few hundred bars.
    let hi_at = first;
    let lo_at = first;
    for i in first..n {
        if high.at(i) >= hi { hi_at = i; }
        if low.at(i) <= lo { lo_at = i; }
    }
    // The swing, oldest point first.
    let swing = if lo_at < hi_at {
        [bar_point(lo_at, lo), bar_point(hi_at, hi)]
    } else {
        [bar_point(hi_at, hi), bar_point(lo_at, lo)]
    };

    // Two points: a rectangle around the window (fill and fill_opacity shade it).
    draw("window", "rectangle", [bar_point(first, hi), bar_point(n - 1, lo)], #{
        color: accent, width: 1, dash: "dotted", fill: true, fill_color: accent, fill_opacity: 0.05,
    });

    // One point: horizontal lines, with a label and a line style.
    draw("top", "horizontal_line", [bar_point(hi_at, hi)], #{ color: "red", dash: "dashed", text: "High" });
    draw("bottom", "horizontal_line", [bar_point(lo_at, lo)], #{ color: "green", dash: "dashed", text: "Low" });

    // Two points: a trend line through the swing, going on to the right.
    draw("swing", "trend_line", swing, #{ color: "white", width: 2, extend_right: true });

    // A Fibonacci on the same swing, with the levels we choose (value, color).
    if show_fib {
        draw("fib", "fib_retracement", swing, #{
            color: accent,
            extend_right: true,
            levels: [
                #{ value: 0.0, color: "gray" },
                #{ value: 0.382, color: "teal" },
                #{ value: 0.5, color: accent },
                #{ value: 0.618, color: "teal" },
                #{ value: 1.0, color: "gray" },
            ],
        });
    }

    // Three points: a parallel channel (two for the line, the third sets how wide it is).
    let start_price = close.at(first);
    draw("channel", "parallel_channel", [
        bar_point(first, start_price),
        bar_point(n - 1, close.last()),
        bar_point(first, start_price + (hi - lo) * 0.25),
    ], #{ color: "#42a5f5", width: 1, fill_opacity: 0.06 });

    // One point: a text with a tag behind it, and a price label.
    draw("title", "text", [bar_point(n - 1, hi)], #{
        text: `Last ${lookback} bars`,
        text_size: 13,
        bold: true,
        text_color: "white",
        text_layout: #{ align: "end", background: true },
    });
    draw("now", "price_label", [bar_point(n - 1, close.last())], #{ color: accent });

    // An arrow under the bar where the close last crossed above its average.
    let since = barssince(cross_over(close, sma(close, avg_length))).last();
    if !since.is_nan() {
        let bar = n - 1 - since.to_int();
        draw("cross", "arrow_mark_up", [bar_point(bar, low.at(bar))], #{ color: "green", scale: 1.5 });
    }

    // Four points: a long position (entry, stop, target, and the time the box ends). The point of
    // the end is a time_point: it is after the last bar, where there is no bar yet.
    // An illustration of the drawing, not a suggestion to trade.
    let entry = close.last();
    let stop = lowest(low, 10).last();
    if show_plan && entry > stop {
        let step = time.at(n - 1) - time.at(n - 2);
        let end = time.at(n - 1).to_int() + step.to_int() * 15;
        draw("plan", "long_position", [
            bar_point(n - 1, entry),
            bar_point(n - 1, stop),
            bar_point(n - 1, entry + (entry - stop) * 2.0),
            time_point(end, entry),
        ], #{ style: #{ position: #{ show_pips: true } }, text: "Example" });
    }

    print(`${drawing_tools().len()} drawing tools are available; this script used a dozen.`);
}
