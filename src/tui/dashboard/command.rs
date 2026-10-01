use super::*;
use crate::tui::commands::{self, Command};

impl Dashboard {
    pub(super) fn execute_command(&mut self, text: &str) -> Outcome {
        let command = match commands::parse(text) {
            Ok(command) => command,
            Err(error) => {
                self.notify(Tone::Error, error);
                return Outcome::None;
            }
        };
        match command {
            Command::Help => {
                self.tab = Tab::Console;
                self.notify(Tone::Info, commands::HELP);
            }
            Command::Watchlist => self.tab = Tab::Watchlist,
            Command::Positions => self.tab = Tab::Positions,
            Command::Orders => self.tab = Tab::Orders,
            Command::Console => self.tab = Tab::Console,
            Command::Clear => self.console.clear(),
            Command::Quit => return Outcome::Quit,
            Command::Logout => self.popup = Popup::Confirm(Confirm::SignOut),
            Command::Refresh => {
                if self.busy || self.loading {
                    self.notify(Tone::Warning, "Wait for the current operation to finish");
                    return Outcome::None;
                }
                if self.session.is_some() {
                    self.spawn_load();
                    self.notify(Tone::Info, "Refreshing account...");
                } else {
                    self.notify(Tone::Info, "Preview: sample data, no server connection");
                }
            }
            Command::Add(symbol) => {
                let form = Form::new(
                    "Add a symbol",
                    "Enter: add   Esc: cancel",
                    vec![Field::new("Symbol", &symbol, false)],
                );
                self.key_add(form, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
            }
            Command::Remove(symbol) => {
                if self.busy || self.loading {
                    self.notify(Tone::Warning, "Wait for the current operation to finish");
                    return Outcome::None;
                }
                if let Some(index) = self
                    .watch
                    .iter()
                    .position(|id| self.book.name(*id).eq_ignore_ascii_case(&symbol))
                {
                    self.selected[0] = index;
                    self.remove_selected();
                    self.notify(
                        Tone::Success,
                        format!("{symbol} removed from the watchlist"),
                    );
                } else {
                    self.notify(Tone::Error, format!("{symbol} is not in the watchlist"));
                }
            }
            Command::Trade {
                side,
                symbol,
                lots,
                sl,
                tp,
            } => {
                if let Some(index) = self
                    .watch
                    .iter()
                    .position(|id| self.book.name(*id).eq_ignore_ascii_case(&symbol))
                {
                    self.selected[0] = index;
                    self.open_ticket(side);
                    if let Popup::Ticket(ticket) = &mut self.popup {
                        ticket.form.fields[0].input.set(&lots.to_string());
                        ticket.form.fields[1]
                            .input
                            .set(&sl.map(|p| p.to_string()).unwrap_or_default());
                        ticket.form.fields[2]
                            .input
                            .set(&tp.map(|p| p.to_string()).unwrap_or_default());
                    }
                } else {
                    self.notify(Tone::Error, format!("Use /add {symbol} before trading it"));
                }
            }
            Command::Close(id) => {
                if let Some(position) = self.book.positions.get(&id) {
                    self.popup = Popup::Confirm(Confirm::Close(id, position.trade_data.volume));
                } else {
                    self.notify(Tone::Error, format!("Position {id} is not open"));
                }
            }
            Command::Cancel(id) => {
                if self.book.orders.contains_key(&id) {
                    self.popup = Popup::Confirm(Confirm::Cancel(id));
                } else {
                    self.notify(Tone::Error, format!("Order {id} is not working"));
                }
            }
        }
        Outcome::None
    }
}
