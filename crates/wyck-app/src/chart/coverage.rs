//! Which stretches of time a series has already been fetched for.
//!
//! Knowing the bars we hold is not enough to know what to ask the server. A weekend has no bars,
//! and a symbol that starts trading in 2019 has none before it: from the bars alone these look
//! like holes that must be fetched again on every launch. [`Coverage`] records the *asked-for*
//! ranges instead. A range that came back empty is still covered, so it is never requested twice.
//!
//! A range is half open, `[from, to)`, in Unix milliseconds. Ranges that touch or overlap are
//! merged, so the list stays short: in practice one range, growing at both ends.

use wyck_engine::domain::UnixMillis;

/// A half open span of time, `[from, to)`.
pub type Span = (UnixMillis, UnixMillis);

/// The ranges already fetched, sorted, disjoint and never touching.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Coverage {
    spans: Vec<Span>,
}

impl Coverage {
    /// Nothing fetched yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The covered ranges, oldest first.
    #[must_use]
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Whether nothing is covered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// The start of the oldest covered range.
    #[must_use]
    pub fn earliest(&self) -> Option<UnixMillis> {
        self.spans.first().map(|s| s.0)
    }

    /// The end of the newest covered range.
    #[must_use]
    pub fn latest(&self) -> Option<UnixMillis> {
        self.spans.last().map(|s| s.1)
    }

    /// Records `[from, to)` as fetched. An empty or backwards range is ignored.
    pub fn add(&mut self, from: UnixMillis, to: UnixMillis) {
        if to <= from {
            return;
        }
        let (mut from, mut to) = (from, to);
        let mut merged = Vec::with_capacity(self.spans.len() + 1);
        let mut placed = false;
        for &(a, b) in &self.spans {
            if b < from {
                merged.push((a, b));
            } else if a > to {
                if !placed {
                    merged.push((from, to));
                    placed = true;
                }
                merged.push((a, b));
            } else {
                // Overlapping or touching: absorb it.
                from = from.min(a);
                to = to.max(b);
            }
        }
        if !placed {
            merged.push((from, to));
        }
        self.spans = merged;
    }

    /// The parts of `[from, to)` that are not covered, oldest first.
    #[must_use]
    pub fn missing(&self, from: UnixMillis, to: UnixMillis) -> Vec<Span> {
        let mut gaps = Vec::new();
        let mut cursor = from;
        for &(a, b) in &self.spans {
            if cursor >= to {
                break;
            }
            if b <= cursor {
                continue;
            }
            if a > cursor {
                gaps.push((cursor, a.min(to)));
            }
            cursor = cursor.max(b);
        }
        if cursor < to {
            gaps.push((cursor, to));
        }
        gaps
    }

    /// Whether all of `[from, to)` is covered.
    #[must_use]
    pub fn covers(&self, from: UnixMillis, to: UnixMillis) -> bool {
        self.missing(from, to).is_empty()
    }

    /// The list as bytes, 16 per range: `from` then `to`, little endian. It is what the store
    /// keeps.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.spans.len() * 16);
        for (a, b) in &self.spans {
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        out
    }

    /// The list from [`Coverage::to_bytes`]. Bytes that are not a whole number of ranges are
    /// refused, since half a record cannot be trusted; the caller then treats the series as
    /// uncovered and fetches again.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if !bytes.len().is_multiple_of(16) {
            return None;
        }
        let mut coverage = Self::new();
        for chunk in bytes.as_chunks::<16>().0 {
            let from = i64::from_le_bytes(chunk[..8].try_into().ok()?);
            let to = i64::from_le_bytes(chunk[8..].try_into().ok()?);
            coverage.add(from, to);
        }
        Some(coverage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cov(spans: &[Span]) -> Coverage {
        let mut c = Coverage::new();
        for &(a, b) in spans {
            c.add(a, b);
        }
        c
    }

    #[test]
    fn overlapping_and_touching_ranges_merge() {
        assert_eq!(cov(&[(0, 10), (10, 20)]).spans(), &[(0, 20)]);
        assert_eq!(cov(&[(0, 10), (5, 30)]).spans(), &[(0, 30)]);
        assert_eq!(cov(&[(20, 30), (0, 10)]).spans(), &[(0, 10), (20, 30)]);
        assert_eq!(cov(&[(0, 10), (20, 30), (5, 25)]).spans(), &[(0, 30)]);
    }

    #[test]
    fn empty_and_backwards_ranges_are_ignored() {
        assert!(cov(&[(5, 5), (9, 3)]).is_empty());
    }

    #[test]
    fn missing_lists_the_holes() {
        let c = cov(&[(10, 20), (30, 40)]);
        assert_eq!(c.missing(0, 50), vec![(0, 10), (20, 30), (40, 50)]);
        assert_eq!(c.missing(12, 18), vec![]);
        assert_eq!(c.missing(15, 35), vec![(20, 30)]);
        assert_eq!(c.missing(45, 50), vec![(45, 50)]);
        assert_eq!(c.missing(5, 5), vec![]);
    }

    #[test]
    fn an_empty_coverage_misses_everything() {
        assert_eq!(Coverage::new().missing(0, 10), vec![(0, 10)]);
    }

    #[test]
    fn ends_are_reported() {
        let c = cov(&[(10, 20), (30, 40)]);
        assert_eq!((c.earliest(), c.latest()), (Some(10), Some(40)));
        assert!(c.covers(11, 19) && !c.covers(11, 31));
    }

    #[test]
    fn bytes_round_trip_and_a_torn_record_is_refused() {
        let c = cov(&[(10, 20), (30, 40)]);
        assert_eq!(Coverage::from_bytes(&c.to_bytes()), Some(c));
        assert_eq!(Coverage::from_bytes(&[]), Some(Coverage::new()));
        assert_eq!(Coverage::from_bytes(&[1, 2, 3]), None);
    }
}
