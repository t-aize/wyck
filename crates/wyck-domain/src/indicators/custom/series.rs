//! The value a script works with: a number for every bar of the chart.
//!
//! A script does not loop over bars. It writes `sma(close, 20) - sma(close, 50)` and the work is
//! done for all the bars at once, by the native code behind each function. That is what keeps a
//! script fast: the interpreter runs a handful of operations, not one per bar.
//!
//! A bar without a value (the first ones of a moving average) holds `NaN`, which the script
//! sees as `na`. `NaN` flows through the arithmetic, so a line simply starts where its inputs do.
//! A result that is not a number (a division by zero, the logarithm of 0) is `na` too, never an
//! infinity: the `ta` functions read every value that is not finite as a gap.
//!
//! The numbers are shared (`Arc`), so handing a series to a function or keeping it in a variable
//! copies nothing; writing to one that is shared copies it first. Every series a run makes is
//! counted by its [`Meter`](super::meter::Meter), so a run that holds too much is stopped.

use super::meter::{self, Meter};
use crate::base::math::{count_i64, i64_to_f64};
use std::sync::Arc;

use rhai::{Engine, EvalAltResult};

/// What a registered function returns: a value, or the error the script stops with.
pub(crate) type Fallible<T> = Result<T, Box<EvalAltResult>>;

/// The most bars a script may make a series of by itself, so `series(1e12, 0.0)` is refused.
pub(crate) const MAX_LEN: usize = 2_000_000;

/// The numbers of a series, and the meter of the run that made them (when one did).
#[derive(Debug, Default)]
struct Buffer {
    values: Vec<f64>,
    meter: Option<Arc<Meter>>,
}

impl Buffer {
    fn new(values: Vec<f64>) -> Self {
        let meter = meter::current();
        if let Some(meter) = &meter {
            meter.made(values.len());
        }
        Self { values, meter }
    }
}

impl Clone for Buffer {
    fn clone(&self) -> Self {
        Self::new(self.values.clone())
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        if let Some(meter) = &self.meter {
            meter.dropped(self.values.len());
        }
    }
}

impl PartialEq for Buffer {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

/// One number per bar. `NaN` is "no value".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Series(Arc<Buffer>);

impl Series {
    pub fn new(values: Vec<f64>) -> Self {
        Self(Arc::new(Buffer::new(values)))
    }

    /// `len` bars that all hold `value`.
    pub fn constant(len: usize, value: f64) -> Self {
        Self::new(vec![value; len])
    }

    pub fn len(&self) -> usize {
        self.0.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.values.is_empty()
    }

    pub fn values(&self) -> &[f64] {
        &self.0.values
    }

    /// The numbers, moved out when nobody else holds them.
    pub(crate) fn into_vec(self) -> Vec<f64> {
        match Arc::try_unwrap(self.0) {
            Ok(mut buffer) => std::mem::take(&mut buffer.values),
            Err(shared) => shared.values.clone(),
        }
    }

    /// The value at `index`, or `NaN` past either end.
    pub fn at(&self, index: i64) -> f64 {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.0.values.get(i).copied())
            .unwrap_or(f64::NAN)
    }

    /// The value of the last bar, `NaN` if there is none.
    pub fn last(&self) -> f64 {
        self.0.values.last().copied().unwrap_or(f64::NAN)
    }

    fn values_mut(&mut self) -> &mut Vec<f64> {
        &mut Arc::make_mut(&mut self.0).values
    }

    /// Each value through `f`.
    pub fn map(&self, f: impl Fn(f64) -> f64) -> Self {
        Self::new(self.0.values.iter().map(|v| f(*v)).collect())
    }

    /// The two series bar by bar through `f`. They must be as long as each other.
    pub fn zip(&self, other: &Self, f: impl Fn(f64, f64) -> f64) -> Fallible<Self> {
        if self.len() != other.len() {
            return Err(format!(
                "the two series have {} and {} bars: they must be as long as each other",
                self.len(),
                other.len()
            )
            .into());
        }
        Ok(Self::new(
            self.0
                .values
                .iter()
                .zip(other.0.values.iter())
                .map(|(a, b)| f(*a, *b))
                .collect(),
        ))
    }

    /// The series moved `bars` bars later: the value of `bars` bars ago. The first ones have no
    /// value. A negative `bars` looks ahead. A shift past the length of the series leaves no value
    /// at all.
    pub fn shifted(&self, bars: i64) -> Self {
        let values = self.values();
        let mut out = vec![f64::NAN; values.len()];
        for (i, slot) in out.iter_mut().enumerate() {
            let from = count_i64(i).checked_sub(bars);
            if let Some(&value) = from
                .and_then(|from| usize::try_from(from).ok())
                .and_then(|at| values.get(at))
            {
                *slot = value;
            }
        }
        Self::new(out)
    }

    /// What `print` shows of a series: too long to list.
    fn summary(&self) -> String {
        format!("Series({} bars, last {})", self.len(), self.last())
    }

    /// Stores `value` at `index`.
    fn put(&mut self, index: i64, value: f64) -> Fallible<()> {
        let len = self.len();
        match usize::try_from(index)
            .ok()
            .and_then(|i| self.values_mut().get_mut(i))
        {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err(format!("bar {index} is outside the series (it has {len} bars)").into()),
        }
    }
}

impl From<Vec<f64>> for Series {
    fn from(values: Vec<f64>) -> Self {
        Self::new(values)
    }
}

/// A truth value as a number, the way series hold them: 1 for true, 0 for false.
pub(crate) fn truth(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

/// Whether a series value counts as true: anything but 0 and `NaN`.
pub(crate) fn is_true(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

/// `value`, or `NaN` when it is an infinity: a result of arithmetic is a number or no value.
pub(crate) fn finite_or_na(value: f64) -> f64 {
    if value.is_infinite() { f64::NAN } else { value }
}

/// A number a script gave, whole or decimal, as a decimal. `None` for anything else.
pub(crate) fn number_of(value: &rhai::Dynamic) -> Option<f64> {
    value
        .as_float()
        .ok()
        .or_else(|| value.as_int().ok().map(i64_to_f64))
}

/// A length given by a script, checked against [`MAX_LEN`].
fn length_of(len: i64) -> Fallible<usize> {
    let len = usize::try_from(len).map_err(|_| "a series cannot have a negative length")?;
    if len > MAX_LEN {
        return Err(format!("a series of {len} bars is too long (the limit is {MAX_LEN})").into());
    }
    Ok(len)
}

/// Registers the type, its arithmetic, its comparisons and the reading of its bars.
#[expect(
    clippy::float_cmp,
    reason = "the `==` and `!=` operators of the script language compare series values for exact equality, as every language does"
)]
pub(crate) fn register(engine: &mut Engine) {
    engine.register_type_with_name::<Series>("Series");

    // Arithmetic and comparison, for a series with a series, a series with a number and a number
    // with a series. Whole numbers and decimals both work.
    macro_rules! binary {
        ($name:literal, $f:expr) => {{
            let f = $f;
            engine.register_fn($name, move |a: Series, b: Series| a.zip(&b, f));
            engine.register_fn($name, move |a: Series, b: f64| {
                Ok::<_, Box<EvalAltResult>>(a.map(|x| f(x, b)))
            });
            engine.register_fn($name, move |a: f64, b: Series| {
                Ok::<_, Box<EvalAltResult>>(b.map(|x| f(a, x)))
            });
            engine.register_fn($name, move |a: Series, b: i64| {
                Ok::<_, Box<EvalAltResult>>(a.map(|x| f(x, i64_to_f64(b))))
            });
            engine.register_fn($name, move |a: i64, b: Series| {
                Ok::<_, Box<EvalAltResult>>(b.map(|x| f(i64_to_f64(a), x)))
            });
        }};
    }
    binary!("+", |a: f64, b: f64| a + b);
    binary!("-", |a: f64, b: f64| a - b);
    binary!("*", |a: f64, b: f64| a * b);
    binary!("/", |a: f64, b: f64| finite_or_na(a / b));
    binary!("%", |a: f64, b: f64| finite_or_na(a % b));
    binary!("**", |a: f64, b: f64| finite_or_na(a.powf(b)));
    binary!("<", |a: f64, b: f64| truth(a < b));
    binary!("<=", |a: f64, b: f64| truth(a <= b));
    binary!(">", |a: f64, b: f64| truth(a > b));
    binary!(">=", |a: f64, b: f64| truth(a >= b));
    binary!("==", |a: f64, b: f64| truth(a == b));
    binary!("!=", |a: f64, b: f64| truth(a != b));
    // `&&` and `||` cannot work on a series (they stop at the first operand), so the single
    // characters do the job.
    binary!("&", |a: f64, b: f64| truth(is_true(a) && is_true(b)));
    binary!("|", |a: f64, b: f64| truth(is_true(a) || is_true(b)));

    engine.register_fn("-", |a: Series| a.map(|x| -x));
    engine.register_fn("!", |a: Series| a.map(|x| truth(!is_true(x))));

    // `close[1]` is the close of the bar before, for every bar. It is the same as `shift(close, 1)`.
    engine.register_indexer_get(|s: &mut Series, bars: i64| -> Fallible<Series> {
        if bars < 0 {
            return Err("a series cannot be read ahead with [ ]: use shift(series, -bars)".into());
        }
        Ok(s.shifted(bars))
    });

    engine.register_fn("len", |s: &mut Series| count_i64(s.len()));
    engine.register_fn("at", |s: &mut Series, index: i64| s.at(index));
    engine.register_fn("last", |s: &mut Series| s.last());
    engine.register_fn("shift", |s: Series, bars: i64| s.shifted(bars));
    engine.register_fn("to_string", |s: &mut Series| s.summary());
    engine.register_fn("to_debug", |s: &mut Series| s.summary());

    // A series made by the script, for a loop that needs to remember (see the examples).
    engine.register_fn("series", |len: i64, value: f64| -> Fallible<Series> {
        Ok(Series::constant(length_of(len)?, value))
    });
    engine.register_fn("series", |len: i64, value: i64| -> Fallible<Series> {
        Ok(Series::constant(length_of(len)?, i64_to_f64(value)))
    });
    engine.register_fn(
        "set",
        |s: &mut Series, index: i64, value: f64| -> Fallible<()> { s.put(index, value) },
    );
    engine.register_fn(
        "set",
        |s: &mut Series, index: i64, value: i64| -> Fallible<()> {
            s.put(index, i64_to_f64(value))
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_series_reads_past_its_ends_as_no_value() {
        let s = Series::new(vec![1.0, 2.0, 3.0]);
        assert_eq!(s.at(1), 2.0);
        assert!(s.at(3).is_nan());
        assert!(s.at(-1).is_nan());
        assert_eq!(s.last(), 3.0);
        assert!(Series::default().last().is_nan());
    }

    #[test]
    fn shifting_moves_the_values_later_and_leaves_the_first_empty() {
        let s = Series::new(vec![1.0, 2.0, 3.0, 4.0]);
        let back = s.shifted(2);
        assert!(back.at(0).is_nan() && back.at(1).is_nan());
        assert_eq!((back.at(2), back.at(3)), (1.0, 2.0));
        let ahead = s.shifted(-1);
        assert_eq!((ahead.at(0), ahead.at(2)), (2.0, 4.0));
        assert!(ahead.at(3).is_nan());
    }

    #[test]
    fn a_shift_of_any_size_leaves_no_value_and_does_not_panic() {
        let s = Series::new(vec![1.0, 2.0, 3.0]);
        for bars in [
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            -i64::MAX,
            1_000_000_000_000,
        ] {
            assert!(
                s.shifted(bars).values().iter().all(|v| v.is_nan()),
                "{bars}"
            );
        }
    }

    #[test]
    fn two_series_of_different_lengths_do_not_mix() {
        let a = Series::new(vec![1.0, 2.0]);
        let b = Series::new(vec![1.0]);
        assert!(a.zip(&b, |x, y| x + y).is_err());
        assert_eq!(a.zip(&a, |x, y| x + y).unwrap().values(), &[2.0, 4.0]);
    }

    #[test]
    fn writing_to_a_shared_series_copies_it_first() {
        let mut a = Series::new(vec![1.0, 2.0]);
        let b = a.clone();
        a.values_mut()[0] = 9.0;
        assert_eq!(a.values(), &[9.0, 2.0]);
        assert_eq!(b.values(), &[1.0, 2.0]);
    }

    #[test]
    fn a_result_that_is_not_a_number_is_no_value_and_never_an_infinity() {
        assert!(finite_or_na(1.0 / 0.0).is_nan());
        assert!(finite_or_na(f64::NEG_INFINITY).is_nan());
        assert_eq!(finite_or_na(2.5), 2.5);
        assert!(finite_or_na(f64::NAN).is_nan());
    }

    #[test]
    fn the_memory_of_a_series_is_counted_while_it_lives() {
        let meter = Meter::new(1_000);
        meter::set_current(Some(Arc::clone(&meter)));
        let kept = Series::constant(100, 1.0);
        let copy = kept.clone();
        assert!(!meter.exceeded(), "sharing copies nothing");
        let mut changed = copy;
        changed.values_mut()[0] = 2.0;
        meter::set_current(None);
        assert!(
            meter.exceeded(),
            "writing to a shared series copies it: 2 x 800 bytes"
        );
        drop((kept, changed));
    }
}
