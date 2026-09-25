//! What the user chooses about the order ticket, kept between runs: where the panel sits and how
//! wide it is, which sections show and in what order, the shortcuts for the size, the values a
//! new order starts with, and how the volume is sized.
//!
//! Everything has a default, and everything read from a file is put back in range by
//! [`TicketPrefs::normalized`], so a file edited by hand or written by an older version never
//! breaks the panel.

use serde::{Deserialize, Serialize};

use crate::app::trading::math::{Offset, SizeMode};

/// The kind of order the ticket sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Market,
    Limit,
    Stop,
    StopLimit,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Market, Self::Limit, Self::Stop, Self::StopLimit];

    pub fn label(self) -> &'static str {
        match self {
            Self::Market => "Market",
            Self::Limit => "Limit",
            Self::Stop => "Stop",
            Self::StopLimit => "Stop limit",
        }
    }

    /// Whether the order waits at a price of its own.
    pub fn is_pending(self) -> bool {
        self != Self::Market
    }
}

/// How long a pending order stays working.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tif {
    /// Until it is filled or cancelled.
    #[default]
    GoodTillCancel,
    /// Until a time after which it is cancelled.
    GoodTillDate,
}

/// The unit an expiry is given in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Span {
    Minutes,
    #[default]
    Hours,
    Days,
}

impl Span {
    pub const ALL: [Self; 3] = [Self::Minutes, Self::Hours, Self::Days];

    pub fn label(self) -> &'static str {
        match self {
            Self::Minutes => "Minutes",
            Self::Hours => "Hours",
            Self::Days => "Days",
        }
    }

    /// How many milliseconds one of them is.
    pub fn millis(self) -> i64 {
        match self {
            Self::Minutes => 60_000,
            Self::Hours => 3_600_000,
            Self::Days => 86_400_000,
        }
    }
}

/// Which side of the charts the ticket sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dock {
    Left,
    #[default]
    Right,
}

/// How much room the ticket gives its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}

/// A block of the ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    /// The buy and sell buttons with their prices.
    Sides,
    /// Market, limit, stop or stop limit, and the price of a pending order.
    Order,
    /// The volume and its shortcuts.
    Size,
    StopLoss,
    TakeProfit,
    /// How long a pending order lives, the slippage, the trailing stop, the comment.
    Options,
    /// What is open on the symbol, with what can be done to it.
    Positions,
    Summary,
    /// The button that sends the order.
    Send,
}

/// A row of the summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Line {
    Volume,
    Notional,
    Risk,
    Reward,
    RiskReward,
    Margin,
    PipValue,
    SpreadCost,
}

/// Something that can be listed, shown or hidden, and moved.
pub trait Slot: Copy + PartialEq + 'static {
    const ALL: &'static [Self];
    fn label(self) -> &'static str;
    fn shown_by_default(self) -> bool {
        true
    }
}

impl Slot for Section {
    const ALL: &'static [Self] = &[
        Self::Sides,
        Self::Order,
        Self::Size,
        Self::StopLoss,
        Self::TakeProfit,
        Self::Options,
        Self::Positions,
        Self::Summary,
        Self::Send,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Sides => "Buy and sell buttons",
            Self::Order => "Order type and price",
            Self::Size => "Size",
            Self::StopLoss => "Stop loss",
            Self::TakeProfit => "Take profit",
            Self::Options => "Expiry, slippage, trailing, comment",
            Self::Positions => "Open on this symbol",
            Self::Summary => "Summary of the order",
            Self::Send => "Send button",
        }
    }

    fn shown_by_default(self) -> bool {
        self != Self::Options
    }
}

impl Slot for Line {
    const ALL: &'static [Self] = &[
        Self::Volume,
        Self::Notional,
        Self::Risk,
        Self::Reward,
        Self::RiskReward,
        Self::Margin,
        Self::PipValue,
        Self::SpreadCost,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Volume => "Volume",
            Self::Notional => "Value of the position",
            Self::Risk => "Risk",
            Self::Reward => "Reward",
            Self::RiskReward => "Risk to reward",
            Self::Margin => "Margin",
            Self::PipValue => "Pip value",
            Self::SpreadCost => "Cost of the spread",
        }
    }

    fn shown_by_default(self) -> bool {
        !matches!(self, Self::Notional | Self::SpreadCost)
    }
}

/// One entry of an ordered list of slots.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placed<T> {
    pub item: T,
    pub shown: bool,
}

/// The default list: every slot, in order, shown or not as it is by default.
pub fn default_list<T: Slot>() -> Vec<Placed<T>> {
    T::ALL
        .iter()
        .map(|item| Placed {
            item: *item,
            shown: item.shown_by_default(),
        })
        .collect()
}

/// A list put back in shape: each slot once, in the order given, and the slots a file did not
/// know (added by a later version) at the end, as they are by default.
pub fn mend<T: Slot>(list: &mut Vec<Placed<T>>) {
    let mut seen: Vec<T> = Vec::new();
    list.retain(|placed| {
        if seen.contains(&placed.item) {
            false
        } else {
            seen.push(placed.item);
            true
        }
    });
    for item in T::ALL {
        if !seen.contains(item) {
            list.push(Placed {
                item: *item,
                shown: item.shown_by_default(),
            });
        }
    }
}

/// Moves the slot at `index` up (`-1`) or down (`1`) the list. Returns whether it moved.
pub fn shift<T>(list: &mut [Placed<T>], index: usize, delta: isize) -> bool {
    let Some(target) = index.checked_add_signed(delta) else {
        return false;
    };
    if index >= list.len() || target >= list.len() {
        return false;
    }
    list.swap(index, target);
    true
}

/// The shortcuts under the volume field, one list per way of sizing. An empty list means the
/// shortcuts are worked out from the account and the symbol.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presets {
    pub lots: Vec<f64>,
    pub units: Vec<f64>,
    pub risk_percent: Vec<f64>,
    pub risk_money: Vec<f64>,
    pub free_margin: Vec<f64>,
}

/// The most shortcuts a row holds.
pub const MAX_PRESETS: usize = 6;

impl Default for Presets {
    fn default() -> Self {
        Self {
            lots: vec![0.01, 0.1, 0.5, 1.0],
            units: Vec::new(),
            risk_percent: vec![0.25, 0.5, 1.0, 2.0],
            risk_money: Vec::new(),
            free_margin: vec![5.0, 10.0, 25.0, 50.0],
        }
    }
}

impl Presets {
    /// The list of a way of sizing.
    pub fn of(&self, mode: SizeMode) -> &[f64] {
        match mode {
            SizeMode::Lots => &self.lots,
            SizeMode::Units => &self.units,
            SizeMode::RiskBalance | SizeMode::RiskEquity => &self.risk_percent,
            SizeMode::RiskMoney => &self.risk_money,
            SizeMode::FreeMargin => &self.free_margin,
        }
    }

    pub fn of_mut(&mut self, mode: SizeMode) -> &mut Vec<f64> {
        match mode {
            SizeMode::Lots => &mut self.lots,
            SizeMode::Units => &mut self.units,
            SizeMode::RiskBalance | SizeMode::RiskEquity => &mut self.risk_percent,
            SizeMode::RiskMoney => &mut self.risk_money,
            SizeMode::FreeMargin => &mut self.free_margin,
        }
    }

    fn normalize(&mut self) {
        for list in [
            &mut self.lots,
            &mut self.units,
            &mut self.risk_percent,
            &mut self.risk_money,
            &mut self.free_margin,
        ] {
            list.retain(|v| v.is_finite() && *v > 0.0);
            list.truncate(MAX_PRESETS);
        }
    }
}

/// What a new order starts with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    pub kind: Kind,
    /// Whether the stop loss and the take profit are on when a symbol is picked.
    pub stop_on: bool,
    pub target_on: bool,
    /// How far a stop loss that is turned on starts, in pips.
    pub stop_pips: f64,
    /// Where a take profit that is turned on starts, in multiples of the risk.
    pub target_ratio: f64,
    pub tif: Tif,
    /// How long a good-till-date order lives when the field is opened, and in what unit.
    pub expiry: f64,
    pub expiry_span: Span,
    /// The slippage a market range takes, and how far the limit of a stop limit sits from its
    /// stop, in pips.
    pub slippage_pips: f64,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            kind: Kind::Market,
            stop_on: false,
            target_on: false,
            stop_pips: 20.0,
            target_ratio: 2.0,
            tif: Tif::GoodTillCancel,
            expiry: 24.0,
            expiry_span: Span::Hours,
            slippage_pips: 2.0,
        }
    }
}

/// The room and the look of the panel, and what is in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub dock: Dock,
    pub width: f32,
    pub density: Density,
    pub sections: Vec<Placed<Section>>,
    pub lines: Vec<Placed<Line>>,
    /// Sell on the right and buy on the left, instead of the other way round.
    pub buy_first: bool,
    pub show_spread: bool,
    /// The price on each of the two buttons.
    pub show_prices: bool,
    pub presets: Presets,
    pub defaults: Defaults,
    /// The share of the balance above which the ticket warns about what an order risks.
    pub high_risk: f64,
}

pub const WIDTH_MIN: f32 = 240.0;
pub const WIDTH_MAX: f32 = 560.0;
pub const WIDTH_DEFAULT: f32 = 320.0;

impl Default for Layout {
    fn default() -> Self {
        Self {
            dock: Dock::Right,
            width: WIDTH_DEFAULT,
            density: Density::Comfortable,
            sections: default_list(),
            lines: default_list(),
            buy_first: false,
            show_spread: true,
            show_prices: true,
            presets: Presets::default(),
            defaults: Defaults::default(),
            high_risk: 5.0,
        }
    }
}

impl Layout {
    /// Whether a block of the ticket shows.
    pub fn shows(&self, section: Section) -> bool {
        self.sections
            .iter()
            .any(|placed| placed.item == section && placed.shown)
    }

    /// The blocks that show, in order.
    pub fn visible_sections(&self) -> impl Iterator<Item = Section> + '_ {
        self.sections
            .iter()
            .filter(|placed| placed.shown)
            .map(|placed| placed.item)
    }

    pub fn visible_lines(&self) -> impl Iterator<Item = Line> + '_ {
        self.lines
            .iter()
            .filter(|placed| placed.shown)
            .map(|placed| placed.item)
    }

    /// The layout with every value in a range that means something.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let fix = |value: f64, fallback: f64, least: f64, most: f64| {
            if value.is_finite() {
                value.clamp(least, most)
            } else {
                fallback
            }
        };
        self.width = if self.width.is_finite() {
            self.width.clamp(WIDTH_MIN, WIDTH_MAX)
        } else {
            WIDTH_DEFAULT
        };
        mend(&mut self.sections);
        mend(&mut self.lines);
        // An order needs a way to be sent: the button, or the two sides (which send with
        // one-click trading).
        if !self.shows(Section::Send) && !self.shows(Section::Sides) {
            for placed in &mut self.sections {
                if placed.item == Section::Send {
                    placed.shown = true;
                }
            }
        }
        self.presets.normalize();
        let d = &mut self.defaults;
        d.stop_pips = fix(d.stop_pips, 20.0, 0.1, 100_000.0);
        d.target_ratio = fix(d.target_ratio, 2.0, 0.1, 1_000.0);
        d.expiry = fix(d.expiry, 24.0, 1.0, 100_000.0);
        d.slippage_pips = fix(d.slippage_pips, 2.0, 0.0, 10_000.0);
        self.high_risk = fix(self.high_risk, 5.0, 0.1, 100.0);
        self
    }
}

/// How the ticket sizes orders and looks, kept between runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TicketPrefs {
    pub size_mode: SizeMode,
    /// The last value typed for the volume, in the unit of `size_mode`.
    pub size: f64,
    pub stop_unit: Offset,
    pub target_unit: Offset,
    pub layout: Layout,
}

impl Default for TicketPrefs {
    fn default() -> Self {
        Self {
            size_mode: SizeMode::Lots,
            size: 0.1,
            stop_unit: Offset::Pips,
            target_unit: Offset::Pips,
            layout: Layout::default(),
        }
    }
}

impl TicketPrefs {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if !(self.size.is_finite() && self.size > 0.0) {
            self.size = Self::default().size;
        }
        self.layout = self.layout.normalized();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_list_every_section_once() {
        let layout = Layout::default();
        assert_eq!(layout.sections.len(), Section::ALL.len());
        assert!(layout.shows(Section::Send));
        assert!(!layout.shows(Section::Options));
        assert_eq!(layout, layout.clone().normalized());
    }

    #[test]
    fn a_list_is_mended_and_lost_sections_come_back_at_the_end() {
        let mut layout = Layout {
            sections: vec![
                Placed {
                    item: Section::Summary,
                    shown: true,
                },
                Placed {
                    item: Section::Summary,
                    shown: false,
                },
                Placed {
                    item: Section::Size,
                    shown: false,
                },
            ],
            ..Layout::default()
        };
        layout = layout.normalized();
        assert_eq!(layout.sections.len(), Section::ALL.len());
        assert_eq!(layout.sections[0].item, Section::Summary);
        assert!(layout.sections[0].shown, "the first of two copies wins");
        assert_eq!(layout.sections[1].item, Section::Size);
        assert!(!layout.sections[1].shown);
    }

    #[test]
    fn there_is_always_a_way_to_send() {
        let mut layout = Layout::default();
        for placed in &mut layout.sections {
            if matches!(placed.item, Section::Send | Section::Sides) {
                placed.shown = false;
            }
        }
        let layout = layout.normalized();
        assert!(layout.shows(Section::Send));
    }

    #[test]
    fn wild_values_are_brought_back_in_range() {
        let mut layout = Layout {
            width: 9_000.0,
            high_risk: f64::NAN,
            ..Layout::default()
        };
        layout.defaults.stop_pips = -3.0;
        layout.presets.lots = vec![0.0, -1.0, f64::NAN, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let layout = layout.normalized();
        assert_eq!(layout.width, WIDTH_MAX);
        assert_eq!(layout.high_risk, 5.0);
        assert_eq!(layout.defaults.stop_pips, 0.1);
        assert_eq!(layout.presets.lots.len(), MAX_PRESETS);
        assert_eq!(layout.presets.lots[0], 1.0);
        assert_eq!(
            Layout {
                width: f32::NAN,
                ..Layout::default()
            }
            .normalized()
            .width,
            WIDTH_DEFAULT
        );
    }

    #[test]
    fn a_slot_moves_along_its_list_and_stops_at_the_ends() {
        let mut list = default_list::<Section>();
        let first = list[0].item;
        assert!(!shift(&mut list, 0, -1));
        assert!(shift(&mut list, 0, 1));
        assert_eq!(list[1].item, first);
        let last = list.len() - 1;
        assert!(!shift(&mut list, last, 1));
        assert!(!shift(&mut list, last + 5, -1));
    }

    #[test]
    fn prefs_saved_before_the_layout_existed_load_with_the_defaults() {
        let old: TicketPrefs =
            toml::from_str("size_mode = \"risk_balance\"\nsize = 1.5\n").unwrap();
        assert_eq!(old.size_mode, SizeMode::RiskBalance);
        assert_eq!(old.size, 1.5);
        assert_eq!(old.layout, Layout::default());
    }

    #[test]
    fn the_layout_survives_a_round_trip() {
        let mut prefs = TicketPrefs::default();
        prefs.layout.dock = Dock::Left;
        prefs.layout.sections.swap(0, 3);
        prefs.layout.presets.lots = vec![0.02, 0.2];
        let text = toml::to_string(&prefs).unwrap();
        let back: TicketPrefs = toml::from_str(&text).unwrap();
        assert_eq!(back, prefs);
    }

    #[test]
    fn presets_are_read_by_the_way_of_sizing() {
        let presets = Presets::default();
        assert_eq!(presets.of(SizeMode::Lots), &[0.01, 0.1, 0.5, 1.0]);
        assert_eq!(
            presets.of(SizeMode::RiskEquity),
            presets.of(SizeMode::RiskBalance)
        );
        assert!(presets.of(SizeMode::RiskMoney).is_empty());
    }
}
