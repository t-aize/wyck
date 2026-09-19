//! Plans and "submits" an order against an in-memory broker, to show the whole flow with no
//! network and no risk: connect, plan (sizing), dry-run submit, then arm and send for real
//! against the mock.
//!
//! ```text
//! cargo run -p wyck-engine --example dry_run_order --features testing
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use wyck_engine::broker::{Broker, ConnectRequest, Connector, MockBroker, ServiceKind};
use wyck_engine::domain::{AccountKind, Side};
use wyck_engine::{
    ArmRequest, CalendarSource, Engine, EngineConfig, EngineOptions, EntryIntent, OrderOutcome,
    RiskSpec, SizeSpec, StopSpec, TakeProfitSpec,
};

struct Fixed(Arc<MockBroker>);

#[async_trait]
impl Connector for Fixed {
    async fn connect(&self, _request: &ConnectRequest) -> wyck_engine::Result<Arc<dyn Broker>> {
        Ok(self.0.clone())
    }
}

#[tokio::main]
async fn main() -> wyck_engine::Result<()> {
    let broker = Arc::new(MockBroker::new());
    let engine = Engine::start_with(
        EngineConfig::default(),
        EngineOptions {
            connector: Some(Arc::new(Fixed(Arc::clone(&broker)))),
            calendar: CalendarSource::Disabled,
        },
    )?;
    let handle = engine.handle();

    handle
        .connect(ConnectRequest::new(
            ServiceKind::CtraderRemote,
            "mock://broker",
            None,
        ))
        .await?;
    let state = handle.state();
    println!(
        "connected to account {} ({:?}), balance {:?}, mode {:?}",
        state.account_id.as_ref().unwrap(),
        state.account.as_ref().unwrap().kind,
        state.account.as_ref().unwrap().balance,
        state.mode
    );

    let plan = handle
        .plan_entry(EntryIntent {
            symbol: "EURUSD".into(),
            side: Side::Buy,
            size: SizeSpec::Risk(RiskSpec::PercentOfBalance(1.0)),
            stop_loss: Some(StopSpec::Pips(30.0)),
            take_profit: Some(TakeProfitSpec::RiskReward(2.0)),
        })
        .await?;
    println!(
        "plan: {} {} {} at {}, stop {:?}, target {:?}, risk {:.2} ({:.2}%)",
        plan.side,
        plan.volume,
        plan.symbol,
        plan.entry_reference,
        plan.stop_loss_price,
        plan.take_profit_price,
        plan.risk_amount.unwrap_or_default(),
        plan.risk_percent.unwrap_or_default()
    );

    // Disarmed: a dry run. Nothing reaches the broker.
    let outcome = handle.submit(plan.id).await?;
    println!("disarmed submit -> {outcome:?}");
    println!(
        "orders the broker received: {}",
        broker.call_count(wyck_engine::broker::BrokerCall::PlaceMarket)
    );

    // Arm (acknowledging the account kind), plan again, and send for real.
    handle
        .arm(ArmRequest {
            account: state.account_id.clone().unwrap(),
            acknowledged_kind: AccountKind::Demo,
        })
        .await?;
    let plan = handle
        .plan_entry(EntryIntent {
            symbol: "EURUSD".into(),
            side: Side::Buy,
            size: SizeSpec::Risk(RiskSpec::PercentOfBalance(1.0)),
            stop_loss: Some(StopSpec::Pips(30.0)),
            take_profit: None,
        })
        .await?;
    match handle.submit(plan.id).await? {
        OrderOutcome::Filled { position, .. } => {
            println!(
                "armed submit -> filled: position {} of {}",
                position.id, position.volume
            );
        }
        other => println!("armed submit -> {other:?}"),
    }
    println!(
        "orders the broker received: {}",
        broker.call_count(wyck_engine::broker::BrokerCall::PlaceMarket)
    );

    engine.shutdown().await;
    Ok(())
}
