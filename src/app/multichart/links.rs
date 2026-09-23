//! What a chart's event means for the other charts, given what is linked. Plain data in and out,
//! so the rules of the multichart are tested without a window.
//!
//! - **Symbol**: every chart shows the symbol picked on any of them.
//! - **Interval**: every chart shows the same timeframe.
//! - **Crosshair**: the pointer over one chart shows on the others, at the same time (and at the
//!   same price when they show the same symbol).
//! - **Time**: scrolling one chart brings the others to the same time at the right edge.
//! - **Date range**: zooming and scrolling one chart gives the others the same span of time.

use crate::app::chart::{ChartEvent, Hover, Span};

pub use crate::app::workspace::LinksPref as Links;

/// One of the links, for switching it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Symbol,
    Interval,
    Crosshair,
    Time,
    Range,
}

impl Link {
    #[cfg(test)]
    pub const ALL: [Self; 5] = [
        Self::Symbol,
        Self::Interval,
        Self::Crosshair,
        Self::Time,
        Self::Range,
    ];

    pub fn is_on(self, links: &Links) -> bool {
        match self {
            Self::Symbol => links.symbol,
            Self::Interval => links.interval,
            Self::Crosshair => links.crosshair,
            Self::Time => links.time,
            Self::Range => links.range,
        }
    }

    pub fn toggle(self, links: &mut Links) {
        let flag = match self {
            Self::Symbol => &mut links.symbol,
            Self::Interval => &mut links.interval,
            Self::Crosshair => &mut links.crosshair,
            Self::Time => &mut links.time,
            Self::Range => &mut links.range,
        };
        *flag = !*flag;
    }
}

/// What another chart is told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Follow {
    /// Show this pointer as a crosshair (or clear it). The price is left out (`NaN`) when the
    /// charts show different symbols: only the time means something there.
    Pointer(Option<Hover>),
    /// Show this span of time.
    Span(Span),
    /// Show this time at the right edge.
    RightEdge(i64),
}

/// What the other charts are told when chart `from` (of `symbols.len()`) reports `event`.
/// `symbols` is the symbol of each chart, when it has one.
pub fn route(
    links: &Links,
    from: usize,
    symbols: &[Option<i64>],
    event: &ChartEvent,
) -> Vec<(usize, Follow)> {
    let others = (0..symbols.len()).filter(|&i| i != from);
    match event {
        ChartEvent::Hover(pointer) if links.crosshair => others
            .map(|i| {
                let same = symbols[i].is_some() && symbols[i] == symbols[from];
                let pointer = pointer.map(|p| Hover {
                    time_ms: p.time_ms,
                    price: if same { p.price } else { f64::NAN },
                });
                (i, Follow::Pointer(pointer))
            })
            .collect(),
        ChartEvent::ViewChanged(span) if links.range => {
            others.map(|i| (i, Follow::Span(*span))).collect()
        }
        ChartEvent::ViewChanged(span) if links.time => others
            .map(|i| (i, Follow::RightEdge(span.right_ms)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The charts a new timeframe goes to: all of them when the interval is linked, else the one
/// that asked.
pub fn timeframe_targets(links: &Links, active: usize, count: usize) -> Vec<usize> {
    if links.interval {
        (0..count).collect()
    } else {
        vec![active.min(count.saturating_sub(1))]
    }
}

/// The charts a new symbol goes to: all of them when the symbol is linked, else the one it was
/// picked for.
pub fn symbol_targets(links: &Links, target: usize, count: usize) -> Vec<usize> {
    if links.symbol {
        (0..count).collect()
    } else {
        vec![target.min(count.saturating_sub(1))]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links() -> Links {
        Links {
            symbol: true,
            interval: false,
            crosshair: true,
            time: true,
            range: false,
        }
    }

    const POINTER: Hover = Hover {
        time_ms: 1_000,
        price: 1.5,
    };

    #[test]
    fn a_pointer_goes_to_every_other_chart_with_its_price_only_on_the_same_symbol() {
        let symbols = [Some(1), Some(1), Some(2)];
        let routed = route(&links(), 0, &symbols, &ChartEvent::Hover(Some(POINTER)));
        assert_eq!(routed.len(), 2);
        assert_eq!(routed[0], (1, Follow::Pointer(Some(POINTER))));
        match routed[1] {
            (2, Follow::Pointer(Some(hover))) => {
                assert_eq!(hover.time_ms, 1_000);
                assert!(hover.price.is_nan());
            }
            other => panic!("{other:?}"),
        }
        // Leaving the chart clears the others.
        let cleared = route(&links(), 1, &symbols, &ChartEvent::Hover(None));
        assert!(cleared.iter().all(|(_, f)| *f == Follow::Pointer(None)));
        assert!(cleared.iter().all(|(i, _)| *i != 1));
    }

    #[test]
    fn nothing_is_shared_when_nothing_is_linked() {
        let none = Links {
            symbol: false,
            interval: false,
            crosshair: false,
            time: false,
            range: false,
        };
        let symbols = [Some(1), Some(1)];
        let span = Span {
            left_ms: 0,
            right_ms: 9,
        };
        assert!(route(&none, 0, &symbols, &ChartEvent::Hover(Some(POINTER))).is_empty());
        assert!(route(&none, 0, &symbols, &ChartEvent::ViewChanged(span)).is_empty());
    }

    #[test]
    fn a_range_link_wins_over_the_time_link() {
        let span = Span {
            left_ms: 100,
            right_ms: 900,
        };
        let symbols = [None, None, None];
        let time_only = route(&links(), 1, &symbols, &ChartEvent::ViewChanged(span));
        assert_eq!(
            time_only,
            vec![(0, Follow::RightEdge(900)), (2, Follow::RightEdge(900))]
        );
        let mut both = links();
        both.range = true;
        let ranged = route(&both, 1, &symbols, &ChartEvent::ViewChanged(span));
        assert_eq!(
            ranged,
            vec![(0, Follow::Span(span)), (2, Follow::Span(span))]
        );
    }

    #[test]
    fn other_events_are_never_routed() {
        let symbols = [Some(1), Some(1)];
        for event in [
            ChartEvent::Activated,
            ChartEvent::SettingsChanged,
            ChartEvent::PickSymbol,
            ChartEvent::Screenshot,
        ] {
            assert!(route(&links(), 0, &symbols, &event).is_empty(), "{event:?}");
        }
    }

    #[test]
    fn timeframes_and_symbols_go_where_the_links_say() {
        let mut l = links();
        assert_eq!(timeframe_targets(&l, 2, 4), vec![2]);
        l.interval = true;
        assert_eq!(timeframe_targets(&l, 2, 4), vec![0, 1, 2, 3]);
        assert_eq!(symbol_targets(&l, 1, 3), vec![0, 1, 2]);
        l.symbol = false;
        assert_eq!(symbol_targets(&l, 1, 3), vec![1]);
        assert_eq!(
            symbol_targets(&l, 9, 3),
            vec![2],
            "an index past the end is the last"
        );
    }

    #[test]
    fn links_switch_one_at_a_time() {
        let mut l = links();
        for link in Link::ALL {
            let before = link.is_on(&l);
            link.toggle(&mut l);
            assert_eq!(link.is_on(&l), !before);
        }
    }
}
