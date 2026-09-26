/// What a horizontal chart line represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineId {
    Order(i64),
    OrderStopLoss(i64),
    OrderTakeProfit(i64),
    Position(i64),
    StopLoss(i64),
    TakeProfit(i64),
    Alert(u64),
    Pending(u8),
}
