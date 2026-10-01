use super::*;
use crate::tui::header::{Header, Market};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Tabs, Wrap,
};

impl Dashboard {
    // ---- drawing ----

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(theme::base()), area);
        if area.width < 32 || area.height < 12 {
            frame.render_widget(
                Paragraph::new("wyck\nTerminal too small\nMinimum: 32 columns, 12 rows")
                    .style(theme::base())
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let [header, tabs, body, log, input] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(2),
            Constraint::Length(if self.tab == Tab::Console || area.height < 18 {
                0
            } else if area.height < 24 {
                3
            } else {
                7
            }),
            Constraint::Length(5),
        ])
        .areas(area);

        self.draw_header(frame, header);
        let titles = [
            format!("Watchlist {}", self.watch.len()),
            format!("Positions {}", self.book.positions.len()),
            format!("Orders {}", self.book.orders.len()),
            "Console".to_owned(),
        ];
        frame.render_widget(
            Tabs::new(titles)
                .select(self.tab_index())
                .style(Style::default().fg(theme::MUTED))
                .highlight_style(theme::bold(theme::ACCENT))
                .divider("  ")
                .block(
                    Block::default()
                        .borders(Borders::BOTTOM)
                        .border_style(Style::default().fg(theme::BORDER)),
                ),
            tabs,
        );
        match self.tab {
            Tab::Watchlist => self.draw_watchlist(frame, body),
            Tab::Positions => self.draw_positions(frame, body),
            Tab::Orders => self.draw_orders(frame, body),
            Tab::Console => self.console.draw_log(frame, body),
        }
        self.console.draw_log(frame, log);
        let hints = if self.busy {
            " Working..."
        } else if self.console.focused {
            " Command   /help   Shift+Tab: tables   F1-F4: view"
        } else {
            match self.tab {
                Tab::Watchlist => " Watchlist   a add   d remove   b buy   s sell   / command",
                Tab::Positions => " Positions   x close   / command",
                Tab::Orders => " Orders   c cancel   / command",
                Tab::Console => " Console   PgUp/PgDn scroll   / command",
            }
        };
        self.console.draw_input(
            frame,
            input,
            self.console.focused && matches!(self.popup, Popup::None),
            hints,
        );
        self.draw_popup(frame, area);
    }

    fn table<'a>(
        &self,
        rows: Vec<Row<'a>>,
        widths: &[u16],
        header: Vec<&'a str>,
        title: &'a str,
        selected: usize,
    ) -> (Table<'a>, TableState) {
        let total: u32 = widths.iter().map(|w| u32::from(*w)).sum();
        let table = Table::new(
            rows,
            widths
                .iter()
                .map(|w| Constraint::Ratio(u32::from(*w), total)),
        )
        .column_spacing(1)
        .header(
            Row::new(header)
                .style(theme::bold(theme::MUTED))
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .title(title)
                .title_style(theme::bold(theme::TEXT)),
        )
        .highlight_symbol("> ")
        .row_highlight_style(Style::default().bg(theme::SELECTED).fg(theme::TEXT));
        (table, TableState::default().with_selected(Some(selected)))
    }

    fn draw_watchlist(&self, frame: &mut Frame, area: Rect) {
        if self.watch.is_empty() {
            frame.render_widget(
                Paragraph::new("No symbols")
                    .style(Style::default().fg(theme::MUTED))
                    .block(
                        Block::default()
                            .title(" Watchlist ")
                            .title_style(theme::bold(theme::TEXT)),
                    ),
                area,
            );
            return;
        }
        let compact = area.width < 55;
        let rows: Vec<Row> = self
            .watch
            .iter()
            .map(|id| {
                let contract = self.book.contract(*id);
                let spot = self.quotes.get(*id);
                let price = |raw: Option<i64>| {
                    raw.map_or_else(|| "-".to_owned(), |v| contract.format_price(to_price(v)))
                };
                let spread = spot
                    .and_then(|s| Some((s.ask? - s.bid?) as f64 * to_price(1) / contract.pip()))
                    .map_or_else(|| "-".to_owned(), |p| format!("{p:.1}"));
                let mut cells = vec![
                    self.book.name(*id),
                    price(spot.and_then(|s| s.bid)),
                    price(spot.and_then(|s| s.ask)),
                ];
                if !compact {
                    cells.push(spread);
                }
                Row::new(cells)
            })
            .collect();
        let (table, mut state) = self.table(
            rows,
            if compact {
                &[12, 14, 14]
            } else {
                &[14, 14, 14, 10]
            },
            if compact {
                vec!["Symbol", "Bid", "Ask"]
            } else {
                vec!["Symbol", "Bid", "Ask", "Spread"]
            },
            " Watchlist ",
            self.selected[0],
        );
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_positions(&self, frame: &mut Frame, area: Rect) {
        if self.book.positions.is_empty() {
            self.draw_empty(frame, area, " Positions ", "No open positions");
            return;
        }
        let quotes = self.quote_fn();
        let columns: &[usize] = if area.width >= 110 {
            &[0, 1, 2, 3, 4, 5, 6, 7]
        } else if area.width >= 80 {
            &[0, 1, 2, 3, 4, 7]
        } else {
            &[1, 2, 3, 7]
        };
        let rows: Vec<Row> = self
            .book
            .positions
            .values()
            .map(|p| {
                let contract = self.book.contract(p.trade_data.symbol_id);
                let buy = is_buy(p.trade_data.trade_side);
                let price =
                    |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| contract.format_price(v));
                let profit = self.book.net_profit(p.position_id, &quotes);
                let profit_text = profit.map_or_else(|| "-".to_owned(), |v| self.money(v));
                let color = match profit {
                    Some(v) if v > 0.0 => theme::GREEN,
                    Some(v) if v < 0.0 => theme::RED,
                    _ => theme::MUTED,
                };
                let cells = [
                    Cell::from(p.position_id.to_string()),
                    Cell::from(self.book.name(p.trade_data.symbol_id)),
                    Cell::from(if buy { "Buy" } else { "Sell" })
                        .style(Style::default().fg(if buy { theme::GREEN } else { theme::RED })),
                    Cell::from(format_lots(contract.lots_of_volume(p.trade_data.volume))),
                    Cell::from(price(p.price)),
                    Cell::from(price(p.stop_loss)),
                    Cell::from(price(p.take_profit)),
                    Cell::from(profit_text).style(Style::default().fg(color)),
                ];
                Row::new(
                    columns
                        .iter()
                        .map(|i| cells[*i].clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let headers = ["Id", "Symbol", "Side", "Lots", "Entry", "SL", "TP", "P/L"];
        let all_widths = [12, 12, 6, 8, 12, 12, 12, 16];
        let widths: Vec<u16> = columns.iter().map(|i| all_widths[*i]).collect();
        let header = columns.iter().map(|i| headers[*i]).collect();
        let (table, mut state) = self.table(rows, &widths, header, " Positions ", self.selected[1]);
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_orders(&self, frame: &mut Frame, area: Rect) {
        if self.book.orders.is_empty() {
            self.draw_empty(frame, area, " Orders ", "No working orders");
            return;
        }
        let rows: Vec<Row> = self
            .book
            .orders
            .values()
            .map(|o| {
                let contract = self.book.contract(o.trade_data.symbol_id);
                let price = o.limit_price.or(o.stop_price);
                Row::new(vec![
                    o.order_id.to_string(),
                    self.book.name(o.trade_data.symbol_id),
                    if is_buy(o.trade_data.trade_side) {
                        "Buy"
                    } else {
                        "Sell"
                    }
                    .to_owned(),
                    o.kind().map_or("order", |k| k.label()).to_owned(),
                    format_lots(contract.lots_of_volume(o.trade_data.volume)),
                    price.map_or_else(|| "-".to_owned(), |p| contract.format_price(p)),
                ])
            })
            .collect();
        let header = vec!["Id", "Symbol", "Side", "Type", "Lots", "Price"];
        let (table, mut state) = self.table(
            rows,
            &[12, 12, 6, 14, 8, 12],
            header,
            " Orders ",
            self.selected[2],
        );
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn draw_popup(&self, frame: &mut Frame, area: Rect) {
        match &self.popup {
            Popup::None => {}
            Popup::Add(form) => form.render(frame, area, 50),
            Popup::Ticket(ticket) => ticket.form.render(frame, area, 60),
            Popup::Confirm(confirm) => {
                let text = match confirm {
                    Confirm::Close(id, _) => format!("Close position {id}? (y/n)"),
                    Confirm::Cancel(id) => format!("Cancel order {id}? (y/n)"),
                    Confirm::SignOut => {
                        "Sign out and forget the saved connection? (y/n)".to_owned()
                    }
                };
                let rect = centered(area, 64, 6);
                frame.render_widget(Clear, rect);
                frame.render_widget(
                    Paragraph::new(format!(
                        "\n {text}\n\n Enter / y: confirm   Esc / n: cancel"
                    ))
                    .wrap(Wrap { trim: false })
                    .block(theme::popup(" Confirm action ".to_owned())),
                    rect,
                );
            }
        }
    }

    fn active_symbol(&self) -> Option<i64> {
        match self.tab {
            Tab::Positions => self
                .book
                .positions
                .values()
                .nth(self.selected[1])
                .map(|p| p.trade_data.symbol_id),
            Tab::Orders => self
                .book
                .orders
                .values()
                .nth(self.selected[2])
                .map(|o| o.trade_data.symbol_id),
            _ => self.watch.get(self.selected[0]).copied(),
        }
        .or_else(|| self.watch.first().copied())
    }

    pub(super) fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let market = self.active_symbol().map(|id| {
            let contract = self.book.contract(id);
            let spot = self.quotes.get(id);
            let price = |raw: Option<i64>| {
                raw.map_or_else(|| "--".to_owned(), |p| contract.format_price(to_price(p)))
            };
            let spread = spot
                .and_then(|s| Some((s.ask? - s.bid?) as f64 * to_price(1) / contract.pip()))
                .map(|s| contract.format_price(s * contract.pip()))
                .unwrap_or_else(|| "--".to_owned());
            let change = self
                .price_history
                .get(&id)
                .and_then(|h| Some(h.back()?.cmp(h.get(h.len().checked_sub(2)?)?)));
            let (direction, color) = match change {
                Some(std::cmp::Ordering::Greater) => ("\u{25b2}", theme::GREEN),
                Some(std::cmp::Ordering::Less) => ("\u{25bc}", theme::RED),
                _ => ("\u{25c6}", theme::DIM),
            };
            Market {
                symbol: self.book.name(id),
                bid: price(spot.and_then(|s| s.bid)),
                ask: price(spot.and_then(|s| s.ask)),
                spread,
                spread_color: if self.wide_spread(id) {
                    theme::RED
                } else {
                    theme::DIM
                },
                direction,
                color,
                history: self
                    .price_history
                    .get(&id)
                    .map(|h| h.iter().copied().collect())
                    .unwrap_or_default(),
            }
        });
        let summary = self
            .book
            .trader
            .as_ref()
            .map(|_| self.book.summary(&self.quote_fn()));
        Header {
            context: &self.label,
            environment: Some(self.environment),
            status: &self.status,
            market,
            balance: summary.map(|s| self.money(s.balance)),
        }
        .render(frame, area);
    }

    fn wide_spread(&self, id: i64) -> bool {
        let Some(history) = self.spread_history.get(&id) else {
            return false;
        };
        let Some(current) = history.back().copied() else {
            return false;
        };
        if self.book.name(id).to_ascii_uppercase().contains("XAUUSD") && to_price(current) > 1.0 {
            return true;
        }
        if history.len() < 5 {
            return false;
        }
        let mut sorted: Vec<i64> = history.iter().copied().collect();
        sorted.sort_unstable();
        let middle = sorted.len() / 2;
        let median = if sorted.len().is_multiple_of(2) {
            sorted[middle - 1] as f64 / 2.0 + sorted[middle] as f64 / 2.0
        } else {
            sorted[middle] as f64
        };
        median > 0.0 && current as f64 > median * 1.5
    }

    pub(super) fn draw_empty(&self, frame: &mut Frame, area: Rect, title: &str, text: &str) {
        frame.render_widget(
            Paragraph::new(format!("\n {text}"))
                .style(Style::default().fg(theme::MUTED))
                .block(
                    Block::default()
                        .title(title)
                        .title_style(theme::bold(theme::TEXT)),
                ),
            area,
        );
    }
}
