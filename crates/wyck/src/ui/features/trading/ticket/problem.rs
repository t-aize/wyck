//! The sentences for what keeps an order from being sent (see [`TicketProblem`]).

use wyck_trading::math::TicketProblem;
use wyck_trading::plan::ExitsProblem;

/// What the ticket says about a problem, as a sentence to show.
pub(super) fn text(problem: &TicketProblem) -> String {
    match problem {
        TicketProblem::NoPrice => "Set the price of the order".to_owned(),
        TicketProblem::StopLossWrongSide => {
            "The stop loss is on the wrong side of the entry".to_owned()
        }
        TicketProblem::TakeProfitWrongSide => {
            "The take profit is on the wrong side of the entry".to_owned()
        }
        TicketProblem::NoVolume => "Set the volume".to_owned(),
        TicketProblem::NoRisk => "Set the risk".to_owned(),
        TicketProblem::StopLossOff => "Turn the stop loss on to size by risk".to_owned(),
        TicketProblem::NoStopLoss => "Set the stop loss".to_owned(),
        TicketProblem::NoTakeProfit => "Set the take profit".to_owned(),
        TicketProblem::WaitingForRate => "Waiting for the conversion rate".to_owned(),
        TicketProblem::NoMarginShare => "Set the share of the free margin".to_owned(),
        TicketProblem::WaitingForMargin => "Waiting for the margin".to_owned(),
        TicketProblem::ExitsNeedStopLoss => {
            "Exits need a stop loss: their targets are multiples of the risk".to_owned()
        }
        TicketProblem::Exits(exits) => exits_text(exits),
        TicketProblem::OcoNeedsPending => "An OCO pair needs a limit or stop order".to_owned(),
        TicketProblem::AtrUnavailable => {
            "ATR unavailable for the selected timeframe and bar".to_owned()
        }
        TicketProblem::RatioNeedsStopLoss => "A take profit in R needs a stop loss".to_owned(),
        TicketProblem::NoExpiry => "Set when the order expires".to_owned(),
        TicketProblem::NoLimitRange => "Set the range of the limit".to_owned(),
        TicketProblem::NoSlippage => "Set the largest slippage".to_owned(),
        TicketProblem::NoSymbol => "Pick a symbol first".to_owned(),
        TicketProblem::VolumeTooSmallForExits => {
            "The volume is too small to cut into exits".to_owned()
        }
        TicketProblem::OcoPriceInvalid => {
            "The opposite price of the OCO pair is not a price".to_owned()
        }
    }
}

fn exits_text(problem: &ExitsProblem) -> String {
    match problem {
        ExitsProblem::EmptyShare => "Every exit needs a share above 0".to_owned(),
        ExitsProblem::SharesTotal(total) => {
            format!("The shares of the exits add up to {total:.0}%, not 100%")
        }
        ExitsProblem::TargetsNotRising => {
            "Each take profit must be further than the one before".to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentences_are_the_ones_the_ticket_always_said() {
        assert_eq!(text(&TicketProblem::NoPrice), "Set the price of the order");
        assert_eq!(
            text(&TicketProblem::Exits(ExitsProblem::SharesTotal(60.0))),
            "The shares of the exits add up to 60%, not 100%"
        );
        assert_eq!(text(&TicketProblem::NoSymbol), "Pick a symbol first");
    }
}
