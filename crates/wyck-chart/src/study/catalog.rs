//! Every indicator that can be added to a chart, in one list: the ones the app ships and the ones
//! the user wrote as scripts. The menus that add an indicator read this, and nothing else needs
//! to know which is which.

use super::custom::library::registry;
use super::{Placement, StudyConfig, StudyKind};

/// Where an indicator comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// One the app ships.
    Builtin(StudyKind),
    /// A script, by its id.
    Script(String),
}

/// One indicator that can be added.
#[derive(Debug, Clone)]
pub struct Item {
    pub source: Source,
    pub label: String,
    pub short: String,
    pub category: String,
    pub description: String,
    pub placement: Placement,
    pub author: String,
    /// Whether it can be added: false for a script that has a mistake.
    pub ready: bool,
    /// How many problems a script has.
    pub problems: usize,
}

impl Item {
    /// A stable name to remember the indicator by (a favorite): it does not change when the
    /// wording of a menu does.
    pub fn key(&self) -> String {
        match &self.source {
            Source::Builtin(kind) => format!("builtin:{}", builtin_code(*kind)),
            Source::Script(id) => format!("script:{id}"),
        }
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self.source, Source::Builtin(_))
    }

    /// The indicator with every input and plot at its default, to put on a chart.
    pub fn config(&self) -> StudyConfig {
        match &self.source {
            Source::Builtin(kind) => StudyConfig::new(*kind),
            Source::Script(id) => StudyConfig::for_script(id),
        }
    }

    /// Whether `query` (words, any case) is in the name, the short name, the category or the
    /// description.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {} {}",
            self.label, self.short, self.category, self.description, self.author
        )
        .to_lowercase();
        query.split_whitespace().all(|word| haystack.contains(word))
    }
}

/// The name a built-in indicator is remembered by. It never changes.
fn builtin_code(kind: StudyKind) -> String {
    toml::Value::try_from(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The category and the one line a menu shows for a built-in indicator.
pub fn builtin_info(kind: StudyKind) -> (&'static str, &'static str) {
    match kind {
        StudyKind::Sma => (
            "Trend",
            "The average of the last prices, each counting the same.",
        ),
        StudyKind::Ema => ("Trend", "An average that counts the newest prices more."),
        StudyKind::Wma => (
            "Trend",
            "An average that weighs each price by how recent it is.",
        ),
        StudyKind::Hma => ("Trend", "A fast average with very little lag."),
        StudyKind::Vwap => (
            "Volume",
            "The average price weighted by volume, starting over every day.",
        ),
        StudyKind::Bollinger => (
            "Volatility",
            "An average with bands at a number of standard deviations.",
        ),
        StudyKind::Keltner => (
            "Volatility",
            "An average with bands at a number of average true ranges.",
        ),
        StudyKind::Donchian => (
            "Volatility",
            "The highest high and the lowest low of the last bars.",
        ),
        StudyKind::Ichimoku => (
            "Trend",
            "Five lines that show trend, momentum and support in one look.",
        ),
        StudyKind::ParabolicSar => ("Trend", "Dots that trail the price and flip when it turns."),
        StudyKind::VolumeProfile => (
            "Volume",
            "How much traded at each price, on the bars in view.",
        ),
        StudyKind::Volume => ("Volume", "The volume of each bar, with its average."),
        StudyKind::Rsi => (
            "Momentum",
            "How strong recent gains are against recent losses, from 0 to 100.",
        ),
        StudyKind::Macd => (
            "Momentum",
            "The gap between two averages, its signal and their difference.",
        ),
        StudyKind::Stochastic => (
            "Momentum",
            "Where the close sits in the recent range, from 0 to 100.",
        ),
        StudyKind::Atr => ("Volatility", "The average size of a bar's range."),
        StudyKind::Cci => (
            "Momentum",
            "How far the price is from its average, in units of its spread.",
        ),
        StudyKind::WilliamsR => (
            "Momentum",
            "Where the close sits in the recent range, from -100 to 0.",
        ),
        StudyKind::Momentum => ("Momentum", "The change in price over some bars."),
        StudyKind::Obv => (
            "Volume",
            "A running total of volume, added on rises and taken away on falls.",
        ),
        StudyKind::Dmi => (
            "Trend",
            "The strength of a trend (ADX) and which side leads (+DI and -DI).",
        ),
        StudyKind::Custom | StudyKind::Unknown => ("", ""),
    }
}

/// The order categories are listed in: the ones the app ships first, then the others by name.
pub const CATEGORY_ORDER: [&str; 4] = ["Trend", "Momentum", "Volatility", "Volume"];

/// Every indicator that can be added, the ones the app ships and then the scripts, each group by
/// category and name.
pub fn items() -> Vec<Item> {
    let mut items: Vec<Item> = StudyKind::ALL
        .into_iter()
        .map(|kind| {
            let spec = kind.spec();
            let (category, description) = builtin_info(kind);
            Item {
                source: Source::Builtin(kind),
                label: spec.label.to_owned(),
                short: spec.short.to_owned(),
                category: category.to_owned(),
                description: description.to_owned(),
                placement: spec.placement,
                author: String::new(),
                ready: true,
                problems: 0,
            }
        })
        .collect();
    items.sort_by(|a, b| (rank(&a.category), &a.label).cmp(&(rank(&b.category), &b.label)));
    let mut scripts: Vec<Item> = registry::all()
        .iter()
        .map(|entry| Item {
            source: Source::Script(entry.id.clone()),
            label: entry.info.name.clone(),
            short: entry.info.short.clone(),
            category: entry.info.category.clone(),
            description: entry.info.description.clone(),
            placement: entry.spec.placement,
            author: entry.info.author.clone(),
            ready: entry.is_ready(),
            problems: entry.problems.len(),
        })
        .collect();
    scripts.sort_by(|a, b| (&a.category, &a.label).cmp(&(&b.category, &b.label)));
    items.extend(scripts);
    items
}

fn rank(category: &str) -> usize {
    CATEGORY_ORDER
        .iter()
        .position(|c| *c == category)
        .unwrap_or(CATEGORY_ORDER.len())
}

/// The categories of `items`, in the order they are listed.
pub fn categories(items: &[Item]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if !out.contains(&item.category) {
            out.push(item.category.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_is_listed_once_with_a_category_and_a_description() {
        let items = items();
        let builtin: Vec<&Item> = items.iter().filter(|i| i.is_builtin()).collect();
        assert_eq!(builtin.len(), StudyKind::ALL.len());
        for item in &builtin {
            assert!(
                !item.category.is_empty() && !item.description.is_empty(),
                "{}",
                item.label
            );
            assert!(item.ready);
        }
        let mut keys: Vec<String> = items.iter().map(Item::key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), items.len(), "every indicator has its own key");
        assert!(builtin.iter().any(|i| i.key() == "builtin:rsi"));
    }

    #[test]
    fn the_builtin_categories_come_in_the_usual_order_before_the_scripts() {
        let cats = categories(&items());
        assert_eq!(&cats[..4], &CATEGORY_ORDER);
    }

    #[test]
    fn a_search_reads_the_name_the_short_name_and_the_words_around_it() {
        let rsi = items()
            .into_iter()
            .find(|i| i.key() == "builtin:rsi")
            .unwrap();
        assert!(rsi.matches("rsi") && rsi.matches("relative") || rsi.matches("momentum"));
        assert!(rsi.matches("  MOMENTUM  strength"));
        assert!(!rsi.matches("bollinger"));
        assert!(rsi.matches(""));
    }

    #[test]
    fn an_item_makes_the_config_that_goes_on_a_chart() {
        let sma = items()
            .into_iter()
            .find(|i| i.key() == "builtin:sma")
            .unwrap();
        assert_eq!(sma.config().kind, StudyKind::Sma);
        let script = Item {
            source: Source::Script("x/y".to_owned()),
            label: String::new(),
            short: String::new(),
            category: String::new(),
            description: String::new(),
            placement: Placement::Pane,
            author: String::new(),
            ready: true,
            problems: 0,
        };
        assert_eq!(script.config().script.as_deref(), Some("x/y"));
        assert_eq!(script.key(), "script:x/y");
    }
}
