//! What a chart tells the layout around it, and what it asks the rest of the app to do.

use super::LineId;

/// A time and price under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hover {
    pub time_ms: i64,
    pub price: f64,
}

/// The times at the left and right edges of the plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub left_ms: i64,
    pub right_ms: i64,
}

/// What a chart tells the layout around it.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartEvent {
    /// The user clicked in this chart.
    Activated,
    /// The user clicked a candle to inspect the same time on linked charts.
    TimePicked(i64),
    /// The pointer is over a point of this chart (or left it).
    Hover(Option<Hover>),
    /// The user moved or zoomed the view.
    ViewChanged(Span),
    /// Something that is saved changed: the timeframe, the type, the indicators, the scale.
    SettingsChanged,
    /// How the chart looks changed. It is the same for every chart: the layout hands it to them all.
    LookChanged(Box<crate::domain::chart::ChartLook>),
    CopyIndicators,
    Paste,
    /// The user asked to change the symbol of this chart.
    PickSymbol,
    /// The user asked for something at a price: an order, an alert.
    Action(ChartAction),
    /// A line was dragged to a new price (real units).
    LineMoved(LineId, f64),
    /// The close button of a line was clicked.
    LineClosed(LineId),
    /// A picture of the chart was asked for.
    Screenshot,
    /// A picture of every chart of the layout, in one image, was asked for.
    ScreenshotAll,
    /// The user asked for the editor of the indicator scripts.
    IndicatorEditor(EditorRequest),
}

/// What a chart asks the dashboard to open in the editor of the indicator scripts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorRequest {
    /// Just the editor.
    Open,
    /// The editor, asking for the name of a new script.
    New,
    /// The editor, on the script with this id.
    Edit(String),
}

/// Something asked from the chart's context menu or a drawing, at a price in real units.
#[derive(Debug, Clone, PartialEq)]
pub enum ChartAction {
    /// Open the order ticket with these values.
    Ticket {
        buy: bool,
        /// A limit or stop price; `None` for a market order.
        entry: Option<f64>,
        stop_loss: Option<f64>,
        take_profit: Option<f64>,
        /// The position drawing the values come from, when the ticket is to follow it.
        link: Option<PositionLink>,
    },
    AddAlert(f64),
    /// Add an alert on an indicator or a drawing of the chart.
    AddAlertOn(AlertSeed),
}

/// What an alert asked from a chart is made on.
#[derive(Debug, Clone, PartialEq)]
pub enum AlertSeed {
    /// An indicator of the chart, and the timeframe it is worked out on.
    Indicator {
        study: Box<crate::domain::indicators::config::StudyConfig>,
        timeframe: String,
    },
    /// A drawing of the chart.
    Drawing {
        id: u64,
        zone: bool,
        timeframe: String,
    },
}

/// What a long or short position drawing gives the ticket besides its prices: which drawing it is
/// and how its stop loss and take profit are set, so the ticket can be set the same way and follow
/// the drawing until the order is sent.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionLink {
    pub drawing: u64,
    /// The stop loss is this many ATRs from the entry.
    pub atr: Option<crate::domain::trading::atr_stop::AtrStop>,
    /// The take profit is this multiple of the risk from the entry.
    pub rr: Option<f64>,
}

/// A position drawing as it stands now, with its ATR stop and reward multiple worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct PositionPlan {
    pub drawing: u64,
    pub buy: bool,
    pub entry: f64,
    pub stop_loss: f64,
    pub take_profit: f64,
    pub atr: Option<crate::domain::trading::atr_stop::AtrStop>,
    pub rr: Option<f64>,
}

/// What became of a position drawing a ticket follows.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanState {
    /// The drawing is gone, or is no longer a position.
    Gone,
    /// It cannot be read yet: this chart is not the one of its symbol, or the ATR is not loaded.
    Waiting,
    Ready(PositionPlan),
}
