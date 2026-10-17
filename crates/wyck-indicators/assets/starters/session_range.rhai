// THE RANGE OF A SESSION
//
// The high and the low of each day's session so far (the Asian range, the London session, the
// cash open...), drawn on the prices. It uses a session setting (HHMM-HHMM), the time of every
// bar (`time`, in milliseconds since 1970, UTC) and a loop that starts over every day.
//
// The hours are in UTC, and a session that crosses midnight is not handled here: split it in two.

indicator(#{
    name: "Session range",
    short: "SESS",
    overlay: true,
    format: "price",
    category: "Examples",
    description: "The high and the low of a daily session, in UTC.",
});

let hours = input_session("hours", "0800-1600", #{
    label: "Session (UTC)",
    tooltip: "Written HHMM-HHMM, such as 0800-1600. Empty: the whole day.",
});

// The session as minutes since midnight.
let from = 0;
let to = 1440;
if hours != "" {
    let parts = hours.split("-");
    from = parts[0].sub_string(0, 2).parse_int() * 60 + parts[0].sub_string(2, 2).parse_int();
    to = parts[1].sub_string(0, 2).parse_int() * 60 + parts[1].sub_string(2, 2).parse_int();
}

// For every bar: the minute of the day, the day, and whether the bar is in the session.
let minute = floor(time / 60000.0) % 1440.0;
let day = floor(time / 86400000.0);
let inside = (minute >= from) & (minute < to);

let session_high = series(n, na);
let session_low = series(n, na);
let top = na;
let bottom = na;
let current_day = -1.0;

for i in 0..n {
    // A new day: forget the range of the day before.
    if day.at(i) != current_day {
        current_day = day.at(i);
        top = na;
        bottom = na;
    }
    if inside.at(i) == 1.0 {
        if top.is_nan() || high.at(i) > top { top = high.at(i); }
        if bottom.is_nan() || low.at(i) < bottom { bottom = low.at(i); }
    }
    session_high.set(i, top);
    session_low.set(i, bottom);
}

plot("session_high", session_high, #{ title: "Session high", color: "#26a69a", width: 1.5 });
plot("session_low", session_low, #{ title: "Session low", color: "#ef5350", width: 1.5 });
fill("session_high", "session_low", #{ color: "gray", alpha: 0.08 });
