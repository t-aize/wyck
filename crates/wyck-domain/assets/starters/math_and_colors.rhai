// Math and colors: the arithmetic on series, a few lists of numbers, text and colors.
//
// An array is a list in square brackets. sum, avg, min, max, median and percentile read an array
// of numbers; given an array of series, min and max work bar by bar.

indicator(#{
    name: "Math and colors",
    short: "Math",
    overlay: false,
    format: "plain",
    decimals: 3,
    category: "Examples",
    description: "Trigonometry, constants, arrays of numbers, text from numbers, colors and gradients, and the Pine names of crossings, momentum and SAR.",
});

let kind = input_choice("show", "angles", [
    "angles",
    "constants",
    "arrays",
    "text",
    "colors",
    "crossings",
    "momentum and range",
    "tools",
]);

// A number between -1 and 1 for each bar, to feed the functions that need one.
let unit = sin(close);

let line = switch kind {
    "angles" => todegrees(asin(unit)) + todegrees(acos(unit)) + todegrees(atan(unit)) + toradians(tan(unit)),
    "constants" => log(close) * 0 + pi() + e() + phi() + rphi(),
    "arrays" => {
        let numbers = [4, 8, 6, 2, 9];
        let spread = max(numbers) - min(numbers);
        let both = max([close, open, low]) - min([close, open, low]);
        both * 0 + sum(numbers) + avg(numbers) + median(numbers) + stdev(numbers) + percentile(numbers, 75) + spread + avg(high, low) * 0 + min(close, open, 0) * 0 + max(close, open, 0) * 0
    },
    "text" => {
        let rounded = fmt(close.last(), 2);
        let full = tostring(close.last());
        print(`${rounded} as text: ${full}, ${len(close)} bars`);
        close * 0
    },
    "colors" => {
        let warm = color_mix("yellow", "red", 0.5);
        let blend = from_gradient(unit.last(), -1, 1, "red", "green");
        let red = color_r(warm);
        let green = color_g(warm);
        let blue = color_b(warm);
        let again = rgb(red, green, blue);
        print(`${warm} ${blend} ${again}`);
        close * 0 + color_r(blend)
    },
    "crossings" => {
        let fast = sma(close, 5);
        let slow = sma(close, 20);
        crossover(fast, slow) - crossunder(fast, slow)
    },
    "momentum and range" => mom(close, 10) + wpr(14) + sar(0.02, 0.02, 0.2) * 0,
    _ => {
        let tools = drawing_tools();
        print(`${tools.len()} drawing tools, the first is ${tools[0]}, as text ${tools.to_string()}`);
        close * 0 + close.to_string().len()
    },
};

plot("line", line);
