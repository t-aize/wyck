//! The columns of the tables of the account panel: what each one shows, how wide it starts, and
//! which are on. The user picks the columns, their order and their width, and a column to sort
//! by, for each table on its own.

use serde::{Deserialize, Serialize};

use crate::app::trading::ticket::prefs::{Placed, Slot, default_list, mend};

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

columns! {
    /// The columns of the open positions.
    PositionCol {
        Symbol => ("Symbol", 110., false, true),
        Side => ("Side", 56., false, true),
        Lots => ("Lots", 80., true, true),
        Entry => ("Entry", 92., true, true),
        Price => ("Price", 92., true, true),
        StopLoss => ("Stop loss", 92., true, true),
        TakeProfit => ("Take profit", 92., true, true),
        SlPips => ("To stop (pips)", 100., true, false),
        TpPips => ("To target (pips)", 110., true, false),
        Pips => ("Pips", 72., true, false),
        Swap => ("Swap", 80., true, true),
        Commission => ("Commission", 96., true, false),
        Margin => ("Margin", 96., true, false),
        Risk => ("Risk at stop", 100., true, false),
        Profit => ("Profit", 150., true, true),
        ProfitPercent => ("Profit %", 84., true, false),
        RMultiple => ("R", 64., true, false),
        Opened => ("Opened", 140., false, false),
        Age => ("Open for", 90., true, false),
        Id => ("Id", 90., true, false),
        Comment => ("Comment", 140., false, false),
    }
}

columns! {
    /// The columns of the working orders.
    OrderCol {
        Symbol => ("Symbol", 110., false, true),
        Side => ("Side", 56., false, true),
        Kind => ("Type", 100., false, true),
        Lots => ("Lots", 80., true, true),
        Price => ("Price", 92., true, true),
        Distance => ("Distance", 92., true, true),
        StopLoss => ("Stop loss", 92., true, true),
        TakeProfit => ("Take profit", 92., true, true),
        Expires => ("Expires", 140., false, false),
        Created => ("Placed", 140., false, false),
        Id => ("Id", 90., true, false),
        Comment => ("Comment", 140., false, false),
    }
}

columns! {
    /// The columns of the recent deals.
    DealCol {
        Time => ("Time", 150., false, true),
        Symbol => ("Symbol", 110., false, true),
        Side => ("Side", 56., false, true),
        Kind => ("Kind", 64., false, false),
        Lots => ("Lots", 80., true, true),
        Entry => ("Entry", 92., true, false),
        Price => ("Price", 92., true, true),
        Pips => ("Pips", 72., true, false),
        Commission => ("Commission", 96., true, true),
        Swap => ("Swap", 80., true, false),
        Gross => ("Gross", 96., true, false),
        Profit => ("Profit", 150., true, true),
        Balance => ("Balance", 110., true, false),
        Id => ("Deal", 90., true, false),
        Order => ("Order", 90., true, false),
        Position => ("Position", 90., true, false),
    }
}

columns! {
    /// The columns of the exposure by symbol.
    ExposureCol {
        Symbol => ("Symbol", 110., false, true),
        Net => ("Net lots", 90., true, true),
        Long => ("Long", 80., true, true),
        Short => ("Short", 80., true, true),
        Positions => ("Positions", 84., true, true),
        Orders => ("Orders", 70., true, false),
        Entry => ("Average entry", 110., true, true),
        Price => ("Price", 92., true, false),
        Margin => ("Margin", 96., true, false),
        Profit => ("Profit", 130., true, true),
    }
}

columns! {
    /// The columns of the price alerts.
    AlertCol {
        Symbol => ("Symbol", 110., false, true),
        Condition => ("Condition", 130., false, true),
        Price => ("Price", 92., true, true),
        Distance => ("Distance", 100., true, false),
        State => ("State", 140., false, true),
        Message => ("Message", 220., false, true),
        Repeats => ("Repeats", 80., false, false),
        Created => ("Created", 140., false, false),
        Fired => ("Last fired", 140., false, false),
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
