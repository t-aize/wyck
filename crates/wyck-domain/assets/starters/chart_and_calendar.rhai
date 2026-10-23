// Chart and calendar: what a script can ask about the chart it runs on.
//
// Times are read in the time zone of the chart. A price series holds integers (108501 for the
// price 1.08501): price_scale(), to_raw and from_raw go between them and what a person writes.

indicator(#{
    name: "Chart and calendar",
    short: "Cal",
    overlay: false,
    format: "plain",
    decimals: 2,
    category: "Examples",
    description: "The calendar fields of each bar, periods, sessions, the timeframe, the symbol and where a bar is in the chart.",
});

let kind = input_choice("show", "hour", [
    "year",
    "month",
    "day of month",
    "day of week",
    "hour",
    "minute",
    "second",
    "week of year",
    "UTC offset",
    "day number",
    "new day",
    "new week",
    "new month",
    "new year",
    "period key",
    "in session",
    "timeframe",
    "symbol",
    "bar state",
    "price scale",
]);
let session = input_session("session", "0800-1600", #{ tooltip: "The part of the day marked by in session." });

// A number made into a series of the length of the chart, to be plotted.
let flat = close * 0;

let line = switch kind {
    "year" => year(),
    "month" => month(),
    "day of month" => dayofmonth(),
    "day of week" => dayofweek(),
    "hour" => hour(),
    "minute" => minute(),
    "second" => second(),
    "week of year" => weekofyear(),
    "UTC offset" => utc_offset(),
    "day number" => day(),
    "new day" => is_new_period("day"),
    "new week" => is_new_period("week"),
    "new month" => is_new_period("month"),
    "new year" => is_new_period("year"),
    "period key" => period_key("week"),
    "in session" => in_session(session),
    "timeframe" => {
        let is_day = if timeframe_is_daily() || timeframe_is_weekly() || timeframe_is_monthly() { 1.0 } else { 0.0 };
        let is_minutes = if timeframe_is_minutes() || timeframe_is_seconds() || timeframe_is_intraday() { 1.0 } else { 0.0 };
        let code = timeframe_code();
        let seconds = timeframe_seconds();
        let multiplier = timeframe_multiplier();
        print(`timeframe ${code}: ${seconds} s, x${multiplier}`);
        flat + seconds + is_day + is_minutes
    },
    "symbol" => {
        let ticker = syminfo_ticker();
        let digits = syminfo_digits();
        let tick = syminfo_mintick();
        print(`symbol ${ticker}: ${digits} digits, tick ${tick}`);
        round_to_mintick(from_raw(close) * price_scale()) * 0 + digits
    },
    "bar state" => {
        let flags = barstate_isfirst() + barstate_islast() * 2 + barstate_isrealtime() * 4 + barstate_isconfirmed() * 8;
        flags + flat + last_bar_index() * 0 + to_raw(0)
    },
    _ => flat + price_scale(),
};

plot("line", line);
