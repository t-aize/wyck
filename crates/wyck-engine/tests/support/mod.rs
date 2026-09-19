//! Test support: a scripted in-process Remote MCP server, so the real `RemoteBroker` (and
//! through it the real `ctrader-mcp` transport) can be exercised without a network.
//!
//! The world it simulates is small but stateful: opening, amending and closing positions
//! through the tools changes what `get_positions` returns, and every mutating request is
//! recorded so tests can assert what was, and was not, sent.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use ctrader_mcp::test_support::{MockMcpServer, spawn_mock_mcp_server};
use rmcp::model::CallToolResult;
use serde_json::{Value, json};
use tokio::task::JoinHandle;

/// Shared, inspectable state of the simulated server.
#[derive(Debug, Default)]
pub struct World {
    pub positions: Vec<Value>,
    pub next_position_id: i64,
    /// `(tool, arguments)` of every mutating call received, in order.
    pub mutations: Vec<(String, Value)>,
    /// Number of `get_spot_prices` requests, and the ids of the last one.
    pub last_price_ids: Vec<i64>,
}

pub struct RemoteScenario {
    pub url: String,
    pub world: Arc<Mutex<World>>,
    pub handle: JoinHandle<()>,
}

impl RemoteScenario {
    pub fn mutation_count(&self) -> usize {
        self.world.lock().unwrap().mutations.len()
    }

    pub fn mutations(&self) -> Vec<(String, Value)> {
        self.world.lock().unwrap().mutations.clone()
    }
}

impl Drop for RemoteScenario {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

fn args(arguments: Option<rmcp::model::JsonObject>) -> Value {
    Value::Object(arguments.unwrap_or_default())
}

fn side_of(value: &Value) -> String {
    value["tradeSide"]
        .as_str()
        .unwrap_or("BUY")
        .to_ascii_uppercase()
}

/// Starts the server. `trading` controls whether the mutating tools are advertised (the
/// `trading` profile) or only the read tools (the `data` profile).
pub async fn remote_scenario(trading: bool) -> RemoteScenario {
    let world = Arc::new(Mutex::new(World {
        next_position_id: 100,
        ..World::default()
    }));

    let mut builder = MockMcpServer::builder()
        .with_tool("get_version", |_| {
            CallToolResult::structured(json!({
                "version": "1.0.18", "build_time": "2026-01-01T00:00:00Z", "service": "rest-proxy"
            }))
        })
        .with_tool("get_server_time", |_| {
            CallToolResult::structured(json!({ "timestamp": 1_789_389_000_000_i64 }))
        })
        .with_tool("get_balance", |_| {
            CallToolResult::structured(json!({
                "trader_id": 42, "balance": 1_000_000, "equity": 1_000_000,
                "free_margin": 1_000_000, "money_digits": 2, "deposit_asset_id": 1
            }))
        })
        .with_tool("get_assets", |_| {
            CallToolResult::structured(json!({
                "assets": [
                    {"asset_id": 1, "name": "USD"},
                    {"asset_id": 2, "name": "EUR"},
                    {"asset_id": 3, "name": "JPY"}
                ]
            }))
        })
        .with_tool("get_symbols", |_| {
            CallToolResult::structured(json!({
                "symbols": [
                    {"symbol_id": 1, "symbol_name": "EURUSD", "enabled": true,
                     "base_asset_id": 2, "quote_asset_id": 1, "pip_digits": 5},
                    {"symbol_id": 2, "symbol_name": "USDJPY", "enabled": true,
                     "base_asset_id": 1, "quote_asset_id": 3, "pip_digits": 3}
                ]
            }))
        });

    {
        let world = Arc::clone(&world);
        builder = builder.with_tool("get_spot_prices", move |arguments| {
            let ids: Vec<i64> = args(arguments)["symbolId"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default();
            world.lock().unwrap().last_price_ids.clone_from(&ids);
            // Q-R8: one unknown id empties the whole batch.
            if ids.iter().any(|id| !(1..=2).contains(id)) {
                return CallToolResult::structured(json!({ "prices": [] }));
            }
            let prices: Vec<Value> = ids
                .iter()
                .map(|id| match id {
                    1 => json!({"symbol_id": 1, "bid": 108_499, "ask": 108_501, "timestamp": 1_789_389_000_000_i64}),
                    _ => json!({"symbol_id": 2, "bid": 150_123, "ask": 150_125, "timestamp": 1_789_389_000_000_i64}),
                })
                .collect();
            CallToolResult::structured(json!({ "prices": prices }))
        });
    }
    {
        let world = Arc::clone(&world);
        builder = builder.with_tool("get_positions", move |_| {
            let w = world.lock().unwrap();
            CallToolResult::structured(json!({ "positions": w.positions, "orders": [] }))
        });
    }
    {
        let world = Arc::clone(&world);
        builder = builder.with_tool("get_position_details", move |arguments| {
            let id = args(arguments)["positionId"].as_i64().unwrap_or(-1);
            let w = world.lock().unwrap();
            let position = w.positions.iter().find(|p| p["position_id"] == id).cloned();
            CallToolResult::structured(json!({ "position": position, "orders": [], "deals": [] }))
        });
    }

    if trading {
        {
            let world = Arc::clone(&world);
            builder = builder.with_tool("create_order", move |arguments| {
                let a = args(arguments);
                let mut w = world.lock().unwrap();
                w.mutations.push(("create_order".to_owned(), a.clone()));
                let (entry, sign) = if side_of(&a) == "BUY" {
                    (108_501_i64, 1)
                } else {
                    (108_499, -1)
                };
                let symbol_id = a["symbolId"].as_i64().unwrap_or(1);
                let rel = |key: &str| a[key].as_i64();
                let id = w.next_position_id;
                w.next_position_id += 1;
                let position = json!({
                    "position_id": id, "symbol_id": symbol_id, "trade_side": side_of(&a),
                    "volume": a["volume"], "entry_price": entry,
                    "stop_loss": rel("relativeStopLoss").map(|p| entry - sign * p),
                    "take_profit": rel("relativeTakeProfit").map(|p| entry + sign * p),
                    "swap": 0, "commission": 0, "unrealized_pnl": 0,
                    "label": a["label"]
                });
                w.positions.push(position.clone());
                CallToolResult::structured(
                    json!({ "position": position, "order": {"order_id": id} }),
                )
            });
        }
        {
            let world = Arc::clone(&world);
            builder = builder.with_tool("amend_position", move |arguments| {
                let a = args(arguments);
                let mut w = world.lock().unwrap();
                w.mutations.push(("amend_position".to_owned(), a.clone()));
                let id = a["positionId"].as_i64().unwrap_or(-1);
                let Some(p) = w.positions.iter_mut().find(|p| p["position_id"] == id) else {
                    return CallToolResult::structured_error(json!({"error": "no such position"}));
                };
                p["stop_loss"] = a["stopLoss"].clone();
                p["take_profit"] = a["takeProfit"].clone();
                let updated = p.clone();
                CallToolResult::structured(json!({ "position": updated }))
            });
        }
        {
            let world = Arc::clone(&world);
            builder = builder.with_tool("close_position", move |arguments| {
                let a = args(arguments);
                let mut w = world.lock().unwrap();
                w.mutations.push(("close_position".to_owned(), a.clone()));
                let id = a["positionId"].as_i64().unwrap_or(-1);
                let close = a["volume"].as_i64().unwrap_or(0);
                let Some(index) = w.positions.iter().position(|p| p["position_id"] == id) else {
                    return CallToolResult::structured_error(json!({"error": "no such position"}));
                };
                let open = w.positions[index]["volume"].as_i64().unwrap_or(0);
                if close >= open {
                    w.positions.remove(index);
                } else {
                    w.positions[index]["volume"] = json!(open - close);
                }
                CallToolResult::structured(json!({ "deal": {} }))
            });
        }
        builder = builder.with_tool("amend_order", |_| CallToolResult::structured(json!({})));
        {
            let world = Arc::clone(&world);
            builder = builder.with_tool("cancel_order", move |arguments| {
                world
                    .lock()
                    .unwrap()
                    .mutations
                    .push(("cancel_order".to_owned(), args(arguments)));
                CallToolResult::structured(json!({}))
            });
        }
    }

    let (url, handle) = spawn_mock_mcp_server(builder.build()).await;
    RemoteScenario { url, world, handle }
}

// ---------------------------------------------------------------------------------------
// Engine-level helpers
// ---------------------------------------------------------------------------------------

use std::collections::VecDeque;

use async_trait::async_trait;
use wyck_engine::EngineError;
use wyck_engine::broker::{Broker, ConnectRequest, Connector, MockBroker, ServiceKind};

/// A connector that hands out pre-made mock brokers, one per connect call, and counts calls.
pub struct MockConnector {
    queue: Mutex<VecDeque<Result<Arc<MockBroker>, EngineError>>>,
    last: Mutex<Option<Result<Arc<MockBroker>, EngineError>>>,
    pub connects: Mutex<usize>,
}

impl MockConnector {
    pub fn new(first: Arc<MockBroker>) -> Arc<Self> {
        Arc::new(Self {
            queue: Mutex::new(VecDeque::from([Ok(first)])),
            last: Mutex::new(None),
            connects: Mutex::new(0),
        })
    }

    pub fn then(self: &Arc<Self>, next: Result<Arc<MockBroker>, EngineError>) {
        self.queue.lock().unwrap().push_back(next);
    }
}

#[async_trait]
impl Connector for MockConnector {
    async fn connect(&self, _request: &ConnectRequest) -> Result<Arc<dyn Broker>, EngineError> {
        *self.connects.lock().unwrap() += 1;
        // Serve queued entries in order; once the queue is empty keep serving the last one, so
        // a reconnect loop is never starved by the test's own bookkeeping.
        let next = {
            let mut q = self.queue.lock().unwrap();
            let mut last = self.last.lock().unwrap();
            match q.pop_front() {
                Some(entry) => {
                    *last = Some(clone_entry(&entry));
                    Some(entry)
                }
                None => last.as_ref().map(clone_entry),
            }
        };
        match next {
            Some(Ok(broker)) => Ok(broker as Arc<dyn Broker>),
            Some(Err(e)) => Err(e),
            None => Err(EngineError::Internal("no broker queued".into())),
        }
    }
}

fn clone_entry(
    entry: &Result<Arc<MockBroker>, EngineError>,
) -> Result<Arc<MockBroker>, EngineError> {
    match entry {
        Ok(b) => Ok(Arc::clone(b)),
        Err(e) => Err(e.clone()),
    }
}

pub fn request() -> ConnectRequest {
    ConnectRequest::new(ServiceKind::CtraderRemote, "mock://broker", None)
}
