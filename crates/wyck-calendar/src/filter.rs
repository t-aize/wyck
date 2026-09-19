//! Filtering events by impact, currency and a custom watchlist.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::currency::Currency;
use crate::event::{CalendarEvent, Impact, Scope};

/// A serializable, composable event filter.
///
/// An event is kept when **either**
///
/// 1. it satisfies the *criteria* — `impact >= min_impact` **and** its scope is allowed
///    by `currencies` / `include_global` — **or**
/// 2. its title contains one of the [`watchlist`](Self::watchlist) terms.
///
/// The watchlist is an override, not a further restriction: it exists to say "always
/// show me *FOMC* and *NFP*, whatever the impact/currency filter says".
///
/// Scope rules: an empty `currencies` set means *no currency restriction* (every scope
/// passes, global events included). A non-empty set keeps only events for those
/// currencies, plus global events iff `include_global` is `true`.
///
/// The default filter keeps everything. Every field has a serde default, so a config
/// file written by an older version keeps loading after fields are added.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EventFilter {
    /// Drop events rated below this. `None` keeps every impact level.
    pub min_impact: Option<Impact>,
    /// Currencies to keep (empty = no restriction). See the type docs.
    pub currencies: BTreeSet<Currency>,
    /// With a non-empty `currencies`, whether currency-less events (`BRICS Summit`,
    /// `ECOFIN Meetings`, …) are kept too.
    pub include_global: bool,
    /// Case-insensitive title substrings that always pass, e.g. `"fomc"`, `"nonfarm"`.
    /// Stored trimmed, lower-cased and deduplicated by [`EventFilter::watch`].
    pub watchlist: Vec<String>,
}

impl EventFilter {
    /// A filter that keeps everything. Same as [`Default::default`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep only events rated `impact` or higher.
    #[must_use]
    pub fn with_min_impact(mut self, impact: Impact) -> Self {
        self.min_impact = Some(impact);
        self
    }

    /// Restrict to these currencies (replacing any previous set). Pass
    /// [`crate::currencies_from_symbols`] to default to "what I trade".
    #[must_use]
    pub fn with_currencies(mut self, currencies: impl IntoIterator<Item = Currency>) -> Self {
        self.currencies = currencies.into_iter().collect();
        self
    }

    /// Whether currency-less events pass a non-empty currency restriction.
    #[must_use]
    pub fn with_global(mut self, include_global: bool) -> Self {
        self.include_global = include_global;
        self
    }

    /// Adds a watchlist term. Blank terms and duplicates (ignoring case) are ignored.
    #[must_use]
    pub fn watch(mut self, term: &str) -> Self {
        self.add_watch(term);
        self
    }

    /// In-place variant of [`watch`](Self::watch). Returns whether the term was added.
    pub fn add_watch(&mut self, term: &str) -> bool {
        let term = term.trim().to_lowercase();
        if term.is_empty() || self.watchlist.contains(&term) {
            return false;
        }
        self.watchlist.push(term);
        true
    }

    /// Removes a watchlist term (case-insensitive). Returns whether it was present.
    pub fn remove_watch(&mut self, term: &str) -> bool {
        let term = term.trim().to_lowercase();
        let before = self.watchlist.len();
        self.watchlist.retain(|t| *t != term);
        self.watchlist.len() != before
    }

    /// Whether `event` passes this filter. See the type docs for the exact rule.
    #[must_use]
    pub fn matches(&self, event: &CalendarEvent) -> bool {
        self.matches_criteria(event) || self.matches_watchlist(event)
    }

    /// The events that pass, in their original order.
    pub fn apply<'a>(&self, events: &'a [CalendarEvent]) -> Vec<&'a CalendarEvent> {
        events.iter().filter(|e| self.matches(e)).collect()
    }

    fn matches_criteria(&self, event: &CalendarEvent) -> bool {
        if self.min_impact.is_some_and(|min| event.impact < min) {
            return false;
        }
        if self.currencies.is_empty() {
            return true;
        }
        match event.scope {
            Scope::Global => self.include_global,
            Scope::Currency(c) => self.currencies.contains(&c),
        }
    }

    fn matches_watchlist(&self, event: &CalendarEvent) -> bool {
        if self.watchlist.is_empty() {
            return false;
        }
        let title = event.title.to_lowercase();
        self.watchlist.iter().any(|term| title.contains(term))
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::event::testing::event;

    fn ev(title: &str, scope: Scope, impact: Impact) -> CalendarEvent {
        event(title, scope, datetime!(2026-09-14 08:30 -4), impact)
    }

    fn usd() -> Scope {
        Scope::Currency(Currency::USD)
    }

    fn eur() -> Scope {
        Scope::Currency(Currency::EUR)
    }

    #[test]
    fn default_keeps_everything() {
        let f = EventFilter::default();
        assert!(f.matches(&ev("a", Scope::Global, Impact::Unknown)));
        assert!(f.matches(&ev("a", usd(), Impact::Low)));
    }

    #[test]
    fn min_impact_is_inclusive() {
        let f = EventFilter::new().with_min_impact(Impact::Medium);
        assert!(!f.matches(&ev("a", usd(), Impact::Low)));
        assert!(f.matches(&ev("a", usd(), Impact::Medium)));
        assert!(f.matches(&ev("a", usd(), Impact::High)));
    }

    #[test]
    fn currency_restriction_keeps_only_listed_currencies() {
        let f = EventFilter::new().with_currencies([Currency::USD]);
        assert!(f.matches(&ev("a", usd(), Impact::Low)));
        assert!(!f.matches(&ev("a", eur(), Impact::High)));
    }

    #[test]
    fn global_events_need_include_global_only_when_currencies_are_restricted() {
        let global = ev("BRICS Summit", Scope::Global, Impact::Low);
        assert!(EventFilter::new().matches(&global), "no restriction");
        let restricted = EventFilter::new().with_currencies([Currency::USD]);
        assert!(!restricted.matches(&global));
        assert!(restricted.with_global(true).matches(&global));
    }

    #[test]
    fn criteria_combine_with_and() {
        let f = EventFilter::new()
            .with_min_impact(Impact::High)
            .with_currencies([Currency::USD]);
        assert!(f.matches(&ev("a", usd(), Impact::High)));
        assert!(!f.matches(&ev("a", usd(), Impact::Medium)));
        assert!(!f.matches(&ev("a", eur(), Impact::High)));
    }

    #[test]
    fn watchlist_overrides_the_criteria() {
        let f = EventFilter::new()
            .with_min_impact(Impact::High)
            .with_currencies([Currency::USD])
            .watch("  Lagarde ");
        let speech = ev("ECB President Lagarde Speaks", eur(), Impact::Medium);
        assert!(f.matches(&speech));
        assert!(!f.matches(&ev("ECB Press Conference", eur(), Impact::Medium)));
    }

    #[test]
    fn watchlist_terms_are_normalized_and_deduplicated() {
        let mut f = EventFilter::new();
        assert!(f.add_watch(" FOMC "));
        assert!(!f.add_watch("fomc"));
        assert!(!f.add_watch("   "));
        assert_eq!(f.watchlist, ["fomc"]);
        assert!(f.remove_watch("FOMC"));
        assert!(!f.remove_watch("fomc"));
        assert!(f.watchlist.is_empty());
    }

    #[test]
    fn apply_preserves_order() {
        let events = vec![
            ev("1", usd(), Impact::High),
            ev("2", eur(), Impact::High),
            ev("3", usd(), Impact::Low),
            ev("4", usd(), Impact::Medium),
        ];
        let f = EventFilter::new()
            .with_currencies([Currency::USD])
            .with_min_impact(Impact::Medium);
        let titles: Vec<_> = f.apply(&events).iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["1", "4"]);
    }

    #[test]
    fn serde_round_trips_and_tolerates_missing_fields() {
        let f = EventFilter::new()
            .with_min_impact(Impact::High)
            .with_currencies([Currency::EUR, Currency::USD])
            .with_global(true)
            .watch("nfp");
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(serde_json::from_str::<EventFilter>(&json).unwrap(), f);
        assert_eq!(
            serde_json::from_str::<EventFilter>("{}").unwrap(),
            EventFilter::default()
        );
        assert_eq!(
            serde_json::from_str::<EventFilter>(r#"{"min_impact":"high"}"#)
                .unwrap()
                .min_impact,
            Some(Impact::High)
        );
    }
}
