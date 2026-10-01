use crate::openapi::account::TradeSide;
use crate::openapi::market::PRICE_SCALE;
use crate::openapi::trading::NewOrderReq;
use crate::openapi::trading::contract::{
    Contract, check_protection, format_lots, relative_distance,
};

pub struct MarketDraft {
    pub symbol_id: i64,
    pub side: TradeSide,
    pub lots: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
}

impl MarketDraft {
    pub fn request(
        &self,
        contract: Contract,
        bid: Option<f64>,
        ask: Option<f64>,
    ) -> Result<NewOrderReq, String> {
        if !self.lots.is_finite() || self.lots <= 0.0 {
            return Err("Lots must be finite and above zero".to_owned());
        }
        let stepped = contract.volume_near(self.lots);
        if stepped.limit.is_some() {
            return Err(format!(
                "Lots must be between {} and {}",
                format_lots(contract.lots_of_volume(contract.min_volume)),
                format_lots(contract.lots_of_volume(contract.max_volume))
            ));
        }
        let actual = contract.lots_of_volume(stepped.volume);
        if (actual - self.lots).abs() > 1e-9 {
            return Err(format!(
                "Lots must follow the broker's volume step. Try {}",
                format_lots(actual)
            ));
        }
        let buy = self.side == TradeSide::Buy;
        let entry = (if buy { ask } else { bid })
            .filter(|p| p.is_finite() && *p > 0.0)
            .ok_or_else(|| "Waiting for a market quote".to_owned())?;
        for price in [self.stop_loss, self.take_profit].into_iter().flatten() {
            if !price.is_finite()
                || price <= 0.0
                || (entry - price).abs() * PRICE_SCALE as f64 > i64::MAX as f64 / 2.0
            {
                return Err(
                    "Protection prices must be finite, positive and within range".to_owned(),
                );
            }
        }
        check_protection(buy, entry, self.stop_loss, self.take_profit)
            .map_err(|e| e.to_string())?;
        let mut request =
            NewOrderReq::market(self.symbol_id, self.side, stepped.volume).with_label("wyck");
        request.relative_stop_loss = self
            .stop_loss
            .map(|price| relative_distance(entry - price, contract.digits));
        request.relative_take_profit = self
            .take_profit
            .map(|price| relative_distance(entry - price, contract.digits));
        request.validate().map_err(|e| e.to_string())?;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> MarketDraft {
        MarketDraft {
            symbol_id: 1,
            side: TradeSide::Buy,
            lots: 0.01,
            stop_loss: Some(1.09),
            take_profit: Some(1.12),
        }
    }

    #[test]
    fn market_protection_uses_the_entry_side_and_relative_wire_units() {
        let req = draft()
            .request(Contract::default(), Some(1.0999), Some(1.10))
            .unwrap();
        assert_eq!(req.relative_stop_loss, Some(1000));
        assert_eq!(req.relative_take_profit, Some(2000));
        assert_eq!(req.stop_loss, None);
        assert!(req.validate().is_ok());
        let sell = MarketDraft {
            side: TradeSide::Sell,
            stop_loss: Some(1.11),
            take_profit: Some(1.08),
            ..draft()
        };
        let req = sell
            .request(Contract::default(), Some(1.10), Some(1.1001))
            .unwrap();
        assert_eq!(req.relative_stop_loss, Some(1000));
        assert_eq!(req.relative_take_profit, Some(2000));
    }

    #[test]
    fn missing_prices_wrong_sides_and_volume_changes_are_refused() {
        assert!(draft().request(Contract::default(), None, None).is_err());
        for lots in [f64::NAN, f64::INFINITY, 0.001, 100000.0, 0.015] {
            assert!(
                MarketDraft { lots, ..draft() }
                    .request(Contract::default(), Some(1.1), Some(1.1))
                    .is_err()
            );
        }
        for price in [1.11, f64::MAX, f64::NAN, -1.0] {
            assert!(
                MarketDraft {
                    stop_loss: Some(price),
                    ..draft()
                }
                .request(Contract::default(), Some(1.1), Some(1.1))
                .is_err()
            );
        }
    }
}
