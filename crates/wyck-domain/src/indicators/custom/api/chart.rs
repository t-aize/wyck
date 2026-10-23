//! What a script can ask about the chart it runs on: the calendar fields of each bar, the periods
//! and sessions, the timeframe, the symbol and where the bar is in the chart.
//!
//! Times are read in the time zone of the chart. Prices are the server's integers, so
//! `price_scale()`, `to_raw` and `from_raw` go between those and the prices a person writes.

use super::scalar;
use crate::base::calendar::{Anchor, Fields, Session, fields};
use crate::base::math::{count_i64, i64_to_f64};
use crate::indicators::custom::context::with_run;
use crate::indicators::custom::series::{Fallible, Series, truth};
use crate::market::{PRICE_SCALE, Period, Timeframe};
use rhai::{Dynamic, Engine};

/// The time of each bar moved by the offset of the zone, so that its UTC fields read as local.
fn local_times() -> Fallible<Vec<i64>> {
    with_run(|run| {
        let offsets = &run.context.offsets;
        run.columns
            .raw_time
            .iter()
            .enumerate()
            .map(|(i, time)| time.saturating_add(offsets.get(i).copied().unwrap_or(0)))
            .collect()
    })
}

/// One calendar field for every bar; `NaN` for a time that is not a date.
fn field(pick: fn(&Fields) -> u32) -> Fallible<Series> {
    Ok(Series::new(
        local_times()?
            .into_iter()
            .map(|t| fields(t).map_or(f64::NAN, |f| f64::from(pick(&f))))
            .collect(),
    ))
}

fn anchor(name: &str, text: &str) -> Fallible<Anchor> {
    Anchor::parse(text)
        .ok_or_else(|| format!("{name}: \"{text}\" is not day, week, month or year").into())
}

fn timeframe() -> Fallible<Option<Timeframe>> {
    with_run(|run| run.context.timeframe)
}

/// The days, weeks or months of a timeframe: how many of the unit it is.
fn multiplier(tf: Timeframe) -> i64 {
    match tf {
        Timeframe::Ticks => 0,
        Timeframe::Seconds(n) => i64::from(n),
        Timeframe::Bars(period) | Timeframe::Multiple(period, _) => {
            let group = i64::from(tf.group_size());
            match period {
                Period::D1 | Period::W1 | Period::MN1 => group,
                _ => period.millis() / 60_000 * group,
            }
        }
    }
}

fn is_period(tf: Option<Timeframe>, wanted: Period) -> bool {
    tf.and_then(Timeframe::period) == Some(wanted)
}

pub(super) fn chart(engine: &mut Engine) {
    calendar(engine);
    periods(engine);
    timeframes(engine);
    symbol(engine);
    bar_state(engine);
}

fn calendar(engine: &mut Engine) {
    engine.register_fn("year", || field(|f| u32::try_from(f.year).unwrap_or(0)));
    engine.register_fn("month", || field(|f| f.month));
    engine.register_fn("dayofmonth", || field(|f| f.day_of_month));
    // Sunday is 1 and Saturday is 7, as in Pine.
    engine.register_fn("dayofweek", || field(|f| f.day_of_week));
    engine.register_fn("hour", || field(|f| f.hour));
    engine.register_fn("minute", || field(|f| f.minute));
    engine.register_fn("second", || field(|f| f.second));
    engine.register_fn("weekofyear", || field(|f| f.week_of_year));
    // The offset of the time zone from UTC at each bar, in minutes.
    engine.register_fn("utc_offset", || -> Fallible<Series> {
        with_run(|run| {
            Series::new(
                (0..run.columns.len)
                    .map(|i| {
                        i64_to_f64(run.context.offsets.get(i).copied().unwrap_or(0)) / 60_000.0
                    })
                    .collect(),
            )
        })
    });
}

fn periods(engine: &mut Engine) {
    // The day of each bar, counted from the epoch in the zone of the chart.
    engine.register_fn("day", || -> Fallible<Series> {
        with_run(|run| Series::new(run.columns.day.iter().map(|d| i64_to_f64(*d)).collect()))
    });
    // A number that is the same for every bar of a day, week, month or year.
    engine.register_fn("period_key", |name: &str| -> Fallible<Series> {
        let anchor = anchor("period_key", name)?;
        with_run(|run| {
            Series::new(
                run.columns
                    .day
                    .iter()
                    .map(|d| i64_to_f64(anchor.key(*d)))
                    .collect(),
            )
        })
    });
    // 1 on the first bar of each day, week, month or year.
    engine.register_fn("is_new_period", |name: &str| -> Fallible<Series> {
        let anchor = anchor("is_new_period", name)?;
        with_run(|run| {
            let mut before = None;
            Series::new(
                run.columns
                    .day
                    .iter()
                    .map(|d| {
                        let key = anchor.key(*d);
                        let new = before != Some(key);
                        before = Some(key);
                        truth(new)
                    })
                    .collect(),
            )
        })
    });
    // 1 on the bars inside a session such as "0930-1600", in the zone of the chart.
    engine.register_fn("in_session", |text: &str| -> Fallible<Series> {
        let session = Session::parse(text)
            .ok_or_else(|| format!("in_session: \"{text}\" is not a session (write 0930-1600)"))?;
        Ok(Series::new(
            local_times()?
                .into_iter()
                .map(|t| {
                    fields(t).map_or(f64::NAN, |f| {
                        truth(session.contains(f.hour * 60 + f.minute))
                    })
                })
                .collect(),
        ))
    });
}

fn timeframes(engine: &mut Engine) {
    // The length of a bar in seconds, 0 when it is not known (and for ticks).
    engine.register_fn("timeframe_seconds", || -> Fallible<i64> {
        Ok(timeframe()?
            .and_then(Timeframe::bar_ms)
            .map_or(0, |ms| ms / 1_000))
    });
    // How many of its unit a bar is: 15 for 15 minutes, 4 for 4 hours is 240 minutes, 3 for 3 days.
    engine.register_fn("timeframe_multiplier", || -> Fallible<i64> {
        Ok(timeframe()?.map_or(0, multiplier))
    });
    // The code of the timeframe, such as "M15", "H4", "D1", "T" for ticks; empty when unknown.
    engine.register_fn("timeframe_code", || -> Fallible<String> {
        Ok(timeframe()?.map(Timeframe::code).unwrap_or_default())
    });
    engine.register_fn("timeframe_is_seconds", || -> Fallible<bool> {
        Ok(matches!(timeframe()?, Some(Timeframe::Seconds(_))))
    });
    engine.register_fn("timeframe_is_intraday", || -> Fallible<bool> {
        Ok(timeframe()?
            .and_then(Timeframe::bar_ms)
            .is_some_and(|ms| ms < 86_400_000))
    });
    engine.register_fn("timeframe_is_minutes", || -> Fallible<bool> {
        Ok(timeframe()?
            .and_then(Timeframe::bar_ms)
            .is_some_and(|ms| (60_000..86_400_000).contains(&ms)))
    });
    engine.register_fn("timeframe_is_daily", || -> Fallible<bool> {
        Ok(is_period(timeframe()?, Period::D1))
    });
    engine.register_fn("timeframe_is_weekly", || -> Fallible<bool> {
        Ok(is_period(timeframe()?, Period::W1))
    });
    engine.register_fn("timeframe_is_monthly", || -> Fallible<bool> {
        Ok(is_period(timeframe()?, Period::MN1))
    });
}

fn symbol(engine: &mut Engine) {
    engine.register_fn("syminfo_ticker", || -> Fallible<String> {
        with_run(|run| run.context.symbol.clone())
    });
    // The digits of the prices (5 for EURUSD), 0 when it is not known.
    engine.register_fn("syminfo_digits", || -> Fallible<i64> {
        with_run(|run| i64::from(run.context.digits))
    });
    // The smallest step of a price, in the integers prices are read in (1 for 5 digits, 100 for 3).
    engine.register_fn("syminfo_mintick", || -> Fallible<f64> {
        with_run(|run| match run.context.digits {
            1..=5 => 10_f64.powi(5 - i32::try_from(run.context.digits).unwrap_or(5)),
            _ => 1.0,
        })
    });
    // What every price is multiplied by: a price series holds integers, 108501 for 1.08501.
    engine.register_fn("price_scale", || PRICE_SCALE);
    // A price as a person writes it, in the integers of the series: to_raw(1.1) is 110000.
    engine.register_fn("to_raw", |x: Dynamic| -> Fallible<f64> {
        Ok(scalar("to_raw", &x)? * i64_to_f64(PRICE_SCALE))
    });
    engine.register_fn("to_raw", |s: Series| s.map(|x| x * i64_to_f64(PRICE_SCALE)));
    // The other way: from_raw(close) is the price as a person reads it.
    engine.register_fn("from_raw", |x: Dynamic| -> Fallible<f64> {
        Ok(scalar("from_raw", &x)? / i64_to_f64(PRICE_SCALE))
    });
    engine.register_fn("from_raw", |s: Series| {
        s.map(|x| x / i64_to_f64(PRICE_SCALE))
    });
    // A price rounded to the smallest step of the symbol.
    engine.register_fn("round_to_mintick", |s: Series| -> Fallible<Series> {
        let tick = with_run(|run| match run.context.digits {
            1..=5 => 10_f64.powi(5 - i32::try_from(run.context.digits).unwrap_or(5)),
            _ => 1.0,
        })?;
        Ok(s.map(|x| (x / tick).round() * tick))
    });
}

fn bar_state(engine: &mut Engine) {
    // 1 on the first bar of the chart.
    engine.register_fn("barstate_isfirst", || -> Fallible<Series> {
        with_run(|run| Series::new((0..run.columns.len).map(|i| truth(i == 0)).collect()))
    });
    // 1 on the last bar of the chart.
    engine.register_fn("barstate_islast", || -> Fallible<Series> {
        with_run(|run| {
            let last = run.columns.len.checked_sub(1);
            Series::new(
                (0..run.columns.len)
                    .map(|i| truth(Some(i) == last))
                    .collect(),
            )
        })
    });
    // 1 on the last bar while it is still forming.
    engine.register_fn("barstate_isrealtime", || -> Fallible<Series> {
        with_run(|run| {
            let last = run.columns.len.checked_sub(1);
            let live = run.context.live;
            Series::new(
                (0..run.columns.len)
                    .map(|i| truth(live && Some(i) == last))
                    .collect(),
            )
        })
    });
    // 1 on every bar that is closed.
    engine.register_fn("barstate_isconfirmed", || -> Fallible<Series> {
        with_run(|run| {
            let last = run.columns.len.checked_sub(1);
            let live = run.context.live;
            Series::new(
                (0..run.columns.len)
                    .map(|i| truth(!(live && Some(i) == last)))
                    .collect(),
            )
        })
    });
    // The index of the last bar of the chart.
    engine.register_fn("last_bar_index", || -> Fallible<i64> {
        with_run(|run| count_i64(run.columns.len.saturating_sub(1)))
    });
}
