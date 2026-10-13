//! The watchlists: named lists of symbols.

use super::{MAX_LIST_NAME, WATCHLISTS_VERSION};
use serde::{Deserialize, Serialize};

/// What a watchlists document without a version is taken to be.
fn schema_version() -> u32 {
    WATCHLISTS_VERSION
}

/// A named list of symbols, by the names the broker gives them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchlist {
    pub name: String,
    #[serde(default)]
    pub symbols: Vec<String>,
}

/// Why a watchlist name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    TooLong,
    Taken,
}

impl std::fmt::Display for NameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("Give the list a name"),
            Self::TooLong => write!(f, "A name has at most {MAX_LIST_NAME} characters"),
            Self::Taken => f.write_str("A list with this name already exists"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watchlists {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub favorites: Vec<String>,
    #[serde(default)]
    pub lists: Vec<Watchlist>,
}

impl Default for Watchlists {
    fn default() -> Self {
        Self {
            schema_version: WATCHLISTS_VERSION,
            favorites: Vec::new(),
            lists: Vec::new(),
        }
    }
}

impl Watchlists {
    /// The lists repaired: repeated symbols and lists with a repeated or empty name dropped.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.schema_version = WATCHLISTS_VERSION;
        dedup_keep_order(&mut self.favorites);
        let mut seen: Vec<String> = Vec::new();
        self.lists.retain_mut(|list| {
            list.name = list.name.trim().to_owned();
            let key = list.name.to_lowercase();
            if list.name.is_empty() || seen.contains(&key) {
                return false;
            }
            seen.push(key);
            dedup_keep_order(&mut list.symbols);
            true
        });
        self
    }

    pub fn is_favorite(&self, symbol: &str) -> bool {
        self.favorites.iter().any(|s| s == symbol)
    }

    pub fn toggle_favorite(&mut self, symbol: &str) {
        if let Some(at) = self.favorites.iter().position(|s| s == symbol) {
            self.favorites.remove(at);
        } else {
            self.favorites.push(symbol.to_owned());
        }
    }

    fn check_name(&self, name: &str, except: Option<usize>) -> Result<String, NameError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if name.chars().count() > MAX_LIST_NAME {
            return Err(NameError::TooLong);
        }
        let key = name.to_lowercase();
        let taken = self
            .lists
            .iter()
            .enumerate()
            .any(|(i, list)| Some(i) != except && list.name.to_lowercase() == key);
        if taken {
            return Err(NameError::Taken);
        }
        Ok(name.to_owned())
    }

    /// Adds an empty list and returns its position.
    pub fn create_list(&mut self, name: &str) -> Result<usize, NameError> {
        let name = self.check_name(name, None)?;
        self.lists.push(Watchlist {
            name,
            symbols: Vec::new(),
        });
        Ok(self.lists.len() - 1)
    }

    pub fn rename_list(&mut self, index: usize, name: &str) -> Result<(), NameError> {
        let name = self.check_name(name, Some(index))?;
        if let Some(list) = self.lists.get_mut(index) {
            list.name = name;
        }
        Ok(())
    }

    pub fn delete_list(&mut self, index: usize) {
        if index < self.lists.len() {
            self.lists.remove(index);
        }
    }

    pub fn in_list(&self, index: usize, symbol: &str) -> bool {
        self.lists
            .get(index)
            .is_some_and(|list| list.symbols.iter().any(|s| s == symbol))
    }

    /// Puts the symbol in the list, or takes it out if it is already there.
    pub fn toggle_in_list(&mut self, index: usize, symbol: &str) {
        let Some(list) = self.lists.get_mut(index) else {
            return;
        };
        if let Some(at) = list.symbols.iter().position(|s| s == symbol) {
            list.symbols.remove(at);
        } else {
            list.symbols.push(symbol.to_owned());
        }
    }
}

pub(super) fn dedup_keep_order(items: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

#[cfg(test)]
mod tests {
    use super::super::watchlists::*;

    #[test]
    fn watchlist_names_are_checked() {
        let mut lists = Watchlists::default();
        assert_eq!(lists.create_list("  Majors "), Ok(0));
        assert_eq!(lists.lists[0].name, "Majors");
        assert_eq!(lists.create_list("majors"), Err(NameError::Taken));
        assert_eq!(lists.create_list("   "), Err(NameError::Empty));
        assert_eq!(
            lists.create_list(&"x".repeat(MAX_LIST_NAME + 1)),
            Err(NameError::TooLong)
        );
        assert_eq!(lists.create_list("Metals"), Ok(1));
        assert_eq!(lists.rename_list(1, "Majors"), Err(NameError::Taken));
        assert_eq!(
            lists.rename_list(1, "METALS"),
            Ok(()),
            "its own name is fine"
        );
        assert_eq!(lists.rename_list(1, "Gold and silver"), Ok(()));
    }

    #[test]
    fn symbols_move_in_and_out_of_lists_and_favorites() {
        let mut lists = Watchlists::default();
        lists.create_list("A").unwrap();
        lists.toggle_in_list(0, "EURUSD");
        lists.toggle_in_list(0, "XAUUSD");
        assert!(lists.in_list(0, "EURUSD"));
        lists.toggle_in_list(0, "EURUSD");
        assert!(!lists.in_list(0, "EURUSD"));
        assert_eq!(lists.lists[0].symbols, vec!["XAUUSD"]);
        lists.toggle_in_list(7, "X");
        assert!(!lists.in_list(7, "X"));

        assert!(!lists.is_favorite("US100.cash"));
        lists.toggle_favorite("US100.cash");
        assert!(lists.is_favorite("US100.cash"));
        lists.toggle_favorite("US100.cash");
        assert!(!lists.is_favorite("US100.cash"));

        lists.delete_list(0);
        lists.delete_list(0);
        assert!(lists.lists.is_empty());
    }

    #[test]
    fn watchlists_survive_awkward_symbol_names_through_toml() {
        let mut lists = Watchlists::default();
        lists.create_list("Odd \"names\" & more").unwrap();
        lists.toggle_in_list(0, "US100.cash");
        lists.toggle_in_list(0, "EUR/USD #1");
        let text = toml::to_string_pretty(&lists).unwrap();
        let back: Watchlists = toml::from_str(&text).unwrap();
        assert_eq!(back, lists);
    }
}
