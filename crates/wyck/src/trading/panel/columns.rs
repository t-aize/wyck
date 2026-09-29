//! The columns of the tables of the account panel: what each one shows, how wide it starts, and
//! which are on. The user picks the columns, their order and their width, and a column to sort
//! by, for each table on its own.

use serde::{Deserialize, Serialize};

use crate::trading::ticket::prefs::{Placed, Slot, default_list, mend};

/// A column of a table.
pub trait Column: Slot + Serialize + for<'de> Deserialize<'de> {
    /// How wide it starts, in pixels.
    fn width(self) -> f32;
    /// Whether its text sits at the right, as numbers do.
    fn right(self) -> bool;
}

/// The narrowest and the widest a column can be made.
pub const WIDTH_MIN: f32 = 40.0;
pub const WIDTH_MAX: f32 = 600.0;

macro_rules! columns {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($variant:ident => ($label:expr, $width:expr, $right:expr, $on:expr)),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl Slot for $name {
            const ALL: &'static [Self] = &[$(Self::$variant),+];

            fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            fn shown_by_default(self) -> bool {
                match self {
                    $(Self::$variant => $on),+
                }
            }
        }

        impl Column for $name {
            fn width(self) -> f32 {
                match self {
                    $(Self::$variant => $width),+
                }
            }

            fn right(self) -> bool {
                match self {
                    $(Self::$variant => $right),+
                }
            }
        }
    };
}

// Widths come from the sizes every screen shares, so a column of one table is as wide as the
// same kind of column of the others.
// They are the base sizes: the table scales them with the interface when it draws (see
// `tokens::scaled`).
const SHORT: f32 = 84.0;
const NUMBER: f32 = 110.0;
const WIDE: f32 = 130.0;
const TEXT: f32 = 220.0;

columns! {
    /// The columns of the open positions.
    PositionCol {
        Symbol => ("Symbol", NUMBER, false, true),
        Side => ("Side", SHORT, false, true),
        Lots => ("Lots", SHORT, true, true),
        Entry => ("Entry", NUMBER, true, true),
        Price => ("Price", NUMBER, true, true),
        StopLoss => ("Stop loss", NUMBER, true, true),
        TakeProfit => ("Take profit", NUMBER, true, true),
        SlPips => ("To stop (pips)", NUMBER, true, false),
        TpPips => ("To target (pips)", NUMBER, true, false),
        Pips => ("Pips", SHORT, true, false),
        Swap => ("Swap", SHORT, true, false),
        Commission => ("Commission", NUMBER, true, false),
        Margin => ("Margin", NUMBER, true, false),
        Risk => ("Risk at stop", NUMBER, true, false),
        Profit => ("Profit", WIDE, true, true),
        ProfitPercent => ("Profit %", SHORT, true, false),
        RMultiple => ("R", SHORT, true, false),
        Opened => ("Opened", WIDE, false, false),
        Age => ("Open for", SHORT, true, false),
        Id => ("Id", SHORT, true, false),
        Comment => ("Comment", TEXT, false, false),
    }
}

columns! {
    /// The columns of the working orders.
    OrderCol {
        Symbol => ("Symbol", NUMBER, false, true),
        Side => ("Side", SHORT, false, true),
        Kind => ("Type", NUMBER, false, true),
        Lots => ("Lots", SHORT, true, true),
        Price => ("Price", NUMBER, true, true),
        Distance => ("Distance", NUMBER, true, true),
        StopLoss => ("Stop loss", NUMBER, true, true),
        TakeProfit => ("Take profit", NUMBER, true, true),
        Expires => ("Expires", WIDE, false, false),
        Created => ("Placed", WIDE, false, false),
        Id => ("Id", SHORT, true, false),
        Comment => ("Comment", TEXT, false, false),
    }
}

columns! {
    /// The columns of the recent deals.
    DealCol {
        Time => ("Time", WIDE, false, true),
        Symbol => ("Symbol", NUMBER, false, true),
        Side => ("Side", SHORT, false, true),
        Kind => ("Kind", SHORT, false, false),
        Lots => ("Lots", SHORT, true, true),
        Entry => ("Entry", NUMBER, true, false),
        Price => ("Price", NUMBER, true, true),
        Pips => ("Pips", SHORT, true, false),
        Commission => ("Commission", NUMBER, true, true),
        Swap => ("Swap", SHORT, true, false),
        Gross => ("Gross", NUMBER, true, false),
        Profit => ("Profit", WIDE, true, true),
        Balance => ("Balance", NUMBER, true, false),
        Id => ("Deal", SHORT, true, false),
        Order => ("Order", SHORT, true, false),
        Position => ("Position", SHORT, true, false),
    }
}

columns! {
    /// The columns of the exposure by symbol.
    ExposureCol {
        Symbol => ("Symbol", NUMBER, false, true),
        Net => ("Net lots", SHORT, true, true),
        Long => ("Long", SHORT, true, true),
        Short => ("Short", SHORT, true, true),
        Positions => ("Positions", SHORT, true, true),
        Orders => ("Orders", SHORT, true, false),
        Entry => ("Average entry", NUMBER, true, true),
        Price => ("Price", NUMBER, true, false),
        Margin => ("Margin", NUMBER, true, false),
        Profit => ("Profit", WIDE, true, true),
    }
}

columns! {
    /// The columns of the price alerts.
    AlertCol {
        Symbol => ("Symbol", NUMBER, false, true),
        Condition => ("Condition", WIDE, false, true),
        Price => ("Price", NUMBER, true, true),
        Distance => ("Distance", NUMBER, true, false),
        State => ("State", WIDE, false, true),
        Message => ("Message", TEXT, false, true),
        Repeats => ("Trigger", SHORT, false, false),
        Created => ("Created", WIDE, false, false),
        Fired => ("Last fired", WIDE, false, false),
    }
}

columns! {
    /// The columns of the log of alerts that fired.
    AlertLogCol {
        Time => ("Time", WIDE, false, true),
        Symbol => ("Symbol", NUMBER, false, true),
        Watched => ("Watched", NUMBER, false, true),
        Condition => ("Condition", WIDE, false, true),
        Value => ("Value", NUMBER, true, true),
        Message => ("Message", TEXT, false, true),
    }
}

/// One column of a table as the user arranged it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Col<C> {
    pub item: C,
    pub shown: bool,
    /// The width the user gave it, when it is not the column's own.
    #[serde(default)]
    pub width: Option<f32>,
}

/// The column a table is sorted by, and which way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sort<C> {
    pub column: C,
    pub descending: bool,
}

/// How a table is arranged: its columns in order, and how it is sorted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(serialize = "C: Serialize", deserialize = "C: Deserialize<'de>"))]
pub struct TablePrefs<C> {
    pub columns: Vec<Col<C>>,
    #[serde(default)]
    pub sort: Option<Sort<C>>,
}

impl<C: Column> Default for TablePrefs<C> {
    fn default() -> Self {
        Self {
            columns: default_list::<C>()
                .into_iter()
                .map(|placed| Col {
                    item: placed.item,
                    shown: placed.shown,
                    width: None,
                })
                .collect(),
            sort: None,
        }
    }
}

impl<C: Column> TablePrefs<C> {
    /// Every column once, the ones a file did not know at the end, widths in range, and at
    /// least one column showing.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let mut placed: Vec<Placed<C>> = self
            .columns
            .iter()
            .map(|c| Placed {
                item: c.item,
                shown: c.shown,
            })
            .collect();
        mend(&mut placed);
        let old = std::mem::take(&mut self.columns);
        self.columns = placed
            .into_iter()
            .map(|p| Col {
                item: p.item,
                shown: p.shown,
                width: old
                    .iter()
                    .find(|c| c.item == p.item)
                    .and_then(|c| c.width)
                    .filter(|w| w.is_finite())
                    .map(|w| w.clamp(WIDTH_MIN, WIDTH_MAX)),
            })
            .collect();
        if !self.columns.iter().any(|c| c.shown)
            && let Some(first) = self.columns.first_mut()
        {
            first.shown = true;
        }
        self
    }

    /// Clicking a header: sorts by the column, then the other way, then not at all.
    pub fn cycle_sort(&mut self, column: C) {
        self.sort = match self.sort {
            Some(s) if s.column == column && !s.descending => Some(Sort {
                column,
                descending: true,
            }),
            Some(s) if s.column == column => None,
            _ => Some(Sort {
                column,
                descending: false,
            }),
        };
    }

    pub fn set_width(&mut self, index: usize, width: f32) {
        if let Some(col) = self.columns.get_mut(index) {
            col.width = Some(width.clamp(WIDTH_MIN, WIDTH_MAX));
        }
    }

    /// The width of the column at `index` now.
    pub fn width_at(&self, index: usize) -> f32 {
        self.columns
            .get(index)
            .map_or(WIDTH_MIN, |c| c.width.unwrap_or_else(|| c.item.width()))
    }

    /// Shows or hides the column at `index`, keeping at least one showing.
    pub fn toggle(&mut self, index: usize) {
        let showing = self.columns.iter().filter(|c| c.shown).count();
        if let Some(col) = self.columns.get_mut(index) {
            if col.shown && showing <= 1 {
                return;
            }
            col.shown = !col.shown;
        }
    }

    /// Moves the column at `index` one place up or down the list.
    pub fn shift(&mut self, index: usize, delta: isize) {
        let Some(target) = index.checked_add_signed(delta) else {
            return;
        };
        if index < self.columns.len() && target < self.columns.len() {
            self.columns.swap(index, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_starts_with_the_default_columns() {
        let table = TablePrefs::<PositionCol>::default();
        assert_eq!(table.columns.len(), PositionCol::ALL.len());
        let shown: Vec<PositionCol> = table
            .columns
            .iter()
            .filter(|c| c.shown)
            .map(|c| c.item)
            .collect();
        assert_eq!(shown[0], PositionCol::Symbol);
        assert!(shown.contains(&PositionCol::Profit));
        assert!(!shown.contains(&PositionCol::Id));
        assert_eq!(table, table.clone().normalized());
    }

    #[test]
    fn clicking_a_header_sorts_up_then_down_then_not_at_all() {
        let mut table = TablePrefs::<OrderCol>::default();
        table.cycle_sort(OrderCol::Price);
        assert_eq!(table.sort.map(|s| s.descending), Some(false));
        table.cycle_sort(OrderCol::Price);
        assert_eq!(table.sort.map(|s| s.descending), Some(true));
        table.cycle_sort(OrderCol::Price);
        assert_eq!(table.sort, None);
        table.cycle_sort(OrderCol::Price);
        table.cycle_sort(OrderCol::Lots);
        assert_eq!(
            table.sort.map(|s| (s.column, s.descending)),
            Some((OrderCol::Lots, false))
        );
    }

    #[test]
    fn the_last_column_showing_stays() {
        let mut table = TablePrefs::<AlertCol>::default();
        // Hide every column that shows: the last one refuses.
        for index in 0..table.columns.len() {
            if table.columns[index].shown {
                table.toggle(index);
            }
        }
        assert_eq!(table.columns.iter().filter(|c| c.shown).count(), 1);
    }

    #[test]
    fn widths_are_kept_in_range_and_columns_move() {
        let mut table = TablePrefs::<DealCol>::default();
        table.set_width(0, 5.0);
        assert_eq!(table.width_at(0), WIDTH_MIN);
        table.set_width(0, 9_000.0);
        assert_eq!(table.width_at(0), WIDTH_MAX);
        let first = table.columns[0].item;
        table.shift(0, 1);
        assert_eq!(table.columns[1].item, first);
        table.shift(0, -1);
        table.shift(100, 1);
        assert_eq!(table.columns.len(), DealCol::ALL.len());
    }

    #[test]
    fn a_saved_table_with_missing_and_repeated_columns_is_mended() {
        let mut table = TablePrefs::<ExposureCol>::default();
        table.columns.truncate(2);
        table.columns.push(table.columns[0]);
        table.columns[1].width = Some(f32::NAN);
        let table = table.normalized();
        assert_eq!(table.columns.len(), ExposureCol::ALL.len());
        assert_eq!(table.columns[1].width, None);
    }

    #[test]
    fn a_table_survives_a_round_trip() {
        let mut table = TablePrefs::<PositionCol>::default();
        table.cycle_sort(PositionCol::Profit);
        table.set_width(2, 123.0);
        table.toggle(3);
        let text = toml::to_string(&table).unwrap();
        let back: TablePrefs<PositionCol> = toml::from_str(&text).unwrap();
        assert_eq!(back, table);
    }
}
