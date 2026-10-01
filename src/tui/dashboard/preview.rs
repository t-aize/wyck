use super::*;
use crate::openapi::market::SpotEvent;
use serde::de::DeserializeOwned;
use serde_json::json;

fn sample<T: DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("valid preview data")
}

impl Dashboard {
    pub fn preview(tx: UnboundedSender<Msg>) -> Self {
        let mut dashboard = Self {
            label: "Sample account".to_owned(),
            environment: Environment::Demo,
            session: None,
            docs: None,
            risk: RiskPrefs::default(),
            status: "Preview".to_owned(),
            symbols: Vec::new(),
            book: AccountBook::default(),
            quotes: SpotTracker::new(),
            price_history: BTreeMap::new(),
            spread_history: BTreeMap::new(),
            watch: vec![1, 2, 3],
            wanted: Vec::new(),
            tab: Tab::Watchlist,
            selected: [0; 4],
            popup: Popup::None,
            console: Console::default(),
            loading: false,
            load_id: 0,
            busy: false,
            tx: Sender::new(tx, 0),
        };
        dashboard.book.currency = "USD".to_owned();
        dashboard.book.trader = Some(sample(
            json!({"ctidTraderAccountId": 1, "balance": 2500000, "moneyDigits": 2}),
        ));
        for (id, name, bid, ask, contract) in [
            (1, "EURUSD", 108542, 108550, Contract::default()),
            (
                2,
                "XAUUSD",
                265432000,
                265467000,
                Contract {
                    digits: 2,
                    pip_position: 2,
                    lot_size: 10000,
                    min_volume: 100,
                    max_volume: 1000000,
                    step_volume: 100,
                },
            ),
            (
                3,
                "US100",
                2012450000,
                2012520000,
                Contract {
                    digits: 1,
                    pip_position: 1,
                    lot_size: 100,
                    min_volume: 100,
                    max_volume: 100000,
                    step_volume: 100,
                },
            ),
        ] {
            dashboard.book.names.insert(id, name.to_owned());
            dashboard.book.contracts.insert(id, contract);
            dashboard.symbols.push(sample(
                json!({"symbolId": id, "symbolName": name, "enabled": true}),
            ));
            for delta in [-15, -10, -14, -5, -2, -7, 0] {
                dashboard.on_event(Event::Spot(SpotEvent {
                    ctid_trader_account_id: Some(1),
                    symbol_id: id,
                    bid: Some(bid + delta),
                    ask: Some(ask + delta),
                    session_close: None,
                    timestamp: Some(now_ms()),
                }));
            }
        }
        dashboard.book.reconcile(
            vec![sample(json!({"positionId": 1042, "positionStatus": 1, "tradeData": {"symbolId": 1, "volume": 1000000, "tradeSide": 1}, "price": 1.08400, "stopLoss": 1.08150, "takeProfit": 1.08900, "usedMargin": 3620, "moneyDigits": 2}))],
            vec![sample(json!({"orderId": 2084, "orderType": 2, "orderStatus": 1, "tradeData": {"symbolId": 2, "volume": 100, "tradeSide": 2}, "limitPrice": 2660.00, "stopLoss": 2668.00, "takeProfit": 2640.00}))],
        );
        dashboard.console.push(Tone::Info, "wyck terminal");
        dashboard
            .console
            .push(Tone::Warning, "PREVIEW / Sample prices / Trading disabled");
        dashboard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn dashboard() -> Dashboard {
        Dashboard::preview(tokio::sync::mpsc::unbounded_channel().0)
    }

    #[test]
    fn all_views_and_popups_render_at_small_and_large_sizes() {
        let mut dashboard = dashboard();
        for (width, height) in [
            (140, 42),
            (100, 30),
            (80, 24),
            (40, 16),
            (32, 12),
            (12, 5),
            (1, 1),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            for tab in TABS {
                dashboard.tab = tab;
                terminal.draw(|frame| dashboard.draw(frame)).unwrap();
                if width >= 32 && height >= 12 {
                    let text: String = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .map(|c| c.symbol())
                        .collect();
                    assert!(text.contains("WYCK"), "{width}x{height}");
                    assert!(text.contains("/help"), "{width}x{height}");
                }
            }
            dashboard.open_ticket(TradeSide::Buy);
            terminal.draw(|frame| dashboard.draw(frame)).unwrap();
            dashboard.popup = Popup::None;
        }
    }

    #[test]
    fn commands_only_prepare_trades_and_preview_never_connects() {
        let mut dashboard = dashboard();
        dashboard.execute_command("/buy EURUSD 0.02 --sl 1.08");
        assert!(matches!(dashboard.popup, Popup::Ticket(_)));
        dashboard.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let Popup::Ticket(ticket) = &dashboard.popup else {
            panic!("ticket should remain open");
        };
        assert!(ticket.form.error.is_some());
        assert!(dashboard.session.is_none());
        assert!(!dashboard.busy);
    }

    #[test]
    fn confirmation_survives_navigation_and_only_accepts_explicit_consent() {
        let mut dashboard = dashboard();
        dashboard.execute_command("/close 1042");
        dashboard.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert!(matches!(
            dashboard.popup,
            Popup::Confirm(Confirm::Close(1042, _))
        ));
        dashboard.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(dashboard.popup, Popup::None));
    }

    #[test]
    fn an_old_account_load_cannot_replace_a_reconnecting_dashboard() {
        let mut dashboard = dashboard();
        dashboard.loading = true;
        dashboard.load_id = 3;
        dashboard.on_loaded(2, Err("stale error".into()));
        assert!(dashboard.loading);
        assert_eq!(dashboard.status, "Preview");
        dashboard.on_loaded(3, Err("current error".into()));
        assert!(!dashboard.loading);
        assert_eq!(dashboard.status, "Load failed");
    }

    #[test]
    fn preview_sign_out_cannot_reach_the_real_login_flow() {
        let mut dashboard = dashboard();
        dashboard.execute_command("/logout");
        assert!(matches!(
            dashboard.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Outcome::None
        ));
        assert!(dashboard.session.is_none());
    }
}
