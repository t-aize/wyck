//! The engine hosting a calendar: news warnings for what the person trades, and their
//! appearance next to an order plan.

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{MockConnector, request};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::runtime::Handle;
use wyck_calendar::{CalendarService, Fetch, FetchOutcome, ServiceConfig, Validators, parse_feed};
use wyck_engine::broker::MockBroker;
use wyck_engine::state::{NewsFreshness, WarningKind};
use wyck_engine::{
    CalendarSource, Engine, EngineConfig, EngineOptions, EntryIntent, RiskSpec, SizeSpec, StopSpec,
};

/// A calendar source that serves one fixed feed.
struct StaticFeed(String);

impl Fetch for StaticFeed {
    async fn fetch(&self, _validators: Option<&Validators>) -> wyck_calendar::Result<FetchOutcome> {
        Ok(FetchOutcome::Modified {
            feed: parse_feed(self.0.as_bytes())?,
            validators: None,
        })
    }
}

fn in_minutes(minutes: i64) -> String {
    (OffsetDateTime::now_utc() + time::Duration::minutes(minutes))
        .format(&Rfc3339)
        .unwrap()
}

fn feed() -> String {
    format!(
        r#"[
          {{"title":"Federal Funds Rate","country":"USD","date":"{}","impact":"High"}},
          {{"title":"BOJ Policy Rate","country":"JPY","date":"{}","impact":"High"}},
          {{"title":"Building Permits","country":"USD","date":"{}","impact":"Low"}}
        ]"#,
        in_minutes(10),
        in_minutes(12),
        in_minutes(5)
    )
}

async fn sleep(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

#[tokio::test(start_paused = true)]
async fn news_warnings_follow_the_currencies_being_traded() {
    let calendar = CalendarService::spawn(StaticFeed(feed()), ServiceConfig::default());
    let broker = Arc::new(MockBroker::new());
    let engine = Engine::start_on(
        Handle::current(),
        EngineConfig::default(),
        EngineOptions {
            connector: Some(MockConnector::new(broker)),
            calendar: CalendarSource::Custom(calendar),
        },
    )
    .unwrap();
    let h = engine.handle();
    h.connect(request()).await.unwrap();
    h.watch_symbols(["EURUSD".to_owned()]);

    sleep(16).await;

    let s = h.state();
    assert_eq!(s.news.freshness, NewsFreshness::Fresh);
    let news: Vec<&str> = s
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::News)
        .map(|w| w.message.as_str())
        .collect();
    assert_eq!(news.len(), 1, "{news:?}");
    assert!(news[0].contains("Federal Funds Rate (USD)"), "{news:?}");
    let upcoming: Vec<&str> = s.news.upcoming.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(
        upcoming,
        ["Federal Funds Rate"],
        "low impact and other currencies are filtered out"
    );

    // The warning also shows up next to an order plan for a USD pair.
    let plan = h
        .plan_entry(EntryIntent {
            symbol: "EURUSD".into(),
            side: wyck_engine::domain::Side::Buy,
            size: SizeSpec::Risk(RiskSpec::PercentOfBalance(1.0)),
            stop_loss: Some(StopSpec::Pips(30.0)),
            take_profit: None,
        })
        .await
        .unwrap();
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("Federal Funds Rate")),
        "{:?}",
        plan.warnings
    );

    // Switch to a yen pair: the BOJ replaces the Fed, and the old warning is cleared.
    h.watch_symbols(["GBPJPY".to_owned()]);
    sleep(16).await;
    let s = h.state();
    let news: Vec<&str> = s
        .warnings
        .iter()
        .filter(|w| w.kind == WarningKind::News)
        .map(|w| w.message.as_str())
        .collect();
    assert_eq!(news.len(), 1, "{news:?}");
    assert!(news[0].contains("BOJ Policy Rate (JPY)"), "{news:?}");
    engine.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn without_a_calendar_the_news_view_is_disabled_and_nothing_warns() {
    let engine = Engine::start_on(
        Handle::current(),
        EngineConfig::default(),
        EngineOptions {
            connector: Some(MockConnector::new(Arc::new(MockBroker::new()))),
            calendar: CalendarSource::Disabled,
        },
    )
    .unwrap();
    let h = engine.handle();
    h.connect(request()).await.unwrap();
    sleep(20).await;
    assert_eq!(h.state().news.freshness, NewsFreshness::Disabled);
    assert!(h.state().warnings.is_empty());
}
