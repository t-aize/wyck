//! Best-effort interpretation of the feed's free-form `forecast` / `previous` strings.
//!
//! The feed publishes these as display strings, not numbers: `"0.1%"`, `"-1.00T"`,
//! `"8.3K"`, `"49B"`, `"31.3"`, but also `"3.65|1.3"` (a bond auction's yield | bid-to-
//! cover), `"3-0-6"` (MPC vote split) and `"<1.25%"` (a bounded rate). [`Reading::parse`]
//! recognizes the numeric shapes and falls back to [`Reading::Text`] for everything else,
//! so a caller can compute a surprise where a number exists and simply display the raw
//! string where it does not. The raw string is always kept on
//! [`crate::CalendarEvent`]; a `Reading` is a view of it, never a replacement.

/// The unit suffix a numeric reading carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// No suffix: an index level, a ratio, a raw count.
    Plain,
    /// `%`.
    Percent,
    /// `K` — thousands.
    Thousand,
    /// `M` — millions.
    Million,
    /// `B` — billions.
    Billion,
    /// `T` — trillions.
    Trillion,
}

impl Unit {
    /// The factor a suffix multiplies the printed number by (`1.0` for
    /// [`Plain`](Unit::Plain) and [`Percent`](Unit::Percent), which are not magnitudes).
    #[must_use]
    pub fn multiplier(self) -> f64 {
        match self {
            Self::Plain | Self::Percent => 1.0,
            Self::Thousand => 1e3,
            Self::Million => 1e6,
            Self::Billion => 1e9,
            Self::Trillion => 1e12,
        }
    }
}

/// A parsed `forecast` / `previous` value.
#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    /// A single number with its unit, exactly as printed (`"8.3K"` → `8.3` +
    /// [`Unit::Thousand`]).
    Number {
        /// The printed number, unscaled.
        value: f64,
        /// The suffix it carried.
        unit: Unit,
    },
    /// Several `|`-separated readings, e.g. an auction's `"3.65|1.3"`.
    Compound(Vec<Reading>),
    /// Anything that is not a plain number: vote splits, bounded ranges, prose.
    Text(String),
}

impl Reading {
    /// Interprets one feed string. Never fails: unrecognized shapes become
    /// [`Reading::Text`].
    ///
    /// ```
    /// use wyck_calendar::{Reading, Unit};
    ///
    /// assert_eq!(Reading::parse("-0.1%"), Reading::Number { value: -0.1, unit: Unit::Percent });
    /// assert_eq!(Reading::parse("8.3K").scaled(), Some(8300.0));
    /// assert!(matches!(Reading::parse("3.65|1.3"), Reading::Compound(_)));
    /// assert!(matches!(Reading::parse("3-0-6"), Reading::Text(_)));
    /// ```
    #[must_use]
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        if raw.contains('|') {
            return Self::Compound(raw.split('|').map(Self::parse).collect());
        }
        parse_number(raw).unwrap_or_else(|| Self::Text(raw.to_owned()))
    }

    /// The comparable magnitude of a [`Reading::Number`]: the printed value multiplied
    /// out by its `K`/`M`/`B`/`T` suffix (percentages and plain numbers are returned
    /// as printed). `None` for compound and text readings.
    #[must_use]
    pub fn scaled(&self) -> Option<f64> {
        match self {
            Self::Number { value, unit } => Some(value * unit.multiplier()),
            Self::Compound(_) | Self::Text(_) => None,
        }
    }
}

fn parse_number(raw: &str) -> Option<Reading> {
    let split = raw
        .find(|c: char| !matches!(c, '0'..='9' | '.' | '-' | '+'))
        .unwrap_or(raw.len());
    let (number, suffix) = raw.split_at(split);
    let unit = match suffix {
        "" => Unit::Plain,
        "%" => Unit::Percent,
        "K" | "k" => Unit::Thousand,
        "M" | "m" => Unit::Million,
        "B" | "b" => Unit::Billion,
        "T" | "t" => Unit::Trillion,
        _ => return None,
    };
    let value: f64 = number.parse().ok()?;
    value.is_finite().then_some(Reading::Number { value, unit })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(value: f64, unit: Unit) -> Reading {
        Reading::Number { value, unit }
    }

    #[test]
    fn parses_every_numeric_shape_seen_in_the_feed() {
        assert_eq!(Reading::parse("0.1%"), num(0.1, Unit::Percent));
        assert_eq!(Reading::parse("-11.0K"), num(-11.0, Unit::Thousand));
        assert_eq!(Reading::parse("480B"), num(480.0, Unit::Billion));
        assert_eq!(Reading::parse("-1775M"), num(-1775.0, Unit::Million));
        assert_eq!(Reading::parse("-1.00T"), num(-1.0, Unit::Trillion));
        assert_eq!(Reading::parse("31.3"), num(31.3, Unit::Plain));
        assert_eq!(Reading::parse("+2.5%"), num(2.5, Unit::Percent));
        assert_eq!(Reading::parse("  76.4% "), num(76.4, Unit::Percent));
    }

    #[test]
    fn scaled_applies_the_suffix_but_not_for_percent() {
        assert_eq!(Reading::parse("8.3K").scaled(), Some(8300.0));
        assert_eq!(Reading::parse("-1.00T").scaled(), Some(-1e12));
        assert_eq!(Reading::parse("3.1%").scaled(), Some(3.1));
    }

    #[test]
    fn compound_values_are_split_and_each_side_parsed() {
        assert_eq!(
            Reading::parse("3.65|1.3"),
            Reading::Compound(vec![num(3.65, Unit::Plain), num(1.3, Unit::Plain)])
        );
        assert_eq!(Reading::parse("3.65|1.3").scaled(), None);
    }

    #[test]
    fn non_numeric_shapes_fall_back_to_text() {
        for raw in [
            "3-0-6", "<1.25%", ">2%", "", "n/a", "1e5", "NaN", "inf", ".",
        ] {
            assert_eq!(
                Reading::parse(raw),
                Reading::Text(raw.trim().to_owned()),
                "{raw:?}"
            );
        }
    }
}
